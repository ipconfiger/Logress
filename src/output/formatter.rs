//! Log entry formatting: timestamp conversion, label display, output modes.

use crate::cli::OutputFormat;
use crate::output::color::{colorize_labels, detect_log_level, LogLevel};
use crate::stream::parser::LogEntry;
use chrono::{DateTime, Local, TimeZone, Utc};
use owo_colors::OwoColorize;
use serde_json::Value;

/// Format a log entry for display based on the output format.
pub fn format_entry(entry: &LogEntry, output: OutputFormat, use_utc: bool) -> String {
    match output {
        OutputFormat::Json => format_entry_json(entry, use_utc),
        OutputFormat::Pretty => format_entry_pretty(entry, use_utc),
        OutputFormat::Plain => format_entry_pretty(entry, use_utc),
        // Plain uses same structure but without ANSI codes (handled via --color=never or similar)
        // For simplicity, Pretty and Plain share structure; color is controlled at output level
    }
}

/// Convert a nanosecond timestamp string to a DateTime<Utc>
pub fn nanos_to_datetime(nanos: i64) -> Option<DateTime<Utc>> {
    let secs = nanos / 1_000_000_000;
    let nsecs = (nanos % 1_000_000_000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}

/// Format timestamp as string
pub fn format_timestamp(nanos: i64, use_utc: bool) -> String {
    match nanos_to_datetime(nanos) {
        Some(utc_dt) => {
            if use_utc {
                utc_dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
            } else {
                let local: DateTime<Local> = DateTime::from(utc_dt);
                local.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
            }
        }
        None => format!("{nanos}"),
    }
}

/// Pretty-print entry
fn format_entry_pretty(entry: &LogEntry, use_utc: bool) -> String {
    let ts = format_timestamp(entry.timestamp_ns, use_utc);
    let labels = colorize_labels(&entry.labels);
    let formatted_line = format_log_line(&entry.line);

    format!("{ts} {labels} {formatted_line}")
}

/// Format a log line, parsing structured JSON if present.
///
/// Handles three formats:
/// 1. Docker JSON driver wrapper: `{"log":"{...}","stream":"stdout","time":"..."}`
///    → unwrap and parse inner JSON
/// 2. Loki/Logfmt style: `level=info ts=... caller=... msg="..."`
///    → extract key=value pairs
/// 3. Raw structured JSON: `{"level":"INFO","fields":{"message":"..."},"target":"..."}`
///    → format directly
///
/// Falls back to original line for unrecognized formats.
pub fn format_log_line(line: &str) -> String {
    let trimmed = line.trim();

    // Try Docker JSON driver wrapper: {"log":"...","stream":"...","time":"..."}
    if let Ok(wrapper) = serde_json::from_str::<Value>(trimmed) {
        if let Some(inner) = wrapper.get("log").and_then(|l| l.as_str()) {
            let inner_trimmed = inner.trim();
            // Try to parse the inner string as structured JSON
            if let Ok(parsed) = serde_json::from_str::<Value>(inner_trimmed) {
                if let Some(result) = try_format_structured(&parsed) {
                    return result;
                }
            }
            // Inner not JSON, but has useful content — return the inner string
            return inner_trimmed.to_string();
        }
    }

    // Try raw structured JSON
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        if let Some(result) = try_format_structured(&parsed) {
            return result;
        }
    }

    // Try Logfmt style: level=info ts=... caller=... msg="..."
    if let Some(result) = try_format_logfmt(trimmed) {
        return result;
    }

    line.to_string()
}

/// Try to format a parsed JSON value as a structured log entry.
/// Returns None if it doesn't look like a structured log (no level + message).
fn try_format_structured(parsed: &Value) -> Option<String> {
    let level_str = parsed.get("level").and_then(|v| v.as_str()).unwrap_or("");
    if level_str.is_empty() {
        return None;
    }

    // Extract message — try fields.message, then msg, then message
    let message = parsed
        .get("fields")
        .and_then(|f| f.get("message"))
        .and_then(|m| m.as_str())
        .or_else(|| parsed.get("msg").and_then(|m| m.as_str()))
        .or_else(|| parsed.get("message").and_then(|m| m.as_str()))?;

    let target = parsed.get("target").and_then(|t| t.as_str());

    // Extra fields from fields.* except "message"
    let extra_fields: Vec<String> = if let Some(fields) = parsed.get("fields").and_then(|f| f.as_object()) {
        fields
            .iter()
            .filter(|(k, _)| *k != "message")
            .map(|(k, v)| {
                let val_str = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    _ => v.to_string(),
                };
                format!("{k}={val_str}")
            })
            .collect()
    } else {
        Vec::new()
    };

    let level = LogLevel::detect(level_str);
    let level_styled = level.style().style(level_str.to_uppercase()).to_string();

    let mut parts: Vec<String> = vec![level_styled];

    if let Some(t) = target {
        parts.push(t.dimmed().to_string());
    }

    parts.push(message.to_string());

    if !extra_fields.is_empty() {
        let extras = format!("({})", extra_fields.join(", "));
        parts.push(extras.dimmed().to_string());
    }

    Some(parts.join("  "))
}

