use crate::components::composer_bar::{ComposerChipIcon, ComposerMenuKind};
use crate::components::icon::Icon;
use crate::components::skeleton::Skeleton;
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
use crate::util::cn;

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
    let resume_section = match &item {
        ResultItem::Resume { section, .. } => section.clone(),
        _ => None,
    };
    rsx! {
        if let Some(section) = resume_section {
            ResumeSectionRow { section }
        }
        div {
            id: "command-bar-item-{index}",
            class: result_item_class(selected),
            onclick: move |_| on_activate.call(()),
            onmouseenter: move |_| on_hover.call(()),
            match &item {
                            ResultItem::Pick { label, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: result_primary_text_class(), "{label}" }
                                }
                                span { class: result_trailing_slot_class(), "\u{21b5}" }
                            },
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
                            ResultItem::Slash { name, hint } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 font-mono text-sm text-muted-foreground", "/" }
                                    span { class: "shrink-0 font-mono text-sm text-foreground", "{name}" }
                                    span { class: "{result_secondary_text_class()} min-w-0 truncate", "{hint}" }
                                }
                                span { class: result_trailing_slot_class(), "\u{21b5}" }
                            },
                            ResultItem::Resume { entry, .. } => {
                                let preview = ResumePreview::after_title(&entry.title, &entry.latest);
                                rsx! {
                                    div { class: result_content_row_class(),
                                        span { class: "shrink-0 text-sm text-muted-foreground", "\u{21ba}" }
                                        div { class: "flex min-w-0 flex-1 flex-col gap-0.5",
                                            span { class: "min-w-0 truncate text-sm text-foreground", "{entry.title}" }
                                            if let Some(preview) = preview {
                                                span { class: "{result_secondary_text_class()} min-w-0 truncate", "{preview}" }
                                            }
                                        }
                                        span { class: "ml-3 w-20 shrink-0 text-right font-mono text-xs tabular-nums text-muted-foreground/75",
                                            {SessionWhen::new(entry.age_seconds, &entry.updated_at).label()}
                                        }
                                    }
                                    span { class: "ml-3 flex h-5 w-5 shrink-0 items-center justify-end text-xs text-muted-foreground", "\u{21b5}" }
                                }
                            },
                            ResultItem::ResumePending { row } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground/40", "\u{21ba}" }
                                    div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                                        Skeleton { class: cn(["h-3 bg-muted-foreground/20", SkeletonWidth::title(*row)]) }
                                        Skeleton { class: cn(["h-2.5 bg-muted-foreground/10", SkeletonWidth::latest(*row)]) }
                                    }
                                }
                                span { class: result_trailing_slot_class() }
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
                                let location = FileLocation::resolve(project, relative, path);
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

struct ResumePreview;

impl ResumePreview {
    fn after_title<'a>(title: &str, latest: &'a str) -> Option<&'a str> {
        let title = title.trim();
        let latest = latest.trim();
        if latest.is_empty() || latest == title {
            return None;
        }
        if title.is_empty() {
            return Some(latest);
        }
        let Some(remainder) = latest.strip_prefix(title) else {
            return Some(latest);
        };
        let remainder = remainder.trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '.' | ':' | '-' | '\u{2014}' | '\u{00b7}' | '|')
        });
        (!remainder.is_empty()).then_some(remainder)
    }
}

#[component]
fn ResumeSectionRow(section: crate::launcher::results::ResumeSection) -> Element {
    if section.agent.is_empty() && section.project.is_empty() && section.branch.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "sticky top-0 z-10 flex min-w-0 items-center gap-1.5 border-y border-foreground/[0.06] bg-background/95 px-4 py-1.5 backdrop-blur",
            if !section.agent.is_empty() {
                ResumeSectionPart {
                    kind: ComposerMenuKind::Agent,
                    label: section.agent,
                }
            }
            if !section.project.is_empty() {
                ResumeSectionPart {
                    kind: ComposerMenuKind::Project,
                    label: section.project,
                }
            }
            if !section.branch.is_empty() {
                ResumeSectionPart {
                    kind: ComposerMenuKind::Branch,
                    label: section.branch,
                }
            }
            span { class: "ml-auto flex h-5 min-w-5 shrink-0 items-center justify-center rounded-full bg-foreground/[0.055] px-1.5 font-mono text-[10px] tabular-nums text-muted-foreground/55", "{section.count}" }
        }
    }
}

