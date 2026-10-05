use super::format::{ImportedMessages, ResumeMenuState};
use super::scroll;
use crate::event::{
    ApprovalDecision, ChatApproval, ChatAttachments, ChatBranchesState, ChatComposerEffect,
    ChatComposerMenuKind, ChatComposerMenuRequest, ChatComposerMenuState, ChatDraftChanged,
    ChatHistoryMoreRequest, ChatListChooseRequest, ChatListKind, ChatListSelectionChanged,
    ChatListSelectionState, ChatMediaState, ChatRemoveAttachment, ChatSelectorState, ChatSnapshot,
    ChatStop, ChatSubmit, ChatTranscriptState, ComposerContext, ModelOptionEntry,
    QueuedPromptSnapshot, SelectMode, SlashCommandEntry,
};
use crate::event::{ChatDismissSelectorRequest, ChatResumeState};
use crate::state::ChatUiState;
use crate::tab::Accent;
use dioxus::prelude::*;
use vmux_api::mcp::McpServersUiState;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposerMode, PromptFocus};
use vmux_ui::components::composer_bar::{ComposerChip, ComposerMenuKind};
use vmux_ui::hooks::{send, use_selector, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[derive(PartialEq)]
pub struct ChatValue<T: 'static> {
    pub value: Memo<T>,
    pub ready: Memo<bool>,
}

impl<T> Clone for ChatValue<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ChatValue<T> {}

#[derive(Clone, Copy, PartialEq)]
pub struct Chat {
    root: Signal<ChatUiState>,
    pub transcript: Transcript,
    pub run: RunState,
    pub identity: AgentIdentity,
    pub user: UserIdentity,
    pub handoff: Handoff,
    pub composer: ComposerDraft,
    pub queue: PromptQueue,
    pub media: MediaPicker,
    pub mcp: ChatValue<McpServersUiState>,
    pub models: ModelPicker,
    pub effort: EffortPicker,
    pub permissions: PermissionPicker,
    pub projects: ProjectPicker,
    pub slash: SlashCommands,
    pub resume: Resume,
    pub selector: ChatValue<ChatSelectorState>,
    selection: Memo<Option<ChatListSelectionState>>,
    pub menu: ChatMenu,
}

pub fn use_chat() -> Chat {
    use_theme();
    let root = use_ui_state::<ChatUiState>().state;
    let always_ready = use_memo(|| true);
    let snapshot = ChatValue {
        value: use_memo(move || root.read().snapshot.clone()),
        ready: use_memo(move || root.read().snapshot_ready),
    };
    let composer_context = ChatValue {
        value: use_memo(move || root.read().composer.clone()),
        ready: use_memo(move || root.read().composer_ready),
    };
    let mode = ChatValue {
        value: use_memo(move || root.read().mode.clone()),
        ready: always_ready,
    };
    let model = ChatValue {
        value: use_memo(move || root.read().model.clone()),
        ready: use_memo(move || root.read().model_ready),
    };
    let attachments = ChatValue {
        value: use_memo(move || root.read().attachments.clone()),
        ready: always_ready,
    };
    let media = ChatValue {
        value: use_memo(move || root.read().media.clone()),
        ready: always_ready,
    };
    let branches = ChatValue {
        value: use_memo(move || root.read().branches.clone()),
        ready: always_ready,
    };
    let resume = ChatValue {
        value: use_memo(move || root.read().resume.clone()),
        ready: always_ready,
    };
    let selector = ChatValue {
        value: use_memo(move || root.read().selector.clone()),
        ready: always_ready,
    };
    let mcp = ChatValue {
        value: use_memo(move || root.read().mcp.clone()),
        ready: always_ready,
    };
    let chat = Chat {
        root,
        transcript: use_transcript(root),
        run: RunState { snapshot },
        identity: AgentIdentity { snapshot },
        user: UserIdentity { snapshot },
        handoff: Handoff { snapshot },
        composer: use_composer_draft(attachments),
        queue: PromptQueue { snapshot },
        media: MediaPicker { state: media },
        mcp,
        models: ModelPicker { state: model },
        effort: EffortPicker { state: model },
        permissions: PermissionPicker { state: mode },
        projects: ProjectPicker {
            context: composer_context,
            branches,
        },
        slash: SlashCommands {
            context: composer_context,
        },
        resume: Resume { state: resume },
        selector,
        selection: use_memo(move || root.read().list_selection),
        menu: ChatMenu {
            state: use_memo(move || root.read().composer_menu.clone()),
        },
    };
    chat.listen(root);
    chat.watch();
    chat
}

