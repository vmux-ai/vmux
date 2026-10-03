use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::group::ChatMessages;
use super::presentation::ChatPresentation;
#[cfg(test)]
use crate::event::ChatItem;
use crate::event::{
    CHAT_INITIAL_ITEM_LIMIT, ChatHistoryMoreRequest, ChatSnapshot, PendingApproval,
    QueuedPromptSnapshot,
};
use crate::host::{ChatAttachmentHydrationRequest, ImportedConversation};
use crate::host::{
    ChatAttachmentProjection, ChatHistoryQuery, ChatHistoryResult, ChatSnapshotProjection,
    ChatSynced, ChatTranscriptProjection, ChatView, PendingAgentChoice, TranscriptPage,
    TranscriptTail,
};
use vmux_ecs::PageMetadata;
use vmux_ecs::service::ServiceMessageSet;
use vmux_ecs::team::{Profile, User};
use vmux_session::{
    AcpSession, AgentConversationTitle, AgentMessageTimes, AgentMessages, PromptQueue,
};
use vmux_session::{AgentRunState, AgentTurnMeta};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ChatHistoryMoreRequest,)>::default())
            .add_observer(request_more)
            .add_observer(reset_synced)
            .add_systems(
                Update,
                (
                    track_turn_duration,
                    push_to_page,
                    sync_ready_views,
                    resolve_queries,
                    bevy::ecs::schedule::ApplyDeferred,
                    apply_results,
                )
                    .chain()
                    .after(ServiceMessageSet),
            );
    }
}

struct ChatProjection {
    snapshot: ChatSnapshot,
    transcript: TranscriptTail,
}

type ChangedSession = (
    Entity,
    &'static AcpSession,
    Ref<'static, AgentMessages>,
    Ref<'static, AgentMessageTimes>,
    Ref<'static, AgentRunState>,
    Option<Ref<'static, AgentTurnMeta>>,
    Option<Ref<'static, Profile>>,
    Option<&'static PageMetadata>,
    Ref<'static, PromptQueue>,
    Option<Ref<'static, ImportedConversation>>,
    Option<Ref<'static, AgentConversationTitle>>,
);

type ProjectionView = (
    &'static mut ChatTranscriptProjection,
    &'static mut ChatSnapshotProjection,
    &'static ChatAttachmentProjection,
);

#[derive(SystemParam)]
struct PushWorld<'w, 's> {
    sessions: Query<'w, 's, ChangedSession>,
    children: Query<'w, 's, &'static Children>,
    chat_views: Query<'w, 's, ProjectionView, With<ChatView>>,
    choices: Query<'w, 's, &'static PendingAgentChoice>,
    user_profiles: Query<'w, 's, Ref<'static, Profile>, With<User>>,
}

type ReadyView = (
    Entity,
    &'static mut ChatTranscriptProjection,
    &'static mut ChatSnapshotProjection,
    &'static ChatAttachmentProjection,
);

type ReadyViewFilter = (
    With<ChatView>,
    With<vmux_ecs::page::PageReady>,
    Without<ChatSynced>,
);

type SessionProjection = (
    &'static AcpSession,
    &'static AgentMessages,
    &'static AgentMessageTimes,
    &'static AgentRunState,
    Option<&'static AgentTurnMeta>,
    Option<&'static Profile>,
    Option<&'static PageMetadata>,
    &'static PromptQueue,
    Option<&'static ImportedConversation>,
    Option<&'static AgentConversationTitle>,
);

type HistorySource = (
    &'static AgentMessages,
    &'static AgentMessageTimes,
    &'static AgentRunState,
    Option<&'static AgentTurnMeta>,
    Option<&'static ImportedConversation>,
);

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

