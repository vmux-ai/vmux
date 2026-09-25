use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};

use super::media::ChatAttachmentHydrationRequest;
use super::model::{ModeProjection, ModelProjection};
use super::{
    AgentChatView, ChatAttachmentProjection, ChatSnapshotProjection, ChatSynced,
    ChatTranscriptProjection,
};
use crate::handoff::ImportedConversation;
use crate::run_state::{AgentRunState, AgentTurnMeta};
use crate::runtime::acp::{AcpModeState, AcpModelState};
use crate::strategy::{acp_agent_kind, kind_supports_cross_runtime};
use vmux_chat::event::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_SIZE, CHAT_INITIAL_ITEM_LIMIT,
    ChatHistoryRequest, ChatItem, ChatSnapshot, PendingApproval, QueuedPromptSnapshot,
};
use vmux_core::PageMetadata;
use vmux_core::team::{Profile, User};
use vmux_service::chat::{group_turns_before, group_turns_tail, grouped_item_count};
use vmux_session::AcpSession;
use vmux_session::{AgentConversationTitle, AgentMessageTimes, AgentMessages, PromptQueue};

pub(super) struct ChatTranscriptPlugin;

impl Plugin for ChatTranscriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ChatHistoryRequest,)>::default())
            .add_observer(on_chat_history_request)
            .add_observer(reset_chat_synced_on_page_ready)
            .add_systems(
                Update,
                (
                    track_turn_duration,
                    push_chat_to_page,
                    sync_chat_to_ready_views,
                    resolve_chat_history_queries,
                    bevy::ecs::schedule::ApplyDeferred,
                    apply_chat_history_results,
                )
                    .chain(),
            );
    }
}

struct ChatProjection {
    snapshot: ChatSnapshot,
    transcript: TranscriptTail,
}

struct TranscriptTail {
    items: Vec<ChatItem>,
    start: u32,
    total: u32,
}

struct TranscriptPage {
    items: Vec<ChatItem>,
    start: u32,
    end: u32,
    total: u32,
}

#[derive(Component)]
struct ChatHistoryQuery {
    webview: Entity,
    session: Entity,
    generation: u64,
    request_id: u64,
    before: u32,
    limit: u32,
}

#[derive(Component)]
struct ChatHistoryResult {
    webview: Entity,
    generation: u64,
    request_id: u64,
    page: Option<TranscriptPage>,
}

impl ChatTranscriptProjection {
    fn merge_tail(&mut self, tail: TranscriptTail) -> bool {
        if self.tail_start == tail.start
            && self.tail == tail.items
            && self.state.total == tail.total
        {
            return false;
        }
        let initialized = self.state.generation != 0;
        let compatible = initialized
            && tail.total >= self.state.total
            && self.state.loaded_start <= tail.start
            && tail.start.saturating_sub(self.state.loaded_start) as usize
                <= self.state.items.len();
        self.tail.clone_from(&tail.items);
        self.tail_start = tail.start;
        if compatible {
            let keep = tail.start.saturating_sub(self.state.loaded_start) as usize;
            self.state.items.truncate(keep);
            self.state.items.extend(tail.items);
        } else {
            self.state.generation = self.state.generation.wrapping_add(1).max(1);
            self.state.request_id = 0;
            self.state.prepend_revision = 0;
            self.state.items = tail.items;
            self.state.loaded_start = tail.start;
            self.state.loading = false;
        }
        self.state.total = tail.total;
        if self.state.loaded_start == 0 {
            self.state.loading = false;
        }
        true
    }

    fn start_history_query(
        &mut self,
        webview: Entity,
        session: Entity,
        request: &ChatHistoryRequest,
    ) -> Option<ChatHistoryQuery> {
        if request.generation != self.state.generation
            || request.request_id <= self.state.request_id
            || self.state.loaded_start == 0
            || self.state.loading
        {
            return None;
        }
        self.state.request_id = request.request_id;
        self.state.loading = true;
        Some(ChatHistoryQuery {
            webview,
            session,
            generation: request.generation,
            request_id: request.request_id,
            before: self.state.loaded_start,
            limit: CHAT_HISTORY_PAGE_SIZE.min(CHAT_HISTORY_MAX_PAGE_SIZE),
        })
    }

