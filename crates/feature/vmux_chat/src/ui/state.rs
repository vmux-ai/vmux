use super::format::{ChatPageTitle, ImportedMessages, ModelOptions, ResumeMenuState, SelectorMode};
use super::scroll;
use crate::event::ChatResumeState;
use crate::event::{
    ApprovalDecision, ChatApproval, ChatAttachPaths, ChatAttachment, ChatAttachments, ChatBranch,
    ChatBranchesRequest, ChatBranchesState, ChatChoiceSelected, ChatComposerEffect,
    ChatComposerMenuChanged, ChatComposerMenuKind, ChatComposerMenuState, ChatDraftChanged,
    ChatHistoryMoreRequest, ChatItem, ChatListKind, ChatListSelectionChanged,
    ChatListSelectionState, ChatMediaEntry, ChatMediaState, ChatRemoveAttachment,
    ChatSlashCommandRequest, ChatSnapshot, ChatStop, ChatSubmit, ChatTranscriptState,
    ComposerContext, ModelOptionEntry, QueuedPromptSnapshot, ResumableSessionEntry, ResumeSession,
    SelectMode, SelectModel, SlashCommand, SlashCommandEntry,
};
use crate::host::{ChatUiState, ChatUiStatePatch};
use crate::tab::Accent;
use dioxus::prelude::*;
use vmux_api::prompt_media::{inline_media_query, replace_inline_media_query};
use vmux_core::prompt_media::MediaPath;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::composer::{
    PROMPT_INPUT_ID, PromptComposerAttachment, PromptComposerMode, focus_prompt_end,
};
use vmux_ui::components::composer_bar::{
    ComposerChip, ComposerMenu, ComposerMenuKind, use_composer_menu,
};
use vmux_ui::components::mcp_menu::{McpConnections, use_mcp_connections};
use vmux_ui::components::prompt_media_options::PromptMediaOption;
use vmux_ui::file_icon::FilePath;
use vmux_ui::hooks::{send, use_selector, use_theme, use_ui_state_patches};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[derive(Clone, Copy, PartialEq)]
pub struct Chat {
    pub agent: Signal<String>,
    pub transcript: Transcript,
    pub run: RunState,
    pub identity: AgentIdentity,
    pub user: UserIdentity,
    pub handoff: Handoff,
    pub composer: ComposerDraft,
    pub queue: PromptQueue,
    pub media: MediaPicker,
    pub mcp: McpConnections,
    pub models: ModelPicker,
    pub effort: EffortPicker,
    pub permissions: PermissionPicker,
    pub projects: ProjectPicker,
    pub slash: SlashCommands,
    pub resume: Resume,
    pub menu: ComposerMenu,
}

pub fn use_chat() -> Chat {
    use_theme();
    let transcript = use_transcript();
    let chat = Chat {
        agent: use_signal(CurrentAgent::read),
        transcript,
        run: use_run_state(),
        identity: use_agent_identity(),
        user: use_user_identity(),
        handoff: use_handoff(),
        composer: use_composer_draft(),
        queue: use_prompt_queue(),
        media: use_media_picker(),
        mcp: use_mcp_connections(),
        models: use_model_picker(),
        effort: use_effort_picker(),
        permissions: use_permission_picker(),
        projects: use_project_picker(),
        slash: use_slash_commands(),
        resume: use_resume(),
        menu: use_composer_menu(),
    };
    chat.listen();
    chat.watch();
    chat
}

impl Chat {
    fn listen(&self) {
        let chat = *self;
        let _error = use_ui_state_patches::<ChatUiState>(move |patch| {
            chat.apply_ui_state(patch);
        });
    }

