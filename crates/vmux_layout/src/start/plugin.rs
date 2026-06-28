use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use vmux_command::event::COMMAND_BAR_OPEN_EVENT;
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
};
use vmux_core::{CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

use crate::command_bar::handler::{
    TabGatherParams, build_command_bar_open_payload, gather_command_bar_tabs,
};
use crate::start::START_PAGE_URL;
use crate::start::event::StartDataRequest;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

/// Bevy plugin for `vmux://start/`: spawns the page manifest, claims start page-open
/// tasks, and answers [`StartDataRequest`] with the shared command-bar launcher payload.
pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::start::PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(StartDataRequest,)>::for_hosts(&[
            "start",
        ]))
        .add_observer(on_start_data_request)
        .add_systems(
            Update,
            handle_start_page_open.in_set(PageOpenSet::HandleKnownPages),
        );
    }
}

/// Claim `vmux://start/` page-open tasks and attach the launcher webview (titled "Start"),
/// so the start page is a first-class known page rather than falling to the unknown-URL page.
fn handle_start_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    mut attach: MessageWriter<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url != START_PAGE_URL {
            continue;
        }
        attach.write(CefPageAttachRequest {
            stack: task.stack,
            url: START_PAGE_URL.to_string(),
            title: "Start".to_string(),
            bg_color: None,
        });
        commands.entity(entity).insert(PageOpenHandled);
    }
}

/// Builds the command-bar launcher payload and emits it back to the requesting
/// `vmux://start/` webview as a `CommandBarOpenEvent` (opening in place).
fn on_start_data_request(
    trigger: On<BinReceive<StartDataRequest>>,
    tab_gather: TabGatherParams,
    spaces_snapshot: Res<CommandBarSpacesSnapshot>,
    agents_snapshot: Res<CommandBarAgentsSnapshot>,
    pages_snapshot: Res<CommandBarPagesSnapshot>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let active_stack_count = tab_gather.stack_q.iter().count();
    let space_name = spaces_snapshot.active_space_name.clone();
    let tabs = gather_command_bar_tabs(
        tab_gather.active_tab.get(),
        &tab_gather.all_children,
        &tab_gather.leaf_panes,
        &tab_gather.pane_ts,
        &tab_gather.pane_children,
        &tab_gather.stack_ts,
        &tab_gather.stack_q,
        &tab_gather.browser_meta,
        &tab_gather.child_of_q,
    );
    let payload = build_command_bar_open_payload(
        0,
        false,
        space_name,
        String::new(),
        &spaces_snapshot,
        &agents_snapshot,
        &pages_snapshot,
        active_stack_count,
        tabs,
        Some(OpenTarget::InPlace),
    );
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        COMMAND_BAR_OPEN_EVENT,
        &payload,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::page::PageManifest;

    #[test]
    fn start_plugin_spawns_manifest() {
        let mut app = App::new();
        app.add_plugins(StartPlugin);
        let mut q = app.world_mut().query::<&PageManifest>();
        assert!(q.iter(app.world()).any(|m| m.host == "start"));
    }
}
