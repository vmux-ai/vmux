//! The desktop half of the chat page: owning the session's ECS state and moving it between the
//! daemon and the webview.
//!
//! Gated as a whole rather than item by item — a hundred attributes down one file said nothing
//! that one on the module does not. The rendered counterpart is the sibling `page`.

mod media;
mod model;
mod resume;

use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

use self::model::{effort_current_for, emit_model_state};
use crate::client::acp::{AcpModelState, AcpSession};
use crate::components::{
    AgentApprovalPolicy, AgentConversationTitle, AgentMessages, AgentSession, PromptQueue,
    provisional_conversation_title,
};
use crate::event::chat::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_EVENT, CHAT_INITIAL_ITEM_LIMIT,
    CHAT_SNAPSHOT_EVENT, COMPOSER_CONTEXT_EVENT, ChatApproval, ChatCancel, ChatCancelQueuedPrompt,
    ChatChoiceSelected, ChatClearQueue, ChatCreateWorktree, ChatEscape, ChatHistoryPage,
    ChatHistoryRequest, ChatOpenPage, ChatResume, ChatSelectWorkspace, ChatSnapshot, ChatSubmit,
    ComposerContext, QueuedPromptSnapshot,
};
use crate::events::{
    AgentApprovalReply, AgentChoiceSelected, AgentCommandRequest, ApprovalDecision, CommandOrigin,
};
use crate::handoff::ImportedConversation;
use crate::run_state::{AgentRunState, AgentTurnMeta};
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_core::PageMetadata;
use vmux_core::team::Profile;
use vmux_service::chat::{group_turns_before, group_turns_tail, grouped_item_count};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{
    AgentAttachment, AgentCommand as ServiceAgentCommand, AgentRequestId, ClientMessage,
    SharedMessage,
};

pub struct AgentChatPagePlugin;

impl Plugin for AgentChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(
            ChatSubmit,
            ChatApproval,
            ChatCancel,
            ChatResume,
            ChatClearQueue,
            ChatCancelQueuedPrompt,
            ChatEscape,
        )>::for_hosts(&["agent", "start"]))
            .add_plugins(BinEventEmitterPlugin::<(
                ChatChoiceSelected,
                ChatHistoryRequest,
                ChatSelectWorkspace,
                ChatCreateWorktree,
                ChatOpenPage,
            )>::for_hosts(&["agent", "start"]))
            .add_plugins((
                media::ChatMediaPlugin,
                model::ChatModelPlugin,
                resume::ChatResumePlugin,
            ))
            .add_observer(on_chat_submit)
            .add_observer(on_chat_approval)
            .add_observer(on_chat_cancel)
            .add_observer(on_chat_resume)
            .add_observer(on_chat_clear_queue)
            .add_observer(on_chat_cancel_queued_prompt)
            .add_observer(on_chat_escape)
            .add_observer(on_chat_choice_selected)
            .add_observer(on_chat_history_request)
            .add_observer(on_chat_open_page)
            .add_observer(on_chat_select_workspace)
            .add_observer(on_chat_create_worktree)
            .add_observer(reset_chat_synced_on_page_ready)
            .add_systems(
                Update,
                (
                    (track_turn_duration, push_chat_to_page).chain(),
                    sync_chat_to_ready_views,
                    push_composer_context_to_page,
                ),
            );
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

/// Record per-turn wall-clock from `AgentRunState` edges (covers page + ACP mutation sites
/// uniformly). Idempotent: the `turn_start` guard tolerates repeated same-state sets and does
/// not reset across a mid-turn `AwaitingApproval`.
fn track_turn_duration(
    time: Res<Time>,
    mut sessions: Query<(&AgentRunState, &mut AgentTurnMeta), Changed<AgentRunState>>,
) {
    for (state, mut meta) in &mut sessions {
        match state {
            AgentRunState::Streaming => {
                if meta.turn_start.is_none() {
                    meta.turn_start = Some(time.elapsed());
                }
            }
            AgentRunState::Idle | AgentRunState::Errored(_) => {
                if let Some(start) = meta.turn_start.take() {
                    meta.durations
                        .push(time.elapsed().saturating_sub(start).as_secs() as u32);
                }
            }
            AgentRunState::AwaitingApproval { .. } | AgentRunState::Installing { .. } => {}
        }
    }
}

