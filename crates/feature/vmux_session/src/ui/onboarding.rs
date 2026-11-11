use dioxus::prelude::*;
use vmux_ui::components::composer::PromptComposer;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

use super::approval::{ChoiceKey, ChoiceOptions};
use super::setup::{Identity, Turn};
use crate::onboarding::{AgentOption, OnboardingConnect, OnboardingUiState};

#[component]
pub(super) fn Onboarding() -> Element {
    vmux_ui::hooks::use_theme();
    let state = UiStateBinding::<OnboardingUiState>::use_context()
        .state
        .read()
        .clone();
    rsx! { Conversation { state } }
}

#[component]
fn Conversation(state: OnboardingUiState) -> Element {
    rsx! {
        super::surface::Page { accent: Identity::COLOR.to_string(),
            super::surface::Transcript { id: "setup-scroll".to_string(),
                Turn {
                    p { class: "text-sm leading-relaxed", {translate("onboarding-setup-intro")} }
                }
                if state.failed {
                    Turn { p { class: "text-sm leading-relaxed", {translate("onboarding-setup-failed")} } }
                }
            }
            AgentComposer { state }
        }
    }
}

#[component]
fn AgentComposer(state: OnboardingUiState) -> Element {
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| {
        state
            .agents
            .iter()
            .position(|agent| agent.id == state.agent_id)
            .unwrap_or_default()
    });
    let mut choices = Vec::new();
    let needle = query().trim().to_lowercase();
    for agent in &state.agents {
        if agent.name.to_lowercase().contains(&needle) || agent.id.to_lowercase().contains(&needle)
        {
            choices.push(agent.clone());
        }
    }
    let mut labels = Vec::new();
    for agent in &choices {
        let label = translate_with(
            if agent.installed {
                "onboarding-connect-agent"
            } else {
                "onboarding-install-agent"
            },
            &[("agent", TranslationValue::String(&agent.name))],
        );
        if agent.environment_key {
            labels.push(format!(
                "{label} · {}",
                translate("onboarding-key-available")
            ));
        } else {
            labels.push(label);
        }
    }
    let loading = state.agents.is_empty();
    let cursor = selected().min(choices.len().saturating_sub(1));
    let picker = Picker(choices);
    let clicked = picker.clone();
    let typed = picker.clone();
    let submitted = picker.clone();
    rsx! {
        super::surface::Dock {
            if loading {
                Skeleton { class: "h-10 w-full rounded-xl" }
                Skeleton { class: "h-10 w-full rounded-xl" }
            } else {
                ChoiceOptions {
                    options: labels,
                    selected: cursor,
                    disabled: state.starting,
                    on_select: move |index| { if !state.starting { clicked.choose(index); } },
                    on_hover: move |index| selected.set(index),
                    on_keydown: move |event| { if !state.starting { typed.key(event, selected, query().is_empty()); } },
                }
            }
            PromptComposer {
                value: query(),
                placeholder: translate("composer-choose-agent"),
                accent_color: Identity::COLOR.to_string(),
                accent_gradient: "from-indigo-500 to-violet-600".to_string(),
                footer: Some(rsx! {}),
                disabled: state.starting || loading,
                show_add_button: false,
                action_title: translate("onboarding-setup"),
                action_enabled: !picker.0.is_empty(),
                on_input: move |value| { query.set(value); selected.set(0); },
                on_keydown: move |event| picker.key(event, selected, query().is_empty()),
                on_action: move |_| submitted.choose(cursor),
                on_paste: move |_| {},
                on_attach: move |_| {},
                on_remove_attachment: move |_| {},
            }
        }
    }
}

#[derive(Clone)]
struct Picker(Vec<AgentOption>);

impl Picker {
    fn choose(&self, index: usize) {
        if let Some(agent) = self.0.get(index) {
            let _ = send(&OnboardingConnect {
                agent_id: agent.id.clone(),
            });
        }
    }

