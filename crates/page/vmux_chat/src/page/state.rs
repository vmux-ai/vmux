use std::collections::{HashMap, HashSet};

use super::scroll;
use crate::event::{
    ApprovalDecision, CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT,
    CHAT_HISTORY_PAGE_EVENT, CHAT_HISTORY_PAGE_SIZE, CHAT_MEDIA_ENTRIES_EVENT,
    CHAT_PROJECT_BRANCHES_EVENT, CHAT_SNAPSHOT_EVENT, COMPOSER_CONTEXT_EVENT, ChatApproval,
    ChatAttachPaths, ChatAttachment, ChatAttachmentPreviewRequest, ChatAttachments, ChatBranch,
    ChatBranchesRequest, ChatCancel, ChatChoiceSelected, ChatEscape, ChatHistoryPage,
    ChatHistoryRequest, ChatItem, ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest,
    ChatPickFiles, ChatProjectBranches, ChatSnapshot, ChatSubmit, ChatSubmitAttachment,
    ComposerContext, MODEL_STATE_EVENT, ModelOptionEntry, ModelState, QueuedPromptSnapshot,
    RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions, ResumeListRequest,
    ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT, SelectModel, SlashCommandEntry,
    SlashCommands as SlashCommandsEvent, latest_tool_location,
};
use crate::format::composer::{
    ResumeMenuState, SelectorMode, chat_page_title, filter_models, filter_sessions,
    resume_menu_state, selector_mode, should_clear_draft_on_escape, should_fetch_resume,
};
use crate::tab::Accent;
use dioxus::prelude::*;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::composer::{
    PROMPT_INPUT_ID, PromptComposerAction, PromptComposerAttachment, focus_prompt_end,
};
use vmux_ui::components::prompt_media_options::PromptMediaOption;
use vmux_ui::file_icon::FilePath;
use vmux_ui::hooks::{send, use_listener, use_selector, use_theme};
use vmux_ui::i18n::translate;
use vmux_wire::prompt_media::{
    inline_media_query, merge_chat_attachments, replace_inline_media_query,
};

#[derive(Clone, Copy, PartialEq)]
pub struct Chat {
    pub agent: Signal<String>,
    pub transcript: Transcript,
    pub run: RunState,
    pub identity: AgentIdentity,
    pub handoff: Handoff,
    pub composer: ComposerDraft,
    pub queue: PromptQueue,
    pub media: MediaPicker,
    pub models: ModelPicker,
    pub effort: EffortPicker,
    pub projects: ProjectPicker,
    pub slash: SlashCommands,
    pub resume: Resume,
    pub activity_counts: Memo<(usize, usize)>,
    pub latest_tool: Memo<Option<(usize, usize)>>,
}

pub fn use_chat() -> Chat {
    use_theme();
    let transcript = use_transcript();
    let items = transcript.items;
    let chat = Chat {
        agent: use_signal(current_agent),
        transcript,
        run: use_run_state(),
        identity: use_agent_identity(),
        handoff: use_handoff(),
        composer: use_composer_draft(),
        queue: use_prompt_queue(),
        media: use_media_picker(),
        models: use_model_picker(),
        effort: use_effort_picker(),
        projects: use_project_picker(),
        slash: use_slash_commands(),
        resume: use_resume(),
        activity_counts: use_memo(move || vmux_wire::chat::activity_counts(&items.read())),
        latest_tool: use_memo(move || latest_tool_location(&items.read())),
    };
    chat.listen();
    chat.watch();
    chat
}

