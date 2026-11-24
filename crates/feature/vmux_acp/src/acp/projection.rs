use std::collections::HashSet;

use agent_client_protocol::schema::v1::{
    ContentBlock, Plan, PlanEntryStatus, RequestPermissionRequest, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions, SessionModeState,
    SessionUpdate, ToolCall, ToolCallContent, ToolCallLocation, ToolCallStatus, ToolCallUpdate,
    ToolKind,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use tokio::sync::oneshot;
use vmux_api::protocol::{
    AcpSessionConfig, AcpSessionConfigValue, AgentAttachment, AgentFileTouched, AgentRequest,
    AgentRequestId, FileTouchKind, ServiceMessage, SharedEvent,
};
use vmux_api::session::{AssistantBlock, Message, PlanStep};
use vmux_ecs::{AsyncChannel, Order};

use super::workspace_driver::WorkspaceLocation;
use super::{
    AcpApprovalDetails, AcpHistoryReplay, AcpReplayBegin, AcpReplayFinish, AcpSessionConfigs,
    AcpSessionCwd, AcpSessionShared, AcpSessionUpdate, AcpSnapshotRequest, SessionId,
};

pub(super) struct AcpProjectionPlugin;

impl Plugin for AcpProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(begin_history_replay)
            .add_observer(apply_session_update)
            .add_observer(finish_history_replay)
            .add_observer(emit_transcript_snapshot)
            .add_observer(emit_proposed_diff)
            .add_observer(emit_file_touched)
            .add_observer(resolve_approval_details)
            .add_observer(record_user_message)
            .add_systems(Update, drain_inboxes.in_set(AcpProjectionSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AcpProjectionSet;

#[derive(Component, Default)]
pub(super) struct AcpProjectionOrder(u32);

impl AcpProjectionOrder {
    fn next(&mut self) -> Order {
        let current = self.0;
        self.0 = self.0.saturating_add(1);
        Order(current)
    }
}

#[derive(Component)]
pub(super) struct AcpTranscriptEntry;

#[derive(Component)]
pub(super) struct AcpMessage(pub(super) Message);

#[derive(Component)]
struct AcpHiddenToolCall;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct AcpToolCallId(String);

impl AcpToolCallId {
    pub(super) fn is_conversation_title(title: &str) -> bool {
        title
            .trim()
            .to_ascii_lowercase()
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '-' | '.' | ':' | '_')
            })
            .filter(|part| !part.is_empty())
            .eq(["mcp", "vmux", "set", "conversation", "title"])
    }
}

#[derive(Component)]
struct AcpToolInput(String);

impl AcpToolInput {
    fn from_raw(raw: Option<&serde_json::Value>) -> String {
        raw.map(serde_json::Value::to_string)
            .unwrap_or_else(|| "{}".to_string())
    }
}

#[derive(Component)]
struct AcpFileTouch {
    kind: ToolKind,
    locations: Vec<ToolCallLocation>,
    has_pending_edit: bool,
}

impl AcpFileTouch {
    const ACTIVE_LIMIT: usize = 1024;
    const FINALIZED_LIMIT: usize = 1024;

    fn file_touch_kind(kind: ToolKind) -> Option<FileTouchKind> {
        match kind {
            ToolKind::Read => Some(FileTouchKind::Read),
            ToolKind::Edit | ToolKind::Delete | ToolKind::Move => Some(FileTouchKind::Edit),
            _ => None,
        }
    }

    fn is_edit(kind: ToolKind) -> bool {
        matches!(kind, ToolKind::Edit | ToolKind::Delete | ToolKind::Move)
    }

    fn touches(&self) -> Vec<(String, Option<u32>, FileTouchKind)> {
        let Some(kind) = Self::file_touch_kind(self.kind) else {
            return Vec::new();
        };
        let mut items = Vec::with_capacity(self.locations.len());
        for location in &self.locations {
            items.push((
                location.path.to_string_lossy().into_owned(),
                location.line,
                kind,
            ));
        }
        items
    }

    fn update(
        &mut self,
        kind: Option<ToolKind>,
        locations: Option<&[ToolCallLocation]>,
        status: Option<ToolCallStatus>,
        full_update: bool,
    ) -> (Vec<(String, Option<u32>, FileTouchKind)>, bool) {
        let identity_changed = full_update
            || kind.is_some_and(|kind| kind != self.kind)
            || locations.is_some_and(|locations| locations != self.locations);
        if let Some(kind) = kind {
            self.kind = kind;
        }
        if let Some(locations) = locations {
            self.locations = locations.to_vec();
        }
        match status {
            Some(ToolCallStatus::Failed) => {
                self.has_pending_edit = false;
                (Vec::new(), true)
            }
            Some(ToolCallStatus::Completed) => {
                let emitted = if identity_changed {
                    self.touches()
                } else if self.has_pending_edit {
                    self.has_pending_edit = false;
                    self.touches()
                } else {
                    Vec::new()
                };
                (emitted, true)
            }
            _ if identity_changed => {
                self.has_pending_edit = Self::is_edit(self.kind);
                (self.touches(), false)
            }
            _ => (Vec::new(), false),
        }
    }
}

#[derive(Component)]
struct AcpFinalizedFileTouch;

#[derive(Clone)]
pub(super) struct ApprovalDetailsQuery {
    call_id: String,
    name: Option<String>,
    args: Option<String>,
    kind: Option<ToolKind>,
}

impl ApprovalDetailsQuery {
    pub(super) fn from_request(request: &RequestPermissionRequest) -> Self {
        Self {
            call_id: request.tool_call.tool_call_id.to_string(),
            name: request
                .tool_call
                .fields
                .title
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string),
            args: request
                .tool_call
                .fields
                .raw_input
                .as_ref()
                .map(serde_json::Value::to_string),
            kind: request.tool_call.fields.kind,
        }
    }

    pub(super) fn fallback(&self) -> (String, String) {
        let name = match self.kind {
            Some(ToolKind::Read) => "Read data",
            Some(ToolKind::Edit) => "Edit files",
            Some(ToolKind::Delete) => "Delete files",
            Some(ToolKind::Move) => "Move files",
            Some(ToolKind::Search) => "Search",
            Some(ToolKind::Execute) => "Execute command",
            Some(ToolKind::Think) => "Think",
            Some(ToolKind::Fetch) => "Fetch data",
            Some(ToolKind::SwitchMode) => "Switch mode",
            _ => "Use tool",
        };
        (
            name.to_string(),
            self.args.clone().unwrap_or_else(|| "{}".to_string()),
        )
    }

    fn resolve(&self, projected: Option<(String, String)>) -> Option<(String, String)> {
        let (projected_name, projected_args) = projected.unwrap_or_default();
        let name = self
            .name
            .clone()
            .or_else(|| (!projected_name.is_empty()).then_some(projected_name))?;
        let args = self
            .args
            .clone()
            .or_else(|| (!projected_args.is_empty()).then_some(projected_args))
            .unwrap_or_else(|| "{}".to_string());
        Some((name, args))
    }
}

pub(super) struct ApprovalDetailsRequest {
    pub(super) query: ApprovalDetailsQuery,
    pub(super) response: oneshot::Sender<Option<(String, String)>>,
}

#[derive(EntityEvent)]
struct BeginHistoryReplay {
    #[event_target]
    session: Entity,
}

#[derive(EntityEvent)]
struct ApplySessionUpdate {
    #[event_target]
    session: Entity,
    update: Box<SessionUpdate>,
}

#[derive(EntityEvent)]
struct FinishHistoryReplay {
    #[event_target]
    session: Entity,
    loaded: bool,
}

#[derive(EntityEvent)]
struct EmitTranscriptSnapshot {
    #[event_target]
    session: Entity,
}

#[derive(EntityEvent)]
struct EmitProposedDiff {
    #[event_target]
    session: Entity,
    call_id: String,
    path: String,
    old_text: Option<String>,
    new_text: String,
}

