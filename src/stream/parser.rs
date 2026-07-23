use serde::Deserialize;

/// Response from Loki Tail WebSocket (JSON text frame)
#[derive(Debug, Deserialize)]
pub struct LokiTailResponse {
    pub streams: Vec<LokiStream>,
}

/// A single log stream grouped by labels
#[derive(Debug, Deserialize)]
pub struct LokiStream {
    /// Label key-value pairs, e.g. {"app": "nginx", "pod": "nginx-abc"}
    pub stream: std::collections::HashMap<String, String>,

    /// Log entries: [[nanosecond_timestamp, log_line], ...]
    pub values: Vec<[String; 2]>,
}

/// Response from Loki Query Range API
#[derive(Debug, Deserialize)]
pub struct LokiQueryRangeResponse {
    pub status: String,
    pub data: LokiQueryRangeData,
}

#[derive(Debug, Deserialize)]
pub struct LokiQueryRangeData {
    #[serde(rename = "resultType")]
    #[allow(dead_code)]
    pub result_type: String,
    pub result: Vec<LokiStream>,
}

/// A single parsed log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Label key-value pairs
    pub labels: std::collections::HashMap<String, String>,

    /// Timestamp in nanoseconds (from Loki)
    pub timestamp_ns: i64,

    /// The log line content
    pub line: String,
}

impl LogEntry {
    /// Create a LogEntry from a raw Loki stream values entry
    pub fn from_raw(
        labels: std::collections::HashMap<String, String>,
        raw_entry: &[String; 2],
    ) -> crate::error::Result<Self> {
        let timestamp_ns: i64 = raw_entry[0]
            .parse()
            .map_err(|e| crate::error::GraftailError::Timestamp(format!(
                "Failed to parse timestamp '{}': {}",
                raw_entry[0], e
            )))?;

        Ok(LogEntry {
            labels,
            timestamp_ns,
            line: raw_entry[1].clone(),
        })
    }
}

/// Parse a LokiTailResponse into a flat Vec<LogEntry>
pub fn parse_tail_response(response: &LokiTailResponse) -> crate::error::Result<Vec<LogEntry>> {
    let mut entries = Vec::new();

    for stream in &response.streams {
        for value in &stream.values {
            let entry = LogEntry::from_raw(stream.stream.clone(), value)?;
            entries.push(entry);
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tail_response() {
        let json = r#"{
            "streams": [
                {
                    "stream": {"app": "nginx", "job": "varlogs"},
                    "values": [
                        ["1698386400000000000", "GET /api/health 200"],
                        ["1698386401000000000", "ERROR: connection refused"]
                    ]
                }
            ]
        }"#;

        let response: LokiTailResponse = serde_json::from_str(json).unwrap();
        let entries = parse_tail_response(&response).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].line, "GET /api/health 200");
        assert_eq!(entries[0].timestamp_ns, 1698386400000000000);
        assert_eq!(entries[0].labels.get("app").unwrap(), "nginx");
        assert_eq!(entries[1].line, "ERROR: connection refused");
    }

    #[test]
    fn test_parse_multi_stream() {
        let json = r#"{
            "streams": [
                {
                    "stream": {"app": "api"},
                    "values": [["1000000000000000000", "log from api"]]
                },
                {
                    "stream": {"app": "worker"},
                    "values": [["2000000000000000000", "log from worker"]]
                }
            ]
        }"#;

        let response: LokiTailResponse = serde_json::from_str(json).unwrap();
        let entries = parse_tail_response(&response).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].labels.get("app").unwrap(), "api");
        assert_eq!(entries[1].labels.get("app").unwrap(), "worker");
    }
}