#[component]
fn ResumeSectionPart(kind: ComposerMenuKind, label: String) -> Element {
    let label_class = match kind {
        ComposerMenuKind::Branch => "font-mono text-[10px]",
        _ => "text-[11px]",
    };
    rsx! {
        span {
            class: "flex h-6 min-w-0 items-center gap-1.5 rounded-md bg-foreground/[0.045] px-2 text-muted-foreground/75",
            title: "{label}",
            ComposerChipIcon { kind }
            span { class: "min-w-0 truncate {label_class}", "{label}" }
        }
    }
}

struct SessionWhen<'a> {
    age_seconds: u64,
    updated_at: &'a str,
}

impl<'a> SessionWhen<'a> {
    fn new(age_seconds: u64, updated_at: &'a str) -> Self {
        Self {
            age_seconds,
            updated_at,
        }
    }

    fn label(self) -> String {
        if let Some(date) = self.date() {
            return date.to_string();
        }
        SessionAge(self.age_seconds).label()
    }

    fn date(&self) -> Option<&str> {
        if self.age_seconds < SessionAge::DAY || self.updated_at.is_empty() {
            return None;
        }
        Some(
            self.updated_at
                .split_whitespace()
                .next()
                .unwrap_or(self.updated_at),
        )
    }
}

pub struct SessionAge(pub u64);

impl SessionAge {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * Self::MINUTE;
    pub const DAY: u64 = 24 * Self::HOUR;
    const WEEK: u64 = 7 * Self::DAY;
    const MONTH: u64 = 30 * Self::DAY;
    const YEAR: u64 = 365 * Self::DAY;

    pub fn label(self) -> String {
        let (id, count) = self.unit();
        if count == 0 {
            return translate(id);
        }
        translate_with(id, &[("count", TranslationValue::Number(count as i64))])
    }

    fn unit(self) -> (&'static str, u64) {
        let seconds = self.0;
        if seconds < Self::MINUTE {
            return ("resume-age-now", 0);
        }
        if seconds < Self::HOUR {
            return ("resume-age-minutes", seconds / Self::MINUTE);
        }
        if seconds < Self::DAY {
            return ("resume-age-hours", seconds / Self::HOUR);
        }
        if seconds < Self::WEEK {
            return ("resume-age-days", seconds / Self::DAY);
        }
        if seconds < Self::MONTH {
            return ("resume-age-weeks", seconds / Self::WEEK);
        }
        if seconds < Self::YEAR {
            return ("resume-age-months", seconds / Self::MONTH);
        }
        ("resume-age-years", seconds / Self::YEAR)
    }
}

struct SkeletonWidth;

impl SkeletonWidth {
    const TITLE: [&'static str; 4] = ["w-2/5", "w-3/5", "w-1/2", "w-7/12"];
    const LATEST: [&'static str; 4] = ["w-3/4", "w-1/2", "w-5/6", "w-2/3"];

    fn title(row: usize) -> &'static str {
        Self::TITLE[row % Self::TITLE.len()]
    }

    fn latest(row: usize) -> &'static str {
        Self::LATEST[row % Self::LATEST.len()]
    }
}

struct FileLocation;

impl FileLocation {
    fn resolve(project: &str, relative: &str, path: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_preview_removes_the_title_from_the_latest_message() {
        assert_eq!(
            ResumePreview::after_title(
                "Assess the request.",
                "Assess the request. Treat tools carefully."
            ),
            Some("Treat tools carefully.")
        );
    }

    #[test]
    fn resume_preview_hides_an_exact_duplicate() {
        assert_eq!(
            ResumePreview::after_title("Assess the request.", "Assess the request."),
            None
        );
    }

    #[test]
    fn resume_preview_keeps_unrelated_latest_text() {
        assert_eq!(
            ResumePreview::after_title("Fix the layout", "Tests are passing"),
            Some("Tests are passing")
        );
    }

    #[test]
    fn recent_sessions_use_relative_time() {
        assert_eq!(
            SessionWhen::new(SessionAge::DAY - 1, "2026-09-06").date(),
            None
        );
    }

    #[test]
    fn older_sessions_use_the_calendar_date() {
        assert_eq!(
            SessionWhen::new(SessionAge::DAY, "2026-09-06 14:00").date(),
            Some("2026-09-06")
        );
    }
}