    fn apply_ui_state(&self, patch: &ChatUiStatePatch) {
        if let Some(snapshot) = &patch.snapshot {
            self.apply_snapshot(*snapshot.clone());
        }
        if let Some(context) = &patch.composer {
            let mut composer_context = self.slash.composer_context;
            let mut loaded = self.projects.loaded;
            composer_context.set(context.clone());
            loaded.set(true);
        }
        if let Some(state) = &patch.mode {
            let mut modes = self.permissions.modes;
            let mut current_mode_id = self.permissions.current_mode_id;
            modes.set(state.modes.clone());
            current_mode_id.set(state.current_mode_id.clone());
        }
        if let Some(state) = &patch.model {
            let mut models = self.models.models;
            let mut current_model_id = self.models.current_model_id;
            let mut default_model_id = self.models.default_model_id;
            let mut current_model = self.models.current_model;
            let mut loaded = self.models.loaded;
            let mut levels = self.effort.levels;
            let mut current = self.effort.current;
            let mut default_level = self.effort.default_level;
            let mut agent_key = self.effort.agent_key;
            let mut menu_sel = self.slash.menu_sel;
            models.set(state.models.clone());
            current_model_id.set(state.current_model_id.clone());
            default_model_id.set(state.default_model_id.clone());
            current_model.set(state.current_model_name.clone());
            levels.set(state.effort_levels.clone());
            current.set(state.effort_current.clone());
            default_level.set(state.effort_default.clone());
            agent_key.set(state.agent_key.clone());
            menu_sel.set(0);
            loaded.set(true);
        }
        if let Some(incoming) = &patch.slash_commands {
            let mut commands = self.slash.commands;
            commands.set(incoming.commands.clone());
        }
        if let Some(state) = &patch.transcript {
            self.apply_transcript(state);
        }
        if let Some(selected) = &patch.attachments {
            self.apply_attachments(selected);
        }
        if let Some(state) = &patch.media {
            self.apply_media(state);
        }
        if let Some(incoming) = &patch.branches {
            self.apply_branches(incoming);
        }
        if let Some(state) = &patch.resume {
            self.apply_sessions(state);
        }
        if let Some(effect) = &patch.composer_effect {
            self.apply_composer_effect(effect);
        }
        if let Some(effect) = patch.prompt_focus
            && effect.revision > *self.composer.focus_revision.peek()
        {
            let mut focus_revision = self.composer.focus_revision;
            focus_revision.set(effect.revision);
            focus_prompt_end(PROMPT_INPUT_ID);
        }
        if let Some(selection) = patch.list_selection {
            self.apply_list_selection(selection);
        }
        if let Some(menu) = &patch.composer_menu {
            self.apply_composer_menu(menu);
        }
    }

    fn apply_list_selection(&self, selection: ChatListSelectionState) {
        match selection.kind {
            ChatListKind::Approval => {
                set_if_changed(self.run.approval_sel, selection.index as usize);
            }
            ChatListKind::Composer => self.menu.point_at(selection.index as usize),
            ChatListKind::Choice
            | ChatListKind::Media
            | ChatListKind::Mcp
            | ChatListKind::Session
            | ChatListKind::Model
            | ChatListKind::Command => {
                set_if_changed(self.slash.menu_sel, selection.index as usize);
            }
        }
    }

    fn apply_composer_menu(&self, state: &ChatComposerMenuState) {
        let Some(kind) = state.menu else {
            self.menu.close();
            return;
        };
        let kind = match kind {
            ChatComposerMenuKind::Effort => ComposerMenuKind::Effort,
            ChatComposerMenuKind::Permission => ComposerMenuKind::Permission,
            ChatComposerMenuKind::Project => ComposerMenuKind::Project,
            ChatComposerMenuKind::Branch => ComposerMenuKind::Branch,
        };
        self.menu.show_at(kind, state.index as usize);
    }

    fn apply_composer_effect(&self, effect: &ChatComposerEffect) {
        if effect.revision <= *self.composer.effect_revision.peek() {
            return;
        }
        let mut revision = self.composer.effect_revision;
        let mut draft = self.composer.draft;
        let mut menu_sel = self.slash.menu_sel;
        revision.set(effect.revision);
        draft.set(effect.draft.clone());
        menu_sel.set(0);
        if effect.focus {
            focus_prompt_end(PROMPT_INPUT_ID);
        }
    }

    fn apply_attachments(&self, selected: &ChatAttachments) {
        set_if_changed(self.composer.attachments, selected.attachments.clone());
    }

    fn apply_media(&self, state: &ChatMediaState) {
        set_if_changed(self.media.query, state.query.clone());
        set_if_changed(self.media.entries, state.entries.clone());
        set_if_changed(self.media.loading, state.loading);
        set_if_changed(self.slash.menu_sel, 0);
    }

    fn apply_sessions(&self, state: &ChatResumeState) {
        set_if_changed(self.resume.query, state.query.clone());
        set_if_changed(self.resume.sessions, state.sessions.clone());
        set_if_changed(self.resume.rows, state.rows.clone());
        set_if_changed(self.resume.total, state.total);
        set_if_changed(self.resume.loading, state.loading);
        set_if_changed(self.resume.active, state.active);
        set_if_changed(self.slash.menu_sel, 0);
    }

