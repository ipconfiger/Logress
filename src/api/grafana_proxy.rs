use crate::error::Result;

/// Build a Grafana Data Source Proxy URL.
///
/// Template: `{grafana_url}/api/datasources/proxy/uid/{datasource_uid}/{loki_api_path}`
pub fn build_proxy_url(grafana_url: &str, datasource_uid: &str, loki_path: &str) -> String {
    let base = grafana_url.trim_end_matches('/');
    format!("{base}/api/datasources/proxy/uid/{datasource_uid}/{loki_path}")
}

/// Build a WebSocket version of the Grafana Proxy URL.
/// Converts `http://` → `ws://` and `https://` → `wss://`.
pub fn build_proxy_ws_url(grafana_url: &str, datasource_uid: &str, loki_path: &str) -> Result<String> {
    let ws_base = if grafana_url.starts_with("https://") {
        grafana_url.replacen("https://", "wss://", 1)
    } else if grafana_url.starts_with("http://") {
        grafana_url.replacen("http://", "ws://", 1)
    } else {
        return Err(crate::error::GraftailError::Config(format!(
            "Invalid Grafana URL scheme: {grafana_url}"
        )));
    };

    Ok(build_proxy_url(&ws_base, datasource_uid, loki_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_proxy_url() {
        let url = build_proxy_url("https://grafana.example.com", "abc123", "loki/api/v1/tail");
        assert_eq!(url, "https://grafana.example.com/api/datasources/proxy/uid/abc123/loki/api/v1/tail");
    }

    #[test]
    fn test_build_proxy_url_trailing_slash() {
        let url = build_proxy_url("https://grafana.example.com/", "abc123", "loki/api/v1/query_range");
        assert_eq!(url, "https://grafana.example.com/api/datasources/proxy/uid/abc123/loki/api/v1/query_range");
    }

    #[test]
    fn test_build_proxy_ws_url_https() {
        let url = build_proxy_ws_url("https://grafana.example.com", "abc123", "loki/api/v1/tail").unwrap();
        assert_eq!(url, "wss://grafana.example.com/api/datasources/proxy/uid/abc123/loki/api/v1/tail");
    }

    #[test]
    fn test_build_proxy_ws_url_http() {
        let url = build_proxy_ws_url("http://grafana.local", "uid1", "loki/api/v1/tail").unwrap();
        assert_eq!(url, "ws://grafana.local/api/datasources/proxy/uid/uid1/loki/api/v1/tail");
    }

    #[test]
    fn test_build_proxy_ws_url_invalid_scheme() {
        let result = build_proxy_ws_url("ftp://bad", "uid1", "tail");
        assert!(result.is_err());
    }
}
