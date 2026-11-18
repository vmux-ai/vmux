use super::composer::{ComposerChanged, ComposerState};
use super::model::{ModeProjection, ModelPickerProjection, SlashCommandProjection};
use super::session::{
    ChatBranchesProjection, ChatComposerContext, ChatMediaProjection, ChatResumeProjection,
    ChatSnapshotProjection, ChatTranscriptProjection, PendingAgentChoice, SessionView,
};
use crate::event::{
    ApprovalDecision, ChatApproval, ChatAttachPaths, ChatBranchesRequest, ChatCancel,
    ChatChoiceSelected, ChatComposerMenuKind, ChatComposerMenuRequest, ChatComposerMenuState,
    ChatDismissSelectorRequest, ChatEscape, ChatGoToBranch, ChatListChooseRequest, ChatListKind,
    ChatListSelectionChanged, ChatListSelectionState, ChatSelectWorkspace, ChatSelectorState,
    ChatSlashCommandRequest, ChatSubmit, ResumeSession, SelectMode, SelectModel, SetAgentEffort,
};
use bevy_app::{App, Plugin, Update};
use bevy_cef::prelude::UiInput;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::mcp::{McpServerEntry, McpServerSelectionRequest, McpServersUiState};
use vmux_api::prompt_media::InlineMediaQuery;
use vmux_command::{CommandBinding, CommandDispatch, CommandRuntimePlugin};
use vmux_ecs::UiState;
#[cfg(test)]
use vmux_feature::manifest::FeaturePlugin;
use vmux_ui::prompt_recall::PromptHistoryDirection;

use crate::selector::SelectorMode;

pub struct ChatKeyPlugin;

impl Plugin for ChatKeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins(bevy_cef::prelude::UiEventPlugin::<(
            ChatListSelectionChanged,
            ChatListChooseRequest,
            ChatComposerMenuRequest,
            ChatDismissSelectorRequest,
        )>::default())
            .add_systems(Update, project_selector)
            .add_observer(move_list)
            .add_observer(choose_shortcut)
            .add_observer(choose_from_ui)
            .add_observer(choose)
            .add_observer(choose_composer)
            .add_observer(choose_number)
            .add_observer(select_list)
            .add_observer(composer_menu)
            .add_observer(move_history)
            .add_observer(submit)
            .add_observer(dismiss_command)
            .add_observer(dismiss_input)
            .add_observer(dismiss)
            .add_observer(interrupt)
            .add_observer(cancel);
    }
}

#[derive(Component, Default)]
pub(super) struct ChatListSelection {
    list: Option<ChatListIdentity>,
    index: usize,
}

#[derive(Component, Default)]
pub(super) struct ChatSelectorProjection(pub ChatSelectorState);

impl ChatListSelection {
    fn current(&mut self, list: &ActiveChatList) -> &mut usize {
        if self.list.as_ref() != Some(&list.identity) {
            self.list = Some(list.identity.clone());
            self.index = list.initial;
        }
        self.index = self.index.min(list.len.saturating_sub(1));
        &mut self.index
    }

    fn update(&mut self, list: &ActiveChatList, index: usize) {
        self.list = Some(list.identity.clone());
        self.index = index.min(list.len.saturating_sub(1));
    }

    fn close_composer_menu(&mut self) {
        if matches!(self.list, Some(ChatListIdentity::Composer(_))) {
            self.list = None;
            self.index = 0;
        }
    }