impl Chat {
    fn listen(&self) {
        let chat = *self;
        let _snapshot = use_listener::<ChatSnapshot, _>(CHAT_SNAPSHOT_EVENT, move |snapshot| {
            chat.apply_snapshot(snapshot);
        });
        let _history = use_listener::<ChatHistoryPage, _>(CHAT_HISTORY_PAGE_EVENT, move |page| {
            chat.apply_history_page(page);
        });
        let _attachments =
            use_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
                let mut attachments = chat.composer.attachments;
                let current = attachments.peek().clone();
                attachments.set(merge_chat_attachments(&current, &selected.attachments));
                focus_prompt_end(PROMPT_INPUT_ID);
            });
        let _previews =
            use_listener::<ChatAttachments, _>(CHAT_ATTACHMENT_PREVIEWS_EVENT, move |loaded| {
                let mut known = chat.composer.attachment_previews;
                let mut previews = known.peek().clone();
                for attachment in &loaded.attachments {
                    previews.insert(attachment.path.clone(), attachment.clone());
                }
                known.set(previews);
            });
        let _media =
            use_listener::<ChatMediaEntries, _>(CHAT_MEDIA_ENTRIES_EVENT, move |response| {
                if response.request_id != (chat.media.request_id)() {
                    return;
                }
                let mut entries = chat.media.entries;
                let mut loading = chat.media.loading;
                let mut menu_sel = chat.slash.menu_sel;
                entries.set(response.entries.clone());
                loading.set(false);
                menu_sel.set(0);
            });
        let _commands =
            use_listener::<SlashCommandsEvent, _>(SLASH_COMMANDS_EVENT, move |incoming| {
                let mut commands = chat.slash.commands;
                commands.set(incoming.commands.clone());
            });
        let _models = use_listener::<ModelState, _>(MODEL_STATE_EVENT, move |state| {
            let mut models = chat.models.models;
            let mut current_model_id = chat.models.current_model_id;
            let mut current_model = chat.models.current_model;
            let mut levels = chat.effort.levels;
            let mut current = chat.effort.current;
            let mut agent_key = chat.effort.agent_key;
            let mut menu_sel = chat.slash.menu_sel;
            models.set(state.models.clone());
            current_model_id.set(state.current_model_id.clone());
            current_model.set(state.current_model_name.clone());
            levels.set(state.effort_levels.clone());
            current.set(state.effort_current.clone());
            agent_key.set(state.agent_key.clone());
            menu_sel.set(0);
        });
        let _context = use_listener::<ComposerContext, _>(COMPOSER_CONTEXT_EVENT, move |context| {
            let mut composer_context = chat.slash.composer_context;
            composer_context.set(context.clone());
        });
        let _branches =
            use_listener::<ChatProjectBranches, _>(CHAT_PROJECT_BRANCHES_EVENT, move |incoming| {
                let mut branches = chat.projects.branches;
                let mut branches_for = chat.projects.branches_for;
                branches.set(incoming.branches.clone());
                branches_for.set(incoming.project.clone());
            });
        let _sessions =
            use_listener::<ResumableSessions, _>(RESUMABLE_SESSIONS_EVENT, move |incoming| {
                let mut sessions = chat.resume.sessions;
                let mut menu_sel = chat.slash.menu_sel;
                let mut loading = chat.resume.loading;
                sessions.set(incoming.sessions.clone());
                menu_sel.set(0);
                loading.set(false);
            });
    }

    fn watch(&self) {
        let chat = *self;
        use_effect(move || focus_prompt_end(PROMPT_INPUT_ID));
        use_effect(move || {
            let _ = chat.transcript.items.read().len();
            let _ = chat.run.status.read();
            if !*chat.transcript.at_bottom.peek() {
                return;
            }
            scroll::to_bottom(chat.transcript.scroll_container);
        });
        use_effect(move || chat.fetch_resume_sessions());
        use_effect(move || chat.fetch_media_entries());
        use_selector(chat.slash.menu_sel, move |selected| {
            let media_open = {
                let draft = chat.composer.draft.read();
                inline_media_query(&draft).is_some()
            };
            let _ = chat.resume.sessions.read().len();
            let _ = chat.models.models.read().len();
            let _ = chat.media.entries.read().len();
            if !chat.run.choice_options.read().is_empty() {
                format!("agent-choice-item-{selected}")
            } else if media_open {
                format!("prompt-media-item-{selected}")
            } else {
                format!("agent-selector-item-{selected}")
            }
        });
    }

    fn apply_snapshot(&self, snapshot: ChatSnapshot) {
        let transcript = self.transcript;
        let messages_changed = (transcript.recent_messages_start)() != snapshot.messages_start
            || *transcript.recent_messages_json.peek() != snapshot.messages_json;
        if messages_changed
            && let Ok(parsed) = serde_json::from_str::<Vec<ChatItem>>(&snapshot.messages_json)
        {
            self.request_attachment_previews(&parsed);
            let mut items = transcript.items;
            let mut recent_json = transcript.recent_messages_json;
            let mut recent_start = transcript.recent_messages_start;
            let start = merge_transcript_page(
                &mut items.write(),
                (transcript.loaded_start)(),
                parsed,
                snapshot.messages_start,
            );
            set_if_changed(transcript.loaded_start, start);
            recent_json.set(snapshot.messages_json.clone());
            recent_start.set(snapshot.messages_start);
            if start == 0 {
                set_if_changed(transcript.history_loading, false);
            }
        }
        set_if_changed(transcript.messages_total, snapshot.messages_total);
        set_if_changed(self.run.status, snapshot.status.clone());
        set_if_changed(self.run.error, snapshot.error.clone());
        set_if_changed(self.queue.queued, snapshot.queued.clone());
        set_if_changed(self.composer.transition_preview, String::new());
        set_if_changed(self.composer.transition_attachments, Vec::new());
        set_if_changed(self.queue.paused, snapshot.paused);
        set_if_changed(self.identity.agent_name, snapshot.agent_name.clone());
        set_if_changed(
            self.identity.conversation_title,
            snapshot.conversation_title.clone(),
        );
        set_if_changed(self.identity.agent_icon, snapshot.agent_icon.clone());
        set_if_changed(self.identity.accent, snapshot.accent_color.clone());
        set_if_changed(self.handoff.source, snapshot.handoff_source.clone());
        set_if_changed(self.handoff.truncated, snapshot.handoff_truncated);
        set_if_changed(self.handoff.message_count, snapshot.handoff_message_count);
        set_if_changed(self.run.choice_question, snapshot.choice_question.clone());
        let mut choice_options = self.run.choice_options;
        if choice_options.peek().as_slice() != snapshot.choice_options.as_slice() {
            set_if_changed(self.slash.menu_sel, 0);
            choice_options.set(snapshot.choice_options.clone());
        }
        let next_approval = if snapshot.status == "awaiting" {
            Some((
                snapshot.approval_call_id.clone(),
                snapshot.approval_name.clone(),
                snapshot.approval_args_json.clone(),
            ))
        } else {
            None
        };
        let mut approval = self.run.approval;
        if approval.peek().ne(&next_approval) {
            approval.set(next_approval);
            set_if_changed(self.run.approval_sel, 0);
        }
    }

    fn apply_history_page(&self, page: ChatHistoryPage) {
        let transcript = self.transcript;
        let mut history_loading = transcript.history_loading;
        history_loading.set(false);
        if page.end != (transcript.loaded_start)() {
            return;
        }
        let Ok(older) = serde_json::from_str::<Vec<ChatItem>>(&page.items_json) else {
            return;
        };
        self.request_attachment_previews(&older);
        let metrics = scroll::metrics(transcript.scroll_container);
        let mut items = transcript.items;
        let mut loaded_start = transcript.loaded_start;
        let mut messages_total = transcript.messages_total;
        drop(items.write().splice(0..0, older));
        loaded_start.set(page.start);
        messages_total.set(page.total);
        if let Some((height, top)) = metrics {
            scroll::restore(transcript.scroll_container, height, top);
        }
    }

    fn request_attachment_previews(&self, items: &[ChatItem]) {
        let previews = self.composer.attachment_previews;
        let mut requests = self.composer.attachment_preview_requests;
        let known = previews.peek().keys().cloned().collect::<HashSet<_>>();
        let mut requested = requests.peek().clone();
        let mut paths = Vec::new();
        for item in items {
            let ChatItem::User { attachments, .. } = item else {
                continue;
            };
            for attachment in attachments {
                if !attachment.mime_type.starts_with("image/")
                    || known.contains(&attachment.path)
                    || !requested.insert(attachment.path.clone())
                {
                    continue;
                }
                paths.push(attachment.path.clone());
            }
        }
        if !paths.is_empty() && send(&ChatAttachmentPreviewRequest { paths }).is_ok() {
            requests.set(requested);
        }
    }

    fn fetch_resume_sessions(&self) {
        let mut requested = self.resume.requested;
        let mut loading = self.resume.loading;
        let should_fetch =
            should_fetch_resume(&(self.composer.draft)(), &self.slash.commands.read());
        if should_fetch && !requested() {
            loading.set(true);
            if send(&ResumeListRequest).is_err() {
                loading.set(false);
            }
            requested.set(true);
        } else if !should_fetch && requested() {
            requested.set(false);
            loading.set(false);
        }
    }

    fn fetch_media_entries(&self) {
        let mut entries = self.media.entries;
        let mut request_id = self.media.request_id;
        let mut requested_query = self.media.requested_query;
        let mut loading = self.media.loading;
        let value = (self.composer.draft)();
        let Some(query) = inline_media_query(&value).map(|query| query.query.to_string()) else {
            entries.set(Vec::new());
            if requested_query.peek().is_some() {
                request_id.set(request_id().wrapping_add(1).max(1));
            }
            requested_query.set(None);
            loading.set(false);
            return;
        };
        if requested_query().as_deref() == Some(query.as_str()) {
            return;
        }
        let next_id = request_id().wrapping_add(1).max(1);
        request_id.set(next_id);
        requested_query.set(Some(query.clone()));
        entries.set(Vec::new());
        loading.set(true);
        if send(&ChatMediaListRequest {
            request_id: next_id,
            query,
        })
        .is_err()
        {
            loading.set(false);
        }
    }

    pub fn request_history(&self) {
        let mut loading = self.transcript.history_loading;
        let before = (self.transcript.loaded_start)();
        if before == 0 || *loading.peek() {
            return;
        }
        if send(&ChatHistoryRequest {
            before,
            limit: CHAT_HISTORY_PAGE_SIZE,
        })
        .is_ok()
        {
            loading.set(true);
        }
    }
}

