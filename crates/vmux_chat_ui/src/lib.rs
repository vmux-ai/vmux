#![allow(non_snake_case)]

use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct PlanItem {
    pub content: String,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivityKind {
    Thinking,
    Python,
    ReadFile,
    Search,
    Image,
    Command,
    Browser,
    Guardian,
    Subagent,
    Tool,
    Output,
    Error,
    Plan,
    Diff,
    Reconnect,
}

#[component]
pub fn UserBubble(children: Element) -> Element {
    rsx! {
        div { class: "chat-user-bubble flex max-w-[80%] self-end flex-col gap-2 rounded-[1.35rem] rounded-tr-md border p-2.5 text-sm",
            {children}
        }
    }
}

#[component]
pub fn AssistantTurn(#[props(default = true)] standalone: bool, children: Element) -> Element {
    let placement = if standalone {
        "max-w-[94%] self-start"
    } else {
        "w-full"
    };
    rsx! {
        div { class: "chat-assistant-turn relative flex flex-col gap-2.5 overflow-hidden rounded-2xl border px-3.5 py-3 {placement}",
            {children}
        }
    }
}

#[component]
pub fn TextBlock(text: String) -> Element {
    rsx! {
        div {
            class: "min-w-0 break-words px-0.5 text-sm leading-6 text-[var(--foreground,#f4f4f5)] [&_a]:text-violet-400 [&_a]:underline [&_blockquote]:my-3 [&_blockquote]:border-l-2 [&_blockquote]:border-zinc-500/30 [&_blockquote]:pl-3 [&_code]:rounded [&_code]:bg-zinc-500/10 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.82em] [&_h1]:mb-3 [&_h1]:mt-5 [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:mb-2 [&_h2]:mt-4 [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:mb-2 [&_h3]:mt-3 [&_h3]:font-semibold [&_li]:my-1 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:mb-3 [&_p:last-child]:mb-0 [&_pre]:my-3 [&_pre]:max-w-full [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-black/25 [&_pre]:p-3 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5",
            dangerous_inner_html: markdown_html(&text),
        }
    }
}

#[component]
pub fn ThinkingBlock(
    text: String,
    #[props(default)] open: bool,
    #[props(default)] icon: Option<Element>,
) -> Element {
    rsx! {
        ActivityRow { kind: ActivityKind::Thinking, icon,
            details { open, class: "min-w-0 text-sm text-[var(--muted-foreground,#a1a1aa)]",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "Thinking" }
                    DisclosureIcon {}
                }
                div { class: "mt-2 whitespace-pre-wrap border-l border-zinc-500/20 pl-3 text-xs leading-relaxed", "{text}" }
            }
        }
    }
}

#[component]
pub fn ToolUseBlock(
    name: String,
    args: String,
    #[props(default)] compact: bool,
    #[props(default)] open: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] icon: Option<Element>,
    #[props(default)] arguments: Option<Element>,
    children: Element,
) -> Element {
    let (kind, default_label) = tool_presentation(&name, &args);
    let label = label.unwrap_or(default_label);
    if compact {
        return rsx! {
            details { open, class: "text-xs text-[var(--muted-foreground,#a1a1aa)]",
                summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "{label}" }
                    DisclosureIcon {}
                }
                div { class: "mt-1 text-[11px] font-medium opacity-60", "{name}" }
                if let Some(arguments) = arguments {
                    {arguments}
                } else if !args.is_empty() && args != "{}" {
                    CodePanel { value: args }
                }
                {children}
            }
        };
    }
    rsx! {
        ActivityRow { kind, icon,
            div { class: "min-w-0",
                details { open, class: "text-sm text-[var(--muted-foreground,#a1a1aa)]",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "{label}" }
                        DisclosureIcon {}
                    }
                    div { class: "mt-1 text-[11px] font-medium opacity-60", "{name}" }
                    if let Some(arguments) = arguments {
                        {arguments}
                    } else if !args.is_empty() && args != "{}" {
                        CodePanel { value: args }
                    }
                }
                {children}
            }
        }
    }
}