#[derive(EntityEvent)]
struct EmitFileTouched {
    #[event_target]
    session: Entity,
    path: String,
    line: Option<u32>,
    kind: FileTouchKind,
}

#[derive(EntityEvent)]
struct ResolveApprovalDetails {
    #[event_target]
    session: Entity,
    request: ApprovalDetailsRequest,
}

#[derive(EntityEvent)]
pub(super) struct RecordUserMessage {
    #[event_target]
    pub(super) session: Entity,
    pub(super) text: String,
    pub(super) attachments: Vec<AgentAttachment>,
}

#[derive(SystemParam)]
pub(super) struct AcpTranscripts<'w, 's> {
    entries: Query<
        'w,
        's,
        (&'static ChildOf, &'static Order, &'static AcpMessage),
        With<AcpTranscriptEntry>,
    >,
}

impl AcpTranscripts<'_, '_> {
    pub(super) fn messages(&self, session: Entity) -> Vec<Message> {
        let mut ordered = Vec::new();
        for (parent, order, message) in &self.entries {
            if parent.parent() == session {
                ordered.push((order.0, &message.0));
            }
        }
        ordered.sort_by_key(|(order, _)| *order);
        let mut messages = Vec::with_capacity(ordered.len());
        for (_, message) in ordered {
            messages.push(message.clone());
        }
        messages
    }
}

#[derive(SystemParam)]
struct TranscriptWriter<'w, 's> {
    entries: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Order,
            &'static mut AcpMessage,
        ),
        With<AcpTranscriptEntry>,
    >,
    hidden: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static AcpToolCallId,
            &'static mut Name,
            &'static mut AcpToolInput,
        ),
        With<AcpHiddenToolCall>,
    >,
    orders: Query<'w, 's, &'static mut AcpProjectionOrder>,
    commands: Commands<'w, 's>,
}

impl TranscriptWriter<'_, '_> {
    fn next_order(&mut self, session: Entity) -> Order {
        match self.orders.get_mut(session) {
            Ok(mut counter) => counter.next(),
            Err(_) => Order(0),
        }
    }

    fn last_entry(&self, session: Entity) -> Option<Entity> {
        let mut latest: Option<(u32, Entity)> = None;
        for (entity, parent, order, _) in &self.entries {
            if parent.parent() != session {
                continue;
            }
            if latest.is_none_or(|(best, _)| order.0 >= best) {
                latest = Some((order.0, entity));
            }
        }
        latest.map(|(_, entity)| entity)
    }

    fn spawn_entry(&mut self, session: Entity, message: Message) {
        let order = self.next_order(session);
        self.commands.spawn((
            AcpTranscriptEntry,
            AcpMessage(message),
            order,
            ChildOf(session),
        ));
    }

    fn push_assistant_blocks(&mut self, session: Entity, new_blocks: Vec<AssistantBlock>) {
        if new_blocks.is_empty() {
            return;
        }
        if let Some(last) = self.last_entry(session)
            && let Ok((_, _, _, mut entry)) = self.entries.get_mut(last)
            && let Message::Assistant { blocks } = &mut entry.0
        {
            blocks.extend(new_blocks);
            return;
        }
        self.spawn_entry(session, Message::Assistant { blocks: new_blocks });
    }

    fn push_assistant_block(&mut self, session: Entity, block: AssistantBlock) {
        self.push_assistant_blocks(session, vec![block]);
    }

    fn append_assistant_text(&mut self, session: Entity, text: &str) {
        if let Some(last) = self.last_entry(session)
            && let Ok((_, _, _, mut entry)) = self.entries.get_mut(last)
            && let Message::Assistant { blocks } = &mut entry.0
        {
            match blocks.last_mut() {
                Some(AssistantBlock::Text(existing)) => existing.push_str(text),
                _ => blocks.push(AssistantBlock::Text(text.to_string())),
            }
            return;
        }
        self.spawn_entry(
            session,
            Message::Assistant {
                blocks: vec![AssistantBlock::Text(text.to_string())],
            },
        );
    }

    fn append_thinking(&mut self, session: Entity, text: &str) {
        if let Some(last) = self.last_entry(session)
            && let Ok((_, _, _, mut entry)) = self.entries.get_mut(last)
            && let Message::Assistant { blocks } = &mut entry.0
        {
            match blocks.last_mut() {
                Some(AssistantBlock::Thinking(existing)) => existing.push_str(text),
                _ => blocks.push(AssistantBlock::Thinking(text.to_string())),
            }
            return;
        }
        self.spawn_entry(
            session,
            Message::Assistant {
                blocks: vec![AssistantBlock::Thinking(text.to_string())],
            },
        );
    }

    fn append_user_chunk(&mut self, session: Entity, text: &str) {
        if let Some(last) = self.last_entry(session)
            && let Ok((_, _, _, mut entry)) = self.entries.get_mut(last)
            && let Message::User { text: existing, .. } = &mut entry.0
        {
            existing.push_str(text);
            return;
        }
        self.spawn_entry(session, Message::user(text.to_string()));
    }

    fn upsert_plan(&mut self, session: Entity, plan: Plan) {
        let mut steps = Vec::with_capacity(plan.entries.len());
        for entry in &plan.entries {
            let status = match entry.status {
                PlanEntryStatus::InProgress => "in_progress",
                PlanEntryStatus::Completed => "completed",
                _ => "pending",
            };
            steps.push(PlanStep {
                content: entry.content.clone(),
                status: status.to_string(),
            });
        }
        for (_, parent, _, mut entry) in &mut self.entries {
            if parent.parent() != session {
                continue;
            }
            if let Message::Assistant { blocks } = &mut entry.0 {
                for block in blocks.iter_mut() {
                    if let AssistantBlock::Plan { steps: existing } = block {
                        *existing = steps;
                        return;
                    }
                }
            }
        }
        self.push_assistant_block(session, AssistantBlock::Plan { steps });
    }

    fn upsert_tool_use(
        &mut self,
        session: Entity,
        call_id: &str,
        name: &str,
        args: &str,
        kind: Option<vmux_api::session::ToolKind>,
        pending: &mut Vec<AssistantBlock>,
    ) {
        for (_, parent, _, mut entry) in &mut self.entries {
            if parent.parent() != session {
                continue;
            }
            let Message::Assistant { blocks } = &mut entry.0 else {
                continue;
            };
            for block in blocks.iter_mut() {
                if let AssistantBlock::ToolUse {
                    call_id: existing,
                    name: existing_name,
                    args: existing_args,
                    kind: existing_kind,
                    ..
                } = block
                    && existing == call_id
                {
                    if !name.is_empty() {
                        *existing_name = name.to_string();
                    }
                    if args != "{}" {
                        *existing_args = args.to_string();
                    }
                    if kind.is_some() {
                        *existing_kind = kind;
                    }
                    return;
                }
            }
        }
        pending.push(AssistantBlock::ToolUse {
            call_id: call_id.to_string(),
            name: name.to_string(),
            args: args.to_string(),
            parent_call_id: None,
            kind,
        });
    }

    fn upsert_diff(
        &mut self,
        session: Entity,
        call_id: &str,
        path: &str,
        old_text: Option<String>,
        new_text: String,
        pending: &mut Vec<AssistantBlock>,
    ) {
        for (_, parent, _, mut entry) in &mut self.entries {
            if parent.parent() != session {
                continue;
            }
            let Message::Assistant { blocks } = &mut entry.0 else {
                continue;
            };
            for block in blocks.iter_mut() {
                if let AssistantBlock::Diff {
                    call_id: existing,
                    old_text: existing_old,
                    new_text: existing_new,
                    ..
                } = block
                    && existing == call_id
                {
                    *existing_old = old_text;
                    *existing_new = new_text;
                    return;
                }
            }
        }
        for block in pending.iter_mut() {
            if let AssistantBlock::Diff {
                call_id: existing,
                old_text: existing_old,
                new_text: existing_new,
                ..
            } = block
                && existing == call_id
            {
                *existing_old = old_text;
                *existing_new = new_text;
                return;
            }
        }
        pending.push(AssistantBlock::Diff {
            call_id: call_id.to_string(),
            path: path.to_string(),
            old_text,
            new_text,
        });
    }

