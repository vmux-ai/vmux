use super::state::Chat;
use crate::onboarding::OnboardingUiState;
use crate::onboarding::{
    ChatAuthenticate, ChatCancelSetup, ChatSaveKey, KeyEntryUiState, OnboardingChooseAgent,
};
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::composer::{
    ComposerFrame, ComposerNotice, ComposerTask, PromptComposer, PromptComposerMode, ShortcutHint,
};
use vmux_ui::components::message::MessageFrame;
use vmux_ui::components::progress::{Progress, ProgressIndicator};
use vmux_ui::components::prompt::{ConversationPrompt, PromptConfirmation};
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[component]
pub(super) fn Turn(#[props(default)] agent_name: String, children: Element) -> Element {
    rsx! {
        MessageFrame {
            name: if agent_name.is_empty() { Identity::name() } else { translate_with("onboarding-setup-agent", &[("agent", TranslationValue::String(&agent_name))]) },
            seed: "vmux-setup-agent".to_string(),
            {children}
        }
    }
}

#[component]
pub(super) fn Welcome(
    #[props(default)] agent_name: String,
    #[props(default = rsx! {})] children: Element,
) -> Element {
    rsx! {
        div { class: "onboarding-origin min-h-24",
            Turn { agent_name: agent_name.clone(),
                p { class: "text-sm leading-relaxed", {if agent_name.is_empty() { translate("onboarding-start-description") } else { translate_with("onboarding-start-agent-description", &[("agent", TranslationValue::String(&agent_name))]) }} }
                {children}
            }
        }
    }
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
    let resumable = onboarding.paused || chat.status() == "errored";
    let notice = if resumable {
        Some(rsx! { RecoveryControls { pending: keys.pending, show_retry: false } })
    } else if chat.authentication_pending() {
        Some(rsx! { RecoveryControls { pending: keys.pending, allow_change: false } })
    } else if chat.setup_busy() && chat.status() != "authentication" {
        Some(rsx! { CancelControl {} })
    } else if chat.status() == "authentication" {
        if chat.key_input_requested() {
            None
        } else {
            Some(rsx! { ConnectionControls { chat, keys: keys.clone() } })
        }
    } else {
        Some(rsx! { RecoveryControls { pending: keys.pending } })
    };
    let on_choose = (!keys.pending && !chat.authentication_pending())
        .then_some(EventHandler::new(move |_| Choice::ChangeAgent.choose()));
    let on_resume =
        (resumable && !keys.pending).then_some(EventHandler::new(move |_| Choice::Retry.choose()));
    rsx! { Frame { notice, chat: Some(chat), onboarding, on_choose, on_resume, SetupComposer { chat, keys } } }
}

#[component]
pub(super) fn Frame(
    #[props(default)] notice: Option<Element>,
    #[props(default)] compact: bool,
    #[props(default)] chat: Option<Chat>,
    #[props(default)] onboarding: OnboardingUiState,
    #[props(default)] on_choose: Option<EventHandler<()>>,
    #[props(default)] on_resume: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let task = Task::of(chat, &onboarding);
    rsx! {
        ComposerFrame {
            borderless: true,
            content_class: if compact { "h-12".to_string() } else { String::new() },
            notice: Some(rsx! { ComposerNotice {
                content: Some(rsx! { ComposerTask {
                    title: task.title,
                    steps: task.steps,
                    completed_steps: task.completed,
                    pending_step: task.pending,
                    detail_step: Some(task.current),
                    on_select: on_resume.or(on_choose),
                    action_step: if on_resume.is_some() { task.current } else { 0 },
                    action_label: if on_resume.is_some() { translate("common-resume") } else { Choice::ChangeAgent.label() },
                    message: task.message,
                    icon: task.icon,
                    if let Some(notice) = notice { {notice} }
                } }),
                class: "setup-notice".to_string(),
            } }),
            {children}
        }
    }
}

