use dioxus::prelude::*;
use vmux_ui::components::composer::PromptComposer;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::starter_prompts::StarterPrompt;
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::translate;

use super::approval::{ChoiceKey, ChoiceOptions};
use super::item::UserBubble;
use super::setup::{Identity, Turn};
use super::state::UserIdentity;
use crate::onboarding::{
    OnboardingBack, OnboardingContinue, OnboardingDraftChanged, OnboardingStart, OnboardingUiState,
    Step,
};

#[component]
pub(super) fn Onboarding() -> Element {
    vmux_ui::hooks::use_theme();
    let state = UiStateBinding::<OnboardingUiState>::use_context()
        .state
        .read()
        .clone();
    let user = UserIdentity::use_state();
    rsx! { Conversation { state, user: Some(user) } }
}

#[component]
fn Conversation(state: OnboardingUiState, #[props(default)] user: Option<UserIdentity>) -> Element {
    let reply = Reply(OnboardingDraftChanged {
        text: state.text.clone(),
        agent_id: state.agent_id.clone(),
        cwd: state.cwd.clone(),
    });
    let user_name = user
        .map(UserIdentity::name)
        .unwrap_or_else(|| translate("team-you"));
    let user_color = user
        .map(UserIdentity::color)
        .unwrap_or_else(|| "#3b82f6".to_string());
    let selected = state.agents.iter().find(|agent| agent.id == state.agent_id);
    let selected_name = selected.map(|agent| agent.name.clone()).unwrap_or_default();
    let step_key = format!("{:?}", state.step);
    rsx! {
        super::surface::Page { accent: Identity::COLOR.to_string(),
            super::surface::Transcript { id: "setup-scroll".to_string(),
                Turn {
                    p { class: "text-sm leading-relaxed", {translate("onboarding-title")} }
                    p { class: "text-sm leading-relaxed", {translate("onboarding-intro")} }
                }
                if state.step != Step::Prompt {
                    UserBubble { avatar_name: user_name.clone(), avatar_color: user_color.clone(), copy_text: state.text.clone(),
                        p { class: "whitespace-pre-wrap", "{state.text}" }
                    }
                    Turn {
                        p { class: "text-sm leading-relaxed", {translate("onboarding-agent-question")} }
                    }
                }
                if state.step == Step::Workspace {
                    UserBubble { avatar_name: user_name.clone(), avatar_color: user_color.clone(),
                        p { "{selected_name}" }
                    }
                    Turn {
                        p { class: "text-sm leading-relaxed", {translate("onboarding-workspace-question")} }
                        p { class: "text-sm leading-relaxed", {translate("onboarding-workspace-hint")} }
                    }
                    if !state.cwd.is_empty() {
                        UserBubble { avatar_name: user_name.clone(), avatar_color: user_color.clone(),
                            p { "{state.cwd}" }
                        }
                    }
                }
            }
            {rsx! { SetupComposer { key: "{step_key}", state, reply } }}
        }
    }
}