impl Chat {
    pub fn agent(&self) -> String {
        (self.agent)()
    }

    pub fn header_name(&self) -> String {
        let name = (self.identity.agent_name)();
        if name.is_empty() { self.agent() } else { name }
    }

    pub fn title(&self) -> String {
        chat_page_title(&(self.identity.conversation_title)(), &self.header_name())
    }

    pub fn accent(&self) -> Accent {
        Accent::resolve(
            &(self.identity.accent)(),
            agent_accent(&self.agent()).rain_rgb,
        )
    }

    pub fn status(&self) -> String {
        (self.run.status)()
    }

    pub fn installing(&self) -> bool {
        self.status() == "installing"
    }

    pub fn installing_splash(&self) -> bool {
        self.installing() && self.transcript.items.read().is_empty()
    }

    pub fn install_detail(&self) -> String {
        let detail = (self.run.error)();
        if detail.is_empty() {
            translate("agent-preparing")
        } else {
            detail
        }
    }

    pub fn show_examples(&self) -> bool {
        self.transcript.items.read().is_empty()
            && self.queue.queued.read().is_empty()
            && self.composer.attachments.read().is_empty()
            && self.composer.transition_attachments.read().is_empty()
    }

    pub fn draft(&self) -> String {
        (self.composer.draft)()
    }

