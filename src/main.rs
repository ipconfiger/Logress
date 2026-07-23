//! graftail — Real-time Loki log tailing via Grafana Data Source Proxy.
//!
//! Usage: graftail -q '{app="nginx"}' [OPTIONS]

use clap::Parser;
use graftail::api;
use graftail::cli::{self, Commands};
use graftail::config;
use graftail::error::{self, Result};
use graftail::output;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env files (silently skip if missing)
    load_env_files();

    // Install rustls crypto provider (required before any TLS)
    let _ = rustls::crypto::ring::default_provider().install_default();
    // 1. Parse CLI arguments
    let cli = cli::Cli::parse();

    // 2. Load and merge configuration
    let app_config = config::load_config(&cli)?;

    // 3. Build auth header
    let auth_header = app_config.auth.header_value();

    // 3b. Handle list subcommand
    if let Some(Commands::List { label }) = &cli.command {
        let client = if app_config.insecure {
            reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new())
        } else {
            reqwest::Client::new()
        };

        if let Some(label_name) = label {
            let values = api::labels::list_label_values(
                &client,
                &app_config.grafana_url,
                &app_config.datasource_uid,
                &auth_header,
                label_name,
            )
            .await?;
            for v in values {
                println!("{v}");
            }
        } else {
            let labels = api::labels::list_labels(
                &client,
                &app_config.grafana_url,
                &app_config.datasource_uid,
                &auth_header,
            )
            .await?;
            for label_name in labels {
                print!("{label_name}:");
                match api::labels::list_label_values(
                    &client,
                    &app_config.grafana_url,
                    &app_config.datasource_uid,
                    &auth_header,
                    &label_name,
                )
                .await
                {
                    Ok(values) => {
                        let joined = values.join(", ");
                        println!(" {joined}");
                    }
                    Err(e) => {
                        eprintln!(" [error: {e}]");
                    }
                }
            }
        }

        return Ok(());
    }

    // Validate query is present for tail mode
    let query = app_config.query.clone().ok_or_else(|| {
        error::GraftailError::Config(
            "query is required for tail mode. Set via -q/--query, GRAFTAIL_QUERY env var, or config file default_query."
                .into(),
        )
    })?;

    // 4. Set up cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();

    // 5. Set up keyboard listener for freeze-screen interaction
    let screen_state = Arc::new(output::screen::ScreenState::new());
    let screen_state_clone = screen_state.clone();
    let keyboard_cancel = cancel_token.clone();
    output::screen::spawn_keyboard_listener(screen_state_clone, keyboard_cancel);

    // 6. Set up signal handler
    let signal_cancel = cancel_token.clone();
    tokio::spawn(async move {
        watch_signals(signal_cancel).await;
    });

    // 7. Create output handler
    let output_handler = Arc::new(output::OutputHandler::new(
        app_config.output,
        app_config.use_utc,
        screen_state,
    ));

    // 8. Channel from tail session to output
    let (tx, mut rx) = mpsc::unbounded_channel();

    // 9. Fetch historical logs if --last is specified
    if app_config.last > 0 {
        let client = if app_config.insecure {
            reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new())
        } else {
            reqwest::Client::new()
        };
        let params = api::query_range::QueryRangeParams {
            query: query.clone(),
            limit: app_config.last,
            start: None,
            end: None,
        };

        eprintln!("[graftail] Fetching last {} historical logs...", app_config.last);
        match api::query_range::fetch_history(
            &client,
            &app_config.grafana_url,
            &app_config.datasource_uid,
            &auth_header,
            &params,
        )
        .await
        {
            Ok(entries) if !entries.is_empty() => {
                output_handler.write_entries(&entries);
            }
            Ok(_) => {
                eprintln!("[graftail] No historical logs found.");
            }
            Err(e) => {
                eprintln!("[graftail] History fetch failed: {e}");
            }
        }
        eprintln!("[graftail] Switching to live tail...");
    }

    // 10. Build tail config — set start to current time so we only get NEW logs
    let start = if let Some(s) = &app_config.since {
        // --since: compute absolute timestamp = now - duration
        parse_since_absolute(s)
    } else {
        // No --since: start from right now
        Some(current_nanos())
    };

    let tail_config = api::tail::TailConfig {
        grafana_url: app_config.grafana_url.clone(),
        datasource_uid: app_config.datasource_uid.clone(),
        query: query.clone(),
        start,
        limit: 100,
        delay_for: 0,
        auth_header,
        insecure: app_config.insecure,
        cancel_token: cancel_token.clone(),
    };

    // 11. Spawn tail session
    let tail_session = api::tail::TailSession::new(tail_config);
    let tail_task = tokio::spawn(async move { tail_session.run(tx).await });

    // 12. Output loop: read from channel and write to stdout
    let output_handler_clone = output_handler.clone();
    let _output_task = tokio::spawn(async move {
        while let Some(entries) = rx.recv().await {
            output_handler_clone.write_entries(&entries);
        }
    });

    // 13. Wait for tail session to finish
    let tail_result = tail_task.await.unwrap_or_else(|_| {
        Err(error::GraftailError::Config("Tail task panicked".into()))
    });

    // 14. Signal output task to stop
    drop(output_handler);
    cancel_token.cancel();

    // 15. Restore terminal state
    restore_terminal();

    match tail_result {
        Ok(()) | Err(error::GraftailError::Interrupted) => {
            eprintln!("\n[graftail] Shutdown complete.");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Watch for SIGINT (Ctrl+C) and SIGTERM signals
#[cfg(unix)]
async fn watch_signals(cancel: CancellationToken) {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => {
            eprintln!("\n[graftail] Received SIGINT, shutting down...");
        }
        _ = sigterm.recv() => {
            eprintln!("\n[graftail] Received SIGTERM, shutting down...");
        }
    }

    cancel.cancel();
}

/// Watch for Ctrl+C on Windows
#[cfg(not(unix))]
async fn watch_signals(cancel: CancellationToken) {
    tokio::signal::ctrl_c().await.expect("Failed to register Ctrl+C handler");
    eprintln!("\n[graftail] Received Ctrl+C, shutting down...");
    cancel.cancel();
}

/// Restore terminal to normal state (exit crossterm raw mode if enabled)
fn restore_terminal() {
    use crossterm::terminal;
    let _ = terminal::disable_raw_mode();
}

/// Parse a human-readable duration like "1h", "30m" into absolute nanosecond timestamp (now - duration).
fn parse_since_absolute(s: &str) -> Option<i64> {
    let duration = humantime::parse_duration(s).ok()?;
    let now_ns = current_nanos();
    Some(now_ns - duration.as_nanos() as i64)
}

/// Get current time in nanoseconds since Unix epoch
fn current_nanos() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

/// Load .env files from standard locations.
/// Searches: current directory, then ~/.config/graftail/.env
/// Silently ignores missing files.
fn load_env_files() {
    // Load from current directory
    let _ = dotenvy::dotenv();

    // Load from ~/.config/graftail/.env
    if let Ok(home) = std::env::var("HOME") {
        let graftail_env = std::path::PathBuf::from(home)
            .join(".config")
            .join("graftail")
            .join(".env");
        let _ = dotenvy::from_filename(graftail_env);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_nanos_is_recent() {
        let now = current_nanos();
        assert!(now > 1_700_000_000_000_000_000);
    }

    #[test]
    fn test_parse_since_absolute() {
        let result = parse_since_absolute("1h");
        assert!(result.is_some());
        let now = current_nanos();
        let since = result.unwrap();
        let diff = now - since;
        assert!(diff >= 3_600_000_000_000);
        assert!(diff < 3_605_000_000_000);
    }

    #[test]
    fn test_parse_since_invalid() {
        assert!(parse_since_absolute("xyz").is_none());
    }
}