    fn apply_branches(&self, incoming: &ChatBranchesState) {
        set_if_changed(self.projects.branches_for, incoming.project.clone());
        set_if_changed(self.projects.branches, incoming.branches.clone());
        set_if_changed(self.projects.branches_loading, incoming.loading);
    }

    fn watch(&self) {
        let chat = *self;
        use_effect(move || focus_prompt_end(PROMPT_INPUT_ID));
        use_effect(move || {
            let menu = match chat.menu.opened() {
                Some(ComposerMenuKind::Effort) => Some(ChatComposerMenuKind::Effort),
                Some(ComposerMenuKind::Permission) => Some(ChatComposerMenuKind::Permission),
                Some(ComposerMenuKind::Project) => Some(ChatComposerMenuKind::Project),
                Some(ComposerMenuKind::Branch) => Some(ChatComposerMenuKind::Branch),
                Some(ComposerMenuKind::Agent | ComposerMenuKind::Model) | None => None,
            };
            let _ = send(&ChatComposerMenuChanged {
                menu,
                index: chat.menu.cursor() as u32,
            });
        });
        use_effect(move || {
            let _ = chat.transcript.items.read().len();
            let _ = chat.run.status.read();
            if !*chat.transcript.at_bottom.peek() {
                return;
            }
            scroll::to_bottom(chat.transcript.scroll_container);
        });
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
        if !snapshot.user_name.is_empty() {
            set_if_changed(self.user.name, snapshot.user_name.clone());
        }
        if !snapshot.user_initials.is_empty() {
            set_if_changed(self.user.initials, snapshot.user_initials.clone());
        }
        if !snapshot.user_color.is_empty() {
            set_if_changed(self.user.color, snapshot.user_color.clone());
        }
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
            snapshot.approval.clone()
        } else {
            None
        };
        let mut approval = self.run.approval;
        if approval.peek().ne(&next_approval) {
            approval.set(next_approval);
            set_if_changed(self.run.approval_sel, 0);
        }
    }

    fn apply_transcript(&self, state: &ChatTranscriptState) {
        let transcript = self.transcript;
        let preserve_scroll = (transcript.generation)() == state.generation
            && (transcript.prepend_revision)() != state.prepend_revision;
        let metrics = if preserve_scroll {
            scroll::metrics(transcript.scroll_container)
        } else {
            None
        };
        set_if_changed(transcript.items, state.items.clone());
        set_if_changed(transcript.loaded_start, state.loaded_start);
        set_if_changed(transcript.messages_total, state.total);
        set_if_changed(transcript.history_loading, state.loading);
        set_if_changed(transcript.generation, state.generation);
        set_if_changed(transcript.prepend_revision, state.prepend_revision);
        set_if_changed(transcript.active_subagents, state.active_subagents);
        set_if_changed(transcript.active_tasks, state.active_tasks);
        if let Some((height, top)) = metrics {
            scroll::restore(transcript.scroll_container, height, top);
        }
    }

    pub fn request_history(&self) {
        let _ = send(&ChatHistoryMoreRequest);
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
        ChatPageTitle::resolve(&(self.identity.conversation_title)(), &self.header_name())
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

    pub fn draft(&self) -> String {
        (self.composer.draft)()
    }

    pub fn filtered_commands(&self) -> Vec<SlashCommandEntry> {
        let draft = self.draft();
        let SelectorMode::Commands(query) = SelectorMode::from_draft(&draft) else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        let mut matching = Vec::new();
        for command in self.slash.commands.read().iter() {
            if SlashCommands::name(command.command).starts_with(&query) {
                matching.push(command.clone());
            }
        }
        matching
    }

    pub fn filtered_models(&self) -> Vec<ModelOptionEntry> {
        let draft = self.draft();
        let SelectorMode::Models(query) = SelectorMode::from_draft(&draft) else {
            return Vec::new();
        };
        self.models.filtered(query)
    }

    pub fn filtered_mcp_servers(&self) -> Vec<vmux_api::mcp::McpServerEntry> {
        let draft = self.draft();
        let SelectorMode::Mcp(query) = SelectorMode::from_draft(&draft) else {
            return Vec::new();
        };
        self.mcp.filtered(query)
    }

    pub fn command_menu_open(&self) -> bool {
        !self.filtered_commands().is_empty()
    }

    pub fn resume_menu_open(&self) -> bool {
        matches!(
            SelectorMode::from_draft(&self.draft()),
            SelectorMode::Resume(_)
        )
    }

    pub fn model_menu_open(&self) -> bool {
        matches!(
            SelectorMode::from_draft(&self.draft()),
            SelectorMode::Models(_)
        )
    }

    pub fn mcp_menu_open(&self) -> bool {
        #[cfg(host)]
        {
            matches!(
                SelectorMode::from_draft(&self.draft()),
                SelectorMode::Mcp(_)
            )
        }
        #[cfg(not(host))]
        {
            false
        }
    }

    pub fn selector_open(&self) -> bool {
        self.media_menu_open()
            || self.command_menu_open()
            || self.mcp_menu_open()
            || self.resume_menu_open()
            || self.model_menu_open()
    }

    pub fn resume_state(&self) -> Option<ResumeMenuState> {
        if !self.resume_menu_open() {
            return None;
        }
        Some(ResumeMenuState::resolve(
            (self.resume.active)(),
            (self.resume.loading)(),
            &(self.resume.query)(),
            self.resume.rows.read().len(),
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
                display_path: MediaPath::new(entry).display(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FilePath(&entry.name).extension_label(),
                is_dir: entry.is_dir,
            });
        }
        options
    }

    pub fn composer_attachments(&self) -> Vec<PromptComposerAttachment> {
        let mut pills =
            PromptComposerAttachment::pinned(&self.composer.transition_attachments.read());
        pills.extend(PromptComposerAttachment::removable(
            &self.composer.attachments.read(),
        ));
        pills
    }

    pub fn streaming(&self) -> bool {
        matches!(self.status().as_str(), "streaming" | "awaiting")
    }

    pub fn prompt_mode(&self) -> PromptComposerMode {
        if self.streaming() && self.queue.queued.read().is_empty() {
            PromptComposerMode::Stop
        } else {
            PromptComposerMode::Send
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
    pub fn model_chip(&self) -> Option<ComposerChip> {
        if !(self.models.loaded)() {
            return Some(ComposerChip::loading());
        }
        let name = (self.models.current_model)();
        if name.is_empty() {
            return None;
        }
        let label = match (self.models.current_model_id)() == (self.models.default_model_id)() {
            true => translate_with(
                "agent-option-default",
                &[("value", TranslationValue::String(&name))],
            ),
            false => name,
        };
        let chat = *self;
        let open = EventHandler::new(move |()| {
            let mut menu_sel = chat.slash.menu_sel;
            chat.menu.close();
            chat.set_draft("/model ".to_string());
            menu_sel.set(0);
            focus_prompt_end(PROMPT_INPUT_ID);
        });
        Some(ComposerChip::ready(label, translate("agent-change-model")).opens(open))
    }

    pub fn effort_chip(&self) -> Option<ComposerChip> {
        if !(self.models.loaded)() {
            return Some(ComposerChip::loading());
        }
        if self.effort.levels.read().is_empty() {
            return None;
        }
        let selected = (self.effort.current)();
        let label = match selected.is_empty() {
            false => selected,
            true => translate_with(
                "agent-option-default",
                &[(
                    "value",
                    TranslationValue::String(&(self.effort.default_level)()),
                )],
            ),
        };
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.open_menu(ComposerMenuKind::Effort);
            focus_prompt_end(PROMPT_INPUT_ID);
        });
        Some(ComposerChip::ready(label, translate("agent-effort-tooltip")).opens(open))
    }

    pub fn permission_chip(&self) -> Option<ComposerChip> {
        let modes = self.permissions.modes.read();
        if modes.is_empty() {
            return None;
        }
        let current_mode_id = (self.permissions.current_mode_id)();
        let current = modes.iter().find(|mode| mode.id == current_mode_id);
        let label = current
            .map(|mode| mode.name.clone())
            .unwrap_or_else(|| current_mode_id.clone());
        let title = current
            .and_then(|mode| mode.description.clone())
            .filter(|description| !description.is_empty())
            .unwrap_or_else(|| translate("composer-permission-change"));
        let selected = modes
            .iter()
            .position(|mode| mode.id == current_mode_id)
            .unwrap_or(0);
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.open_menu_at(ComposerMenuKind::Permission, selected);
            focus_prompt_end(PROMPT_INPUT_ID);
        });
        Some(ComposerChip::ready(label, title).opens(open))
    }

    pub fn project_chip(&self) -> Option<ComposerChip> {
        if !(self.projects.loaded)() {
            return Some(ComposerChip::loading());
        }
        let context = (self.slash.composer_context)();
        let active_project = context.projects.iter().find(|project| project.is_active);
        let label = if let Some(project) = active_project {
            project.label.clone()
        } else if context.workspace_selected && !context.workspace_name.is_empty() {
            context.workspace_name.clone()
        } else {
            translate("agent-project-select")
        };
        if context.can_manage_workspace {
            let workspace_path = active_project
                .map(|project| project.path.as_str())
                .filter(|path| !path.is_empty())
                .unwrap_or(&context.cwd);
            let title = if workspace_path.is_empty() {
                translate("agent-project-choose")
            } else {
                format!("{} · {}", translate("agent-project-choose"), workspace_path)
            };
            let selected = context
                .projects
                .iter()
                .filter(|project| project.depth == 0)
                .position(|project| project.is_active)
                .unwrap_or(0);
            let chat = *self;
            let open =
                EventHandler::new(move |()| chat.open_menu_at(ComposerMenuKind::Project, selected));
            return Some(ComposerChip::ready(label, title).opens(open));
        }
        if context.cwd.is_empty() {
            return None;
        }
        Some(ComposerChip::ready(label, context.cwd))
    }

    pub fn branch_chip(&self) -> Option<ComposerChip> {
        if !(self.projects.loaded)() {
            return Some(ComposerChip::loading());
        }
        let context = (self.slash.composer_context)();
        if !context.is_git_repo {
            return None;
        }
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.open_menu(ComposerMenuKind::Branch);
            if chat.menu.is(ComposerMenuKind::Branch) {
                let _ = send(&ChatBranchesRequest);
            }
        });
        if context.branch.is_empty() {
            return Some(
                ComposerChip::ready(
                    translate("composer-git"),
                    translate("composer-git-repository"),
                )
                .opens(open),
            );
        }
        let title = translate_with(
            "composer-branch-name",
            &[("branch", TranslationValue::String(&context.branch))],
        );
        Some(ComposerChip::ready(context.branch, title).opens(open))
    }
}