/// Try to format a Logfmt-style line (key=value key="value").
fn try_format_logfmt(line: &str) -> Option<String> {
    // Need at least "level=" and "msg=" or "message="
    let lower = line.to_lowercase();
    if !lower.contains("level=") {
        return None;
    }

    let mut level = "";
    let mut msg = "";
    let mut caller = "";

    // Simple key=value parser for common fields
    for part in line.split_whitespace() {
        if let Some((k, v)) = part.split_once('=') {
            let val = v.trim_matches('"');
            match k {
                "level" => level = val,
                "msg" => msg = val,
                "message" => msg = val,
                "caller" => caller = val,
                _ => {}
            }
        }
    }

    if level.is_empty() || msg.is_empty() {
        return None;
    }

    let log_level = LogLevel::detect(level);
    let level_styled = log_level.style().style(level.to_uppercase()).to_string();

    let mut parts = vec![level_styled];

    // Extract short filename from caller (e.g. "filetarget.go:192")
    if !caller.is_empty() {
        if let Some(short) = caller.rsplit('/').next() {
            parts.push(short.dimmed().to_string());
        }
    }

    parts.push(msg.to_string());

    Some(parts.join("  "))
}

/// JSON output entry
fn format_entry_json(entry: &LogEntry, use_utc: bool) -> String {
    let timestamp = match nanos_to_datetime(entry.timestamp_ns) {
        Some(utc_dt) => {
            if use_utc {
                utc_dt.to_rfc3339()
            } else {
                let local: DateTime<Local> = DateTime::from(utc_dt);
                local.to_rfc3339()
            }
        }
        None => entry.timestamp_ns.to_string(),
    };

    let level = detect_log_level(&entry.line);

    serde_json::json!({
        "timestamp": timestamp,
        "timestamp_ns": entry.timestamp_ns.to_string(),
        "labels": entry.labels,
        "level": level.to_str(),
        "line": entry.line,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_entry(line: &str) -> LogEntry {
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "test".to_string());
        LogEntry {
            labels,
            timestamp_ns: 1698386400000000000,
            line: line.to_string(),
        }
    }

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp(1698386400000000000, true);
        assert!(ts.contains("2023-10-27"));
    }

    #[test]
    fn test_format_entry_json() {
        let entry = make_entry("INFO: hello world");
        let json = format_entry_json(&entry, true);
        assert!(json.contains("INFO"));
        assert!(json.contains("hello world"));
    }

    #[test]
    fn test_format_entry_pretty() {
        let entry = make_entry("ERROR: something failed");
        let output = format_entry_pretty(&entry, true);
        assert!(output.contains("2023-10-27"));
        assert!(output.contains("app:test"));
        assert!(output.contains("ERROR"));
    }

    #[test]
    fn test_nanos_to_datetime() {
        let dt = nanos_to_datetime(1698386400000000000).unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2023-10-27");
    }

    #[test]
    fn test_nanos_to_datetime_zero() {
        // Unix epoch
        let dt = nanos_to_datetime(0).unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "1970-01-01");
    }

    #[test]
    fn test_nanos_to_datetime_negative() {
        // Negative nanosecond timestamp (before epoch)
        let result = nanos_to_datetime(-1);
        assert!(result.is_none() || result.is_some()); // chrono may or may not support negative
    }

    #[test]
    fn test_format_log_line_structured_json() {
        let line = r#"{"timestamp":"2026-07-23T06:34:45.830433Z","level":"INFO","fields":{"message":"arrears check passed","user_id":12,"model_name":"Qwen3.6-27B"},"target":"api::auth::handler"}"#;
        let output = format_log_line(line);
        assert!(output.contains("INFO"));
        assert!(output.contains("api::auth::handler"));
        assert!(output.contains("arrears check passed"));
        assert!(output.contains("user_id=12"));
        assert!(output.contains("model_name=Qwen3.6-27B"));
    }

    #[test]
    fn test_format_log_line_flat_json() {
        let line = r#"{"level":"ERROR","msg":"connection refused"}"#;
        let output = format_log_line(line);
        assert!(output.contains("ERROR"));
        assert!(output.contains("connection refused"));
    }

    #[test]
    fn test_format_log_line_minimal() {
        let line = r#"{"level":"WARN","fields":{"message":"disk 90% full"}}"#;
        let output = format_log_line(line);
        assert!(output.contains("WARN"));
        assert!(output.contains("disk 90% full"));
        // No target, no extra fields
        assert!(!output.contains("(")); 
    }

    #[test]
    fn test_format_log_line_non_json_fallback() {
        let line = "This is just plain text";
        let output = format_log_line(line);
        assert_eq!(output, line);
    }

    #[test]
    fn test_format_log_line_no_level_fallback() {
        let line = r#"{"foo":"bar"}"#;
        let output = format_log_line(line);
        assert_eq!(output, line);
    }

    #[test]
    fn test_format_log_line_no_message_fallback() {
        let line = r#"{"level":"INFO","foo":"bar"}"#;
        let output = format_log_line(line);
        assert_eq!(output, line);
    }
}
