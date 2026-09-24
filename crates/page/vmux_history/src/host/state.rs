use bevy::prelude::*;
use bevy_cef::prelude::BinReceive;
use vmux_core::page::PageReady;

use crate::event::HistoryQueryRequest;
use crate::state::HistoryUiState;

pub(super) type HistoryUiStateUpdates = vmux_core::host::UiStateUpdates<HistoryUiState>;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::UiStatePlugin::<HistoryUiState>::default())
            .add_observer(on_page_ready);
    }
}

#[derive(Component, Clone, Debug)]
#[require(HistoryUiStateUpdates)]
pub(super) struct HistoryQueryState {
    pub(super) query: Option<String>,
    pub(super) limit: u32,
    pub(super) request_id: u64,
}

impl Default for HistoryQueryState {
    fn default() -> Self {
        Self {
            query: None,
            limit: 50,
            request_id: 0,
        }
    }
}

impl HistoryQueryState {
    pub(super) fn from_request(request: &HistoryQueryRequest) -> Self {
        Self {
            query: request.query.clone(),
            limit: request.limit,
            request_id: request.request_id,
        }
    }
}

fn on_page_ready(
    trigger: On<BinReceive<PageReady>>,
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
    commands.entity(entity).insert(HistoryQueryState::default());
}