#[component]
fn SetupComposer(state: OnboardingUiState, reply: Reply) -> Element {
    let mut text = use_signal(|| {
        if state.step == Step::Prompt {
            state.text.clone()
        } else {
            String::new()
        }
    });
    let mut selected = use_signal(|| 0_usize);
    let choices = Choices::for_question(&state, &text());
    let labels = choices.labels();
    let loading = state.step == Step::Agent && state.agents.is_empty();
    let current = selected().min(choices.entries.len().saturating_sub(1));
    let clicked = choices.clone();
    let clicked_reply = reply.clone();
    let keyed = choices.clone();
    let keyed_reply = reply.clone();
    let submitted = choices.clone();
    let submitted_reply = reply.clone();
    let typed = reply.clone();
    let placeholder = match state.step {
        Step::Prompt => translate("onboarding-prompt"),
        Step::Agent => translate("composer-choose-agent"),
        Step::Workspace => translate("onboarding-workspace-question"),
    };
    rsx! {
        super::surface::Dock {
            if loading {
                Skeleton { class: "h-10 w-full rounded-xl" }
                Skeleton { class: "h-10 w-full rounded-xl" }
            } else {
            ChoiceOptions {
                options: labels,
                selected: current,
                disabled: state.starting,
                on_select: move |index| {
                    if !state.starting { clicked.choose(index, &clicked_reply); }
                },
                on_hover: move |index| selected.set(index),
                on_keydown: move |event| {
                    if !state.starting { keyed.key(event, selected, &text(), &keyed_reply); }
                },
            }
            }
            PromptComposer {
                value: text(),
                placeholder,
                accent_color: Identity::COLOR.to_string(),
                accent_gradient: "from-indigo-500 to-violet-600".to_string(),
                footer: Some(rsx! {}),
                disabled: state.starting || loading,
                show_add_button: false,
                action_title: translate("command-send"),
                action_enabled: !choices.entries.is_empty() || state.step != Step::Agent && !text().trim().is_empty(),
                on_input: move |value: String| {
                    text.set(value.clone());
                    selected.set(0);
                    if state.step == Step::Prompt {
                        let _ = send(&OnboardingDraftChanged { text: value, ..typed.0.clone() });
                    }
                },
                on_keydown: move |event| choices.key(event, selected, &text(), &reply),
                on_action: move |_| submitted.submit(&text(), current, &submitted_reply),
                on_paste: move |_| {},
                on_attach: move |_| {},
                on_remove_attachment: move |_| {},
            }
        }
    }
}

#[derive(Clone)]
struct Choices {
    step: Step,
    entries: Vec<Choice>,
}

impl Choices {
    fn for_question(state: &OnboardingUiState, text: &str) -> Self {
        let mut entries = Vec::new();
        match state.step {
            Step::Prompt => {
                for prompt in StarterPrompt::all() {
                    entries.push(Choice {
                        label: prompt.text(),
                        answer: Answer::Prompt(prompt.text()),
                    });
                }
            }
            Step::Agent => {
                let query = text.trim().to_lowercase();
                for agent in &state.agents {
                    if !agent.name.to_lowercase().contains(&query)
                        && !agent.id.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    let status = translate(if agent.environment_key {
                        "onboarding-key-available"
                    } else if agent.installed {
                        "onboarding-installed"
                    } else {
                        "onboarding-install"
                    });
                    entries.push(Choice {
                        label: format!("{} · {status}", agent.name),
                        answer: Answer::Agent(agent.id.clone()),
                    });
                }
            }
            Step::Workspace => {
                if !state.cwd.is_empty() {
                    entries.push(Choice {
                        label: format!("{} · {}", translate("onboarding-start"), state.cwd),
                        answer: Answer::Workspace(state.cwd.clone()),
                    });
                }
                entries.push(Choice {
                    label: translate("onboarding-default-workspace"),
                    answer: Answer::Workspace(String::new()),
                });
                entries.push(Choice {
                    label: translate("agent-project-choose"),
                    answer: Answer::Directory,
                });
            }
        }
        if state.step != Step::Prompt && (state.step != Step::Agent || text.trim().is_empty()) {
            entries.push(Choice {
                label: translate("layout-back"),
                answer: Answer::Back(state.step),
            });
        }
        Self {
            step: state.step,
            entries,
        }
    }

    fn labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        for choice in &self.entries {
            labels.push(choice.label.clone());
        }
        labels
    }

    fn choose(&self, index: usize, reply: &Reply) {
        if let Some(choice) = self.entries.get(index) {
            choice.answer.send(reply);
        }
    }

    fn submit(&self, text: &str, selected: usize, reply: &Reply) {
        if self.step == Step::Prompt && !text.trim().is_empty() {
            reply.prompt(text.to_string());
        } else if self.step == Step::Workspace && !text.trim().is_empty() {
            reply.start(text.trim().to_string());
        } else {
            self.choose(selected, reply);
        }
    }

    fn key(&self, event: KeyboardEvent, mut selected: Signal<usize>, text: &str, reply: &Reply) {
        let Some(key) = ChoiceKey::of(&event, text.is_empty()) else {
            return;
        };
        let current = selected().min(self.entries.len().saturating_sub(1));
        match key {
            ChoiceKey::Next => {
                selected.set((current + 1).min(self.entries.len().saturating_sub(1)))
            }
            ChoiceKey::Previous => selected.set(current.saturating_sub(1)),
            ChoiceKey::Submit => self.submit(text, current, reply),
            ChoiceKey::Number(index) if index < self.entries.len() => self.choose(index, reply),
            ChoiceKey::Number(_) => return,
        }
        event.prevent_default();
        event.stop_propagation();
    }
}