    pub fn filtered_commands(&self) -> Vec<SlashCommandEntry> {
        let draft = self.draft();
        let SelectorMode::Commands(query) = selector_mode(&draft) else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        let mut matching = Vec::new();
        for command in self.slash.commands.read().iter() {
            if command.name.starts_with(&query) {
                matching.push(command.clone());
            }
        }
        matching
    }

    pub fn filtered_sessions(&self) -> Vec<ResumableSessionEntry> {
        let draft = self.draft();
        let SelectorMode::Resume(query) = selector_mode(&draft) else {
            return Vec::new();
        };
        filter_sessions(&self.resume.sessions.read(), query)
    }

    pub fn filtered_models(&self) -> Vec<ModelOptionEntry> {
        let draft = self.draft();
        let SelectorMode::Models(query) = selector_mode(&draft) else {
            return Vec::new();
        };
        filter_models(&self.models.models.read(), query)
    }

    pub fn command_menu_open(&self) -> bool {
        !self.filtered_commands().is_empty()
    }

    pub fn resume_menu_open(&self) -> bool {
        matches!(selector_mode(&self.draft()), SelectorMode::Resume(_))
    }

    pub fn model_menu_open(&self) -> bool {
        matches!(selector_mode(&self.draft()), SelectorMode::Models(_))
    }

