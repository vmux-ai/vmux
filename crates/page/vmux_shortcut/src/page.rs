#![allow(non_snake_case)]

use std::rc::Rc;

use crate::{
    EVENT, PRESSED_EVENT, ShortcutBinding, ShortcutPressedEvent, ShortcutStroke, ShortcutsEvent,
};
use dioxus::prelude::*;
use vmux_ui::hooks::{use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::BuiltinIconView;
use vmux_ui::platform::{now_millis, sleep_ms};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(|| Rc::new(ShortcutCatalog::default()));
    let mut probe = use_signal(ShortcutProbe::default);
    let _listener = use_listener::<ShortcutsEvent, _>(EVENT, move |event| {
        state.set(Rc::new(ShortcutCatalog::of(event)));
    });
    let _pressed = use_listener::<ShortcutPressedEvent, _>(PRESSED_EVENT, move |event| {
        record_stroke(probe, state, event.stroke, event.pressed_at_ms);
    });

    let probe_value = probe();
    let catalog = state();
    let groups = catalog.filtered(&probe_value);
    let status = probe_value.status(&catalog.shortcuts);
    let status_tone = status.tone_class();
    let count = catalog
        .shortcuts
        .groups
        .iter()
        .map(|group| {
            group
                .entries
                .iter()
                .map(|entry| entry.shortcuts.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let subtitle = translate_with(
        "shortcuts-count",
        &[("count", TranslationValue::Number(count as i64))],
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
                record_stroke(probe, state, stroke, now_millis());
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
                        if !probe_value.sequence.is_empty() {
                            button {
                                r#type: "button",
                                tabindex: "-1",
                                class: "absolute right-5 top-4 inline-flex items-center gap-2 rounded-lg px-2 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground",
                                onpointerdown: move |event| event.prevent_default(),
                                onclick: move |event| {
                                    event.stop_propagation();
                                    let mut current = probe();
                                    current.clear();
                                    probe.set(current);
                                },
                                kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[10px]", "Esc" }
                                {translate("shortcuts-clear")}
                            }
                        }
                        div { class: "flex min-w-0 flex-col items-center gap-4",
                            if probe_value.sequence.is_empty() {
                                span { class: "text-lg font-medium tracking-tight text-foreground/80", {translate("shortcuts-try-hint")} }
                            } else {
                                ShortcutSequence { strokes: probe_value.sequence.clone() }
                            }
                            if !probe_value.sequence.is_empty() {
                                span { class: "text-sm font-medium {status_tone}", "{status.label()}" }
                            }
                        }
                    }
                    if groups.is_empty() {
                        div { class: "flex min-h-72 items-center justify-center rounded-2xl border border-dashed border-border/70 bg-foreground/[0.015] text-sm text-muted-foreground",
                            if probe_value.missed {
                                {translate("shortcuts-no-match")}
                            } else {
                                {translate("shortcuts-empty")}
                            }
                        }
                    } else {
                        div { class: "columns-1 gap-3 md:columns-2 xl:columns-3",
                            for group in groups {
                                ShortcutGroupView {
                                    key: "{group.group_index}",
                                    catalog: catalog.clone(),
                                    group_index: group.group_index,
                                    entry_indices: group.entry_indices,
                                    probe: probe_value.clone(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct ShortcutProbe {
    sequence: Vec<ShortcutStroke>,
    pending: bool,
    pending_at_ms: Option<i64>,
    contextual: bool,
    missed: bool,
    generation: u64,
}

#[derive(Default, PartialEq)]
struct ShortcutCatalog {
    shortcuts: ShortcutsEvent,
}

struct FilteredShortcutGroup {
    group_index: usize,
    entry_indices: Vec<usize>,
}

impl ShortcutCatalog {
    fn of(shortcuts: ShortcutsEvent) -> Self {
        Self { shortcuts }
    }

    fn filtered(&self, probe: &ShortcutProbe) -> Vec<FilteredShortcutGroup> {
        let mut filtered = Vec::new();
        for (group_index, group) in self.shortcuts.groups.iter().enumerate() {
            let mut entry_indices = Vec::new();
            for (entry_index, entry) in group.entries.iter().enumerate() {
                if entry
                    .shortcuts
                    .iter()
                    .any(|shortcut| probe.accepts(shortcut))
                {
                    entry_indices.push(entry_index);
                }
            }
            if !entry_indices.is_empty() {
                filtered.push(FilteredShortcutGroup {
                    group_index,
                    entry_indices,
                });
            }
        }
        filtered
    }
}

impl ShortcutProbe {
    fn capture(&mut self, stroke: ShortcutStroke, shortcuts: &ShortcutsEvent, pressed_at_ms: i64) {
        if stroke.is_plain_escape() {
            self.clear();
            return;
        }
        self.press(stroke, shortcuts, pressed_at_ms);
    }

    fn press(&mut self, stroke: ShortcutStroke, shortcuts: &ShortcutsEvent, pressed_at_ms: i64) {
        self.generation = self.generation.wrapping_add(1);
        if self.pending
            && self.pending_at_ms.is_some_and(|started| {
                pressed_at_ms > started.saturating_add(shortcuts.chord_timeout_ms as i64)
            })
        {
            self.sequence.clear();
            self.pending = false;
            self.pending_at_ms = None;
        }
        let sequence = if self.pending {
            let second = stroke.after(&self.sequence[0]);
            vec![self.sequence[0].clone(), second]
        } else {
            vec![stroke.clone()]
        };
        if self.resolve(&sequence, shortcuts, pressed_at_ms) {
            return;
        }
        if self.pending && self.resolve(std::slice::from_ref(&stroke), shortcuts, pressed_at_ms) {
            return;
        }
        self.sequence = vec![stroke];
        self.pending = false;
        self.pending_at_ms = None;
        self.contextual = false;
        self.missed = true;
    }

    fn resolve(
        &mut self,
        sequence: &[ShortcutStroke],
        shortcuts: &ShortcutsEvent,
        pressed_at_ms: i64,
    ) -> bool {
        if let Some((_, shortcut)) = shortcuts.resolutions().find(|(_, shortcut)| {
            shortcut.strokes.len() > sequence.len() && shortcut.starts_with(sequence)
        }) {
            self.sequence = shortcut.strokes[..sequence.len()].to_vec();
            self.pending = true;
            self.pending_at_ms = Some(pressed_at_ms);
            self.contextual = false;
            self.missed = false;
            return true;
        }
        if let Some((_, shortcut)) = shortcuts
            .resolutions()
            .find(|(_, shortcut)| shortcut.matches(sequence))
        {
            self.sequence = shortcut.strokes.clone();
            self.pending = false;
            self.pending_at_ms = None;
            self.contextual = false;
            self.missed = false;
            return true;
        }
        if let Some((_, shortcut)) = shortcuts.bindings().find(|(_, shortcut)| {
            shortcut.strokes.len() > sequence.len() && shortcut.starts_with(sequence)
        }) {
            self.sequence = shortcut.strokes[..sequence.len()].to_vec();
            self.pending = true;
            self.pending_at_ms = Some(pressed_at_ms);
            self.contextual = true;
            self.missed = false;
            return true;
        }
        if let Some((_, shortcut)) = shortcuts
            .bindings()
            .find(|(_, shortcut)| shortcut.matches(sequence))
        {
            self.sequence = shortcut.strokes.clone();
            self.pending = false;
            self.pending_at_ms = None;
            self.contextual = true;
            self.missed = false;
            return true;
        }
        false
    }

    fn clear(&mut self) {
        self.sequence.clear();
        self.pending = false;
        self.pending_at_ms = None;
        self.contextual = false;
        self.missed = false;
        self.generation = self.generation.wrapping_add(1);
    }

    fn accepts(&self, shortcut: &ShortcutBinding) -> bool {
        if self.sequence.is_empty() {
            return true;
        }
        if self.missed {
            return false;
        }
        if !self.contextual && !shortcut.resolves {
            return false;
        }
        if self.pending {
            shortcut.starts_with(&self.sequence)
        } else {
            shortcut.matches(&self.sequence)
        }
    }

    fn status(&self, shortcuts: &ShortcutsEvent) -> ProbeStatus {
        if self.sequence.is_empty() {
            return ProbeStatus::Idle;
        }
        if self.missed {
            return ProbeStatus::Miss;
        }
        if self.pending {
            return ProbeStatus::Pending;
        }
        if self.contextual {
            let mut labels = Vec::new();
            for (entry, shortcut) in shortcuts.bindings() {
                if !shortcut.matches(&self.sequence) {
                    continue;
                }
                let contexts = shortcut.contexts.join(" / ");
                let label = if contexts.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{} · {}", entry.name, contexts)
                };
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
            return ProbeStatus::Contextual(labels);
        }
        let mut names = Vec::new();
        for (entry, shortcut) in shortcuts.resolutions() {
            if shortcut.matches(&self.sequence) && !names.contains(&entry.name) {
                names.push(entry.name.clone());
            }
        }
        ProbeStatus::Match(names)
    }
}

fn record_stroke(
    mut probe: Signal<ShortcutProbe>,
    state: Signal<Rc<ShortcutCatalog>>,
    stroke: ShortcutStroke,
    pressed_at_ms: i64,
) {
    let timeout_ms = state.read().shortcuts.chord_timeout_ms.min(u32::MAX as u64) as u32;
    let mut next = probe();
    next.capture(stroke, &state.read().shortcuts, pressed_at_ms);
    let generation = next.generation;
    let pending = next.pending;
    let elapsed_ms = next
        .pending_at_ms
        .map(|started| now_millis().saturating_sub(started).max(0) as u64)
        .unwrap_or_default();
    let remaining_ms = u64::from(timeout_ms).saturating_sub(elapsed_ms) as u32;
    probe.set(next);
    if !pending {
        return;
    }
    spawn(async move {
        sleep_ms(remaining_ms).await;
        let mut current = probe();
        if !current.pending || current.generation != generation {
            return;
        }
        current.clear();
        probe.set(current);
    });
}

enum ProbeStatus {
    Idle,
    Pending,
    Match(Vec<String>),
    Contextual(Vec<String>),
    Miss,
}

impl ProbeStatus {
    fn label(&self) -> String {
        match self {
            Self::Idle => translate("shortcuts-try-hint"),
            Self::Pending => translate("shortcuts-waiting"),
            Self::Match(names) => {
                let action = names.join(", ");
                translate_with(
                    "shortcuts-triggered",
                    &[("action", TranslationValue::String(&action))],
                )
            }
            Self::Contextual(labels) => labels.join(", "),
            Self::Miss => translate("shortcuts-no-match"),
        }
    }

    fn tone_class(&self) -> &'static str {
        match self {
            Self::Idle => "text-muted-foreground",
            Self::Pending => "text-amber-500",
            Self::Match(_) => "text-primary",
            Self::Contextual(_) => "text-sky-500",
            Self::Miss => "text-rose-500",
        }
    }
}

impl ShortcutStroke {
    fn is_plain_escape(&self) -> bool {
        self.code == "Escape" && !self.ctrl && !self.shift && !self.alt && !self.super_key
    }

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
fn ShortcutGroupView(
    catalog: Rc<ShortcutCatalog>,
    group_index: usize,
    entry_indices: Vec<usize>,
    probe: ShortcutProbe,
) -> Element {
    let group = &catalog.shortcuts.groups[group_index];
    rsx! {
        section { class: "mb-3 inline-block w-full break-inside-avoid overflow-hidden rounded-2xl border border-border/70 bg-[color-mix(in_oklab,var(--glass)_88%,transparent)] shadow-sm",
            h2 { class: "flex items-center gap-2 border-b border-primary/15 bg-primary/[0.075] px-4 py-2.5 text-[11px] font-semibold uppercase tracking-[0.15em] text-primary",
                span { class: "h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_10px_var(--primary)]" }
                "{group.name}"
            }
            div { class: "divide-y divide-border/45",
                for entry_index in entry_indices {
                    {
                        let entry_id = group.entries[entry_index].id.clone();
                        rsx! {
                            ShortcutEntryView {
                                key: "{entry_id}",
                                catalog: catalog.clone(),
                                group_index,
                                entry_index,
                                probe: probe.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ShortcutEntryView(
    catalog: Rc<ShortcutCatalog>,
    group_index: usize,
    entry_index: usize,
    probe: ShortcutProbe,
) -> Element {
    let entry = &catalog.shortcuts.groups[group_index].entries[entry_index];
    let selected = !probe.sequence.is_empty()
        && entry
            .shortcuts
            .iter()
            .any(|shortcut| probe.accepts(shortcut));
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
                        let emphasized = !probe.sequence.is_empty() && probe.accepts(shortcut);
                        rsx! {
                            span { key: "{shortcut_index}", class: "flex flex-col items-end gap-1",
                                span { class: "inline-flex items-center gap-1",
                                    for (stroke_index, stroke) in shortcut.strokes.iter().enumerate() {
                                        if stroke_index > 0 {
                                            span { class: "px-0.5 text-[11px] text-muted-foreground/55", "›" }
                                        }
                                        for keycap in stroke.keycaps() {
                                            kbd { class: shortcut_key_class(emphasized), "{keycap}" }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShortcutEntry, ShortcutGroup};

    fn stroke(code: &str, ctrl: bool) -> ShortcutStroke {
        ShortcutStroke {
            code: code.to_string(),
            label: code.to_string(),
            ctrl,
            ..Default::default()
        }
    }

    fn shortcuts() -> ShortcutsEvent {
        ShortcutsEvent {
            groups: vec![ShortcutGroup {
                name: "Stack".into(),
                entries: vec![ShortcutEntry {
                    id: "stack_close".into(),
                    name: "Close Stack".into(),
                    shortcuts: vec![ShortcutBinding {
                        label: "⌃G, X".into(),
                        strokes: vec![stroke("KeyG", true), stroke("KeyX", false)],
                        resolves: true,
                        contexts: Vec::new(),
                    }],
                }],
            }],
            chord_timeout_ms: 1000,
        }
    }

    #[test]
    fn probe_waits_for_and_resolves_a_chord() {
        let shortcuts = shortcuts();
        let mut probe = ShortcutProbe::default();

        probe.press(stroke("KeyG", true), &shortcuts, 1_000);
        assert!(probe.pending);
        assert!(matches!(probe.status(&shortcuts), ProbeStatus::Pending));

        probe.press(stroke("KeyX", true), &shortcuts, 1_500);
        assert!(!probe.pending);
        assert!(matches!(probe.status(&shortcuts), ProbeStatus::Match(_)));
    }

    #[test]
    fn probe_restarts_from_a_failed_chord_second_key() {
        let mut shortcuts = shortcuts();
        shortcuts.groups[0].entries.push(ShortcutEntry {
            id: "new".into(),
            name: "New".into(),
            shortcuts: vec![ShortcutBinding {
                label: "N".into(),
                strokes: vec![stroke("KeyN", false)],
                resolves: true,
                contexts: Vec::new(),
            }],
        });
        let mut probe = ShortcutProbe::default();

        probe.press(stroke("KeyG", true), &shortcuts, 1_000);
        probe.press(stroke("KeyN", false), &shortcuts, 1_500);

        assert_eq!(probe.sequence, [stroke("KeyN", false)]);
        assert!(!probe.missed);
    }

    #[test]
    fn chord_prefix_wins_over_a_direct_binding_like_a_tmux_leader() {
        let mut shortcuts = shortcuts();
        shortcuts.groups[0].entries[0].shortcuts[0].strokes[0] = stroke("KeyB", true);
        shortcuts.groups[0].entries.push(ShortcutEntry {
            id: "leader".into(),
            name: "Leader".into(),
            shortcuts: vec![ShortcutBinding {
                label: "⌃B".into(),
                strokes: vec![stroke("KeyB", true)],
                resolves: true,
                contexts: Vec::new(),
            }],
        });
        let mut probe = ShortcutProbe::default();

        probe.press(stroke("KeyB", true), &shortcuts, 1_000);
        assert!(probe.pending);
        assert!(matches!(probe.status(&shortcuts), ProbeStatus::Pending));

        probe.press(stroke("KeyX", false), &shortcuts, 1_500);

        assert!(!probe.pending);
        assert!(matches!(probe.status(&shortcuts), ProbeStatus::Match(_)));
    }

    #[test]
    fn escape_can_be_matched_like_any_other_shortcut() {
        let shortcuts = ShortcutsEvent {
            groups: vec![ShortcutGroup {
                name: "General".into(),
                entries: vec![ShortcutEntry {
                    id: "escape".into(),
                    name: "Escape".into(),
                    shortcuts: vec![ShortcutBinding {
                        label: "Esc".into(),
                        strokes: vec![stroke("Escape", false)],
                        resolves: true,
                        contexts: Vec::new(),
                    }],
                }],
            }],
            chord_timeout_ms: 1000,
        };
        let mut probe = ShortcutProbe::default();

        probe.press(stroke("Escape", false), &shortcuts, 1_000);

        assert!(matches!(probe.status(&shortcuts), ProbeStatus::Match(_)));
    }

    #[test]
    fn capture_escape_clears_the_current_shortcut() {
        let shortcuts = shortcuts();
        let mut probe = ShortcutProbe::default();

        probe.capture(stroke("KeyG", true), &shortcuts, 1_000);
        assert!(probe.pending);

        probe.capture(stroke("Escape", false), &shortcuts, 1_100);

        assert!(probe.sequence.is_empty());
        assert!(!probe.pending);
        assert!(!probe.missed);
    }

    #[test]
    fn expired_chord_does_not_accept_a_late_second_key() {
        let shortcuts = shortcuts();
        let mut probe = ShortcutProbe::default();

        probe.press(stroke("KeyG", true), &shortcuts, 1_000);
        probe.press(stroke("KeyX", true), &shortcuts, 2_001);

        assert!(probe.missed);
        assert_eq!(probe.sequence, [stroke("KeyX", true)]);
    }

    #[test]
    fn contextual_shortcut_reports_its_required_context() {
        let shortcuts = ShortcutsEvent {
            groups: vec![ShortcutGroup {
                name: "Chat".into(),
                entries: vec![ShortcutEntry {
                    id: "chat_dismiss_selector".into(),
                    name: "Close Selector".into(),
                    shortcuts: vec![ShortcutBinding {
                        label: "Esc".into(),
                        strokes: vec![stroke("Escape", false)],
                        resolves: false,
                        contexts: vec!["chat.selector".into()],
                    }],
                }],
            }],
            chord_timeout_ms: 1000,
        };
        let mut probe = ShortcutProbe::default();

        probe.press(stroke("Escape", false), &shortcuts, 1_000);

        assert!(probe.contextual);
        assert_eq!(
            probe.status(&shortcuts).label(),
            "Close Selector · chat.selector"
        );
    }
}