impl Chat {
    pub fn submit(&self) {
        let mut at_bottom = self.transcript.at_bottom;
        let text = self.composer.draft.peek().trim().to_string();
        let selected = self.composer.attachments.peek().clone();
        if text.is_empty() && selected.is_empty() {
            return;
        }
        if send(&ChatSubmit { text }).is_err() {
            return;
        }
        at_bottom.set(true);
    }

    pub fn stop_or_flush(&self) {
        let _ = send(&ChatStop);
    }

    pub fn select_slash_command(&self, command: SlashCommand) {
        let _ = send(&ChatSlashCommandRequest { command });
    }

    pub fn select_model(&self, model: &ModelOptionEntry) {
        let _ = send(&SelectModel {
            model_id: model.id.clone(),
        });
        self.set_draft(String::new());
    }

    pub fn select_mode(&self, mode_id: String) {
        let _ = send(&SelectMode { mode_id });
        focus_prompt_end(PROMPT_INPUT_ID);
    }

    pub fn activate_mcp_server(&self, index: usize) {
        let servers = self.filtered_mcp_servers();
        let index = index.min(servers.len().saturating_sub(1));
        let Some(server) = servers.get(index) else {
            return;
        };
        self.mcp.activate(server);
    }

    pub fn mcp_selected(&self) -> usize {
        (self.slash.menu_sel)().min(self.filtered_mcp_servers().len().saturating_sub(1))
    }