    fn upsert_tool_result(
        &mut self,
        session: Entity,
        call_id: &str,
        content: String,
        is_error: bool,
    ) {
        for (_, parent, _, mut entry) in &mut self.entries {
            if parent.parent() != session {
                continue;
            }
            if let Message::ToolResult {
                call_id: existing,
                content: existing_content,
                is_error: existing_error,
            } = &mut entry.0
                && existing == call_id
            {
                *existing_content = content;
                *existing_error = is_error;
                return;
            }
        }
        self.spawn_entry(
            session,
            Message::ToolResult {
                call_id: call_id.to_string(),
                content,
                is_error,
            },
        );
    }

    fn record_tool_content(
        &mut self,
        session: Entity,
        call_id: &str,
        content: &[ToolCallContent],
        failed: bool,
        pending: &mut Vec<AssistantBlock>,
    ) -> Vec<(String, String, Option<String>, String)> {
        let mut diffs = Vec::new();
        let mut has_terminal = false;
        let mut output = String::new();
        for item in content {
            match item {
                ToolCallContent::Diff(diff) => {
                    let path = diff.path.to_string_lossy().into_owned();
                    self.upsert_diff(
                        session,
                        call_id,
                        &path,
                        diff.old_text.clone(),
                        diff.new_text.clone(),
                        pending,
                    );
                    diffs.push((
                        call_id.to_string(),
                        path,
                        diff.old_text.clone(),
                        diff.new_text.clone(),
                    ));
                }
                ToolCallContent::Terminal(_) => has_terminal = true,
                ToolCallContent::Content(inner) => {
                    if let ContentBlock::Text(text) = &inner.content {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(&text.text);
                    }
                }
                _ => {}
            }
        }
        self.push_assistant_blocks(session, std::mem::take(pending));
        if !output.is_empty() {
            self.upsert_tool_result(session, call_id, output, failed);
        } else if has_terminal {
            self.upsert_tool_result(
                session,
                call_id,
                "[terminal output shown in the attached pane]".to_string(),
                failed,
            );
        }
        diffs
    }

    fn apply_tool_call(
        &mut self,
        session: Entity,
        tc: &ToolCall,
        replay_active: bool,
        touches: &mut FileTouchTracker,
    ) {
        let call_id = tc.tool_call_id.to_string();
        if AcpToolCallId::is_conversation_title(&tc.title) {
            let finished = matches!(
                tc.status,
                ToolCallStatus::Completed | ToolCallStatus::Failed
            );
            self.update_hidden(
                session,
                &call_id,
                &tc.title,
                tc.raw_input.as_ref(),
                finished,
            );
            return;
        }
        let mut pending = Vec::new();
        self.upsert_tool_use(
            session,
            &call_id,
            &tc.title,
            &AcpToolInput::from_raw(tc.raw_input.as_ref()),
            Some(Self::classify_tool_kind(tc.kind)),
            &mut pending,
        );
        let diff_locs = tc
            .locations
            .is_empty()
            .then(|| Self::diff_locations(&tc.content));
        let locations = diff_locs.as_deref().unwrap_or(&tc.locations);
        let order = self.next_order(session);
        let touched = touches.project(
            session,
            &call_id,
            Some(tc.kind),
            Some(locations),
            Some(tc.status),
            true,
            order,
        );
        let failed = matches!(tc.status, ToolCallStatus::Failed);
        let diffs = self.record_tool_content(session, &call_id, &tc.content, failed, &mut pending);
        if !Self::has_text_content(&tc.content) {
            self.record_raw_output(session, &call_id, tc.raw_output.as_ref(), failed);
        }
        if !replay_active {
            self.emit_tool_events(session, touched, diffs);
        }
    }

    fn apply_tool_call_update(
        &mut self,
        session: Entity,
        update: &ToolCallUpdate,
        replay_active: bool,
        touches: &mut FileTouchTracker,
    ) {
        let call_id = update.tool_call_id.to_string();
        let title = update.fields.title.as_deref().unwrap_or_default();
        if self.is_hidden(session, &call_id) || AcpToolCallId::is_conversation_title(title) {
            let finished = matches!(
                update.fields.status,
                Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
            );
            self.update_hidden(
                session,
                &call_id,
                title,
                update.fields.raw_input.as_ref(),
                finished,
            );
            return;
        }
        let mut pending = Vec::new();
        self.upsert_tool_use(
            session,
            &call_id,
            title,
            &AcpToolInput::from_raw(update.fields.raw_input.as_ref()),
            update.fields.kind.map(Self::classify_tool_kind),
            &mut pending,
        );
        let diff_locs = update
            .fields
            .content
            .as_deref()
            .map(Self::diff_locations)
            .unwrap_or_default();
        let locations = update.fields.locations.as_deref();
        let locations = if locations.is_none_or(|items| items.is_empty()) && !diff_locs.is_empty() {
            Some(diff_locs.as_slice())
        } else {
            locations
        };
        let order = self.next_order(session);
        let touched = touches.project(
            session,
            &call_id,
            update.fields.kind,
            locations,
            update.fields.status,
            false,
            order,
        );
        let mut diffs = Vec::new();
        if let Some(content) = &update.fields.content {
            let failed = matches!(update.fields.status, Some(ToolCallStatus::Failed));
            diffs = self.record_tool_content(session, &call_id, content, failed, &mut pending);
            if !Self::has_text_content(content) {
                self.record_raw_output(
                    session,
                    &call_id,
                    update.fields.raw_output.as_ref(),
                    failed,
                );
            }
        } else {
            self.push_assistant_blocks(session, pending);
            if matches!(
                update.fields.status,
                Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
            ) {
                let failed = matches!(update.fields.status, Some(ToolCallStatus::Failed));
                self.record_raw_output(
                    session,
                    &call_id,
                    update.fields.raw_output.as_ref(),
                    failed,
                );
            }
        }
        if !replay_active {
            self.emit_tool_events(session, touched, diffs);
        }
    }

    fn emit_tool_events(
        &mut self,
        session: Entity,
        touched: Vec<(String, Option<u32>, FileTouchKind)>,
        diffs: Vec<(String, String, Option<String>, String)>,
    ) {
        self.commands.trigger(EmitTranscriptSnapshot { session });
        for (path, line, kind) in touched {
            self.commands.trigger(EmitFileTouched {
                session,
                path,
                line,
                kind,
            });
        }
        for (call_id, path, old_text, new_text) in diffs {
            self.commands.trigger(EmitProposedDiff {
                session,
                call_id,
                path,
                old_text,
                new_text,
            });
        }
    }

    fn classify_tool_kind(kind: ToolKind) -> vmux_api::session::ToolKind {
        match kind {
            ToolKind::Read => vmux_api::session::ToolKind::Read,
            ToolKind::Edit => vmux_api::session::ToolKind::Edit,
            ToolKind::Delete => vmux_api::session::ToolKind::Delete,
            ToolKind::Move => vmux_api::session::ToolKind::Move,
            ToolKind::Search => vmux_api::session::ToolKind::Search,
            ToolKind::Execute => vmux_api::session::ToolKind::Execute,
            ToolKind::Think => vmux_api::session::ToolKind::Think,
            ToolKind::Fetch => vmux_api::session::ToolKind::Fetch,
            ToolKind::SwitchMode => vmux_api::session::ToolKind::SwitchMode,
            _ => vmux_api::session::ToolKind::Other,
        }
    }