/// Marks a chat-page webview (ACP or Page agent) so the ready→resync path can find it cheaply.
#[derive(Component)]
pub struct AgentChatView;

/// Set once the current snapshot has been pushed to a ready chat webview; cleared when the page
/// (re)signals ready (mount or Cmd+R reload) so the transcript is re-pushed instead of blanking.
#[derive(Component)]
pub(crate) struct ChatSynced;

fn on_chat_choice_selected(trigger: On<BinReceive<ChatChoiceSelected>>, mut commands: Commands) {
    commands.trigger(AgentChoiceSelected {
        webview: trigger.event().webview,
        index: trigger.event().payload.index as usize,
    });
}

/// Push the current transcript + slash commands to any chat webview that is ready but not yet
/// synced. Runs every frame and retries until the webview's emit channel is ready, so the very
/// first snapshot always lands. Re-runs after a reload because [`reset_chat_synced_on_page_ready`]
/// clears `ChatSynced` when the page re-signals ready — without this, Cmd+R blanked the chat
/// (the `Changed`/`Added` pushes never re-fire for an unchanged, already-added session).
fn sync_chat_to_ready_views(
    pending: Query<
        Entity,
        (
            With<AgentChatView>,
            With<vmux_core::page::PageReady>,
            Without<ChatSynced>,
        ),
    >,
    child_of: Query<&ChildOf>,
    sessions: Query<(
        &AgentMessages,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&Profile>,
        Option<&PageMetadata>,
        &PromptQueue,
        Option<&ImportedConversation>,
        Option<&AgentConversationTitle>,
    )>,
    acp_sessions: Query<(&AcpSession, Option<&AcpModelState>)>,
    choices: Query<&crate::plugin::PendingAgentChoice>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for webview in &pending {
        let Ok(parent) = child_of.get(webview) else {
            continue;
        };
        let stack = parent.parent();
        let Ok((messages, state, turn_meta, profile, meta, queue, imported, title)) =
            sessions.get(stack)
        else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(
                messages,
                state,
                turn_meta,
                profile,
                meta,
                queue,
                imported,
                title,
                choices.get(webview).ok(),
            ),
        ));
        let (cross, model_state, agent_key) = acp_sessions
            .get(stack)
            .ok()
            .map(|(acp, model)| {
                (
                    acp_agent_kind(&acp.agent_id)
                        .map(kind_supports_cross_runtime)
                        .unwrap_or(false),
                    model,
                    acp.agent_id.clone(),
                )
            })
            .unwrap_or((false, None, String::new()));
        emit_model_state(
            webview,
            model_state,
            cross,
            &agent_key,
            effort_current_for(settings.as_ref(), &agent_key),
            &mut commands,
        );
        commands.entity(webview).insert(ChatSynced);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposerContextInput {
    cwd: std::path::PathBuf,
    workspace_selected: bool,
    worktree: Option<vmux_layout::tab::TabWorktree>,
    can_manage_workspace: bool,
    auto_allow_count: u32,
}

#[derive(Default)]
struct ComposerContextCache {
    entries: std::collections::HashMap<Entity, ComposerContextCacheEntry>,
}

struct ComposerContextCacheEntry {
    input: ComposerContextInput,
    context: ComposerContext,
}

