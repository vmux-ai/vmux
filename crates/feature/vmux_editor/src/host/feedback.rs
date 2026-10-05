use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::event::{ExplorerNotice, ExplorerNoticeDismissRequest, FileEditNotice};

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ExplorerNoticeDismissRequest,)>::default())
            .add_observer(show_edit_failure)
            .add_observer(clear_edit_failure)
            .add_observer(show_explorer_notice)
            .add_observer(clear_explorer_notice)
            .add_observer(dismiss_explorer_notice)
            .add_systems(Update, (expire_edit_failure, expire_explorer_notice));
    }
}

#[derive(EntityEvent)]
pub(crate) struct EditFailure {
    #[event_target]
    target: Entity,
    reason: String,
}

impl EditFailure {
    pub(crate) fn new(target: Entity, reason: String) -> Self {
        Self { target, reason }
    }
}

#[derive(EntityEvent)]
pub(crate) struct ClearEditFailure(#[event_target] pub(crate) Entity);

#[derive(EntityEvent)]
pub(crate) struct ExplorerFeedback {
    #[event_target]
    target: Entity,
    ok: bool,
    message: String,
}

impl ExplorerFeedback {
    pub(crate) fn new(target: Entity, ok: bool, message: String) -> Self {
        Self {
            target,
            ok,
            message,
        }
    }
}

#[derive(EntityEvent)]
struct ClearExplorerNotice(#[event_target] Entity);

#[derive(Component)]
struct EditFailureTimer(Timer);

#[derive(Component)]
struct ExplorerNoticeTimer(Timer);

fn show_edit_failure(trigger: On<EditFailure>, mut commands: Commands) {
    let event = trigger.event();
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        trigger.event_target(),
        &FileEditNotice {
            reason: Some(event.reason.clone()),
        },
    ));
    commands
        .entity(trigger.event_target())
        .insert(EditFailureTimer(Timer::from_seconds(2.4, TimerMode::Once)));
}

fn clear_edit_failure(trigger: On<ClearEditFailure>, mut commands: Commands) {
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        trigger.event_target(),
        &FileEditNotice::default(),
    ));
    commands
        .entity(trigger.event_target())
        .remove::<EditFailureTimer>();
}

fn expire_edit_failure(
    time: Res<Time>,
    mut notices: Query<(Entity, &mut EditFailureTimer)>,
    mut commands: Commands,
) {
    for (entity, mut timer) in &mut notices {
        timer.0.tick(time.delta());
        if !timer.0.just_finished() {
            continue;
        }
        commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
            entity,
            &FileEditNotice::default(),
        ));
        commands.entity(entity).remove::<EditFailureTimer>();
    }
}

fn show_explorer_notice(trigger: On<ExplorerFeedback>, mut commands: Commands) {
    let event = trigger.event();
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        trigger.event_target(),
        &ExplorerNotice {
            ok: event.ok,
            message: Some(event.message.clone()),
        },
    ));
    commands
        .entity(trigger.event_target())
        .insert(ExplorerNoticeTimer(Timer::from_seconds(
            2.4,
            TimerMode::Once,
        )));
}

fn clear_explorer_notice(trigger: On<ClearExplorerNotice>, mut commands: Commands) {
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        trigger.event_target(),
        &ExplorerNotice::default(),
    ));
    commands
        .entity(trigger.event_target())
        .remove::<ExplorerNoticeTimer>();
}

fn dismiss_explorer_notice(
    trigger: On<UiInput<ExplorerNoticeDismissRequest>>,
    mut commands: Commands,
) {
    commands.trigger(ClearExplorerNotice(trigger.event().webview));
}

fn expire_explorer_notice(
    time: Res<Time>,
    mut notices: Query<(Entity, &mut ExplorerNoticeTimer)>,
    mut commands: Commands,
) {
    for (entity, mut timer) in &mut notices {
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            commands.trigger(ClearExplorerNotice(entity));
        }
    }
}
