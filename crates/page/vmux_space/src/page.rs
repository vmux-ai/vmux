#![allow(non_snake_case)]

use crate::event::{
    SPACE_KEY_EVENT, SPACES_LIST_EVENT, SpaceCommandEvent, SpaceKey, SpaceRow, SpacesListEvent,
};
use dioxus::prelude::*;
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::hooks::{MenuDirection, send, use_key_claim, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(SpacesListEvent::default);
    let mut selected = use_signal(|| 0usize);
    let mut new_name = use_signal(String::new);

    let _listener = use_listener::<SpacesListEvent, _>(SPACES_LIST_EVENT, move |data| {
        selected.set(0);
        state.set(data);
    });

    let keys = use_key_claim(Unclaimed::Types, || vec!["spaces".to_string()]);
    SpaceKeys { state, selected }.listen();
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });

    let spaces = state.read().spaces.clone();
    let count = spaces.len();
    let sel = selected().min(count.saturating_sub(1));
    let active_name = spaces
        .iter()
        .find(|space| space.is_active)
        .map(|space| space.name.clone())
        .unwrap_or_else(|| {
            translate_with(
                "spaces-default-name",
                &[("number", TranslationValue::Number(1))],
            )
        });

    rsx! {
        div {
            id: "spaces-root",
            tabindex: "0",
            class: "flex h-full min-h-0 flex-col bg-background text-foreground outline-none",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onkeydown: move |e| keys.on_keydown(&e, |_| false),
            div { class: "flex items-center justify-between border-b border-border px-5 py-4",
                div { class: "min-w-0",
                    h1 { class: "text-lg font-semibold", {translate("spaces-title")} }
                    div { class: "mt-1 truncate text-xs text-muted-foreground", "{active_name}" }
                }
                div { class: "flex shrink-0 items-center gap-2",
                    input {
                        class: "w-44 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-cyan-400/50",
                        r#type: "text",
                        placeholder: translate("spaces-new-placeholder"),
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                        onkeydown: move |e| {
                            e.stop_propagation();
                            if e.key() == Key::Enter {
                                emit_command("new", None, Some(new_space_name(&new_name(), count)));
                                new_name.set(String::new());
                            }
                        },
                    }
                    button {
                        class: "rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground transition-colors hover:border-cyan-400/40 hover:bg-foreground/[0.04]",
                        onclick: move |_| {
                            emit_command("new", None, Some(new_space_name(&new_name(), count)));
                            new_name.set(String::new());
                        },
                        {translate("common-new")}
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-3",
                if spaces.is_empty() {
                    div { class: "flex h-full items-center justify-center text-sm text-muted-foreground", {translate("spaces-empty")} }
                } else {
                    div { class: "flex flex-col gap-2",
                        for (index, space) in spaces.iter().enumerate() {
                            SpaceRowView {
                                key: "{space.id}",
                                space: space.clone(),
                                selected: index == sel,
                                deletable: count > 1,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The spaces page's keyboard, on the far side of the keymap.
///
/// Nothing here names a key. The page hands the stroke over, the core decides, and this performs
/// the verb it came back as — which is the only reason `Ctrl+n` can be rebound in `settings.json`
/// without this file agreeing.
///
/// Both fields are signals rather than values, because the answer arrives in a listener registered
/// on first render: a captured list would be one keystroke stale by the time a key was pressed.
#[derive(Clone, Copy)]
struct SpaceKeys {
    state: Signal<SpacesListEvent>,
    selected: Signal<usize>,
}

impl SpaceKeys {
    /// Take the host's answers about keys this page handed over.
    fn listen(self) {
        let mut keys = self;
        let _resolved = use_listener::<SpaceKey, _>(SPACE_KEY_EVENT, move |key| keys.apply(key));
    }

    fn apply(&mut self, key: SpaceKey) {
        match key {
            SpaceKey::Next => self.move_selection(MenuDirection::Next),
            SpaceKey::Previous => self.move_selection(MenuDirection::Previous),
            SpaceKey::Attach => self.attach(),
            SpaceKey::Delete => self.delete(),
        }
    }

    /// Clamped at both ends rather than wrapping, which is what this list has always done.
    fn move_selection(&mut self, direction: MenuDirection) {
        let count = self.state.peek().spaces.len();
        let from = self.row();
        let landed = match direction {
            MenuDirection::Next => (from + 1).min(count.saturating_sub(1)),
            MenuDirection::Previous => from.saturating_sub(1),
        };
        self.selected.set(landed);
    }

    fn attach(&self) {
        let Some(id) = self.selected_id() else {
            return;
        };
        emit_command("attach", Some(id), None);
    }

    /// The last space is never deleted: the window it holds would have nowhere to put a tab.
    fn delete(&self) {
        if self.state.peek().spaces.len() <= 1 {
            return;
        }
        let Some(id) = self.selected_id() else {
            return;
        };
        emit_command("delete", Some(id), None);
    }

    /// The highlighted row, clamped to what the list holds now, so a selection survives a list that
    /// shrank under it.
    fn row(&self) -> usize {
        let count = self.state.peek().spaces.len();
        (*self.selected.peek()).min(count.saturating_sub(1))
    }

    fn selected_id(&self) -> Option<String> {
        let row = self.row();
        let state = self.state.peek();
        state.spaces.get(row).map(|space| space.id.clone())
    }
}

fn new_space_name(typed: &str, count: usize) -> String {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        translate_with(
            "spaces-default-name",
            &[("number", TranslationValue::Number((count + 1) as i64))],
        )
    } else {
        trimmed.to_string()
    }
}

fn emit_command(command: &str, space_id: Option<String>, name: Option<String>) {
    let _ = send(&SpaceCommandEvent {
        command: command.to_string(),
        space_id,
        name,
    });
}

#[component]
fn SpaceRowView(space: SpaceRow, selected: bool, deletable: bool) -> Element {
    let nav_id = space.id.clone();
    let delete_id = space.id.clone();
    let class = if selected {
        "flex cursor-pointer items-center justify-between rounded-lg border border-cyan-400/40 bg-cyan-400/[0.08] px-3 py-3 shadow-[0_0_18px_-6px_rgba(34,211,238,0.5)]"
    } else {
        "flex cursor-pointer items-center justify-between rounded-lg border border-border bg-card px-3 py-3 transition-colors hover:border-cyan-400/40 hover:bg-foreground/[0.04]"
    };
    let tab_label = translate_with(
        "spaces-tabs",
        &[("count", TranslationValue::Number(space.tab_count as i64))],
    );

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                emit_command("attach", Some(nav_id.clone()), None);
            },
            div { class: "min-w-0",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "truncate text-sm font-medium text-foreground", "{space.name}" }
                    if space.is_active {
                        span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-600 dark:text-blue-300", {translate("common-active")} }
                    }
                }
                div { class: "mt-1 truncate text-xs text-muted-foreground", "{space.profile}" }
            }
            div { class: "ml-3 flex shrink-0 items-center gap-2",
                div { class: "text-xs text-muted-foreground", "{tab_label}" }
                if deletable {
                    button {
                        class: "flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground",
                        title: translate("spaces-delete"),
                        "aria-label": translate("spaces-delete"),
                        onclick: move |e| {
                            e.stop_propagation();
                            emit_command("delete", Some(delete_id.clone()), None);
                        },
                        span { class: "text-base leading-none", "\u{00d7}" }
                    }
                }
            }
        }
    }
}
