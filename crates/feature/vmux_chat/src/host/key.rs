use super::composer::{ComposerChanged, ComposerState};
use super::session::{
    ChatSnapshotProjection, ChatTranscriptProjection, ChatView, PendingAgentChoice,
};
use crate::event::{
    ApprovalDecision, ChatApproval, ChatApprovalSelectionChanged, ChatApprovalSelectionEffect,
    ChatCancel, ChatChoiceSelected, ChatChoiceSelectionChanged, ChatChoiceSelectionEffect,
    ChatEscape, ChatListChooseEffect, ChatListMoveEffect, ChatSelectorDismissEffect, ChatSubmit,
};
use bevy_app::{App, Plugin, Startup};
use bevy_cef::prelude::UiInput;
use bevy_ecs::prelude::*;
use vmux_command::{BindCommands, CommandDispatch, CommandRegistry, CommandRuntimePlugin};
use vmux_ui::hooks::{MenuDirection, move_selection};
use vmux_ui::prompt_recall::PromptHistoryDirection;

pub struct ChatKeyPlugin;

impl Plugin for ChatKeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins((
            vmux_core::host::manifest::FeatureManifestPlugin::new(include_str!("../feature.ron")),
            bevy_cef::prelude::UiEventPlugin::<(
                ChatApprovalSelectionChanged,
                ChatChoiceSelectionChanged,
            )>::default(),
        ))
        .add_systems(Startup, bind_commands.in_set(BindCommands))
        .add_observer(move_list)
        .add_observer(choose_list)
        .add_observer(choose_number)
        .add_observer(select_approval)
        .add_observer(select_choice)
        .add_observer(move_history)
        .add_observer(submit)
        .add_observer(dismiss_selector)
        .add_observer(interrupt)
        .add_observer(cancel);
    }
}

#[derive(Component, Default)]
pub(crate) struct ChatKeyEffectRevision(u64);

#[derive(Component, Default)]
pub(super) struct ChatListSelection {
    approval_call_id: String,
    approval: usize,
    choice_question: String,
    choice_options: Vec<String>,
    choice: usize,
}

impl ChatListSelection {
    fn approval(&mut self, call_id: &str, len: usize) -> &mut usize {
        if self.approval_call_id != call_id {
            self.approval_call_id = call_id.to_string();
            self.approval = 0;
        }
        self.approval = self.approval.min(len.saturating_sub(1));
        &mut self.approval
    }

    fn choice(&mut self, choice: &PendingAgentChoice) -> &mut usize {
        if self.choice_question != choice.question || self.choice_options != choice.options {
            self.choice_question.clone_from(&choice.question);
            self.choice_options.clone_from(&choice.options);
            self.choice = 0;
        }
        self.choice = self.choice.min(choice.options.len().saturating_sub(1));
        &mut self.choice
    }
}

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

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.bind(&mut commands, "chat_list_next", ListNextBinding);
    registry.bind(&mut commands, "chat_list_previous", ListPreviousBinding);
    registry.bind(&mut commands, "chat_list_choose", ListChooseBinding);
    registry.bind(&mut commands, "chat_choice_1", ChoiceNumberBinding(0));
    registry.bind(&mut commands, "chat_choice_2", ChoiceNumberBinding(1));
    registry.bind(&mut commands, "chat_choice_3", ChoiceNumberBinding(2));
    registry.bind(&mut commands, "chat_history_older", HistoryOlderBinding);
    registry.bind(&mut commands, "chat_history_newer", HistoryNewerBinding);
    registry.bind(&mut commands, "chat_submit", SubmitBinding);
    registry.bind(
        &mut commands,
        "chat_dismiss_selector",
        DismissSelectorBinding,
    );
    registry.bind(&mut commands, "chat_interrupt", InterruptBinding);
    registry.bind(&mut commands, "chat_cancel", CancelBinding);
}

