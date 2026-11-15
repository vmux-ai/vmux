use dioxus::core::current_scope_id;
use dioxus::prelude::*;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::progress::LoadingIcon;
use crate::i18n::translate;

#[component]
pub fn ConversationPrompt(
    #[props(default)] question: Option<String>,
    #[props(default)] hint: String,
    #[props(default)] actions: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        section { class: "flex min-w-0 flex-col gap-2 text-inherit",
            if let Some(question) = question {
                p { class: "text-sm font-medium leading-relaxed", {question} }
            }
            if !hint.is_empty() {
                p { class: "text-xs leading-relaxed opacity-65", {hint} }
            }
            {children}
            if let Some(actions) = actions { {actions} }
        }
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct PromptOption {
    pub value: String,
    pub label: String,
    pub description: String,
    pub disabled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptSelectionMode {
    #[default]
    Single,
    Multiple,
}

impl PromptSelectionMode {
    fn input_type(self) -> &'static str {
        match self {
            Self::Single => "radio",
            Self::Multiple => "checkbox",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct PromptSelectionChange {
    pub value: String,
    pub mode: PromptSelectionMode,
}

impl PromptSelectionChange {
    pub fn apply(self, selected: &mut Vec<String>) {
        if self.mode == PromptSelectionMode::Single {
            selected.clear();
            selected.push(self.value);
            return;
        }
        if let Some(index) = selected.iter().position(|value| value == &self.value) {
            selected.remove(index);
        } else {
            selected.push(self.value);
        }
    }
}

#[component]
pub fn PromptSelection(
    label: String,
    options: Vec<PromptOption>,
    selected: Vec<String>,
    #[props(default)] mode: PromptSelectionMode,
    #[props(default)] disabled: bool,
    #[props(default)] leading: Vec<Element>,
    #[props(default)] trailing: Vec<Element>,
    #[props(default = "max-h-64".to_string())] viewport_class: String,
    on_change: EventHandler<PromptSelectionChange>,
) -> Element {
    let name = format!("prompt-selection-{}", current_scope_id().0);
    rsx! {
        fieldset {
            class: "min-w-0 border-0 p-0",
            role: if mode == PromptSelectionMode::Single { "radiogroup" } else { "group" },
            aria_label: label,
            div { class: "flex flex-col gap-0.5 overflow-y-auto overscroll-contain {viewport_class}",
                for (index, option) in options.into_iter().enumerate() {
                    {
                        let checked = selected.contains(&option.value);
                        let unavailable = disabled || option.disabled;
                        let value = option.value.clone();
                        rsx! {
                            label {
                                key: "prompt-{option.value}",
                                class: if unavailable {
                                    "flex min-h-9 shrink-0 cursor-not-allowed items-center gap-3 rounded-lg px-3 py-1.5 opacity-45"
                                } else if checked {
                                    "flex min-h-9 shrink-0 cursor-pointer items-center gap-3 rounded-lg bg-background/[0.06] px-3 py-1.5 transition-colors focus-within:outline focus-within:outline-2 focus-within:outline-primary motion-reduce:transition-none"
                                } else {
                                    "flex min-h-9 shrink-0 cursor-pointer items-center gap-3 rounded-lg px-3 py-1.5 transition-colors hover:bg-background/[0.04] focus-within:outline focus-within:outline-2 focus-within:outline-primary motion-reduce:transition-none"
                                },
                                input {
                                    r#type: mode.input_type(),
                                    name: name.clone(),
                                    value: option.value.clone(),
                                    aria_label: option.label.clone(),
                                    checked,
                                    disabled: unavailable,
                                    class: "size-4 shrink-0 cursor-inherit accent-[var(--primary-action-start)]",
                                    onchange: move |_| {
                                        if !unavailable {
                                    on_change.call(PromptSelectionChange { value: value.clone(), mode });
                                        }
                                    },
                                }
                                if let Some(icon) = leading.get(index) { {icon.clone()} }
                                span { class: "flex min-w-0 flex-1 flex-col gap-0.5",
                                    span { class: "text-sm font-medium", {option.label.clone()} }
                                    if !option.description.is_empty() {
                                        span { class: "text-xs leading-relaxed opacity-60", {option.description} }
                                    }
                                }
                                if let Some(detail) = trailing.get(index) { {detail.clone()} }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn PromptConfirmation(
    #[props(default = translate("common-confirm"))] confirm_label: String,
    on_confirm: EventHandler<()>,
    #[props(default = true)] enabled: bool,
    #[props(default)] pending: bool,
    #[props(default = Some(translate("common-cancel")))] cancel_label: Option<String>,
    #[props(default)] on_cancel: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: if on_cancel.is_some() { "flex flex-wrap items-center justify-between gap-2" } else { "flex flex-wrap items-center justify-end gap-2" },
            if let (Some(label), Some(on_cancel)) = (cancel_label, on_cancel) {
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    r#type: "button",
                    class: "min-h-8 [color:inherit] [&:hover]:bg-background/5",
                    aria_label: label.clone(),
                    onclick: move |_| on_cancel.call(()),
                    {label}
                }
            }
            Button {
                size: ButtonSize::Sm,
                r#type: "button",
                class: "min-h-8",
                aria_label: confirm_label.clone(),
                aria_busy: pending,
                disabled: !enabled || pending,
                onclick: move |_| {
                    if enabled && !pending {
                        on_confirm.call(());
                    }
                },
                if pending { LoadingIcon { class: "size-3.5".to_string() } }
                {confirm_label}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{
        EventData, SerializedFormData, SerializedHtmlEventConverter, SerializedMouseData,
    };
    use std::{cell::RefCell, rc::Rc};

    type Records = Rc<RefCell<Vec<(String, Vec<String>)>>>;

    #[component]
    fn Fixture(mode: PromptSelectionMode, pending: bool, records: Records) -> Element {
        let mut selected = use_signal(Vec::<String>::new);
        let changed = records.clone();
        let confirmed = records.clone();
        rsx! {
            ConversationPrompt {
                question: Some("Choose options".to_string()),
                actions: Some(rsx! { PromptConfirmation {
                    confirm_label: "Continue".to_string(),
                    enabled: !selected().is_empty(),
                    pending,
                    cancel_label: Some("Cancel".to_string()),
                    on_cancel: move |_| records.borrow_mut().push(("cancel".into(), selected())),
                    on_confirm: move |_| confirmed.borrow_mut().push(("confirm".into(), selected())),
                } }),
                PromptSelection {
                    label: "Choose options".to_string(),
                    mode,
                    disabled: pending,
                    selected: selected(),
                    options: vec![
                        PromptOption { value: "one".into(), label: "One".into(), ..Default::default() },
                        PromptOption { value: "two".into(), label: "Two".into(), ..Default::default() },
                        PromptOption { value: "unavailable".into(), label: "Unavailable".into(), disabled: true, ..Default::default() },
                    ],
                    on_change: move |change: PromptSelectionChange| {
                        change.apply(&mut selected.write());
                        changed.borrow_mut().push(("select".into(), selected()));
                    },
                }
            }
        }
    }

    struct Harness {
        dom: VirtualDom,
        initial: dioxus::core::Mutations,
        records: Records,
    }

    impl Harness {
        fn new(mode: PromptSelectionMode, pending: bool) -> Self {
            dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let records = Rc::new(RefCell::new(Vec::new()));
            let mut dom = VirtualDom::new_with_props(
                Fixture,
                FixtureProps {
                    mode,
                    pending,
                    records: records.clone(),
                },
            );
            let initial = dom.rebuild_to_vec();
            Self {
                dom,
                initial,
                records,
            }
        }

        fn click(&mut self, label: &str) {
            self.dispatch(
                label,
                "click",
                EventData::Mouse(SerializedMouseData::default()),
            );
            self.dom.render_immediate_to_vec();
        }

        fn choose(&mut self, label: &str) {
            self.queue_choice(label);
            self.dom.render_immediate_to_vec();
        }

        fn queue_choice(&mut self, label: &str) {
            self.dispatch(
                label,
                "change",
                EventData::Form(SerializedFormData {
                    value: "true".into(),
                    values: Vec::new(),
                    valid: true,
                }),
            );
        }

        fn dispatch(&mut self, label: &str, event: &str, data: EventData) {
            let id = self
                .initial
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == label => Some(*id),
                    _ => None,
                })
                .expect("prompt control");
            self.dom
                .runtime()
                .handle_event(event, Event::new(data.into_any(), true), id);
        }
    }

    #[test]
    fn single_choice_replaces_the_draft_without_confirming_it() {
        let mut harness = Harness::new(PromptSelectionMode::Single, false);
        let html = dioxus_ssr::render(&harness.dom);
        assert!(html.contains("role=\"radiogroup\""));
        assert_eq!(html.matches("type=\"radio\"").count(), 3);
        harness.click("Continue");
        harness.queue_choice("One");
        harness.queue_choice("Two");
        harness.dom.render_immediate_to_vec();
        harness.choose("Unavailable");
        assert_eq!(
            *harness.records.borrow(),
            [
                ("select".into(), vec!["one".into()]),
                ("select".into(), vec!["two".into()])
            ]
        );
        harness.click("Continue");
        assert_eq!(
            harness.records.borrow().last().unwrap(),
            &("confirm".into(), vec!["two".into()])
        );
    }

    #[test]
    fn multiple_choices_toggle_independently_and_cancel_does_not_confirm() {
        let mut harness = Harness::new(PromptSelectionMode::Multiple, false);
        assert_eq!(
            dioxus_ssr::render(&harness.dom)
                .matches("type=\"checkbox\"")
                .count(),
            3
        );
        harness.queue_choice("One");
        harness.queue_choice("Two");
        harness.dom.render_immediate_to_vec();
        harness.choose("One");
        harness.click("Cancel");
        assert_eq!(
            *harness.records.borrow(),
            [
                ("select".into(), vec!["one".into()]),
                ("select".into(), vec!["one".into(), "two".into()]),
                ("select".into(), vec!["two".into()]),
                ("cancel".into(), vec!["two".into()]),
            ]
        );
        let mut pending = Harness::new(PromptSelectionMode::Single, true);
        pending.choose("One");
        pending.click("Continue");
        assert!(pending.records.borrow().is_empty());
        pending.click("Cancel");
        assert_eq!(*pending.records.borrow(), [("cancel".into(), Vec::new())]);
    }
}