fn composer_context_input(
    stack: Entity,
    acp: Option<&AcpSession>,
    policy: Option<&AgentApprovalPolicy>,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(
        &vmux_layout::tab::Tab,
        Option<&vmux_layout::tab::TabWorkspace>,
        Option<&vmux_layout::tab::TabWorktree>,
    )>,
) -> ComposerContextInput {
    let mut current = stack;
    let mut tab_dir = None;
    let mut workspace_selected = false;
    let mut worktree = None;
    loop {
        if let Ok((tab, workspace, managed)) = tabs.get(current) {
            tab_dir = tab.startup_dir.as_ref().map(std::path::PathBuf::from);
            workspace_selected = workspace.is_some() || tab.startup_dir.is_some();
            worktree = managed.cloned();
            break;
        }
        let Ok(parent) = child_of.get(current) else {
            break;
        };
        current = parent.parent();
    }
    ComposerContextInput {
        cwd: tab_dir
            .or_else(|| acp.map(|session| session.cwd.clone()))
            .unwrap_or_default(),
        workspace_selected,
        worktree,
        can_manage_workspace: acp.is_some(),
        auto_allow_count: policy
            .map(|policy| u32::try_from(policy.auto.len()).unwrap_or(u32::MAX))
            .unwrap_or_default(),
    }
}

fn composer_context_from_input(
    input: &ComposerContextInput,
    info: Option<&vmux_git::worktree::RepoInfo>,
) -> ComposerContext {
    let is_git_repo = info.is_some() || input.worktree.is_some() || input.cwd.join(".git").exists();
    let branch = info
        .map(|info| info.branch.clone())
        .filter(|branch| !branch.is_empty())
        .or_else(|| {
            input
                .worktree
                .as_ref()
                .map(|worktree| worktree.branch.clone())
        })
        .unwrap_or_default();
    let workspace_name = input
        .cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| input.cwd.to_string_lossy().into_owned());
    ComposerContext {
        cwd: input.cwd.to_string_lossy().into_owned(),
        workspace_name,
        workspace_selected: input.workspace_selected,
        is_git_repo,
        is_worktree: info.is_some_and(|info| info.is_worktree) || input.worktree.is_some(),
        branch,
        base_ref: input
            .worktree
            .as_ref()
            .map(|worktree| worktree.base_ref.clone())
            .unwrap_or_default(),
        uncommitted: info.map(|info| info.uncommitted).unwrap_or_default(),
        ahead: info.map(|info| info.ahead).unwrap_or_default(),
        can_manage_workspace: input.can_manage_workspace,
        auto_allow_count: input.auto_allow_count,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_composer_context_to_page(
    views: Query<(Entity, &ChildOf, Ref<vmux_core::page::PageReady>), With<AgentChatView>>,
    sessions: Query<(Option<&AcpSession>, Option<&AgentApprovalPolicy>)>,
    child_of: Query<&ChildOf>,
    tabs: Query<(
        &vmux_layout::tab::Tab,
        Option<&vmux_layout::tab::TabWorkspace>,
        Option<&vmux_layout::tab::TabWorktree>,
    )>,
    browsers: NonSend<Browsers>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
    mut cache: Local<ComposerContextCache>,
    mut commands: Commands,
) {
    let live_views = views
        .iter()
        .map(|(webview, _, _)| webview)
        .collect::<std::collections::HashSet<_>>();
    cache
        .entries
        .retain(|webview, _| live_views.contains(webview));
    for (webview, parent, ready) in &views {
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        let stack = parent.parent();
        let Ok((acp, policy)) = sessions.get(stack) else {
            continue;
        };
        let input = composer_context_input(stack, acp, policy, &child_of, &tabs);
        let info = (!input.cwd.as_os_str().is_empty())
            .then(|| {
                repo_info
                    .as_mut()
                    .and_then(|cache| cache.bypass_change_detection().get(&input.cwd))
            })
            .flatten();
        let context = composer_context_from_input(&input, info.as_ref());
        let changed = cache
            .entries
            .get(&webview)
            .is_none_or(|entry| entry.input != input || entry.context != context);
        if changed || ready.is_changed() {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                webview,
                COMPOSER_CONTEXT_EVENT,
                &context,
            ));
        }
        cache
            .entries
            .insert(webview, ComposerContextCacheEntry { input, context });
    }
}

