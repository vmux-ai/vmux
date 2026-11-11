use dioxus::prelude::*;
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::composer::PromptComposer;
use vmux_ui::components::manager::{
    ManagerCard, ManagerCollection, ManagerIconButton, ManagerRow, ManagerSearch, ManagerSearchRow,
    ManagerSection, ManagerView, ManagerViewToggle,
};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

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
                AgentGallery { state }
            }
            super::surface::Dock {
                PromptComposer {
                    value: String::new(),
                    placeholder: translate("onboarding-composer-wait"),
                    accent_color: Identity::COLOR.to_string(),
                    accent_gradient: "from-indigo-500 to-violet-600".to_string(),
                    footer: Some(rsx! {}),
                    disabled: true,
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

#[component]
fn AgentGallery(state: OnboardingUiState) -> Element {
    let mut query = use_signal(String::new);
    let mut view = use_signal(|| ManagerView::Grid);
    let gallery = Gallery::of(&state, &query());
    let loading = state.agents.is_empty();
    rsx! {
        div { class: "flex flex-col gap-3",
            ManagerSearchRow {
                ManagerSearch {
                    value: query(),
                    placeholder: translate("onboarding-search-agents"),
                    disabled: state.starting || loading,
                    onsearch: move |value| query.set(value),
                    onkeydown: None,
                }
                ManagerViewToggle {
                    view: view(),
                    list_label: translate("tools-view-list"),
                    grid_label: translate("tools-view-grid"),
                    onchange: move |value| view.set(value),
                }
            }
            if loading {
                ManagerCollection { view: view(),
                    for index in 0..6 {
                        Skeleton { key: "{index}", class: if view() == ManagerView::Grid { "h-28 w-full rounded-xl" } else { "h-16 w-full rounded-lg" } }
                    }
                }
            } else {
                if !gallery.featured.is_empty() {
                    AgentSection {
                        title: translate("onboarding-featured-agents"),
                        agents: gallery.featured,
                        current: state.agent_id.clone(),
                        pending: state.starting,
                        view: view(),
                    }
                }
                if !gallery.other.is_empty() {
                    AgentSection {
                        title: translate(if query().trim().is_empty() && gallery.has_featured { "onboarding-more-agents" } else { "onboarding-all-agents" }),
                        agents: gallery.other,
                        current: state.agent_id,
                        pending: state.starting,
                        view: view(),
                    }
                } else if gallery.empty {
                    p { role: "status", class: "py-4 text-sm text-muted-foreground", {translate("common-no-results")} }
                }
            }
        }
    }
}

#[component]
fn AgentSection(
    title: String,
    agents: Vec<AgentOption>,
    current: String,
    pending: bool,
    view: ManagerView,
) -> Element {
    rsx! {
        ManagerSection { label: title, count: agents.len(),
            ManagerCollection { view,
                for agent in agents {
                    AgentEntry { key: "{agent.id}", selected: pending && agent.id == current, agent, pending, view }
                }
            }
        }
    }
}

#[component]
fn AgentEntry(agent: AgentOption, selected: bool, pending: bool, view: ManagerView) -> Element {
    let id = agent.id.clone();
    let label = translate_with(
        if agent.installed {
            "onboarding-connect-agent"
        } else {
            "onboarding-install-agent"
        },
        &[("agent", TranslationValue::String(&agent.name))],
    );
    let icon = rsx! {
        AgentThumbnail {
            src: (!agent.icon.is_empty()).then_some(agent.icon),
            seed: agent.id,
            background: String::new(),
            alt: agent.name.clone(),
            class: (if view == ManagerView::Grid { "size-10 text-[10px]" } else { "size-7 text-[8px]" }).to_string(),
        }
    };
    let operation_id = id.clone();
    let actions = rsx! {
        ManagerIconButton {
            label: label.clone(),
            disabled: pending,
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                if !pending { let _ = send(&OnboardingConnect { agent_id: operation_id.clone() }); }
            },
            LineIconView { icon: if agent.installed { LineIcon::MessagesRound } else { LineIcon::Plus }, class: "size-3.5" }
        }
    };
    let meta = rsx! {
        span { class: "text-[10px] text-muted-foreground/70", {translate(if agent.installed { "common-installed" } else { "tools-status-available" })} }
        if agent.environment_key {
            span { class: "text-[10px] text-muted-foreground/60", {translate("onboarding-key-available")} }
        }
    };
    if view == ManagerView::List {
        return rsx! {
            ManagerRow {
                selected,
                disabled: pending,
                aria_label: Some(label),
                icon_surface: false,
                icon,
                title: agent.name,
                subtitle: String::new(),
                actions,
                meta,
                onselect: move |_| { let _ = send(&OnboardingConnect { agent_id: id.clone() }); },
            }
        };
    }
    rsx! {
        ManagerCard {
            selected,
            disabled: pending,
            aria_label: Some(label),
            icon_surface: false,
            icon,
            title: agent.name,
            subtitle: String::new(),
            actions,
            meta,
            onselect: move |_| { let _ = send(&OnboardingConnect { agent_id: id.clone() }); },
        }
    }
}

struct Gallery {
    featured: Vec<AgentOption>,
    other: Vec<AgentOption>,
    has_featured: bool,
    empty: bool,
}

impl Gallery {
    fn of(state: &OnboardingUiState, query: &str) -> Self {
        let needle = query.trim().to_lowercase();
        let mut featured = Vec::new();
        let mut other = Vec::new();
        for agent in &state.agents {
            if !agent.name.to_lowercase().contains(&needle)
                && !agent.id.to_lowercase().contains(&needle)
            {
                continue;
            }
            if needle.is_empty()
                && (agent.id == state.agent_id
                    || agent.installed
                    || agent.environment_key
                    || Self::rank(&agent.id).is_some())
            {
                featured.push(agent.clone());
            } else {
                other.push(agent.clone());
            }
        }
        featured.sort_by(|left, right| {
            (
                right.id == state.agent_id,
                right.installed,
                right.environment_key,
            )
                .cmp(&(
                    left.id == state.agent_id,
                    left.installed,
                    left.environment_key,
                ))
                .then_with(|| {
                    Self::rank(&left.id)
                        .unwrap_or(usize::MAX)
                        .cmp(&Self::rank(&right.id).unwrap_or(usize::MAX))
                })
                .then_with(|| left.name.cmp(&right.name))
        });
        other.sort_by(|left, right| left.name.cmp(&right.name));
        let has_featured = !featured.is_empty();
        let empty = featured.is_empty() && other.is_empty();
        Self {
            featured,
            other,
            has_featured,
            empty,
        }
    }

    fn rank(id: &str) -> Option<usize> {
        match id {
            "claude" | "claude-acp" => Some(0),
            "codex" | "codex-acp" => Some(1),
            "gemini" | "gemini-cli" => Some(2),
            "mistral-vibe" | "vibe" | "vibe-acp" => Some(3),
            "opencode" | "opencode-acp" => Some(4),
            "goose" | "goose-acp" => Some(5),
            _ => None,
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
            self.elements(name, label)
                .into_iter()
                .next()
                .expect("required setup control must be rendered")
        }

        fn elements(&self, name: &str, label: &str) -> Vec<dioxus::core::ElementId> {
            self.initial
                .edits
                .iter()
                .filter_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: attribute,
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if *attribute == name && value == label => Some(*id),
                    _ => None,
                })
                .collect()
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
            self.initial = self.dom.render_immediate_to_vec();
        }
    }

    #[test]
    fn setup_asks_only_for_an_agent_and_installation_starts_from_its_choice() {
        let mut fixture = Fixture::new(false);
        let html = dioxus_ssr::render(&fixture.dom);
        let intro = translate("onboarding-setup-intro");
        assert!(
            fixture.initial.edits.iter().any(
                |edit| matches!(edit, Mutation::CreateTextNode { value, .. } if value == &intro)
            )
        );
        assert!(!html.contains(&translate("onboarding-title")));
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
    fn grid_cards_support_keyboard_activation_and_the_composer_is_disabled() {
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
        let label = translate_with(
            "onboarding-connect-agent",
            &[("agent", TranslationValue::String("Agent B"))],
        );
        let card = fixture.element("aria-label", &label);
        fixture.press(card, Key::Enter, Code::Enter);
        assert_eq!(fixture.selected(), "b");
    }

    #[test]
    fn pending_agent_selection_refuses_mouse_and_keyboard_activation() {
        let mut fixture = Fixture::new(true);
        let label = translate_with(
            "onboarding-install-agent",
            &[("agent", TranslationValue::String("Agent A"))],
        );
        let card = fixture.element("aria-label", &label);
        fixture.dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            card,
        );
        fixture.dom.render_immediate_to_vec();
        fixture.press(card, Key::Enter, Code::Enter);
        fixture.press(card, Key::Character(" ".into()), Code::Space);
        assert!(fixture.host.0.borrow().is_empty());
    }

    #[test]
    fn featured_partition_and_search_keep_the_complete_catalog_without_duplicates() {
        let state = OnboardingUiState {
            agents: vec![
                AgentOption {
                    id: "uncommon".into(),
                    name: "Uncommon".into(),
                    ..Default::default()
                },
                AgentOption {
                    id: "codex-acp".into(),
                    name: "Codex".into(),
                    ..Default::default()
                },
                AgentOption {
                    id: "custom".into(),
                    name: "Custom".into(),
                    installed: true,
                    ..Default::default()
                },
                AgentOption {
                    id: "claude-acp".into(),
                    name: "Claude".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let gallery = Gallery::of(&state, "");
        assert_eq!(
            gallery
                .featured
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            ["custom", "claude-acp", "codex-acp"]
        );
        assert_eq!(
            gallery
                .other
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            ["uncommon"]
        );
        let filtered = Gallery::of(&state, "Co");
        assert!(filtered.featured.is_empty());
        assert_eq!(
            filtered
                .other
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            ["codex-acp", "uncommon"]
        );
        assert!(Gallery::of(&state, "missing").empty);
    }

    #[test]
    fn shared_collection_controls_select_once_in_grid_and_list_views() {
        for list in [false, true] {
            for pending in [false, true] {
                let mut fixture = Fixture::new(pending);
                if list {
                    let toggle = fixture.element("aria-label", &translate("tools-view-list"));
                    fixture.click(toggle);
                    let html = dioxus_ssr::render(&fixture.dom);
                    let label = format!("aria-label=\"{}\"", translate("tools-view-list"));
                    let button = html
                        .split(&label)
                        .nth(1)
                        .unwrap()
                        .split('>')
                        .next()
                        .unwrap();
                    assert!(
                        button.replace('"', "").contains("aria-pressed=true"),
                        "{button}"
                    );
                }
                let label = translate_with(
                    "onboarding-connect-agent",
                    &[("agent", TranslationValue::String("Agent B"))],
                );
                let controls = fixture.elements("aria-label", &label);
                let button = *controls.last().expect("entry operation must be rendered");
                fixture.press(button, Key::Enter, Code::Enter);
                fixture.press(button, Key::Character(" ".into()), Code::Space);
                assert!(fixture.host.0.borrow().is_empty());
                fixture.click(button);
                if pending {
                    assert!(fixture.host.0.borrow().is_empty());
                } else {
                    assert_eq!(fixture.selected(), "b");
                }
            }
        }
    }

    #[test]
    fn shared_search_filters_entries_without_starting_setup() {
        let mut fixture = Fixture::new(false);
        let search = fixture.element("aria-label", &translate("onboarding-search-agents"));
        fixture.dom.runtime().handle_event(
            "input",
            Event::new(
                EventData::Form(SerializedFormData {
                    value: "agent b".into(),
                    values: Vec::new(),
                    valid: true,
                })
                .into_any(),
                true,
            ),
            search,
        );
        fixture.dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&fixture.dom);
        assert!(html.contains("Agent B"));
        assert!(!html.contains("Agent A"));
        assert!(fixture.host.0.borrow().is_empty());
    }
}
