use crate::{focus::FocusClaim, ime::use_ime_guard, platform::Platform};
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
    #[props(default)] empty_display: String,
    #[props(default = true)] caret_at_end: bool,
    #[props(default)] multiline: bool,
    #[props(default)] allow_empty: bool,
    #[props(default = true)] trim: bool,
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
        let committed_value = if trim {
            display_value.trim().to_string()
        } else {
            display_value.clone()
        };
        if multiline {
            return rsx! {
                InlineTextarea {
                    draft,
                    class: input_class,
                    placeholder,
                    aria_label: accessible_label,
                    allow_empty,
                    trim,
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
                                Platform::sleep(1_500).await;
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
        return rsx! {
            InlineEdit {
                draft,
                class: input_class,
                placeholder,
                aria_label: accessible_label,
                caret_at_end,
                allow_empty,
                trim,
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
                            Platform::sleep(1_500).await;
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
    let rendered_value = if display_value.is_empty() {
        empty_display
    } else {
        display_value.clone()
    };
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
            "{rendered_value}"
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
    #[props(default = true)] trim: bool,
    #[props(default)] on_active_change: Option<EventHandler<bool>>,
    #[props(default)] restore_focus_id: Option<String>,
    on_commit: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let controller = use_inline_editing_controller(
        draft,
        allow_empty,
        trim,
        on_active_change,
        restore_focus_id,
        on_commit,
        on_cancel,
    );
    let id = use_hook(|| {
        format!(
            "vmux-inline-edit-{}",
            NEXT_INLINE_EDIT_ID.fetch_add(1, Ordering::Relaxed)
        )
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
            onmounted: move |event: Event<MountedData>| controller.activate(event, Some(id.clone()), caret_at_end),
            oninput: move |event| controller.input(event.value()),
            oncompositionstart: move |_| controller.composition_started(),
            oncompositionend: move |_| controller.composition_committed(),
            onkeydown: move |event: Event<KeyboardData>| {
                event.stop_propagation();
                if controller.swallows(&event) {
                    return;
                }
                match event.key() {
                    Key::Enter => {
                        event.prevent_default();
                        controller.commit(true);
                    }
                    Key::Escape => {
                        event.prevent_default();
                        controller.cancel(true);
                    }
                    _ => {}
                }
            },
            onblur: move |_| controller.commit(false),
        }
    }
}

#[component]
pub fn InlineTextarea(
    draft: Signal<String>,
    class: String,
    #[props(default)] placeholder: String,
    aria_label: String,
    #[props(default)] allow_empty: bool,
    #[props(default = true)] trim: bool,
    #[props(default)] on_active_change: Option<EventHandler<bool>>,
    #[props(default)] restore_focus_id: Option<String>,
    on_commit: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let controller = use_inline_editing_controller(
        draft,
        allow_empty,
        trim,
        on_active_change,
        restore_focus_id,
        on_commit,
        on_cancel,
    );
    let id = use_hook(|| {
        format!(
            "vmux-inline-edit-{}",
            NEXT_INLINE_EDIT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    rsx! {
        textarea {
            id: "{id}",
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
            oncontextmenu: move |event: Event<MouseData>| event.stop_propagation(),
            onmounted: move |event: Event<MountedData>| controller.activate(event, None, false),
            oninput: move |event| controller.input(event.value()),
            oncompositionstart: move |_| controller.composition_started(),
            oncompositionend: move |_| controller.composition_committed(),
            onkeydown: move |event: Event<KeyboardData>| {
                event.stop_propagation();
                if controller.swallows(&event) || event.key() != Key::Escape {
                    return;
                }
                event.prevent_default();
                controller.cancel(true);
            },
            onblur: move |_| controller.commit(false),
        }
    }
}

#[derive(Clone, Copy)]
struct InlineEditingController {
    draft: Signal<String>,
    finished: Signal<bool>,
    ime: crate::ime::ImeGuard,
    allow_empty: bool,
    trim: bool,
    active: Option<EventHandler<bool>>,
    commit_handler: EventHandler<String>,
    cancel_handler: EventHandler<()>,
    focus_return: FocusReturn,
}

impl InlineEditingController {
    fn activate(self, event: Event<MountedData>, id: Option<String>, caret_at_end: bool) {
        if let Some(callback) = self.active {
            callback.call(true);
        }
        spawn(async move {
            let _ = event.data().set_focus(true).await;
            if caret_at_end && let Some(id) = id {
                FocusClaim::new(id).caret_at_end().request();
            }
        });
    }

    fn input(mut self, value: String) {
        self.draft.set(value);
    }

    fn composition_started(self) {
        self.ime.start();
    }

    fn composition_committed(self) {
        self.ime.commit();
    }

    fn swallows(self, event: &Event<KeyboardData>) -> bool {
        self.ime.swallows(event)
    }

    fn commit(mut self, restore_focus: bool) {
        if (self.finished)() {
            return;
        }
        self.finished.set(true);
        let value = if self.trim {
            (self.draft)().trim().to_string()
        } else {
            (self.draft)()
        };
        if value.is_empty() && !self.allow_empty {
            self.cancel_handler.call(());
        } else {
            self.commit_handler.call(value);
        }
        if restore_focus {
            self.focus_return.request();
        }
    }

    fn cancel(mut self, restore_focus: bool) {
        if (self.finished)() {
            return;
        }
        self.finished.set(true);
        self.cancel_handler.call(());
        if restore_focus {
            self.focus_return.request();
        }
    }
}

fn use_inline_editing_controller(
    draft: Signal<String>,
    allow_empty: bool,
    trim: bool,
    active: Option<EventHandler<bool>>,
    restore_focus_id: Option<String>,
    commit_handler: EventHandler<String>,
    cancel_handler: EventHandler<()>,
) -> InlineEditingController {
    let controller = InlineEditingController {
        draft,
        finished: use_signal(|| false),
        ime: use_ime_guard(),
        allow_empty,
        trim,
        active,
        commit_handler,
        cancel_handler,
        focus_return: use_hook(|| FocusReturn(CopyValue::new(restore_focus_id))),
    };
    use_drop(move || {
        if let Some(callback) = controller.active {
            callback.call(false);
        }
    });
    controller
}

#[derive(Clone, Copy)]
struct FocusReturn(CopyValue<Option<String>>);

impl FocusReturn {
    fn request(&self) {
        let Some(id) = self.0.peek().clone() else {
            return;
        };
        spawn(async move {
            Platform::sleep(0).await;
            FocusClaim::new(id).request();
        });
    }
}