    fn project(&mut self, list: &ActiveChatList) -> Option<ChatListSelectionState> {
        let previous_list = self.list.clone();
        let previous_index = self.index;
        let index = *self.current(list);
        if previous_list == self.list && previous_index == index {
            return None;
        }
        Some(ChatListSelectionState {
            kind: list.kind,
            index: index as u32,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
enum ChatListIdentity {
    Approval(String),
    Choice(String, Vec<String>),
    Composer(ChatComposerMenuKind),
    Media(u64, String),
    Mcp(String, Vec<String>),
    Session(u64, String),
    Model(String, Vec<String>),
    Command(String, Vec<String>),
}

struct ActiveChatList {
    kind: ChatListKind,
    identity: ChatListIdentity,
    len: usize,
    initial: usize,
}

impl ActiveChatList {
    fn composer_menu(&self, index: usize) -> Option<ActiveComposerMenu> {
        let ChatListIdentity::Composer(kind) = &self.identity else {
            return None;
        };
        Some(ActiveComposerMenu {
            menu: Some(*kind),
            index,
        })
    }
}

#[derive(Component, Clone, Copy, Default)]
pub(super) struct ActiveComposerMenu {
    menu: Option<ChatComposerMenuKind>,
    index: usize,
}

impl ChatSnapshotProjection {
    fn selected_approval(&self, index: usize) -> Option<ChatApproval> {
        let approval = self.0.approval.as_ref()?;
        let decision = match index {
            0 => ApprovalDecision::Allow,
            1 => ApprovalDecision::AllowAlways,
            2 => ApprovalDecision::Deny,
            _ => return None,
        };
        Some(ChatApproval {
            call_id: approval.call_id.clone(),
            decision,
        })
    }
}

impl ChatResumeProjection {
    fn selected_resume(&self, index: usize) -> Option<ResumeSession> {
        let session = self.0.sessions.get(index)?;
        Some(ResumeSession {
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            cwd: session.cwd.clone(),
            open: None,
        })
    }
}

impl ModelPickerProjection {
    fn selected_model(&self, draft: &str, index: usize) -> Option<SelectModel> {
        let SelectorMode::Models(query) = SelectorMode::from_draft(draft) else {
            return None;
        };
        let models = self.filtered(query);
        let model = models.get(index)?;
        Some(SelectModel {
            model_id: model.id.clone(),
        })
    }
}

impl SlashCommandProjection {
    fn selected_command(&self, draft: &str, index: usize) -> Option<ChatSlashCommandRequest> {
        let SelectorMode::Commands(query) = SelectorMode::from_draft(draft) else {
            return None;
        };
        let commands = self.filtered(query);
        let command = commands.get(index)?;
        Some(ChatSlashCommandRequest {
            command: command.command,
        })
    }
}

#[vmux_command::command]
struct ChatListNextBinding;

#[vmux_command::command]
struct ChatListPreviousBinding;

#[vmux_command::command]
struct ChatListChooseBinding;

#[derive(EntityEvent)]
struct ChooseComposer {
    #[event_target]
    target: Entity,
    index: usize,
}

#[derive(SystemParam)]
struct SelectionOutput<'w, 's> {
    commands: Commands<'w, 's>,
}

impl SelectionOutput<'_, '_> {
    fn select(
        &mut self,
        selection: &mut ChatListSelection,
        list: &ActiveChatList,
        index: Option<usize>,
        caller: Entity,
    ) -> usize {
        let Some(index) = index else {
            return *selection.current(list);
        };
        selection.update(list, index);
        self.commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                caller,
                &ChatListSelectionState {
                    kind: list.kind,
                    index: selection.index as u32,
                },
            ),
        );
        selection.index
    }

    fn finish(
        &mut self,
        caller: Entity,
        composer: &mut ComposerState,
        selection: &mut ChatListSelection,
        close_menu: bool,
        change_composer: Option<String>,
    ) {
        if close_menu {
            selection.close_composer_menu();
            self.commands
                .entity(caller)
                .insert(ActiveComposerMenu::default());
            self.commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                    caller,
                    &ChatComposerMenuState {
                        menu: None,
                        index: 0,
                    },
                ),
            );
            let draft = composer.draft().to_string();
            let effect = composer.effect(draft, true);
            self.commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
            );
        }
        if let Some(draft) = change_composer {
            let effect = composer.effect(draft, true);
            self.commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
            );
            self.commands.trigger(ComposerChanged::new(caller));
        }
    }
}

#[derive(EntityEvent)]
struct ChooseList {
    #[event_target]
    target: Entity,
    index: Option<usize>,
}

#[derive(EntityEvent)]
struct DismissSelector {
    #[event_target]
    target: Entity,
}

#[vmux_command::command(binding)]
#[derive(Component)]
struct ChoiceNumberBinding(u32);

impl CommandBinding for ChoiceNumberBinding {
    fn for_command(id: &str) -> Option<Self> {
        match id {
            "chat_choice_1" => Some(Self(0)),
            "chat_choice_2" => Some(Self(1)),
            "chat_choice_3" => Some(Self(2)),
            _ => None,
        }
    }
}

#[vmux_command::command]
struct ChatHistoryOlderBinding;

#[vmux_command::command]
struct ChatHistoryNewerBinding;

#[vmux_command::command]
struct ChatSubmitBinding;

#[vmux_command::command]
struct ChatDismissSelectorBinding;

#[vmux_command::command]
struct ChatInterruptBinding;

#[vmux_command::command]
struct ChatCancelBinding;

#[derive(SystemParam)]
struct ChatLists<'w, 's> {
    snapshots: Query<'w, 's, &'static ChatSnapshotProjection>,
    choices: Query<'w, 's, &'static PendingAgentChoice>,
    menus: Query<'w, 's, &'static ActiveComposerMenu>,
    media: Query<'w, 's, &'static ChatMediaProjection>,
    resumes: Query<'w, 's, &'static ChatResumeProjection>,
    models: Query<'w, 's, &'static ModelPickerProjection>,
    modes: Query<'w, 's, &'static ModeProjection>,
    contexts: Query<'w, 's, &'static ChatComposerContext>,
    branches: Query<'w, 's, &'static ChatBranchesProjection>,
    commands: Query<'w, 's, &'static SlashCommandProjection>,
    mcp: Query<'w, 's, &'static UiState<McpServersUiState>>,
}