/// A chat webview re-signals `PageReady` on every (re)mount, including a Cmd+R reload. Clear its
/// `ChatSynced` marker so [`sync_chat_to_ready_views`] re-pushes the full transcript.
fn reset_chat_synced_on_page_ready(
    trigger: On<BinReceive<vmux_core::page::PageReady>>,
    chat_views: Query<(), With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if chat_views.get(webview).is_ok() {
        commands.entity(webview).remove::<ChatSynced>();
    }
}

fn snapshot_of(
    messages: &AgentMessages,
    state: &AgentRunState,
    turn_meta: Option<&AgentTurnMeta>,
    profile: Option<&Profile>,
    meta: Option<&PageMetadata>,
    queue: &PromptQueue,
    imported: Option<&ImportedConversation>,
    conversation_title: Option<&AgentConversationTitle>,
    choice: Option<&crate::plugin::PendingAgentChoice>,
) -> ChatSnapshot {
    let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
    let running = matches!(state, AgentRunState::Streaming);
    let imported_messages = imported
        .map(|conversation| conversation.messages.as_slice())
        .unwrap_or_default();
    let page = group_turns_tail(
        imported_messages,
        &messages.0,
        durations,
        running,
        CHAT_INITIAL_ITEM_LIMIT as usize,
    );
    let messages_json = serde_json::to_string(&page.items).unwrap_or_else(|_| "[]".to_string());
    let (status, error) = match state {
        AgentRunState::Idle => ("idle", String::new()),
        AgentRunState::Installing { pct, message } => {
            let text = match pct {
                Some(p) => format!("{message} ({p}%)"),
                None => message.clone(),
            };
            ("installing", text)
        }
        AgentRunState::Streaming => ("streaming", String::new()),
        AgentRunState::AwaitingApproval { .. } => ("awaiting", String::new()),
        AgentRunState::Errored(message) => ("errored", message.clone()),
    };
    let (call_id, name, args_json) = match state {
        AgentRunState::AwaitingApproval {
            call_id,
            name,
            args,
        } => (call_id.clone(), name.clone(), args.to_string()),
        _ => (String::new(), String::new(), String::new()),
    };
    let (agent_name, accent_color) = profile
        .map(|p| (p.name.clone(), p.avatar.color.clone()))
        .unwrap_or_default();
    let agent_icon = meta
        .map(|m| m.icon.favicon_url().to_string())
        .unwrap_or_default();
    ChatSnapshot {
        messages_json,
        messages_start: u32::try_from(page.start).unwrap_or(u32::MAX),
        messages_total: u32::try_from(page.total).unwrap_or(u32::MAX),
        status: status.to_string(),
        error,
        approval_call_id: call_id,
        approval_name: name,
        approval_args_json: args_json,
        agent_name,
        conversation_title: conversation_title
            .map(|title| title.0.clone())
            .unwrap_or_default(),
        agent_icon,
        accent_color,
        handoff_source: imported
            .map(|imported| imported.source_agent.clone())
            .unwrap_or_default(),
        handoff_truncated: imported.is_some_and(|imported| imported.truncated),
        handoff_message_count: imported
            .map(|imported| {
                u32::try_from(grouped_item_count(&imported.messages, &[])).unwrap_or(u32::MAX)
            })
            .unwrap_or_default(),
        choice_question: choice
            .map(|choice| choice.question.clone())
            .unwrap_or_default(),
        choice_options: choice
            .map(|choice| choice.options.clone())
            .unwrap_or_default(),
        queued: queue
            .items
            .iter()
            .map(|item| QueuedPromptSnapshot {
                id: item.id,
                text: item.text.clone(),
                attachment_names: item
                    .attachments
                    .iter()
                    .map(|attachment| attachment.name.clone())
                    .collect(),
            })
            .collect(),
        paused: queue.paused,
    }
}

