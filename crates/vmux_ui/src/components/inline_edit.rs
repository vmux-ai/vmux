use crate::{focus::FocusClaim, ime::use_ime_guard, platform::sleep_ms};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_INLINE_EDIT_ID: AtomicUsize = AtomicUsize::new(0);

#[component]
pub fn EditableText(
    value: String,
    editing: Signal<bool>,
    draft: Signal<String>,
    display_class: String,
    input_class: String,
    title: String,
    #[props(default)] placeholder: String,
    #[props(default = true)] caret_at_end: bool,
    #[props(default)] on_active_change: Option<EventHandler<bool>>,
    on_commit: EventHandler<String>,
) -> Element {
    let mut editing = editing;
    let mut draft = draft;
    let mut optimistic = use_signal(|| None::<OptimisticText>);
    let mut optimistic_generation = use_signal(|| 0u64);
    let trigger_id = use_hook(|| {
        format!(
            "vmux-editable-text-{}",
            NEXT_INLINE_EDIT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let observed_value = value.clone();
    use_effect(use_reactive!(|observed_value| {
        let acknowledged = {
            let pending = optimistic.peek();
            pending.as_ref().is_some_and(|pending| {
                pending.expected == observed_value || pending.baseline != observed_value
            })
        };
        if acknowledged {
            optimistic.set(None);
        }
    }));
    let display_value = optimistic
        .read()
        .as_ref()
        .map(|pending| pending.expected.clone())
        .unwrap_or_else(|| value.clone());
    let requested_value = display_value.clone();
    let mut activated = use_signal(|| false);
    use_effect(move || {
        let active = editing();
        if active == *activated.peek() {
            return;
        }
        activated.set(active);
        if active {
            draft.set(requested_value.clone());
        }
    });
    let accessible_label = format!("{title}: {display_value}");
    if editing() {
        let cancel_value = display_value.clone();
        let committed_value = display_value.trim().to_string();
        return rsx! {
            InlineEdit {
                draft,
                class: input_class,
                placeholder,
                aria_label: accessible_label,
                caret_at_end,
                on_active_change,
                restore_focus_id: trigger_id.clone(),
                on_commit: move |value: String| {
                    editing.set(false);
                    if value != committed_value {
                        let generation = *optimistic_generation.peek() + 1;
                        optimistic_generation.set(generation);
                        optimistic.set(Some(OptimisticText {
                            expected: value.clone(),
                            baseline: committed_value.clone(),
                            generation,
                        }));
                        spawn(async move {
                            sleep_ms(1_500).await;
                            if optimistic
                                .peek()
                                .as_ref()
                                .is_some_and(|pending| pending.generation == generation)
                            {
                                optimistic.set(None);
                            }
                        });
                        on_commit.call(value);
                    }
                },
                on_cancel: move |_| {
                    draft.set(cancel_value.clone());
                    editing.set(false);
                },
            }
        };
    }
    let edit_value = display_value.clone();
    let keyboard_value = display_value.clone();
    rsx! {
        button {
            id: "{trigger_id}",
            r#type: "button",
            class: display_class,
            title: title.clone(),
            aria_label: accessible_label,
            onclick: move |event| {
                event.prevent_default();
                event.stop_propagation();
                draft.set(edit_value.clone());
                editing.set(true);
            },
            onkeydown: move |event| {
                let activates = match event.key() {
                    Key::Enter => true,
                    Key::Character(value) => value == " ",
                    _ => false,
                };
                if !activates {
                    return;
                }
                event.prevent_default();
                event.stop_propagation();
                draft.set(keyboard_value.clone());
                editing.set(true);
            },
            onpointerdown: move |event| event.stop_propagation(),
            "{display_value}"
        }
    }
}

#[derive(Clone, PartialEq)]
struct OptimisticText {
    expected: String,
    baseline: String,
    generation: u64,
}

#[component]
pub fn InlineEdit(
    draft: Signal<String>,
    class: String,
    #[props(default)] placeholder: String,
    aria_label: String,
    #[props(default = true)] caret_at_end: bool,
    #[props(default)] allow_empty: bool,
    #[props(default)] on_active_change: Option<EventHandler<bool>>,
    #[props(default)] restore_focus_id: Option<String>,
    on_commit: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut draft = draft;
    let mut finished = use_signal(|| false);
    let ime = use_ime_guard();
    let focus_return = FocusReturn(restore_focus_id);
    let id = use_hook(|| {
        format!(
            "vmux-inline-edit-{}",
            NEXT_INLINE_EDIT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let dropped = on_active_change;
    use_drop(move || {
        if let Some(callback) = dropped {
            callback.call(false);
        }
    });

    rsx! {
        input {
            id: "{id}",
            r#type: "text",
            class,
            placeholder,
            aria_label,
            value: "{draft}",
            autofocus: true,
            autocomplete: "off",
            autocapitalize: "off",
            autocorrect: "off",
            spellcheck: "false",
            onclick: move |event: Event<MouseData>| event.stop_propagation(),
            onpointerdown: move |event: Event<PointerData>| event.stop_propagation(),
            oncontextmenu: move |event: Event<MouseData>| {
                event.stop_propagation();
            },
            onmounted: move |event: Event<MountedData>| {
                if let Some(callback) = on_active_change {
                    callback.call(true);
                }
                let id = id.clone();
                spawn(async move {
                    let _ = event.data().set_focus(true).await;
                    if caret_at_end {
                        FocusClaim::new(id).caret_at_end().request();
                    }
                });
            },
            oninput: move |event| draft.set(event.value()),
            oncompositionstart: move |_| ime.start(),
            oncompositionend: move |_| ime.commit(),
            onkeydown: move |event: Event<KeyboardData>| {
                event.stop_propagation();
                if ime.swallows(&event) {
                    return;
                }
                match event.key() {
                    Key::Enter => {
                        event.prevent_default();
                        let name = draft().trim().to_string();
                        if !finished() {
                            finished.set(true);
                            if name.is_empty() && !allow_empty {
                                on_cancel.call(());
                            } else {
                                on_commit.call(name);
                            }
                            focus_return.request();
                        }
                    }
                    Key::Escape => {
                        event.prevent_default();
                        if !finished() {
                            finished.set(true);
                            on_cancel.call(());
                            focus_return.request();
                        }
                    }
                    _ => {}
                }
            },
            onblur: move |_| {
                if finished() {
                    return;
                }
                finished.set(true);
                let name = draft().trim().to_string();
                if name.is_empty() && !allow_empty {
                    on_cancel.call(());
                } else {
                    on_commit.call(name);
                }
            },
        }
    }
}

#[derive(Clone)]
struct FocusReturn(Option<String>);

impl FocusReturn {
    fn request(&self) {
        let Some(id) = self.0.clone() else {
            return;
        };
        spawn(async move {
            sleep_ms(0).await;
            FocusClaim::new(id).request();
        });
    }
}
