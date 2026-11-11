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
    let Some(approval) = chat.run.approval() else {
        return rsx! {};
    };
    rsx! {
        ApprovalPanel {
            tool: approval.name,
            details: approval.details,
            selected: Some(chat.approval_selection()),
            on_answer: move |decision| chat.answer_approval(approval.call_id.clone(), decision),
            on_hover: move |index| chat.point_at_approval(index),
        }
    }
}

#[component]
pub fn ApprovalPanel(
    tool: String,
    details: Vec<ApprovalDetail>,
    #[props(default)] selected: Option<usize>,
    on_answer: EventHandler<ApprovalDecision>,
    on_hover: EventHandler<usize>,
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
                            onmouseenter: move |_| on_hover.call(index),
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
    let options = chat.run.choice_options();
    if options.is_empty() {
        return rsx! {};
    }
    let question = chat.run.choice_question();
    rsx! {
        ChoiceOptions {
            question: Some(question),
            options,
            selected: chat.list_selection(),
            on_select: move |index| chat.choose_list(index),
            on_hover: move |index| chat.point_at_choice(index),
        }
    }
}

#[component]
pub(super) fn ChoiceOptions(
    options: Vec<String>,
    selected: usize,
    #[props(default)] question: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] compact: bool,
    #[props(default)] framed: bool,
    #[props(default = true)] show_numbers: bool,
    #[props(default = true)] show_help: bool,
    #[props(default)] leading: Vec<Element>,
    #[props(default)] trailing: Vec<Element>,
    #[props(default)] disabled_options: Vec<usize>,
    #[props(default)] on_keydown: Option<EventHandler<KeyboardEvent>>,
    on_select: EventHandler<usize>,
    on_hover: EventHandler<usize>,
) -> Element {
    let label = question
        .clone()
        .unwrap_or_else(|| translate("agent-choose-option"));
    rsx! {
        div { class: if framed { "px-2 py-2" } else if compact { "" } else { "border-l-2 border-foreground/15 py-2 pl-3.5" },
            if let Some(question) = question {
                div { class: "mb-3 text-sm font-medium text-foreground", "{question}" }
            }
            div { class: "flex max-h-64 flex-col gap-1.5 overflow-y-auto overscroll-contain", role: "listbox", aria_label: label,
                onkeydown: move |event| { if let Some(handler) = on_keydown { handler.call(event); } },
                for (index , option) in options.into_iter().enumerate() {
                    button {
                        key: "choice-{index}",
                        id: "agent-choice-item-{index}",
                        r#type: "button",
                        role: "option",
                        aria_label: option.clone(),
                        aria_selected: index == selected,
                        disabled: disabled || disabled_options.contains(&index),
                        onfocus: move |_| on_hover.call(index),
                        onmouseenter: move |_| on_hover.call(index),
                        class: if framed {
                            if index == selected { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg bg-background/10 px-2 py-1.5 text-left text-sm text-inherit" } else { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm text-inherit hover:bg-background/[0.06]" }
                        } else if compact {
                            if index == selected { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg bg-foreground/[0.08] px-2 py-1.5 text-left text-sm text-foreground" } else { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm text-foreground hover:bg-foreground/[0.04]" }
                        } else if index == selected { "flex shrink-0 items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex shrink-0 items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                        onclick: move |_| on_select.call(index),
                        if show_numbers {
                            span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                        } else {
                            span { aria_hidden: "true", class: "flex size-3.5 shrink-0 items-center justify-center rounded-full border border-current/30",
                                if index == selected { span { class: "size-1.5 rounded-full bg-current" } }
                            }
                        }
                        if let Some(icon) = leading.get(index) { {icon.clone()} }
                        span { class: "min-w-0 flex-1", "{option}" }
                        if let Some(detail) = trailing.get(index) { {detail.clone()} }
                    }
                }
            }
            if show_help {
                div { class: if framed { "mt-2 px-2 text-[11px] opacity-60" } else { "mt-2.5 text-[11px] text-muted-foreground" }, {translate("agent-choice-help")} }
            }
        }
    }
}

pub(super) enum ChoiceKey {
    Next,
    Previous,
    Submit,
    Number(usize),
}

impl ChoiceKey {
    pub(super) fn of(event: &KeyboardEvent, numbers: bool) -> Option<Self> {
        let modifiers = event.modifiers();
        if event.data().is_composing() || modifiers.meta() || modifiers.alt() || modifiers.shift() {
            return None;
        }
        let key = event.key();
        if key == Key::ArrowDown || modifiers.ctrl() && key == Key::Character("n".into()) {
            return Some(Self::Next);
        }
        if key == Key::ArrowUp || modifiers.ctrl() && key == Key::Character("p".into()) {
            return Some(Self::Previous);
        }
        if key == Key::Enter && !modifiers.ctrl() {
            return Some(Self::Submit);
        }
        if !numbers || modifiers.ctrl() {
            return None;
        }
        let Key::Character(number) = key else {
            return None;
        };
        let number = number.parse::<usize>().ok()?;
        if !(1..=9).contains(&number) {
            return None;
        }
        Some(Self::Number(number - 1))
    }
}
