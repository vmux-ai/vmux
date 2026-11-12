use dioxus::prelude::*;
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::composer::{ComposerFrame, PromptComposer};
use vmux_ui::components::manager::{ManagerSearch, ManagerSearchRow};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::translate;

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
            super::surface::Transcript { id: "setup-scroll".to_string(), at_bottom: false,
                Turn {
                    p { class: "text-sm leading-relaxed", {translate("onboarding-setup-intro")} }
                    if state.failed {
                        p { class: "text-sm leading-relaxed", {translate("onboarding-setup-failed")} }
                    }
                }
            }
            super::surface::Dock {
                ComposerFrame {
                    muted: true,
                    notice: Some(rsx! { div { "data-slot": "composer-notice", class: "px-1 py-1", AgentSelector { state } } }),
                PromptComposer {
                    value: String::new(),
                    placeholder: translate("onboarding-composer-wait"),
                    accent_color: Identity::COLOR.to_string(),
                    accent_gradient: "from-indigo-500 to-violet-600".to_string(),
                    footer: Some(rsx! {}),
                    input_disabled: true,
                    show_add_button: false,
                    show_send_button: false,
                    action_title: translate("onboarding-setup"),
                    action_enabled: false,
                    on_input: move |_| {},
                    on_keydown: move |_| {},
                    on_action: move |_| {},
                    on_paste: move |_| {},
                    on_attach: move |_| {},
                    on_remove_attachment: move |_| {},
                }
                }
            }
        }
    }
}

#[component]
fn AgentSelector(state: OnboardingUiState) -> Element {
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0_usize);
    let candidates = Candidates::of(&state, &query());
    let loading = state.agents.is_empty();
    let pending = state.starting;
    let cursor = selected().min(candidates.0.len().saturating_sub(1));
    let clicked = candidates.clone();
    let keyed = candidates.clone();
    let searched = candidates.clone();
    let mut labels = Vec::new();
    let mut icons = Vec::new();
    let mut details = Vec::new();
    for agent in &candidates.0 {
        labels.push(agent.name.clone());
        icons.push(rsx! {
            AgentThumbnail {
                src: (!agent.icon.is_empty()).then_some(agent.icon.clone()),
                seed: agent.id.clone(),
                background: String::new(),
                alt: agent.name.clone(),
                class: "size-5 text-[7px]".to_string(),
            }
        });
        details.push(rsx! {
            if agent.installed {
                span { class: "text-[10px] opacity-60", {translate("common-installed")} }
            }
        });
    }
    rsx! {
        div { class: "flex w-full flex-col gap-2",
            if loading {
                for index in 0..3 {
                    Skeleton { key: "{index}", class: "h-8 w-full rounded-lg" }
                }
            } else if candidates.0.is_empty() {
                p { role: "status", class: "px-2 py-2 text-sm text-muted-foreground", {translate("common-no-results")} }
            } else {
                ChoiceOptions {
                    options: labels,
                    leading: icons,
                    trailing: details,
                    compact: true,
                    framed: true,
                    muted: true,
                    viewport_class: "max-h-[min(10rem,35vh)]".to_string(),
                    show_numbers: false,
                    show_help: false,
                    selected: cursor,
                    disabled: pending,
                    on_select: move |index| { if !pending { clicked.choose(index); } },
                    on_hover: move |index| { if !pending { selected.set(index); } },
                    on_keydown: move |event| { if !pending { keyed.key(event, selected, true); } },
                }
            }
            ManagerSearchRow {
                ManagerSearch {
                    value: query(),
                    placeholder: translate("onboarding-search-agents"),
                    disabled: pending || loading,
                    inherit_color: true,
                    onsearch: move |value| { query.set(value); selected.set(0); },
                    onkeydown: Some(EventHandler::new(move |event| {
                        if !pending { searched.key(event, selected, false); }
                    })),
                }
            }
        }
    }
}

#[derive(Clone)]
struct Candidates(Vec<AgentOption>);

