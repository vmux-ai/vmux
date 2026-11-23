use crate::i18n::translate;
use dioxus::prelude::*;

#[component]
pub fn StarterPrompts(on_choose: EventHandler<String>) -> Element {
    rsx! {
        div { class: "flex flex-wrap gap-2",
            for prompt in StarterPrompt::all() {
                button {
                    r#type: "button",
                    aria_label: prompt.text(),
                    class: "rounded-xl bg-muted/40 px-4 py-3 text-left text-sm transition-colors hover:bg-muted/70 focus-visible:outline-2 focus-visible:outline-primary",
                    onclick: move |_| on_choose.call(prompt.text()),
                    {prompt.text()}
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum StarterPrompt {
    Explain,
    Fix,
    Build,
}

impl StarterPrompt {
    pub fn all() -> [Self; 3] {
        [Self::Explain, Self::Fix, Self::Build]
    }

    pub fn text(self) -> String {
        translate(match self {
            Self::Explain => "onboarding-explain",
            Self::Fix => "onboarding-fix",
            Self::Build => "onboarding-build",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, Default)]
    struct Choices(Rc<RefCell<Vec<String>>>);

    #[component]
    fn PromptChoices() -> Element {
        let choices = use_context::<Choices>();
        rsx! { StarterPrompts { on_choose: move |text| choices.0.borrow_mut().push(text) } }
    }

    #[test]
    fn every_starter_prompt_is_selectable_before_any_agent_is_configured() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let choices = Choices::default();
        let mut dom = VirtualDom::new(PromptChoices);
        dom.provide_root_context(choices.clone());
        let initial = dom.rebuild_to_vec();
        let expected = ["onboarding-explain", "onboarding-fix", "onboarding-build"].map(translate);
        for label in &expected {
            let button = initial
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
                .expect("every starter prompt must expose a selectable control");
            dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                button,
            );
            dom.render_immediate_to_vec();
        }
        assert_eq!(*choices.0.borrow(), expected);
    }
}