    fn diff_locations(content: &[ToolCallContent]) -> Vec<ToolCallLocation> {
        let mut paths = HashSet::new();
        let mut locations = Vec::new();
        for item in content {
            if let ToolCallContent::Diff(diff) = item
                && paths.insert(diff.path.clone())
            {
                locations.push(ToolCallLocation::new(diff.path.clone()));
            }
        }
        locations
    }

    fn has_text_content(content: &[ToolCallContent]) -> bool {
        for item in content {
            if let ToolCallContent::Content(inner) = item
                && let ContentBlock::Text(text) = &inner.content
                && !text.text.is_empty()
            {
                return true;
            }
        }
        false
    }

    fn record_raw_output(
        &mut self,
        session: Entity,
        call_id: &str,
        raw_output: Option<&serde_json::Value>,
        is_error: bool,
    ) {
        let Some(raw_output) = raw_output.filter(|value| !value.is_null()) else {
            return;
        };
        let pretty =
            serde_json::to_string_pretty(raw_output).unwrap_or_else(|_| raw_output.to_string());
        self.upsert_tool_result(session, call_id, pretty, is_error);
    }

    fn is_hidden(&self, session: Entity, call_id: &str) -> bool {
        self.hidden
            .iter()
            .any(|(_, parent, id, _, _)| parent.parent() == session && id.0 == call_id)
    }

    fn update_hidden(
        &mut self,
        session: Entity,
        call_id: &str,
        title: &str,
        raw_input: Option<&serde_json::Value>,
        finished: bool,
    ) {
        for (entity, parent, id, mut name, mut input) in &mut self.hidden {
            if parent.parent() != session || id.0 != call_id {
                continue;
            }
            if finished {
                self.commands.entity(entity).despawn();
            } else {
                if !title.is_empty() {
                    name.set(title.to_string());
                }
                if let Some(raw_input) = raw_input {
                    input.0 = raw_input.to_string();
                }
            }
            return;
        }
        if !finished {
            self.commands.spawn((
                AcpHiddenToolCall,
                AcpToolCallId(call_id.to_string()),
                Name::new(title.to_string()),
                AcpToolInput(AcpToolInput::from_raw(raw_input)),
                ChildOf(session),
            ));
        }
    }

    fn clear(&mut self, session: Entity) {
        for (entity, parent, _, _) in &self.entries {
            if parent.parent() == session {
                self.commands.entity(entity).despawn();
            }
        }
        for (entity, parent, _, _, _) in &self.hidden {
            if parent.parent() == session {
                self.commands.entity(entity).despawn();
            }
        }
    }
}

#[derive(SystemParam)]
struct FileTouchTracker<'w, 's> {
    active: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Order,
            &'static AcpToolCallId,
            &'static mut AcpFileTouch,
        ),
    >,
    finalized: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Order,
            &'static AcpToolCallId,
        ),
        With<AcpFinalizedFileTouch>,
    >,
    commands: Commands<'w, 's>,
}

impl FileTouchTracker<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn project(
        &mut self,
        session: Entity,
        call_id: &str,
        kind: Option<ToolKind>,
        locations: Option<&[ToolCallLocation]>,
        status: Option<ToolCallStatus>,
        full_update: bool,
        order: Order,
    ) -> Vec<(String, Option<u32>, FileTouchKind)> {
        if self
            .finalized
            .iter()
            .any(|(_, parent, _, id)| parent.parent() == session && id.0 == call_id)
        {
            return Vec::new();
        }
        let existing = self.active.iter().find_map(|(entity, parent, _, id, _)| {
            (parent.parent() == session && id.0 == call_id).then_some(entity)
        });
        let should_track = existing.is_some()
            || kind.is_some_and(|kind| AcpFileTouch::file_touch_kind(kind).is_some())
            || locations.is_some_and(|locations| !locations.is_empty());
        if !should_track {
            return Vec::new();
        }
        let (emitted, finalized) = if let Some(entity) = existing {
            let Ok((_, _, _, _, mut state)) = self.active.get_mut(entity) else {
                return Vec::new();
            };
            let result = state.update(kind, locations, status, full_update);
            if result.1 {
                self.commands.entity(entity).despawn();
            }
            result
        } else {
            let mut state = AcpFileTouch {
                kind: ToolKind::default(),
                locations: Vec::new(),
                has_pending_edit: false,
            };
            let result = state.update(kind, locations, status, full_update);
            if !result.1 {
                self.evict_oldest_active(session);
                self.commands.spawn((
                    state,
                    AcpToolCallId(call_id.to_string()),
                    order,
                    ChildOf(session),
                ));
            }
            result
        };
        if finalized {
            self.evict_oldest_finalized(session);
            self.commands.spawn((
                AcpFinalizedFileTouch,
                AcpToolCallId(call_id.to_string()),
                order,
                ChildOf(session),
            ));
        }
        emitted
    }

    fn evict_oldest_active(&mut self, session: Entity) {
        let mut count = 0usize;
        let mut oldest: Option<(u32, Entity)> = None;
        for (entity, parent, order, _, _) in &self.active {
            if parent.parent() != session {
                continue;
            }
            count += 1;
            if oldest.is_none_or(|(best, _)| order.0 < best) {
                oldest = Some((order.0, entity));
            }
        }
        if count >= AcpFileTouch::ACTIVE_LIMIT
            && let Some((_, entity)) = oldest
        {
            self.commands.entity(entity).despawn();
        }
    }

    fn evict_oldest_finalized(&mut self, session: Entity) {
        let mut count = 0usize;
        let mut oldest: Option<(u32, Entity)> = None;
        for (entity, parent, order, _) in &self.finalized {
            if parent.parent() != session {
                continue;
            }
            count += 1;
            if oldest.is_none_or(|(best, _)| order.0 < best) {
                oldest = Some((order.0, entity));
            }
        }
        if count >= AcpFileTouch::FINALIZED_LIMIT
            && let Some((_, entity)) = oldest
        {
            self.commands.entity(entity).despawn();
        }
    }

    fn clear(&mut self, session: Entity) {
        for (entity, parent, _, _, _) in &self.active {
            if parent.parent() == session {
                self.commands.entity(entity).despawn();
            }
        }
        for (entity, parent, _, _) in &self.finalized {
            if parent.parent() == session {
                self.commands.entity(entity).despawn();
            }
        }
    }
}

impl AcpSessionConfigs {
    fn category_name(category: &SessionConfigOptionCategory) -> String {
        match category {
            SessionConfigOptionCategory::Mode => "mode".to_string(),
            SessionConfigOptionCategory::Model => "model".to_string(),
            SessionConfigOptionCategory::ModelConfig => "model_config".to_string(),
            SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
            SessionConfigOptionCategory::Other(value) => value.clone(),
            _ => "unknown".to_string(),
        }
    }

    fn options(options: &SessionConfigSelectOptions) -> Vec<AcpSessionConfigValue> {
        let mut values = Vec::new();
        match options {
            SessionConfigSelectOptions::Ungrouped(options) => {
                for option in options {
                    values.push(AcpSessionConfigValue {
                        value: option.value.to_string(),
                        name: option.name.clone(),
                        description: option.description.clone(),
                        group: None,
                    });
                }
            }
            SessionConfigSelectOptions::Grouped(groups) => {
                for group in groups {
                    for option in &group.options {
                        values.push(AcpSessionConfigValue {
                            value: option.value.to_string(),
                            name: option.name.clone(),
                            description: option.description.clone(),
                            group: Some(group.name.clone()),
                        });
                    }
                }
            }
            _ => {}
        }
        values
    }

