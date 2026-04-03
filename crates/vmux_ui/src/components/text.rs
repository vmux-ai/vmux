//! Text span with tone/size defaults (vmux chrome; not part of upstream gallery).

use crate::util::merge_class;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum UiTextTone {
    /// No extra color class (inherit from parent / strip).
    #[default]
    Inherit,
    Default,
    Muted,
    Dim,
    /// Sky accent (aligned with primary chrome).
    Accent,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum UiTextSize {
    /// No extra font-size class (inherit).
    #[default]
    Inherit,
    Sm,
    Xs,
    Xxs,
}

impl UiTextTone {
    fn default_class(self) -> &'static str {
        match self {
            UiTextTone::Inherit => "",
            UiTextTone::Default => "text-white/90",
            UiTextTone::Muted => "text-white/55",
            UiTextTone::Dim => "text-white/28",
            UiTextTone::Accent => "text-sky-300/95",
        }
    }
}

impl UiTextSize {
    fn default_class(self) -> &'static str {
        match self {
            UiTextSize::Inherit => "",
            UiTextSize::Sm => "text-[13px]",
            UiTextSize::Xs => "text-[11px]",
            UiTextSize::Xxs => "text-[10px]",
        }
    }
}

#[component]
pub fn UiText(
    #[props(default)] class: Option<String>,
    #[props(default)] tone: UiTextTone,
    #[props(default)] size: UiTextSize,
    children: Element,
) -> Element {
    let tone_s = tone.default_class();
    let sz = size.default_class();
    let base = match (tone_s.is_empty(), sz.is_empty()) {
        (true, true) => String::new(),
        (true, false) => sz.to_string(),
        (false, true) => tone_s.to_string(),
        _ => format!("{tone_s} {sz}"),
    };
    let c = merge_class(&base, class.as_deref());
    rsx! {
        span { class: "{c}", {children} }
    }
}
