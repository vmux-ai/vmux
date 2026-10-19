#![allow(non_snake_case)]

use crate::{
    ShortcutProbeClearRequest, ShortcutProbePressRequest, ShortcutProbeStatus, ShortcutStroke,
    ShortcutUiState,
};
use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::BuiltinIconView;

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "ShortcutUiState"]
)]
pub(crate) struct ShortcutPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ShortcutUiState>().state;
    let state = state();
    let probe = state.probe;
    let groups = state.groups;
    let (status_label, status_tone) = probe_presentation(&probe.status);
    let subtitle = translate_with(
        "shortcuts-count",
        &[(
            "count",
            TranslationValue::Number(state.shortcut_count as i64),
        )],
    );
    rsx! {
        main {
            class: "relative flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground outline-none",
            tabindex: "-1",
            autofocus: true,
            onkeydown: move |event: KeyboardEvent| {
                let modifiers = event.modifiers();
                if event.code().to_string() == "Tab"
                    && !modifiers.ctrl()
                    && !modifiers.alt()
                    && !modifiers.meta()
                {
                    return;
                }
                event.prevent_default();
                event.stop_propagation();
                if event.is_auto_repeating() {
                    return;
                }
                let Some(stroke) = ShortcutStroke::from_keyboard_event(&event) else {
                    return;
                };
                let _ = send(&ShortcutProbePressRequest { stroke });
            },
            header { class: "shrink-0 px-5 py-3",
                div { class: "mx-auto flex w-full max-w-4xl items-center gap-4",
                    div { class: "min-w-0",
                        h1 { class: "text-lg font-semibold tracking-tight", {translate("shortcuts-title")} }
                        p { class: "mt-0.5 text-xs text-muted-foreground", "{subtitle}" }
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-5 pb-5 [scrollbar-gutter:stable]",
                div { class: "mx-auto w-full max-w-4xl",
                    div {
                        class: "mb-3 flex min-h-12 w-full items-center gap-3 rounded-lg bg-foreground/[0.035] px-3 py-2",
                        aria_label: translate("shortcuts-try-title"),
                        span { class: "flex min-w-0 items-center gap-2 text-xs font-medium text-muted-foreground",
                            span { class: "flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-foreground/[0.055] text-muted-foreground",
                                BuiltinIconView {
                                    icon: vmux_api::BuiltinIcon::Keyboard,
                                    class: "h-4 w-4".to_string(),
                                }
                            }
                            span { class: "hidden sm:inline", {translate("shortcuts-try-title")} }
                        }
                        div { class: "flex min-w-0 flex-1 items-center justify-center gap-3",
                            if probe.sequence.is_empty() {
                                span { class: "truncate text-sm text-foreground/70", {translate("shortcuts-try-hint")} }
                            } else {
                                ShortcutSequence { strokes: probe.sequence.clone() }
                            }
                            if !probe.sequence.is_empty() {
                                span { class: "min-w-0 truncate text-xs font-medium {status_tone}", "{status_label}" }
                            }
                        }
                        if !probe.sequence.is_empty() {
                            button {
                                r#type: "button",
                                tabindex: "-1",
                                class: "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-[10px] font-medium text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                                onpointerdown: move |event| event.prevent_default(),
                                onclick: move |event| {
                                    event.stop_propagation();
                                    let _ = send(&ShortcutProbeClearRequest);
                                },
                                kbd { class: "font-mono", "Esc" }
                                {translate("shortcuts-clear")}
                            }
                        }
                    }
                    if groups.is_empty() {
                        div { class: "flex min-h-40 items-center justify-center rounded-lg bg-foreground/[0.025] text-sm text-muted-foreground",
                            if matches!(probe.status, ShortcutProbeStatus::Miss) {
                                {translate("shortcuts-no-match")}
                            } else {
                                {translate("shortcuts-empty")}
                            }
                        }
                    } else {
                        div { class: "overflow-hidden rounded-lg bg-foreground/[0.025]",
                            for group in groups {
                                ShortcutGroupView {
                                    key: "{group.name}",
                                    group,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl ShortcutStroke {
    fn from_keyboard_event(event: &KeyboardEvent) -> Option<Self> {
        let key = event.key().to_string();
        if matches!(
            key.as_str(),
            "Shift" | "Control" | "Alt" | "Meta" | "OS" | "Fn" | "CapsLock"
        ) {
            return None;
        }
        let code = event.code().to_string();
        let modifiers = event.modifiers();
        Some(Self {
            label: Self::label_for(&code, &key),
            code,
            ctrl: modifiers.ctrl(),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
            super_key: modifiers.meta(),
        })
    }

    fn label_for(code: &str, key: &str) -> String {
        match code {
            "ArrowDown" => "↓".to_string(),
            "ArrowLeft" => "←".to_string(),
            "ArrowRight" => "→".to_string(),
            "ArrowUp" => "↑".to_string(),
            "Backspace" => "⌫".to_string(),
            "Delete" => "⌦".to_string(),
            "Enter" => "↩".to_string(),
            "Escape" => "Esc".to_string(),
            "Space" => "Space".to_string(),
            "Tab" => "⇥".to_string(),
            _ => code
                .strip_prefix("Key")
                .or_else(|| code.strip_prefix("Digit"))
                .map(str::to_string)
                .unwrap_or_else(|| key.to_uppercase()),
        }
    }
}

#[component]
fn ShortcutGroupView(group: crate::ShortcutGroup) -> Element {
    rsx! {
        section { class: "w-full border-b border-foreground/[0.055] last:border-b-0",
            h2 { class: "flex h-8 items-center px-3 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground",
                "{group.name}"
            }
            div {
                for entry in group.entries {
                    ShortcutEntryView {
                        key: "{entry.id}",
                        entry,
                    }
                }
            }
        }
    }
}

#[component]
fn ShortcutEntryView(entry: crate::ShortcutEntry) -> Element {
    let selected = entry.shortcuts.iter().any(|shortcut| shortcut.emphasized);
    let class = if selected {
        "flex min-h-10 items-center gap-3 bg-primary/[0.08] px-3 py-2"
    } else {
        "flex min-h-10 items-center gap-3 px-3 py-2 transition-colors hover:bg-foreground/[0.035]"
    };
    rsx! {
        div { class: "{class}",
            span { class: "min-w-0 flex-1 text-xs leading-snug text-foreground", "{entry.name}" }
            div { class: "flex max-w-[55%] shrink-0 flex-col items-end gap-1",
                for (shortcut_index, shortcut) in entry.shortcuts.iter().enumerate() {
                    {
                        rsx! {
                            span { key: "{shortcut_index}", class: "flex flex-col items-end gap-1",
                                span { class: "inline-flex items-center gap-1",
                                    for (stroke_index, stroke) in shortcut.strokes.iter().enumerate() {
                                        if stroke_index > 0 {
                                            span { class: "px-0.5 text-[11px] text-muted-foreground/55", "›" }
                                        }
                                        for keycap in stroke.keycaps() {
                                            kbd { class: shortcut_key_class(shortcut.emphasized), "{keycap}" }
                                        }
                                    }
                                }
                                if !shortcut.contexts.is_empty() {
                                    span { class: "font-mono text-[9px] text-muted-foreground/65",
                                        {shortcut.contexts.join(" / ")}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn probe_presentation(status: &ShortcutProbeStatus) -> (String, &'static str) {
    match status {
        ShortcutProbeStatus::Idle => (translate("shortcuts-try-hint"), "text-muted-foreground"),
        ShortcutProbeStatus::Pending => (translate("shortcuts-waiting"), "text-amber-500"),
        ShortcutProbeStatus::Match(names) => {
            let commands = names.join(", ");
            (
                translate_with(
                    "shortcuts-triggered",
                    &[("action", TranslationValue::String(&commands))],
                ),
                "text-primary",
            )
        }
        ShortcutProbeStatus::Contextual(labels) => (labels.join(", "), "text-sky-500"),
        ShortcutProbeStatus::Miss => (translate("shortcuts-no-match"), "text-rose-500"),
    }
}

#[component]
fn ShortcutSequence(strokes: Vec<ShortcutStroke>) -> Element {
    rsx! {
        span { class: "inline-flex items-center gap-1.5",
            for (stroke_index, stroke) in strokes.into_iter().enumerate() {
                if stroke_index > 0 {
                    span { class: "text-xs text-muted-foreground/55", "›" }
                }
                for keycap in stroke.keycaps() {
                    kbd { class: "inline-flex min-w-6 items-center justify-center rounded-md bg-primary/10 px-1.5 py-1 font-mono text-[11px] font-semibold leading-none text-primary", "{keycap}" }
                }
            }
        }
    }
}

fn shortcut_key_class(emphasized: bool) -> &'static str {
    if emphasized {
        "inline-flex min-w-6 items-center justify-center rounded-md bg-primary/10 px-1.5 py-1 font-mono text-[11px] font-semibold leading-none text-primary"
    } else {
        "inline-flex min-w-6 items-center justify-center rounded-md bg-foreground/[0.06] px-1.5 py-1 font-mono text-[11px] font-semibold leading-none text-foreground"
    }
}
