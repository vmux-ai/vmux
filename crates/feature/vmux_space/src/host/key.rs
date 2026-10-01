use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_command::{BindCommands, CommandDispatch, CommandRegistry, CommandRuntimePlugin};
use vmux_core::host::UiStateWrite;
#[cfg(test)]
use vmux_core::host::manifest::FeaturePlugin;

use super::spaces::{SpaceSelection, Spaces, SpacesPageSnapshot};
use crate::event::{SpaceAttachRequest, SpaceDeleteRequest, SpacesUiState};

pub(crate) struct SpaceKeyPlugin;

impl Plugin for SpaceKeyPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_systems(Startup, bind_commands.in_set(BindCommands))
            .add_observer(select_next)
            .add_observer(select_previous)
            .add_observer(attach_selected)
            .add_observer(delete_selected);
    }
}

#[vmux_command::command(id = "space_next")]
#[derive(Component)]
struct SelectNextSpace;

#[vmux_command::command(id = "space_previous")]
#[derive(Component)]
struct SelectPreviousSpace;

#[vmux_command::command(id = "space_attach")]
#[derive(Component)]
struct AttachSelectedSpace;

#[vmux_command::command(id = "space_delete")]
#[derive(Component)]
struct DeleteSelectedSpace;

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<SelectNextSpace>(&mut commands);
    registry.bind::<SelectPreviousSpace>(&mut commands);
    registry.bind::<AttachSelectedSpace>(&mut commands);
    registry.bind::<DeleteSelectedSpace>(&mut commands);
}

fn select_next(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<SelectNextSpace>>,
    mut pages: Query<(&mut SpaceSelection, &mut SpacesPageSnapshot), With<Spaces>>,
    mut commands: Commands,
) {
    if bindings.get(trigger.event().command()).is_err() {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok((mut selection, mut snapshot)) = pages.get_mut(caller) else {
        return;
    };
    selection.0 = (selection.0 + 1).min(snapshot.0.spaces.len().saturating_sub(1));
    snapshot.0.selected = selection.0 as u32;
    commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
        caller,
        &snapshot.0,
    ));
}

fn select_previous(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<SelectPreviousSpace>>,
    mut pages: Query<(&mut SpaceSelection, &mut SpacesPageSnapshot), With<Spaces>>,
    mut commands: Commands,
) {
    if bindings.get(trigger.event().command()).is_err() {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok((mut selection, mut snapshot)) = pages.get_mut(caller) else {
        return;
    };
    selection.0 = selection.0.saturating_sub(1);
    snapshot.0.selected = selection.0 as u32;
    commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
        caller,
        &snapshot.0,
    ));
}

fn attach_selected(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<AttachSelectedSpace>>,
    pages: Query<&SpacesPageSnapshot, With<Spaces>>,
    mut commands: Commands,
) {
    if bindings.get(trigger.event().command()).is_err() {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok(snapshot) = pages.get(caller) else {
        return;
    };
    let Some(space) = snapshot.0.spaces.get(snapshot.0.selected as usize) else {
        return;
    };
    commands.trigger(UiInput {
        webview: caller,
        payload: SpaceAttachRequest {
            space_id: space.id.clone(),
        },
    });
}

fn delete_selected(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<DeleteSelectedSpace>>,
    pages: Query<&SpacesPageSnapshot, With<Spaces>>,
    mut commands: Commands,
) {
    if bindings.get(trigger.event().command()).is_err() {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok(snapshot) = pages.get(caller) else {
        return;
    };
    if snapshot.0.spaces.len() <= 1 {
        return;
    }
    let Some(space) = snapshot.0.spaces.get(snapshot.0.selected as usize) else {
        return;
    };
    commands.trigger(UiInput {
        webview: caller,
        payload: SpaceDeleteRequest {
            space_id: space.id.clone(),
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{SpaceRow, SpacesListEvent};
    use vmux_command::CommandInvocation;

    struct Keyboard;

    impl Keyboard {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins).add_plugins(SpaceKeyPlugin);
            app
        }

        fn page(app: &mut App) -> Entity {
            app.world_mut()
                .spawn((
                    Spaces,
                    SpacesPageSnapshot(SpacesListEvent {
                        spaces: vec![
                            SpaceRow {
                                id: "one".into(),
                                ..default()
                            },
                            SpaceRow {
                                id: "two".into(),
                                ..default()
                            },
                        ],
                        selected: 0,
                    }),
                ))
                .id()
        }

        fn issue(app: &mut App, caller: Entity, id: &str) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<CommandInvocation>>()
                .write(CommandInvocation::new(caller, id));
            app.update();
        }
    }

    #[test]
    fn selection_changes_only_for_the_page_that_sent_the_key() {
        let mut app = Keyboard::app();
        let pressed = Keyboard::page(&mut app);
        let other = Keyboard::page(&mut app);

        Keyboard::issue(&mut app, pressed, "space_next");

        assert_eq!(app.world().get::<SpaceSelection>(pressed).unwrap().0, 1);
        assert_eq!(app.world().get::<SpaceSelection>(other).unwrap().0, 0);
        assert_eq!(
            app.world()
                .get::<SpacesPageSnapshot>(pressed)
                .unwrap()
                .0
                .selected,
            1
        );
    }
}
