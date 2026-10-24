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
    if let ActivityIcon::Language(path) = &kind {
        return rsx! { FileActivityIcon { path: path.clone(), write: false } };
    }
    let tone = if kind == ActivityIcon::Error {
        "text-red-500"
    } else {
        "text-muted-foreground/75"
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center {tone}", aria_hidden: "true",
            if let Some(icon) = kind.line_icon() {
                LineIconView { icon, class: "h-4 w-4" }
            }
        }
    }
}

#[component]
pub(super) fn FileActivityIcon(path: String, write: bool) -> Element {
    let _ = write;
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center opacity-75", aria_hidden: "true",
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
        &activity,
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