    fn finish_history_query(&mut self, result: &ChatHistoryResult) -> bool {
        if result.generation != self.state.generation
            || result.request_id != self.state.request_id
            || !self.state.loading
        {
            return false;
        }
        self.state.loading = false;
        let Some(page) = result.page.as_ref() else {
            return true;
        };
        if page.end != self.state.loaded_start || page.start > page.end || page.end > page.total {
            return true;
        }
        self.state.items.splice(0..0, page.items.iter().cloned());
        self.state.loaded_start = page.start;
        self.state.total = self.state.total.max(page.total);
        self.state.prepend_revision = self.state.prepend_revision.wrapping_add(1).max(1);
        true
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
        Ref<AgentMessageTimes>,
        Ref<AgentRunState>,
        Option<Ref<AgentTurnMeta>>,
        Option<Ref<Profile>>,
        Option<&PageMetadata>,
        Ref<PromptQueue>,
        Option<Ref<ImportedConversation>>,
        Option<Ref<AgentConversationTitle>>,
    )>,
    children: Query<&Children>,
    mut chat_views: Query<
        (
            &mut ChatTranscriptProjection,
            &mut ChatSnapshotProjection,
            &ChatAttachmentProjection,
        ),
        With<AgentChatView>,
    >,
    choices: Query<&crate::host::PendingAgentChoice>,
    user_profiles: Query<Ref<Profile>, With<User>>,
    browsers: NonSend<Browsers>,
    mut last_push: Local<std::collections::HashMap<Entity, std::time::Instant>>,
    mut owed: Local<std::collections::HashSet<Entity>>,
    mut removed_messages: RemovedComponents<AgentMessages>,
    mut commands: Commands,
) {
    let user_profile = user_profiles.single().ok();
    let user_moved = user_profile
        .as_ref()
        .is_some_and(|profile| profile.is_changed());
    for stack in removed_messages.read() {
        last_push.remove(&stack);
        owed.remove(&stack);
    }
    for (stack, messages, message_times, state, turn_meta, profile, meta, queue, imported, title) in
        &sessions
    {
        let moved = user_moved
            || state.is_changed()
            || turn_meta.as_ref().is_some_and(|meta| meta.is_changed())
            || profile.as_ref().is_some_and(|profile| profile.is_changed())
            || queue.is_changed()
            || imported
                .as_ref()
                .is_some_and(|imported| imported.is_changed())
            || title.as_ref().is_some_and(|title| title.is_changed());
        if !moved && !messages.is_changed() && !message_times.is_changed() && !owed.contains(&stack)
        {
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
        let mut projection = ChatProjection::new(
            &messages,
            &message_times,
            &state,
            turn_meta.as_deref(),
            profile.as_deref(),
            user_profile.as_deref(),
            meta,
            &queue,
            imported.as_deref(),
            title.as_deref(),
            choices.get(webview).ok(),
        );
        let Ok((mut transcript, mut snapshot, attachments)) = chat_views.get_mut(webview) else {
            owed.insert(stack);
            continue;
        };
        let mut transcript_changed = transcript.merge_tail(projection.transcript);
        transcript_changed |= attachments.hydrate_transcript(&mut transcript.state);
        attachments.hydrate_snapshot(&mut projection.snapshot);
        snapshot.0 = projection.snapshot;
        if !matches!(*state, AgentRunState::Streaming) {
            info!(
                ?stack,
                ?webview,
                error = %snapshot.0.error,
                items = messages.0.len(),
                "chat snapshot pushed"
            );
        }
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview,
                &snapshot.0,
            ),
        );
        if transcript_changed {
            commands.trigger(
                vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                    webview,
                    &transcript.state,
                ),
            );
        }
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest { webview, paths });
        }
        last_push.insert(stack, now);
    }
}

const CHAT_STREAM_PUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

fn chat_snapshot_due(streaming: bool, urgent: bool, elapsed: Option<std::time::Duration>) -> bool {
    urgent || !streaming || elapsed.is_none_or(|elapsed| elapsed >= CHAT_STREAM_PUSH_INTERVAL)
}

