#![allow(non_snake_case)]

use crate::{ShortcutProbeRequest, ShortcutProbeStatus, ShortcutStroke, ShortcutUiState};
use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::BuiltinIconView;

#[vmux_native::page(
    url = crate::PAGE_URL,
    title = "Keyboard Shortcuts",
    component = Page
)]
pub(crate) struct ShortcutPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ShortcutUiState>()();
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
                let _ = send(&ShortcutProbeRequest::Press(stroke));
            },
            div { class: "pointer-events-none absolute inset-0 opacity-70 [background:radial-gradient(circle_at_14%_8%,color-mix(in_oklab,var(--primary)_12%,transparent),transparent_34%),radial-gradient(circle_at_88%_18%,color-mix(in_oklab,var(--primary)_10%,transparent),transparent_30%)]" }
            header { class: "relative z-10 shrink-0 border-b border-border/70 bg-background/75 px-5 py-4 backdrop-blur-xl",
                div { class: "mx-auto flex w-full max-w-6xl items-center gap-4",
                    div { class: "min-w-0",
                        h1 { class: "text-xl font-semibold tracking-tight", {translate("shortcuts-title")} }
                        p { class: "mt-0.5 text-xs text-muted-foreground", "{subtitle}" }
                    }
                }
            }
            div { class: "relative z-10 min-h-0 flex-1 overflow-y-auto px-5 py-5 [scrollbar-gutter:stable]",
                div { class: "mx-auto w-full max-w-6xl",
                    div {
                        class: "group relative mb-5 flex min-h-40 w-full items-center justify-center overflow-hidden rounded-2xl border border-primary/20 bg-[linear-gradient(135deg,color-mix(in_oklab,var(--glass)_88%,var(--primary)_12%),color-mix(in_oklab,var(--glass)_94%,transparent))] px-20 py-8 text-center shadow-[0_18px_60px_-36px_color-mix(in_oklab,var(--primary)_75%,transparent)] backdrop-blur-2xl",
                        aria_label: translate("shortcuts-try-title"),
                        span { class: "absolute left-5 top-4 flex items-center gap-2 text-xs font-semibold text-foreground/80",
                            span { class: "flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary ring-1 ring-inset ring-primary/20",
                                BuiltinIconView {
                                    icon: vmux_core::BuiltinIcon::Keyboard,
                                    class: "h-4 w-4".to_string(),
                                }
                            }
                            {translate("shortcuts-try-title")}
                        }
                        if !probe.sequence.is_empty() {
                            button {
                                r#type: "button",
                                tabindex: "-1",
                                class: "absolute right-5 top-4 inline-flex items-center gap-2 rounded-lg px-2 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground",
                                onpointerdown: move |event| event.prevent_default(),
                                onclick: move |event| {
                                    event.stop_propagation();
                                    let _ = send(&ShortcutProbeRequest::Clear);
                                },
                                kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[10px]", "Esc" }
                                {translate("shortcuts-clear")}
                            }
                        }
                        div { class: "flex min-w-0 flex-col items-center gap-4",
                            if probe.sequence.is_empty() {
                                span { class: "text-lg font-medium tracking-tight text-foreground/80", {translate("shortcuts-try-hint")} }
                            } else {
                                ShortcutSequence { strokes: probe.sequence.clone() }
                            }
                            if !probe.sequence.is_empty() {
                                span { class: "text-sm font-medium {status_tone}", "{status_label}" }
                            }
                        }
                    }
                    if groups.is_empty() {
                        div { class: "flex min-h-72 items-center justify-center rounded-2xl border border-dashed border-border/70 bg-foreground/[0.015] text-sm text-muted-foreground",
                            if matches!(probe.status, ShortcutProbeStatus::Miss) {
                                {translate("shortcuts-no-match")}
                            } else {
                                {translate("shortcuts-empty")}
                            }
                        }
                    } else {
                        div { class: "columns-1 gap-3 md:columns-2 xl:columns-3",
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
        section { class: "mb-3 inline-block w-full break-inside-avoid overflow-hidden rounded-2xl border border-border/70 bg-[color-mix(in_oklab,var(--glass)_88%,transparent)] shadow-sm",
            h2 { class: "flex items-center gap-2 border-b border-primary/15 bg-primary/[0.075] px-4 py-2.5 text-[11px] font-semibold uppercase tracking-[0.15em] text-primary",
                span { class: "h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_10px_var(--primary)]" }
                "{group.name}"
            }
            div { class: "divide-y divide-border/45",
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
        "flex min-h-12 items-center gap-3 bg-primary/[0.09] px-4 py-2.5 ring-1 ring-inset ring-primary/15"
    } else {
        "flex min-h-12 items-center gap-3 px-4 py-2.5 transition-colors hover:bg-foreground/[0.025]"
    };
    rsx! {
        div { class: "{class}",
            span { class: "min-w-0 flex-1 text-[13px] leading-snug text-foreground", "{entry.name}" }
            div { class: "flex shrink-0 flex-col items-end gap-1.5",
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
            let action = names.join(", ");
            (
                translate_with(
                    "shortcuts-triggered",
                    &[("action", TranslationValue::String(&action))],
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
        span { class: "inline-flex items-center gap-2",
            for (stroke_index, stroke) in strokes.into_iter().enumerate() {
                if stroke_index > 0 {
                    span { class: "px-1 text-lg text-muted-foreground/55", "›" }
                }
                for keycap in stroke.keycaps() {
                    kbd { class: "inline-flex min-w-11 items-center justify-center rounded-xl border border-primary/30 bg-primary/10 px-3 py-2.5 font-mono text-lg font-semibold leading-none text-primary shadow-[inset_0_-2px_0_color-mix(in_oklab,var(--primary)_24%,transparent)]", "{keycap}" }
                }
            }
        }
    }
}

fn shortcut_key_class(emphasized: bool) -> &'static str {
    if emphasized {
        "inline-flex min-w-6 items-center justify-center rounded-md border border-primary/25 bg-primary/10 px-1.5 py-1 font-mono text-[11px] font-semibold leading-none text-primary shadow-[inset_0_-1px_0_color-mix(in_oklab,var(--primary)_24%,transparent)]"
    } else {
        "inline-flex min-w-6 items-center justify-center rounded-md border border-foreground/10 bg-foreground/[0.055] px-1.5 py-1 font-mono text-[11px] font-semibold leading-none text-foreground shadow-[inset_0_-1px_0_color-mix(in_oklab,var(--foreground)_10%,transparent)]"
    }
}