fn move_list(
    trigger: On<CommandDispatch>,
    next: Query<(), With<ListNextBinding>>,
    previous: Query<(), With<ListPreviousBinding>>,
    choices: Query<&PendingAgentChoice>,
    snapshots: Query<&ChatSnapshotProjection>,
    mut selections: Query<&mut ChatListSelection>,
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
    let direction = match next {
        true => MenuDirection::Next,
        false => MenuDirection::Previous,
    };
    let caller = trigger.event().invocation().caller;
    let Ok(mut selection) = selections.get_mut(caller) else {
        return;
    };
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    if let Ok(snapshot) = snapshots.get(caller)
        && let Some(approval) = &snapshot.0.approval
    {
        let selected = selection.approval(&approval.call_id, 3);
        *selected = move_selection(*selected, 3, direction);
        commands.trigger(
            vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
                caller,
                &ChatApprovalSelectionEffect {
                    revision: revision.0,
                    index: *selected as u32,
                },
            ),
        );
        return;
    }
    if let Ok(choice) = choices.get(caller) {
        let selected = selection.choice(choice);
        *selected = move_selection(*selected, choice.options.len(), direction);
        commands.trigger(
            vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
                caller,
                &ChatChoiceSelectionEffect {
                    revision: revision.0,
                    index: *selected as u32,
                },
            ),
        );
        return;
    }
    commands.trigger(
        vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
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
    choices: Query<&PendingAgentChoice>,
    snapshots: Query<&ChatSnapshotProjection>,
    mut selections: Query<&mut ChatListSelection>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
    let Ok(mut selection) = selections.get_mut(caller) else {
        return;
    };
    if let Ok(snapshot) = snapshots.get(caller)
        && let Some(approval) = &snapshot.0.approval
    {
        let selected = *selection.approval(&approval.call_id, 3);
        let decision = match selected {
            0 => ApprovalDecision::Allow,
            1 => ApprovalDecision::AllowAlways,
            2 => ApprovalDecision::Deny,
            _ => return,
        };
        commands.trigger(UiInput {
            webview: caller,
            payload: ChatApproval {
                call_id: approval.call_id.clone(),
                decision,
            },
        });
        return;
    }
    if let Ok(choice) = choices.get(caller) {
        let selected = *selection.choice(choice);
        commands.trigger(UiInput {
            webview: caller,
            payload: ChatChoiceSelected {
                index: selected as u32,
            },
        });
        return;
    }
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
            caller,
            &ChatListChooseEffect {
                revision: revision.0,
            },
        ),
    );
}

fn select_approval(
    trigger: On<UiInput<ChatApprovalSelectionChanged>>,
    snapshots: Query<&ChatSnapshotProjection>,
    mut selections: Query<&mut ChatListSelection>,
) {
    let webview = trigger.event().webview;
    let Ok(snapshot) = snapshots.get(webview) else {
        return;
    };
    let Some(approval) = &snapshot.0.approval else {
        return;
    };
    let Ok(mut selection) = selections.get_mut(webview) else {
        return;
    };
    *selection.approval(&approval.call_id, 3) = (trigger.event().payload.index as usize).min(2);
}

fn select_choice(
    trigger: On<UiInput<ChatChoiceSelectionChanged>>,
    choices: Query<&PendingAgentChoice>,
    mut selections: Query<&mut ChatListSelection>,
) {
    let webview = trigger.event().webview;
    let Ok(choice) = choices.get(webview) else {
        return;
    };
    let Ok(mut selection) = selections.get_mut(webview) else {
        return;
    };
    *selection.choice(choice) =
        (trigger.event().payload.index as usize).min(choice.options.len().saturating_sub(1));
}

fn choose_number(
    trigger: On<CommandDispatch>,
    bindings: Query<&ChoiceNumberBinding>,
    choices: Query<&PendingAgentChoice>,
    snapshots: Query<&ChatSnapshotProjection>,
    mut commands: Commands,
) {
    let Ok(binding) = bindings.get(trigger.event().command()) else {
        return;
    };
    let caller = trigger.event().invocation().caller;
    if let Ok(snapshot) = snapshots.get(caller)
        && let Some(approval) = &snapshot.0.approval
    {
        let decision = match binding.0 {
            0 => ApprovalDecision::Allow,
            1 => ApprovalDecision::AllowAlways,
            2 => ApprovalDecision::Deny,
            _ => return,
        };
        commands.trigger(UiInput {
            webview: caller,
            payload: ChatApproval {
                call_id: approval.call_id.clone(),
                decision,
            },
        });
        return;
    }
    let Ok(choice) = choices.get(caller) else {
        return;
    };
    if binding.0 as usize >= choice.options.len() {
        return;
    }
    commands.trigger(UiInput {
        webview: caller,
        payload: ChatChoiceSelected { index: binding.0 },
    });
}

