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
use vmux_api::mcp::{McpServerEntry, McpServerRequest, McpServersUiState};
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
    kind: Option<ChatListKind>,
    composer_menu: Option<ChatComposerMenuKind>,
    identity: String,
    index: usize,
}

#[derive(Component, Default)]
pub(super) struct ChatSelectorProjection(pub ChatSelectorState);

impl ChatListSelection {
    fn matches(&self, list: &ActiveChatList) -> bool {
        self.kind == Some(list.kind)
            && self.composer_menu == list.composer_menu
            && self.identity == list.identity
    }

    fn current(&mut self, list: &ActiveChatList) -> &mut usize {
        if !self.matches(list) {
            self.kind = Some(list.kind);
            self.composer_menu = list.composer_menu;
            self.identity.clone_from(&list.identity);
            self.index = list.initial;
        }
        self.index = self.index.min(list.len.saturating_sub(1));
        &mut self.index
    }

    fn update(&mut self, list: &ActiveChatList, index: usize) {
        self.kind = Some(list.kind);
        self.composer_menu = list.composer_menu;
        self.identity.clone_from(&list.identity);
        self.index = index.min(list.len.saturating_sub(1));
    }

    fn open_composer_menu(&mut self, kind: ChatComposerMenuKind, index: usize) {
        self.kind = Some(ChatListKind::Composer);
        self.composer_menu = Some(kind);
        self.identity.clear();
        self.index = index;
    }

    fn finish_composer_menu(
        &mut self,
        caller: Entity,
        composer: &mut ComposerState,
        commands: &mut Commands,
    ) {
        self.close_composer_menu();
        commands
            .entity(caller)
            .insert(ActiveComposerMenu::default());
        commands.trigger(
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
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
        );
    }

    fn close_composer_menu(&mut self) {
        if self.kind == Some(ChatListKind::Composer) {
            self.kind = None;
            self.composer_menu = None;
            self.identity.clear();
            self.index = 0;
        }
    }

    fn project(&mut self, list: &ActiveChatList) -> Option<ChatListSelectionState> {
        let same_list = self.matches(list);
        let previous_index = self.index;
        let index = *self.current(list);
        if same_list && previous_index == index {
            return None;
        }
        Some(ChatListSelectionState {
            kind: list.kind,
            index: index as u32,
        })
    }
}

struct ActiveChatList {
    kind: ChatListKind,
    composer_menu: Option<ChatComposerMenuKind>,
    identity: String,
    len: usize,
    initial: usize,
}

impl ActiveChatList {
    fn key(primary: &str, items: impl IntoIterator<Item = impl AsRef<str>>) -> String {
        let mut out = format!("{}:{primary}", primary.len());
        for item in items {
            let s = item.as_ref();
            out.push_str(&format!("|{}:{s}", s.len()));
        }
        out
    }

    fn composer_menu(&self, index: usize) -> Option<ActiveComposerMenu> {
        let kind = self.composer_menu?;
        Some(ActiveComposerMenu {
            menu: Some(kind),
            index,
        })
    }
}

#[derive(Component, Clone, Copy, Default)]
pub(super) struct ActiveComposerMenu {
    menu: Option<ChatComposerMenuKind>,
    index: usize,
}

#[vmux_command::command]
struct ChatListNextBinding;

#[vmux_command::command]
struct ChatListPreviousBinding;

