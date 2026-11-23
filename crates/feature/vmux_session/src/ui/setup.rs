use super::state::Chat;
use crate::onboarding::OnboardingUiState;
use crate::onboarding::{
    ChatAuthenticate, ChatCancelSetup, ChatSaveKey, KeyEntryUiState, OnboardingChooseAgent,
};
use dioxus::prelude::*;
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::composer::{
    ComposerButton, ComposerFrame, ComposerNotice, PromptComposer, PromptComposerMode, ShortcutHint,
};
use vmux_ui::components::composer_bar::{ComposerBar, ComposerChip};
use vmux_ui::components::message::MessageFrame;
use vmux_ui::components::progress::{Progress, ProgressIndicator, TaskSteps};
use vmux_ui::components::prompt::{
    ConversationPrompt, PromptOption, PromptSelection, PromptSelectionChange,
};
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[component]
pub(super) fn Turn(#[props(default)] agent_name: String, children: Element) -> Element {
    rsx! {
        MessageFrame {
            name: if agent_name.is_empty() { Identity::name() } else { translate_with("onboarding-setup-agent", &[("agent", TranslationValue::String(&agent_name))]) },
            name_icon: (!agent_name.is_empty()).then(|| rsx! { AgentThumbnail { src: None, seed: agent_name.clone(), background: String::new(), alt: String::new(), class: "size-3.5".to_string() } }),
            seed: "vmux-setup-agent".to_string(),
            reserve_metadata: false,
            {children}
        }
    }
}

#[component]
pub(super) fn Welcome(
    #[props(default)] agent_name: String,
    #[props(default)] chat: Option<Chat>,
    #[props(default)] onboarding: OnboardingUiState,
    #[props(default = rsx! {})] children: Element,
) -> Element {
    let task = Task::of(chat, &onboarding);
    rsx! {
        div { class: "onboarding-origin",
            Turn { agent_name: agent_name.clone(),
                p { class: "text-sm leading-relaxed", {if agent_name.is_empty() { translate("onboarding-start-description") } else { translate_with("onboarding-start-agent-description", &[("agent", TranslationValue::String(&agent_name))]) }} }
                TaskSteps { steps: task.steps, completed_steps: task.completed, pending_step: task.pending, current_step: Some(task.current) }
                if onboarding.paused { p { role: "status", class: "text-sm text-muted-foreground", {task.message} } }
                {children}
            }
        }
    }
}

#[component]
pub(super) fn ConnectedWelcome(chat: Chat) -> Element {
    let state = vmux_ui::hooks::use_ui_state::<OnboardingUiState>()
        .state
        .read()
        .clone();
    rsx! { Welcome { agent_name: chat.header_name(), chat: Some(chat), onboarding: state } }
}

#[component]
pub(super) fn Controls(chat: Chat) -> Element {
    vmux_ui::hooks::use_ui_state::<OnboardingUiState>();
    let onboarding = UiStateBinding::<OnboardingUiState>::use_context()
        .state
        .read()
        .clone();
    let keys = UiStateBinding::<KeyEntryUiState>::use_context()
        .state
        .read()
        .clone();
    let connection = Connection::of(chat, !keys.key_variable.is_empty());
    let selected = use_signal(|| "connect".to_string());
    let resumable = onboarding.paused || chat.status() == "errored";
    let busy =
        chat.authentication_pending() || chat.setup_busy() && chat.status() != "authentication";
    let choosing =
        !resumable && !busy && chat.status() == "authentication" && !chat.key_input_requested();
    let notice =
        choosing.then(|| rsx! { ConnectionControls { chat, keys: keys.clone(), selected } });
    let action = if resumable {
        Some(
            rsx! { ComposerButton { action_title: translate("common-resume"), label: Some(translate("common-resume")),
                icon: Some(vmux_ui::icon::LineIcon::Play), icon_class: "[&_path]:fill-current".to_string(), enabled: !keys.pending,
                on_action: move |_| Choice::Retry.choose(),
            } },
        )
    } else if busy {
        Some(
            rsx! { ComposerButton { mode: PromptComposerMode::Stop, action_title: translate("common-stop"), on_action: move |_| Choice::Cancel.choose() } },
        )
    } else if choosing {
        let available = if selected() == "key" {
            connection.key
        } else {
            connection.method.is_some()
        };
        Some(
            rsx! { ComposerButton { action_title: translate("common-continue"), label: Some(translate("common-continue")),
                icon: Some(vmux_ui::icon::LineIcon::Play), icon_class: "[&_path]:fill-current".to_string(), enabled: available && !keys.pending,
                on_action: move |_| { if selected() == "key" { chat.request_key_input(true); } else { connection.connect(chat); } },
            } },
        )
    } else {
        None
    };
    let on_choose = (!keys.pending && !chat.authentication_pending())
        .then_some(EventHandler::new(move |_| Choice::ChangeAgent.choose()));
    let key_entry = !resumable
        && !busy
        && chat.status() == "authentication"
        && chat.key_input_requested()
        && !keys.key_variable.is_empty();
    rsx! { Frame { notice, onboarding,
        if key_entry {
            SetupComposer { chat, keys }
            DisabledComposerBar { chat: Some(chat), on_choose }
        } else {
            DisabledComposer { chat: Some(chat), on_choose, action }
        }
    } }
}