fn move_history(
    trigger: On<CommandDispatch>,
    older: Query<(), With<HistoryOlderBinding>>,
    newer: Query<(), With<HistoryNewerBinding>>,
    mut views: Query<
        (
            &mut ComposerState,
            &ChatTranscriptProjection,
            &ChatSnapshotProjection,
        ),
        With<ChatView>,
    >,
    mut commands: Commands,
) {
    let command = trigger.event().command();
    let direction = if older.contains(command) {
        PromptHistoryDirection::Older
    } else if newer.contains(command) {
        PromptHistoryDirection::Newer
    } else {
        return;
    };
    let caller = trigger.event().invocation().caller;
    let Ok((mut composer, transcript, snapshot)) = views.get_mut(caller) else {
        return;
    };
    let history = transcript.prompt_history(snapshot);
    let effect = composer.recall(&history, direction);
    commands.trigger(
        vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(caller, &effect),
    );
    commands.trigger(ComposerChanged::new(caller));
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
    mut composers: Query<&mut ComposerState, With<ChatView>>,
    mut revisions: Query<&mut ChatKeyEffectRevision>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
    if let Ok(mut composer) = composers.get_mut(caller)
        && let Some(effect) = composer.dismiss_selector()
    {
        commands.trigger(
            vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(caller, &effect),
        );
        commands.trigger(ComposerChanged::new(caller));
        return;
    }
    let Ok(mut revision) = revisions.get_mut(caller) else {
        return;
    };
    revision.0 = revision.0.wrapping_add(1).max(1);
    commands.trigger(
        vmux_core::host::UiStateWrite::<super::state::ChatUiState>::from_event(
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
    use super::super::state::ChatUiState;
    use super::*;
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
    struct ChoiceNumbers(Vec<(Entity, u32)>);

    impl ChoiceNumbers {
        fn record(trigger: On<UiInput<ChatChoiceSelected>>, mut choices: ResMut<Self>) {
            choices
                .0
                .push((trigger.event().webview, trigger.event().payload.index));
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
        let pressed = app
            .world_mut()
            .spawn((
                ChatKeyEffectRevision::default(),
                ChatListSelection::default(),
            ))
            .id();
        let other = app
            .world_mut()
            .spawn((
                ChatKeyEffectRevision::default(),
                ChatListSelection::default(),
            ))
            .id();

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
        let page = app
            .world_mut()
            .spawn((
                ChatKeyEffectRevision::default(),
                ChatListSelection::default(),
                PendingAgentChoice {
                    session_entity: Entity::PLACEHOLDER,
                    question: "pick".to_string(),
                    options: vec!["one".to_string(), "two".to_string()],
                },
            ))
            .id();

        Echo::issue(&mut app, page, "chat_choice_2");

        assert_eq!(app.world().resource::<ChoiceNumbers>().0, vec![(page, 1)]);
    }

    #[test]
    fn choice_arrows_and_enter_are_resolved_in_host_ecs() {
        let mut app = Echo::app();
        let page = app
            .world_mut()
            .spawn((
                ChatKeyEffectRevision::default(),
                ChatListSelection::default(),
                PendingAgentChoice {
                    session_entity: Entity::PLACEHOLDER,
                    question: "pick".to_string(),
                    options: vec!["one".to_string(), "two".to_string()],
                },
            ))
            .id();

        Echo::issue(&mut app, page, "chat_list_next");
        Echo::issue(&mut app, page, "chat_list_choose");

        assert_eq!(app.world().resource::<ChoiceNumbers>().0, vec![(page, 1)]);
    }
}