#[component]
pub fn SubagentActivity(
    title: String,
    status: String,
    provider: String,
    action: String,
    agent_name: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    prompt: Option<String>,
    thread_id: Option<String>,
    parent_thread_id: Option<String>,
    child_thread_ids: Vec<String>,
    call_id: String,
    raw_input: String,
    #[props(default)] compact: bool,
    #[props(default)] icon: Option<Element>,
    children: Element,
) -> Element {
    let title = if title.is_empty() {
        "Subagent".to_string()
    } else {
        title.replace('_', " ")
    };
    let action = action.replace('_', " ");
    let status_label = status_label(&status);
    let status_class = status_class(&status);
    let child_threads = child_thread_ids.join(", ");
    let content = rsx! {
        details { open: !compact && status == "in_progress", class: "min-w-0 text-sm text-[var(--muted-foreground,#a1a1aa)]",
            summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium text-[var(--foreground,#f4f4f5)]/85", "{title}" }
                span { class: "rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                DisclosureIcon {}
            }
            div { class: "mt-2 flex flex-wrap gap-1.5 text-[10px]",
                if !provider.is_empty() {
                    span { class: "rounded-full bg-violet-500/10 px-2 py-0.5 font-semibold text-violet-300", "{provider}" }
                }
                if !action.is_empty() {
                    span { class: "rounded-full bg-zinc-500/10 px-2 py-0.5", "{action}" }
                }
                if let Some(agent_name) = agent_name {
                    span { class: "rounded-full bg-zinc-500/10 px-2 py-0.5", "{agent_name}" }
                }
                if let Some(model) = model {
                    span { class: "rounded-full bg-zinc-500/10 px-2 py-0.5 font-mono", "{model}" }
                }
                if let Some(effort) = reasoning_effort {
                    span { class: "rounded-full bg-zinc-500/10 px-2 py-0.5", "{effort}" }
                }
            }
            if let Some(prompt) = prompt {
                div { class: "mt-2 rounded-lg bg-zinc-500/[0.045] p-2 text-xs leading-relaxed ring-1 ring-inset ring-zinc-500/15",
                    div { class: "mb-1 text-[10px] font-semibold uppercase tracking-wide opacity-60", "Prompt" }
                    div { class: "whitespace-pre-wrap", "{prompt}" }
                }
            }
            if !compact {
                div { class: "mt-2 grid gap-1 text-[10px] opacity-75",
                    if let Some(thread_id) = thread_id {
                        div { span { class: "font-semibold", "Thread " } code { class: "break-all font-mono", "{thread_id}" } }
                    }
                    if let Some(parent_thread_id) = parent_thread_id {
                        div { span { class: "font-semibold", "Parent " } code { class: "break-all font-mono", "{parent_thread_id}" } }
                    }
                    if !child_threads.is_empty() {
                        div { span { class: "font-semibold", "Children " } code { class: "break-all font-mono", "{child_threads}" } }
                    }
                    if !call_id.is_empty() {
                        div { span { class: "font-semibold", "Call " } code { class: "break-all font-mono", "{call_id}" } }
                    }
                }
                if !raw_input.is_empty() && raw_input != "{}" {
                    details { class: "mt-2 text-[11px]",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium", "Raw event" }
                            DisclosureIcon {}
                        }
                        CodePanel { value: raw_input }
                    }
                }
            }
            {children}
        }
    };
    if compact {
        content
    } else {
        rsx! {
            ActivityRow { kind: ActivityKind::Subagent, icon, tone: "bg-violet-500/[0.025] ring-1 ring-inset ring-violet-500/10",
                {content}
            }
        }
    }
}

#[component]
pub fn PlanBlock(steps: Vec<PlanItem>, #[props(default)] icon: Option<Element>) -> Element {
    let count = steps.len();
    rsx! {
        ActivityRow { kind: ActivityKind::Plan, icon,
            details { open: true, class: "min-w-0 text-sm",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "Plan" }
                    span { class: "text-xs text-[var(--muted-foreground,#a1a1aa)]", "{count} tasks" }
                    DisclosureIcon {}
                }
                ul { class: "mt-2 flex flex-col gap-1.5 border-l border-indigo-500/20 pl-3",
                    for (index, step) in steps.into_iter().enumerate() {
                        li { key: "{index}", class: "flex items-start gap-2 text-xs",
                            span { class: plan_glyph_class(&step.status), "{plan_glyph(&step.status)}" }
                            span { class: plan_text_class(&step.status), "{step.content}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn DiffBlock(
    path: String,
    old_text: Option<String>,
    new_text: String,
    #[props(default)] icon: Option<Element>,
) -> Element {
    let old = old_text.as_deref().unwrap_or("");
    let lines = similar::TextDiff::from_lines(old, &new_text)
        .iter_all_changes()
        .filter_map(|change| match change.tag() {
            similar::ChangeTag::Delete => Some((
                format!("- {}", change.value().trim_end_matches('\n')),
                "bg-red-500/10 text-red-300",
            )),
            similar::ChangeTag::Insert => Some((
                format!("+ {}", change.value().trim_end_matches('\n')),
                "bg-emerald-500/10 text-emerald-300",
            )),
            similar::ChangeTag::Equal => None,
        })
        .collect::<Vec<_>>();
    let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
    rsx! {
        ActivityRow { kind: language_kind(&path).unwrap_or(ActivityKind::Diff), icon,
            details { class: "min-w-0 text-sm text-[var(--muted-foreground,#a1a1aa)]",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "Edited" }
                    code { class: "truncate font-mono text-xs text-[var(--foreground,#f4f4f5)]/70", "{filename}" }
                    DisclosureIcon {}
                }
                div { class: "mt-2 overflow-hidden rounded-lg ring-1 ring-inset ring-zinc-500/15",
                    div { class: "overflow-x-auto bg-zinc-500/[0.025] py-1 font-mono text-[11px] leading-relaxed",
                        for (index, (line, class)) in lines.into_iter().enumerate() {
                            div { key: "{index}", class: "px-3 {class}", "{line}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ToolResultBlock(
    content: String,
    is_error: bool,
    #[props(default)] compact: bool,
    #[props(default)] icon: Option<Element>,
) -> Element {
    let label = if is_error { "Error" } else { "Output" };
    let tone = if is_error {
        "text-red-300"
    } else {
        "text-teal-300/80"
    };
    let panel = if is_error {
        "bg-red-500/[0.045] ring-red-500/15"
    } else {
        "bg-teal-500/[0.035] ring-teal-500/10"
    };
    let detail = rsx! {
        details { class: "min-w-0 text-sm {tone}",
            summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium", "{label}" }
                DisclosureIcon {}
            }
            pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-lg p-2 font-mono text-[11px] text-[var(--muted-foreground,#a1a1aa)] ring-1 ring-inset {panel}", "{content}" }
        }
    };
    if compact {
        detail
    } else {
        rsx! {
            ActivityRow { kind: if is_error { ActivityKind::Error } else { ActivityKind::Output }, icon,
                {detail}
            }
        }
    }
}

#[component]
pub fn ReconnectBlock(
    attempt: u32,
    total: u32,
    #[props(default)] icon: Option<Element>,
) -> Element {
    rsx! {
        ActivityRow { kind: ActivityKind::Reconnect, icon,
            span { class: "font-medium tabular-nums text-[var(--muted-foreground,#a1a1aa)]", "Reconnecting {attempt}/{total}" }
        }
    }
}

#[component]
pub fn WorkingBlock(verb: String, elapsed: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 px-1 text-sm text-muted-foreground",
            span { class: "agent-working-label font-medium", "{verb}" }
            span { class: "flex items-end gap-0.5 text-[color:var(--agent-accent)]",
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current" }
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current [animation-delay:120ms]" }
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current [animation-delay:240ms]" }
            }
            span { class: "tabular-nums text-xs", "{elapsed}" }
        }
    }
}

#[component]
pub fn TurnMeta(label: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 px-1 text-sm text-muted-foreground/70",
            span { class: "h-1.5 w-1.5 rounded-full bg-[color:var(--agent-accent)]" }
            span { class: "tabular-nums", "{label}" }
        }
    }
}

#[component]
fn ActivityRow(
    kind: ActivityKind,
    #[props(default)] icon: Option<Element>,
    #[props(default)] tone: &'static str,
    children: Element,
) -> Element {
    rsx! {
        div { class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-zinc-500/[0.035] {tone}",
            if let Some(icon) = icon {
                {icon}
            } else {
                ActivityIcon { kind }
            }
            {children}
        }
    }
}

#[component]
fn ActivityIcon(kind: ActivityKind) -> Element {
    if kind == ActivityKind::Thinking {
        return rsx! {
            span { class: "flex h-6 w-6 shrink-0 items-center justify-center text-[17px] leading-none", aria_hidden: "true", "🧠" }
        };
    }
    if kind == ActivityKind::Python {
        return rsx! {
            span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-sky-500/10 ring-1 ring-inset ring-sky-500/20", aria_hidden: "true",
                svg { class: "h-[17px] w-[17px]", view_box: "0 0 24 24",
                    path { fill: "#3776ab", d: "M11.7 2C7 2 7.3 4 7.3 4v2.1h4.5V7H5.5S2 6.6 2 12.2s3.1 5.4 3.1 5.4h1.8v-2.5s-.1-3 2.9-3h4.7s2.7 0 2.7-2.7V4.8S17.6 2 11.7 2Zm-2.5 1.5a.8.8 0 1 1 0 1.6.8.8 0 0 1 0-1.6Z" }
                    path { fill: "#ffd43b", d: "M12.3 22c4.7 0 4.4-2 4.4-2v-2.1h-4.5V17h6.3s3.5.4 3.5-5.2-3.1-5.4-3.1-5.4h-1.8v2.5s.1 3-2.9 3H9.5s-2.7 0-2.7 2.7v4.6S6.4 22 12.3 22Zm2.5-1.5a.8.8 0 1 1 0-1.6.8.8 0 0 1 0 1.6Z" }
                }
            }
        };
    }
    let tone = activity_tone(kind);
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            svg { class: "h-4 w-4", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                for path in activity_paths(kind) {
                    path { d: "{path}" }
                }
            }
        }
    }
}

#[component]
fn DisclosureIcon() -> Element {
    rsx! {
        svg { class: "h-3 w-3 shrink-0 transition-transform [[open]>&]:rotate-90", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "m9 18 6-6-6-6" }
        }
    }
}

#[component]
fn CodePanel(value: String) -> Element {
    rsx! {
        pre { class: "mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-zinc-500/[0.045] p-2 font-mono text-[11px] text-[var(--muted-foreground,#a1a1aa)] ring-1 ring-inset ring-zinc-500/15", "{value}" }
    }
}

fn markdown_html(markdown: &str) -> String {
    use pulldown_cmark::Event;

    let parser = pulldown_cmark::Parser::new_ext(
        markdown,
        pulldown_cmark::Options::ENABLE_STRIKETHROUGH | pulldown_cmark::Options::ENABLE_TABLES,
    )
    .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

fn tool_presentation(name: &str, args: &str) -> (ActivityKind, String) {
    let lower = name.to_ascii_lowercase();
    let (kind, label) = if lower.contains("guardian") || lower.contains("review") {
        (ActivityKind::Guardian, "Guardian Review".to_string())
    } else if lower.contains("read") || lower.contains("file") {
        (ActivityKind::ReadFile, "Read files".to_string())
    } else if lower.contains("image") || lower.contains("screenshot") {
        (ActivityKind::Image, "Viewed image".to_string())
    } else if lower.contains("browser") || lower.contains("navigate") {
        (ActivityKind::Browser, "Used browser".to_string())
    } else if lower.contains("search") || lower.contains("find") || lower.contains("grep") {
        (ActivityKind::Search, "Searched files".to_string())
    } else if lower.contains("command")
        || lower.contains("shell")
        || lower.contains("exec")
        || lower.contains("terminal")
    {
        (ActivityKind::Command, "Ran commands".to_string())
    } else {
        (
            ActivityKind::Tool,
            name.rsplit(['.', ':'])
                .next()
                .unwrap_or(name)
                .replace('_', " "),
        )
    };
    (
        language_kind(args)
            .or_else(|| language_kind(name))
            .unwrap_or(kind),
        label,
    )
}

fn language_kind(value: &str) -> Option<ActivityKind> {
    let lower = value.to_ascii_lowercase();
    (lower.contains(".py") || lower == "py" || lower.contains("python"))
        .then_some(ActivityKind::Python)
}

fn status_label(status: &str) -> &'static str {
    match status {
        "in_progress" => "Running",
        "completed" => "Done",
        "failed" => "Failed",
        _ => "Pending",
    }
}

fn status_class(status: &str) -> &'static str {
    match status {
        "in_progress" => "bg-violet-500/10 text-violet-300",
        "completed" => "bg-emerald-500/10 text-emerald-300",
        "failed" => "bg-red-500/10 text-red-300",
        _ => "bg-amber-500/10 text-amber-300",
    }
}

fn plan_glyph(status: &str) -> &'static str {
    match status {
        "completed" => "✓",
        "in_progress" => "●",
        _ => "○",
    }
}

fn plan_glyph_class(status: &str) -> &'static str {
    match status {
        "completed" => "mt-px text-emerald-400",
        "in_progress" => "mt-px text-violet-300",
        _ => "mt-px text-zinc-500",
    }
}

fn plan_text_class(status: &str) -> &'static str {
    match status {
        "completed" => "text-[var(--muted-foreground,#a1a1aa)] line-through opacity-70",
        "in_progress" => "text-[var(--foreground,#f4f4f5)]",
        _ => "text-[var(--muted-foreground,#a1a1aa)]",
    }
}