impl ChatLists<'_, '_> {
    fn selected_media(
        &self,
        caller: Entity,
        draft: &str,
        index: usize,
    ) -> Option<(Option<ChatAttachPaths>, String)> {
        let media = self.media.get(caller).ok()?;
        let entry = media.0.entries.get(index)?;
        let query = InlineMediaQuery::parse(draft)?;
        let reference = entry.reference();
        let (attachment, replacement) = if entry.is_dir {
            (None, format!("@{reference}/"))
        } else {
            (
                Some(ChatAttachPaths {
                    paths: vec![entry.path.clone()],
                }),
                String::new(),
            )
        };
        Some((attachment, query.replace(draft, &replacement)))
    }

    fn selected_command(
        &self,
        caller: Entity,
        draft: &str,
        index: usize,
    ) -> Option<ChatSlashCommandRequest> {
        let projection = self.commands.get(caller).ok()?;
        projection.selected_command(draft, index)
    }

    fn selected_model(&self, caller: Entity, draft: &str, index: usize) -> Option<SelectModel> {
        let projection = self.models.get(caller).ok()?;
        projection.selected_model(draft, index)
    }

    fn selected_resume(&self, caller: Entity, index: usize) -> Option<ResumeSession> {
        let projection = self.resumes.get(caller).ok()?;
        projection.selected_resume(index)
    }

    fn selected_approval(&self, caller: Entity, index: usize) -> Option<ChatApproval> {
        let projection = self.snapshots.get(caller).ok()?;
        projection.selected_approval(index)
    }

    fn selected_mcp(
        &self,
        caller: Entity,
        draft: &str,
        index: usize,
    ) -> Option<McpServerSelectionRequest> {
        let state = self.mcp.get(caller).ok()?.current()?;
        if state.pending.is_some() {
            return None;
        }
        let SelectorMode::Mcp(query) = SelectorMode::from_draft(draft) else {
            return None;
        };
        let entries = self.mcp_entries(caller, query);
        let server = entries.get(index)?;
        Some(McpServerSelectionRequest {
            id: server.id.clone(),
        })
    }

    fn selector(&self, webview: Entity, draft: &str) -> ChatSelectorState {
        let Some(active) = self.active(webview, draft) else {
            return ChatSelectorState {
                key_context: vec!["chat".to_string()],
                ..Default::default()
            };
        };
        let mut state = match active.kind {
            ChatListKind::Approval
            | ChatListKind::Choice
            | ChatListKind::Composer
            | ChatListKind::Media
            | ChatListKind::Session => ChatSelectorState {
                active: Some(active.kind),
                ..Default::default()
            },
            ChatListKind::Mcp => {
                let SelectorMode::Mcp(query) = SelectorMode::from_draft(draft) else {
                    return ChatSelectorState::default();
                };
                ChatSelectorState {
                    active: Some(active.kind),
                    mcp_servers: self.mcp_entries(webview, query),
                    ..Default::default()
                }
            }
            ChatListKind::Model => {
                let SelectorMode::Models(query) = SelectorMode::from_draft(draft) else {
                    return ChatSelectorState::default();
                };
                ChatSelectorState {
                    active: Some(active.kind),
                    models: self
                        .models
                        .get(webview)
                        .map(|projection| projection.filtered(query))
                        .unwrap_or_default(),
                    ..Default::default()
                }
            }
            ChatListKind::Command => {
                let SelectorMode::Commands(query) = SelectorMode::from_draft(draft) else {
                    return ChatSelectorState::default();
                };
                ChatSelectorState {
                    active: Some(active.kind),
                    commands: self
                        .commands
                        .get(webview)
                        .map(|projection| projection.filtered(query))
                        .unwrap_or_default(),
                    ..Default::default()
                }
            }
        };
        state.key_context.push("chat".to_string());
        state.key_context.push("chat.list".to_string());
        if matches!(active.kind, ChatListKind::Approval | ChatListKind::Choice) {
            state.key_context.push("chat.choice".to_string());
        }
        if active.kind.is_selector() {
            state.key_context.push("chat.selector".to_string());
        }
        state
    }

