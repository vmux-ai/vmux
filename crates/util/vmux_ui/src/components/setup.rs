use base64::Engine;
use dioxus::prelude::*;

use crate::components::avatar::AgentThumbnail;
use crate::components::button::{Button, ButtonSize};
use crate::components::composer::{PromptComposer, PromptComposerMode, ShortcutHint};
use crate::components::progress::{LoadingIcon, Progress, ProgressIndicator};
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};

#[component]
pub fn SetupTurn(children: Element) -> Element {
    rsx! {
        div { class: "chat-message-enter chat-assistant-turn group flex w-full gap-3 px-1 py-1 [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]", "data-vmux-scroll-layout": "",
            AgentThumbnail {
                src: Some(SetupGuide::avatar()), seed: SetupGuide::ID.to_string(),
                background: SetupGuide::COLOR.to_string(), alt: SetupGuide::name(),
                class: "mt-0.5 h-6 w-6 text-[8px]".to_string(),
            }
            div { class: "relative min-w-0 flex-1",
                div { class: "mb-1 text-xs font-medium text-muted-foreground", {SetupGuide::name()} }
                div { class: "flex min-w-0 flex-col gap-2.5", {children} }
                div { class: "mt-1 h-6" }
            }
        }
    }
}

#[component]
pub fn SetupConnect(
    #[props(default)] pending: bool,
    #[props(default = true)] available: bool,
    #[props(default = rsx! {})] secondary: Element,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { "data-slot": "composer-notice", class: "flex flex-wrap items-center gap-2 px-3 py-3",
            Button {
                size: ButtonSize::Sm, aria_label: translate("onboarding-connect"),
                disabled: pending || !available, onclick,
                {translate("onboarding-connect")}
            }
            {secondary}
        }
    }
}

#[component]
pub fn SetupComposer(
    accent: String,
    #[props(default)] busy: bool,
    #[props(default)] cancel_hint: bool,
    #[props(default)] value: String,
    on_stop: EventHandler<()>,
) -> Element {
    rsx! {
        PromptComposer {
            value, input_disabled: true,
            placeholder: translate("onboarding-composer-wait"), accent_color: accent,
            accent_gradient: "from-indigo-500 to-violet-600".to_string(),
            footer: Some(rsx! { if cancel_hint { ShortcutHint { shortcut: "Esc", label: translate("common-cancel") } } }),
            show_add_button: false, show_send_button: false,
            mode: if busy { PromptComposerMode::Stop } else { PromptComposerMode::Send },
            action_title: translate("common-stop"), action_enabled: busy,
            on_input: move |_| {}, on_keydown: move |_| {}, on_paste: move |_| {}, on_attach: move |_| {},
            on_remove_attachment: move |_| {}, on_action: on_stop,
        }
    }
}

#[component]
pub fn SetupProgress(
    label: String,
    icon: LineIcon,
    #[props(default)] detail: String,
    #[props(default)] installation: bool,
    #[props(default)] percent: Option<u8>,
) -> Element {
    let detail = detail.trim();
    let show_detail =
        !detail.is_empty() && detail != label && detail != translate("agent-preparing");
    rsx! {
        div { role: "status", aria_live: "polite", class: "flex items-center gap-2 text-sm",
            LineIconView { icon, class: "size-4 shrink-0 text-muted-foreground" }
            span { "{label}" }
            LoadingIcon { class: "size-3.5 text-muted-foreground".to_string() }
        }
        if show_detail { p { class: "text-xs text-muted-foreground", "{detail}" } }
        if installation {
            div { class: "w-full max-w-xs",
                Progress { value: percent.map(|value| f64::from(value.min(100))), max: 100.0, aria_label: translate("setup-installing"),
                    ProgressIndicator {}
                }
            }
        }
    }
}

pub struct SetupGuide;

impl SetupGuide {
    pub const ID: &str = "vmux-setup-guide";
    pub const COLOR: &str = "#6366f1";

    pub fn name() -> String {
        translate("onboarding-guide")
    }

    pub fn avatar() -> String {
        let svg = include_str!("../../assets/agent-logos/setup-guide.svg");
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        )
    }
}