fn activity_tone(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Thinking | ActivityKind::Python => unreachable!(),
        ActivityKind::ReadFile => "bg-sky-500/10 text-sky-300 ring-sky-500/20",
        ActivityKind::Search => "bg-cyan-500/10 text-cyan-300 ring-cyan-500/20",
        ActivityKind::Image => "bg-pink-500/10 text-pink-300 ring-pink-500/20",
        ActivityKind::Command | ActivityKind::Reconnect => {
            "bg-amber-500/10 text-amber-300 ring-amber-500/20"
        }
        ActivityKind::Browser => "bg-blue-500/10 text-blue-300 ring-blue-500/20",
        ActivityKind::Guardian => "bg-emerald-500/10 text-emerald-300 ring-emerald-500/20",
        ActivityKind::Subagent => "bg-violet-500/10 text-violet-300 ring-violet-500/20",
        ActivityKind::Tool => "bg-orange-500/10 text-orange-300 ring-orange-500/20",
        ActivityKind::Output => "bg-teal-500/10 text-teal-300 ring-teal-500/20",
        ActivityKind::Error => "bg-red-500/10 text-red-300 ring-red-500/20",
        ActivityKind::Plan => "bg-indigo-500/10 text-indigo-300 ring-indigo-500/20",
        ActivityKind::Diff => "bg-green-500/10 text-green-300 ring-green-500/20",
    }
}

