//! Loki label API queries via Grafana Data Source Proxy.
//!
//! Calls Loki's `/loki/api/v1/labels` and `/loki/api/v1/label/{name}/values` endpoints.

use crate::error::{GraftailError, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LabelListResponse {
    status: String,
    data: Vec<String>,
}

/// Query Loki for all available label names.
pub async fn list_labels(
    client: &Client,
    grafana_url: &str,
    datasource_uid: &str,
    auth_header: &str,
) -> Result<Vec<String>> {
    let url = crate::api::grafana_proxy::build_proxy_url(
        grafana_url,
        datasource_uid,
        "loki/api/v1/labels",
    );

    let response = client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(GraftailError::Auth(format!(
            "Labels query failed (HTTP {status}): {body}"
        )));
    }

    let label_response: LabelListResponse = response.json().await?;
    if label_response.status != "success" {
        return Err(GraftailError::Config(format!(
            "Loki labels API returned non-success status: {}",
            label_response.status
        )));
    }

    Ok(label_response.data)
}

/// Query Loki for all values of a specific label.
pub async fn list_label_values(
    client: &Client,
    grafana_url: &str,
    datasource_uid: &str,
    auth_header: &str,
    label_name: &str,
) -> Result<Vec<String>> {
    let path = format!("loki/api/v1/label/{label_name}/values");
    let url = crate::api::grafana_proxy::build_proxy_url(grafana_url, datasource_uid, &path);

    let response = client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(GraftailError::Auth(format!(
            "Label values query failed (HTTP {status}): {body}"
        )));
    }

    let label_response: LabelListResponse = response.json().await?;
    if label_response.status != "success" {
        return Err(GraftailError::Config(format!(
            "Loki label values API returned non-success status: {}",
            label_response.status
        )));
    }

    Ok(label_response.data)
}