    fn active(&self, webview: Entity, draft: &str) -> Option<ActiveChatList> {
        if let Ok(snapshot) = self.snapshots.get(webview)
            && let Some(approval) = &snapshot.0.approval
        {
            return Some(ActiveChatList {
                kind: ChatListKind::Approval,
                identity: ChatListIdentity::Approval(approval.call_id.clone()),
                len: 3,
                initial: 0,
            });
        }
        if let Ok(choice) = self.choices.get(webview) {
            return Some(ActiveChatList {
                kind: ChatListKind::Choice,
                identity: ChatListIdentity::Choice(choice.question.clone(), choice.options.clone()),
                len: choice.options.len(),
                initial: 0,
            });
        }
        if let Ok(menu) = self.menus.get(webview)
            && let Some(kind) = menu.menu
        {
            return Some(ActiveChatList {
                kind: ChatListKind::Composer,
                identity: ChatListIdentity::Composer(kind),
                len: self.composer_rows(webview, kind),
                initial: menu.index,
            });
        }
        if let Some(query) = InlineMediaQuery::parse(draft) {
            let projection = self.media.get(webview).ok();
            return Some(ActiveChatList {
                kind: ChatListKind::Media,
                identity: ChatListIdentity::Media(
                    projection
                        .map(|projection| projection.0.request_id)
                        .unwrap_or_default(),
                    query.query.to_string(),
                ),
                len: projection
                    .map(|projection| projection.0.entries.len())
                    .unwrap_or_default(),
                initial: 0,
            });
        }
        match SelectorMode::from_draft(draft) {
            SelectorMode::Mcp(query) => {
                let entries = self.mcp_entries(webview, query);
                Some(ActiveChatList {
                    kind: ChatListKind::Mcp,
                    identity: ChatListIdentity::Mcp(
                        query.to_string(),
                        entries.iter().map(|entry| entry.id.clone()).collect(),
                    ),
                    len: entries.len(),
                    initial: 0,
                })
            }
            SelectorMode::Resume(query) => {
                let projection = self.resumes.get(webview).ok();
                Some(ActiveChatList {
                    kind: ChatListKind::Session,
                    identity: ChatListIdentity::Session(
                        projection
                            .map(|projection| projection.0.request_id)
                            .unwrap_or_default(),
                        query.to_string(),
                    ),
                    len: projection
                        .map(|projection| projection.0.sessions.len())
                        .unwrap_or_default(),
                    initial: 0,
                })
            }
            SelectorMode::Models(query) => {
                let entries = self
                    .models
                    .get(webview)
                    .map(|projection| projection.filtered(query))
                    .unwrap_or_default();
                Some(ActiveChatList {
                    kind: ChatListKind::Model,
                    identity: ChatListIdentity::Model(
                        query.to_string(),
                        entries.iter().map(|entry| entry.id.clone()).collect(),
                    ),
                    len: entries.len(),
                    initial: 0,
                })
            }
            SelectorMode::Commands(query) => {
                let entries = self
                    .commands
                    .get(webview)
                    .map(|projection| projection.filtered(query))
                    .unwrap_or_default();
                if entries.is_empty() {
                    return None;
                }
                Some(ActiveChatList {
                    kind: ChatListKind::Command,
                    identity: ChatListIdentity::Command(
                        query.to_string(),
                        entries
                            .iter()
                            .map(|entry| entry.command.name().to_string())
                            .collect(),
                    ),
                    len: entries.len(),
                    initial: 0,
                })
            }
            SelectorMode::None => None,
        }
    }

    fn composer_rows(&self, webview: Entity, kind: ChatComposerMenuKind) -> usize {
        match kind {
            ChatComposerMenuKind::Effort => self
                .models
                .get(webview)
                .map(|projection| projection.0.effort_levels.len() + 1)
                .unwrap_or_default(),
            ChatComposerMenuKind::Permission => self
                .modes
                .get(webview)
                .map(|projection| projection.0.modes.len())
                .unwrap_or_default(),
            ChatComposerMenuKind::Project => self
                .contexts
                .get(webview)
                .map(|context| {
                    let roots = context
                        .0
                        .projects
                        .iter()
                        .filter(|project| project.depth == 0)
                        .count();
                    roots + 1
                })
                .unwrap_or_default(),
            ChatComposerMenuKind::Branch => self
                .branches
                .get(webview)
                .map(|projection| projection.0.branches.len())
                .unwrap_or_default(),
        }
    }

    fn mcp_entries(&self, webview: Entity, query: &str) -> Vec<McpServerEntry> {
        let Ok(state) = self.mcp.get(webview) else {
            return Vec::new();
        };
        let Some(state) = state.current() else {
            return Vec::new();
        };
        let query = query.trim().to_ascii_lowercase();
        let mut matching = Vec::new();
        for server in &state.servers {
            if query.is_empty()
                || server.id.to_ascii_lowercase().contains(&query)
                || server.name.to_ascii_lowercase().contains(&query)
                || server.description.to_ascii_lowercase().contains(&query)
            {
                matching.push(server.clone());
            }
        }
        matching
    }
}

fn project_selector(
    lists: ChatLists,
    composers: Query<&ComposerState>,
    mut projections: Query<(Entity, &mut ChatSelectorProjection), With<SessionView>>,
    mut selections: Query<&mut ChatListSelection>,
    mut commands: Commands,
) {
    for (webview, mut projection) in &mut projections {
        let Ok(composer) = composers.get(webview) else {
            continue;
        };
        if let Some(list) = lists.active(webview, composer.draft())
            && let Ok(mut selection) = selections.get_mut(webview)
            && let Some(state) = selection.project(&list)
        {
            commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &state),
            );
        }
        let state = lists.selector(webview, composer.draft());
        if projection.0 == state {
            continue;
        }
        projection.0 = state.clone();
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &state),
        );
    }
}