const CHAT_STREAM_PUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

fn chat_snapshot_due(streaming: bool, urgent: bool, elapsed: Option<std::time::Duration>) -> bool {
    urgent || !streaming || elapsed.is_none_or(|elapsed| elapsed >= CHAT_STREAM_PUSH_INTERVAL)
}

/// Push each changed session's conversation + run-state to its pane webview (the child
/// `Browser` of the session entity).
fn push_chat_to_page(
    sessions: Query<
        (
            Entity,
            Ref<AgentMessages>,
            Ref<AgentRunState>,
            Option<Ref<AgentTurnMeta>>,
            Option<Ref<Profile>>,
            Option<&PageMetadata>,
            Ref<PromptQueue>,
            Option<Ref<ImportedConversation>>,
            Option<Ref<AgentConversationTitle>>,
        ),
        Or<(
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<AgentTurnMeta>,
            Changed<PromptQueue>,
            Changed<Profile>,
            Changed<ImportedConversation>,
            Changed<AgentConversationTitle>,
        )>,
    >,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    choices: Query<&crate::plugin::PendingAgentChoice>,
    browsers: NonSend<Browsers>,
    mut last_push: Local<std::collections::HashMap<Entity, std::time::Instant>>,
    mut removed_messages: RemovedComponents<AgentMessages>,
    mut commands: Commands,
) {
    for stack in removed_messages.read() {
        last_push.remove(&stack);
    }
    for (stack, messages, state, turn_meta, profile, meta, queue, imported, title) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&e| is_browser.contains(e)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        let urgent = state.is_changed()
            || turn_meta.as_ref().is_some_and(|meta| meta.is_changed())
            || profile.as_ref().is_some_and(|profile| profile.is_changed())
            || queue.is_changed()
            || imported
                .as_ref()
                .is_some_and(|imported| imported.is_changed())
            || title.as_ref().is_some_and(|title| title.is_changed());
        let now = std::time::Instant::now();
        let elapsed = last_push
            .get(&stack)
            .map(|last| now.saturating_duration_since(*last));
        if !chat_snapshot_due(matches!(*state, AgentRunState::Streaming), urgent, elapsed) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(
                &messages,
                &state,
                turn_meta.as_deref(),
                profile.as_deref(),
                meta,
                &queue,
                imported.as_deref(),
                title.as_deref(),
                choices.get(webview).ok(),
            ),
        ));
        last_push.insert(stack, now);
    }
}

fn on_chat_history_request(
    trigger: On<BinReceive<ChatHistoryRequest>>,
    child_of: Query<&ChildOf>,
    sessions: Query<(
        &AgentMessages,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&ImportedConversation>,
    )>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let Ok((messages, state, turn_meta, imported)) = sessions.get(parent.parent()) else {
        return;
    };
    if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
        return;
    }
    let request = &trigger.event().payload;
    if request.before == 0 || request.limit == 0 {
        return;
    }
    let imported_messages = imported
        .map(|conversation| conversation.messages.as_slice())
        .unwrap_or_default();
    let durations = turn_meta
        .map(|meta| meta.durations.as_slice())
        .unwrap_or(&[]);
    let page = group_turns_before(
        imported_messages,
        &messages.0,
        durations,
        matches!(state, AgentRunState::Streaming),
        request.before as usize,
        request.limit.clamp(1, CHAT_HISTORY_MAX_PAGE_SIZE) as usize,
    );
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        CHAT_HISTORY_PAGE_EVENT,
        &ChatHistoryPage {
            items_json: serde_json::to_string(&page.items).unwrap_or_else(|_| "[]".to_string()),
            start: u32::try_from(page.start).unwrap_or(u32::MAX),
            end: u32::try_from(page.end).unwrap_or(u32::MAX),
            total: u32::try_from(page.total).unwrap_or(u32::MAX),
        },
    ));
}

fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AgentConversationTitle>,
    )>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let text = payload.text.clone();
    let attachments = payload
        .attachments
        .iter()
        .filter(|attachment| !attachment.path.is_empty())
        .map(|attachment| AgentAttachment {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        })
        .collect::<Vec<_>>();
    if text.trim().is_empty() && attachments.is_empty() {
        return;
    }
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let session = parent.parent();
    if let Ok((mut queue, mut state, title)) = sessions.get_mut(session) {
        if title.is_none()
            && let Some(title) = provisional_conversation_title(&text)
        {
            commands
                .entity(session)
                .insert(AgentConversationTitle(title));
        }
        enqueue_prompt(&mut queue, &mut state, text, attachments);
    }
}

fn enqueue_prompt(
    queue: &mut PromptQueue,
    state: &mut AgentRunState,
    text: String,
    attachments: Vec<AgentAttachment>,
) {
    queue.enqueue_with_attachments(text, attachments);
    if matches!(state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
}

fn cancel_session(
    service: Option<&ServiceClient>,
    acp: Option<&AcpSession>,
    page: Option<&AgentSession>,
) {
    let Some(service) = service else {
        return;
    };
    let Some(sid) = acp
        .map(|session| session.sid.clone())
        .or_else(|| page.map(|session| session.sid.clone()))
    else {
        return;
    };
    service.0.send(ClientMessage::Shared(SharedMessage::agent(
        sid,
        vmux_wire::protocol::AgentAction::Cancel,
    )));
}

fn on_chat_cancel(
    trigger: On<BinReceive<ChatCancel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, Option<&AcpSession>, Option<&AgentSession>)>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.flush_pending() {
        queue.cancel_flush();
    }
    cancel_session(service.as_deref(), acp, page);
}

fn on_chat_resume(
    trigger: On<BinReceive<ChatResume>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.resume();
    }
}

fn on_chat_clear_queue(
    trigger: On<BinReceive<ChatClearQueue>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.clear();
    }
}

fn on_chat_cancel_queued_prompt(
    trigger: On<BinReceive<ChatCancelQueuedPrompt>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.remove(trigger.event().payload.id);
    }
}

fn on_chat_escape(
    trigger: On<BinReceive<ChatEscape>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AcpSession>,
        Option<&AgentSession>,
    )>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, mut state, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    let running = matches!(
        *state,
        AgentRunState::Streaming | AgentRunState::AwaitingApproval { .. }
    );
    let flush = if queue.items.is_empty() {
        if queue.flush_pending() {
            queue.cancel_flush();
        }
        false
    } else {
        queue.request_flush()
    };
    if flush && matches!(*state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
    if running {
        cancel_session(service.as_deref(), acp, page);
    }
}

fn on_chat_approval(
    trigger: On<BinReceive<ChatApproval>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let decision = match payload.decision {
        1 => ApprovalDecision::Allow,
        2 => ApprovalDecision::AllowAlways,
        _ => ApprovalDecision::Deny,
    };
    commands.trigger(AgentApprovalReply {
        session: parent.parent(),
        call_id: payload.call_id.clone(),
        decision,
    });
}

/// Open a vmux page URL in a new stack (the error card's "change version" action → `vmux://agents`).
fn on_chat_open_page(
    trigger: On<BinReceive<ChatOpenPage>>,
    mut commands: MessageWriter<vmux_command::AppCommand>,
) {
    let url = trigger.event().payload.url.clone();
    if url.is_empty() {
        return;
    }
    commands.write(vmux_command::AppCommand::Browser(
        vmux_command::BrowserCommand::Open(vmux_command::open::OpenCommand::InNewStack {
            url: Some(url),
        }),
    ));
}

fn on_chat_select_workspace(
    trigger: On<BinReceive<ChatSelectWorkspace>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    requests.write(AgentCommandRequest {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        command: ServiceAgentCommand::ChooseWorkspace {
            anchor: session.anchor,
        },
    });
}