impl Chat {
    fn listen(&self, root: Signal<ChatUiState>) {
        let chat = *self;
        use_effect(move || {
            chat.apply_composer_effect(&root.read().composer_effect);
        });
        let chat = *self;
        use_effect(move || {
            let Some(effect) = root.read().prompt_focus else {
                return;
            };
            if effect.revision <= *chat.composer.focus_revision.peek() {
                return;
            }
            let mut focus_revision = chat.composer.focus_revision;
            focus_revision.set(effect.revision);
            PromptFocus::end(PROMPT_INPUT_ID);
        });
    }

    fn apply_composer_effect(&self, effect: &ChatComposerEffect) {
        if effect.revision <= *self.composer.effect_revision.peek() {
            return;
        }
        let mut revision = self.composer.effect_revision;
        revision.set(effect.revision);
        if effect.focus {
            PromptFocus::end(PROMPT_INPUT_ID);
        }
    }

    fn watch(&self) {
        let chat = *self;
        use_effect(move || PromptFocus::end(PROMPT_INPUT_ID));
        use_effect(move || {
            let _ = chat.transcript.current().items.len();
            let _ = chat.run.status();
            if !*chat.transcript.at_bottom.peek() {
                return;
            }
            scroll::to_bottom(chat.transcript.scroll_container);
        });
        use_selector(
            move || chat.list_selection(),
            move |selected| {
                let selector = chat.selector.value.read().active;
                let _ = chat.resume.state.value.read().sessions.len();
                let _ = chat.media.state.value.read().entries.len();
                if !chat.run.choice_options().is_empty() {
                    format!("agent-choice-item-{selected}")
                } else if selector == Some(ChatListKind::Media) {
                    format!("prompt-media-item-{selected}")
                } else {
                    format!("agent-selector-item-{selected}")
                }
            },
        );
    }

    pub fn request_history(&self) {
        let _ = send(&ChatHistoryMoreRequest);
    }

    pub fn list_selection(&self) -> usize {
        let Some(selection) = *self.selection.read() else {
            return 0;
        };
        match selection.kind {
            ChatListKind::Choice
            | ChatListKind::Media
            | ChatListKind::Mcp
            | ChatListKind::Session
            | ChatListKind::Model
            | ChatListKind::Command => selection.index as usize,
            ChatListKind::Approval | ChatListKind::Composer => 0,
        }
    }

