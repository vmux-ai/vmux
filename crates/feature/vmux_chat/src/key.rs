use crate::composer::ComposerState;
use crate::event::{
    ChatCancel, ChatChoiceNumberEffect, ChatEscape, ChatHistoryMoveEffect, ChatListChooseEffect,
    ChatListMoveEffect, ChatSelectorDismissEffect, ChatSubmit,
};
use crate::host::ChatView;
use bevy_app::{App, Plugin, Startup};
use bevy_cef::prelude::UiInput;
use bevy_ecs::prelude::*;
use vmux_command::{
    CommandDefinitions, CommandDispatch, CommandRuntimePlugin, RegisterCommandDefinitions,
};

pub struct ChatKeyPlugin;

impl Plugin for ChatKeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_systems(Startup, spawn_commands.in_set(RegisterCommandDefinitions))
            .add_observer(move_list)
            .add_observer(choose_list)
            .add_observer(choose_number)
            .add_observer(move_history)
            .add_observer(submit)
            .add_observer(dismiss_selector)
            .add_observer(interrupt)
            .add_observer(cancel);
    }
}

#[derive(Component, Default)]
pub(crate) struct ChatKeyEffectRevision(u64);

#[derive(Component)]
struct ListNextBinding;

#[derive(Component)]
struct ListPreviousBinding;

#[derive(Component)]
struct ListChooseBinding;

#[derive(Component)]
struct ChoiceNumberBinding(u32);

#[derive(Component)]
struct HistoryOlderBinding;

#[derive(Component)]
struct HistoryNewerBinding;

#[derive(Component)]
struct SubmitBinding;

#[derive(Component)]
struct DismissSelectorBinding;

#[derive(Component)]
struct InterruptBinding;

#[derive(Component)]
struct CancelBinding;

fn spawn_commands(mut commands: Commands) {
    let mut definitions = CommandDefinitions::from_feature_ron(include_str!("feature.ron"), "key");
    commands.spawn((definitions.take("chat_list_next"), ListNextBinding));
    commands.spawn((definitions.take("chat_list_previous"), ListPreviousBinding));
    commands.spawn((definitions.take("chat_list_choose"), ListChooseBinding));
    commands.spawn((definitions.take("chat_choice_1"), ChoiceNumberBinding(0)));
    commands.spawn((definitions.take("chat_choice_2"), ChoiceNumberBinding(1)));
    commands.spawn((definitions.take("chat_choice_3"), ChoiceNumberBinding(2)));
    commands.spawn((definitions.take("chat_history_older"), HistoryOlderBinding));
    commands.spawn((definitions.take("chat_history_newer"), HistoryNewerBinding));
    commands.spawn((definitions.take("chat_submit"), SubmitBinding));
    commands.spawn((
        definitions.take("chat_dismiss_selector"),
        DismissSelectorBinding,
    ));
    commands.spawn((definitions.take("chat_interrupt"), InterruptBinding));
    commands.spawn((definitions.take("chat_cancel"), CancelBinding));
    definitions.assert_all_registered();
}

fn move_list(
    trigger: On<CommandDispatch>,
    next: Query<(), With<ListNextBinding>>,
    previous: Query<(), With<ListPreviousBinding>>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    let command = trigger.event().command();
    let next = if next.contains(command) {
        true
    } else if previous.contains(command) {
        false
    } else {
        return;
    };
    let caller = trigger.event().invocation().caller;
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatListMoveEffect {
                revision: revision.0,
                next,
            },
        ),
    );
}

fn choose_list(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<ListChooseBinding>>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatListChooseEffect {
                revision: revision.0,
            },
        ),
    );
}

fn choose_number(
    trigger: On<CommandDispatch>,
    bindings: Query<&ChoiceNumberBinding>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    let Ok(binding) = bindings.get(trigger.event().command()) else {
        return;
    };
    let caller = trigger.event().invocation().caller;
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatChoiceNumberEffect {
                revision: revision.0,
                index: binding.0,
            },
        ),
    );
}

fn move_history(
    trigger: On<CommandDispatch>,
    older: Query<(), With<HistoryOlderBinding>>,
    newer: Query<(), With<HistoryNewerBinding>>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    let command = trigger.event().command();
    let older = if older.contains(command) {
        true
    } else if newer.contains(command) {
        false
    } else {
        return;
    };
    let caller = trigger.event().invocation().caller;
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatHistoryMoveEffect {
                revision: revision.0,
                older,
            },
        ),
    );
}

fn submit(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<SubmitBinding>>,
    composers: Query<&ComposerState, With<ChatView>>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok(composer) = composers.get(caller) else {
        return;
    };
    commands.trigger(UiInput {
        webview: caller,
        payload: ChatSubmit {
            text: composer.draft().trim().to_string(),
        },
    });
}

fn dismiss_selector(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<DismissSelectorBinding>>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatSelectorDismissEffect {
                revision: revision.0,
            },
        ),
    );
}

fn interrupt(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<InterruptBinding>>,
    views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    let caller = trigger.event().invocation().caller;
    if !bindings.contains(trigger.event().command()) || !views.contains(caller) {
        return;
    }
    commands.trigger(UiInput {
        webview: caller,
        payload: ChatEscape,
    });
}

fn cancel(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CancelBinding>>,
    views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    let caller = trigger.event().invocation().caller;
    if !bindings.contains(trigger.event().command()) || !views.contains(caller) {
        return;
    }
    commands.trigger(UiInput {
        webview: caller,
        payload: ChatCancel,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ChatUiState;
    use vmux_command::CommandInvocation;
    use vmux_core::host::UiStateWrite;

    #[derive(Resource, Default)]
    struct ListChoices(Vec<(Entity, u64)>);

    impl ListChoices {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut choices: ResMut<Self>) {
            let Some(effect) = trigger.event().patch().list_choose else {
                return;
            };
            choices.0.push((trigger.event().webview(), effect.revision));
        }
    }

    #[derive(Resource, Default)]
    struct ChoiceNumbers(Vec<(Entity, u64, u32)>);

    impl ChoiceNumbers {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut choices: ResMut<Self>) {
            let Some(effect) = trigger.event().patch().choice_number else {
                return;
            };
            choices
                .0
                .push((trigger.event().webview(), effect.revision, effect.index));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(ChatKeyPlugin)
                .init_resource::<ListChoices>()
                .init_resource::<ChoiceNumbers>()
                .add_observer(ListChoices::record)
                .add_observer(ChoiceNumbers::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, id: &str) {
            app.world_mut()
                .resource_mut::<bevy_ecs::message::Messages<CommandInvocation>>()
                .write(CommandInvocation::new(caller, id));
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn(ChatKeyEffectRevision::default()).id();
        let other = app.world_mut().spawn(ChatKeyEffectRevision::default()).id();

        Echo::issue(&mut app, pressed, "chat_list_choose");

        assert_eq!(app.world().resource::<ListChoices>().0, vec![(pressed, 1)]);
        assert!(
            !app.world()
                .resource::<ListChoices>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }

    #[test]
    fn a_numbered_choice_carries_its_index() {
        let mut app = Echo::app();
        let page = app.world_mut().spawn(ChatKeyEffectRevision::default()).id();

        Echo::issue(&mut app, page, "chat_choice_2");

        assert_eq!(
            app.world().resource::<ChoiceNumbers>().0,
            vec![(page, 1, 1)]
        );
    }
}
