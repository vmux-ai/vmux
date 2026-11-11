use dioxus::prelude::*;
use vmux_ui::components::composer::ComposerFrame;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::starter_prompts::StarterPrompts;
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::translate;

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
    rsx! { Conversation { state } }
}

#[component]
fn Conversation(state: OnboardingUiState) -> Element {
    let reply = Reply(OnboardingDraftChanged {
        text: state.text.clone(),
        agent_id: state.agent_id.clone(),
        cwd: state.cwd.clone(),
    });
    let prompts = reply.clone();
    let workspace = reply.clone();
    let submitted = reply.clone();
    let selected = state.agents.iter().find(|agent| agent.id == state.agent_id);
    let selected_name = selected.map(|agent| agent.name.clone()).unwrap_or_default();
    let directory = state.cwd.clone();
    rsx! {
        main { class: "session-chat-page relative flex h-dvh flex-col overflow-hidden bg-background text-foreground",
            header { class: "mx-auto flex w-full max-w-3xl shrink-0 items-center px-4 py-4 text-sm font-medium", "vmux" }
            div { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-6",
                div { class: "mx-auto flex w-full max-w-3xl flex-col gap-6", role: "log", aria_live: "polite",
                    Message { text: translate("onboarding-title"),
                        p { class: "text-sm text-muted-foreground", {translate("onboarding-intro")} }
                        if state.step == Step::Prompt {
                            StarterPrompts { on_choose: move |text| prompts.prompt(text) }
                        }
                    }
                    if state.step != Step::Prompt {
                        Message { user: true, text: state.text.clone() }
                        Message { text: translate("onboarding-agent-question"),
                            if state.step == Step::Agent {
                                if state.agents.is_empty() {
                                    div { class: "flex gap-3",
                                        Skeleton { class: "h-12 w-40" }
                                        Skeleton { class: "h-12 w-40" }
                                    }
                                } else {
                                    div { class: "flex flex-wrap gap-2",
                                        for agent in state.agents.clone() {
                                            {
                                                let choice = reply.clone();
                                                let agent_id = agent.id.clone();
                                                rsx! {
                                                    button {
                                                        key: "{agent.id}",
                                                        r#type: "button",
                                                        class: "flex items-center gap-3 rounded-xl bg-muted/40 px-4 py-3 text-left hover:bg-muted/60",
                                                        aria_label: agent.name.clone(),
                                                        disabled: state.starting,
                                                        onclick: move |_| choice.agent(agent_id.clone()),
                                                        vmux_ui::components::avatar::AgentThumbnail {
                                                            src: (!agent.icon.is_empty()).then_some(agent.icon),
                                                            seed: agent.id,
                                                            alt: agent.name.clone(),
                                                            background: "#6366f1",
                                                            class: "size-8",
                                                        }
                                                        div { class: "flex flex-col",
                                                            span { class: "text-sm font-medium", "{agent.name}" }
                                                            span { class: "text-xs text-muted-foreground",
                                                                {translate(if agent.environment_key { "onboarding-key-available" } else if agent.installed { "onboarding-installed" } else { "onboarding-install" })}
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if state.step == Step::Workspace {
                        Message { user: true, text: selected_name }
                        Message { text: translate("onboarding-workspace-question"),
                            p { class: "text-sm text-muted-foreground", {translate("onboarding-workspace-hint")} }
                            div { class: "flex flex-wrap gap-2",
                                button {
                                    r#type: "button",
                                    class: "rounded-xl bg-muted/40 px-4 py-3 text-sm hover:bg-muted/60",
                                    disabled: state.starting,
                                    onclick: move |_| workspace.start(String::new()),
                                    {translate("onboarding-default-workspace")}
                                }
                                button {
                                    r#type: "button",
                                    class: "rounded-xl bg-muted/40 px-4 py-3 text-sm hover:bg-muted/60",
                                    disabled: state.starting,
                                    onclick: move |_| { let _ = send(&crate::event::SessionsChooseDirectory { current_dir: directory.clone() }); },
                                    {translate("agent-project-choose")}
                                }
                            }
                        }
                        if !state.cwd.is_empty() {
                            Message { user: true, text: state.cwd.clone() }
                            button {
                                r#type: "button",
                                class: "vmux-primary-action self-start rounded-xl px-4 py-3 text-sm",
                                disabled: state.starting,
                                onclick: move |_| submitted.start(submitted.0.cwd.clone()),
                                {translate("onboarding-install")}
                            }
                        }
                    }
                    if state.step != Step::Prompt {
                        button {
                            r#type: "button",
                            class: "self-start rounded-lg px-3 py-2 text-xs text-muted-foreground hover:bg-muted/40",
                            disabled: state.starting,
                            onclick: move |_| { let _ = send(&OnboardingBack { step: state.step }); },
                            {translate("layout-back")}
                        }
                    }
                }
            }
            if state.step == Step::Prompt {
                Prompt { reply, starting: state.starting }
            }
        }
    }
}

#[component]
fn Message(
    text: String,
    #[props(default)] user: bool,
    #[props(default = rsx! {})] children: Element,
) -> Element {
    rsx! {
        div { class: if user { "ml-auto max-w-[85%] rounded-2xl bg-muted/40 px-4 py-3" } else { "flex flex-col items-start gap-3" },
            if !user {
                span { class: "text-xs font-medium text-muted-foreground", "vmux" }
            }
            p { class: "whitespace-pre-wrap text-sm leading-relaxed", "{text}" }
            {children}
        }
    }
}

#[component]
fn Prompt(reply: Reply, starting: bool) -> Element {
    let mut text = use_signal(|| reply.0.text.clone());
    let ime = vmux_ui::ime::use_ime_guard();
    let entered = reply.clone();
    let typed = reply.clone();
    rsx! {
        div { class: "mx-auto w-full max-w-3xl shrink-0 px-4 pb-4",
            ComposerFrame {
                div { class: "flex items-end gap-2 rounded-2xl bg-muted/40 px-4 py-3",
                    textarea {
                        class: "max-h-40 min-h-10 flex-1 resize-none bg-transparent text-sm outline-none",
                        aria_label: translate("onboarding-prompt"),
                        placeholder: translate("onboarding-prompt"),
                        value: text(),
                        disabled: starting,
                        oncompositionstart: move |_| ime.start(),
                        oncompositionend: move |_| ime.commit(),
                        oninput: move |event| {
                            let value = event.value();
                            text.set(value.clone());
                            let _ = send(&OnboardingDraftChanged { text: value, ..typed.0.clone() });
                        },
                        onkeydown: move |event| {
                            if !ime.swallows(&event) && event.key() == Key::Enter && !event.modifiers().shift() {
                                event.prevent_default();
                                if !starting && !text.peek().trim().is_empty() {
                                    entered.prompt(text());
                                }
                            }
                        },
                    }
                    button {
                        r#type: "button",
                        class: "vmux-primary-action rounded-lg px-3 py-2 text-sm disabled:opacity-40",
                        aria_label: translate("command-send"),
                        disabled: starting || text().trim().is_empty(),
                        onclick: move |_| reply.prompt(text()),
                        {translate("command-send")}
                    }
                }
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
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, PageHost};

    #[derive(Default)]
    struct Host {
        events: RefCell<Vec<(String, Vec<u8>)>>,
    }

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            self.events.borrow_mut().push((id.into(), bytes.into()));
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[test]
    fn setup_greets_before_configuration_and_a_prompt_reply_advances_without_starting_an_agent() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new_with_props(
            Conversation,
            ConversationProps {
                state: OnboardingUiState::default(),
            },
        );
        dom.provide_root_context(HostBinding(host.clone()));
        let initial = dom.rebuild_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(&translate("onboarding-title")));
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
    fn later_questions_keep_earlier_replies_in_the_conversation() {
        for step in [Step::Agent, Step::Workspace] {
            let mut dom = VirtualDom::new_with_props(
                Conversation,
                ConversationProps {
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