    pub fn approval_selection(&self) -> usize {
        let Some(selection) = *self.selection.read() else {
            return 0;
        };
        if selection.kind == ChatListKind::Approval {
            return selection.index as usize;
        }
        0
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ChatMenu {
    state: Memo<ChatComposerMenuState>,
}

impl ChatMenu {
    pub fn opened(self) -> Option<ComposerMenuKind> {
        match self.state.read().menu {
            Some(ChatComposerMenuKind::Effort) => Some(ComposerMenuKind::Effort),
            Some(ChatComposerMenuKind::Permission) => Some(ComposerMenuKind::Permission),
            Some(ChatComposerMenuKind::Project) => Some(ComposerMenuKind::Project),
            Some(ChatComposerMenuKind::Branch) => Some(ComposerMenuKind::Branch),
            None => None,
        }
    }

    pub fn cursor(self) -> usize {
        self.state.read().index as usize
    }
}

impl Chat {
    pub fn agent(&self) -> String {
        self.identity.id()
    }

    pub fn header_name(&self) -> String {
        self.identity.snapshot.value.read().header_name.clone()
    }

    pub fn title(&self) -> String {
        self.identity.snapshot.value.read().page_title.clone()
    }

    pub fn accent(&self) -> Accent {
        let snapshot = self.identity.snapshot.value.read();
        Accent {
            css: snapshot.accent_color.clone(),
            rgb: snapshot.accent_rgb.clone(),
        }
    }

    pub fn status(&self) -> String {
        self.run.status()
    }

    pub fn installing(&self) -> bool {
        self.identity.snapshot.value.read().installing
    }

    pub fn installing_splash(&self) -> bool {
        self.identity.snapshot.value.read().installing_splash
    }

    pub fn install_detail(&self) -> String {
        let detail = self.run.error();
        if detail.is_empty() {
            translate("agent-preparing")
        } else {
            detail
        }
    }

    pub fn draft(&self) -> String {
        self.root.read().composer_draft.clone()
    }

    pub fn filtered_commands(&self) -> Vec<SlashCommandEntry> {
        self.selector.value.read().commands.clone()
    }

    pub fn filtered_models(&self) -> Vec<ModelOptionEntry> {
        self.selector.value.read().models.clone()
    }

    pub fn filtered_mcp_servers(&self) -> Vec<vmux_api::mcp::McpServerEntry> {
        self.selector.value.read().mcp_servers.clone()
    }

    pub fn command_menu_open(&self) -> bool {
        self.selector.value.read().active == Some(ChatListKind::Command)
    }

    pub fn resume_menu_open(&self) -> bool {
        self.selector.value.read().active == Some(ChatListKind::Session)
    }

    pub fn model_menu_open(&self) -> bool {
        self.selector.value.read().active == Some(ChatListKind::Model)
    }

    pub fn mcp_menu_open(&self) -> bool {
        self.selector.value.read().active == Some(ChatListKind::Mcp)
    }

    pub fn selector_open(&self) -> bool {
        self.selector
            .value
            .read()
            .active
            .is_some_and(ChatListKind::is_selector)
    }

    pub fn resume_state(&self) -> Option<ResumeMenuState> {
        if !self.resume_menu_open() {
            return None;
        }
        let state = self.resume.state.value.read();
        Some(ResumeMenuState::resolve(
            state.active,
            state.loading,
            &state.query,
            state.rows.len(),
        ))
    }

    pub fn media_menu_open(&self) -> bool {
        self.selector.value.read().active == Some(ChatListKind::Media)
    }

    pub fn media_options(&self) -> Vec<vmux_api::prompt_media::PromptMediaOption> {
        self.root.read().composer_media.options.clone()
    }

    pub fn composer_attachments(&self) -> Vec<vmux_api::prompt_media::PromptComposerAttachment> {
        self.root.read().composer_media.attachments.clone()
    }

    pub fn streaming(&self) -> bool {
        self.identity.snapshot.value.read().streaming
    }

    pub fn prompt_mode(&self) -> PromptComposerMode {
        if self.streaming() && self.queue.snapshot.value.read().queued.is_empty() {
            PromptComposerMode::Stop
        } else {
            PromptComposerMode::Send
        }
    }

    pub fn prompt_action_title(&self) -> String {
        if self.streaming() && !self.queue.snapshot.value.read().queued.is_empty() {
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
                || !self
                    .composer
                    .attachments
                    .value
                    .read()
                    .attachments
                    .is_empty())
    }

    pub fn choice_pending(&self) -> bool {
        self.identity.snapshot.value.read().choice_pending
    }
}

impl Chat {
    pub fn model_chip(&self) -> Option<ComposerChip> {
        if !(self.models.state.ready)() {
            return Some(ComposerChip::loading());
        }
        let model = self.models.state.value.read();
        let name = model.current_model_name.clone();
        if name.is_empty() {
            return None;
        }
        let label = match model.current_model_id == model.default_model_id {
            true => translate_with(
                "agent-option-default",
                &[("value", TranslationValue::String(&name))],
            ),
            false => name,
        };
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.edit_draft("/model ".to_string());
            PromptFocus::end(PROMPT_INPUT_ID);
        });
        Some(ComposerChip::ready(label, translate("agent-change-model")).opens(open))
    }