#[vmux_command::command]
struct ChatListChooseBinding;

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
                composer_menu: None,
                identity: approval.call_id.clone(),
                len: 3,
                initial: 0,
            });
        }
        if let Ok(choice) = self.choices.get(webview) {
            return Some(ActiveChatList {
                kind: ChatListKind::Choice,
                composer_menu: None,
                identity: ActiveChatList::key(&choice.question, &choice.options),
                len: choice.options.len(),
                initial: 0,
            });
        }
        if let Ok(menu) = self.menus.get(webview)
            && let Some(kind) = menu.menu
        {
            return Some(ActiveChatList {
                kind: ChatListKind::Composer,
                composer_menu: Some(kind),
                identity: String::new(),
                len: self.composer_rows(webview, kind),
                initial: menu.index,
            });
        }
        if let Some(query) = InlineMediaQuery::parse(draft) {
            let projection = self.media.get(webview).ok();
            let request_id = projection
                .map(|projection| projection.0.request_id)
                .unwrap_or_default();
            return Some(ActiveChatList {
                kind: ChatListKind::Media,
                composer_menu: None,
                identity: format!("{request_id}:{}", query.query),
                len: projection
                    .map(|projection| projection.0.entries.len())
                    .unwrap_or_default(),
                initial: 0,
            });
        }
        self.active_selector(webview, draft)
    }

    fn active_selector(&self, webview: Entity, draft: &str) -> Option<ActiveChatList> {
        match SelectorMode::from_draft(draft) {
            SelectorMode::Mcp(query) => {
                let entries = self.mcp_entries(webview, query);
                Some(ActiveChatList {
                    kind: ChatListKind::Mcp,
                    composer_menu: None,
                    identity: ActiveChatList::key(
                        query,
                        entries.iter().map(|entry| entry.id.as_str()),
                    ),
                    len: entries.len(),
                    initial: 0,
                })
            }
            SelectorMode::Resume(query) => {
                let projection = self.resumes.get(webview).ok();
                let request_id = projection
                    .map(|projection| projection.0.request_id)
                    .unwrap_or_default();
                Some(ActiveChatList {
                    kind: ChatListKind::Session,
                    composer_menu: None,
                    identity: format!("{request_id}:{query}"),
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
                    composer_menu: None,
                    identity: ActiveChatList::key(
                        query,
                        entries.iter().map(|entry| entry.id.as_str()),
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
                    composer_menu: None,
                    identity: ActiveChatList::key(
                        query,
                        entries.iter().map(|entry| entry.command.name()),
                    ),
                    len: entries.len(),
                    initial: 0,
                })
            }
            SelectorMode::None => None,
        }
    }

    fn choose_composer_menu(
        &self,
        caller: Entity,
        selected: usize,
        commands: &mut Commands,
    ) -> bool {
        let Ok(menu) = self.menus.get(caller) else {
            return false;
        };
        let Some(kind) = menu.menu else {
            return false;
        };
        match kind {
            ChatComposerMenuKind::Effort => {
                let Ok(model) = self.models.get(caller) else {
                    return false;
                };
                let level = if selected == 0 {
                    String::new()
                } else {
                    let Some(level) = model.0.effort_levels.get(selected - 1) else {
                        return false;
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
                let Ok(mode) = self.modes.get(caller) else {
                    return false;
                };
                let Some(mode) = mode.0.modes.get(selected) else {
                    return false;
                };
                commands.trigger(UiInput {
                    webview: caller,
                    payload: SelectMode {
                        mode_id: mode.id.clone(),
                    },
                });
            }
            ChatComposerMenuKind::Project => {
                let Ok(context) = self.contexts.get(caller) else {
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
            }
            ChatComposerMenuKind::Branch => {
                let Ok(branches) = self.branches.get(caller) else {
                    return false;
                };
                let Some(branch) = branches.0.branches.get(selected) else {
                    return false;
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
        true
    }

    fn choose_selector(
        &self,
        caller: Entity,
        kind: ChatListKind,
        draft: &str,
        selected: usize,
        commands: &mut Commands,
    ) -> Option<Option<String>> {
        match kind {
            ChatListKind::Media => {
                let media = self.media.get(caller).ok()?;
                let entry = media.0.entries.get(selected)?;
                let query = InlineMediaQuery::parse(draft)?;
                let reference = entry.reference();
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
                Some(Some(query.replace(draft, &replacement)))
            }
            ChatListKind::Mcp => {
                let state = self.mcp.get(caller).ok()?.current()?;
                if state.pending.is_some() {
                    return None;
                }
                let SelectorMode::Mcp(query) = SelectorMode::from_draft(draft) else {
                    return None;
                };
                let entries = self.mcp_entries(caller, query);
                let server = entries.get(selected)?;
                commands.trigger(UiInput {
                    webview: caller,
                    payload: McpServerRequest {
                        id: server.id.clone(),
                    },
                });
                Some(None)
            }
            ChatListKind::Session => {
                let sessions = self.resumes.get(caller).ok()?;
                let session = sessions.0.sessions.get(selected)?;
                commands.trigger(UiInput {
                    webview: caller,
                    payload: ResumeSession {
                        kind: session.kind.clone(),
                        sid: session.sid.clone(),
                        cwd: session.cwd.clone(),
                        open: None,
                    },
                });
                Some(Some(String::new()))
            }
            ChatListKind::Model => {
                let models = self.models.get(caller).ok()?;
                let SelectorMode::Models(query) = SelectorMode::from_draft(draft) else {
                    return None;
                };
                let models = models.filtered(query);
                let model = models.get(selected)?;
                commands.trigger(UiInput {
                    webview: caller,
                    payload: SelectModel {
                        model_id: model.id.clone(),
                    },
                });
                Some(Some(String::new()))
            }
            ChatListKind::Command => {
                let command = self.commands.get(caller).ok()?;
                let SelectorMode::Commands(query) = SelectorMode::from_draft(draft) else {
                    return None;
                };
                let commands_list = command.filtered(query);
                let command = commands_list.get(selected)?;
                commands.trigger(UiInput {
                    webview: caller,
                    payload: ChatSlashCommandRequest {
                        command: command.command,
                    },
                });
                Some(None)
            }
            _ => None,
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
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(
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
            if !lists.choose_composer_menu(caller, selected, &mut commands) {
                return;
            }
            close_menu = true;
        }
        ChatListKind::Media
        | ChatListKind::Mcp
        | ChatListKind::Session
        | ChatListKind::Model
        | ChatListKind::Command => {
            let Some(next) =
                lists.choose_selector(caller, kind, composer.draft(), selected, &mut commands)
            else {
                return;
            };
            change_composer = next;
        }
    }
    if close_menu {
        selection.finish_composer_menu(caller, &mut composer, &mut commands);
    }
    if let Some(draft) = change_composer {
        let effect = composer.effect(draft, true);
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::ChatUiState>::from_event(caller, &effect),
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
            Some(kind) => selection.open_composer_menu(kind, menu.index),
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
}
