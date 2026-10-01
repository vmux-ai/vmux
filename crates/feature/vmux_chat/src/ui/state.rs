use super::format::{ChatPageTitle, ImportedMessages, ResumeMenuState};
use super::scroll;
use crate::event::ChatResumeState;
use crate::event::{
    ApprovalDecision, ChatApproval, ChatAttachments, ChatBranchesRequest, ChatBranchesState,
    ChatComposerEffect, ChatComposerMenuChanged, ChatComposerMenuKind, ChatComposerMenuState,
    ChatDraftChanged, ChatHistoryMoreRequest, ChatListChooseRequest, ChatListKind,
    ChatListSelectionChanged, ChatListSelectionState, ChatMediaState, ChatRemoveAttachment,
    ChatSelectorState, ChatSnapshot, ChatStop, ChatSubmit, ChatTranscriptState, ComposerContext,
    ModelOptionEntry, QueuedPromptSnapshot, SelectMode, SlashCommandEntry,
};
use crate::state::ChatUiState;
use crate::tab::Accent;
use dioxus::prelude::*;
use vmux_api::prompt_media::{
    PromptComposerAttachment, PromptMediaOption, inline_media_query, replace_inline_media_query,
};
use vmux_core::prompt_media::MediaPath;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposerMode, focus_prompt_end};
use vmux_ui::components::composer_bar::{
    ComposerChip, ComposerMenu, ComposerMenuKind, use_composer_menu,
};
use vmux_ui::components::mcp_menu::{McpConnections, use_mcp_connections};
use vmux_ui::file_icon::FilePath;
use vmux_ui::hooks::{UiStateBinding, UiStateValue, send, use_selector, use_theme, use_ui_state};
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
    pub selector: UiStateValue<ChatSelectorState>,
    pub menu: ComposerMenu,
}

pub fn use_chat() -> Chat {
    use_theme();
    let ui = use_ui_state::<ChatUiState>();
    let snapshot = ui.use_value::<ChatSnapshot>();
    let composer_context = ui.use_value::<ComposerContext>();
    let mode = ui.use_value::<crate::event::ModeState>();
    let model = ui.use_value::<crate::event::ModelState>();
    let attachments = ui.use_value::<ChatAttachments>();
    let media = ui.use_value::<ChatMediaState>();
    let branches = ui.use_value::<ChatBranchesState>();
    let resume = ui.use_value::<ChatResumeState>();
    let selector = ui.use_value::<ChatSelectorState>();
    let chat = Chat {
        agent: use_signal(CurrentAgent::read),
        transcript: use_transcript(ui),
        run: use_run_state(snapshot),
        identity: AgentIdentity { snapshot },
        user: use_user_identity(snapshot),
        handoff: Handoff { snapshot },
        composer: use_composer_draft(attachments),
        queue: PromptQueue { snapshot },
        media: MediaPicker { state: media },
        mcp: use_mcp_connections(),
        models: ModelPicker { state: model },
        effort: EffortPicker { state: model },
        permissions: PermissionPicker { state: mode },
        projects: ProjectPicker {
            context: composer_context,
            branches,
        },
        slash: use_slash_commands(composer_context),
        resume: Resume { state: resume },
        selector,
        menu: use_composer_menu(),
    };
    chat.listen(ui);
    chat.watch();
    chat
}

