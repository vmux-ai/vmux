//! What the agent is blocked on: permission to run a tool, or an answer to a question it asked.
//!
//! The two arrive on different signals and dock in different places, but they are one thing to
//! the reader — a numbered list the conversation cannot move past — and they share the option
//! styling and the keyboard help line that says how to pick from it.

use super::state::Chat;
use crate::format::approval::ApprovalDetail;
use crate::format::composer::approval_decision_for_index;
use dioxus::prelude::*;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

/// The tool the agent is asking permission to run, and the three answers to it.
#[component]
pub(super) fn ChatApprovalPanel(chat: Chat) -> Element {
    if chat.installing() {
        return rsx! {};
    }
    let Some((call_id, name, args_json)) = (chat.run.approval)() else {
        return rsx! {};
    };
    let approval_sel = chat.run.approval_sel;
    let details = ApprovalDetail::rows(&args_json);
    rsx! {
        div { class: "border-t border-foreground/10 bg-foreground/[0.04] px-4 py-3",
            div { class: "mx-auto flex max-w-3xl flex-col gap-3",
                div { class: "min-w-0",
                    div { class: "text-sm text-foreground",
                        {translate_with(
                            "agent-allow-tool",
                            &[("tool", TranslationValue::String(&name))],
                        )}
                    }
                    if !details.is_empty() {
                        div { class: "mt-2 max-h-40 overflow-auto rounded-lg bg-foreground/[0.05] ring-1 ring-inset ring-foreground/10",
                            for (i , detail) in details.iter().enumerate() {
                                div {
                                    key: "approval-detail-{i}",
                                    class: "grid grid-cols-[7rem_minmax(0,1fr)] items-start gap-3 border-b border-foreground/10 px-3 py-2 last:border-b-0",
                                    span { class: "pt-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/70", "{approval_detail_label(&detail.label)}" }
                                    pre { class: "overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-muted-foreground", "{detail.value}" }
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-col gap-1.5",
                    for (index , label) in [translate("agent-allow"), translate("agent-allow-always"), translate("agent-deny")].into_iter().enumerate() {
                        button {
                            key: "approval-option-{index}",
                            class: if approval_sel() == index { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                            onclick: {
                                let call_id = call_id.clone();
                                move |_| {
                                    if let Some(decision) = approval_decision_for_index(index) {
                                        chat.answer_approval(call_id.clone(), decision);
                                    }
                                }
                            },
                            span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                            span { class: "min-w-0 flex-1", "{label}" }
                        }
                    }
                    div { class: "mt-1 text-[11px] text-muted-foreground", {translate("agent-choice-help").replace("1–9", "1–3")} }
                }
            }
        }
    }
}

fn approval_detail_label(label: &str) -> String {
    match label {
        "Details" => translate("agent-details"),
        "Path" => translate("agent-path"),
        "Tool" => translate("agent-tool"),
        "Server" => translate("agent-server"),
        _ => label.to_string(),
    }
}

/// A question the agent asked, with its numbered answers.
#[component]
pub(super) fn ChoiceList(chat: Chat) -> Element {
    let options = (chat.run.choice_options)();
    if options.is_empty() {
        return rsx! {};
    }
    let mut menu_sel = chat.slash.menu_sel;
    let question = (chat.run.choice_question)();
    rsx! {
        div { class: "rounded-2xl border border-foreground/10 bg-foreground/[0.045] p-3.5 shadow-sm",
            div { class: "mb-3 text-sm font-medium text-foreground", "{question}" }
            div { class: "flex flex-col gap-1.5",
                for (index , option) in options.into_iter().enumerate() {
                    button {
                        key: "choice-{index}",
                        id: "agent-choice-item-{index}",
                        onmouseenter: move |_| menu_sel.set(index),
                        class: if index == menu_sel() { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                        onclick: move |_| chat.answer_choice(index),
                        span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                        span { class: "min-w-0 flex-1", "{option}" }
                    }
                }
            }
            div { class: "mt-2.5 text-[11px] text-muted-foreground", {translate("agent-choice-help")} }
        }
    }
}