#[component]
pub(super) fn Frame(
    #[props(default)] notice: Option<Element>,
    #[props(default)] compact: bool,
    #[props(default)] onboarding: OnboardingUiState,
    children: Element,
) -> Element {
    rsx! {
        ComposerFrame {
            borderless: true,
            content_class: if compact { "p-0".to_string() } else { String::new() },
            notice: Some(rsx! { ComposerNotice { content: notice.map(|notice| rsx! { div { class: "setup-reply p-2", {notice} } }), class: "setup-notice".to_string() } }),
            div { aria_busy: onboarding.starting, {children} }
        }
    }
}

#[component]
pub(super) fn DisabledComposer(
    #[props(default)] chat: Option<Chat>,
    #[props(default)] chip: Option<ComposerChip>,
    #[props(default)] agent_id: String,
    #[props(default)] on_choose: Option<EventHandler<()>>,
    #[props(default)] action: Option<Element>,
) -> Element {
    let agent_id = chat.map(|chat| chat.agent()).unwrap_or(agent_id);
    let accent = vmux_ui::agent_accent::AgentAccent::for_agent(&agent_id);
    rsx! {
        PromptComposer {
            shared_transition: true, input_disabled: true, caret_visible: false,
            value: String::new(), placeholder: translate("onboarding-composer-wait"),
            accent_color: chat.map(|chat| chat.accent().css).unwrap_or_else(|| Identity::COLOR.to_string()), accent_gradient: accent.grad.to_string(),
            action, action_title: translate("common-continue"), action_enabled: false,
            footer: Some(rsx! { DisabledComposerBar { chat, chip, on_choose } }),
            on_input: move |_| {}, on_keydown: move |_| {}, on_paste: move |_| {},
            on_attach: move |_| {}, on_remove_attachment: move |_| {}, on_action: move |_| {},
        }
    }
}

#[component]
fn DisabledComposerBar(
    #[props(default)] chat: Option<Chat>,
    #[props(default)] chip: Option<ComposerChip>,
    #[props(default)] on_choose: Option<EventHandler<()>>,
) -> Element {
    let chip = chip
        .or_else(|| chat.map(|chat| chat.agent_chip()))
        .map(|mut chip| {
            if let Some(on_choose) = on_choose {
                chip.title = Choice::ChangeAgent.label();
                chip = chip.opens(on_choose);
            }
            chip
        });
    rsx! { ComposerBar { opened: None, agent: chip, compact: false } }
}

struct Task {
    steps: Vec<String>,
    completed: Vec<usize>,
    pending: Option<usize>,
    current: usize,
    message: String,
}