fn push_to_page(
    mut world: PushWorld,
    mut last_push: Local<std::collections::HashMap<Entity, std::time::Instant>>,
    mut owed: Local<std::collections::HashSet<Entity>>,
    mut removed_messages: RemovedComponents<AgentMessages>,
    mut commands: Commands,
) {
    let user_profile = world.user_profiles.single().ok();
    let user_moved = user_profile
        .as_ref()
        .is_some_and(|profile| profile.is_changed());
    for stack in removed_messages.read() {
        last_push.remove(&stack);
        owed.remove(&stack);
    }
    for (
        stack,
        session,
        messages,
        message_times,
        state,
        turn_meta,
        profile,
        meta,
        queue,
        imported,
        title,
    ) in &world.sessions
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
        let Ok(kids) = world.children.get(stack) else {
            owed.insert(stack);
            continue;
        };
        let Some(webview) = kids.iter().find(|&e| world.chat_views.contains(e)) else {
            owed.insert(stack);
            continue;
        };
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
            session,
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
            world.choices.get(webview).ok(),
        );
        let Ok((mut transcript, mut snapshot, attachments)) = world.chat_views.get_mut(webview)
        else {
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
            vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &snapshot.0,
            ),
        );
        if transcript_changed {
            commands.trigger(
                vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
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
        session: &AcpSession,
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
        choice: Option<&PendingAgentChoice>,
    ) -> Self {
        let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
        let running = matches!(state, AgentRunState::Streaming);
        let imported_messages = imported
            .map(|conversation| conversation.messages.as_slice())
            .unwrap_or_default();
        let page = ChatMessages::new(
            imported_messages,
            &messages.0,
            &message_times.0,
            durations,
            running,
        )
        .tail(CHAT_INITIAL_ITEM_LIMIT as usize);
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
            } => {
                let args = vmux_api::json::JsonValue::from(args.clone());
                Some(PendingApproval::new(call_id.clone(), name.clone(), &args))
            }
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
        let transcript_empty = page.items.is_empty();
        let mut snapshot = ChatSnapshot {
            status: state.status().to_string(),
            error,
            approval,
            agent_id: session.agent_id.clone(),
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
                    u32::try_from(
                        ChatMessages::new(&imported.messages, &[], &[], &[], false).item_count(),
                    )
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
                        .map(|attachment| crate::event::ChatAttachment {
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
            ..Default::default()
        };
        ChatPresentation::apply(&mut snapshot, transcript_empty);
        Self {
            snapshot,
            transcript: TranscriptTail {
                items: page.items,
                start: u32::try_from(page.start).unwrap_or(u32::MAX),
                total: u32::try_from(page.total).unwrap_or(u32::MAX),
            },
        }
    }
}

fn sync_ready_views(
    mut pending: Query<ReadyView, ReadyViewFilter>,
    child_of: Query<&ChildOf>,
    sessions: Query<SessionProjection>,
    choices: Query<&PendingAgentChoice>,
    user_profiles: Query<&Profile, With<User>>,
    mut commands: Commands,
) {
    let user_profile = user_profiles.single().ok();
    for (webview, mut transcript, mut snapshot, attachments) in &mut pending {
        let Ok(parent) = child_of.get(webview) else {
            continue;
        };
        let stack = parent.parent();
        let Ok((
            session,
            messages,
            message_times,
            state,
            turn_meta,
            profile,
            meta,
            queue,
            imported,
            title,
        )) = sessions.get(stack)
        else {
            continue;
        };
        let mut projection = ChatProjection::new(
            session,
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
            vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &snapshot.0,
            ),
        );
        commands.trigger(
            vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &transcript.state,
            ),
        );
        commands.trigger(
            vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
                webview,
                &attachments.state(),
            ),
        );
        let paths = attachments.hydration_paths(&transcript.state, &snapshot.0);
        if !paths.is_empty() {
            commands.trigger(ChatAttachmentHydrationRequest { webview, paths });
        }
        commands.entity(webview).insert(ChatSynced);
    }
}

fn reset_synced(
    trigger: On<UiInput<vmux_ecs::page::PageReady>>,
    chat_views: Query<(), With<ChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if chat_views.get(webview).is_ok() {
        commands.entity(webview).remove::<ChatSynced>();
    }
}

