use super::*;
use vmux_service::protocol::AgentQueryResult;

#[test]
fn ok_snapshot_maps_to_text() {
    let out = snapshot_response_to_query_result(&Ok("{\"url\":\"x\"}".to_string()));
    assert_eq!(out, AgentQueryResult::Text("{\"url\":\"x\"}".to_string()));
}

#[test]
fn err_snapshot_maps_to_error() {
    let out = snapshot_response_to_query_result(&Err("no page".to_string()));
    assert_eq!(out, AgentQueryResult::Error("no page".to_string()));
}
