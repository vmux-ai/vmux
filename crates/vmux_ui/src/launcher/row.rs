use crate::components::icon::Icon;
use crate::favicon::Favicon;
use crate::file_icon::FilePath;
use crate::i18n::{TranslationValue, translate, translate_with};
use crate::icon::PageIconView;
use dioxus::prelude::*;
use vmux_wire::command_bar::looks_like_url;

use crate::launcher::results::CommandBarResultItem as ResultItem;
use crate::launcher::results::{prompt_target_matches_query, prompt_target_url};
use crate::launcher::style::{
    result_content_row_class, result_favicon_class, result_history_url_class, result_item_class,
    result_leading_icon_class, result_location_class, result_primary_text_class,
    result_secondary_text_class, result_shortcut_badge_class, result_terminal_path_class,
    result_trailing_slot_class,
};

#[component]
pub fn ResultRow(
    index: usize,
    item: ResultItem,
    selected: bool,
    #[props(default)] space_switch: bool,
    #[props(default)] start_prompt_mode: bool,
    #[props(default)] query: String,
    on_activate: EventHandler<()>,
    on_hover: EventHandler<()>,
) -> Element {
    let i = index;
    let q = query.as_str();
    rsx! {
        div {
            id: "command-bar-item-{index}",
            class: result_item_class(selected),
            onclick: move |_| on_activate.call(()),
            onmouseenter: move |_| on_hover.call(()),
            match &item {
                            ResultItem::Terminal { path } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground", ">_" }
                                    if path.is_empty() {
                                        span { class: "text-sm text-foreground", {translate("command-terminal")} }
                                    } else {
                                        span { class: "shrink-0 text-sm text-foreground", {translate("command-open-terminal")} }
                                        span { class: result_terminal_path_class(), "{path}" }
                                    }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::Editor { path } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground", "\u{2261}" }
                                    span { class: "shrink-0 text-sm text-foreground", {translate("command-open-editor")} }
                                    span { class: result_terminal_path_class(), "{path}" }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::Stack { title, url, icon, location, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    PageIconView {
                                        icon: icon.clone(),
                                        url: url.clone(),
                                        img_class: result_favicon_class().to_string(),
                                        icon_class: result_leading_icon_class().to_string(),
                                    }
                                    div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                        span { class: result_primary_text_class(), "{title}" }
                                        span { class: result_secondary_text_class(), "{url}" }
                                    }
                                }
                                span {
                                    class: result_location_class(),
                                    title: "{location}",
                                    if location.is_empty() { {translate("command-stack")} } else { "{location}" }
                                }
                            },
                            ResultItem::Space { name, profile, is_active, tab_count, .. } => rsx! {
                                if space_switch {
                                    span { class: "w-5 shrink-0 text-center font-mono text-xs text-muted-foreground", "{i}" }
                                }
                                div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                    div { class: "flex min-w-0 items-center gap-2",
                                        span { class: result_primary_text_class(), "{name}" }
                                        if *is_active {
                                            span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300", {translate("common-active")} }
                                        }
                                    }
                                    span { class: result_secondary_text_class(), "{profile}" }
                                }
                                span { class: result_trailing_slot_class(), {translate_with("command-tabs", &[("count", TranslationValue::Number(*tab_count as i64))])} }
                            },
                            ResultItem::Command { name, shortcut, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground", ">_" }
                                    span { class: result_primary_text_class(), "{name}" }
                                }
                                span { class: result_trailing_slot_class(),
                                    if !shortcut.is_empty() {
                                        span { class: result_shortcut_badge_class(), "{shortcut}" }
                                    }
                                }
                            },
                            ResultItem::Ex { name, hint } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 font-mono text-sm text-muted-foreground", ":" }
                                    span { class: "shrink-0 font-mono text-sm text-foreground", "{name}" }
                                    span { class: "{result_secondary_text_class()} min-w-0 truncate", "{hint}" }
                                }
                                span { class: result_trailing_slot_class(), "\u{21b5}" }
                            },
                            ResultItem::History { url, title, favicon_url, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    Favicon {
                                        favicon_url: favicon_url.clone(),
                                        url: url.clone(),
                                        class: result_favicon_class().to_string(),
                                        globe_class: result_leading_icon_class().to_string(),
                                    }
                                    span { class: "min-w-0 flex-1 truncate text-sm text-foreground",
                                        if title.is_empty() { "{url}" } else { "{title}" }
                                    }
                                    span { class: result_history_url_class(), "{url}" }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::Page { url, title, icon, shortcut, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    PageIconView {
                                        icon: icon.clone(),
                                        url: url.clone(),
                                        img_class: result_favicon_class().to_string(),
                                        icon_class: result_leading_icon_class().to_string(),
                                    }
                                    div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                        if start_prompt_mode
                                            && prompt_target_url(&item).is_some()
                                            && !prompt_target_matches_query(&item, q)
                                        {
                                            span { class: result_primary_text_class(), "Ask {title}" }
                                        } else {
                                            span { class: result_primary_text_class(), "{title}" }
                                            span { class: result_secondary_text_class(), "{url}" }
                                        }
                                    }
                                }
                                span { class: result_trailing_slot_class(),
                                    if start_prompt_mode
                                        && prompt_target_url(&item).is_some()
                                        && !prompt_target_matches_query(&item, q)
                                    {
                                        {translate("command-prompt")}
                                    } else if shortcut.is_empty() {
                                        {translate("command-new-tab")}
                                    } else {
                                        span { class: result_shortcut_badge_class(), "{shortcut}" }
                                    }
                                }
                            },
                            ResultItem::Navigate { url } => rsx! {
                                div { class: result_content_row_class(),
                                    Icon { class: result_leading_icon_class(),
                                        circle { cx: "11", cy: "11", r: "8" }
                                        path { d: "m21 21-4.3-4.3" }
                                    }
                                    if url.is_empty() {
                                        span { class: "text-sm text-foreground", {translate("command-search")} }
                                    } else if looks_like_url(url) {
                                        span { class: result_primary_text_class(), {translate_with("command-open-value", &[("value", TranslationValue::String(url))])} }
                                    } else {
                                        span { class: result_primary_text_class(), {translate_with("command-search-value", &[("value", TranslationValue::String(url))])} }
                                    }
                                }
                                if !url.is_empty() {
                                    span { class: result_trailing_slot_class(), "\u{21b5}" }
                                } else {
                                    span { class: result_trailing_slot_class() }
                                }
                            },
                            ResultItem::Search { engine, query } => rsx! {
                                div { class: result_content_row_class(),
                                    Favicon {
                                        favicon_url: String::new(),
                                        url: engine.search_url(query),
                                        class: result_favicon_class().to_string(),
                                        globe_class: result_leading_icon_class().to_string(),
                                    }
                                    span { class: result_primary_text_class(), "Search with {engine.name()}" }
                                }
                                span { class: result_trailing_slot_class(), "\u{21b5}" }
                            },
                            ResultItem::File { path, is_dir, project, relative } => {
                                let name = FilePath(path).name();
                                let location = FileLocation::of(project, relative, path);
                                rsx! {
                                    div { class: result_content_row_class(),
                                        if *is_dir {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                            }
                                        } else {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                                path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                            }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            div { class: "flex min-w-0 items-center gap-1.5",
                                                if !project.is_empty() {
                                                    span { class: "{result_shortcut_badge_class()} shrink-0", "{project}" }
                                                }
                                                span { class: "{result_secondary_text_class()} min-w-0 truncate", "{location}" }
                                            }
                                        }
                                    }
                                    if *is_dir {
                                        span { class: result_trailing_slot_class() }
                                    } else {
                                        span { class: result_trailing_slot_class(), "\u{21b5}" }
                                    }
                                }
                            },
                            ResultItem::WorkDir { path, is_dir } => {
                                let name = FilePath(path).name();
                                rsx! {
                                    div { class: result_content_row_class(),
                                        if *is_dir {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                            }
                                        } else {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                                path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                            }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{path}" }
                                        }
                                    }
                                    if *is_dir {
                                        span { class: result_trailing_slot_class() }
                                    } else {
                                        span { class: result_trailing_slot_class(), "\u{21b5}" }
                                    }
                                }
                            },
                            ResultItem::PartialIndex => rsx! {
                                div { class: result_content_row_class(),
                                    Icon { class: result_leading_icon_class(),
                                        circle { cx: "12", cy: "12", r: "10" }
                                        path { d: "M12 8v4" }
                                        path { d: "M12 16h.01" }
                                    }
                                    span { class: result_secondary_text_class(), {translate("command-partial-index")} }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::MoreMatches { shown, total } => rsx! {
                                div { class: result_content_row_class(),
                                    Icon { class: result_leading_icon_class(),
                                        circle { cx: "12", cy: "12", r: "10" }
                                        path { d: "M8 12h8" }
                                    }
                                    span { class: result_secondary_text_class(),
                                        {translate_with(
                                            "command-more-matches",
                                            &[
                                                ("shown", TranslationValue::Number(*shown as i64)),
                                                ("total", TranslationValue::Number(*total as i64)),
                                            ],
                                        )}
                                    }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::RecentFile { url, title } => {
                                let display = url.strip_prefix("file://").unwrap_or(url.as_str()).to_string();
                                let name = if title.is_empty() {
                                    FilePath(&display).name().to_string()
                                } else {
                                    title.clone()
                                };
                                rsx! {
                                    div { class: result_content_row_class(),
                                        Icon { class: result_leading_icon_class(),
                                            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{display}" }
                                        }
                                    }
                                    span { class: result_trailing_slot_class(), "\u{21b5}" }
                                }
                            },
            }
        }
    }
}

struct FileLocation;

impl FileLocation {
    fn of(project: &str, relative: &str, path: &str) -> String {
        let shown = match project.is_empty() {
            true => path,
            false => relative,
        };
        let Some((dir, _)) = shown.trim_end_matches('/').rsplit_once('/') else {
            return String::new();
        };
        dir.to_string()
    }
}