fn move_list(
    trigger: On<CommandDispatch>,
    next: Query<(), With<ChatListNextBinding>>,
    previous: Query<(), With<ChatListPreviousBinding>>,
    lists: ChatLists,
    composers: Query<&ComposerState>,
    mut selections: Query<&mut ChatListSelection>,
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
    let Ok(composer) = composers.get(caller) else {
        return;
    };
    let Some(list) = lists.active(caller, composer.draft()) else {
        return;
    };
    let Ok(mut selection) = selections.get_mut(caller) else {
        return;
    };
    let kind = list.kind;
    let len = list.len;
    let selected = selection.current(&list);
    if len == 0 {
        *selected = 0;
    } else if next {
        *selected = (*selected + 1) % len;
    } else {
        *selected = (*selected + len - 1) % len;
    }
    commands.trigger(
        vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatListSelectionState {
                kind,
                index: *selected as u32,
            },
        ),
    );
    if let Some(menu) = list.composer_menu(*selected) {
        commands.entity(caller).insert(menu);
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                caller,
                &ChatComposerMenuState {
                    menu: menu.menu,
                    index: menu.index as u32,
                },
            ),
        );
    }
}

fn choose_shortcut(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<ChatListChooseBinding>>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(ChooseList {
        target: trigger.event().invocation().caller,
        index: None,
    });
}

fn choose_from_ui(trigger: On<UiInput<ChatListChooseRequest>>, mut commands: Commands) {
    commands.trigger(ChooseList {
        target: trigger.event().webview,
        index: Some(trigger.event().payload.index as usize),
    });
}

fn choose(
    trigger: On<ChooseList>,
    lists: ChatLists,
    mut views: Query<(&mut ComposerState, &mut ChatListSelection)>,
    mut output: SelectionOutput,
) {
    let caller = trigger.event_target();
    let Ok((mut composer, mut selection)) = views.get_mut(caller) else {
        return;
    };
    let Some(list) = lists.active(caller, composer.draft()) else {
        return;
    };
    let kind = list.kind;
    let selected = output.select(&mut selection, &list, trigger.event().index, caller);
    let mut change_composer = None;
    match kind {
        ChatListKind::Approval => {
            let Some(payload) = lists.selected_approval(caller, selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload,
            });
        }
        ChatListKind::Choice => {
            output.commands.trigger(UiInput {
                webview: caller,
                payload: ChatChoiceSelected {
                    index: selected as u32,
                },
            });
        }
        ChatListKind::Composer => {
            output.commands.trigger(ChooseComposer {
                target: caller,
                index: selected,
            });
            return;
        }
        ChatListKind::Media => {
            let Some((attachment, draft)) =
                lists.selected_media(caller, composer.draft(), selected)
            else {
                return;
            };
            if let Some(payload) = attachment {
                output.commands.trigger(UiInput {
                    webview: caller,
                    payload,
                });
            }
            change_composer = Some(draft);
        }
        ChatListKind::Mcp => {
            let Some(payload) = lists.selected_mcp(caller, composer.draft(), selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload,
            });
        }
        ChatListKind::Session => {
            let Some(payload) = lists.selected_resume(caller, selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload,
            });
            change_composer = Some(String::new());
        }
        ChatListKind::Model => {
            let Some(payload) = lists.selected_model(caller, composer.draft(), selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload,
            });
            change_composer = Some(String::new());
        }
        ChatListKind::Command => {
            let Some(payload) = lists.selected_command(caller, composer.draft(), selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload,
            });
        }
    }
    output.finish(
        caller,
        &mut composer,
        &mut selection,
        false,
        change_composer,
    );
}

