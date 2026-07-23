//! WebSocket Tail connection to Loki via Grafana Proxy.
//!
//! Establishes a WebSocket connection, receives log frames,
//! and forwards parsed entries to the output handler.

use crate::error::{GraftailError, Result};
use crate::stream::parser::{self, LogEntry};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

// ─── Insecure TLS ─────────────────────────────────────────────────────────

use rustls::client::danger::{ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::DigitallySignedStruct;
use rustls::SignatureScheme;

/// A TLS certificate verifier that accepts any certificate (insecure).
#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
            .to_vec()
    }
}

/// Build a TLS connector that skips certificate verification
fn build_insecure_connector() -> Connector {
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    Connector::Rustls(Arc::new(config))
}

/// Configuration for a Tail session
#[derive(Debug, Clone)]
pub struct TailConfig {
    pub grafana_url: String,
    pub datasource_uid: String,
    pub query: String,
    pub start: Option<i64>, // nanoseconds
    pub limit: usize,
    pub delay_for: usize,
    pub auth_header: String,
    pub insecure: bool,
    pub cancel_token: CancellationToken,
}

/// A WebSocket Tail session
pub struct TailSession {
    config: Arc<TailConfig>,
}

impl TailSession {
    pub fn new(config: TailConfig) -> Self {
        TailSession {
            config: Arc::new(config),
        }
    }

    /// Build the WebSocket URL for the Loki Tail endpoint
    fn ws_url(&self) -> Result<String> {
        let url = crate::api::grafana_proxy::build_proxy_ws_url(
            &self.config.grafana_url,
            &self.config.datasource_uid,
            "loki/api/v1/tail",
        )?;

        let mut url_with_params = format!("{url}?query={}", urlencoding(&self.config.query));
        url_with_params.push_str(&format!("&limit={}", self.config.limit));
        url_with_params.push_str(&format!("&delay_for={}", self.config.delay_for));

        if let Some(start) = self.config.start {
            url_with_params.push_str(&format!("&start={start}"));
        }

        Ok(url_with_params)
    }

    /// Build a WebSocket upgrade request with auth headers
    fn build_request(&self) -> Result<http::Request<()>> {
        use tokio_tungstenite::tungstenite::handshake::client::generate_key;

        let url = self.ws_url()?;
        let uri: http::Uri = url.parse().map_err(|_| {
            GraftailError::Config(format!("Invalid WebSocket URL: {url}"))
        })?;

        // Extract host for the Host header
        let parsed = url::Url::parse(&url).map_err(|e| {
            GraftailError::Url(e)
        })?;
        let host = parsed.host_str().unwrap_or("localhost");
        let host_header = if let Some(port) = parsed.port() {
            format!("{host}:{port}")
        } else {
            host.to_string()
        };

        let request = http::Request::builder()
            .uri(uri)
            .header("Host", &host_header)
            .header("Authorization", &self.config.auth_header)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", generate_key())
            .body(())
            .map_err(|e| GraftailError::Config(format!("Failed to build request: {e}")))?;

        Ok(request)
    }

    /// Establish the WebSocket connection
    async fn connect(&self) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
        use tokio_tungstenite::connect_async_tls_with_config;

        let request = self.build_request()?;

        let connector: Option<Connector> = if self.config.insecure {
            Some(build_insecure_connector())
        } else {
            None
        };

        let (ws_stream, response) =
            connect_async_tls_with_config(request, None, false, connector).await?;

        // WebSocket upgrade: 101 Switching Protocols is success
        let status = response.status();
        if !status.is_success() && status != http::StatusCode::SWITCHING_PROTOCOLS {
            return Err(GraftailError::Auth(format!(
                "WebSocket upgrade failed (HTTP {}): {:?}",
                status,
                response.headers()
            )));
        }

