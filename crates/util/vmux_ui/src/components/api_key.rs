use crate::components::composer::{PromptComposer, PromptComposerMode};
use crate::i18n::translate;
use dioxus::prelude::*;

#[component]
pub fn ApiKeyEntry(
    pending: bool,
    failed: bool,
    #[props(default = "var(--primary)".to_string())] accent_color: String,
    #[props(default)] on_stop: Option<EventHandler<()>>,
    #[props(default)] footer: Option<Element>,
    on_submit: EventHandler<vmux_api::Secret>,
) -> Element {
    let mut key = use_signal(String::new);
    let stoppable = pending && on_stop.is_some();
    let keyboard_submit = on_submit;
    let button_submit = on_submit;
    rsx! {
        PromptComposer {
            value: String::new(),
            accent_color,
            accent_gradient: "from-indigo-500 to-violet-600".to_string(),
            placeholder: translate("onboarding-api-key"),
            footer: Some(footer.unwrap_or_else(|| rsx! {})),
            show_add_button: false,
            disabled: pending && !stoppable,
            mode: if stoppable { PromptComposerMode::Stop } else { PromptComposerMode::Send },
            action_title: translate(if stoppable { "common-stop" } else { "command-send" }),
            action_enabled: stoppable || !key.peek().trim().is_empty(),
            input: Some(rsx! {
                input {
                    r#type: "password",
                    autocomplete: "off",
                    aria_label: translate("onboarding-api-key"),
                    placeholder: translate("onboarding-api-key"),
                    class: "min-h-11 w-full bg-transparent px-1.5 py-2.5 text-base outline-none placeholder:text-muted-foreground/50 disabled:cursor-not-allowed sm:min-h-10 sm:py-2 sm:text-[15px]",
                    value: key(),
                    disabled: pending,
                    oninput: move |event| { if !pending { key.set(event.value()); } },
                    onpaste: move |event| event.stop_propagation(),
                    onkeydown: move |event| {
                        if event.key() == Key::Escape || event.modifiers().ctrl() && event.key() == Key::Character("c".into()) { return; }
                        event.stop_propagation();
                        if event.key() == Key::Enter {
                            event.prevent_default();
                            if !pending && !key.peek().trim().is_empty() {
                                keyboard_submit.call(vmux_api::Secret::new(std::mem::take(&mut *key.write())));
                            }
                        }
                    },
                }
            }),
            on_input: move |_| {},
            on_keydown: move |_| {},
            on_paste: move |_| {},
            on_attach: move |_| {},
            on_remove_attachment: move |_| {},
            on_action: move |_| {
                if stoppable {
                    if let Some(stop) = on_stop { stop.call(()); }
                } else if !pending && !key.peek().trim().is_empty() {
                    button_submit.call(vmux_api::Secret::new(std::mem::take(&mut *key.write())));
                }
            },
        }
        if failed { p { role: "status", class: "px-2 text-xs text-destructive", {translate("onboarding-key-failed")} } }
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
        let label = translate("command-send");
        let save = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "title",
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
