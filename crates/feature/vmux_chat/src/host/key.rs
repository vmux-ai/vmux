use super::composer::{ComposerChanged, ComposerState};
use super::model::{ModeProjection, ModelPickerProjection, SlashCommandProjection};
use super::session::{
    ChatBranchesProjection, ChatComposerContext, ChatMediaProjection, ChatResumeProjection,
    ChatSnapshotProjection, ChatTranscriptProjection, ChatView, PendingAgentChoice,
};
use crate::event::{
    ApprovalDecision, ChatApproval, ChatAttachPaths, ChatCancel, ChatChoiceSelected,
    ChatComposerMenuChanged, ChatComposerMenuKind, ChatComposerMenuState, ChatEscape,
    ChatGoToBranch, ChatListChooseRequest, ChatListKind, ChatListSelectionChanged,
    ChatListSelectionState, ChatSelectWorkspace, ChatSelectorState, ChatSlashCommandRequest,
    ChatSubmit, ResumeSession, SelectMode, SelectModel, SetAgentEffort,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_cef::prelude::UiInput;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::mcp::{McpServerEntry, McpServerRequest, McpServers};
use vmux_api::prompt_media::{inline_media_query, replace_inline_media_query};
use vmux_command::{
    BindCommands, CommandBinding, CommandDispatch, CommandRegistry, CommandRuntimePlugin,
};
use vmux_core::host::UiState;
#[cfg(test)]
use vmux_core::host::manifest::FeaturePlugin;
use vmux_core::prompt_media::MediaPath;
use vmux_ui::hooks::{MenuDirection, move_selection};
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
            ChatComposerMenuChanged,
        )>::default())
            .add_systems(Startup, bind_commands.in_set(BindCommands))
            .add_systems(Update, project_selector)
            .add_observer(move_list)
            .add_observer(choose_shortcut)
            .add_observer(choose_from_ui)
            .add_observer(choose)
            .add_observer(choose_number)
            .add_observer(select_list)
            .add_observer(update_composer_menu)
            .add_observer(move_history)
            .add_observer(submit)
            .add_observer(dismiss_selector)
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
}

#[derive(Clone, PartialEq, Eq)]
enum ChatListIdentity {
    Approval(String),
    Choice(String, Vec<String>),
    Composer(ChatComposerMenuKind),
    Media(u64, String),
    Mcp(String),
    Session(u64, String),
    Model(String),
    Command(String),
}

struct ActiveChatList {
    kind: ChatListKind,
    identity: ChatListIdentity,
    len: usize,
    initial: usize,
}

#[derive(Component, Default)]
pub(super) struct ActiveComposerMenu {
    menu: Option<ChatComposerMenuKind>,
    index: usize,
}

#[vmux_command::command(id = "chat_list_next")]
struct ListNextBinding;

#[vmux_command::command(id = "chat_list_previous")]
struct ListPreviousBinding;

#[vmux_command::command(id = "chat_list_choose")]
struct ListChooseBinding;

#[derive(EntityEvent)]
struct ChooseList {
    #[event_target]
    target: Entity,
    index: Option<usize>,
}

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

#[vmux_command::command(id = "chat_history_older")]
struct HistoryOlderBinding;

#[vmux_command::command(id = "chat_history_newer")]
struct HistoryNewerBinding;

#[vmux_command::command(id = "chat_submit")]
struct SubmitBinding;

#[vmux_command::command(id = "chat_dismiss_selector")]
struct DismissSelectorBinding;

#[vmux_command::command(id = "chat_interrupt")]
struct InterruptBinding;

#[vmux_command::command(id = "chat_cancel")]
struct CancelBinding;

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<ListNextBinding>(&mut commands);
    registry.bind::<ListPreviousBinding>(&mut commands);
    registry.bind::<ListChooseBinding>(&mut commands);
    registry.bind::<ChoiceNumberBinding>(&mut commands);
    registry.bind::<HistoryOlderBinding>(&mut commands);
    registry.bind::<HistoryNewerBinding>(&mut commands);
    registry.bind::<SubmitBinding>(&mut commands);
    registry.bind::<DismissSelectorBinding>(&mut commands);
    registry.bind::<InterruptBinding>(&mut commands);
    registry.bind::<CancelBinding>(&mut commands);
}

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
    mcp: Query<'w, 's, &'static UiState<McpServers>>,
}

