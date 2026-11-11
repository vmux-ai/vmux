use super::approval::{ChoiceKey, ChoiceOptions};
use super::state::Chat;
use crate::onboarding::{ChatAuthenticate, ChatCancelSetup, ChatSaveKey, KeyEntryUiState};
use dioxus::prelude::*;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use vmux_ui::components::composer::{ComposerFrame, PromptComposer, PromptComposerMode};
use vmux_ui::components::progress::{LoadingIcon, Progress, ProgressIndicator};
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub(super) fn Turn(children: Element) -> Element {
    rsx! {
        super::item::AssistantTurn {
            agent_id: Identity::ID.to_string(),
            name: Identity::name(),
            avatar_src: Some(Identity::avatar()),
            avatar_background: Identity::COLOR.to_string(),
            {children}
        }
    }
}

#[component]
pub(super) fn Controls(chat: Chat) -> Element {
    let mut selected = use_signal(|| 0_usize);
    let keys = UiStateBinding::<KeyEntryUiState>::use_context()
        .state
        .read()
        .clone();
    let authentication = chat.status() == "authentication";
    let pending = chat.authentication_pending();
    if chat.setup_busy() && !authentication {
        return rsx! { ComposerFrame { SetupComposer { chat, keys } } };
    }
    let mut choices = Vec::new();
    if chat.status() == "errored" {
        choices.push(Choice::Retry);
    }
    if authentication {
        for method in chat.authentication_methods() {
            choices.push(Choice::Authenticate(method.id, method.name));
        }
    }
    if !pending {
        choices.push(Choice::Cancel);
        choices.push(Choice::ChangeAgent);
    }
    let mut labels = Vec::new();
    let mut disabled_options = Vec::new();
    for (index, choice) in choices.iter().enumerate() {
        labels.push(choice.label());
        if !choice.available(pending) {
            disabled_options.push(index);
        }
    }
    let count = choices.len();
    let keyboard = choices.clone();
    let notice = rsx! {
        div { "data-slot": "composer-notice",
            ChoiceOptions {
                options: labels,
                selected: selected().min(count.saturating_sub(1)),
                compact: true,
                framed: true,
                disabled: keys.pending,
                disabled_options,
                on_hover: move |index| selected.set(index),
                on_select: move |index: usize| {
                    if !keys.pending && let Some(choice) = choices.get(index) && choice.available(pending) { choice.choose(); }
                },
                on_keydown: move |event| {
                    if keys.pending { return; }
                    let Some(key) = ChoiceKey::of(&event, true) else { return; };
                    let current = selected().min(count.saturating_sub(1));
                    match key {
                        ChoiceKey::Next => selected.set((current + 1).min(count.saturating_sub(1))),
                        ChoiceKey::Previous => selected.set(current.saturating_sub(1)),
                        ChoiceKey::Submit => { if let Some(choice) = keyboard.get(current) && choice.available(pending) { choice.choose(); } },
                        ChoiceKey::Number(index) => { if let Some(choice) = keyboard.get(index) && choice.available(pending) { choice.choose(); } },
                    }
                    event.prevent_default();
                    event.stop_propagation();
                },
            }
        }
    };
    rsx! { ComposerFrame { notice: Some(notice), SetupComposer { chat, keys } } }
}

#[component]
fn SetupComposer(chat: Chat, keys: KeyEntryUiState) -> Element {
    let input = if chat.status() == "authentication" && !keys.key_variable.is_empty() {
        Some(rsx! {
            vmux_ui::components::api_key::ApiKeyEntry {
                pending: keys.pending,
                failed: keys.failed,
                on_submit: move |key| { let _ = send(&ChatSaveKey { key }); },
            }
        })
    } else {
        None
    };
    rsx! {
        PromptComposer {
            value: chat.draft(),
            input,
            input_disabled: true,
            placeholder: translate("onboarding-composer-wait"),
            accent_color: chat.accent().css,
            accent_gradient: "from-indigo-500 to-violet-600".to_string(),
            footer: Some(rsx! {}),
            show_add_button: false,
            mode: PromptComposerMode::Stop,
            action_title: translate("common-stop"),
            action_enabled: chat.status() != "errored",
            on_input: move |_| {},
            on_keydown: move |_| {},
            on_paste: move |_| {},
            on_attach: move |_| {},
            on_remove_attachment: move |_| {},
            on_action: move |_| chat.stop_or_flush(),
        }
    }
}