impl ChatProjection {
    #[allow(clippy::too_many_arguments)]
    fn new(
        messages: &AgentMessages,
        message_times: &AgentMessageTimes,
        state: &AgentRunState,
        turn_meta: Option<&AgentTurnMeta>,
        profile: Option<&Profile>,
        user_profile: Option<&Profile>,
        meta: Option<&PageMetadata>,
        queue: &PromptQueue,
        imported: Option<&ImportedConversation>,
        conversation_title: Option<&AgentConversationTitle>,
        choice: Option<&crate::host::PendingAgentChoice>,
    ) -> Self {
        let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
        let running = matches!(state, AgentRunState::Streaming);
        let imported_messages = imported
            .map(|conversation| conversation.messages.as_slice())
            .unwrap_or_default();
        let page = group_turns_tail(
            imported_messages,
            &messages.0,
            &message_times.0,
            durations,
            running,
            CHAT_INITIAL_ITEM_LIMIT as usize,
        );
        let error = match state {
            AgentRunState::Installing { pct, message } => match pct {
                Some(pct) => format!("{message} ({pct}%)"),
                None => message.clone(),
            },
            AgentRunState::Errored(message) => message.clone(),
            _ => String::new(),
        };
        let approval = match state {
            AgentRunState::AwaitingApproval {
                call_id,
                name,
                args,
            } => Some(PendingApproval {
                call_id: call_id.clone(),
                name: name.clone(),
                args: args.clone().into(),
            }),
            _ => None,
        };
        let (agent_name, accent_color) = profile
            .map(|p| (p.name.clone(), p.avatar.color.clone()))
            .unwrap_or_default();
        let (user_name, user_initials, user_color) = user_profile
            .map(|profile| {
                (
                    profile.name.clone(),
                    profile.avatar.initials.clone(),
                    profile.avatar.color.clone(),
                )
            })
            .unwrap_or_else(|| {
                let profile = Profile::user();
                (profile.name, profile.avatar.initials, profile.avatar.color)
            });
        let agent_icon = meta
            .map(|m| m.icon.favicon_url().to_string())
            .unwrap_or_default();
        Self {
            snapshot: ChatSnapshot {
                status: state.status().to_string(),
                error,
                approval,
                agent_name,
                conversation_title: conversation_title
                    .map(|title| title.0.clone())
                    .unwrap_or_default(),
                agent_icon,
                accent_color,
                user_name,
                user_initials,
                user_color,
                handoff_source: imported
                    .map(|imported| imported.source_agent.clone())
                    .unwrap_or_default(),
                handoff_truncated: imported.is_some_and(|imported| imported.truncated),
                handoff_message_count: imported
                    .map(|imported| {
                        u32::try_from(grouped_item_count(&imported.messages, &[]))
                            .unwrap_or(u32::MAX)
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
                        attachments: item
                            .attachments
                            .iter()
                            .map(|attachment| vmux_chat::event::ChatAttachment {
                                path: attachment.path.clone(),
                                name: attachment.name.clone(),
                                mime_type: attachment.mime_type.clone(),
                                size: attachment.size,
                                preview_data_url: String::new(),
                            })
                            .collect(),
                    })
                    .collect(),
                paused: queue.paused,
            },
            transcript: TranscriptTail {
                items: page.items,
                start: u32::try_from(page.start).unwrap_or(u32::MAX),
                total: u32::try_from(page.total).unwrap_or(u32::MAX),
            },
        }
    }
}

fn sync_chat_to_ready_views(
    mut pending: Query<
        (
            Entity,
            &mut ChatTranscriptProjection,
            &mut ChatSnapshotProjection,
            &ChatAttachmentProjection,
        ),
        (
            With<AgentChatView>,
            With<vmux_core::page::PageReady>,
            Without<ChatSynced>,
        ),
    >,
    child_of: Query<&ChildOf>,
    sessions: Query<(
        &AgentMessages,
        &AgentMessageTimes,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&Profile>,
        Option<&PageMetadata>,
        &PromptQueue,
        Option<&ImportedConversation>,
        Option<&AgentConversationTitle>,
    )>,
    acp_sessions: Query<(&AcpSession, Option<&AcpModelState>, Option<&AcpModeState>)>,
    choices: Query<&crate::host::PendingAgentChoice>,
    user_profiles: Query<&Profile, With<User>>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let user_profile = user_profiles.single().ok();
    for (webview, mut transcript, mut snapshot, attachments) in &mut pending {
        let Ok(parent) = child_of.get(webview) else {
            continue;
        };
        let stack = parent.parent();
        let Ok((messages, message_times, state, turn_meta, profile, meta, queue, imported, title)) =
            sessions.get(stack)
        else {
            continue;
        };
        if !browsers.can_emit_to(&webview) {
            continue;
        }
        let mut projection = ChatProjection::new(
            messages,
            message_times,
            state,
            turn_meta,
            profile,
            user_profile,
            meta,
            queue,
            imported,
            title,
            choices.get(webview).ok(),
        );
        transcript.merge_tail(projection.transcript);
        attachments.hydrate_transcript(&mut transcript.state);
        attachments.hydrate_snapshot(&mut projection.snapshot);
        snapshot.0 = projection.snapshot;
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview,
                &snapshot.0,
            ),
        );
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview,
                &transcript.state,
            ),
        );
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview,
                &attachments.state(),
            ),
        );
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest { webview, paths });
        }
        let (cross, model_state, mode_state, agent_key) = acp_sessions
            .get(stack)
            .ok()
            .map(|(acp, model, mode)| {
                (
                    acp_agent_kind(&acp.agent_id)
                        .map(kind_supports_cross_runtime)
                        .unwrap_or(false),
                    model,
                    mode,
                    acp.agent_id.clone(),
                )
            })
            .unwrap_or((false, None, None, String::new()));
        let model = ModelProjection::new(model_state, cross, &agent_key, settings.as_deref());
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview,
                &model.state,
            ),
        );
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview,
                &model.slash_commands,
            ),
        );
        let mode = ModeProjection::from(mode_state);
        commands.trigger(
            vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                webview, &mode.0,
            ),
        );
        commands.entity(webview).insert(ChatSynced);
    }
}