        Ok(ws_stream)
    }

    /// Exponential backoff with jitter for reconnection
    fn backoff_delay(attempt: u32) -> Duration {
        let base = Duration::from_secs(1);
        let max = Duration::from_secs(60);
        let exp = base * 2u32.pow(attempt.min(10));

        // Add jitter: 0..50% of exponential delay
        let jitter = Duration::from_millis(
            (rand::random::<f64>() * exp.as_millis() as f64 * 0.5) as u64,
        );

        std::cmp::min(exp + jitter, max)
    }

    /// Run the Tail session: connect → receive loop → reconnect on disconnect.
    ///
    /// Parsed log entries are sent through `tx`.
    pub async fn run(&self, tx: mpsc::UnboundedSender<Vec<LogEntry>>) -> Result<()> {
        let mut attempt = 0u32;
        let max_retries = 10u32;

        loop {
            // Check cancellation before connecting
            if self.config.cancel_token.is_cancelled() {
                return Err(GraftailError::Interrupted);
            }

            match self.connect().await {
                Ok(ws) => {
                    if attempt > 0 {
                        eprintln!("[graftail] Reconnected successfully.");
                    }
                    attempt = 0;

                    if let Err(e) = self.receive_loop(ws, &tx).await {
                        if matches!(e, GraftailError::Interrupted) {
                            return Err(e);
                        }
                        eprintln!("[graftail] Connection error: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("[graftail] Connection failed: {e}");
                }
            }

            if self.config.cancel_token.is_cancelled() {
                return Err(GraftailError::Interrupted);
            }

            attempt += 1;
            if attempt > max_retries {
                return Err(GraftailError::MaxRetriesExceeded(max_retries as usize));
            }

            let delay = Self::backoff_delay(attempt);
            eprintln!(
                "[graftail] Connection lost. Reconnecting in {}s... (attempt {}/{})",
                delay.as_secs(),
                attempt,
                max_retries
            );

            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.config.cancel_token.cancelled() => {
                    return Err(GraftailError::Interrupted);
                }
            }
        }
    }

    /// Main WebSocket receive loop
    async fn receive_loop(
        &self,
        mut ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
        tx: &mpsc::UnboundedSender<Vec<LogEntry>>,
    ) -> Result<()> {
        loop {
            tokio::select! {
                msg = ws.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Parse the text frame
                            match serde_json::from_str::<parser::LokiTailResponse>(&text) {
                                Ok(response) => {
                                    match parser::parse_tail_response(&response) {
                                        Ok(entries) if !entries.is_empty() => {
                                            let _ = tx.send(entries);
                                        }
                                        Err(e) => {
                                            eprintln!("[graftail] Parse warning: {e}");
                                        }
                                        _ => {} // empty entries, skip
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[graftail] JSON parse warning: {e}");
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            return Ok(());
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws.send(Message::Pong(data)).await;
                        }
                        Some(Err(e)) => {
                            return Err(GraftailError::WebSocket(e));
                        }
                        None => {
                            // Stream ended
                            return Ok(());
                        }
                        _ => {} // Ignore other message types
                    }
                }
                _ = self.config.cancel_token.cancelled() => {
                    // Send close frame before dropping
                    let _ = ws.close(None).await;
                    return Err(GraftailError::Interrupted);
                }
            }
        }
    }
}

/// Simple URL encoding for the LogQL query parameter
fn urlencoding(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('{', "%7B")
        .replace('}', "%7D")
        .replace('"', "%22")
        .replace('=', "%3D")
        .replace('|', "%7C")
        .replace('~', "%7E")
        .replace('!', "%21")
        .replace('(', "%28")
        .replace(')', "%29")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay_range() {
        let d = TailSession::backoff_delay(0);
        assert!(d.as_millis() >= 500); // at least base/2
        assert!(d.as_millis() <= 1500); // at most base + 50% jitter

        let d = TailSession::backoff_delay(10);
        assert!(d.as_secs() <= 90); // capped by max + jitter
    }

    #[test]
    fn test_urlencoding() {
        let encoded = urlencoding("{app=\"nginx\"} |= \"error\"");
        assert_eq!(encoded, "%7Bapp%3D%22nginx%22%7D%20%7C%3D%20%22error%22");
    }
}
