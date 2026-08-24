use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

use super::model::{effort_current_for, emit_model_state};
use super::{AgentChatView, ChatSynced};
use crate::client::acp::AcpModelState;
use crate::handoff::ImportedConversation;
use crate::run_state::{AgentRunState, AgentTurnMeta};
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_chat::event::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_EVENT, CHAT_INITIAL_ITEM_LIMIT,
    CHAT_SNAPSHOT_EVENT, ChatHistoryPage, ChatHistoryRequest, ChatSnapshot, QueuedPromptSnapshot,
};
use vmux_core::PageMetadata;
use vmux_core::team::Profile;
use vmux_service::chat::{group_turns_before, group_turns_tail, grouped_item_count};
use vmux_session::AcpSession;
use vmux_session::{AgentConversationTitle, AgentMessages, PromptQueue};

pub(super) struct ChatTranscriptPlugin;

impl Plugin for ChatTranscriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(ChatHistoryRequest,)>::for_hosts(
            &["agent", "start"],
        ))
        .add_observer(on_chat_history_request)
        .add_observer(reset_chat_synced_on_page_ready)
        .add_systems(
            Update,
            (
                (track_turn_duration, push_chat_to_page).chain(),
                sync_chat_to_ready_views,
            ),
        );
    }
}

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

fn push_chat_to_page(
    sessions: Query<(
        Entity,
        Ref<AgentMessages>,
        Ref<AgentRunState>,
        Option<Ref<AgentTurnMeta>>,
        Option<Ref<Profile>>,
        Option<&PageMetadata>,
        Ref<PromptQueue>,
        Option<Ref<ImportedConversation>>,
        Option<Ref<AgentConversationTitle>>,
    )>,
    children: Query<&Children>,
    chat_views: Query<(), With<AgentChatView>>,
    choices: Query<&crate::host::PendingAgentChoice>,
    browsers: NonSend<Browsers>,
    mut last_push: Local<std::collections::HashMap<Entity, std::time::Instant>>,
    mut owed: Local<std::collections::HashSet<Entity>>,
    mut removed_messages: RemovedComponents<AgentMessages>,
    mut commands: Commands,
) {
    for stack in removed_messages.read() {
        last_push.remove(&stack);
        owed.remove(&stack);
    }
    for (stack, messages, state, turn_meta, profile, meta, queue, imported, title) in &sessions {
        let moved = state.is_changed()
            || turn_meta.as_ref().is_some_and(|meta| meta.is_changed())
            || profile.as_ref().is_some_and(|profile| profile.is_changed())
            || queue.is_changed()
            || imported
                .as_ref()
                .is_some_and(|imported| imported.is_changed())
            || title.as_ref().is_some_and(|title| title.is_changed());
        if !moved && !messages.is_changed() && !owed.contains(&stack) {
            continue;
        }
        let Ok(kids) = children.get(stack) else {
            owed.insert(stack);
            continue;
        };
        let Some(webview) = kids.iter().find(|&e| chat_views.contains(e)) else {
            owed.insert(stack);
            continue;
        };
        if !browsers.can_emit_to(&webview) {
            if owed.insert(stack) {
                warn!(
                    ?stack,
                    "chat snapshot owed: its view cannot receive one yet"
                );
            }
            continue;
        }
        let now = std::time::Instant::now();
        let elapsed = last_push
            .get(&stack)
            .map(|last| now.saturating_duration_since(*last));
        if !chat_snapshot_due(matches!(*state, AgentRunState::Streaming), moved, elapsed) {
            owed.insert(stack);
            continue;
        }
        owed.remove(&stack);
        let snapshot = snapshot_of(
            &messages,
            &state,
            turn_meta.as_deref(),
            profile.as_deref(),
            meta,
            &queue,
            imported.as_deref(),
            title.as_deref(),
            choices.get(webview).ok(),
        );
        if !matches!(*state, AgentRunState::Streaming) {
            info!(
                ?stack,
                ?webview,
                error = %snapshot.error,
                items = messages.0.len(),
                "chat snapshot pushed"
            );
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot,
        ));
        last_push.insert(stack, now);
    }
}

const CHAT_STREAM_PUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

fn chat_snapshot_due(streaming: bool, urgent: bool, elapsed: Option<std::time::Duration>) -> bool {
    urgent || !streaming || elapsed.is_none_or(|elapsed| elapsed >= CHAT_STREAM_PUSH_INTERVAL)
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
    choice: Option<&crate::host::PendingAgentChoice>,
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
    let error = match state {
        AgentRunState::Installing { pct, message } => match pct {
            Some(pct) => format!("{message} ({pct}%)"),
            None => message.clone(),
        },
        AgentRunState::Errored(message) => message.clone(),
        _ => String::new(),
    };
    let status = state.status();
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
    choices: Query<&crate::host::PendingAgentChoice>,
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
        if !browsers.can_emit_to(&webview) {
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
    if !browsers.can_emit_to(&webview) {
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

#[cfg(test)]
mod tests {
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
    fn page_ready_clears_chat_synced_only_for_chat_views() {
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