impl Candidates {
    fn of(state: &OnboardingUiState, query: &str) -> Self {
        let needle = query.trim().to_lowercase();
        let mut agents = Vec::new();
        if !needle.is_empty() {
            for agent in &state.agents {
                if agent.name.to_lowercase().contains(&needle)
                    || agent.id.to_lowercase().contains(&needle)
                {
                    agents.push(agent.clone());
                }
            }
            agents.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then_with(|| left.id.cmp(&right.id))
            });
            return Self(agents);
        }
        for family in 0..6 {
            let mut candidate: Option<&AgentOption> = None;
            for agent in &state.agents {
                if Self::family(&agent.id) != Some(family) {
                    continue;
                }
                if let Some(current) = candidate
                    && (
                        agent.installed,
                        agent.environment_key,
                        agent.id == state.agent_id,
                    ) <= (
                        current.installed,
                        current.environment_key,
                        current.id == state.agent_id,
                    )
                {
                    continue;
                }
                candidate = Some(agent);
            }
            if let Some(agent) = candidate {
                agents.push(agent.clone());
            }
            if agents.len() == 3 {
                break;
            }
        }
        if agents.is_empty() {
            agents.clone_from(&state.agents);
            agents.sort_by(|left, right| {
                right
                    .installed
                    .cmp(&left.installed)
                    .then_with(|| left.name.cmp(&right.name))
            });
            agents.truncate(3);
        }
        let mut remaining = Vec::new();
        for agent in &state.agents {
            if !agents.iter().any(|suggestion| suggestion.id == agent.id) {
                remaining.push(agent.clone());
            }
        }
        remaining.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.id.cmp(&right.id))
        });
        agents.extend(remaining);
        Self(agents)
    }

    fn family(id: &str) -> Option<usize> {
        match id {
            "claude" | "claude-acp" => Some(0),
            "codex" | "codex-acp" => Some(1),
            "mistral-vibe" | "vibe" | "vibe-acp" => Some(2),
            "gemini" | "gemini-cli" => Some(3),
            "opencode" | "opencode-acp" => Some(4),
            "goose" | "goose-acp" => Some(5),
            _ => None,
        }
    }

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
        if self.0.is_empty() {
            return;
        }
        let current = selected().min(self.0.len() - 1);
        match key {
            ChoiceKey::Next => selected.set((current + 1).min(self.0.len() - 1)),
            ChoiceKey::Previous => selected.set(current.saturating_sub(1)),
            ChoiceKey::Submit => self.choose(current),
            ChoiceKey::Number(index) => self.choose(index),
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
        Code, EventData, Location, Modifiers, SerializedFormData, SerializedHtmlEventConverter,
        SerializedKeyboardData, SerializedMouseData,
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
        fn new(starting: bool) -> Self {
            dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let host = Rc::new(Host::default());
            let state = OnboardingUiState {
                starting,
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

        fn press(&mut self, element: dioxus::core::ElementId, key: Key, code: Code) {
            let keyboard = SerializedKeyboardData::new(
                key,
                code,
                Location::Standard,
                false,
                Modifiers::empty(),
                false,
            );
            self.dom.runtime().handle_event(
                "keydown",
                Event::new(EventData::Keyboard(keyboard).into_any(), true),
                element,
            );
            self.dom.render_immediate_to_vec();
        }

        fn click(&mut self, element: dioxus::core::ElementId) {
            self.dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                element,
            );
            self.dom.render_immediate_to_vec();
        }

        fn search(&mut self, element: dioxus::core::ElementId, value: &str) {
            self.dom.runtime().handle_event(
                "input",
                Event::new(
                    EventData::Form(SerializedFormData {
                        value: value.into(),
                        values: Vec::new(),
                        valid: true,
                    })
                    .into_any(),
                    true,
                ),
                element,
            );
            self.dom.render_immediate_to_vec();
        }
    }

    #[test]
    fn guide_offers_only_agent_choices_and_a_choice_starts_setup() {
        let mut fixture = Fixture::new(false);
        let html = dioxus_ssr::render(&fixture.dom);
        let intro = translate("onboarding-setup-intro");
        assert!(fixture.initial.edits.iter().any(|edit| {
            matches!(edit, Mutation::CreateTextNode { value, .. } if value == &intro)
        }));
        assert!(html.contains("role=\"listbox\""));
        for prompt in vmux_ui::components::starter_prompts::StarterPrompt::all() {
            assert!(!html.contains(&prompt.text()));
        }
        assert!(!html.contains(&translate("onboarding-workspace-question")));
        assert!(!html.contains(&translate("tools-view-list")));
        assert!(!html.contains(&translate("onboarding-more-agents")));
        let choice = fixture.element("aria-label", "Agent A");
        fixture.click(choice);
        assert_eq!(fixture.selected(), "a");
    }

    #[test]
    fn choice_keyboard_activation_works_while_the_normal_composer_stays_disabled() {
        let mut fixture = Fixture::new(false);
        let html = dioxus_ssr::render(&fixture.dom);
        let input = html
            .split("<textarea")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(input.contains("disabled"));
        let composer = fixture.element("id", vmux_ui::components::composer::PROMPT_INPUT_ID);
        fixture.press(composer, Key::Enter, Code::Enter);
        assert!(fixture.host.0.borrow().is_empty());
        let choice = fixture.element("aria-label", "Agent B");
        fixture.press(choice, Key::Enter, Code::Enter);
        assert_eq!(fixture.selected(), "b");
    }

    #[test]
    fn pending_setup_refuses_choice_clicks_keys_and_search() {
        let mut fixture = Fixture::new(true);
        let choice = fixture.element("aria-label", "Agent A");
        let search = fixture.element("aria-label", &translate("onboarding-search-agents"));
        fixture.click(choice);
        fixture.press(choice, Key::Enter, Code::Enter);
        fixture.press(choice, Key::Character(" ".into()), Code::Space);
        fixture.search(search, "agent b");
        fixture.press(search, Key::Enter, Code::Enter);
        assert!(fixture.host.0.borrow().is_empty());
        assert!(dioxus_ssr::render(&fixture.dom).contains("Agent A"));
    }

    #[test]
    fn common_families_lead_the_complete_catalog_without_dropping_entries() {
        let mut state = OnboardingUiState {
            agent_id: "bob".into(),
            ..Default::default()
        };
        for (id, name, installed) in [
            ("bob", "Bob", true),
            ("claude-acp", "Claude Agent", false),
            ("claude", "Claude Code", true),
            ("codex-acp", "Codex", false),
            ("codex", "Codex", true),
            ("gemini", "Gemini CLI", false),
            ("mistral-vibe", "Mistral Vibe", false),
        ] {
            state.agents.push(AgentOption {
                id: id.into(),
                name: name.into(),
                installed,
                ..Default::default()
            });
        }
        let suggestions = Candidates::of(&state, "");
        assert_eq!(
            suggestions
                .0
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            [
                "claude",
                "codex",
                "mistral-vibe",
                "bob",
                "claude-acp",
                "codex-acp",
                "gemini"
            ]
        );
        assert_eq!(
            Candidates::of(&state, "codex")
                .0
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            ["codex", "codex-acp"]
        );
        assert_eq!(Candidates::of(&state, "bob").0[0].id, "bob");
        assert_eq!(Candidates::of(&state, "vibe").0[0].id, "mistral-vibe");
        assert_eq!(Candidates::of(&state, " ").0.len(), state.agents.len());
    }

    #[test]
    fn custom_catalog_remains_complete_before_and_after_search() {
        let mut state = OnboardingUiState::default();
        for (id, name) in [
            ("a", "Agent A"),
            ("b", "Agent B"),
            ("c", "Agent C"),
            ("d", "Agent D"),
        ] {
            state.agents.push(AgentOption {
                id: id.into(),
                name: name.into(),
                ..Default::default()
            });
        }
        assert_eq!(Candidates::of(&state, "").0.len(), 4);
        assert_eq!(Candidates::of(&state, "agent").0.len(), 4);
        assert_eq!(Candidates::of(&state, "D").0[0].id, "d");
        assert!(Candidates::of(&state, "missing").0.is_empty());
    }

    #[test]
    fn search_updates_the_choices_and_enter_connects_the_result_once() {
        let mut fixture = Fixture::new(false);
        let search = fixture.element("aria-label", &translate("onboarding-search-agents"));
        fixture.search(search, "agent b");
        let html = dioxus_ssr::render(&fixture.dom);
        assert!(html.contains("Agent B"));
        assert!(!html.contains("Agent A"));
        assert!(fixture.host.0.borrow().is_empty());
        fixture.press(search, Key::Enter, Code::Enter);
        assert_eq!(fixture.selected(), "b");
    }
}