    pub(super) fn from_acp(
        config_options: &[SessionConfigOption],
        legacy: Option<&SessionModeState>,
    ) -> Self {
        let mut configs = Vec::new();
        for config in config_options {
            let SessionConfigKind::Select(select) = &config.kind else {
                continue;
            };
            configs.push(AcpSessionConfig {
                config_id: Some(config.id.to_string()),
                name: config.name.clone(),
                description: config.description.clone(),
                category: config.category.as_ref().map(Self::category_name),
                current_value: select.current_value.to_string(),
                values: Self::options(&select.options),
            });
        }
        let has_mode = configs
            .iter()
            .any(|config| config.category.as_deref() == Some("mode"));
        if !has_mode && let Some(legacy) = legacy {
            let mut mode_values = Vec::with_capacity(legacy.available_modes.len());
            for mode in &legacy.available_modes {
                mode_values.push(AcpSessionConfigValue {
                    value: mode.id.to_string(),
                    name: mode.name.clone(),
                    description: mode.description.clone(),
                    group: None,
                });
            }
            configs.push(AcpSessionConfig {
                config_id: None,
                name: "Mode".to_string(),
                description: None,
                category: Some("mode".to_string()),
                current_value: legacy.current_mode_id.to_string(),
                values: mode_values,
            });
        }
        Self(configs)
    }

    fn apply_update(&mut self, update: &SessionUpdate) -> bool {
        match update {
            SessionUpdate::ConfigOptionUpdate(config) => {
                let legacy = self
                    .0
                    .iter()
                    .find(|config| config.config_id.is_none())
                    .cloned();
                let mut next = Self::from_acp(&config.config_options, None);
                if !next
                    .0
                    .iter()
                    .any(|config| config.category.as_deref() == Some("mode"))
                    && let Some(legacy) = legacy
                {
                    next.0.push(legacy);
                }
                if *self == next {
                    return false;
                }
                *self = next;
                true
            }
            SessionUpdate::CurrentModeUpdate(current) => {
                let value = current.current_mode_id.to_string();
                let Some(config) = self.0.iter_mut().find(|config| config.config_id.is_none())
                else {
                    return false;
                };
                if config.current_value == value
                    || !config.values.iter().any(|option| option.value == value)
                {
                    return false;
                }
                config.current_value = value;
                true
            }
            _ => false,
        }
    }
}

fn drain_inboxes(
    sessions: Query<(
        Entity,
        &AsyncChannel<AcpReplayBegin>,
        &AsyncChannel<AcpSessionUpdate>,
        &AsyncChannel<AcpReplayFinish>,
        &AsyncChannel<AcpSnapshotRequest>,
        &AsyncChannel<AcpApprovalDetails>,
    )>,
    mut commands: Commands,
) {
    for (session, begin, update, finish, snapshot, approval) in &sessions {
        let mut next_begin = begin.try_recv();
        let mut next_update = update.try_recv();
        let mut next_finish = finish.try_recv();
        let mut next_snapshot = snapshot.try_recv();
        let mut next_approval = approval.try_recv();
        loop {
            let min_seq = [
                next_begin.map(|item| item.0),
                next_update.as_ref().map(|item| item.0),
                next_finish.map(|item| item.0),
                next_snapshot.map(|item| item.0),
                next_approval.as_ref().map(|item| item.0),
            ]
            .into_iter()
            .flatten()
            .min();
            let Some(seq) = min_seq else {
                break;
            };
            if next_begin.is_some_and(|item| item.0 == seq) {
                next_begin = begin.try_recv();
                commands.trigger(BeginHistoryReplay { session });
            } else if next_update.as_ref().is_some_and(|item| item.0 == seq) {
                let AcpSessionUpdate(_, payload) = next_update.take().unwrap();
                next_update = update.try_recv();
                commands.trigger(ApplySessionUpdate {
                    session,
                    update: payload,
                });
            } else if next_finish.is_some_and(|item| item.0 == seq) {
                let AcpReplayFinish(_, loaded) = next_finish.take().unwrap();
                next_finish = finish.try_recv();
                commands.trigger(FinishHistoryReplay { session, loaded });
            } else if next_snapshot.is_some_and(|item| item.0 == seq) {
                next_snapshot = snapshot.try_recv();
                commands.trigger(EmitTranscriptSnapshot { session });
            } else if next_approval.as_ref().is_some_and(|item| item.0 == seq) {
                let AcpApprovalDetails(_, request) = next_approval.take().unwrap();
                next_approval = approval.try_recv();
                commands.trigger(ResolveApprovalDetails { session, request });
            }
        }
    }
}

fn begin_history_replay(
    trigger: On<BeginHistoryReplay>,
    mut sessions: Query<&mut AcpHistoryReplay>,
    mut transcript: TranscriptWriter,
    mut touches: FileTouchTracker,
) {
    let session = trigger.event().session;
    transcript.clear(session);
    touches.clear(session);
    if let Ok(mut replay) = sessions.get_mut(session) {
        replay.active = true;
    }
}

fn finish_history_replay(
    trigger: On<FinishHistoryReplay>,
    mut sessions: Query<&mut AcpHistoryReplay>,
    mut transcript: TranscriptWriter,
    mut touches: FileTouchTracker,
    mut commands: Commands,
) {
    let session = trigger.event().session;
    let loaded = trigger.event().loaded;
    if !loaded {
        transcript.clear(session);
        touches.clear(session);
    }
    if let Ok(mut replay) = sessions.get_mut(session) {
        replay.active = false;
    }
    commands.trigger(EmitTranscriptSnapshot { session });
}

fn apply_session_update(
    trigger: On<ApplySessionUpdate>,
    mut sessions: Query<(
        &SessionId,
        &AcpSessionShared,
        &AcpHistoryReplay,
        &mut AcpSessionConfigs,
        Option<&mut AcpSessionCwd>,
    )>,
    mut transcript: TranscriptWriter,
    mut touches: FileTouchTracker,
    mut commands: Commands,
) {
    let session = trigger.event().session;
    let Ok((sid, shared, replay, mut configs, mut session_cwd)) = sessions.get_mut(session) else {
        return;
    };
    let update = trigger.event().update.as_ref();
    if configs.apply_update(update) {
        shared.0.emit(ServiceMessage::AcpSessionConfigState {
            sid: sid.0.clone(),
            configs: configs.0.clone(),
        });
    }
    let replay_active = replay.active;
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                transcript.append_assistant_text(session, &text.text);
                if !replay_active {
                    shared
                        .0
                        .emit(ServiceMessage::Shared(SharedEvent::AgentDelta {
                            sid: sid.0.clone(),
                            text: text.text.clone(),
                        }));
                    commands.trigger(EmitTranscriptSnapshot { session });
                }
            }
        }
        SessionUpdate::UserMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                transcript.append_user_chunk(session, &text.text);
                if !replay_active {
                    commands.trigger(EmitTranscriptSnapshot { session });
                }
            }
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                transcript.append_thinking(session, &text.text);
                if !replay_active {
                    commands.trigger(EmitTranscriptSnapshot { session });
                }
            }
        }
        SessionUpdate::Plan(plan) => {
            transcript.upsert_plan(session, plan.clone());
            if !replay_active {
                commands.trigger(EmitTranscriptSnapshot { session });
            }
        }
        SessionUpdate::ToolCall(tc) => {
            transcript.apply_tool_call(session, tc, replay_active, &mut touches);
        }
        SessionUpdate::ToolCallUpdate(update) => {
            transcript.apply_tool_call_update(session, update, replay_active, &mut touches);
        }
        SessionUpdate::SessionInfoUpdate(info) => {
            if let Some(worktree) = info.meta.as_ref().and_then(|meta| meta.get("worktree"))
                && let (Some(name), Some(branch), Some(cwd), Some(workspace_cwd)) = (
                    worktree.get("name").and_then(|value| value.as_str()),
                    worktree.get("branch").and_then(|value| value.as_str()),
                    worktree.get("cwd").and_then(|value| value.as_str()),
                    worktree
                        .get("workspaceCwd")
                        .and_then(|value| value.as_str()),
                )
                && let Ok(workspace) =
                    WorkspaceLocation::new(name.to_string(), branch.to_string(), cwd, workspace_cwd)
            {
                if let Some(session_cwd) = session_cwd.as_deref_mut() {
                    session_cwd.0 = workspace.working_directory.clone();
                }
                shared.0.publish_workspace_change(&workspace);
            }
        }
        _ => {}
    }
    shared
        .0
        .projector_updates
        .send_modify(|revision| *revision += 1);
}

