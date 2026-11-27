use bevy_app::{App, Last, Plugin, PostUpdate};
use bevy_cef::prelude::{
    BinHostEmitEvent, BinIpcBlocked, BinIpcEpoch, BinIpcNavigationRevision,
    BinIpcSurfaceGeneration, Browsers, UiInput,
};
use bevy_ecs::prelude::{
    Add, Commands, Entity, EntityEvent, Has, IntoScheduleConfigs, NonSend, On, Query, Remove,
    SystemSet, With, World,
};
use bevy_ecs::schedule::ApplyDeferred;
use vmux_api::PageReady;

use crate::page::PageActivated;
use crate::ui::{
    UiEmitEvent, UiIpcBlocked, UiIpcEpoch, UiIpcGeneration, UiIpcNavigationRevision, UiIpcPlugin,
    UiIpcReady, UiPageActivated, UiPageReady,
};
use crate::ui_state::{UiStateEmitSet, UiStateFlushSet};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CefUiSyncSet;

pub(crate) struct CefUiBridgePlugin;

impl Plugin for CefUiBridgePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<UiIpcPlugin>() {
            app.add_plugins(UiIpcPlugin);
        }
        app.add_observer(mirror_blocked_added)
            .add_observer(mirror_blocked_removed)
            .add_observer(forward_page_ready)
            .add_observer(forward_page_activated)
            .add_observer(forward_emit)
            .add_systems(
                Last,
                (sync_authority, ApplyDeferred)
                    .chain()
                    .in_set(CefUiSyncSet)
                    .before(UiStateEmitSet),
            )
            .add_systems(
                PostUpdate,
                (sync_authority, ApplyDeferred)
                    .chain()
                    .in_set(CefUiSyncSet)
                    .before(UiStateFlushSet),
            );
    }
}

fn mirror_blocked_added(trigger: On<Add, BinIpcBlocked>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .try_insert(UiIpcBlocked);
}

fn mirror_blocked_removed(trigger: On<Remove, BinIpcBlocked>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .try_remove::<UiIpcBlocked>();
}

fn forward_page_ready(trigger: On<UiInput<PageReady>>, mut commands: Commands) {
    commands.trigger(UiPageReady {
        page: trigger.event().webview,
    });
}

fn forward_page_activated(trigger: On<PageActivated>, mut commands: Commands) {
    commands.trigger(UiPageActivated {
        page: trigger.event().webview(),
    });
}

fn forward_emit(
    trigger: On<UiEmitEvent>,
    browsers: Option<NonSend<Browsers>>,
    cef_targets: Query<(), With<BinIpcSurfaceGeneration>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let target = event.target;
    if !cef_targets.contains(target) {
        return;
    }
    let Some(browsers) = browsers else {
        return;
    };
    if !browsers.can_emit_to(&target) {
        return;
    }
    let bin_event = BinHostEmitEvent::from_raw(
        target,
        event.id,
        event.permission,
        std::sync::Arc::clone(&event.payload),
        (event.generation, event.navigation_revision, event.ipc_epoch),
    );
    let receipt = event.receipt.clone();
    commands.queue(move |world: &mut World| {
        let bin_receipt = bin_event.receipt();
        world.trigger(bin_event);
        if bin_receipt.delivered() {
            receipt.mark_delivered();
        }
    });
}

type CefAuthority<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BinIpcSurfaceGeneration,
        &'static BinIpcNavigationRevision,
        &'static BinIpcEpoch,
        Option<&'static UiIpcGeneration>,
        Option<&'static UiIpcNavigationRevision>,
        Option<&'static UiIpcEpoch>,
        Has<BinIpcBlocked>,
        Has<UiIpcBlocked>,
        Has<UiIpcReady>,
    ),
>;

fn sync_authority(
    targets: CefAuthority,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    for (
        entity,
        bin_gen,
        bin_rev,
        bin_epoch,
        cur_gen,
        cur_rev,
        cur_epoch,
        bin_blocked,
        ipc_blocked,
        ipc_ready,
    ) in &targets
    {
        if cur_gen.is_none_or(|value| value.0 != bin_gen.0)
            || cur_rev.is_none_or(|value| value.0 != bin_rev.0)
            || cur_epoch.is_none_or(|value| value.0 != bin_epoch.0)
        {
            commands.entity(entity).try_insert((
                UiIpcGeneration(bin_gen.0),
                UiIpcNavigationRevision(bin_rev.0),
                UiIpcEpoch(bin_epoch.0),
            ));
        }
        if bin_blocked && !ipc_blocked {
            commands.entity(entity).try_insert(UiIpcBlocked);
        } else if !bin_blocked && ipc_blocked {
            commands.entity(entity).try_remove::<UiIpcBlocked>();
        }
        let can_emit = browsers
            .as_ref()
            .is_some_and(|browsers| browsers.can_emit_to(&entity));
        if can_emit && !ipc_ready {
            commands.entity(entity).try_insert(UiIpcReady);
        } else if !can_emit && ipc_ready {
            commands.entity(entity).try_remove::<UiIpcReady>();
        }
    }
}