#[component]
pub(super) fn ProgressTurn(
    phase: Phase,
    #[props(default)] detail: String,
    #[props(default)] percent: Option<u8>,
) -> Element {
    let label = phase.label();
    let detail = detail.trim();
    let show_detail =
        !detail.is_empty() && detail != label && detail != translate("agent-preparing");
    rsx! {
        Turn {
            div { role: "status", aria_live: "polite", class: "flex items-center gap-2 text-sm",
                LineIconView { icon: phase.icon(), class: "size-4 shrink-0 text-muted-foreground" }
                span { "{label}" }
                LoadingIcon { class: "size-3.5 text-muted-foreground".to_string() }
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
    Authentication,
}

impl Phase {
    fn label(self) -> String {
        translate(match self {
            Self::Preparation => "agent-preparing",
            Self::Installation => "setup-installing",
            Self::Authentication => "onboarding-auth-pending",
        })
    }

    fn icon(self) -> LineIcon {
        match self {
            Self::Preparation => LineIcon::RefreshCw,
            Self::Installation => LineIcon::Package,
            Self::Authentication => LineIcon::Shield,
        }
    }
}

#[derive(Clone)]
enum Choice {
    Retry,
    Authenticate(String, String),
    Cancel,
    ChangeAgent,
}

impl Choice {
    fn available(&self, pending: bool) -> bool {
        !pending || !matches!(self, Self::Authenticate(..))
    }

    fn label(&self) -> String {
        match self {
            Self::Retry => translate("common-retry"),
            Self::Authenticate(_, name) => name.clone(),
            Self::Cancel => translate("common-cancel"),
            Self::ChangeAgent => translate("composer-choose-agent"),
        }
    }

    fn choose(&self) {
        match self {
            Self::Retry => {
                let _ = send(&crate::event::ChatRetrySession);
            }
            Self::Authenticate(method_id, _) => {
                let _ = send(&ChatAuthenticate {
                    method_id: method_id.clone(),
                });
            }
            Self::Cancel | Self::ChangeAgent => {
                if send(&ChatCancelSetup).is_ok() {
                    vmux_ui::navigation::PageNavigation::replace("vmux://sessions/inbox?setup=1");
                }
            }
        }
    }
}

pub(super) struct Identity;

impl Identity {
    pub(super) const ID: &str = "vmux-setup-guide";
    pub(super) const COLOR: &str = "#6366f1";

    pub(super) fn name() -> String {
        translate("onboarding-guide")
    }

    fn avatar() -> String {
        let svg = include_str!("../../../../util/vmux_ui/assets/agent-logos/setup-guide.svg");
        format!(
            "data:image/svg+xml,{}",
            utf8_percent_encode(svg, NON_ALPHANUMERIC)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ChatSnapshot;
    use crate::state::ChatUiState;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedHtmlEventConverter, SerializedKeyboardData,
        SerializedMouseData,
    };
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, PageHost};

    #[derive(Default)]
    struct Host(RefCell<Vec<String>>);

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == ChatAuthenticate::id() {
                let request =
                    rkyv::from_bytes::<ChatAuthenticate, rkyv::rancor::Error>(bytes).unwrap();
                self.0.borrow_mut().push(request.method_id);
            } else if id == crate::event::ChatRetrySession::id() {
                self.0.borrow_mut().push("retry".into());
            } else if id == crate::event::ChatStop::id() {
                self.0.borrow_mut().push("stop".into());
            } else if id == ChatCancelSetup::id() {
                self.0.borrow_mut().push("cancel".into());
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
                    authentication_methods: vec![vmux_api::protocol::AuthenticationMethod {
                        id: "browser-login".into(),
                        name: "Sign in with browser".into(),
                        description: String::new(),
                    }],
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
        assert!(dioxus_ssr::render(&composer).contains("role=\"listbox\""));
        let option = initial
            .edits
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == "Sign in with browser" => Some(*id),
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
        assert_eq!(*host.0.borrow(), ["browser-login"]);
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
            assert!(!html.contains(&translate("common-cancel")));
            assert!(!html.contains(&translate("composer-choose-agent")));
            if status != "authentication" {
                assert!(!html.contains("role=\"listbox\""));
            }
            if key_available {
                assert!(!html.contains("<textarea"));
                assert!(html.contains("type=\"password\""));
            } else {
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
    fn progress_uses_one_label_and_exposes_real_or_indeterminate_installation_progress() {
        let label = translate("agent-preparing");
        let mut preparing = VirtualDom::new_with_props(
            ProgressTurn,
            ProgressTurnProps {
                phase: Phase::Preparation,
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
    fn FailedSetup() -> Element {
        let mut state = vmux_ui::hooks::use_ui_state::<ChatUiState>().state;
        use_hook(move || {
            state.set(ChatUiState {
                snapshot: ChatSnapshot {
                    status: "errored".into(),
                    error: "ACP installation failed".into(),
                    ..Default::default()
                },
                snapshot_ready: true,
                ..Default::default()
            })
        });
        vmux_ui::hooks::use_ui_state::<KeyEntryUiState>();
        let chat = super::super::state::use_chat("failed-setup".into());
        rsx! { super::super::composer::ChatDock { chat } }
    }

    #[test]
    fn setup_failure_offers_retry_instead_of_an_unrelated_work_composer() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new(FailedSetup);
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
        assert!(textarea.contains("disabled"));
        assert!(
            html.find("data-slot=\"composer-notice\"").unwrap() < html.find("<fieldset").unwrap()
        );
        let label = translate("common-retry");
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
            .expect("failed setup must offer retry");
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
}