    pub fn resume_state(&self) -> Option<ResumeMenuState> {
        if !self.resume_menu_open() {
            return None;
        }
        Some(resume_menu_state(
            (self.resume.requested)(),
            (self.resume.loading)(),
            self.resume.sessions.read().len(),
            self.filtered_sessions().len(),
        ))
    }

    pub fn media_menu_open(&self) -> bool {
        inline_media_query(&self.draft()).is_some()
    }

    pub fn media_options(&self) -> Vec<PromptMediaOption> {
        let mut options = Vec::new();
        for entry in self.media.entries.read().iter() {
            options.push(PromptMediaOption {
                key: format!("media-{}", entry.path),
                name: entry.name.clone(),
                display_path: entry.display_path(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FilePath(&entry.name).extension_label(),
                is_dir: entry.is_dir,
            });
        }
        options
    }

    pub fn composer_attachments(&self) -> Vec<PromptComposerAttachment> {
        let previews = self.composer.attachment_previews.read();
        let preview_of = |attachment: &ChatAttachment| {
            let loaded = previews
                .get(&attachment.path)
                .filter(|preview| !preview.preview_data_url.is_empty());
            match loaded {
                Some(preview) => preview.preview_data_url.clone(),
                None => attachment.preview_data_url.clone(),
            }
        };
        let mut pills = Vec::new();
        for attachment in self.composer.transition_attachments.read().iter() {
            pills.push(PromptComposerAttachment {
                key: format!("transition-attachment-{}", attachment.path),
                name: attachment.name.clone(),
                label: FilePath(&attachment.name).extension_label(),
                preview_data_url: preview_of(attachment),
                remove_index: None,
            });
        }
        for (index, attachment) in self.composer.attachments.read().iter().enumerate() {
            pills.push(PromptComposerAttachment {
                key: format!("attachment-pill-{}", attachment.path),
                name: attachment.name.clone(),
                label: FilePath(&attachment.name).extension_label(),
                preview_data_url: preview_of(attachment),
                remove_index: Some(index),
            });
        }
        pills
    }

    pub fn streaming(&self) -> bool {
        matches!(self.status().as_str(), "streaming" | "awaiting")
    }

    pub fn prompt_action(&self) -> PromptComposerAction {
        if self.streaming() && self.queue.queued.read().is_empty() {
            PromptComposerAction::Stop
        } else {
            PromptComposerAction::Send
        }
    }

    pub fn prompt_action_title(&self) -> String {
        if self.streaming() && !self.queue.queued.read().is_empty() {
            translate("agent-send-all-queued")
        } else if self.streaming() {
            translate("common-stop")
        } else {
            translate("agent-send")
        }
    }

    pub fn prompt_action_enabled(&self) -> bool {
        !self.choice_pending()
            && (self.streaming()
                || !self.draft().trim().is_empty()
                || !self.composer.attachments.read().is_empty())
    }

    pub fn choice_pending(&self) -> bool {
        !self.run.choice_options.read().is_empty() || self.run.approval.read().is_some()
    }
}

impl Chat {
    pub fn submit(&self) {
        let mut draft = self.composer.draft;
        let mut attachments = self.composer.attachments;
        let mut history_cursor = self.composer.history_cursor;
        let mut history_scratch = self.composer.history_scratch;
        let mut at_bottom = self.transcript.at_bottom;
        let text = draft.peek().trim().to_string();
        let selected = attachments.peek().clone();
        if text.is_empty() && selected.is_empty() {
            return;
        }
        let mut to_submit = Vec::with_capacity(selected.len());
        for attachment in &selected {
            to_submit.push(ChatSubmitAttachment {
                path: attachment.path.clone(),
                name: attachment.name.clone(),
                mime_type: attachment.mime_type.clone(),
                size: attachment.size,
            });
        }
        if send(&ChatSubmit {
            text,
            attachments: to_submit,
        })
        .is_err()
        {
            return;
        }
        at_bottom.set(true);
        draft.set(String::new());
        attachments.set(Vec::new());
        history_cursor.set(None);
        history_scratch.set(String::new());
    }