    pub fn select_resume_session(&self, session: &ResumableSessionEntry) {
        let _ = send(&ResumeSession {
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            cwd: session.cwd.clone(),
        });
        self.set_draft(String::new());
    }

    pub fn select_media_entry(&self, entry: &ChatMediaEntry) {
        let mut menu_sel = self.slash.menu_sel;
        let value = self.composer.draft.peek().clone();
        let Some(query) = inline_media_query(&value) else {
            return;
        };
        let reference = MediaPath::new(entry).reference();
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
        self.set_draft(replace_inline_media_query(&value, query, &replacement));
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

    pub fn point_at_choice(&self, index: usize) {
        set_if_changed(self.slash.menu_sel, index);
        let _ = send(&ChatListSelectionChanged {
            index: index as u32,
        });
    }

    pub fn answer_approval(&self, call_id: String, decision: ApprovalDecision) {
        let mut approval = self.run.approval;
        let mut approval_sel = self.run.approval_sel;
        if send(&ChatApproval { call_id, decision }).is_ok() {
            approval.set(None);
            approval_sel.set(0);
        }
    }

    pub fn point_at_approval(&self, index: usize) {
        set_if_changed(self.run.approval_sel, index);
        let _ = send(&ChatListSelectionChanged {
            index: index as u32,
        });
    }

    pub fn point_at_list(&self, index: usize) {
        set_if_changed(self.slash.menu_sel, index);
        let _ = send(&ChatListSelectionChanged {
            index: index as u32,
        });
    }

    pub fn open_menu(&self, kind: ComposerMenuKind) {
        self.open_menu_at(kind, 0);
    }

    pub fn open_menu_at(&self, kind: ComposerMenuKind, index: usize) {
        if self.selector_open() {
            self.dismiss_selector();
        }
        self.menu.toggle_at(kind, index);
    }

    pub fn dismiss_selector(&self) {
        if self.menu.opened().is_some() {
            self.menu.close();
            focus_prompt_end(PROMPT_INPUT_ID);
            return;
        }
        let mut menu_sel = self.slash.menu_sel;
        let value = self.composer.draft.peek().clone();
        if let Some(query) = inline_media_query(&value) {
            self.set_draft(replace_inline_media_query(&value, query, ""));
            focus_prompt_end(PROMPT_INPUT_ID);
        } else {
            self.set_draft(String::new());
        }
        menu_sel.set(0);
    }

    pub fn edit_draft(&self, value: String) {
        let mut menu_sel = self.slash.menu_sel;
        self.menu.close();
        self.set_draft(value);
        menu_sel.set(0);
    }

    pub(crate) fn set_draft(&self, value: String) {
        let mut draft = self.composer.draft;
        if draft.peek().as_str() == value.as_str() {
            return;
        }
        draft.set(value.clone());
        let _ = send(&ChatDraftChanged { text: value });
    }

    pub fn remove_attachment(&self, index: usize) {
        let attachments = self.composer.attachments.read();
        let Some(attachment) = attachments.get(index) else {
            return;
        };
        let _ = send(&ChatRemoveAttachment {
            path: attachment.path.clone(),
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Transcript {
    pub items: Signal<Vec<ChatItem>>,
    pub loaded_start: Signal<u32>,
    pub messages_total: Signal<u32>,
    pub history_loading: Signal<bool>,
    pub generation: Signal<u64>,
    pub prepend_revision: Signal<u64>,
    pub active_subagents: Signal<u32>,
    pub active_tasks: Signal<u32>,
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
        generation: use_signal(|| 0),
        prepend_revision: use_signal(|| 0),
        active_subagents: use_signal(|| 0),
        active_tasks: use_signal(|| 0),
        at_bottom: use_signal(|| true),
        last_top: use_signal(|| 0),
        scroll_container: use_signal(|| None),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RunState {
    pub status: Signal<String>,
    pub error: Signal<String>,
    pub approval: Signal<Option<crate::event::PendingApproval>>,
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
pub struct UserIdentity {
    pub name: Signal<String>,
    pub initials: Signal<String>,
    pub color: Signal<String>,
}

pub fn use_user_identity() -> UserIdentity {
    let name = translate("team-you");
    let initials = name
        .chars()
        .next()
        .map(|character| character.to_uppercase().collect())
        .unwrap_or_default();
    UserIdentity {
        name: use_signal(move || name),
        initials: use_signal(move || initials),
        color: use_signal(|| "#3b82f6".to_string()),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Handoff {
    pub source: Signal<String>,
    pub truncated: Signal<bool>,
    pub message_count: Signal<u32>,
}

impl Handoff {
    pub fn boundary(&self, message_index: usize) -> bool {
        ImportedMessages::new((self.message_count)()).boundary(message_index)
    }
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
    pub effect_revision: Signal<u64>,
    pub focus_revision: Signal<u64>,
    pub attachments: Signal<Vec<ChatAttachment>>,
    pub transition_preview: Signal<String>,
    pub transition_attachments: Signal<Vec<ChatAttachment>>,
}

pub fn use_composer_draft() -> ComposerDraft {
    ComposerDraft {
        draft: use_signal(String::new),
        effect_revision: use_signal(|| 0),
        focus_revision: use_signal(|| 0),
        attachments: use_signal(Vec::new),
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
    pub query: Signal<String>,
    pub loading: Signal<bool>,
}

pub fn use_media_picker() -> MediaPicker {
    MediaPicker {
        entries: use_signal(Vec::new),
        query: use_signal(String::new),
        loading: use_signal(|| false),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ModelPicker {
    pub models: Signal<Vec<ModelOptionEntry>>,
    pub current_model_id: Signal<String>,
    pub default_model_id: Signal<String>,
    pub current_model: Signal<String>,
    pub loaded: Signal<bool>,
}

impl ModelPicker {
    fn filtered(&self, query: &str) -> Vec<ModelOptionEntry> {
        ModelOptions::new(self.models.read().clone()).filtered(query)
    }
}

pub fn use_model_picker() -> ModelPicker {
    ModelPicker {
        models: use_signal(Vec::new),
        current_model_id: use_signal(String::new),
        default_model_id: use_signal(String::new),
        current_model: use_signal(String::new),
        loaded: use_signal(|| false),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ProjectPicker {
    pub loaded: Signal<bool>,
    pub branches: Signal<Vec<ChatBranch>>,
    pub branches_for: Signal<String>,
    pub branches_loading: Signal<bool>,
}

pub fn use_project_picker() -> ProjectPicker {
    ProjectPicker {
        loaded: use_signal(|| false),
        branches: use_signal(Vec::new),
        branches_for: use_signal(String::new),
        branches_loading: use_signal(|| false),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct EffortPicker {
    pub levels: Signal<Vec<String>>,
    pub current: Signal<String>,
    pub default_level: Signal<String>,
    pub agent_key: Signal<String>,
}

pub fn use_effort_picker() -> EffortPicker {
    EffortPicker {
        levels: use_signal(Vec::new),
        current: use_signal(String::new),
        default_level: use_signal(String::new),
        agent_key: use_signal(String::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PermissionPicker {
    pub modes: Signal<Vec<vmux_api::protocol::AcpModeOption>>,
    pub current_mode_id: Signal<String>,
}

pub fn use_permission_picker() -> PermissionPicker {
    PermissionPicker {
        modes: use_signal(Vec::new),
        current_mode_id: use_signal(String::new),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SlashCommands {
    pub commands: Signal<Vec<SlashCommandEntry>>,
    pub menu_sel: Signal<usize>,
    pub composer_context: Signal<ComposerContext>,
}

impl SlashCommands {
    pub const fn name(command: SlashCommand) -> &'static str {
        match command {
            SlashCommand::Upload => "upload",
            SlashCommand::Resume => "resume",
            SlashCommand::Mcp => "mcp",
            SlashCommand::Model => "model",
            SlashCommand::Cli => "cli",
        }
    }
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
    pub rows: Signal<Vec<vmux_api::command_bar::CommandBarResultItem>>,
    pub query: Signal<String>,
    pub total: Signal<u32>,
    pub loading: Signal<bool>,
    pub active: Signal<bool>,
}

pub fn use_resume() -> Resume {
    Resume {
        sessions: use_signal(Vec::new),
        rows: use_signal(Vec::new),
        query: use_signal(String::new),
        total: use_signal(|| 0),
        loading: use_signal(|| false),
        active: use_signal(|| false),
    }
}

fn set_if_changed<T: PartialEq + 'static>(mut signal: Signal<T>, value: T) {
    if signal.peek().ne(&value) {
        signal.set(value);
    }
}

struct CurrentAgent;

impl CurrentAgent {
    fn read() -> String {
        if let Some(meta) = try_consume_context::<vmux_core::PageMetadata>()
            && let Some(rest) = meta
                .url
                .strip_prefix("vmux://sessions/")
                .or_else(|| meta.url.strip_prefix("vmux://agent/"))
            && let Some(agent) = Self::provider(rest)
        {
            return agent;
        }
        "agent".to_string()
    }

    fn provider(path: &str) -> Option<String> {
        Some(path.split('/').find(|part| !part.is_empty())?.to_string())
    }
}