struct Task {
    title: String,
    steps: Vec<String>,
    completed: Vec<usize>,
    pending: Option<usize>,
    current: usize,
    message: String,
    icon: Option<String>,
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
        let title = if name.is_empty() {
            translate("onboarding-setup")
        } else {
            translate_with(
                "onboarding-setup-agent",
                &[("agent", TranslationValue::String(&name))],
            )
        };
        let mut steps = Vec::new();
        for (generic, named) in [
            (
                "onboarding-checklist-select",
                "onboarding-checklist-select-agent",
            ),
            (
                "onboarding-checklist-install",
                "onboarding-checklist-install-agent",
            ),
            (
                "onboarding-checklist-connect",
                "onboarding-checklist-connect-agent",
            ),
        ] {
            steps.push(if name.is_empty() {
                translate(generic)
            } else {
                translate_with(named, &[("agent", TranslationValue::String(&name))])
            });
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
            title,
            steps,
            completed,
            pending,
            current,
            message,
            icon: agent
                .filter(|agent| !agent.icon.is_empty())
                .map(|agent| agent.icon.clone()),
        }
    }
    fn message(key: &str, name: &str) -> String {
        translate_with(key, &[("agent", TranslationValue::String(name))])
    }
}

#[component]
fn CancelControl() -> Element {
    rsx! { div { class: "flex items-center gap-2 px-2 pt-2",
        Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, style: "color:inherit;", class: "[&:hover]:bg-background/5", aria_label: translate("common-cancel"), onclick: move |_| Choice::Cancel.choose(), {translate("common-cancel")} }
    } }
}

