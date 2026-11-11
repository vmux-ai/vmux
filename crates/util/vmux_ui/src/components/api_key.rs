use crate::i18n::translate;
use dioxus::prelude::*;

#[component]
pub fn ApiKeyEntry(
    pending: bool,
    failed: bool,
    on_submit: EventHandler<vmux_api::Secret>,
) -> Element {
    let mut key = use_signal(String::new);
    let value = key();
    rsx! {
        div { class: "flex flex-col gap-2",
            label { class: "text-xs text-muted-foreground", {translate("onboarding-api-key")} }
            div { class: "flex flex-wrap gap-2",
                input {
                    r#type: "password",
                    autocomplete: "off",
                    aria_label: translate("onboarding-api-key"),
                    class: "min-w-0 flex-1 rounded-lg bg-muted/40 px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30",
                    value,
                    disabled: pending,
                    oninput: move |event| key.set(event.value()),
                    onkeydown: move |event| {
                        if event.key() == Key::Escape
                            || event.modifiers().ctrl() && event.key() == Key::Character("c".into())
                        {
                            return;
                        }
                        event.stop_propagation();
                        if event.key() == Key::Enter {
                            event.prevent_default();
                            if !pending && !key.peek().trim().is_empty() {
                                on_submit.call(vmux_api::Secret::new(std::mem::take(&mut *key.write())));
                            }
                        }
                    },
                }
                button {
                    r#type: "button",
                    aria_label: translate("onboarding-save-key"),
                    class: "rounded-lg bg-muted/50 px-3 py-2 text-sm hover:bg-muted/70 disabled:opacity-40",
                    disabled: pending || key.peek().trim().is_empty(),
                    onclick: move |_| {
                        on_submit.call(vmux_api::Secret::new(std::mem::take(&mut *key.write())));
                    },
                    {translate("onboarding-save-key")}
                }
            }
            if failed {
                p { class: "text-xs text-destructive", {translate("onboarding-key-failed")} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedFormData, SerializedHtmlEventConverter,
        SerializedKeyboardData, SerializedMouseData,
    };
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, Default)]
    struct Keys(Rc<RefCell<Vec<vmux_api::Secret>>>);

    #[component]
    fn KeyInput() -> Element {
        let keys = use_context::<Keys>();
        rsx! { ApiKeyEntry { pending: false, failed: false, on_submit: move |key| keys.0.borrow_mut().push(key) } }
    }

    #[test]
    fn saving_a_key_uses_a_password_input_and_clears_the_value_without_logging_it() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let keys = Keys::default();
        let mut dom = VirtualDom::new(KeyInput);
        dom.provide_root_context(keys.clone());
        let initial = dom.rebuild_to_vec();
        assert!(dioxus_ssr::render(&dom).contains("type=\"password\""));
        let input_label = translate("onboarding-api-key");
        let input = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &input_label => Some(*id),
                _ => None,
            })
            .expect("API keys must be entered through a password input");
        let label = translate("onboarding-save-key");
        let save = initial
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
            .unwrap();
        dom.runtime().handle_event(
            "input",
            Event::new(
                EventData::Form(SerializedFormData {
                    value: "private-test-key".into(),
                    values: Vec::new(),
                    valid: true,
                })
                .into_any(),
                true,
            ),
            input,
        );
        dom.render_immediate_to_vec();
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            save,
        );
        dom.render_immediate_to_vec();
        let saved = keys.0.borrow();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].expose(), "private-test-key");
        assert!(!format!("{:?}", saved[0]).contains("private-test-key"));
        assert!(!dioxus_ssr::render(&dom).contains("private-test-key"));
    }

    #[derive(Clone, Default)]
    struct Interruptions(Rc<RefCell<Vec<Key>>>);

    #[component]
    fn InterruptibleKeyInput() -> Element {
        let interruptions = use_context::<Interruptions>();
        rsx! {
            div { onkeydown: move |event| interruptions.0.borrow_mut().push(event.key()),
                KeyInput {}
            }
        }
    }

    #[test]
    fn interruption_keys_bubble_from_password_input_without_submitting_a_key() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let keys = Keys::default();
        let interruptions = Interruptions::default();
        let mut dom = VirtualDom::new(InterruptibleKeyInput);
        dom.provide_root_context(keys.clone());
        dom.provide_root_context(interruptions.clone());
        let initial = dom.rebuild_to_vec();
        let label = translate("onboarding-api-key");
        let input = initial
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
            .expect("password input must render");
        for (key, code, modifiers) in [
            (Key::Enter, Code::Enter, Modifiers::empty()),
            (Key::Escape, Code::Escape, Modifiers::empty()),
            (Key::Character("c".into()), Code::KeyC, Modifiers::CONTROL),
        ] {
            let keyboard =
                SerializedKeyboardData::new(key, code, Location::Standard, false, modifiers, false);
            dom.runtime().handle_event(
                "keydown",
                Event::new(EventData::Keyboard(keyboard).into_any(), true),
                input,
            );
            dom.render_immediate_to_vec();
        }
        assert_eq!(
            *interruptions.0.borrow(),
            [Key::Escape, Key::Character("c".into())]
        );
        assert!(keys.0.borrow().is_empty());
    }
}