fn emit_transcript_snapshot(
    trigger: On<EmitTranscriptSnapshot>,
    sessions: Query<&AcpSessionShared>,
    transcripts: AcpTranscripts,
) {
    let session = trigger.event().session;
    let Ok(shared) = sessions.get(session) else {
        return;
    };
    let messages = transcripts.messages(session);
    shared.0.emit(shared.0.snapshot_message(&messages));
}

fn emit_proposed_diff(
    trigger: On<EmitProposedDiff>,
    sessions: Query<(&SessionId, &AcpSessionShared)>,
) {
    let event = trigger.event();
    let Ok((sid, shared)) = sessions.get(event.session) else {
        return;
    };
    shared.0.emit(ServiceMessage::AcpProposedDiff {
        sid: sid.0.clone(),
        call_id: event.call_id.clone(),
        path: event.path.clone(),
        old_text: event.old_text.clone(),
        new_text: event.new_text.clone(),
    });
}

fn emit_file_touched(trigger: On<EmitFileTouched>, sessions: Query<&AcpSessionShared>) {
    let event = trigger.event();
    let Ok(shared) = sessions.get(event.session) else {
        return;
    };
    let Ok(request) = AgentRequest::encode(&AgentFileTouched {
        anchor: shared.0.anchor,
        path: event.path.clone(),
        line: event.line,
        col: None,
        end_col: None,
        kind: event.kind,
    }) else {
        return;
    };
    shared.0.emit(ServiceMessage::AgentRequest {
        request_id: AgentRequestId::new(),
        anchor: Some(shared.0.anchor),
        request,
    });
}

fn resolve_approval_details(
    mut trigger: On<ResolveApprovalDetails>,
    hidden: Query<(&ChildOf, &AcpToolCallId, &Name, &AcpToolInput), With<AcpHiddenToolCall>>,
    entries: Query<(&ChildOf, &AcpMessage), With<AcpTranscriptEntry>>,
) {
    let session = trigger.event().session;
    let query = &trigger.event().request.query;
    let mut projected = None;
    for (parent, id, name, input) in &hidden {
        if parent.parent() == session && id.0 == query.call_id {
            projected = Some((name.as_str().to_string(), input.0.clone()));
            break;
        }
    }
    if projected.is_none() {
        'entries: for (parent, message) in &entries {
            if parent.parent() != session {
                continue;
            }
            let Message::Assistant { blocks } = &message.0 else {
                continue;
            };
            for block in blocks {
                match block {
                    AssistantBlock::ToolUse {
                        call_id,
                        name,
                        args,
                        ..
                    } if call_id == &query.call_id => {
                        projected = Some((name.clone(), args.clone()));
                        break 'entries;
                    }
                    AssistantBlock::Subagent(subagent) if subagent.call_id == query.call_id => {
                        projected = Some((subagent.title.clone(), subagent.raw_input.clone()));
                        break 'entries;
                    }
                    _ => {}
                }
            }
        }
    }
    let resolved = query.resolve(projected);
    let response = std::mem::replace(
        &mut trigger.event_mut().request.response,
        oneshot::channel().0,
    );
    let _ = response.send(resolved);
}

