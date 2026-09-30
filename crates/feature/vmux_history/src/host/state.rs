use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_core::page::PageReady;

use crate::state::HistoryUiState;

type HistoryUiStateUpdates = vmux_core::host::UiState<HistoryUiState>;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::UiStatePlugin::<HistoryUiState>::default())
            .add_observer(on_page_ready);
    }
}

#[derive(Component, Clone, Debug)]
#[require(HistoryUiStateUpdates)]
pub(super) struct HistoryPageState {
    pub(super) query: Option<String>,
    pub(super) limit: u32,
    pub(super) has_more: bool,
}

impl Default for HistoryPageState {
    fn default() -> Self {
        Self {
            query: None,
            limit: 50,
            has_more: false,
        }
    }
}

impl HistoryPageState {
    pub(super) fn search(&mut self, query: &str) {
        let query = query.trim();
        self.query = if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        };
        self.limit = 50;
        self.has_more = false;
    }

    pub(super) fn load_more(&mut self) {
        if !self.has_more {
            return;
        }
        self.has_more = false;
        self.limit = self.limit.saturating_add(50);
    }
}

fn on_page_ready(
    trigger: On<UiInput<PageReady>>,
    pages: Query<&vmux_core::PageMetadata>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(page) = pages.get(entity) else {
        return;
    };
    if !vmux_api::VmuxRoute::parse(&page.url).is_some_and(|route| route.is_host("history")) {
        return;
    }
    commands.entity(entity).insert(HistoryPageState::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_resets_pagination_and_duplicate_load_more_is_ignored() {
        let mut state = HistoryPageState {
            has_more: true,
            ..Default::default()
        };
        state.load_more();
        state.load_more();
        assert_eq!(state.limit, 100);

        state.search(" git ");
        assert_eq!(state.query.as_deref(), Some("git"));
        assert_eq!(state.limit, 50);

        state.search("  ");
        assert_eq!(state.query, None);
    }
}
