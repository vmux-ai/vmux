use crate::event::HistoryEntry;

#[vmux_api::ui_state(Default)]
pub struct HistoryUiState {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_snapshot_round_trips_through_ui_state() {
        let state = HistoryUiState {
            entries: vec![HistoryEntry {
                url_entity_bits: 7,
                url: "https://example.com".to_string(),
                title: "Example".to_string(),
                favicon_url: String::new(),
                visit_created_at: 1,
                visit_count: 2,
                last_visited_at: 3,
            }],
            has_more: true,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<HistoryUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.entries.len(), 1);
        assert_eq!(decoded.entries[0].url_entity_bits, 7);
        assert!(decoded.has_more);
    }
}
