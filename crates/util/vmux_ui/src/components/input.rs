use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::cn::cn;
use crate::dom::DomCaret;

static NEXT_ACTION_INPUT_ID: AtomicUsize = AtomicUsize::new(0);

const INPUT: &str = "relative box-border flex cursor-pointer flex-row items-center justify-between gap-1 rounded-md border-0 bg-background px-3 py-2 text-muted-foreground shadow-[inset_0_0_0_1px_var(--border)] transition-colors placeholder:text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--primary)] disabled:cursor-not-allowed dark:bg-[color-mix(in_oklab,#FFFFFF26_30%,transparent)] dark:hover:bg-[color-mix(in_oklab,#FFFFFF26_50%,transparent)]";

#[component]
pub fn Input(
    oninput: Option<EventHandler<FormEvent>>,
    onchange: Option<EventHandler<FormEvent>>,
    oninvalid: Option<EventHandler<FormEvent>>,
    onselect: Option<EventHandler<SelectionEvent>>,
    onselectionchange: Option<EventHandler<SelectionEvent>>,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
    onfocusin: Option<EventHandler<FocusEvent>>,
    onfocusout: Option<EventHandler<FocusEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    onkeypress: Option<EventHandler<KeyboardEvent>>,
    onkeyup: Option<EventHandler<KeyboardEvent>>,
    oncompositionstart: Option<EventHandler<CompositionEvent>>,
    oncompositionupdate: Option<EventHandler<CompositionEvent>>,
    oncompositionend: Option<EventHandler<CompositionEvent>>,
    oncopy: Option<EventHandler<ClipboardEvent>>,
    oncut: Option<EventHandler<ClipboardEvent>>,
    onpaste: Option<EventHandler<ClipboardEvent>>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = input)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(input { class: INPUT });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        input {
            oninput: move |e| _ = oninput.map(|callback| callback(e)),
            onchange: move |e| _ = onchange.map(|callback| callback(e)),
            oninvalid: move |e| _ = oninvalid.map(|callback| callback(e)),
            onselect: move |e| _ = onselect.map(|callback| callback(e)),
            onselectionchange: move |e| _ = onselectionchange.map(|callback| callback(e)),
            onfocus: move |e| _ = onfocus.map(|callback| callback(e)),
            onblur: move |e| _ = onblur.map(|callback| callback(e)),
            onfocusin: move |e| _ = onfocusin.map(|callback| callback(e)),
            onfocusout: move |e| _ = onfocusout.map(|callback| callback(e)),
            onkeydown: move |e| _ = onkeydown.map(|callback| callback(e)),
            onkeypress: move |e| _ = onkeypress.map(|callback| callback(e)),
            onkeyup: move |e| _ = onkeyup.map(|callback| callback(e)),
            oncompositionstart: move |e| _ = oncompositionstart.map(|callback| callback(e)),
            oncompositionupdate: move |e| _ = oncompositionupdate.map(|callback| callback(e)),
            oncompositionend: move |e| _ = oncompositionend.map(|callback| callback(e)),
            oncopy: move |e| _ = oncopy.map(|callback| callback(e)),
            oncut: move |e| _ = oncut.map(|callback| callback(e)),
            onpaste: move |e| _ = onpaste.map(|callback| callback(e)),
            ..merged,
            {children}
        }
    }
}

#[component]
pub fn ActionInput(
    value: String,
    placeholder: String,
    aria_label: String,
    action_label: String,
    class: String,
    input_class: String,
    oninput: EventHandler<FormEvent>,
    onkeydown: EventHandler<KeyboardEvent>,
    onblur: EventHandler<FocusEvent>,
    onaction: EventHandler<()>,
    children: Element,
    #[props(default)] autofocus: bool,
    #[props(default)] select_all: bool,
    #[props(default)] selection_key: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] onfocus: Option<EventHandler<FocusEvent>>,
) -> Element {
    let id = use_hook(|| {
        format!(
            "vmux-action-input-{}",
            NEXT_ACTION_INPUT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let mounted_id = id.clone();
    let action_id = id.clone();
    let selection_id = id.clone();
    let mut observed_selection_key = use_signal(|| selection_key.clone());
    use_effect(use_reactive!(|selection_key| {
        if selection_key == *observed_selection_key.peek() {
            return;
        }
        observed_selection_key.set(selection_key);
        DomCaret::select_all_in_next_frame(&selection_id);
    }));
    rsx! {
        div {
            class: cn([
                "relative flex min-w-0 items-center",
                class.as_str(),
            ]),
            input {
                id,
                r#type: "text",
                class: cn([
                    "h-full min-w-0 flex-1 bg-transparent outline-none",
                    input_class.as_str(),
                ]),
                value,
                placeholder,
                aria_label,
                autofocus,
                autocomplete: "off",
                onmounted: move |_| {
                    if select_all {
                        DomCaret::select_all_in_next_frame(&mounted_id);
                    }
                },
                onclick: move |event| {
                    if let Some(handler) = onclick {
                        handler.call(event);
                    }
                },
                onfocus: move |event| {
                    if let Some(handler) = onfocus {
                        handler.call(event);
                    }
                },
                oninput: move |event| oninput.call(event),
                onkeydown: move |event| onkeydown.call(event),
                onblur: move |event| onblur.call(event),
            }
            button {
                r#type: "button",
                tabindex: "-1",
                class: "mr-1 flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.07] hover:text-foreground",
                title: action_label.clone(),
                aria_label: action_label,
                onpointerdown: move |event| {
                    event.prevent_default();
                    event.stop_propagation();
                },
                onclick: move |event| {
                    event.stop_propagation();
                    onaction.call(());
                    DomCaret::select_all_in_next_frame(&action_id);
                },
                {children}
            }
        }
    }
}