fn record_user_message(
    trigger: On<RecordUserMessage>,
    mut transcript: TranscriptWriter,
    mut commands: Commands,
) {
    let event = trigger.event();
    transcript.spawn_entry(
        event.session,
        Message::user_with_attachments(event.text.clone(), event.attachments.clone()),
    );
    commands.trigger(EmitTranscriptSnapshot {
        session: event.session,
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use agent_client_protocol::schema::v1::{
        Content, ContentChunk, Diff, Plan, PlanEntry, PlanEntryPriority, SessionInfoUpdate,
        SessionUpdate, Terminal, TextContent, ToolCall, ToolCallContent, ToolCallUpdate,
        ToolCallUpdateFields,
    };
    use bevy::ecs::system::RunSystemOnce;
    use tokio::sync::{broadcast, mpsc};
    use vmux_ecs::ProcessId;
    use vmux_process::ProcessRuntime;

    use super::super::driver::{AcpProjectionSenders, AcpShared};
    use super::*;

    struct ProjectionFixture {
        app: App,
        session: Entity,
        projection: AcpProjectionSenders,
        stream: broadcast::Receiver<ServiceMessage>,
    }

    impl ProjectionFixture {
        fn new() -> Self {
            let (stream_tx, stream) = broadcast::channel(64);
            let (wake, _) = mpsc::unbounded_channel();
            let (processes, _process_runtime) = ProcessRuntime::new(wake.clone());
            let (projection, projection_inboxes) =
                AcpProjectionSenders::open_with_waker(super::super::AcpWake::waker(wake));
            let shared = Arc::new(AcpShared::new(
                "s1".into(),
                PathBuf::from("/tmp"),
                ProcessId::new(),
                stream_tx,
                processes,
            ));
            let mut app = App::new();
            app.add_plugins(AcpProjectionPlugin);
            let session = app
                .world_mut()
                .spawn((
                    SessionId("s1".into()),
                    AcpSessionShared(shared),
                    AcpSessionCwd(PathBuf::from("/tmp")),
                    AcpProjectionOrder::default(),
                    projection_inboxes,
                    AcpSessionConfigs::default(),
                    AcpHistoryReplay::default(),
                ))
                .id();
            Self {
                app,
                session,
                projection,
                stream,
            }
        }

        fn apply(&mut self, update: SessionUpdate) -> Vec<ServiceMessage> {
            self.projection.session_update(update);
            self.app.update();
            let mut emitted = Vec::new();
            while let Ok(message) = self.stream.try_recv() {
                emitted.push(message);
            }
            emitted
        }

        fn push_user(&mut self, text: &str, attachments: Vec<AgentAttachment>) {
            self.app.world_mut().trigger(RecordUserMessage {
                session: self.session,
                text: text.to_string(),
                attachments,
            });
            while self.stream.try_recv().is_ok() {}
        }

        fn messages(&mut self) -> Vec<Message> {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |transcripts: AcpTranscripts| transcripts.messages(session))
                .unwrap()
        }

        fn approval_details(&mut self, query: ApprovalDetailsQuery) -> Option<(String, String)> {
            let (response, mut receiver) = oneshot::channel();
            self.projection.approval_details(query, response);
            self.app.update();
            receiver.try_recv().unwrap()
        }

        fn active_file_touches(&mut self) -> Vec<String> {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(
                    move |query: Query<(&ChildOf, &AcpToolCallId), With<AcpFileTouch>>| {
                        let mut ids = Vec::new();
                        for (parent, id) in &query {
                            if parent.parent() == session {
                                ids.push(id.0.clone());
                            }
                        }
                        ids
                    },
                )
                .unwrap()
        }

        fn finalized_file_touch_count(&mut self) -> usize {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |query: Query<&ChildOf, With<AcpFinalizedFileTouch>>| {
                    query
                        .iter()
                        .filter(|parent| parent.parent() == session)
                        .count()
                })
                .unwrap()
        }

        fn file_touches(messages: &[ServiceMessage]) -> Vec<AgentFileTouched> {
            let mut touches = Vec::new();
            for message in messages {
                if let ServiceMessage::AgentRequest { request, .. } = message
                    && let Ok(Some(touched)) = request.decode::<AgentFileTouched>()
                {
                    touches.push(touched);
                }
            }
            touches
        }

        fn chunk(text: &str) -> SessionUpdate {
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text),
            )))
        }

        fn thought(text: &str) -> SessionUpdate {
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text),
            )))
        }
    }

    #[test]
    fn tool_kind_survives_title_changes_and_updates_without_kind_metadata() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("typed", "Inspect configuration").kind(ToolKind::Read),
        ));
        fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "typed",
            ToolCallUpdateFields::default().title("Execute command"),
        )));
        let messages = fixture.messages();
        let Some(Message::Assistant { blocks }) = messages.first() else {
            panic!("assistant tool");
        };
        assert!(
            matches!(blocks.first(), Some(AssistantBlock::ToolUse { kind: Some(vmux_api::session::ToolKind::Read), name, .. }) if name == "Execute command")
        );
    }

    #[test]
    fn session_info_worktree_metadata_emits_workspace_change() {
        let project = tempfile::tempdir().unwrap();
        let working = project.path().join("quiet-amber-wolf");
        std::fs::create_dir(&working).unwrap();
        let working_path = working.to_string_lossy().into_owned();
        let project_path = project.path().to_string_lossy().into_owned();
        let mut meta = serde_json::Map::new();
        meta.insert(
            "worktree".to_string(),
            serde_json::json!({
                "name": "quiet-amber-wolf",
                "branch": "vibe/quiet-amber-wolf",
                "cwd": working_path,
                "workspaceCwd": project_path
            }),
        );
        let mut fixture = ProjectionFixture::new();

        let emitted = fixture.apply(SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new().meta(meta),
        ));

        assert!(emitted.iter().any(|message| matches!(
            message,
            ServiceMessage::Shared(SharedEvent::AcpWorkspaceChanged { name, branch, cwd, workspace_cwd, .. })
                if name == "quiet-amber-wolf"
                    && branch == "vibe/quiet-amber-wolf"
                    && cwd == &working.canonicalize().unwrap().to_string_lossy()
                    && workspace_cwd == &project.path().canonicalize().unwrap().to_string_lossy()
        )));
    }

    #[test]
    fn session_info_rejects_relative_worktree_paths() {
        let project = tempfile::tempdir().unwrap();
        let mut meta = serde_json::Map::new();
        meta.insert(
            "worktree".to_string(),
            serde_json::json!({
                "name": "quiet-amber-wolf",
                "branch": "vibe/quiet-amber-wolf",
                "cwd": ".",
                "workspaceCwd": project.path().to_string_lossy()
            }),
        );
        let mut fixture = ProjectionFixture::new();

        let emitted = fixture.apply(SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new().meta(meta),
        ));

        assert!(emitted.is_empty());
    }

    #[test]
    fn message_chunks_accumulate_into_one_assistant_message() {
        let mut fixture = ProjectionFixture::new();
        let first = fixture.apply(ProjectionFixture::chunk("Hel"));
        let second = fixture.apply(ProjectionFixture::chunk("lo"));
        assert!(matches!(
            first.first(),
            Some(ServiceMessage::Shared(SharedEvent::AgentDelta { text, .. })) if text == "Hel"
        ));
        assert!(matches!(
            second.first(),
            Some(ServiceMessage::Shared(SharedEvent::AgentDelta { text, .. })) if text == "lo"
        ));
        let messages = fixture.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0],
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("Hello".to_string())],
            }
        );
    }

    #[test]
    fn push_user_records_a_turn_before_following_assistant_text() {
        let mut fixture = ProjectionFixture::new();
        let attachment = AgentAttachment {
            path: "/tmp/image.png".into(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            size: 3,
        };
        fixture.push_user("hi", vec![attachment.clone()]);
        fixture.apply(ProjectionFixture::chunk("hello"));
        let messages = fixture.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0],
            Message::User {
                text: "hi".to_string(),
                attachments: vec![attachment],
            }
        );
        assert_eq!(
            messages[1],
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("hello".to_string())],
            }
        );
    }

    #[test]
    fn tool_call_with_diff_emits_proposed_diff_and_records_block() {
        let mut fixture = ProjectionFixture::new();
        let tc = ToolCall::new("c1", "Edit file").content(vec![ToolCallContent::Diff(
            Diff::new("/tmp/a.rs", "b").old_text("a"),
        )]);
        let emitted = fixture.apply(SessionUpdate::ToolCall(tc));
        assert!(emitted.iter().any(|message| matches!(
            message,
            ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { .. })
        )));
        assert!(emitted.iter().any(|message| matches!(
            message,
            ServiceMessage::AcpProposedDiff { call_id, path, old_text, new_text, .. }
                if call_id == "c1"
                    && path == "/tmp/a.rs"
                    && old_text.as_deref() == Some("a")
                    && new_text == "b"
        )));
        let messages = fixture.messages();
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            Message::Assistant { blocks } => assert!(matches!(
                blocks.first(),
                Some(AssistantBlock::ToolUse { call_id, .. }) if call_id == "c1"
            )),
            other => panic!("expected assistant message, got {other:?}"),
        }
    }

    #[test]
    fn edit_diff_without_locations_emits_and_retries_file_touch() {
        let mut fixture = ProjectionFixture::new();
        let started = fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Editing files")
                .kind(ToolKind::Edit)
                .content(vec![ToolCallContent::Diff(Diff::new(
                    "/repo/src/main.rs",
                    "new",
                ))]),
        ));
        let completed = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        for emitted in [started, completed] {
            let touches = ProjectionFixture::file_touches(&emitted);
            assert!(touches.iter().any(|touch| {
                touch.path == "/repo/src/main.rs"
                    && touch.line.is_none()
                    && touch.kind == FileTouchKind::Edit
            }));
        }
    }

    #[test]
    fn tool_call_details_returns_projected_title_and_input() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "vmux.run")
                .raw_input(serde_json::json!({"command": "echo hi", "focus": true})),
        ));
        let request = RequestPermissionRequest::new(
            "session-1",
            ToolCallUpdate::new("c1", ToolCallUpdateFields::new()),
            Vec::new(),
        );

        assert_eq!(
            fixture.approval_details(ApprovalDetailsQuery::from_request(&request)),
            Some((
                "vmux.run".to_string(),
                r#"{"command":"echo hi","focus":true}"#.to_string(),
            ))
        );
    }

    #[test]
    fn conversation_title_tool_stays_out_of_transcript() {
        let mut fixture = ProjectionFixture::new();
        let started = fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("title-1", "mcp__vmux__set_conversation_title")
                .raw_input(serde_json::json!({"title": "Paris Izakaya Website"})),
        ));
        let request = RequestPermissionRequest::new(
            "session-1",
            ToolCallUpdate::new("title-1", ToolCallUpdateFields::new()),
            Vec::new(),
        );
        assert_eq!(
            fixture.approval_details(ApprovalDetailsQuery::from_request(&request)),
            Some((
                "mcp__vmux__set_conversation_title".to_string(),
                r#"{"title":"Paris Izakaya Website"}"#.to_string(),
            ))
        );
        let completed = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "title-1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        assert!(started.is_empty());
        assert!(completed.is_empty());
        assert!(fixture.messages().is_empty());
        assert_eq!(
            fixture.approval_details(ApprovalDetailsQuery::from_request(&request)),
            None
        );
    }

    #[test]
    fn conversation_title_tool_recognizes_agent_identifier_variants() {
        for title in [
            "mcp__vmux__set_conversation_title",
            "mcp.vmux.set_conversation_title",
            "mcp vmux set conversation title",
            "mcp-vmux-set-conversation-title",
            "mcp:vmux:set:conversation:title",
            "mcp__vmux.set-conversation title",
        ] {
            assert!(AcpToolCallId::is_conversation_title(title));
        }
        assert!(!AcpToolCallId::is_conversation_title(
            "mcp__other__set_conversation_title"
        ));
        assert!(!AcpToolCallId::is_conversation_title(
            "set_conversation_title"
        ));
    }

    #[test]
    fn read_tool_call_locations_emit_file_touched() {
        let mut fixture = ProjectionFixture::new();
        let tc = ToolCall::new("c1", "Read file")
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new("/repo/src/main.rs")]);
        let emitted = fixture.apply(SessionUpdate::ToolCall(tc));
        assert!(
            ProjectionFixture::file_touches(&emitted)
                .iter()
                .any(|touch| {
                    touch.path == "/repo/src/main.rs"
                        && touch.line.is_none()
                        && touch.kind == FileTouchKind::Read
                })
        );
    }

    #[test]
    fn completed_edit_retries_file_touch_from_initial_tool_call() {
        let mut fixture = ProjectionFixture::new();
        let tc = ToolCall::new("c1", "Write file")
            .kind(ToolKind::Edit)
            .locations(vec![ToolCallLocation::new("/repo/new.rs")]);
        fixture.apply(SessionUpdate::ToolCall(tc));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        assert!(
            ProjectionFixture::file_touches(&emitted)
                .iter()
                .any(|touch| {
                    touch.path == "/repo/new.rs"
                        && touch.line.is_none()
                        && touch.kind == FileTouchKind::Edit
                })
        );
    }

    #[test]
    fn failed_edit_clears_pending_and_suppresses_future_touches() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        ));

        let failed = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Failed),
        )));
        let completed = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert!(ProjectionFixture::file_touches(&failed).is_empty());
        assert!(ProjectionFixture::file_touches(&completed).is_empty());
        assert!(!fixture.active_file_touches().iter().any(|id| id == "c1"));
    }

    #[test]
    fn locations_only_update_uses_initial_edit_kind() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file").kind(ToolKind::Edit),
        ));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::InProgress)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert!(
            ProjectionFixture::file_touches(&emitted)
                .iter()
                .any(|touch| {
                    touch.path == "/repo/new.rs"
                        && touch.line.is_none()
                        && touch.kind == FileTouchKind::Edit
                })
        );
    }

    #[test]
    fn kind_only_update_uses_initial_locations() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        ));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::InProgress)
                .kind(ToolKind::Edit),
        )));

        assert!(
            ProjectionFixture::file_touches(&emitted)
                .iter()
                .any(|touch| {
                    touch.path == "/repo/new.rs"
                        && touch.line.is_none()
                        && touch.kind == FileTouchKind::Edit
                })
        );
    }

    #[test]
    fn completion_with_explicit_locations_uses_replacement() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write files")
                .kind(ToolKind::Edit)
                .locations(vec![
                    ToolCallLocation::new("/repo/a.rs"),
                    ToolCallLocation::new("/repo/b.rs"),
                ]),
        ));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/b.rs")]),
        )));
        let touches: Vec<String> = ProjectionFixture::file_touches(&emitted)
            .into_iter()
            .map(|touch| touch.path)
            .collect();

        assert_eq!(touches, vec!["/repo/b.rs"]);
    }

    #[test]
    fn completion_reclassification_does_not_replay_initial_edit() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/old.rs")]),
        ));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));
        let touches = ProjectionFixture::file_touches(&emitted);

        assert_eq!(touches.len(), 1);
        assert_eq!(touches[0].path, "/repo/new.rs");
        assert_eq!(touches[0].line, None);
        assert_eq!(touches[0].kind, FileTouchKind::Read);
    }

    #[test]
    fn repeated_completion_emits_no_duplicate_file_touch() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Write file")
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        ));
        fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Edit)
                .locations(vec![ToolCallLocation::new("/repo/new.rs")]),
        )));

        assert!(ProjectionFixture::file_touches(&emitted).is_empty());
        assert!(!fixture.active_file_touches().iter().any(|id| id == "c1"));
    }

    #[test]
    fn read_completion_with_unchanged_identity_emits_no_duplicate_touch() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(
            ToolCall::new("c1", "Read file")
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("/repo/file.rs")]),
        ));

        let emitted = fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("/repo/file.rs")]),
        )));

        assert!(ProjectionFixture::file_touches(&emitted).is_empty());
    }

    #[test]
    fn finalized_file_touch_tombstones_are_bounded() {
        let mut fixture = ProjectionFixture::new();
        for index in 0..1025 {
            fixture.apply(SessionUpdate::ToolCall(
                ToolCall::new(format!("c{index}"), "Read file")
                    .kind(ToolKind::Read)
                    .status(ToolCallStatus::Completed)
                    .locations(vec![ToolCallLocation::new(format!("/repo/{index}.rs"))]),
            ));
        }

        assert!(fixture.finalized_file_touch_count() <= 1024);
    }

    #[test]
    fn in_progress_file_touches_are_bounded() {
        let mut fixture = ProjectionFixture::new();
        for index in 0..1025 {
            fixture.apply(SessionUpdate::ToolCall(
                ToolCall::new(format!("c{index}"), "Read file")
                    .kind(ToolKind::Read)
                    .locations(vec![ToolCallLocation::new(format!("/repo/{index}.rs"))]),
            ));
        }

        let active = fixture.active_file_touches();
        assert!(active.len() <= 1024);
        assert!(!active.iter().any(|id| id == "c0"));
        assert!(active.iter().any(|id| id == "c1024"));
    }

    #[test]
    fn non_file_tool_call_emits_no_file_touched() {
        let mut fixture = ProjectionFixture::new();
        let tc = ToolCall::new("c1", "run a command")
            .kind(ToolKind::Execute)
            .locations(vec![ToolCallLocation::new("/repo/x")]);
        let emitted = fixture.apply(SessionUpdate::ToolCall(tc));
        assert!(ProjectionFixture::file_touches(&emitted).is_empty());
    }

    #[test]
    fn thought_chunks_accumulate_into_a_thinking_block() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(ProjectionFixture::thought("plan"));
        fixture.apply(ProjectionFixture::thought("ning"));
        let messages = fixture.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0],
            Message::Assistant {
                blocks: vec![AssistantBlock::Thinking("planning".to_string())],
            }
        );
    }

    #[test]
    fn plan_update_replaces_the_single_plan_block() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
            "step one",
            PlanEntryPriority::High,
            PlanEntryStatus::Pending,
        )])));
        fixture.apply(SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
            "step one",
            PlanEntryPriority::High,
            PlanEntryStatus::Completed,
        )])));
        let messages = fixture.messages();
        let blocks = match &messages[0] {
            Message::Assistant { blocks } => blocks,
            other => panic!("expected assistant, got {other:?}"),
        };
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AssistantBlock::Plan { steps } => {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].content, "step one");
                assert_eq!(steps[0].status, "completed");
            }
            other => panic!("expected plan, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_update_content_becomes_a_tool_result() {
        let mut fixture = ProjectionFixture::new();
        fixture.apply(SessionUpdate::ToolCall(ToolCall::new("c1", "run")));
        let fields = ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new("hello output")),
            ))]);
        fixture.apply(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "c1", fields,
        )));
        assert!(fixture.messages().iter().any(|m| matches!(
            m,
            Message::ToolResult { call_id, content, is_error: false }
                if call_id == "c1" && content == "hello output"
        )));
    }

    #[test]
    fn tool_call_with_terminal_folds_to_pane_pointer_result() {
        let mut fixture = ProjectionFixture::new();
        let tc = ToolCall::new("c1", "Run")
            .content(vec![ToolCallContent::Terminal(Terminal::new("t1"))]);
        fixture.apply(SessionUpdate::ToolCall(tc));
        assert!(fixture.messages().iter().any(|m| matches!(
            m,
            Message::ToolResult { call_id, content, .. }
                if call_id == "c1" && content.contains("pane")
        )));
    }

    #[test]
    fn tool_call_with_terminal_and_text_prefers_text_output() {
        let mut fixture = ProjectionFixture::new();
        let tc = ToolCall::new("c1", "Run").content(vec![
            ToolCallContent::Terminal(Terminal::new("t1")),
            ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
                "real output",
            )))),
        ]);
        fixture.apply(SessionUpdate::ToolCall(tc));
        assert!(fixture.messages().iter().any(|m| matches!(
            m,
            Message::ToolResult { call_id, content, .. }
                if call_id == "c1" && content == "real output"
        )));
    }
}