fn choose_composer(
    trigger: On<ChooseComposer>,
    lists: ChatLists,
    mut states: Query<(&mut ComposerState, &mut ChatListSelection)>,
    mut output: SelectionOutput,
) {
    let caller = trigger.event().target;
    let selected = trigger.event().index;
    let Ok((mut composer, mut selection)) = states.get_mut(caller) else {
        return;
    };
    let Ok(menu) = lists.menus.get(caller) else {
        return;
    };
    let Some(kind) = menu.menu else {
        return;
    };
    match kind {
        ChatComposerMenuKind::Effort => {
            let Ok(model) = lists.models.get(caller) else {
                return;
            };
            let level = if selected == 0 {
                String::new()
            } else {
                let Some(level) = model.0.effort_levels.get(selected - 1) else {
                    return;
                };
                level.clone()
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload: SetAgentEffort {
                    agent_key: model.0.agent_key.clone(),
                    level,
                },
            });
        }
        ChatComposerMenuKind::Permission => {
            let Ok(mode) = lists.modes.get(caller) else {
                return;
            };
            let Some(mode) = mode.0.modes.get(selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload: SelectMode {
                    mode_id: mode.id.clone(),
                },
            });
        }
        ChatComposerMenuKind::Project => {
            if !choose_project(caller, selected, &lists, &mut output.commands) {
                return;
            }
        }
        ChatComposerMenuKind::Branch => {
            let Ok(branches) = lists.branches.get(caller) else {
                return;
            };
            let Some(branch) = branches.0.branches.get(selected) else {
                return;
            };
            output.commands.trigger(UiInput {
                webview: caller,
                payload: ChatGoToBranch {
                    project: branches.0.project.clone(),
                    branch: branch.branch.clone(),
                    checkout: branch.checkout.clone(),
                },
            });
        }
    }
    output.finish(caller, &mut composer, &mut selection, true, None);
}

fn choose_project(
    caller: Entity,
    selected: usize,
    lists: &ChatLists,
    commands: &mut Commands,
) -> bool {
    let Ok(context) = lists.contexts.get(caller) else {
        return false;
    };
    let mut roots = Vec::new();
    for project in &context.0.projects {
        if project.depth == 0 {
            roots.push(project);
        }
    }
    if selected == roots.len() {
        commands.trigger(UiInput {
            webview: caller,
            payload: ChatSelectWorkspace,
        });
    } else {
        let Some(project) = roots.get(selected) else {
            return false;
        };
        commands.trigger(UiInput {
            webview: caller,
            payload: ChatGoToBranch {
                project: project.path.clone(),
                branch: String::new(),
                checkout: String::new(),
            },
        });
    }

    true
}

fn select_list(
    trigger: On<UiInput<ChatListSelectionChanged>>,
    lists: ChatLists,
    composers: Query<&ComposerState>,
    mut selections: Query<&mut ChatListSelection>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(composer) = composers.get(webview) else {
        return;
    };
    let Some(list) = lists.active(webview, composer.draft()) else {
        return;
    };
    let Ok(mut selection) = selections.get_mut(webview) else {
        return;
    };
    selection.update(&list, trigger.event().payload.index as usize);
    commands.trigger(
        vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
            webview,
            &ChatListSelectionState {
                kind: list.kind,
                index: selection.index as u32,
            },
        ),
    );
    if let Some(menu) = list.composer_menu(selection.index) {
        commands.entity(webview).insert(menu);
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &ChatComposerMenuState {
                    menu: menu.menu,
                    index: menu.index as u32,
                },
            ),
        );
    }
}

fn composer_menu(
    trigger: On<UiInput<ChatComposerMenuRequest>>,
    mut menus: Query<&mut ActiveComposerMenu>,
    mut selections: Query<&mut ChatListSelection>,
    mut composers: Query<&mut ComposerState, With<SessionView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(mut menu) = menus.get_mut(webview) else {
        return;
    };
    let requested = trigger.event().payload.menu;
    menu.menu = match requested {
        Some(requested) if menu.menu == Some(requested) => None,
        requested => requested,
    };
    menu.index = trigger.event().payload.index as usize;
    if let Ok(mut selection) = selections.get_mut(webview) {
        match menu.menu {
            Some(kind) => {
                selection.list = Some(ChatListIdentity::Composer(kind));
                selection.index = menu.index;
            }
            None => selection.close_composer_menu(),
        }
    }
    if menu.menu.is_some()
        && let Ok(mut composer) = composers.get_mut(webview)
        && let Some(effect) = composer.dismiss_selector()
    {
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &effect),
        );
        commands.trigger(ComposerChanged::new(webview));
    }
    commands.trigger(
        vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
            webview,
            &ChatComposerMenuState {
                menu: menu.menu,
                index: menu.index as u32,
            },
        ),
    );
    if menu.menu == Some(ChatComposerMenuKind::Branch) {
        commands.trigger(UiInput {
            webview,
            payload: ChatBranchesRequest,
        });
    }
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
    older: Query<(), With<ChatHistoryOlderBinding>>,
    newer: Query<(), With<ChatHistoryNewerBinding>>,
    mut views: Query<
        (
            &mut ComposerState,
            &ChatTranscriptProjection,
            &ChatSnapshotProjection,
        ),
        With<SessionView>,
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
    commands
        .trigger(vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect));
    commands.trigger(ComposerChanged::new(caller));
}