    pub fn stop_or_flush(&self) {
        if self.queue.queued.peek().is_empty() {
            let _ = send(&ChatCancel);
        } else {
            let _ = send(&ChatEscape);
        }
    }

    pub fn interrupt(&self) {
        let _ = send(&ChatEscape);
        let mut draft = self.composer.draft;
        if should_clear_draft_on_escape(
            self.streaming(),
            self.queue.queued.peek().is_empty(),
            draft.peek().is_empty(),
        ) {
            draft.set(String::new());
        }
    }

    pub fn cancel(&self) {
        let _ = send(&ChatCancel);
    }

    pub fn run_slash_command(&self, name: &str) {
        let mut draft = self.composer.draft;
        let mut menu_sel = self.slash.menu_sel;
        match name {
            "upload" => {
                let _ = send(&ChatPickFiles);
                draft.set(String::new());
            }
            "resume" => {
                menu_sel.set(0);
                draft.set("/resume ".to_string());
            }
            "model" => {
                menu_sel.set(0);
                draft.set("/model ".to_string());
            }
            "cli" => {
                let _ = send(&RuntimeSwitchRequest { to: "cli".into() });
                draft.set(String::new());
            }
            "acp" => {
                let _ = send(&RuntimeSwitchRequest { to: "acp".into() });
                draft.set(String::new());
            }
            _ => {}
        }
    }

    pub fn select_model(&self, model: &ModelOptionEntry) {
        let mut draft = self.composer.draft;
        let _ = send(&SelectModel {
            model_id: model.id.clone(),
        });
        draft.set(String::new());
    }

    pub fn select_resume_session(&self, session: &ResumableSessionEntry) {
        let mut draft = self.composer.draft;
        let _ = send(&ResumeSession {
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            cwd: session.cwd.clone(),
        });
        draft.set(String::new());
    }

    pub fn select_media_entry(&self, entry: &ChatMediaEntry) {
        let mut draft = self.composer.draft;
        let mut menu_sel = self.slash.menu_sel;
        let value = draft.peek().clone();
        let Some(query) = inline_media_query(&value) else {
            return;
        };
        let reference = entry.reference();
        let replacement = if entry.is_dir {
            format!("@{reference}/")
        } else {
            if send(&ChatAttachPaths {
                paths: vec![entry.path.clone()],
            })
            .is_err()
            {
                return;
            }
            String::new()
        };
        draft.set(replace_inline_media_query(&value, query, &replacement));
        menu_sel.set(0);
        focus_prompt_end(PROMPT_INPUT_ID);
    }

    pub fn answer_choice(&self, index: usize) {
        let mut question = self.run.choice_question;
        let mut options = self.run.choice_options;
        let mut menu_sel = self.slash.menu_sel;
        if send(&ChatChoiceSelected {
            index: index as u32,
        })
        .is_ok()
        {
            question.set(String::new());
            options.set(Vec::new());
            menu_sel.set(0);
        }
    }

    pub fn answer_approval(&self, call_id: String, decision: ApprovalDecision) {
        let mut approval = self.run.approval;
        let mut approval_sel = self.run.approval_sel;
        if send(&ChatApproval { call_id, decision }).is_ok() {
            approval.set(None);
            approval_sel.set(0);
        }
    }

    pub fn dismiss_selector(&self) {
        let mut draft = self.composer.draft;
        let mut menu_sel = self.slash.menu_sel;
        let value = draft.peek().clone();
        if let Some(query) = inline_media_query(&value) {
            draft.set(replace_inline_media_query(&value, query, ""));
            focus_prompt_end(PROMPT_INPUT_ID);
        } else {
            draft.set(String::new());
        }
        menu_sel.set(0);
    }