    pub fn effort_chip(&self) -> Option<ComposerChip> {
        if !(self.models.state.ready)() {
            return Some(ComposerChip::loading());
        }
        let effort = self.effort.state.value.read();
        if effort.effort_levels.is_empty() {
            return None;
        }
        let selected = effort.effort_current.clone();
        let label = match selected.is_empty() {
            false => selected,
            true => translate_with(
                "agent-option-default",
                &[("value", TranslationValue::String(&effort.effort_default))],
            ),
        };
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.open_menu(ComposerMenuKind::Effort);
            PromptFocus::end(PROMPT_INPUT_ID);
        });
        Some(ComposerChip::ready(label, translate("agent-effort-tooltip")).opens(open))
    }

    pub fn permission_chip(&self) -> Option<ComposerChip> {
        let state = self.permissions.state.value.read();
        if state.modes.is_empty() {
            return None;
        }
        let current = state
            .modes
            .iter()
            .find(|mode| mode.id == state.current_mode_id);
        let label = current
            .map(|mode| mode.name.clone())
            .unwrap_or_else(|| state.current_mode_id.clone());
        let title = current
            .and_then(|mode| mode.description.clone())
            .filter(|description| !description.is_empty())
            .unwrap_or_else(|| translate("composer-permission-change"));
        let selected = state
            .modes
            .iter()
            .position(|mode| mode.id == state.current_mode_id)
            .unwrap_or(0);
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.open_menu_at(ComposerMenuKind::Permission, selected);
            PromptFocus::end(PROMPT_INPUT_ID);
        });
        Some(ComposerChip::ready(label, title).opens(open))
    }

    pub fn project_chip(&self) -> Option<ComposerChip> {
        if !(self.projects.context.ready)() {
            return Some(ComposerChip::loading());
        }
        let context = self.slash.context();
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
        if !(self.projects.context.ready)() {
            return Some(ComposerChip::loading());
        }
        let context = self.slash.context();
        if !context.is_git_repo {
            return None;
        }
        let chat = *self;
        let open = EventHandler::new(move |()| {
            chat.open_menu(ComposerMenuKind::Branch);
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
        let text = self.draft().trim().to_string();
        let selected = self.composer.attachments.value.peek().attachments.clone();
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

    pub fn select_mode(&self, mode_id: String) {
        let _ = send(&SelectMode { mode_id });
        PromptFocus::end(PROMPT_INPUT_ID);
    }

    pub fn choose_list(&self, index: usize) {
        let _ = send(&ChatListChooseRequest {
            index: index as u32,
        });
    }

    pub fn point_at_choice(&self, index: usize) {
        let _ = send(&ChatListSelectionChanged {
            index: index as u32,
        });
    }

    pub fn answer_approval(&self, call_id: String, decision: ApprovalDecision) {
        let _ = send(&ChatApproval { call_id, decision });
    }

    pub fn point_at_approval(&self, index: usize) {
        let _ = send(&ChatListSelectionChanged {
            index: index as u32,
        });
    }

    pub fn point_at_list(&self, index: usize) {
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
        let menu = match kind {
            ComposerMenuKind::Effort => ChatComposerMenuKind::Effort,
            ComposerMenuKind::Permission => ChatComposerMenuKind::Permission,
            ComposerMenuKind::Project => ChatComposerMenuKind::Project,
            ComposerMenuKind::Branch => ChatComposerMenuKind::Branch,
            ComposerMenuKind::Agent | ComposerMenuKind::Model => return,
        };
        let _ = send(&ChatComposerMenuRequest {
            menu: Some(menu),
            index: index as u32,
        });
    }

    pub fn dismiss_selector(&self) {
        let _ = send(&ChatDismissSelectorRequest);
    }

    pub fn edit_draft(&self, value: String) {
        let _ = send(&ChatComposerMenuRequest {
            menu: None,
            index: 0,
        });
        self.set_draft(value);
    }

    pub(crate) fn set_draft(&self, value: String) {
        if self.root.peek().composer_draft == value {
            return;
        }
        let _ = send(&ChatDraftChanged { text: value });
    }

    pub fn remove_attachment(&self, index: usize) {
        let attachments = self.composer.attachments.value.read();
        let Some(attachment) = attachments.attachments.get(index) else {
            return;
        };
        let _ = send(&ChatRemoveAttachment {
            path: attachment.path.clone(),
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Transcript {
    root: Signal<ChatUiState>,
    pub at_bottom: Signal<bool>,
    pub last_top: Signal<i32>,
    pub scroll_container: scroll::Container,
}

pub fn use_transcript(root: Signal<ChatUiState>) -> Transcript {
    Transcript {
        root,
        at_bottom: use_signal(|| true),
        last_top: use_signal(|| 0),
        scroll_container: use_signal(|| None),
    }
}

impl Transcript {
    pub fn current(self) -> ChatTranscriptState {
        self.root.read().transcript.clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RunState {
    snapshot: ChatValue<ChatSnapshot>,
}

impl RunState {
    pub fn status(self) -> String {
        if !(self.snapshot.ready)() {
            return "installing".to_string();
        }
        self.snapshot.value.read().status.clone()
    }

    pub fn error(self) -> String {
        self.snapshot.value.read().error.clone()
    }

    pub fn approval(self) -> Option<crate::event::PendingApproval> {
        let snapshot = self.snapshot.value.read();
        if snapshot.status != "awaiting" {
            return None;
        }
        snapshot.approval.clone()
    }

    pub fn choice_question(self) -> String {
        self.snapshot.value.read().choice_question.clone()
    }

    pub fn choice_options(self) -> Vec<String> {
        self.snapshot.value.read().choice_options.clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct AgentIdentity {
    snapshot: ChatValue<ChatSnapshot>,
}

impl AgentIdentity {
    pub fn id(self) -> String {
        self.snapshot.value.read().agent_id.clone()
    }

    pub fn icon(self) -> String {
        self.snapshot.value.read().agent_icon.clone()
    }

    pub fn accent(self) -> String {
        self.snapshot.value.read().accent_color.clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct UserIdentity {
    snapshot: ChatValue<ChatSnapshot>,
}

impl UserIdentity {
    pub fn name(self) -> String {
        let name = self.snapshot.value.read().user_name.clone();
        if name.is_empty() {
            translate("team-you")
        } else {
            name
        }
    }

    pub fn color(self) -> String {
        let color = self.snapshot.value.read().user_color.clone();
        if color.is_empty() {
            "#3b82f6".to_string()
        } else {
            color
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Handoff {
    snapshot: ChatValue<ChatSnapshot>,
}

impl Handoff {
    pub fn boundary(&self, message_index: usize) -> bool {
        ImportedMessages::new(self.snapshot.value.read().handoff_message_count)
            .boundary(message_index)
    }

    pub fn source(self) -> String {
        self.snapshot.value.read().handoff_source.clone()
    }

    pub fn truncated(self) -> bool {
        self.snapshot.value.read().handoff_truncated
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ComposerDraft {
    pub effect_revision: Signal<u64>,
    pub focus_revision: Signal<u64>,
    pub attachments: ChatValue<ChatAttachments>,
}

pub fn use_composer_draft(attachments: ChatValue<ChatAttachments>) -> ComposerDraft {
    ComposerDraft {
        effect_revision: use_signal(|| 0),
        focus_revision: use_signal(|| 0),
        attachments,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PromptQueue {
    snapshot: ChatValue<ChatSnapshot>,
}

impl PromptQueue {
    pub fn queued(self) -> Vec<QueuedPromptSnapshot> {
        self.snapshot.value.read().queued.clone()
    }

    pub fn paused(self) -> bool {
        self.snapshot.value.read().paused
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct MediaPicker {
    state: ChatValue<ChatMediaState>,
}

impl MediaPicker {
    pub fn current(self) -> ChatMediaState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ModelPicker {
    state: ChatValue<crate::event::ModelState>,
}

impl ModelPicker {
    pub fn current(self) -> crate::event::ModelState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ProjectPicker {
    context: ChatValue<ComposerContext>,
    branches: ChatValue<ChatBranchesState>,
}

impl ProjectPicker {
    pub fn context_ready(self) -> bool {
        (self.context.ready)()
    }

    pub fn branches(self) -> ChatBranchesState {
        self.branches.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct EffortPicker {
    state: ChatValue<crate::event::ModelState>,
}

impl EffortPicker {
    pub fn current(self) -> crate::event::ModelState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PermissionPicker {
    state: ChatValue<crate::event::ModeState>,
}

impl PermissionPicker {
    pub fn current(self) -> crate::event::ModeState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SlashCommands {
    context: ChatValue<ComposerContext>,
}

impl SlashCommands {
    pub fn context(self) -> ComposerContext {
        self.context.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Resume {
    state: ChatValue<ChatResumeState>,
}

impl Resume {
    pub fn current(self) -> ChatResumeState {
        self.state.value.read().clone()
    }
}