fn submit(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<ChatSubmitBinding>>,
    composers: Query<&ComposerState, With<SessionView>>,
    targets: super::session::SessionViews,
    sessions: Query<(
        &crate::Preparation,
        &crate::Authentication,
        Has<crate::SetupPending>,
    )>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
    if let Some(session) = targets.session(caller)
        && sessions
            .get(session)
            .is_ok_and(|(preparation, authentication, setup)| {
                setup || preparation.active() || authentication.active
            })
    {
        return;
    }
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

fn dismiss_command(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<ChatDismissSelectorBinding>>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(DismissSelector {
        target: trigger.event().invocation().caller,
    });
}

fn dismiss_input(trigger: On<UiInput<ChatDismissSelectorRequest>>, mut commands: Commands) {
    commands.trigger(DismissSelector {
        target: trigger.event().webview,
    });
}

fn dismiss(
    trigger: On<DismissSelector>,
    menus: Query<&ActiveComposerMenu>,
    mut composers: Query<&mut ComposerState, With<SessionView>>,
    mut selections: Query<&mut ChatListSelection>,
    mut commands: Commands,
) {
    let caller = trigger.event_target();
    if let Ok(menu) = menus.get(caller)
        && menu.menu.is_some()
    {
        commands
            .entity(caller)
            .insert(ActiveComposerMenu::default());
        if let Ok(mut selection) = selections.get_mut(caller) {
            selection.close_composer_menu();
        }
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
                caller,
                &ChatComposerMenuState {
                    menu: None,
                    index: 0,
                },
            ),
        );
        if let Ok(mut composer) = composers.get_mut(caller) {
            let draft = composer.draft().to_string();
            let effect = composer.effect(draft, true);
            commands.trigger(
                vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
            );
        }
        return;
    }
    if let Ok(mut composer) = composers.get_mut(caller)
        && let Some(effect) = composer.dismiss_selector()
    {
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
        );
        commands.trigger(ComposerChanged::new(caller));
    }
}