impl Task {
    fn of(chat: Option<Chat>, state: &OnboardingUiState) -> Self {
        let agent = state.agents.iter().find(|agent| agent.id == state.agent_id);
        let mut name = String::new();
        if (state.starting
            || state.paused
            || state.failed
            || !state.session_id.is_empty()
            || chat.is_some())
            && let Some(agent) = agent
        {
            name.clone_from(&agent.name);
        }
        if name.is_empty()
            && let Some(chat) = chat
        {
            name = chat.header_name();
        }
        let mut steps = Vec::new();
        for key in [
            "onboarding-checklist-select",
            "onboarding-checklist-install",
            "onboarding-checklist-connect",
        ] {
            steps.push(translate(key));
        }
        let status = chat.map(|chat| chat.status()).unwrap_or_default();
        let mut completed = Vec::new();
        if chat.is_some() || state.starting || state.paused || !state.session_id.is_empty() {
            completed.push(0);
        }
        if agent.is_some_and(|agent| agent.installed)
            || matches!(status.as_str(), "authentication" | "idle" | "streaming")
        {
            completed.push(1);
        }
        if chat.is_some_and(|chat| !chat.setup_pending())
            && matches!(status.as_str(), "idle" | "streaming")
        {
            completed.push(2);
        }
        let pending = if state.paused || status == "errored" {
            None
        } else if chat.is_some_and(|chat| chat.authentication_pending()) {
            Some(2)
        } else if chat.is_some_and(|chat| chat.setup_busy()) {
            Some(1)
        } else if state.starting {
            Some(0)
        } else {
            None
        };
        let message = if state.paused {
            Self::message("onboarding-setup-paused-agent", &name)
        } else if chat.is_some_and(|chat| chat.authentication_pending()) {
            Self::message("onboarding-login-finish-agent", &name)
        } else {
            String::new()
        };
        let current =
            pending.unwrap_or_else(|| (0..3).find(|index| !completed.contains(index)).unwrap_or(2));
        Self {
            steps,
            completed,
            pending,
            current,
            message,
        }
    }
    fn message(key: &str, name: &str) -> String {
        translate_with(key, &[("agent", TranslationValue::String(name))])
    }
}

#[component]
fn ConnectionControls(chat: Chat, keys: KeyEntryUiState, mut selected: Signal<String>) -> Element {
    let connection = Connection::of(chat, !keys.key_variable.is_empty());
    let options = vec![
        PromptOption {
            value: "connect".into(),
            label: translate("onboarding-connect"),
            description: translate("onboarding-connect-description"),
            disabled: connection.method.is_none(),
        },
        PromptOption {
            value: "key".into(),
            label: translate("onboarding-use-key"),
            description: translate("onboarding-use-key-description"),
            disabled: !connection.key,
        },
    ];
    rsx! {
        ConversationPrompt {
            question: Some(translate("onboarding-connection-choice")),
            PromptSelection { numbered: true, label: translate("onboarding-auth-question"), options, selected: vec![selected()], disabled: keys.pending,
                on_change: move |change: PromptSelectionChange| selected.set(change.value),
            }
        }
    }
}

#[derive(Clone)]
struct Connection {
    method: Option<vmux_api::protocol::AuthenticationMethod>,
    key: bool,
}

impl Connection {
    fn of(chat: Chat, key: bool) -> Self {
        let methods = chat.authentication_methods();
        let mut chosen = None;
        for method in &methods {
            if method.id == "api-key" {
                continue;
            }
            if chosen.is_none() || method.id == "chat-gpt" {
                chosen = Some(method.clone());
            }
            if method.id == "chat-gpt" {
                break;
            }
        }
        if chosen.is_none() {
            chosen = methods.first().cloned();
        }
        Self {
            method: chosen,
            key,
        }
    }

    fn connect(&self, chat: Chat) {
        if let Some(method) = &self.method {
            chat.request_key_input(false);
            let _ = send(&ChatAuthenticate {
                method_id: method.id.clone(),
            });
        } else if self.key {
            chat.request_key_input(true);
        }
    }
}

#[component]
fn SetupComposer(chat: Chat, keys: KeyEntryUiState) -> Element {
    if chat.status() == "authentication"
        && chat.key_input_requested()
        && !keys.key_variable.is_empty()
    {
        return rsx! {
            vmux_ui::components::api_key::ApiKeyEntry {
                pending: keys.pending,
                failed: keys.failed,
                accent_color: chat.accent().css,
                footer: Some(rsx! { ShortcutHint { shortcut: "Esc", label: translate("common-cancel") } }),
                on_stop: Some(EventHandler::new(move |_| chat.stop_or_flush())),
                on_submit: move |key| { let _ = send(&ChatSaveKey { key }); },
            }
        };
    }
    rsx! {}
}