impl Chat {
    fn listen(&self, ui: UiStateBinding<ChatUiState>) {
        let chat = *self;
        let mut previous_snapshot = use_signal(ChatSnapshot::default);
        ui.use_updates::<ChatSnapshot>(move |snapshot| {
            let previous = previous_snapshot.peek();
            let choices_changed = previous.choice_options != snapshot.choice_options;
            let approval_changed = {
                let previous_approval = if previous.status == "awaiting" {
                    previous.approval.as_ref()
                } else {
                    None
                };
                let next_approval = if snapshot.status == "awaiting" {
                    snapshot.approval.as_ref()
                } else {
                    None
                };
                previous_approval != next_approval
            };
            drop(previous);
            if choices_changed {
                set_if_changed(chat.slash.menu_sel, 0);
            }
            if approval_changed {
                set_if_changed(chat.run.approval_sel, 0);
            }
            previous_snapshot.set(snapshot);
        });
        let chat = *self;
        ui.use_updates::<crate::event::ModelState>(move |_| {
            set_if_changed(chat.slash.menu_sel, 0);
        });
        let chat = *self;
        ui.use_updates::<ChatMediaState>(move |_| {
            set_if_changed(chat.slash.menu_sel, 0);
        });
        let chat = *self;
        ui.use_updates::<ChatResumeState>(move |_| {
            set_if_changed(chat.slash.menu_sel, 0);
        });
        let chat = *self;
        ui.use_updates::<ChatComposerEffect>(move |effect| {
            chat.apply_composer_effect(&effect);
        });
        let chat = *self;
        ui.use_updates::<crate::event::ChatPromptFocusEffect>(move |effect| {
            if effect.revision <= *chat.composer.focus_revision.peek() {
                return;
            }
            let mut focus_revision = chat.composer.focus_revision;
            focus_revision.set(effect.revision);
            focus_prompt_end(PROMPT_INPUT_ID);
        });
        let chat = *self;
        ui.use_updates::<ChatListSelectionState>(move |selection| {
            chat.apply_list_selection(selection);
        });
        let chat = *self;
        ui.use_updates::<ChatComposerMenuState>(move |menu| {
            chat.apply_composer_menu(&menu);
        });
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
            let _ = chat.transcript.state.read().items.len();
            let _ = chat.run.status();
            if !*chat.transcript.at_bottom.peek() {
                return;
            }
            scroll::to_bottom(chat.transcript.scroll_container);
        });
        use_selector(chat.slash.menu_sel, move |selected| {
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
        });
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
        let name = self.identity.name();
        if name.is_empty() { self.agent() } else { name }
    }

    pub fn title(&self) -> String {
        ChatPageTitle::resolve(&self.identity.title(), &self.header_name())
    }

    pub fn accent(&self) -> Accent {
        Accent::resolve(
            &self.identity.accent(),
            agent_accent(&self.agent()).rain_rgb,
        )
    }

    pub fn status(&self) -> String {
        self.run.status()
    }

    pub fn installing(&self) -> bool {
        self.status() == "installing"
    }

    pub fn installing_splash(&self) -> bool {
        self.installing() && self.transcript.state.read().items.is_empty()
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
        (self.composer.draft)()
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

    pub fn media_options(&self) -> Vec<PromptMediaOption> {
        let mut options = Vec::new();
        for entry in &self.media.state.value.read().entries {
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
        let attachments = self.composer.attachments.value.read();
        let mut rendered = Vec::with_capacity(attachments.attachments.len());
        for (index, attachment) in attachments.attachments.iter().enumerate() {
            rendered.push(PromptComposerAttachment {
                key: format!("attachment-{}", attachment.path),
                name: attachment.name.clone(),
                label: FilePath(&attachment.name).extension_label(),
                preview_data_url: attachment.preview_data_url.clone(),
                remove_index: Some(index as u32),
            });
        }
        rendered
    }

    pub fn streaming(&self) -> bool {
        matches!(self.status().as_str(), "streaming" | "awaiting")
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
        !self.run.choice_options().is_empty() || self.run.approval().is_some()
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
            let mut menu_sel = chat.slash.menu_sel;
            chat.menu.close();
            chat.set_draft("/model ".to_string());
            menu_sel.set(0);
            focus_prompt_end(PROMPT_INPUT_ID);
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
            focus_prompt_end(PROMPT_INPUT_ID);
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
            focus_prompt_end(PROMPT_INPUT_ID);
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
        focus_prompt_end(PROMPT_INPUT_ID);
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
    pub state: Signal<ChatTranscriptState>,
    pub at_bottom: Signal<bool>,
    pub last_top: Signal<i32>,
    pub scroll_container: scroll::Container,
}

pub fn use_transcript(ui: UiStateBinding<ChatUiState>) -> Transcript {
    let transcript = Transcript {
        state: use_signal(ChatTranscriptState::default),
        at_bottom: use_signal(|| true),
        last_top: use_signal(|| 0),
        scroll_container: use_signal(|| None),
    };
    let current = transcript;
    ui.use_updates::<ChatTranscriptState>(move |state| current.apply(state));
    transcript
}

impl Transcript {
    fn apply(self, state: ChatTranscriptState) {
        let previous = self.state.peek();
        let preserve_scroll = previous.generation == state.generation
            && previous.prepend_revision != state.prepend_revision;
        let metrics = if preserve_scroll {
            scroll::metrics(self.scroll_container)
        } else {
            None
        };
        drop(previous);
        let mut current = self.state;
        current.set(state);
        if let Some((height, top)) = metrics {
            scroll::restore(self.scroll_container, height, top);
        }
    }

    pub fn current(self) -> ChatTranscriptState {
        self.state.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RunState {
    snapshot: UiStateValue<ChatSnapshot>,
    pub approval_sel: Signal<usize>,
}

pub fn use_run_state(snapshot: UiStateValue<ChatSnapshot>) -> RunState {
    RunState {
        snapshot,
        approval_sel: use_signal(|| 0),
    }
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
    snapshot: UiStateValue<ChatSnapshot>,
}

impl AgentIdentity {
    pub fn name(self) -> String {
        self.snapshot.value.read().agent_name.clone()
    }

    pub fn title(self) -> String {
        self.snapshot.value.read().conversation_title.clone()
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
    snapshot: UiStateValue<ChatSnapshot>,
}

pub fn use_user_identity(snapshot: UiStateValue<ChatSnapshot>) -> UserIdentity {
    UserIdentity { snapshot }
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
    snapshot: UiStateValue<ChatSnapshot>,
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
    pub draft: Signal<String>,
    pub effect_revision: Signal<u64>,
    pub focus_revision: Signal<u64>,
    pub attachments: UiStateValue<ChatAttachments>,
}

pub fn use_composer_draft(attachments: UiStateValue<ChatAttachments>) -> ComposerDraft {
    ComposerDraft {
        draft: use_signal(String::new),
        effect_revision: use_signal(|| 0),
        focus_revision: use_signal(|| 0),
        attachments,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PromptQueue {
    snapshot: UiStateValue<ChatSnapshot>,
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
    state: UiStateValue<ChatMediaState>,
}

impl MediaPicker {
    pub fn current(self) -> ChatMediaState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ModelPicker {
    state: UiStateValue<crate::event::ModelState>,
}

impl ModelPicker {
    pub fn current(self) -> crate::event::ModelState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ProjectPicker {
    context: UiStateValue<ComposerContext>,
    branches: UiStateValue<ChatBranchesState>,
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
    state: UiStateValue<crate::event::ModelState>,
}

impl EffortPicker {
    pub fn current(self) -> crate::event::ModelState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct PermissionPicker {
    state: UiStateValue<crate::event::ModeState>,
}

impl PermissionPicker {
    pub fn current(self) -> crate::event::ModeState {
        self.state.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SlashCommands {
    pub menu_sel: Signal<usize>,
    context: UiStateValue<ComposerContext>,
}

pub fn use_slash_commands(context: UiStateValue<ComposerContext>) -> SlashCommands {
    SlashCommands {
        menu_sel: use_signal(|| 0),
        context,
    }
}

impl SlashCommands {
    pub fn context(self) -> ComposerContext {
        self.context.value.read().clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Resume {
    state: UiStateValue<ChatResumeState>,
}

impl Resume {
    pub fn current(self) -> ChatResumeState {
        self.state.value.read().clone()
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