fn request_more(
    trigger: On<UiInput<ChatHistoryMoreRequest>>,
    mut views: Query<(&ChildOf, &mut ChatTranscriptProjection), With<ChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((parent, mut transcript)) = views.get_mut(webview) else {
        return;
    };
    let Some(query) = transcript.start_history_query(webview, parent.parent()) else {
        return;
    };
    commands.spawn(query);
    commands.trigger(
        vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
            webview,
            &transcript.state,
        ),
    );
}

fn resolve_queries(
    queries: Query<(Entity, &ChatHistoryQuery)>,
    sessions: Query<HistorySource>,
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
                let page = ChatMessages::new(
                    imported_messages,
                    &messages.0,
                    &message_times.0,
                    durations,
                    matches!(state, AgentRunState::Streaming),
                )
                .before(query.before as usize, query.limit as usize);
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

fn apply_results(
    results: Query<(Entity, &ChatHistoryResult)>,
    mut views: Query<
        (
            &mut ChatTranscriptProjection,
            &ChatSnapshotProjection,
            &ChatAttachmentProjection,
        ),
        With<ChatView>,
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
                vmux_ecs::host::UiStateWrite::<crate::state::ChatUiState>::from_event(
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
    use vmux_ecs::page::PageReady;

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
        let session = AcpSession {
            agent_id: "codex".into(),
            sid: "session".into(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_ecs::ProcessId::new(),
            resume: None,
        };
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_sid: "codex-1".into(),
            messages: vec![
                vmux_api::room::Message::user("one"),
                vmux_api::room::Message::Assistant {
                    blocks: vec![vmux_api::room::AssistantBlock::ToolUse {
                        call_id: "call-1".into(),
                        name: "run".into(),
                        args: "{}".into(),
                        parent_call_id: None,
                    }],
                },
                vmux_api::room::Message::ToolResult {
                    call_id: "call-1".into(),
                    content: "two".into(),
                    is_error: false,
                },
            ],
            truncated: false,
            first_prompt: None,
        };
        let snapshot = ChatProjection::new(
            &session,
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
        let session = AcpSession {
            agent_id: "codex".into(),
            sid: "session".into(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_ecs::ProcessId::new(),
            resume: None,
        };
        let snapshot = ChatProjection::new(
            &session,
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
        assert_eq!(approval.details[0].label, "Command");
        assert_eq!(approval.details[0].value, "echo hi");
        assert_eq!(approval.details[1].label, "Focus");
        assert_eq!(approval.details[1].value, "true");
    }

    #[test]
    fn snapshot_includes_model_written_conversation_title() {
        let session = AcpSession {
            agent_id: "codex".into(),
            sid: "session".into(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_ecs::ProcessId::new(),
            resume: None,
        };
        let title = AgentConversationTitle("Refine generated chat summaries".into());
        let snapshot = ChatProjection::new(
            &session,
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
        let session = AcpSession {
            agent_id: "codex".into(),
            sid: "session".into(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_ecs::ProcessId::new(),
            resume: None,
        };
        let profile = Profile::user_named("Personal".into());
        let snapshot = ChatProjection::new(
            &session,
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
        let query = transcript
            .start_history_query(webview, session)
            .expect("valid history request");
        let generation = query.generation;
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
        let stale = transcript
            .start_history_query(webview, session)
            .expect("initial generation");
        transcript.merge_tail(TranscriptTail {
            items: vec![ChatItem::user("replacement")],
            start: 1,
            total: 2,
        });
        assert!(!transcript.finish_history_query(&ChatHistoryResult {
            webview,
            generation: stale.generation,
            request_id: stale.request_id,
            page: None,
        }));
        let query = transcript
            .start_history_query(webview, session)
            .expect("current generation");
        let generation = query.generation;
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
        assert_eq!(transcript.state.loaded_start, 1);
        assert_eq!(transcript.state.items.len(), 1);
        assert!(!transcript.state.loading);
    }

    #[test]
    fn page_ready_clears_chat_synced_only_for_chat_views() {
        let mut app = App::new();
        app.add_observer(reset_synced);

        let chat = app.world_mut().spawn((ChatView, ChatSynced)).id();
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