#[component]
pub(super) fn ProgressTurn(
    phase: Phase,
    #[props(default)] agent_name: String,
    #[props(default)] detail: String,
    #[props(default)] percent: Option<u8>,
) -> Element {
    let label = phase.label();
    let detail = detail.trim();
    let show_detail =
        !detail.is_empty() && detail != label && detail != translate("agent-preparing");
    rsx! {
        Turn { agent_name,
            div { role: "status", aria_live: "polite",
                span { "{label}" }
            }
            if show_detail { p { class: "text-xs text-muted-foreground", "{detail}" } }
            if phase == Phase::Installation {
                div { class: "w-full max-w-xs",
                    Progress { value: percent.map(|value| f64::from(value.min(100))), max: 100.0, aria_label: translate("setup-installing"),
                        ProgressIndicator {}
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Phase {
    Preparation,
    Installation,
}

impl Phase {
    fn label(self) -> String {
        translate(match self {
            Self::Preparation => "agent-preparing",
            Self::Installation => "setup-installing",
        })
    }
}

#[derive(Clone, Copy)]
enum Choice {
    Retry,
    Cancel,
    ChangeAgent,
}

impl Choice {
    fn label(&self) -> String {
        match self {
            Self::Retry => translate("common-retry"),
            Self::Cancel => translate("common-cancel"),
            Self::ChangeAgent => translate("composer-choose-agent"),
        }
    }

    fn choose(&self) {
        match self {
            Self::Retry => {
                let _ = send(&crate::event::ChatRetrySession);
            }
            Self::Cancel => {
                let _ = send(&ChatCancelSetup);
            }
            Self::ChangeAgent => {
                let _ = send(&OnboardingChooseAgent);
            }
        }
    }
}

pub(super) struct Identity;

impl Identity {
    pub(super) const COLOR: &str = "#6366f1";

    pub(super) fn name() -> String {
        translate("onboarding-guide")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ChatSnapshot;
    use crate::state::ChatUiState;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedFormData, SerializedHtmlEventConverter,
        SerializedKeyboardData, SerializedMouseData,
    };
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, PageHost};

    #[derive(Default)]
    struct Dom {
        inert: std::collections::HashSet<dioxus::core::ElementId>,
    }

    impl Dom {
        fn notice_attributes(html: &str) -> &str {
            for node in html.split('<') {
                let Some(attributes) = node.strip_prefix("div ") else {
                    continue;
                };
                let attributes = attributes.split('>').next().unwrap();
                if attributes.contains(" aria-hidden=") {
                    return attributes;
                }
            }
            panic!("the notice must expose its interaction state");
        }

        fn notice_hidden(attributes: &str) -> bool {
            attributes.contains("aria-hidden=\"true\"") && attributes.contains(" inert")
        }

        fn apply(&mut self, edits: &[Mutation]) {
            for edit in edits {
                if let Mutation::SetAttribute {
                    name: "inert",
                    value,
                    id,
                    ..
                } = edit
                {
                    if matches!(value, AttributeValue::None) {
                        self.inert.remove(id);
                    } else {
                        self.inert.insert(*id);
                    }
                }
            }
        }

        fn blocked(&self) -> bool {
            !self.inert.is_empty()
        }
    }

    #[derive(Default)]
    struct Host(RefCell<Vec<String>>);

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == ChatAuthenticate::id() {
                let request =
                    rkyv::from_bytes::<ChatAuthenticate, rkyv::rancor::Error>(bytes).unwrap();
                self.0.borrow_mut().push(request.method_id);
            } else if id == ChatSaveKey::id() {
                let request = rkyv::from_bytes::<ChatSaveKey, rkyv::rancor::Error>(bytes).unwrap();
                assert_eq!(request.key.expose(), "private-test-api-key");
                self.0.borrow_mut().push("key".into());
            } else if id == crate::event::ChatSubmit::id()
                || id == crate::event::ChatDraftChanged::id()
            {
                panic!("credential input must not enter the chat draft or prompt path");
            } else if id == crate::event::ChatRetrySession::id() {
                self.0.borrow_mut().push("retry".into());
            } else if id == crate::event::ChatStop::id() {
                self.0.borrow_mut().push("stop".into());
            } else if id == ChatCancelSetup::id() {
                self.0.borrow_mut().push("cancel".into());
            } else if id == OnboardingChooseAgent::id() {
                self.0.borrow_mut().push("choose-agent".into());
            } else if id == vmux_api::command_bar::OpenRequest::id() {
                let request = rkyv::from_bytes::<
                    vmux_api::command_bar::OpenRequest,
                    rkyv::rancor::Error,
                >(bytes)
                .unwrap();
                self.0.borrow_mut().push(request.value);
            }
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[component]
    fn Login(
        controls: bool,
        #[props(default)] pending: bool,
        #[props(default = "authentication".to_string())] status: String,
        #[props(default)] key_available: bool,
        #[props(default)] persisted_key_entry: bool,
    ) -> Element {
        let mut state = vmux_ui::hooks::use_ui_state::<ChatUiState>().state;
        use_hook(move || {
            state.set(ChatUiState {
                snapshot: ChatSnapshot {
                    status,
                    agent_id: "mistral-vibe".into(),
                    agent_name: "Test ACP".into(),
                    header_name: "Test ACP".into(),
                    authentication_pending: pending,
                    authentication_methods: vec![
                        vmux_api::protocol::AuthenticationMethod {
                            id: "api-key".into(),
                            name: "API Key".into(),
                            description: String::new(),
                        },
                        vmux_api::protocol::AuthenticationMethod {
                            id: "chat-gpt".into(),
                            name: "ChatGPT".into(),
                            description: String::new(),
                        },
                    ],
                    ..Default::default()
                },
                snapshot_ready: true,
                ..Default::default()
            })
        });
        let mut key_state = vmux_ui::hooks::use_ui_state::<KeyEntryUiState>().state;
        use_hook(move || {
            if key_available {
                key_state.set(KeyEntryUiState {
                    key_variable: "OPENAI_API_KEY".into(),
                    ..Default::default()
                });
            }
        });
        let mut setup = vmux_ui::hooks::use_ui_state::<OnboardingUiState>().state;
        use_hook(move || {
            if persisted_key_entry {
                setup.set(OnboardingUiState {
                    session_id: "login-session".into(),
                    key_entry_requested: true,
                    ..Default::default()
                });
            }
        });
        let chat = super::super::state::use_chat("login-session".into());
        if controls {
            rsx! { super::super::composer::ChatDock { chat } }
        } else {
            rsx! { super::super::authentication::Authentication { chat } }
        }
    }

    #[test]
    fn active_authentication_suspends_the_password_response_and_keeps_stop_available() {
        let mut dom = VirtualDom::new_with_props(
            Login,
            LoginProps {
                controls: true,
                pending: true,
                status: "authentication".into(),
                key_available: true,
                persisted_key_entry: true,
            },
        );
        dom.provide_root_context(HostBinding(Rc::new(Host::default())));
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(&translate("common-stop")));
        assert!(!html.contains("type=\"password\""));
    }

    #[test]
    fn persisted_key_entry_reopens_the_password_response_without_authenticating_again() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(
            Login,
            LoginProps {
                controls: true,
                pending: false,
                status: "authentication".into(),
                key_available: true,
                persisted_key_entry: true,
            },
        );
        dom.provide_root_context(HostBinding(host.clone()));
        dom.rebuild_in_place();
        assert!(dioxus_ssr::render(&dom).contains("type=\"password\""));
        assert!(host.0.borrow().is_empty());
    }

    #[test]
    fn authentication_defaults_to_connect_when_a_key_can_also_be_entered() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(
            Login,
            LoginProps {
                persisted_key_entry: false,
                controls: true,
                pending: false,
                status: "authentication".into(),
                key_available: true,
            },
        );
        dom.provide_root_context(HostBinding(host.clone()));
        let initial = dom.rebuild_to_vec();
        let button = initial
            .edits
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &translate("common-continue") => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            button,
        );
        dom.render_immediate_to_vec();
        assert_eq!(*host.0.borrow(), ["chat-gpt"]);
        assert!(!dioxus_ssr::render(&dom).contains("type=\"password\""));
    }

    #[test]
    fn authentication_is_a_guide_message_and_sign_in_is_a_composer_reply() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut message = VirtualDom::new_with_props(
            Login,
            LoginProps {
                persisted_key_entry: false,
                controls: false,
                pending: false,
                status: "authentication".into(),
                key_available: false,
            },
        );
        message.provide_root_context(HostBinding(host.clone()));
        message.rebuild_in_place();
        let html = dioxus_ssr::render(&message);
        assert!(html.contains(&translate("onboarding-auth-question")));
        assert!(!html.contains("role=\"listbox\""));
        assert!(!html.contains("Sign in with browser"));
        let mut composer = VirtualDom::new_with_props(
            Login,
            LoginProps {
                persisted_key_entry: false,
                controls: true,
                pending: false,
                status: "authentication".into(),
                key_available: false,
            },
        );
        composer.provide_root_context(HostBinding(host.clone()));
        let initial = composer.rebuild_to_vec();
        let html = dioxus_ssr::render(&composer);
        assert!(!html.contains("role=\"listbox\""));
        assert!(!html.contains("ChatGPT"));
        assert!(!html.contains("API Key"));
        let option = initial
            .edits
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &translate("common-continue") => Some(*id),
                _ => None,
            })
            .expect("the provider sign-in method is selectable in the composer");
        composer.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            option,
        );
        composer.render_immediate_to_vec();
        assert_eq!(*host.0.borrow(), ["chat-gpt"]);
    }

    #[test]
    fn busy_setup_keeps_disabled_composer_with_shared_stop() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        for (status, pending, key_available) in [
            ("preparing", false, false),
            ("installing", false, false),
            ("authentication", true, false),
            ("authentication", true, true),
        ] {
            let host = Rc::new(Host::default());
            let mut dom = VirtualDom::new_with_props(
                Login,
                LoginProps {
                    persisted_key_entry: false,
                    controls: true,
                    pending,
                    status: status.into(),
                    key_available,
                },
            );
            dom.provide_root_context(HostBinding(host.clone()));
            let initial = dom.rebuild_to_vec();
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains(&translate("common-stop")));
            assert!(initial.edits.iter().any(|edit| matches!(edit,
                Mutation::SetAttribute { name: "aria-label" | "title", value: AttributeValue::Text(value), .. } if value == &translate("common-stop"))));
            assert!(!html.contains(&translate("common-retry")));
            let choose_label = format!("aria-label=\"{}\"", translate("composer-choose-agent"));
            for button in html.split("<button").skip(1) {
                let attributes = button.split('>').next().unwrap();
                if attributes.contains(&choose_label) {
                    assert!(
                        !attributes.contains(" disabled=") && !attributes.ends_with(" disabled")
                    );
                    assert!(!attributes.contains("role=\"checkbox\""));
                }
            }
            if status != "authentication" {
                assert!(!html.contains("role=\"listbox\""));
            }
            assert!(!html.contains("type=\"password\""));
            let field = html
                .split("<textarea")
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(field.contains(" disabled"));
            assert!(html.contains("<img"));
            let stop = initial
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &translate("common-stop") => Some(*id),
                    _ => None,
                })
                .expect("busy setup must expose Stop");
            assert!(!initial.edits.iter().any(|edit| matches!(edit,
                Mutation::SetAttribute { name: "disabled", value: AttributeValue::Bool(true), id, .. } if *id == stop)));
            let keyboard = SerializedKeyboardData::new(
                Key::Enter,
                Code::Enter,
                Location::Standard,
                false,
                Modifiers::empty(),
                false,
            );
            dom.runtime().handle_event(
                "keydown",
                Event::new(EventData::Keyboard(keyboard).into_any(), true),
                stop,
            );
            dom.render_immediate_to_vec();
            assert!(host.0.borrow().is_empty());
            dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                stop,
            );
            dom.render_immediate_to_vec();
            assert_eq!(*host.0.borrow(), ["cancel"]);
        }
    }

    #[test]
    fn api_key_is_an_optional_choice_with_normal_composer_submission() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(
            Login,
            LoginProps {
                persisted_key_entry: false,
                controls: true,
                pending: false,
                status: "authentication".into(),
                key_available: true,
            },
        );
        dom.provide_root_context(HostBinding(host.clone()));
        let initial = dom.rebuild_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(!html.contains("type=\"password\""));
        assert!(!html.contains(&translate("onboarding-save-key")));
        let label = translate("onboarding-use-key");
        let choose = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &label => Some(*id),
                _ => None,
            })
            .expect("supported API key option must be available");
        dom.runtime().handle_event(
            "change",
            Event::new(
                EventData::Form(SerializedFormData {
                    value: "key".into(),
                    values: Vec::new(),
                    valid: true,
                })
                .into_any(),
                true,
            ),
            choose,
        );
        dom.render_immediate_to_vec();
        assert!(host.0.borrow().is_empty());
        assert!(!dioxus_ssr::render(&dom).contains("type=\"password\""));
        let proceed = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &translate("common-continue") => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            proceed,
        );
        let fields = dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("type=\"password\""));
        assert!(html.contains("Esc"));
        assert!(html.contains(&translate("common-cancel")));
        assert!(!html.contains("<textarea"));
        let password = html
            .split("<input")
            .find(|node| {
                node.split('>')
                    .next()
                    .is_some_and(|attrs| attrs.contains("type=\"password\""))
            })
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(!password.contains(" disabled=") && !password.ends_with(" disabled"));
        assert!(!html.contains(&translate("onboarding-save-key")));
        assert!(host.0.borrow().is_empty());
        let label = translate("onboarding-api-key");
        let input = fields
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &label => Some(*id),
                _ => None,
            })
            .expect("selected key mode must expose the password input");
        dom.runtime().handle_event(
            "input",
            Event::new(
                EventData::Form(SerializedFormData {
                    value: "private-test-api-key".into(),
                    values: Vec::new(),
                    valid: true,
                })
                .into_any(),
                true,
            ),
            input,
        );
        dom.render_immediate_to_vec();
        let keyboard = SerializedKeyboardData::new(
            Key::Enter,
            Code::Enter,
            Location::Standard,
            false,
            Modifiers::empty(),
            false,
        );
        dom.runtime().handle_event(
            "keydown",
            Event::new(EventData::Keyboard(keyboard).into_any(), true),
            input,
        );
        dom.render_immediate_to_vec();
        assert_eq!(*host.0.borrow(), ["key"]);
        assert!(!dioxus_ssr::render(&dom).contains("private-test-api-key"));
    }

    #[test]
    fn progress_uses_one_label_and_exposes_real_or_indeterminate_installation_progress() {
        let label = translate("agent-preparing");
        let mut preparing = VirtualDom::new_with_props(
            ProgressTurn,
            ProgressTurnProps {
                phase: Phase::Preparation,
                agent_name: String::new(),
                detail: label.clone(),
                percent: None,
            },
        );
        let initial = preparing.rebuild_to_vec();
        assert_eq!(
            initial
                .edits
                .iter()
                .filter(
                    |edit| matches!(edit, Mutation::CreateTextNode { value, .. } if value == &label)
                )
                .count(),
            1
        );
        for percent in [None, Some(72)] {
            let mut installing = VirtualDom::new_with_props(
                ProgressTurn,
                ProgressTurnProps {
                    phase: Phase::Installation,
                    agent_name: String::new(),
                    detail: "Downloading package".into(),
                    percent,
                },
            );
            installing.rebuild_in_place();
            let html = dioxus_ssr::render(&installing);
            assert!(html.contains("role=\"progressbar\""));
            if percent.is_some() {
                assert!(html.replace('"', "").contains("aria-valuenow=72"));
            } else {
                assert!(html.contains("data-state=\"indeterminate\""));
                assert!(!html.contains("aria-valuenow="));
            }
        }
    }

    #[component]
    fn FailedSetup(#[props(default = true)] setup: bool) -> Element {
        let mut state = vmux_ui::hooks::use_ui_state::<ChatUiState>().state;
        use_hook(move || {
            state.set(ChatUiState {
                snapshot: ChatSnapshot {
                    status: "errored".into(),
                    error: "session/load timed out".into(),
                    setup_pending: setup,
                    ..Default::default()
                },
                snapshot_ready: true,
                ..Default::default()
            })
        });
        vmux_ui::hooks::use_ui_state::<KeyEntryUiState>();
        let chat = super::super::state::use_chat("failed-setup".into());
        let keys = super::super::keys::use_chat_keys(chat);
        use_context_provider(|| keys);
        rsx! { super::super::composer::ChatDock { chat } }
    }

    #[test]
    fn setup_failure_offers_resume_on_the_current_step_without_submitting_work() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(FailedSetup, FailedSetupProps { setup: true });
        dom.provide_root_context(HostBinding(host.clone()));
        let initial = dom.rebuild_to_vec();
        let html = dioxus_ssr::render(&dom);
        let field = html
            .split("<textarea")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(field.contains(" disabled"));
        assert!(!html.contains("<ol "));
        let label = translate("common-resume");
        let retry = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &label => Some(*id),
                _ => None,
            })
            .expect("failed setup must offer a current-step Resume action");
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            retry,
        );
        dom.render_immediate_to_vec();
        assert_eq!(*host.0.borrow(), ["retry"]);
    }

    #[test]
    fn ordinary_resume_failure_keeps_the_work_composer() {
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(FailedSetup, FailedSetupProps { setup: false });
        dom.provide_root_context(HostBinding(host));
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        let textarea = html
            .split("<textarea")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(!textarea.contains(" disabled="));
        assert!(!html.contains(&translate("composer-choose-agent")));
    }

    #[test]
    fn recovery_sends_resume_and_change_intent_without_discarding_the_owned_view() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        for (label, event) in [
            ("common-resume", "retry"),
            ("composer-choose-agent", "choose-agent"),
        ] {
            let host = Rc::new(Host::default());
            let mut dom = VirtualDom::new_with_props(FailedSetup, FailedSetupProps { setup: true });
            dom.provide_root_context(HostBinding(host.clone()));
            let initial = dom.rebuild_to_vec();
            assert!(!dioxus_ssr::render(&dom).contains(&translate("common-cancel")));
            let label = translate(label);
            let button = initial
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &label => Some(*id),
                    _ => None,
                })
                .expect("recovery choice must be present");
            dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                button,
            );
            dom.render_immediate_to_vec();
            assert_eq!(*host.0.borrow(), [event]);
        }
    }

    #[component]
    fn NoticePreview() -> Element {
        let mut expanded = use_signal(|| true);
        rsx! {
            button { id: String::from("toggle-notice"), onclick: move |_| expanded.set(!expanded()), "Toggle" }
            ComposerFrame {
                notice: Some(rsx! { ComposerNotice { content: expanded().then(|| rsx! { button { id: "retained-notice", "Connect" } }) } }),
                input { id: String::from("persistent-composer") }
            }
        }
    }

    #[test]
    fn collapsing_a_notice_keeps_its_visual_content_inert_and_the_composer_mounted() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(NoticePreview);
        let initial = dom.rebuild_to_vec();
        let mut rendered = Dom::default();
        rendered.apply(&initial.edits);
        assert!(!rendered.blocked());
        let mut toggle = None;
        let mut composer = None;
        for edit in &initial.edits {
            if let Mutation::SetAttribute {
                name: "id",
                value: AttributeValue::Text(value),
                id,
                ..
            } = edit
            {
                match value.as_str() {
                    "toggle-notice" => toggle = Some(*id),
                    "persistent-composer" => composer = Some(*id),
                    _ => {}
                }
            }
        }
        let toggle = toggle.expect("the notice can be toggled");
        let composer = composer.expect("the composer must be mounted");
        for expanded in [false, true, false] {
            dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                toggle,
            );
            let changed = dom.render_immediate_to_vec();
            rendered.apply(&changed.edits);
            assert_eq!(rendered.blocked(), !expanded);
            for edit in changed.edits {
                assert!(!matches!(edit, Mutation::Remove { id } if id == composer));
                assert!(!matches!(edit, Mutation::ReplaceWith { id, .. } if id == composer));
            }
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains("id=\"retained-notice\""));
            assert!(html.contains("id=\"persistent-composer\""));
            let notice = Dom::notice_attributes(&html);
            assert_eq!(Dom::notice_hidden(notice), !expanded, "{notice}");
        }
    }
}