    pub fn edit_draft(&self, value: String) {
        let mut draft = self.composer.draft;
        let mut history_cursor = self.composer.history_cursor;
        let mut history_scratch = self.composer.history_scratch;
        let mut menu_sel = self.slash.menu_sel;
        draft.set(value);
        history_cursor.set(None);
        history_scratch.set(String::new());
        menu_sel.set(0);
    }

    pub fn remove_attachment(&self, index: usize) {
        let mut attachments = self.composer.attachments;
        let mut next = attachments.peek().clone();
        if index < next.len() {
            next.remove(index);
            attachments.set(next);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Transcript {
    pub items: Signal<Vec<ChatItem>>,
    pub loaded_start: Signal<u32>,
    pub messages_total: Signal<u32>,
    pub history_loading: Signal<bool>,
    pub recent_messages_json: Signal<String>,
    pub recent_messages_start: Signal<u32>,
    pub at_bottom: Signal<bool>,
    pub last_top: Signal<i32>,
    pub scroll_container: scroll::Container,
}

pub fn use_transcript() -> Transcript {
    Transcript {
        items: use_signal(Vec::new),
        loaded_start: use_signal(|| 0),
        messages_total: use_signal(|| 0),
        history_loading: use_signal(|| false),
        recent_messages_json: use_signal(String::new),
        recent_messages_start: use_signal(|| u32::MAX),
        at_bottom: use_signal(|| true),
        last_top: use_signal(|| 0),
        scroll_container: use_signal(|| None),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RunState {
    pub status: Signal<String>,
    pub error: Signal<String>,
    pub approval: Signal<Option<(String, String, String)>>,
    pub approval_sel: Signal<usize>,
    pub choice_question: Signal<String>,
    pub choice_options: Signal<Vec<String>>,
}

pub fn use_run_state() -> RunState {
    RunState {
        status: use_signal(|| "installing".to_string()),
        error: use_signal(String::new),
        approval: use_signal(|| None),
        approval_sel: use_signal(|| 0),
        choice_question: use_signal(String::new),
        choice_options: use_signal(Vec::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct AgentIdentity {
    pub agent_name: Signal<String>,
    pub conversation_title: Signal<String>,
    pub agent_icon: Signal<String>,
    pub accent: Signal<String>,
}

pub fn use_agent_identity() -> AgentIdentity {
    AgentIdentity {
        agent_name: use_signal(String::new),
        conversation_title: use_signal(String::new),
        agent_icon: use_signal(String::new),
        accent: use_signal(String::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Handoff {
    pub source: Signal<String>,
    pub truncated: Signal<bool>,
    pub message_count: Signal<u32>,
}

pub fn use_handoff() -> Handoff {
    Handoff {
        source: use_signal(String::new),
        truncated: use_signal(|| false),
        message_count: use_signal(|| 0),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ComposerDraft {
    pub draft: Signal<String>,
    pub attachments: Signal<Vec<ChatAttachment>>,
    pub attachment_previews: Signal<HashMap<String, ChatAttachment>>,
    pub attachment_preview_requests: Signal<HashSet<String>>,
    pub history_cursor: Signal<Option<usize>>,
    pub history_scratch: Signal<String>,
    pub transition_preview: Signal<String>,
    pub transition_attachments: Signal<Vec<ChatAttachment>>,
}

pub fn use_composer_draft() -> ComposerDraft {
    ComposerDraft {
        draft: use_signal(String::new),
        attachments: use_signal(Vec::new),
        attachment_previews: use_signal(HashMap::new),
        attachment_preview_requests: use_signal(HashSet::new),
        history_cursor: use_signal(|| None),
        history_scratch: use_signal(String::new),
        transition_preview: use_signal(String::new),
        transition_attachments: use_signal(Vec::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PromptQueue {
    pub queued: Signal<Vec<QueuedPromptSnapshot>>,
    pub paused: Signal<bool>,
}

pub fn use_prompt_queue() -> PromptQueue {
    PromptQueue {
        queued: use_signal(Vec::new),
        paused: use_signal(|| false),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct MediaPicker {
    pub entries: Signal<Vec<ChatMediaEntry>>,
    pub request_id: Signal<u64>,
    pub requested_query: Signal<Option<String>>,
    pub loading: Signal<bool>,
}

pub fn use_media_picker() -> MediaPicker {
    MediaPicker {
        entries: use_signal(Vec::new),
        request_id: use_signal(|| 0),
        requested_query: use_signal(|| None),
        loading: use_signal(|| false),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ModelPicker {
    pub models: Signal<Vec<ModelOptionEntry>>,
    pub current_model_id: Signal<String>,
    pub current_model: Signal<String>,
}

pub fn use_model_picker() -> ModelPicker {
    ModelPicker {
        models: use_signal(Vec::new),
        current_model_id: use_signal(String::new),
        current_model: use_signal(String::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ProjectPicker {
    pub open: Signal<bool>,
    pub expanded: Signal<String>,
    pub branches: Signal<Vec<ChatBranch>>,
    pub branches_for: Signal<String>,
}

pub fn use_project_picker() -> ProjectPicker {
    ProjectPicker {
        open: use_signal(|| false),
        expanded: use_signal(String::new),
        branches: use_signal(Vec::new),
        branches_for: use_signal(String::new),
    }
}

impl ProjectPicker {
    pub fn is_open(&self) -> bool {
        (self.open)()
    }

    pub fn toggle(&self) {
        let mut open = self.open;
        let showing = *open.peek();
        open.set(!showing);
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set(false);
    }

    pub fn expand(&self, project: &str) {
        let mut expanded = self.expanded;
        if expanded.peek().as_str() == project {
            expanded.set(String::new());
            return;
        }
        expanded.set(project.to_string());
        if self.branches_for.peek().as_str() != project {
            let mut branches = self.branches;
            branches.set(Vec::new());
        }
        let _ = send(&ChatBranchesRequest {
            project: project.to_string(),
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct EffortPicker {
    pub open: Signal<bool>,
    pub levels: Signal<Vec<String>>,
    pub current: Signal<String>,
    pub agent_key: Signal<String>,
}

impl EffortPicker {
    pub fn is_open(&self) -> bool {
        (self.open)()
    }

    pub fn toggle(&self) {
        let mut open = self.open;
        let showing = *open.peek();
        open.set(!showing);
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set(false);
    }
}

pub fn use_effort_picker() -> EffortPicker {
    EffortPicker {
        open: use_signal(|| false),
        levels: use_signal(Vec::new),
        current: use_signal(String::new),
        agent_key: use_signal(String::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SlashCommands {
    pub commands: Signal<Vec<SlashCommandEntry>>,
    pub menu_sel: Signal<usize>,
    pub composer_context: Signal<ComposerContext>,
}

pub fn use_slash_commands() -> SlashCommands {
    SlashCommands {
        commands: use_signal(Vec::new),
        menu_sel: use_signal(|| 0),
        composer_context: use_signal(ComposerContext::default),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Resume {
    pub sessions: Signal<Vec<ResumableSessionEntry>>,
    pub requested: Signal<bool>,
    pub loading: Signal<bool>,
}

pub fn use_resume() -> Resume {
    Resume {
        sessions: use_signal(Vec::new),
        requested: use_signal(|| false),
        loading: use_signal(|| false),
    }
}

fn set_if_changed<T: PartialEq + 'static>(mut signal: Signal<T>, value: T) {
    if signal.peek().ne(&value) {
        signal.set(value);
    }
}

fn merge_transcript_page(
    current: &mut Vec<ChatItem>,
    current_start: u32,
    incoming: Vec<ChatItem>,
    incoming_start: u32,
) -> u32 {
    if current_start <= incoming_start {
        let keep = incoming_start.saturating_sub(current_start) as usize;
        if keep <= current.len() {
            current.truncate(keep);
            current.extend(incoming);
            return current_start;
        }
    }
    *current = incoming;
    incoming_start
}

fn current_agent() -> String {
    fn provider(path: &str) -> Option<String> {
        Some(path.split('/').find(|part| !part.is_empty())?.to_string())
    }

    if let Some(meta) = try_consume_context::<vmux_core::PageMetadata>()
        && let Some(rest) = meta.url.strip_prefix("vmux://agent/")
        && let Some(agent) = provider(rest)
    {
        return agent;
    }
    "agent".to_string()
}