fn reset_chat_synced_on_page_ready(
    trigger: On<UiInput<vmux_core::page::PageReady>>,
    chat_views: Query<(), With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if chat_views.get(webview).is_ok() {
        commands.entity(webview).remove::<ChatSynced>();
    }
}

fn on_chat_history_request(
    trigger: On<UiInput<ChatHistoryRequest>>,
    mut views: Query<(&ChildOf, &mut ChatTranscriptProjection), With<AgentChatView>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !browsers.can_emit_to(&webview) {
        return;
    }
    let Ok((parent, mut transcript)) = views.get_mut(webview) else {
        return;
    };
    let Some(query) =
        transcript.start_history_query(webview, parent.parent(), &trigger.event().payload)
    else {
        return;
    };
    commands.spawn(query);
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview,
            &transcript.state,
        ),
    );
}

fn resolve_chat_history_queries(
    queries: Query<(Entity, &ChatHistoryQuery)>,
    sessions: Query<(
        &AgentMessages,
        &AgentMessageTimes,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&ImportedConversation>,
    )>,
    mut commands: Commands,
) {
    for (entity, query) in &queries {
        let page = sessions.get(query.session).ok().map(
            |(messages, message_times, state, turn_meta, imported)| {
                let imported_messages = imported
                    .map(|conversation| conversation.messages.as_slice())
                    .unwrap_or_default();
                let durations = turn_meta
                    .map(|meta| meta.durations.as_slice())
                    .unwrap_or(&[]);
                let page = group_turns_before(
                    imported_messages,
                    &messages.0,
                    &message_times.0,
                    durations,
                    matches!(state, AgentRunState::Streaming),
                    query.before as usize,
                    query.limit as usize,
                );
                TranscriptPage {
                    items: page.items,
                    start: u32::try_from(page.start).unwrap_or(u32::MAX),
                    end: u32::try_from(page.end).unwrap_or(u32::MAX),
                    total: u32::try_from(page.total).unwrap_or(u32::MAX),
                }
            },
        );
        commands
            .entity(entity)
            .remove::<ChatHistoryQuery>()
            .insert(ChatHistoryResult {
                webview: query.webview,
                generation: query.generation,
                request_id: query.request_id,
                page,
            });
    }
}