fn interrupt(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<ChatInterruptBinding>>,
    views: Query<(), With<SessionView>>,
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
    bindings: Query<(), With<ChatCancelBinding>>,
    views: Query<(), With<SessionView>>,
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
    use crate::event::{ChatComposerEffect, ModelOptionEntry, ModelState};
    use crate::state::ChatUiState;
    use bevy::MinimalPlugins;
    use vmux_command::CommandInvocation;
    use vmux_ecs::UiStateWrite;

    #[derive(Resource, Default)]
    struct ListSelections(Vec<(Entity, ChatListKind, u32)>);

    impl ListSelections {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut selections: ResMut<Self>) {
            let Some(state) = trigger.event().update().list_selection else {
                return;
            };
            selections
                .0
                .push((trigger.event().webview(), state.kind, state.index));
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

    #[derive(Resource, Default)]
    struct ComposerEffects(Vec<(Entity, ChatComposerEffect)>);

    impl ComposerEffects {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut effects: ResMut<Self>) {
            let Some(effect) = trigger.event().update().composer_effect.clone() else {
                return;
            };
            effects.0.push((trigger.event().webview(), effect));
        }
    }

    #[derive(Resource, Default)]
    struct ComposerMenus(Vec<(Entity, ChatComposerMenuState)>);

    impl ComposerMenus {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut menus: ResMut<Self>) {
            let Some(menu) = trigger.event().update().composer_menu.clone() else {
                return;
            };
            menus.0.push((trigger.event().webview(), menu));
        }
    }

    #[derive(Resource, Default)]
    struct BranchRequests(Vec<Entity>);

    impl BranchRequests {
        fn record(trigger: On<UiInput<ChatBranchesRequest>>, mut requests: ResMut<Self>) {
            requests.0.push(trigger.event().webview);
        }
    }

    #[derive(Resource, Default)]
    struct WorkspaceRequests(Vec<Entity>);

    impl WorkspaceRequests {
        fn record(trigger: On<UiInput<ChatSelectWorkspace>>, mut requests: ResMut<Self>) {
            requests.0.push(trigger.event().webview);
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                FeaturePlugin::<crate::Feature>::default(),
                ChatKeyPlugin,
            ))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .init_resource::<ListSelections>()
            .init_resource::<ChoiceNumbers>()
            .init_resource::<ComposerEffects>()
            .init_resource::<ComposerMenus>()
            .init_resource::<BranchRequests>()
            .add_observer(ListSelections::record)
            .add_observer(ChoiceNumbers::record)
            .add_observer(ComposerEffects::record)
            .add_observer(ComposerMenus::record)
            .add_observer(BranchRequests::record);
            app.update();
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
                ComposerState::default(),
                ChatListSelection::default(),
                PendingAgentChoice {
                    session_entity: Entity::PLACEHOLDER,
                    question: "pick".to_string(),
                    options: vec!["one".to_string(), "two".to_string()],
                },
            ))
            .id();
        let other = app
            .world_mut()
            .spawn((
                ComposerState::default(),
                ChatListSelection::default(),
                PendingAgentChoice {
                    session_entity: Entity::PLACEHOLDER,
                    question: "other".to_string(),
                    options: vec!["one".to_string(), "two".to_string()],
                },
            ))
            .id();

        Echo::issue(&mut app, pressed, "chat_list_next");

        assert_eq!(
            app.world().resource::<ListSelections>().0,
            vec![(pressed, ChatListKind::Choice, 1)]
        );
        assert!(
            !app.world()
                .resource::<ListSelections>()
                .0
                .iter()
                .any(|(entity, _, _)| *entity == other)
        );
    }

    #[test]
    fn a_numbered_choice_carries_its_index() {
        let mut app = Echo::app();
        let page = app
            .world_mut()
            .spawn((
                ComposerState::default(),
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
                ComposerState::default(),
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

    #[test]
    fn pointer_choice_uses_the_same_host_selection_path() {
        let mut app = Echo::app();
        let page = app
            .world_mut()
            .spawn((
                ComposerState::default(),
                ChatListSelection::default(),
                PendingAgentChoice {
                    session_entity: Entity::PLACEHOLDER,
                    question: "pick".to_string(),
                    options: vec!["one".to_string(), "two".to_string()],
                },
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: ChatListChooseRequest { index: 1 },
        });
        app.world_mut().flush();

        assert_eq!(app.world().resource::<ChoiceNumbers>().0, vec![(page, 1)]);
    }

    #[test]
    fn selector_filtering_is_projected_by_host_ecs() {
        let mut app = Echo::app();
        let page = app
            .world_mut()
            .spawn((
                SessionView,
                ModelPickerProjection(ModelState {
                    models: vec![
                        ModelOptionEntry {
                            id: "claude-sonnet".into(),
                            name: "Sonnet".into(),
                            description: "Balanced".into(),
                        },
                        ModelOptionEntry {
                            id: "claude-opus".into(),
                            name: "Opus".into(),
                            description: "Most capable".into(),
                        },
                    ],
                    ..Default::default()
                }),
            ))
            .id();
        app.world_mut()
            .get_mut::<ComposerState>(page)
            .unwrap()
            .update("/model son");

        app.update();

        let selector = &app.world().get::<ChatSelectorProjection>(page).unwrap().0;
        assert_eq!(selector.active, Some(ChatListKind::Model));
        assert_eq!(selector.models.len(), 1);
        assert_eq!(selector.models[0].id, "claude-sonnet");
    }

    #[test]
    fn ui_dismissal_uses_host_composer_state() {
        let mut app = Echo::app();
        let page = app.world_mut().spawn(SessionView).id();
        app.world_mut()
            .get_mut::<ComposerState>(page)
            .unwrap()
            .update("/resume previous");

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: ChatDismissSelectorRequest,
        });
        app.world_mut().flush();

        let effects = &app.world().resource::<ComposerEffects>().0;
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].0, page);
        assert!(effects[0].1.draft.is_empty());
        assert!(effects[0].1.focus);
    }

    #[test]
    fn composer_menu_is_host_authoritative() {
        let mut app = Echo::app();
        let page = app.world_mut().spawn(SessionView).id();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: ChatComposerMenuRequest {
                menu: Some(ChatComposerMenuKind::Branch),
                index: 2,
            },
        });
        app.world_mut().flush();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: ChatComposerMenuRequest {
                menu: Some(ChatComposerMenuKind::Branch),
                index: 2,
            },
        });
        app.world_mut().flush();

        let menus = &app.world().resource::<ComposerMenus>().0;
        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].0, page);
        assert_eq!(menus[0].1.menu, Some(ChatComposerMenuKind::Branch));
        assert_eq!(menus[0].1.index, 2);
        assert_eq!(menus[1].1.menu, None);
        assert_eq!(app.world().resource::<BranchRequests>().0, vec![page]);
    }

    #[test]
    fn composer_selection_dispatches_to_its_page_and_restores_the_draft() {
        let mut app = Echo::app();
        app.init_resource::<WorkspaceRequests>()
            .add_observer(WorkspaceRequests::record);
        let page = app
            .world_mut()
            .spawn((
                SessionView,
                ChatComposerContext(crate::event::ComposerContext::default()),
            ))
            .id();
        app.world_mut()
            .get_mut::<ComposerState>(page)
            .unwrap()
            .update("keep this draft");
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: ChatComposerMenuRequest {
                menu: Some(ChatComposerMenuKind::Project),
                index: 0,
            },
        });
        app.world_mut().flush();
        app.world_mut().resource_mut::<ComposerEffects>().0.clear();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: ChatListChooseRequest { index: 0 },
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<WorkspaceRequests>().0, vec![page]);
        let menus = &app.world().resource::<ComposerMenus>().0;
        assert_eq!(menus.last().unwrap().0, page);
        assert!(menus.last().unwrap().1.menu.is_none());
        let effects = &app.world().resource::<ComposerEffects>().0;
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].0, page);
        assert_eq!(effects[0].1.draft, "keep this draft");
        assert!(effects[0].1.focus);
    }
}
