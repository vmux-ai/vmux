use dioxus::prelude::*;
use vmux_api::chat::ChatActivityKind;
use vmux_ui::file_icon::{FileIcon, FilePath, TypeIcon};
use vmux_ui::icon::LineIconView;

use crate::activity::ActivityIcon;

#[component]
pub(super) fn ActivityIconView(kind: ActivityIcon) -> Element {
    if kind == ActivityIcon::Thinking {
        return rsx! {
            span { class: "flex h-6 w-6 shrink-0 items-center justify-center text-[17px] leading-none", aria_hidden: "true", "🧠" }
        };
    }
    if kind == ActivityIcon::Python {
        return rsx! {
            span { class: "python-activity-icon flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset", aria_hidden: "true",
                svg {
                    class: "h-[17px] w-[17px]",
                    view_box: "0 0 24 24",
                    path {
                        fill: "#3776ab",
                        d: "M11.7 2C7 2 7.3 4 7.3 4v2.1h4.5V7H5.5S2 6.6 2 12.2s3.1 5.4 3.1 5.4h1.8v-2.5s-.1-3 2.9-3h4.7s2.7 0 2.7-2.7V4.8S17.6 2 11.7 2Zm-2.5 1.5a.8.8 0 1 1 0 1.6.8.8 0 0 1 0-1.6Z",
                    }
                    path {
                        fill: "#ffd43b",
                        d: "M12.3 22c4.7 0 4.4-2 4.4-2v-2.1h-4.5V17h6.3s3.5.4 3.5-5.2-3.1-5.4-3.1-5.4h-1.8v2.5s.1 3-2.9 3H9.5s-2.7 0-2.7 2.7v4.6S6.4 22 12.3 22Zm2.5-1.5a.8.8 0 1 1 0-1.6.8.8 0 0 1 0 1.6Z",
                    }
                }
            }
        };
    }
    let tone = match kind {
        ActivityIcon::Thinking
        | ActivityIcon::Writing
        | ActivityIcon::Installing
        | ActivityIcon::Awaiting => "agent-themed-activity",
        ActivityIcon::Python => unreachable!(),
        ActivityIcon::ReadFile => "bg-sky-500/10 text-sky-600 ring-sky-500/20 dark:text-sky-300",
        ActivityIcon::WriteFile => {
            "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
        }
        ActivityIcon::Layout => {
            "bg-violet-500/10 text-violet-600 ring-violet-500/20 dark:text-violet-300"
        }
        ActivityIcon::Worktree => {
            "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
        }
        ActivityIcon::Search => "bg-cyan-500/10 text-cyan-600 ring-cyan-500/20 dark:text-cyan-300",
        ActivityIcon::Image => "bg-pink-500/10 text-pink-600 ring-pink-500/20 dark:text-pink-300",
        ActivityIcon::Screenshot => {
            "bg-fuchsia-500/10 text-fuchsia-600 ring-fuchsia-500/20 dark:text-fuchsia-300"
        }
        ActivityIcon::OpenPage => {
            "bg-blue-500/10 text-blue-600 ring-blue-500/20 dark:text-blue-300"
        }
        ActivityIcon::Command => {
            "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300"
        }
        ActivityIcon::Browser => "bg-blue-500/10 text-blue-600 ring-blue-500/20 dark:text-blue-300",
        ActivityIcon::Guardian => {
            "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
        }
        ActivityIcon::Subagent => {
            "bg-violet-500/10 text-violet-600 ring-violet-500/20 dark:text-violet-300"
        }
        ActivityIcon::Tool => {
            "bg-orange-500/10 text-orange-600 ring-orange-500/20 dark:text-orange-300"
        }
        ActivityIcon::Output => "bg-teal-500/10 text-teal-600 ring-teal-500/20 dark:text-teal-300",
        ActivityIcon::Error => "bg-red-500/10 text-red-600 ring-red-500/20 dark:text-red-300",
        ActivityIcon::Plan => {
            "bg-indigo-500/10 text-indigo-600 ring-indigo-500/20 dark:text-indigo-300"
        }
        ActivityIcon::Diff => {
            "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
        }
        ActivityIcon::Reconnect => {
            "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300"
        }
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            if let Some(icon) = kind.line_icon() {
                LineIconView { icon, class: "h-4 w-4" }
            }
        }
    }
}

#[component]
pub(super) fn FileActivityIcon(path: String, write: bool) -> Element {
    let tone = if write {
        "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
    } else {
        "bg-sky-500/10 text-sky-600 ring-sky-500/20 dark:text-sky-300"
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            TypeIcon { path, is_dir: false, class: "h-4 w-4" }
        }
    }
}

#[component]
pub(super) fn ToolActivityIcon(
    name: String,
    file_path: Option<String>,
    activity: ChatActivityKind,
) -> Element {
    if matches!(
        activity,
        ChatActivityKind::ReadFile | ChatActivityKind::WriteFile | ChatActivityKind::Tool
    ) && let Some(path) = file_path
    {
        let write = activity == ChatActivityKind::WriteFile;
        return rsx! { FileActivityIcon { path, write } };
    }
    if matches!(FilePath(&name).icon(false), FileIcon::Logo(_)) {
        return rsx! { FileActivityIcon { path: name, write: false } };
    }
    rsx! { ActivityIconView { kind: ActivityIcon::from(activity) } }
}
