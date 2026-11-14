use crate::i18n::translate;
use crate::scroll::ScrollIntoView;
use dioxus::core::current_scope_id;
use dioxus::prelude::*;

#[component]
pub fn ChoiceOptions(
    options: Vec<String>,
    selected: usize,
    #[props(default)] question: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] compact: bool,
    #[props(default)] framed: bool,
    #[props(default)] muted: bool,
    #[props(default = "max-h-64".to_string())] viewport_class: String,
    #[props(default = true)] show_numbers: bool,
    #[props(default = true)] show_help: bool,
    #[props(default)] leading: Vec<Element>,
    #[props(default)] trailing: Vec<Element>,
    #[props(default)] disabled_options: Vec<usize>,
    #[props(default)] id_prefix: Option<String>,
    #[props(default)] on_keydown: Option<EventHandler<KeyboardEvent>>,
    on_select: EventHandler<usize>,
    on_hover: EventHandler<usize>,
) -> Element {
    let label = question
        .clone()
        .unwrap_or_else(|| translate("agent-choose-option"));
    let item_prefix = id_prefix.unwrap_or_else(|| format!("choice-{}", current_scope_id().0));
    let scroll_prefix = item_prefix.clone();
    use_effect(use_reactive!(|(selected, scroll_prefix)| {
        ScrollIntoView::nearest(&format!("{scroll_prefix}-{selected}"));
    }));
    rsx! {
        div { class: if framed { "px-2 py-2" } else if compact { "" } else { "border-l-2 border-foreground/15 py-2 pl-3.5" },
            if let Some(question) = question {
                div { class: "mb-3 text-sm font-medium text-foreground", "{question}" }
            }
            div { class: "flex flex-col gap-1.5 overflow-y-auto overscroll-contain {viewport_class}", role: "listbox", aria_label: label,
                onkeydown: move |event| { if let Some(handler) = on_keydown { handler.call(event); } },
                for (index , option) in options.into_iter().enumerate() {
                    button {
                        key: "choice-{index}",
                        id: "{item_prefix}-{index}",
                        r#type: "button",
                        role: "option",
                        aria_label: option.clone(),
                        aria_selected: index == selected,
                        disabled: disabled || disabled_options.contains(&index),
                        onfocus: move |_| on_hover.call(index),
                        onmouseenter: move |_| on_hover.call(index),
                        class: if framed && !muted {
                            if index == selected { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg bg-background/10 px-2 py-1.5 text-left text-sm text-inherit" } else { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm text-inherit hover:bg-background/[0.06]" }
                        } else if compact {
                            if index == selected { "vmux-primary-action flex min-h-8 shrink-0 cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm" } else { "flex min-h-8 shrink-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm text-foreground hover:bg-foreground/[0.04]" }
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

pub enum ChoiceKey {
    Next,
    Previous,
    Submit,
    Number(usize),
}

impl ChoiceKey {
    pub fn of(event: &KeyboardEvent, numbers: bool) -> Option<Self> {
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