fn on_chat_create_worktree(
    trigger: On<BinReceive<ChatCreateWorktree>>,
    child_of: Query<&ChildOf>,
    sessions: Query<&AcpSession>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    requests.write(AgentCommandRequest {
        request_id: AgentRequestId::new(),
        origin: CommandOrigin::User,
        command: ServiceAgentCommand::CreateWorktree {
            anchor: session.anchor,
        },
    });
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use vmux_core::agent::AgentKind;

    #[test]
    fn streaming_snapshots_wait_for_frame_interval() {
        assert!(!chat_snapshot_due(
            true,
            false,
            Some(CHAT_STREAM_PUSH_INTERVAL - std::time::Duration::from_millis(1)),
        ));
        assert!(chat_snapshot_due(
            true,
            false,
            Some(CHAT_STREAM_PUSH_INTERVAL),
        ));
    }

    #[test]
    fn state_changes_and_completed_turns_push_immediately() {
        assert!(chat_snapshot_due(
            true,
            true,
            Some(std::time::Duration::ZERO)
        ));
        assert!(chat_snapshot_due(
            false,
            false,
            Some(std::time::Duration::ZERO),
        ));
    }

    #[test]
    fn composer_workspace_controls_dispatch_for_current_session() {
        let mut app = App::new();
        app.add_message::<AgentCommandRequest>()
            .add_observer(on_chat_select_workspace)
            .add_observer(on_chat_create_worktree);
        let anchor = vmux_core::ProcessId::new();
        let stack = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor,
                resume: None,
            })
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatSelectWorkspace,
        });
        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatCreateWorktree,
        });

        let requests = app
            .world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 2);
        assert!(matches!(requests[0].origin, CommandOrigin::User));
        assert!(matches!(
            requests[0].command,
            ServiceAgentCommand::ChooseWorkspace { anchor: got } if got == anchor
        ));
        assert!(matches!(
            requests[1].command,
            ServiceAgentCommand::CreateWorktree { anchor: got } if got == anchor
        ));
    }

    #[test]
    fn snapshot_reports_grouped_imported_item_boundary() {
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_kind: AgentKind::Codex,
            source_sid: "codex-1".into(),
            messages: vec![
                crate::Message::user("one"),
                crate::Message::Assistant {
                    blocks: vec![crate::AssistantBlock::ToolUse {
                        call_id: "call-1".into(),
                        name: "run".into(),
                        args: "{}".into(),
                        parent_call_id: None,
                    }],
                },
                crate::Message::ToolResult {
                    call_id: "call-1".into(),
                    content: "two".into(),
                    is_error: false,
                },
            ],
            truncated: false,
            first_prompt: None,
        };
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::Idle,
            None,
            None,
            None,
            &PromptQueue::default(),
            Some(&imported),
            None,
            None,
        );

        assert_eq!(snapshot.handoff_message_count, 2);
    }

    #[test]
    fn snapshot_includes_approval_tool_and_input() {
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::AwaitingApproval {
                call_id: "call-1".into(),
                name: "vmux.run".into(),
                args: serde_json::json!({"command": "echo hi", "focus": true}),
            },
            None,
            None,
            None,
            &PromptQueue::default(),
            None,
            None,
            None,
        );

        assert_eq!(snapshot.approval_name, "vmux.run");
        assert_eq!(
            snapshot.approval_args_json,
            r#"{"command":"echo hi","focus":true}"#
        );
    }

    #[test]
    fn snapshot_includes_model_written_conversation_title() {
        let title = AgentConversationTitle("Refine generated chat summaries".into());
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::Idle,
            None,
            None,
            None,
            &PromptQueue::default(),
            None,
            Some(&title),
            None,
        );

        assert_eq!(
            snapshot.conversation_title,
            "Refine generated chat summaries"
        );
    }

    #[test]
    fn first_prompt_updates_conversation_title_immediately() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_submit);
        let session = app
            .world_mut()
            .spawn((PromptQueue::default(), AgentRunState::Idle))
            .id();
        let webview = app.world_mut().spawn(ChildOf(session)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatSubmit {
                text: "  make me a new\nJapanese restaurant website  ".into(),
                attachments: Vec::new(),
            },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle(
                "make me a new Japanese restaurant website".into()
            ))
        );
        assert_eq!(
            app.world()
                .get::<PromptQueue>(session)
                .and_then(|queue| queue.items.front())
                .map(|prompt| prompt.text.as_str()),
            Some("  make me a new\nJapanese restaurant website  ")
        );

        app.world_mut().trigger(BinReceive {
            webview,
            payload: ChatSubmit {
                text: "make it darker".into(),
                attachments: Vec::new(),
            },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle(
                "make me a new Japanese restaurant website".into()
            ))
        );
    }

    #[test]
    fn submitting_after_error_rearms_prompt_dispatch() {
        let mut queue = PromptQueue::default();
        let mut state = AgentRunState::Errored("failed".into());

        enqueue_prompt(&mut queue, &mut state, "retry".into(), Vec::new());

        assert!(matches!(state, AgentRunState::Idle));
        assert_eq!(
            queue.items.front().map(|item| item.text.as_str()),
            Some("retry")
        );
        assert!(!queue.paused);
    }

    #[test]
    fn normal_cancel_overrides_pending_flush() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_cancel);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatCancel> {
            webview,
            payload: ChatCancel,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn escape_flush_rearms_errored_queue() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Errored("failed".into())))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(matches!(
            app.world().get::<AgentRunState>(stack),
            Some(AgentRunState::Idle)
        ));
        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert!(queue.flush_pending());
        assert!(!queue.paused);
    }

    #[test]
    fn escape_without_queue_clears_stale_flush() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        queue.items.clear();
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Streaming))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn cancel_queued_prompt_removes_only_target() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_cancel_queued_prompt);
        let mut queue = PromptQueue::default();
        queue.enqueue("first".into());
        queue.enqueue("second".into());
        let second_id = queue.items[1].id;
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut()
            .trigger(BinReceive::<ChatCancelQueuedPrompt> {
                webview,
                payload: ChatCancelQueuedPrompt { id: second_id },
            });
        app.world_mut().flush();

        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].text, "first");
    }

    #[test]
    fn page_ready_clears_chat_synced_only_for_chat_views() {
        use bevy::prelude::*;
        use bevy_cef::prelude::BinReceive;
        use vmux_core::page::PageReady;

        let mut app = App::new();
        app.add_observer(reset_chat_synced_on_page_ready);

        let chat = app.world_mut().spawn((AgentChatView, ChatSynced)).id();
        let other = app.world_mut().spawn(ChatSynced).id();

        app.world_mut().trigger(BinReceive::<PageReady> {
            webview: chat,
            payload: PageReady {},
        });
        app.world_mut().trigger(BinReceive::<PageReady> {
            webview: other,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert!(
            app.world().get::<ChatSynced>(chat).is_none(),
            "a chat view must re-sync (ChatSynced cleared) when the page reloads"
        );
        assert!(
            app.world().get::<ChatSynced>(other).is_some(),
            "a non-chat view must be left untouched"
        );
    }

    fn duration_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, track_turn_duration);
        app
    }

    #[test]
    fn streaming_then_idle_records_one_duration() {
        let mut app = duration_app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        assert!(
            app.world()
                .get::<AgentTurnMeta>(e)
                .unwrap()
                .turn_start
                .is_some()
        );
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::Idle;
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert_eq!(meta.durations.len(), 1);
        assert!(meta.turn_start.is_none());
    }

    #[test]
    fn awaiting_approval_does_not_finalize() {
        let mut app = duration_app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::AwaitingApproval {
            call_id: "c".into(),
            name: "n".into(),
            args: serde_json::Value::Null,
        };
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert!(meta.durations.is_empty());
        assert!(meta.turn_start.is_some());
    }
}