fn activity_paths(kind: ActivityKind) -> &'static [&'static str] {
    match kind {
        ActivityKind::Thinking | ActivityKind::Python => &[],
        ActivityKind::ReadFile => &[
            "M12 7v14",
            "M3 18a1 1 0 0 1-1-1V5a2 2 0 0 1 2-2h5a3 3 0 0 1 3 3v15a3 3 0 0 0-3-3Z",
            "M21 18a1 1 0 0 0 1-1V5a2 2 0 0 0-2-2h-5a3 3 0 0 0-3 3v15a3 3 0 0 1 3-3Z",
        ],
        ActivityKind::Search => &["M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z", "m21 21-4.35-4.35"],
        ActivityKind::Image => &[
            "M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z",
            "M10.5 8.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z",
            "m21 15-5-5L5 21",
        ],
        ActivityKind::Command => &["m4 17 6-6-6-6", "M12 19h8"],
        ActivityKind::Browser => &[
            "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z",
            "M2 12h20",
            "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z",
        ],
        ActivityKind::Guardian => &[
            "M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z",
            "m9 12 2 2 4-4",
        ],
        ActivityKind::Subagent => &[
            "M12 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
            "M5 21v-2a7 7 0 0 1 14 0v2",
            "M5.5 11a2.5 2.5 0 1 0 0-5",
            "M18.5 11a2.5 2.5 0 1 1 0-5",
        ],
        ActivityKind::Tool => &[
            "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76Z",
        ],
        ActivityKind::Output => &[
            "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5Z",
            "M14 2v6h6",
            "m10 17 3-3-3-3",
            "M13 14H7",
        ],
        ActivityKind::Error => &[
            "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z",
            "M12 8v4",
            "M12 16h.01",
        ],
        ActivityKind::Plan => &[
            "M4 19.5A2.5 2.5 0 0 1 6.5 17H20",
            "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z",
        ],
        ActivityKind::Diff => &[
            "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",
            "M14 2v4a2 2 0 0 0 2 2h4",
        ],
        ActivityKind::Reconnect => &[
            "M5 12.55a11 11 0 0 1 14.08 0",
            "M1.42 9a16 16 0 0 1 21.16 0",
            "M8.53 16.11a6 6 0 0 1 6.95 0",
            "M12 20h.01",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_drops_raw_html() {
        let html = markdown_html("**safe**<script>alert('x')</script>");
        assert!(html.contains("<strong>safe</strong>"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn tool_labels_are_shared() {
        assert_eq!(tool_presentation("read_file", "{}").1, "Read files");
        assert_eq!(tool_presentation("exec_command", "{}").1, "Ran commands");
    }
}
