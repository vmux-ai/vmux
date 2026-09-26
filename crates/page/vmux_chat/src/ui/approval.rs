use super::state::Chat;
use crate::event::{ApprovalDecision, ApprovalDetail};
use dioxus::prelude::*;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

pub(super) const APPROVAL_DECISIONS: [ApprovalDecision; 3] = [
    ApprovalDecision::Allow,
    ApprovalDecision::AllowAlways,
    ApprovalDecision::Deny,
];

#[component]
pub(super) fn ChatApprovalDock(chat: Chat) -> Element {
    if chat.installing() {
        return rsx! {};
    }
    let Some(approval) = (chat.run.approval)() else {
        return rsx! {};
    };
    rsx! {
        ApprovalPanel {
            tool: approval.name,
            details: approval.details,
            selected: Some((chat.run.approval_sel)()),
            on_answer: move |decision| chat.answer_approval(approval.call_id.clone(), decision),
        }
    }
}

#[component]
pub fn ApprovalPanel(
    tool: String,
    details: Vec<ApprovalDetail>,
    #[props(default)] selected: Option<usize>,
    on_answer: EventHandler<ApprovalDecision>,
) -> Element {
    rsx! {
        div { class: "border-t border-foreground/10 bg-foreground/[0.04] px-4 py-3",
            div { class: "mx-auto flex max-w-3xl flex-col gap-3",
                div { class: "min-w-0",
                    div { class: "text-sm text-foreground",
                        {translate_with(
                            "agent-allow-tool",
                            &[("tool", TranslationValue::String(&tool))],
                        )}
                    }
                    if !details.is_empty() {
                        div { class: "mt-2 max-h-40 overflow-auto border-y border-foreground/10",
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
                    for (index , decision) in APPROVAL_DECISIONS.into_iter().enumerate() {
                        button {
                            key: "approval-option-{index}",
                            class: if selected == Some(index) { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                            onclick: move |_| on_answer.call(decision),
                            span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                            span { class: "min-w-0 flex-1", {approval_answer_label(decision)} }
                        }
                    }
                    if selected.is_some() {
                        div { class: "mt-1 text-[11px] text-muted-foreground", {translate("agent-choice-help").replace("1–9", "1–3")} }
                    }
                }
            }
        }
    }
}

fn approval_answer_label(decision: ApprovalDecision) -> String {
    match decision {
        ApprovalDecision::Allow => translate("agent-allow"),
        ApprovalDecision::AllowAlways => translate("agent-allow-always"),
        ApprovalDecision::Deny => translate("agent-deny"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_is_the_last_approval_choice() {
        assert_eq!(
            APPROVAL_DECISIONS,
            [
                ApprovalDecision::Allow,
                ApprovalDecision::AllowAlways,
                ApprovalDecision::Deny,
            ]
        );
    }
}

#[component]
pub(super) fn ChoiceList(chat: Chat) -> Element {
    let options = (chat.run.choice_options)();
    if options.is_empty() {
        return rsx! {};
    }
    let mut menu_sel = chat.slash.menu_sel;
    let question = (chat.run.choice_question)();
    rsx! {
        div { class: "border-l-2 border-foreground/15 py-2 pl-3.5",
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