fn apply_chat_history_results(
    results: Query<(Entity, &ChatHistoryResult)>,
    mut views: Query<
        (
            &mut ChatTranscriptProjection,
            &ChatSnapshotProjection,
            &ChatAttachmentProjection,
        ),
        With<AgentChatView>,
    >,
    mut commands: Commands,
) {
    for (entity, result) in &results {
        let Ok((mut transcript, snapshot, attachments)) = views.get_mut(result.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        let mut changed = transcript.finish_history_query(result);
        changed |= attachments.hydrate_transcript(&mut transcript.state);
        commands.entity(entity).despawn();
        if changed {
            commands.trigger(
                vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                    result.webview,
                    &transcript.state,
                ),
            );
        }
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest {
                webview: result.webview,
                paths,
            });
        }
    }
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
        let snapshot = ChatProjection::new(
            &AgentMessages::default(),
            &AgentMessageTimes::default(),
            &AgentRunState::Idle,
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            Some(&imported),
            None,
            None,
        )
        .snapshot;

        assert_eq!(snapshot.handoff_message_count, 2);
    }

    #[test]
    fn snapshot_includes_approval_tool_and_input() {
        let snapshot = ChatProjection::new(
            &AgentMessages::default(),
            &AgentMessageTimes::default(),
            &AgentRunState::AwaitingApproval {
                call_id: "call-1".into(),
                name: "vmux.run".into(),
                args: serde_json::json!({"command": "echo hi", "focus": true}),
            },
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            None,
            None,
            None,
        )
        .snapshot;

        let approval = snapshot.approval.expect("pending approval");
        assert_eq!(approval.name, "vmux.run");
        assert_eq!(
            serde_json::Value::try_from(&approval.args).unwrap(),
            serde_json::json!({"command": "echo hi", "focus": true})
        );
    }

    #[test]
    fn snapshot_includes_model_written_conversation_title() {
        let title = AgentConversationTitle("Refine generated chat summaries".into());
        let snapshot = ChatProjection::new(
            &AgentMessages::default(),
            &AgentMessageTimes::default(),
            &AgentRunState::Idle,
            None,
            None,
            None,
            None,
            &PromptQueue::default(),
            None,
            Some(&title),
            None,
        )
        .snapshot;

        assert_eq!(
            snapshot.conversation_title,
            "Refine generated chat summaries"
        );
    }

    #[test]
    fn snapshot_uses_the_active_user_profile_avatar() {
        let profile = Profile::user_named("Personal".into());
        let snapshot = ChatProjection::new(
            &AgentMessages::default(),
            &AgentMessageTimes::default(),
            &AgentRunState::Idle,
            None,
            None,
            Some(&profile),
            None,
            &PromptQueue::default(),
            None,
            None,
            None,
        )
        .snapshot;

        assert_eq!(snapshot.user_name, "Personal");
        assert_eq!(snapshot.user_initials, "P");
        assert_eq!(snapshot.user_color, "#3b82f6");
    }

    #[test]
    fn transcript_history_is_validated_and_merged_by_host_state() {
        let mut world = World::new();
        let webview = world.spawn_empty().id();
        let session = world.spawn_empty().id();
        let mut transcript = ChatTranscriptProjection::default();
        assert!(transcript.merge_tail(TranscriptTail {
            items: vec![ChatItem::user("two"), ChatItem::user("three")],
            start: 2,
            total: 4,
        }));
        let generation = transcript.state.generation;
        let query = transcript
            .start_history_query(
                webview,
                session,
                &ChatHistoryRequest {
                    generation,
                    request_id: 1,
                },
            )
            .expect("valid history request");
        assert!(transcript.state.loading);
        assert!(transcript.finish_history_query(&ChatHistoryResult {
            webview,
            generation,
            request_id: query.request_id,
            page: Some(TranscriptPage {
                items: vec![ChatItem::user("zero"), ChatItem::user("one")],
                start: 0,
                end: query.before,
                total: 4,
            }),
        }));
        assert_eq!(transcript.state.loaded_start, 0);
        assert_eq!(transcript.state.items.len(), 4);
        assert_eq!(transcript.state.prepend_revision, 1);
        assert!(!transcript.state.loading);
    }

    #[test]
    fn transcript_rejects_stale_generation_and_cursor_results() {
        let mut world = World::new();
        let webview = world.spawn_empty().id();
        let session = world.spawn_empty().id();
        let mut transcript = ChatTranscriptProjection::default();
        transcript.merge_tail(TranscriptTail {
            items: vec![ChatItem::user("two"), ChatItem::user("three")],
            start: 2,
            total: 4,
        });
        let generation = transcript.state.generation;
        assert!(
            transcript
                .start_history_query(
                    webview,
                    session,
                    &ChatHistoryRequest {
                        generation: generation.saturating_sub(1),
                        request_id: 1,
                    },
                )
                .is_none()
        );
        let query = transcript
            .start_history_query(
                webview,
                session,
                &ChatHistoryRequest {
                    generation,
                    request_id: 2,
                },
            )
            .expect("current generation");
        assert!(transcript.finish_history_query(&ChatHistoryResult {
            webview,
            generation,
            request_id: query.request_id,
            page: Some(TranscriptPage {
                items: vec![ChatItem::user("wrong")],
                start: 0,
                end: query.before.saturating_sub(1),
                total: 4,
            }),
        }));
        assert_eq!(transcript.state.loaded_start, 2);
        assert_eq!(transcript.state.items.len(), 2);
        assert!(!transcript.state.loading);
    }

    #[test]
    fn page_ready_clears_chat_synced_only_for_chat_views() {
        use vmux_core::page::PageReady;

        let mut app = App::new();
        app.add_observer(reset_chat_synced_on_page_ready);

        let chat = app.world_mut().spawn((AgentChatView, ChatSynced)).id();
        let other = app.world_mut().spawn(ChatSynced).id();

        app.world_mut().trigger(UiInput::<PageReady> {
            webview: chat,
            payload: PageReady {},
        });
        app.world_mut().trigger(UiInput::<PageReady> {
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