#[derive(Clone)]
struct Choice {
    label: String,
    answer: Answer,
}

#[derive(Clone)]
enum Answer {
    Prompt(String),
    Agent(String),
    Workspace(String),
    Directory,
    Back(Step),
}

impl Answer {
    fn send(&self, reply: &Reply) {
        match self {
            Self::Prompt(text) => reply.prompt(text.clone()),
            Self::Agent(agent) => reply.agent(agent.clone()),
            Self::Workspace(directory) => reply.start(directory.clone()),
            Self::Directory => {
                let _ = send(&crate::event::SessionsChooseDirectory {
                    current_dir: reply.0.cwd.clone(),
                });
            }
            Self::Back(step) => {
                let _ = send(&OnboardingBack { step: *step });
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct Reply(OnboardingDraftChanged);

impl Reply {
    fn prompt(&self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if send(&OnboardingDraftChanged {
            text,
            ..self.0.clone()
        })
        .is_ok()
        {
            let _ = send(&OnboardingContinue { step: Step::Prompt });
        }
    }

    fn agent(&self, agent_id: String) {
        if send(&OnboardingDraftChanged {
            agent_id,
            ..self.0.clone()
        })
        .is_ok()
        {
            let _ = send(&OnboardingContinue { step: Step::Agent });
        }
    }

    fn start(&self, cwd: String) {
        if send(&OnboardingDraftChanged {
            cwd,
            ..self.0.clone()
        })
        .is_ok()
        {
            let _ = send(&OnboardingStart);
        }
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
    struct Host {
        events: RefCell<Vec<(String, Vec<u8>)>>,
        questions: RefCell<Vec<BytesListener>>,
    }

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id != vmux_api::PageReady::id() {
                self.events.borrow_mut().push((id.into(), bytes.into()));
            }
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == OnboardingUiState::id() {
                self.questions.borrow_mut().push(listener);
            }
            Ok(())
        }
    }

    impl Host {
        fn publish(&self, state: OnboardingUiState) {
            let frame =
                vmux_api::UiStateFrame::<OnboardingUiState, OnboardingUiState>::Snapshot(state);
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            assert!(!self.questions.borrow().is_empty());
            for listener in &mut *self.questions.borrow_mut() {
                listener(&bytes);
            }
        }
    }

    #[component]
    fn LiveConversation() -> Element {
        vmux_ui::hooks::use_ui_state::<crate::state::ChatUiState>();
        vmux_ui::hooks::use_ui_state::<OnboardingUiState>();
        rsx! { Onboarding {} }
    }

    #[test]
    fn setup_greets_before_configuration_and_a_prompt_reply_advances_without_starting_an_agent() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(
            Conversation,
            ConversationProps {
                state: OnboardingUiState::default(),
                user: None,
            },
        );
        dom.provide_root_context(HostBinding(host.clone()));
        let initial = dom.rebuild_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(&translate("onboarding-title")));
        assert!(!html.contains("<header"));
        assert!(html.contains(&format!("alt=\"{}\"", translate("onboarding-guide"))));
        assert!(html.contains("data:image/svg+xml,"));
        assert!(!html.contains(&translate("onboarding-agent-question")));
        assert!(!html.contains(&translate("onboarding-workspace-question")));
        let label = translate("onboarding-build");
        let button = initial
            .edits
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &label => Some(*id),
                _ => None,
            })
            .expect("the greeting must offer a reply before any agent is configured");
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            button,
        );
        dom.render_immediate_to_vec();
        let events = host.events.borrow();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, OnboardingDraftChanged::id());
        let draft =
            rkyv::from_bytes::<OnboardingDraftChanged, rkyv::rancor::Error>(&events[0].1).unwrap();
        assert_eq!(draft.text, label);
        assert_eq!(events[1].0, OnboardingContinue::id());
        let advance =
            rkyv::from_bytes::<OnboardingContinue, rkyv::rancor::Error>(&events[1].1).unwrap();
        assert_eq!(advance.step, Step::Prompt);
    }

    #[test]
    fn composer_choices_accept_number_keys_and_arrow_then_enter() {
        for navigation in [false, true] {
            dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let host = Rc::new(Host::default());
            let mut dom = VirtualDom::new_with_props(
                Conversation,
                ConversationProps {
                    state: OnboardingUiState::default(),
                    user: None,
                },
            );
            dom.provide_root_context(HostBinding(host.clone()));
            let initial = dom.rebuild_to_vec();
            let input = initial
                .edits
                .iter()
                .find_map(|mutation| match mutation {
                    Mutation::SetAttribute {
                        name: "id",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == vmux_ui::components::composer::PROMPT_INPUT_ID => Some(*id),
                    _ => None,
                })
                .expect("setup uses the normal chat composer input");
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
                dom.runtime().handle_event(
                    "keydown",
                    Event::new(EventData::Keyboard(data).into_any(), true),
                    input,
                );
                dom.render_immediate_to_vec();
            }
            let events = host.events.borrow();
            assert_eq!(events.len(), 2);
            let draft =
                rkyv::from_bytes::<OnboardingDraftChanged, rkyv::rancor::Error>(&events[0].1)
                    .unwrap();
            assert_eq!(draft.text, translate("onboarding-fix"));
            assert_eq!(events[1].0, OnboardingContinue::id());
        }
    }

    #[test]
    fn the_next_question_keeps_the_task_in_history_and_clears_the_composer_reply() {
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new(LiveConversation);
        dom.provide_root_context(HostBinding(host.clone()));
        dom.rebuild_in_place();
        dom.render_immediate_to_vec();
        host.publish(OnboardingUiState {
            text: "Build a clock".into(),
            ..Default::default()
        });
        dom.render_immediate_to_vec();
        host.publish(OnboardingUiState {
            text: "Build a clock".into(),
            step: Step::Agent,
            agents: vec![crate::onboarding::AgentOption {
                id: "test-agent".into(),
                name: "Test agent".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("Build a clock"));
        assert!(html.contains("Test agent"));
        let input = html
            .split("<textarea")
            .nth(1)
            .unwrap()
            .split("</textarea>")
            .next()
            .unwrap();
        assert!(!input.contains("Build a clock"));
    }

    #[test]
    fn later_questions_keep_earlier_replies_in_the_conversation() {
        for step in [Step::Agent, Step::Workspace] {
            let mut dom = VirtualDom::new_with_props(
                Conversation,
                ConversationProps {
                    user: None,
                    state: OnboardingUiState {
                        text: "Help with my project".into(),
                        agent_id: "test-agent".into(),
                        agents: vec![crate::onboarding::AgentOption {
                            id: "test-agent".into(),
                            name: "Test agent".into(),
                            ..Default::default()
                        }],
                        step,
                        ..Default::default()
                    },
                },
            );
            dom.rebuild_in_place();
            let html = dioxus_ssr::render(&dom);
            let prompt = html.find("Help with my project").unwrap();
            let agent_question = html.find(&translate("onboarding-agent-question")).unwrap();
            assert!(prompt < agent_question);
            let workspace_question = html.find(&translate("onboarding-workspace-question"));
            match step {
                Step::Agent => assert!(workspace_question.is_none()),
                Step::Workspace => {
                    let agent = html.find("Test agent").unwrap();
                    assert!(agent_question < agent && agent < workspace_question.unwrap());
                }
                Step::Prompt => unreachable!(),
            }
        }
    }
}