impl ChatLists<'_, '_> {
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
        if let Some(query) = inline_media_query(draft) {
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
            SelectorMode::Mcp(query) => Some(ActiveChatList {
                kind: ChatListKind::Mcp,
                identity: ChatListIdentity::Mcp(query.to_string()),
                len: self.mcp_entries(webview, query).len(),
                initial: 0,
            }),
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
            SelectorMode::Models(query) => Some(ActiveChatList {
                kind: ChatListKind::Model,
                identity: ChatListIdentity::Model(query.to_string()),
                len: self
                    .models
                    .get(webview)
                    .map(|projection| projection.filtered(query).len())
                    .unwrap_or_default(),
                initial: 0,
            }),
            SelectorMode::Commands(query) => {
                let len = self
                    .commands
                    .get(webview)
                    .map(|projection| projection.filtered(query).len())
                    .unwrap_or_default();
                if len == 0 {
                    return None;
                }
                Some(ActiveChatList {
                    kind: ChatListKind::Command,
                    identity: ChatListIdentity::Command(query.to_string()),
                    len,
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
    mut projections: Query<(Entity, &mut ChatSelectorProjection), With<ChatView>>,
    mut commands: Commands,
) {
    for (webview, mut projection) in &mut projections {
        let Ok(composer) = composers.get(webview) else {
            continue;
        };
        let state = lists.selector(webview, composer.draft());
        if projection.0 == state {
            continue;
        }
        projection.0 = state.clone();
        commands.trigger(
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(webview, &state),
        );
    }
}

fn move_list(
    trigger: On<CommandDispatch>,
    next: Query<(), With<ListNextBinding>>,
    previous: Query<(), With<ListPreviousBinding>>,
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
    let direction = match next {
        true => MenuDirection::Next,
        false => MenuDirection::Previous,
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
    *selected = move_selection(*selected, len, direction);
    commands.trigger(
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            caller,
            &ChatListSelectionState {
                kind,
                index: *selected as u32,
            },
        ),
    );
}

fn choose_shortcut(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<ListChooseBinding>>,
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
    mut composers: Query<&mut ComposerState>,
    mut selections: Query<&mut ChatListSelection>,
    mut commands: Commands,
) {
    let caller = trigger.event_target();
    let Ok(mut composer) = composers.get_mut(caller) else {
        return;
    };
    let Some(list) = lists.active(caller, composer.draft()) else {
        return;
    };
    let Ok(mut selection) = selections.get_mut(caller) else {
        return;
    };
    let kind = list.kind;
    let selected = if let Some(index) = trigger.event().index {
        selection.update(&list, index);
        commands.trigger(
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                caller,
                &ChatListSelectionState {
                    kind,
                    index: selection.index as u32,
                },
            ),
        );
        selection.index
    } else {
        *selection.current(&list)
    };
    let mut close_menu = false;
    let mut change_composer = None;
    match kind {
        ChatListKind::Approval => {
            let Ok(snapshot) = lists.snapshots.get(caller) else {
                return;
            };
            let Some(approval) = &snapshot.0.approval else {
                return;
            };
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
        }
        ChatListKind::Choice => {
            commands.trigger(UiInput {
                webview: caller,
                payload: ChatChoiceSelected {
                    index: selected as u32,
                },
            });
        }
        ChatListKind::Composer => {
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
                    commands.trigger(UiInput {
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
                    commands.trigger(UiInput {
                        webview: caller,
                        payload: SelectMode {
                            mode_id: mode.id.clone(),
                        },
                    });
                }
                ChatComposerMenuKind::Project => {
                    let Ok(context) = lists.contexts.get(caller) else {
                        return;
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
                            return;
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
                }
                ChatComposerMenuKind::Branch => {
                    let Ok(branches) = lists.branches.get(caller) else {
                        return;
                    };
                    let Some(branch) = branches.0.branches.get(selected) else {
                        return;
                    };
                    commands.trigger(UiInput {
                        webview: caller,
                        payload: ChatGoToBranch {
                            project: branches.0.project.clone(),
                            branch: branch.branch.clone(),
                            checkout: branch.checkout.clone(),
                        },
                    });
                }
            }
            close_menu = true;
        }
        ChatListKind::Media => {
            let Ok(media) = lists.media.get(caller) else {
                return;
            };
            let Some(entry) = media.0.entries.get(selected) else {
                return;
            };
            let Some(query) = inline_media_query(composer.draft()) else {
                return;
            };
            let reference = MediaPath::new(entry).reference();
            let replacement = if entry.is_dir {
                format!("@{reference}/")
            } else {
                commands.trigger(UiInput {
                    webview: caller,
                    payload: ChatAttachPaths {
                        paths: vec![entry.path.clone()],
                    },
                });
                String::new()
            };
            change_composer = Some(replace_inline_media_query(
                composer.draft(),
                query,
                &replacement,
            ));
        }
        ChatListKind::Mcp => {
            let Ok(state) = lists.mcp.get(caller) else {
                return;
            };
            let Some(state) = state.current() else {
                return;
            };
            if state.pending.is_some() {
                return;
            }
            let SelectorMode::Mcp(query) = SelectorMode::from_draft(composer.draft()) else {
                return;
            };
            let entries = lists.mcp_entries(caller, query);
            let Some(server) = entries.get(selected) else {
                return;
            };
            commands.trigger(UiInput {
                webview: caller,
                payload: McpServerRequest {
                    id: server.id.clone(),
                },
            });
        }
        ChatListKind::Session => {
            let Ok(sessions) = lists.resumes.get(caller) else {
                return;
            };
            let Some(session) = sessions.0.sessions.get(selected) else {
                return;
            };
            commands.trigger(UiInput {
                webview: caller,
                payload: ResumeSession {
                    kind: session.kind.clone(),
                    sid: session.sid.clone(),
                    cwd: session.cwd.clone(),
                },
            });
            change_composer = Some(String::new());
        }
        ChatListKind::Model => {
            let Ok(models) = lists.models.get(caller) else {
                return;
            };
            let SelectorMode::Models(query) = SelectorMode::from_draft(composer.draft()) else {
                return;
            };
            let models = models.filtered(query);
            let Some(model) = models.get(selected) else {
                return;
            };
            commands.trigger(UiInput {
                webview: caller,
                payload: SelectModel {
                    model_id: model.id.clone(),
                },
            });
            change_composer = Some(String::new());
        }
        ChatListKind::Command => {
            let Ok(command) = lists.commands.get(caller) else {
                return;
            };
            let SelectorMode::Commands(query) = SelectorMode::from_draft(composer.draft()) else {
                return;
            };
            let commands_list = command.filtered(query);
            let Some(command) = commands_list.get(selected) else {
                return;
            };
            commands.trigger(UiInput {
                webview: caller,
                payload: ChatSlashCommandRequest {
                    command: command.command,
                },
            });
        }
    }
    if close_menu {
        selection.close_composer_menu();
        commands
            .entity(caller)
            .insert(ActiveComposerMenu::default());
        commands.trigger(
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                caller,
                &ChatComposerMenuState {
                    menu: None,
                    index: 0,
                },
            ),
        );
        let draft = composer.draft().to_string();
        let effect = composer.effect(draft, true);
        commands.trigger(
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
        );
    }
    if let Some(draft) = change_composer {
        let effect = composer.effect(draft, true);
        commands.trigger(
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
        );
        commands.trigger(ComposerChanged::new(caller));
    }
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
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            webview,
            &ChatListSelectionState {
                kind: list.kind,
                index: selection.index as u32,
            },
        ),
    );
}

fn update_composer_menu(
    trigger: On<UiInput<ChatComposerMenuChanged>>,
    mut menus: Query<&mut ActiveComposerMenu>,
    mut selections: Query<&mut ChatListSelection>,
) {
    let webview = trigger.event().webview;
    let Ok(mut menu) = menus.get_mut(webview) else {
        return;
    };
    menu.menu = trigger.event().payload.menu;
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
        vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
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
    menus: Query<&ActiveComposerMenu>,
    mut composers: Query<&mut ComposerState, With<ChatView>>,
    mut selections: Query<&mut ChatListSelection>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let caller = trigger.event().invocation().caller;
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
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
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
                vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                    caller, &effect,
                ),
            );
        }
        return;
    }
    if let Ok(mut composer) = composers.get_mut(caller)
        && let Some(effect) = composer.dismiss_selector()
    {
        commands.trigger(
            vmux_core::host::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
        );
        commands.trigger(ComposerChanged::new(caller));
    }
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
    use crate::event::{ModelOptionEntry, ModelState};
    use crate::state::ChatUiState;
    use bevy::MinimalPlugins;
    use vmux_command::CommandInvocation;
    use vmux_core::host::UiStateWrite;

    #[derive(Resource, Default)]
    struct ListSelections(Vec<(Entity, ChatListKind, u32)>);

    impl ListSelections {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut selections: ResMut<Self>) {
            let Some(state) = trigger.event().patch().list_selection else {
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
            .add_observer(ListSelections::record)
            .add_observer(ChoiceNumbers::record);
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
                ChatView,
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
}
