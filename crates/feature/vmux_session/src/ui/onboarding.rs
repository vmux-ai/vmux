use dioxus::prelude::*;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{UiStateBinding, send};
use vmux_ui::i18n::translate;

use vmux_ui::components::starter_prompts::StarterPrompts;

use crate::onboarding::{OnboardingDraftChanged, OnboardingStart, OnboardingUiState};

#[component]
pub(super) fn Onboarding() -> Element {
    let binding = UiStateBinding::<OnboardingUiState>::use_context();
    let state = binding.state.read().clone();
    let key_state = UiStateBinding::<crate::onboarding::KeyEntryUiState>::use_context()
        .state
        .read()
        .clone();
    let selected = state
        .agents
        .iter()
        .find(|agent| agent.id == state.agent_id)
        .or_else(|| state.agents.first());
    let selected_id = selected.map(|agent| agent.id.clone()).unwrap_or_default();
    let key_variable = selected
        .map(|agent| agent.key_variable.clone())
        .unwrap_or_default();
    let key_available = selected.is_some_and(|agent| agent.environment_key);
    let current = OnboardingDraftChanged {
        text: state.text.clone(),
        agent_id: selected_id,
        cwd: state.cwd.clone(),
    };
    let prompts = current.clone();
    let typed = current.clone();
    let submitted = current.clone();
    let directory = current.cwd.clone();
    let default_workspace = current.clone();
    let key_agent = current.agent_id.clone();
    let key_selection = current.clone();
    rsx! {
        section { class: "mx-auto flex w-full max-w-3xl flex-col gap-6 px-4 py-12",
            h1 { class: "text-2xl font-semibold tracking-tight", {translate("onboarding-title")} }
            p { class: "text-sm text-muted-foreground", {translate("onboarding-intro")} }
            StarterPrompts {
                on_choose: move |text| {
                    let _ = send(&OnboardingDraftChanged { text, ..prompts.clone() });
                },
            }
            textarea {
                class: "min-h-24 w-full resize-none rounded-xl bg-muted/40 px-4 py-3 text-sm outline-none focus:ring-2 focus:ring-primary/30",
                aria_label: translate("onboarding-prompt"),
                placeholder: translate("onboarding-prompt"),
                value: state.text.clone(),
                disabled: state.starting,
                oninput: move |event| {
                    let _ = send(&OnboardingDraftChanged { text: event.value(), ..typed.clone() });
                },
            }
            p { class: "text-sm text-muted-foreground", {translate("onboarding-agent-question")} }
            if state.agents.is_empty() {
                div { class: "flex gap-3",
                    Skeleton { class: "h-16 flex-1" }
                    Skeleton { class: "h-16 flex-1" }
                    Skeleton { class: "h-16 flex-1" }
                }
            } else {
                div { class: "grid gap-2 sm:grid-cols-2",
                    for agent in state.agents {
                        {
                            let choice = OnboardingDraftChanged { agent_id: agent.id.clone(), ..current.clone() };
                            let active = current.agent_id == agent.id;
                            rsx! {
                                button {
                                    key: "{agent.id}",
                                    r#type: "button",
                                    class: if active { "flex items-center gap-3 rounded-xl bg-primary/10 px-4 py-3 text-left ring-1 ring-primary/30" } else { "flex items-center gap-3 rounded-xl bg-muted/30 px-4 py-3 text-left hover:bg-muted/60" },
                                    aria_pressed: active,
                                    disabled: state.starting,
                                    onclick: move |_| { let _ = send(&choice); },
                                    vmux_ui::components::avatar::AgentThumbnail {
                                        src: (!agent.icon.is_empty()).then_some(agent.icon),
                                        seed: agent.id,
                                        alt: agent.name.clone(),
                                        background: "#6366f1",
                                        class: "size-8",
                                    }
                                    div { class: "flex min-w-0 flex-col",
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
            if state.session_id.is_empty() && !key_variable.is_empty() && !key_available {
                vmux_ui::components::api_key::ApiKeyEntry {
                    key: "key-{current.agent_id}",
                    pending: state.starting || key_state.pending,
                    failed: key_state.failed && key_state.agent_id == current.agent_id,
                    on_submit: move |key| {
                        if send(&key_selection).is_ok() {
                            let _ = send(&crate::onboarding::OnboardingSaveKey { agent_id: key_agent.clone(), key });
                        }
                    },
                }
            }
            p { class: "text-xs text-muted-foreground", {translate("onboarding-workspace-hint")} }
            div { class: "flex flex-wrap items-center gap-2 text-xs",
                button {
                    r#type: "button",
                    class: "rounded-lg bg-muted/40 px-3 py-2 hover:bg-muted/60",
                    disabled: state.starting,
                    onclick: move |_| {
                        let _ = send(&crate::event::SessionsChooseDirectory { current_dir: directory.clone() });
                    },
                    {translate("agent-project-choose")}
                }
                if !state.cwd.is_empty() {
                    span { class: "min-w-0 truncate text-muted-foreground", "{state.cwd}" }
                    button {
                        r#type: "button",
                        class: "rounded-lg px-3 py-2 hover:bg-muted/40",
                        disabled: state.starting,
                        onclick: move |_| {
                            let _ = send(&OnboardingDraftChanged { cwd: String::new(), ..default_workspace.clone() });
                        },
                        {translate("onboarding-default-workspace")}
                    }
                }
            }
            button {
                r#type: "button",
                class: "vmux-primary-action self-start rounded-lg px-4 py-2 text-sm font-medium disabled:opacity-40",
                disabled: state.starting || key_state.pending || current.agent_id.is_empty() || state.text.trim().is_empty(),
                onclick: move |_| {
                    if send(&submitted).is_ok() { let _ = send(&OnboardingStart); }
                },
                {translate(if state.session_id.is_empty() { "onboarding-start" } else { "sessions-resume" })}
            }
        }
    }
}
