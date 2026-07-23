/// Integration tests using test fixtures.
use graftail::stream::parser;

#[test]
fn test_parse_tail_fixture() {
    let json = include_str!("fixtures/tail_response.json");
    let response: parser::LokiTailResponse = serde_json::from_str(json)
        .expect("Failed to parse tail fixture");

    let entries = parser::parse_tail_response(&response)
        .expect("Failed to parse tail response");

    assert_eq!(entries.len(), 7, "Expected 7 log entries");

    // Verify first entry
    assert_eq!(entries[0].line, "192.168.1.1 - GET /api/health 200");
    assert_eq!(entries[0].timestamp_ns, 1698386400000000000);
    assert_eq!(entries[0].labels.get("app").unwrap(), "nginx");
    assert_eq!(entries[0].labels.get("pod").unwrap(), "nginx-7d4f8b9c-abcde");

    // Verify error entry
    assert!(entries[1].line.contains("ERROR"));
    assert!(entries[1].line.contains("connection refused"));

    // Verify multi-stream labels
    let api_entries: Vec<_> = entries.iter().filter(|e| e.labels.get("app").unwrap() == "api").collect();
    assert_eq!(api_entries.len(), 2);
    assert_eq!(api_entries[1].labels.get("namespace").unwrap(), "production");
}

#[test]
fn test_parse_query_range_fixture() {
    let json = include_str!("fixtures/query_range_response.json");
    let response: parser::LokiQueryRangeResponse = serde_json::from_str(json)
        .expect("Failed to parse query_range fixture");

    assert_eq!(response.status, "success");
    assert_eq!(response.data.result.len(), 1);

    let entries = parser::parse_tail_response(&parser::LokiTailResponse {
        streams: response.data.result,
    })
    .expect("Failed to parse query_range entries");

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[2].line, "ERROR: upstream timeout");
}
