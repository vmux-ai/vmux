use crate::event::HistoryQueryResponse;

#[vmux_api::ui_state_patch(Default)]
pub struct HistoryUiStatePatch {
    pub query: Option<HistoryQueryResponse>,
}

#[vmux_api::ui_state(Default, target = "history")]
pub struct HistoryUiState {
    pub sequence: u64,
    pub patches: Vec<HistoryUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_response_round_trips_through_ui_state() {
        let state = HistoryUiState {
            sequence: 3,
            patches: vec![
                HistoryQueryResponse {
                    request_id: 7,
                    offset: 50,
                    entries: Vec::new(),
                    has_more: false,
                }
                .into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<HistoryUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.sequence, 3);
        let response = decoded.patches[0].query.as_ref().unwrap();
        assert_eq!(response.request_id, 7);
        assert_eq!(response.offset, 50);
    }
}