    fn key(&self, event: KeyboardEvent, mut selected: Signal<usize>, numbers: bool) {
        let Some(key) = ChoiceKey::of(&event, numbers) else {
            return;
        };
        let current = selected().min(self.0.len().saturating_sub(1));
        match key {
            ChoiceKey::Next => selected.set((current + 1).min(self.0.len().saturating_sub(1))),
            ChoiceKey::Previous => selected.set(current.saturating_sub(1)),
            ChoiceKey::Submit => self.choose(current),
            ChoiceKey::Number(index) if index < self.0.len() => self.choose(index),
            ChoiceKey::Number(_) => return,
        }
        event.prevent_default();
        event.stop_propagation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedHtmlEventConverter, SerializedKeyboardData,
        SerializedMouseData,
    };
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, PageHost};

    #[derive(Default)]
    struct Host(RefCell<Vec<(String, Vec<u8>)>>);

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            self.0.borrow_mut().push((id.into(), bytes.into()));
            Ok(())
        }
        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    struct Fixture {
        dom: VirtualDom,
        host: Rc<Host>,
        initial: dioxus::core::Mutations,
    }

    impl Fixture {
        fn new() -> Self {
            dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let host = Rc::new(Host::default());
            let state = OnboardingUiState {
                agents: vec![
                    AgentOption {
                        id: "a".into(),
                        name: "Agent A".into(),
                        ..Default::default()
                    },
                    AgentOption {
                        id: "b".into(),
                        name: "Agent B".into(),
                        installed: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };
            let mut dom = VirtualDom::new_with_props(Conversation, ConversationProps { state });
            dom.provide_root_context(HostBinding(host.clone()));
            let initial = dom.rebuild_to_vec();
            Self { dom, host, initial }
        }

        fn element(&self, name: &str, label: &str) -> dioxus::core::ElementId {
            self.initial
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: attribute,
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if *attribute == name && value == label => Some(*id),
                    _ => None,
                })
                .expect("required setup control must be rendered")
        }

        fn selected(&self) -> String {
            let events = self.host.0.borrow();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].0, OnboardingConnect::id());
            rkyv::from_bytes::<OnboardingConnect, rkyv::rancor::Error>(&events[0].1)
                .unwrap()
                .agent_id
        }
    }

    #[test]
    fn setup_asks_only_for_an_agent_and_installation_starts_from_its_choice() {
        let mut fixture = Fixture::new();
        let html = dioxus_ssr::render(&fixture.dom);
        let intro = translate("onboarding-setup-intro");
        assert!(
            fixture.initial.edits.iter().any(
                |edit| matches!(edit, Mutation::CreateTextNode { value, .. } if value == &intro)
            )
        );
        assert!(!html.contains("<header"));
        for prompt in vmux_ui::components::starter_prompts::StarterPrompt::all() {
            assert!(!html.contains(&prompt.text()));
        }
        assert!(!html.contains(&translate("onboarding-workspace-question")));
        let label = translate_with(
            "onboarding-install-agent",
            &[("agent", TranslationValue::String("Agent A"))],
        );
        let choice = fixture.element("aria-label", &label);
        fixture.dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            choice,
        );
        fixture.dom.render_immediate_to_vec();
        assert_eq!(fixture.selected(), "a");
    }

    #[test]
    fn number_and_arrow_keys_connect_the_selected_agent_without_a_task() {
        for navigation in [false, true] {
            let mut fixture = Fixture::new();
            let input = fixture.element("id", vmux_ui::components::composer::PROMPT_INPUT_ID);
            let keys = if navigation {
                vec![(Key::ArrowDown, Code::ArrowDown), (Key::Enter, Code::Enter)]
            } else {
                vec![(Key::Character("2".into()), Code::Digit2)]
            };
            for (key, code) in keys {
                let data = SerializedKeyboardData::new(
                    key,
                    code,
                    Location::Standard,
                    false,
                    Modifiers::empty(),
                    false,
                );
                fixture.dom.runtime().handle_event(
                    "keydown",
                    Event::new(EventData::Keyboard(data).into_any(), true),
                    input,
                );
                fixture.dom.render_immediate_to_vec();
            }
            assert_eq!(fixture.selected(), "b");
        }
    }
}
