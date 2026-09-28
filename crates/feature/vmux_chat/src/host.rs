use bevy_ecs::prelude::*;

use crate::composer::ComposerState;
use crate::event::{
    CHAT_HISTORY_MAX_PAGE_SIZE, CHAT_HISTORY_PAGE_SIZE, ChatAttachment, ChatBranch,
    ChatBranchesState, ChatItem, ChatMediaState, ChatResumeState, ChatSnapshot,
    ChatTranscriptState, ComposerContext, ResumableSessions,
};

type ChatUiStateUpdates = vmux_core::host::UiState<crate::state::ChatUiState>;

#[derive(Component)]
#[require(
    ChatUiStateUpdates,
    ChatAttachmentProjection,
    ChatSnapshotProjection,
    ChatMediaProjection,
    ChatComposerContext,
    ComposerState,
    crate::key::ChatKeyEffectRevision
)]
pub struct ChatView;

#[derive(Component, Default)]
pub struct ChatComposerContext(pub ComposerContext);

#[derive(Component, Default)]
pub struct ChatSnapshotProjection(pub ChatSnapshot);

#[derive(Component, Default)]
pub struct ChatAttachmentProjection {
    pub selected: Vec<ChatAttachment>,
    pub previews: std::collections::HashMap<String, ChatAttachment>,
    pub pending: std::collections::HashSet<String>,
    pub resolved: std::collections::HashSet<String>,
}

#[derive(Component, Default)]
pub struct ChatMediaProjection(pub ChatMediaState);

#[derive(Component, Default)]
pub struct ChatTranscriptProjection {
    pub state: ChatTranscriptState,
    pub tail: Vec<ChatItem>,
    pub tail_start: u32,
}

pub struct TranscriptTail {
    pub items: Vec<ChatItem>,
    pub start: u32,
    pub total: u32,
}

pub struct TranscriptPage {
    pub items: Vec<ChatItem>,
    pub start: u32,
    pub end: u32,
    pub total: u32,
}

#[derive(Component)]
pub struct ChatHistoryQuery {
    pub webview: Entity,
    pub session: Entity,
    pub generation: u64,
    pub request_id: u64,
    pub before: u32,
    pub limit: u32,
}

#[derive(Component)]
pub struct ChatHistoryResult {
    pub webview: Entity,
    pub generation: u64,
    pub request_id: u64,
    pub page: Option<TranscriptPage>,
}

impl ChatTranscriptProjection {
    pub fn merge_tail(&mut self, tail: TranscriptTail) -> bool {
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
        self.refresh_activity();
        true
    }

    pub fn start_history_query(
        &mut self,
        webview: Entity,
        session: Entity,
    ) -> Option<ChatHistoryQuery> {
        if self.state.loaded_start == 0 || self.state.loading {
            return None;
        }
        self.state.request_id = self.state.request_id.wrapping_add(1).max(1);
        self.state.loading = true;
        Some(ChatHistoryQuery {
            webview,
            session,
            generation: self.state.generation,
            request_id: self.state.request_id,
            before: self.state.loaded_start,
            limit: CHAT_HISTORY_PAGE_SIZE.min(CHAT_HISTORY_MAX_PAGE_SIZE),
        })
    }

    pub fn finish_history_query(&mut self, result: &ChatHistoryResult) -> bool {
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
        self.refresh_activity();
        true
    }

    fn refresh_activity(&mut self) {
        let (subagents, tasks) = vmux_core::chat_projection::activity_counts(&self.state.items);
        self.state.active_subagents = subagents;
        self.state.active_tasks = tasks;
    }
}

#[derive(Component, Default)]
pub struct ChatResumeProjection(pub ChatResumeState);

impl ChatResumeProjection {
    pub fn start(&mut self, active: bool, query: String) -> Option<u64> {
        if self.0.active == active && self.0.query == query {
            return None;
        }
        self.0.request_id = self.0.request_id.wrapping_add(1).max(1);
        self.0.active = active;
        self.0.query = query;
        self.0.sessions.clear();
        self.0.total = 0;
        self.0.loading = active;
        Some(self.0.request_id)
    }

    pub fn finish(&mut self, sessions: &ResumableSessions) -> bool {
        if self.0.request_id != sessions.request_id
            || self.0.query != sessions.query
            || !self.0.active
        {
            return false;
        }
        self.0.sessions.clone_from(&sessions.sessions);
        self.0.total = sessions.total;
        self.0.loading = false;
        true
    }
}

#[derive(Component, Default)]
pub struct ChatBranchesProjection(pub ChatBranchesState);

impl ChatBranchesProjection {
    pub fn start(&mut self, project: String) -> u64 {
        self.0.request_id = self.0.request_id.wrapping_add(1).max(1);
        self.0.project = project;
        self.0.branches.clear();
        self.0.loading = true;
        self.0.request_id
    }

    pub fn finish(&mut self, request_id: u64, project: &str, branches: Vec<ChatBranch>) -> bool {
        if self.0.request_id != request_id || self.0.project != project {
            return false;
        }
        self.0.branches = branches;
        self.0.loading = false;
        true
    }
}

#[derive(Component)]
pub struct ChatSynced;
