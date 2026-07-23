//! HTTP query for historical Loki logs via Grafana Proxy.
//!
//! Calls Loki's `query_range` endpoint through the Grafana Data Source Proxy.

use crate::error::{GraftailError, Result};
use crate::stream::parser::{LokiQueryRangeResponse, LogEntry};
use reqwest::Client;

/// Query parameters for the Loki query_range API
pub struct QueryRangeParams {
    pub query: String,
    pub limit: usize,
    pub start: Option<i64>, // nanoseconds
    pub end: Option<i64>,   // nanoseconds
}

/// Fetch historical logs using Loki's query_range API via Grafana Proxy.
pub async fn fetch_history(
    client: &Client,
    grafana_url: &str,
    datasource_uid: &str,
    auth_header: &str,
    params: &QueryRangeParams,
) -> Result<Vec<LogEntry>> {
    let url = crate::api::grafana_proxy::build_proxy_url(
        grafana_url,
        datasource_uid,
        "loki/api/v1/query_range",
    );

    let mut query_params: Vec<(&str, String)> = vec![
        ("query", params.query.clone()),
        ("limit", params.limit.to_string()),
        ("direction", "backward".to_string()),
    ];

    if let Some(start) = params.start {
        query_params.push(("start", start.to_string()));
    }
    if let Some(end) = params.end {
        query_params.push(("end", end.to_string()));
    }

    let response = client
        .get(&url)
        .header("Authorization", auth_header)
        .query(&query_params)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(GraftailError::Auth(format!(
            "Query range failed (HTTP {status}): {body}"
        )));
    }

    let loki_response: LokiQueryRangeResponse = response.json().await?;
    if loki_response.status != "success" {
        return Err(GraftailError::Config(format!(
            "Loki query_range returned non-success status: {}",
            loki_response.status
        )));
    }

    let mut entries = Vec::new();
    for stream in &loki_response.data.result {
        for value in &stream.values {
            let entry = LogEntry::from_raw(stream.stream.clone(), value)?;
            entries.push(entry);
        }
    }

    Ok(entries)
}