#[component]
fn ConnectionControls(chat: Chat, keys: KeyEntryUiState) -> Element {
    let connection = Connection::of(chat, !keys.key_variable.is_empty());
    let available = connection.method.is_some() || connection.key;
    let pending = chat.authentication_pending() || keys.pending;
    let clicked = connection.clone();
    rsx! {
        div { "data-slot": "composer-notice", class: "px-1 pt-2",
            ConversationPrompt {
                actions: Some(rsx! { PromptConfirmation {
                    confirm_label: translate("onboarding-connect"),
                    enabled: available,
                    pending,
                    cancel_label: Some(translate("common-cancel")),
                    on_confirm: move |_| clicked.connect(chat),
                    on_cancel: move |_| Choice::Cancel.choose(),
                } }),
            if connection.method.is_some() && connection.key && !chat.key_input_requested() && !pending {
                Button {
                    size: ButtonSize::Xs,
                    variant: ButtonVariant::Ghost,
                    class: "text-inherit opacity-60",
                    style: "color:inherit;",
                    aria_label: translate("onboarding-use-key"),
                    onclick: move |_| chat.request_key_input(true),
                    {translate("onboarding-use-key")}
                }
            }
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
        if chosen.is_none() && !key {
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
fn RecoveryControls(
    pending: bool,
    #[props(default = true)] allow_change: bool,
    #[props(default = true)] show_retry: bool,
) -> Element {
    rsx! {
        div { "data-slot": "composer-notice", class: "px-1 pt-2",
            ConversationPrompt {
                actions: Some(rsx! { if show_retry { PromptConfirmation {
                    confirm_label: Choice::Retry.label(),
                    pending,
                    cancel_label: Some(Choice::Cancel.label()),
                    on_confirm: move |_| Choice::Retry.choose(),
                    on_cancel: move |_| Choice::Cancel.choose(),
                } } else { CancelControl {} } }),
                if allow_change {
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    disabled: pending,
                    style: "color:inherit;",
                    class: "[&:hover]:bg-background/5",
                    aria_label: Choice::ChangeAgent.label(),
                    onclick: move |_| Choice::ChangeAgent.choose(),
                    {Choice::ChangeAgent.label()}
                }
                }
            }
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
    let busy = chat.setup_busy() || keys.pending;
    let footer = if chat.status() == "errored" {
        rsx! {}
    } else {
        rsx! { ShortcutHint { shortcut: "Esc", label: translate("common-cancel") } }
    };
    rsx! {
        PromptComposer {
            value: chat.draft(), input_disabled: true,
            placeholder: translate("onboarding-composer-wait"), accent_color: chat.accent().css,
            accent_gradient: "from-indigo-500 to-violet-600".to_string(), footer: Some(footer),
            show_add_button: false, show_send_button: false,
            mode: if busy { PromptComposerMode::Stop } else { PromptComposerMode::Send },
            action_title: translate("common-stop"), action_enabled: busy,
            on_input: move |_| {}, on_keydown: move |_| {}, on_paste: move |_| {}, on_attach: move |_| {},
            on_remove_attachment: move |_| {}, on_action: move |_| chat.stop_or_flush(),
        }
    }
}

#[component]
pub(super) fn ProgressTurn(
    phase: Phase,
    #[props(default)] agent_name: String,
    #[props(default)] detail: String,
    #[props(default)] percent: Option<u8>,
) -> Element {
    let label = if agent_name.is_empty() {
        phase.label()
    } else {
        translate_with(
            match phase {
                Phase::Preparation => "onboarding-preparing-agent",
                Phase::Installation => "onboarding-installing-agent",
            },
            &[("agent", TranslationValue::String(&agent_name))],
        )
    };
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

    struct Notice<'a>(&'a str);

    impl<'a> Notice<'a> {
        fn of(html: &'a str) -> Self {
            for node in html.split('<') {
                let Some(attributes) = node.strip_prefix("div ") else {
                    continue;
                };
                let attributes = attributes.split('>').next().unwrap();
                if attributes.contains(" aria-hidden=") {
                    return Self(attributes);
                }
            }
            panic!("the notice must expose its interaction state");
        }

        fn hidden(&self) -> bool {
            self.0.contains("aria-hidden=\"true\"") && self.0.contains(" inert")
        }
    }

    #[derive(Default)]
    struct Dom {
        inert: std::collections::HashSet<dioxus::core::ElementId>,
    }

    impl Dom {
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
    ) -> Element {
        let mut state = vmux_ui::hooks::use_ui_state::<ChatUiState>().state;
        use_hook(move || {
            state.set(ChatUiState {
                snapshot: ChatSnapshot {
                    status,
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
        let chat = super::super::state::use_chat("login-session".into());
        if controls {
            rsx! { super::super::composer::ChatDock { chat } }
        } else {
            rsx! { super::super::authentication::Authentication { chat } }
        }
    }

    #[test]
    fn authentication_is_a_guide_message_and_sign_in_is_a_composer_reply() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut message = VirtualDom::new_with_props(
            Login,
            LoginProps {
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
                } if value == &translate("onboarding-connect") => Some(*id),
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
    fn busy_setup_uses_standard_stop_and_disables_only_unneeded_text_input() {
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
                    controls: true,
                    pending,
                    status: status.into(),
                    key_available,
                },
            );
            dom.provide_root_context(HostBinding(host.clone()));
            let initial = dom.rebuild_to_vec();
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains(&translate("common-cancel")));
            assert!(html.contains("Esc"));
            assert!(initial.edits.iter().any(|edit| matches!(edit,
                Mutation::SetAttribute { name: "aria-label" | "title", value: AttributeValue::Text(value), .. } if value == &translate("common-cancel"))));
            if status == "authentication" {
                assert!(html.contains(&translate("common-retry")));
            }
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
            {
                let textarea = html
                    .split("<textarea")
                    .nth(1)
                    .unwrap()
                    .split('>')
                    .next()
                    .unwrap();
                assert!(textarea.contains("disabled"));
            }
            let stop = initial
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "title",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &translate("common-stop") => Some(*id),
                    _ => None,
                })
                .expect("busy setup must show the normal stop control");
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
            assert_eq!(*host.0.borrow(), ["stop"]);
        }
    }

    #[test]
    fn api_key_is_an_optional_choice_with_normal_composer_submission() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(
            Login,
            LoginProps {
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
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            choose,
        );
        let fields = dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("type=\"password\""));
        assert!(html.contains("Esc"));
        assert!(html.contains(&translate("common-cancel")));
        assert!(!html.contains("<textarea"));
        assert!(!Notice::of(&html).hidden(), "{}", Notice::of(&html).0);
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
        let textarea = html
            .split("<textarea")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(textarea.contains(" disabled="));
        assert!(
            html.find("data-slot=\"composer-notice\"").unwrap() < html.find("<fieldset").unwrap()
        );
        let label = translate("common-resume");
        assert!(html.split("<li ").skip(1).any(|row| {
            let row = row.split("</li>").next().unwrap();
            row.contains("aria-current=\"step\"")
                && row.contains(&format!("aria-label=\"{label}\""))
        }));
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
    fn recovery_sends_cancel_and_change_intent_without_discarding_the_owned_view() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        for (label, event) in [
            ("common-cancel", "cancel"),
            ("composer-choose-agent", "choose-agent"),
        ] {
            let host = Rc::new(Host::default());
            let mut dom = VirtualDom::new_with_props(FailedSetup, FailedSetupProps { setup: true });
            dom.provide_root_context(HostBinding(host.clone()));
            let initial = dom.rebuild_to_vec();
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
            assert_eq!(
                Notice::of(&html).hidden(),
                !expanded,
                "{}",
                Notice::of(&html).0
            );
        }
    }
}
