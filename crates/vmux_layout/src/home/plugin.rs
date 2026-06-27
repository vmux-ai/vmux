use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use vmux_command::event::COMMAND_BAR_OPEN_EVENT;
use vmux_command::open_target::OpenTarget;
use vmux_command::snapshot::{
    CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
};

use crate::command_bar::handler::{
    TabGatherParams, build_command_bar_open_payload, gather_command_bar_tabs,
};
use crate::home::event::HomeDataRequest;

pub struct HomePlugin;

impl Plugin for HomePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::home::PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(HomeDataRequest,)>::for_hosts(&[
            "home",
        ]))
        .add_observer(on_home_data_request);
    }
}

fn on_home_data_request(
    trigger: On<BinReceive<HomeDataRequest>>,
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
    fn home_plugin_spawns_manifest() {
        let mut app = App::new();
        app.add_plugins(HomePlugin);
        let mut q = app.world_mut().query::<&PageManifest>();
        assert!(q.iter(app.world()).any(|m| m.host == "home"));
    }
}
