use vmux_api::PageIcon;
use vmux_api::chat::SlashCommandEntry;
use vmux_api::command_bar::{
    CommandBarCommandEntry, CommandBarPage, CommandBarPick, CommandBarPickRow, CommandBarPicker,
    CommandBarTab, HistoryEntry, SearchEngine,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

use super::query::PaletteQuery;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandBarResultItem {
    Pick {
        label: String,
        pick: CommandBarPick,
    },
    Stack {
        title: String,
        url: String,
        icon: PageIcon,
        pane_id: u64,
        tab_index: usize,
        location: String,
    },
    Command {
        id: String,
        name: String,
        shortcut: String,
    },
    Page {
        url: String,
        title: String,
        icon: PageIcon,
        shortcut: String,
        prompt_target: bool,
        prompt_hint: bool,
    },
    Navigate {
        url: String,
        is_url: bool,
    },
    Search {
        engine: SearchEngine,
        query: String,
    },
    File {
        path: String,
        is_dir: bool,
        project: String,
        relative: String,
    },
    History {
        url: String,
        title: String,
        favicon_url: String,
        visit_count: u32,
        last_visited_at: i64,
    },
    Slash {
        name: String,
        hint: String,
    },
    PartialIndex,
    MoreMatches {
        shown: usize,
        total: usize,
    },
}

impl CommandBarResultItem {
    pub(crate) fn projection(&self) -> vmux_api::command_bar::CommandBarResultItem {
        let mut row = vmux_api::command_bar::CommandBarResultItem::default();
        match self {
            Self::Pick { label, .. } => {
                row.title.clone_from(label);
                row.trailing = "\u{21b5}".to_string();
            }
            Self::Stack {
                title,
                url,
                icon,
                location,
                ..
            } => {
                row.title.clone_from(title);
                row.subtitle.clone_from(url);
                row.url.clone_from(url);
                row.icon = icon.clone();
                row.trailing = if location.is_empty() {
                    translate("command-stack")
                } else {
                    location.clone()
                };
            }
            Self::Command { name, shortcut, .. } => {
                row.leading = ">_".to_string();
                row.title.clone_from(name);
                row.trailing.clone_from(shortcut);
            }
            Self::Page {
                url,
                title,
                icon,
                shortcut,
                prompt_hint,
                ..
            } => {
                row.title = if *prompt_hint {
                    format!("Ask {title}")
                } else {
                    title.clone()
                };
                if !*prompt_hint {
                    row.subtitle.clone_from(url);
                }
                row.url.clone_from(url);
                row.icon = icon.clone();
                row.trailing = if *prompt_hint {
                    translate("command-prompt")
                } else if shortcut.is_empty() {
                    translate("command-new-tab")
                } else {
                    shortcut.clone()
                };
            }
            Self::Navigate { url, is_url } => {
                row.leading = "\u{2315}".to_string();
                row.title = if url.is_empty() {
                    translate("command-search")
                } else if *is_url {
                    translate_with(
                        "command-open-value",
                        &[("value", TranslationValue::String(url))],
                    )
                } else {
                    translate_with(
                        "command-search-value",
                        &[("value", TranslationValue::String(url))],
                    )
                };
                row.url.clone_from(url);
                if !url.is_empty() {
                    row.trailing = "\u{21b5}".to_string();
                }
            }
            Self::Search { engine, query } => {
                row.title = format!("Search with {}", engine.name);
                row.url = engine.query_url(query);
                row.trailing = "\u{21b5}".to_string();
            }
            Self::File {
                path,
                is_dir,
                project,
                relative,
            } => {
                row.title = FileRow::name(path);
                row.subtitle = FileRow::location(project, relative, path);
                row.badge.clone_from(project);
                row.file_path.clone_from(path);
                row.directory = *is_dir;
                if !is_dir {
                    row.trailing = "\u{21b5}".to_string();
                }
            }
            Self::History {
                url,
                title,
                favicon_url,
                ..
            } => {
                row.title = if title.is_empty() {
                    url.clone()
                } else {
                    title.clone()
                };
                row.subtitle.clone_from(url);
                row.url.clone_from(url);
                row.favicon_url.clone_from(favicon_url);
            }
            Self::Slash { name, hint } => {
                row.leading = "/".to_string();
                row.title.clone_from(name);
                row.subtitle.clone_from(hint);
                row.trailing = "\u{21b5}".to_string();
            }
            Self::PartialIndex => {
                row.leading = "!".to_string();
                row.title = translate("command-partial-index");
                row.disabled = true;
            }
            Self::MoreMatches { shown, total } => {
                row.leading = "+".to_string();
                row.title = translate_with(
                    "command-more-matches",
                    &[
                        ("shown", TranslationValue::Number(*shown as i64)),
                        ("total", TranslationValue::Number(*total as i64)),
                    ],
                );
                row.disabled = true;
            }
        }
        row
    }
}

struct FileRow;

impl FileRow {
    fn name(path: &str) -> String {
        path.trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .to_string()
    }

    fn location(project: &str, relative: &str, path: &str) -> String {
        let shown = if project.is_empty() { path } else { relative };
        let Some((directory, _)) = shown.trim_end_matches('/').rsplit_once('/') else {
            return String::new();
        };
        directory.to_string()
    }
}

pub struct SlashRows;

impl SlashRows {
    pub fn for_query(query: &str, commands: &[SlashCommandEntry]) -> Vec<CommandBarResultItem> {
        let held = PaletteQuery::new(query);
        let (name, _) = match held.slash_token() {
            Some(parts) => parts,
            None if query.trim() == "/" => ("", ""),
            None => return Vec::new(),
        };
        let lowered = name.to_lowercase();
        let mut matching = Vec::new();
        for command in commands {
            if command.command.name().starts_with(&lowered) {
                matching.push(command);
            }
        }
        if commands
            .iter()
            .any(|command| command.command.name() == lowered && command.delegated)
        {
            return Vec::new();
        }
        let mut rows = Vec::new();
        for command in matching {
            rows.push(CommandBarResultItem::Slash {
                name: command.command.name().to_string(),
                hint: command.description.clone(),
            });
        }
        rows
    }
}

pub struct PickerRows;

impl PickerRows {
    pub fn filtered(
        typed: bool,
        picks: &[CommandBarPickRow],
        query: &str,
    ) -> Vec<CommandBarResultItem> {
        if typed {
            return Vec::new();
        }
        let needle = query.trim().to_lowercase();
        let mut rows = Vec::with_capacity(picks.len());
        for row in picks {
            if !needle.is_empty() && !row.label.to_lowercase().contains(&needle) {
                continue;
            }
            rows.push(CommandBarResultItem::Pick {
                label: row.label.clone(),
                pick: row.pick.clone(),
            });
        }
        rows
    }

    pub fn typed(picker: CommandBarPicker, input: &str) -> CommandBarPick {
        CommandBarPick::Typed {
            picker,
            value: input.trim().to_string(),
        }
    }
}

pub(super) struct PageRows;

impl PageRows {
    fn urls_match(a: &str, b: &str) -> bool {
        a == b || a.trim_end_matches('/') == b.trim_end_matches('/')
    }

    fn stack_icon_for(pages: &[CommandBarPage], url: &str) -> PageIcon {
        pages
            .iter()
            .find(|p| Self::urls_match(&p.url, url))
            .map(|p| p.icon.clone())
            .unwrap_or_default()
    }

    fn page_matches(page: &CommandBarPage, search_lower: &str) -> bool {
        search_lower.is_empty()
            || page.title.to_lowercase().contains(search_lower)
            || page.url.to_lowercase().contains(search_lower)
            || page
                .keywords
                .iter()
                .any(|k| k.to_lowercase().contains(search_lower))
    }

    fn page_results(pages: &[CommandBarPage], search_lower: &str) -> Vec<CommandBarResultItem> {
        let mut matched: Vec<&CommandBarPage> = pages
            .iter()
            .filter(|page| Self::page_matches(page, search_lower))
            .collect();
        matched.sort_by_key(|page| page.url.to_lowercase());
        matched
            .into_iter()
            .map(|page| CommandBarResultItem::Page {
                url: page.url.clone(),
                title: page.title.clone(),
                icon: page.icon.clone(),
                shortcut: page.shortcut.clone(),
                prompt_target: false,
                prompt_hint: false,
            })
            .collect()
    }

    pub fn prompt_targets(pages: &[CommandBarPage], query: &str) -> Vec<CommandBarResultItem> {
        let search_lower = query.trim().to_lowercase();
        let targets: Vec<_> = pages.iter().filter(|page| page.prompt_target).collect();
        let matches: Vec<_> = targets
            .iter()
            .copied()
            .filter(|page| Self::page_matches(page, &search_lower))
            .collect();
        let visible = if matches.is_empty() { targets } else { matches };
        visible
            .into_iter()
            .map(|page| CommandBarResultItem::Page {
                url: page.url.clone(),
                title: page.title.clone(),
                icon: page.icon.clone(),
                shortcut: page.shortcut.clone(),
                prompt_target: true,
                prompt_hint: false,
            })
            .collect()
    }

    pub fn prompt_target_url(item: &CommandBarResultItem) -> Option<&str> {
        match item {
            CommandBarResultItem::Page {
                url,
                prompt_target: true,
                ..
            } => Some(url),
            _ => None,
        }
    }

    pub fn prompt_target_matches(item: &CommandBarResultItem, query: &str) -> bool {
        let CommandBarResultItem::Page {
            url,
            title,
            prompt_target: true,
            ..
        } = item
        else {
            return false;
        };
        let search_lower = query.trim().to_lowercase();
        !search_lower.is_empty()
            && (title.to_lowercase().contains(&search_lower)
                || url.to_lowercase().contains(&search_lower))
    }
}

pub(super) struct StartRows;

impl StartRows {
    pub fn prepend_targets(
        results: &mut Vec<CommandBarResultItem>,
        selected_target: Option<&CommandBarResultItem>,
        recent_targets: &[CommandBarResultItem],
        query: &str,
    ) {
        if !PaletteQuery::new(query).is_start_prompt()
            || results
                .iter()
                .any(|item| PageRows::prompt_target_url(item).is_some())
        {
            return;
        }
        let mut suggestions = Vec::new();
        for target in selected_target.into_iter().chain(recent_targets) {
            let Some(url) = PageRows::prompt_target_url(target) else {
                continue;
            };
            if suggestions
                .iter()
                .any(|existing| PageRows::prompt_target_url(existing) == Some(url))
            {
                continue;
            }
            suggestions.push(target.clone());
            if suggestions.len() == 3 {
                break;
            }
        }
        let at = 0;
        let mut leading = Vec::new();
        let mut rest = Vec::new();
        for target in suggestions {
            match leading.is_empty() {
                true => leading.push(target),
                false => rest.push(target),
            }
        }
        let mut after = at + leading.len();
        results.splice(at..at, leading);
        for (index, item) in results.iter().enumerate() {
            if matches!(item, CommandBarResultItem::File { .. }) {
                after = index + 1;
            }
        }
        let tail = results.split_off(after);
        results.extend(rest);
        results.extend(tail);
    }
}

impl PageRows {
    pub fn open_sessions(
        tabs: &[CommandBarTab],
        pages: &[CommandBarPage],
    ) -> Vec<CommandBarResultItem> {
        tabs.iter()
            .filter(|tab| !tab.is_active)
            .map(|tab| CommandBarResultItem::Stack {
                title: tab.title.clone(),
                url: tab.url.clone(),
                icon: Self::stack_icon_for(pages, &tab.url),
                pane_id: tab.pane_id,
                tab_index: tab.tab_index as usize,
                location: tab.location.clone(),
            })
            .collect()
    }
}

impl StartRows {
    pub fn start(
        pages: &[CommandBarPage],
        search_engines: &[SearchEngine],
        query: &str,
    ) -> Vec<CommandBarResultItem> {
        let search_lower = query.trim().to_lowercase();
        let mut results = Vec::new();
        results.extend(
            PageRows::prompt_targets(pages, query)
                .into_iter()
                .filter(|item| PageRows::prompt_target_matches(item, query)),
        );
        let trimmed = query.trim();
        if PaletteQuery::new(trimmed).is_start_prompt() {
            results.extend(search_engines.iter().take(3).cloned().map(|engine| {
                CommandBarResultItem::Search {
                    engine,
                    query: trimmed.to_string(),
                }
            }));
        }
        let mut app_pages: Vec<_> = pages
            .iter()
            .filter(|page| !page.prompt_target && !page.startup)
            .filter(|page| PageRows::page_matches(page, &search_lower))
            .collect();
        app_pages.sort_by_cached_key(|page| page.url.to_lowercase());
        results.extend(
            app_pages
                .into_iter()
                .map(|page| CommandBarResultItem::Page {
                    url: page.url.clone(),
                    title: page.title.clone(),
                    icon: page.icon.clone(),
                    shortcut: page.shortcut.clone(),
                    prompt_target: false,
                    prompt_hint: false,
                }),
        );
        if !PaletteQuery::new(trimmed).is_start_prompt() && !trimmed.is_empty() {
            results.push(CommandBarResultItem::Navigate {
                url: trimmed.to_string(),
                is_url: PaletteQuery::new(trimmed).looks_like_url(),
            });
        }
        results
    }
}

pub(super) struct SearchRows;

#[derive(Default)]
pub(super) struct SearchRowsInput<'a> {
    pub(super) query: &'a str,
    pub(super) tabs: &'a [CommandBarTab],
    pub(super) commands: &'a [CommandBarCommandEntry],
    pub(super) pages: &'a [CommandBarPage],
    pub(super) history: &'a [HistoryEntry],
}

#[cfg(test)]
impl<'a> SearchRowsInput<'a> {
    fn for_pages(query: &'a str, pages: &'a [CommandBarPage]) -> Self {
        Self {
            query,
            pages,
            ..Self::default()
        }
    }
}

impl SearchRows {
    fn command_results(
        commands: &[CommandBarCommandEntry],
    ) -> impl Iterator<Item = CommandBarResultItem> + '_ {
        commands.iter().map(|c| CommandBarResultItem::Command {
            id: c.id.clone(),
            name: c.name.clone(),
            shortcut: c.shortcut.clone(),
        })
    }

    pub fn filter(input: SearchRowsInput<'_>) -> Vec<CommandBarResultItem> {
        let SearchRowsInput {
            query,
            tabs,
            commands,
            pages,
            history,
        } = input;
        let q = query.trim();

        if q.is_empty() {
            let mut items: Vec<CommandBarResultItem> = Vec::new();
            items.push(CommandBarResultItem::Navigate {
                url: String::new(),
                is_url: false,
            });
            items.extend(tabs.iter().filter(|t| !t.is_active).map(|t| {
                CommandBarResultItem::Stack {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    icon: PageRows::stack_icon_for(pages, &t.url),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index as usize,
                    location: t.location.clone(),
                }
            }));
            items.extend(PageRows::page_results(pages, ""));
            items.extend(Self::command_results(commands));
            return items;
        }

        let starts_with_cmd = q.starts_with('>');
        let search = if starts_with_cmd { q[1..].trim() } else { q };
        let search_lower = search.to_lowercase();

        let mut items = Vec::new();

        let is_path = PaletteQuery::new(search).looks_like_path();

        if starts_with_cmd {
            for c in commands {
                if search.is_empty()
                    || c.name.to_lowercase().contains(&search_lower)
                    || c.id.contains(&search_lower)
                {
                    items.push(CommandBarResultItem::Command {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        shortcut: c.shortcut.clone(),
                    });
                }
            }
        }

        if !starts_with_cmd && !is_path {
            items.extend(PageRows::page_results(pages, &search_lower));
        }

        if !starts_with_cmd || !search.is_empty() {
            for t in tabs {
                if t.is_active {
                    continue;
                }
                if search.is_empty()
                    || t.title.to_lowercase().contains(&search_lower)
                    || t.url.to_lowercase().contains(&search_lower)
                {
                    items.push(CommandBarResultItem::Stack {
                        title: t.title.clone(),
                        url: t.url.clone(),
                        icon: PageRows::stack_icon_for(pages, &t.url),
                        pane_id: t.pane_id,
                        tab_index: t.tab_index as usize,
                        location: t.location.clone(),
                    });
                }
            }
        }

        if !starts_with_cmd {
            for h in history.iter().take(5) {
                if h.url.starts_with("file://") {
                    continue;
                }
                items.push(CommandBarResultItem::History {
                    url: h.url.clone(),
                    title: h.title.clone(),
                    favicon_url: h.favicon_url.clone(),
                    visit_count: h.visit_count,
                    last_visited_at: h.last_visited_at,
                });
            }
        }

        if !starts_with_cmd {
            for c in commands {
                if c.name.to_lowercase().contains(&search_lower) || c.id.contains(&search_lower) {
                    items.push(CommandBarResultItem::Command {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        shortcut: c.shortcut.clone(),
                    });
                }
            }
        }

        if !search.is_empty() {
            items.push(CommandBarResultItem::Navigate {
                url: search.to_string(),
                is_url: PaletteQuery::new(search).looks_like_url(),
            });
        }

        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::{CommandBarCommandEntry, CommandBarTab};

    fn search_engine(id: &str) -> SearchEngine {
        SearchEngine {
            id: id.to_string(),
            name: id.to_string(),
            hosts: vec![format!("{id}.example")],
            query_url: format!("https://{id}.example/?q={{query}}"),
        }
    }

    fn sample_pages() -> Vec<CommandBarPage> {
        vec![
            CommandBarPage {
                url: "vmux://settings/".into(),
                title: "Settings".into(),
                keywords: vec!["preferences".into()],
                icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Settings),
                shortcut: String::new(),
                prompt_target: false,
                startup: false,
            },
            CommandBarPage {
                url: "vmux://spaces/".into(),
                title: "Spaces".into(),
                keywords: vec!["space".into()],
                icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Layers),
                shortcut: String::new(),
                prompt_target: false,
                startup: false,
            },
            CommandBarPage {
                url: "vmux://history/".into(),
                title: "History".into(),
                keywords: vec!["recent".into()],
                icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Clock),
                shortcut: "\u{2318}Y".into(),
                prompt_target: false,
                startup: false,
            },
            CommandBarPage {
                url: "vmux://sessions/vibe/".into(),
                title: "Vibe".into(),
                keywords: vec!["vibe".into(), "agent".into()],
                icon: vmux_api::PageIcon::None,
                shortcut: String::new(),
                prompt_target: true,
                startup: false,
            },
        ]
    }

    #[test]
    fn spaces_query_includes_spaces_page_and_command() {
        let commands = vec![CommandBarCommandEntry {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }];

        let results = SearchRows::filter(SearchRowsInput {
            commands: &commands,
            ..SearchRowsInput::for_pages("spaces", &sample_pages())
        });

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Layers),
            shortcut: String::new(),
            prompt_target: false,
            prompt_hint: false,
        }));
        assert!(results.contains(&CommandBarResultItem::Command {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }));
    }

    #[test]
    fn page_matched_by_keyword() {
        let results =
            SearchRows::filter(SearchRowsInput::for_pages("preferences", &sample_pages()));
        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Settings),
            shortcut: String::new(),
            prompt_target: false,
            prompt_hint: false,
        }));
    }

    #[test]
    fn agent_page_matched_by_vmux_prefix_carries_favicon() {
        let results = SearchRows::filter(SearchRowsInput::for_pages("vmux://", &sample_pages()));
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { url, icon, .. }
                if url == "vmux://sessions/vibe/" && matches!(icon, vmux_api::PageIcon::None)
        )));
    }

    #[test]
    fn agent_page_matched_by_name() {
        let results = SearchRows::filter(SearchRowsInput::for_pages("vibe", &sample_pages()));
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, icon, .. }
                if title == "Vibe" && matches!(icon, vmux_api::PageIcon::None)
        )));
    }

    #[test]
    fn start_agent_pages_preserve_input_order_and_exclude_other_pages() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            url: "vmux://sessions/codex/cli".into(),
            title: "Codex (CLI)".into(),
            keywords: vec!["codex".into(), "agent".into()],
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
            startup: false,
        });

        let results = PageRows::prompt_targets(&pages, "");
        let urls: Vec<_> = results
            .iter()
            .filter_map(|result| match result {
                CommandBarResultItem::Page { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(
            urls,
            vec!["vmux://sessions/vibe/", "vmux://sessions/codex/cli"]
        );
    }

    #[test]
    fn start_agent_pages_filter_by_query() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            url: "vmux://sessions/codex/cli".into(),
            title: "Codex (CLI)".into(),
            keywords: vec!["codex".into(), "agent".into()],
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
            startup: false,
        });

        let results = PageRows::prompt_targets(&pages, "vibe");

        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            CommandBarResultItem::Page { url, .. } if url == "vmux://sessions/vibe/"
        ));
    }

    #[test]
    fn start_agent_name_match_is_not_a_prompt() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            url: "vmux://sessions/codex-acp".into(),
            title: "Codex".into(),
            keywords: vec!["codex-acp".into(), "acp".into(), "agent".into()],
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
            startup: false,
        });
        let codex = PageRows::prompt_targets(&pages, "cod").remove(0);

        assert!(PageRows::prompt_target_matches(&codex, "cod"));
        assert!(PageRows::prompt_target_matches(&codex, "codex"));
        assert!(PageRows::prompt_target_matches(&codex, "codex-acp"));
        assert!(!PageRows::prompt_target_matches(
            &codex,
            "fix the failing test"
        ));
    }

    #[test]
    fn start_prompt_text_keeps_all_agent_choices_visible() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            url: "vmux://sessions/codex/cli".into(),
            title: "Codex (CLI)".into(),
            keywords: vec!["codex".into(), "agent".into()],
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
            startup: false,
        });

        let results = PageRows::prompt_targets(&pages, "show me something fun in terminal");
        let urls: Vec<_> = results
            .iter()
            .filter_map(PageRows::prompt_target_url)
            .collect();

        assert_eq!(
            urls,
            vec!["vmux://sessions/vibe/", "vmux://sessions/codex/cli"]
        );
    }

    #[test]
    fn start_page_does_not_show_unmatched_agents() {
        let results = StartRows::start(&sample_pages(), &[], "settings");
        let urls: Vec<_> = results
            .iter()
            .filter_map(|result| match result {
                CommandBarResultItem::Page { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(urls, vec!["vmux://settings/"]);
    }

    #[test]
    fn start_page_offers_three_recent_search_engines_in_supplied_order() {
        let engines = [
            search_engine("kagi"),
            search_engine("google"),
            search_engine("bing"),
            search_engine("duckduckgo"),
        ];
        let results = StartRows::start(&sample_pages(), &engines, "fix the failing test");
        let actual = results
            .iter()
            .filter_map(|result| match result {
                CommandBarResultItem::Search { engine, .. } => Some(engine.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, engines[..3]);
    }

    #[test]
    fn selected_agent_and_two_recent_agents_precede_web_search() {
        let mut pages = sample_pages();
        pages.extend([
            CommandBarPage {
                url: "vmux://sessions/codex/cli".into(),
                title: "Codex".into(),
                keywords: vec!["codex".into(), "agent".into()],
                icon: vmux_api::PageIcon::None,
                shortcut: String::new(),
                prompt_target: true,
                startup: false,
            },
            CommandBarPage {
                url: "vmux://sessions/claude".into(),
                title: "Claude".into(),
                keywords: vec!["claude".into(), "agent".into()],
                icon: vmux_api::PageIcon::None,
                shortcut: String::new(),
                prompt_target: true,
                startup: false,
            },
        ]);
        let agents = PageRows::prompt_targets(&pages, "");
        let selected = agents[1].clone();
        let mut results = StartRows::start(
            &pages,
            &[search_engine("google"), search_engine("bing")],
            "show me something fun",
        );

        StartRows::prepend_targets(
            &mut results,
            Some(&selected),
            &agents,
            "show me something fun",
        );

        assert_eq!(
            PageRows::prompt_target_url(&results[0]),
            Some("vmux://sessions/codex/cli")
        );
        assert_eq!(
            PageRows::prompt_target_url(&results[1]),
            Some("vmux://sessions/vibe/")
        );
        assert_eq!(
            PageRows::prompt_target_url(&results[2]),
            Some("vmux://sessions/claude")
        );
        assert!(matches!(results[3], CommandBarResultItem::Search { .. }));
    }

    #[test]
    fn start_page_uses_the_terminal_page_manifest() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            url: "vmux://terminal/".into(),
            title: "Terminal".into(),
            keywords: vec!["shell".into()],
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: false,
            startup: false,
        });
        let results = StartRows::start(&pages, &[], "terminal");
        assert!(matches!(
            results.first(),
            Some(CommandBarResultItem::Page { url, .. }) if url == "vmux://terminal/"
        ));
    }

    #[test]
    fn prompt_agent_url_only_accepts_agent_page_rows() {
        let agent = PageRows::prompt_targets(&sample_pages(), "").remove(0);
        let settings = CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: false,
            prompt_hint: false,
        };

        assert_eq!(
            PageRows::prompt_target_url(&agent),
            Some("vmux://sessions/vibe/")
        );
        assert_eq!(PageRows::prompt_target_url(&settings), None);
    }

    #[test]
    fn settings_page_reachable_by_name() {
        let results = SearchRows::filter(SearchRowsInput::for_pages("setti", &sample_pages()));
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, .. } if title == "Settings"
        )));
    }

    #[test]
    fn empty_query_lists_all_pages_before_commands() {
        let commands = vec![CommandBarCommandEntry {
            id: "close".to_string(),
            name: "Close".to_string(),
            shortcut: String::new(),
        }];

        let results = SearchRows::filter(SearchRowsInput {
            commands: &commands,
            ..SearchRowsInput::for_pages("", &sample_pages())
        });

        let page_count = results
            .iter()
            .filter(|r| matches!(r, CommandBarResultItem::Page { .. }))
            .count();
        assert_eq!(page_count, sample_pages().len());

        let last_page = results
            .iter()
            .rposition(|r| matches!(r, CommandBarResultItem::Page { .. }))
            .expect("pages present on empty query");
        let first_command = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::Command { .. }))
            .expect("command present");
        assert!(last_page < first_command, "pages must come before commands");
    }

    #[test]
    fn pages_listed_alphabetically_by_url() {
        let results = SearchRows::filter(SearchRowsInput::for_pages("", &sample_pages()));
        let urls: Vec<String> = results
            .iter()
            .filter_map(|r| match r {
                CommandBarResultItem::Page { url, .. } => Some(url.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            urls,
            vec![
                "vmux://history/",
                "vmux://sessions/vibe/",
                "vmux://settings/",
                "vmux://spaces/",
            ]
        );
    }

    #[test]
    fn page_carries_shortcut() {
        let results = SearchRows::filter(SearchRowsInput::for_pages("history", &sample_pages()));
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, shortcut, .. }
                if title == "History" && shortcut == "\u{2318}Y"
        )));
    }

    #[test]
    fn command_prefix_excludes_pages() {
        let results = SearchRows::filter(SearchRowsInput::for_pages("> set", &sample_pages()));
        assert!(
            !results
                .iter()
                .any(|r| matches!(r, CommandBarResultItem::Page { .. }))
        );
    }

    #[test]
    fn file_history_is_hidden_without_at_prefix() {
        let history = [
            HistoryEntry {
                url_entity_bits: 1,
                url: "file:///work/main.rs".into(),
                title: "main.rs".into(),
                favicon_url: String::new(),
                visit_created_at: 1,
                visit_count: 1,
                last_visited_at: 1,
            },
            HistoryEntry {
                url_entity_bits: 2,
                url: "https://example.com/main".into(),
                title: "Main".into(),
                favicon_url: String::new(),
                visit_created_at: 1,
                visit_count: 1,
                last_visited_at: 1,
            },
        ];
        let results = SearchRows::filter(SearchRowsInput {
            history: &history,
            ..SearchRowsInput::for_pages("main", &sample_pages())
        });

        assert!(results.iter().all(|row| !matches!(
            row,
            CommandBarResultItem::History { url, .. } if url.starts_with("file://")
        )));
        assert!(results.iter().any(|row| matches!(
            row,
            CommandBarResultItem::History { url, .. } if url == "https://example.com/main"
        )));
    }

    #[test]
    fn open_sessions_are_the_launcher_resting_state() {
        let tabs = vec![
            CommandBarTab {
                title: "Fun terminal demo".into(),
                url: "vmux://sessions/claude/abc".into(),
                pane_id: 7,
                tab_index: 0,
                is_active: true,
                location: "space-1 / pane 1".into(),
            },
            CommandBarTab {
                title: "Docs".into(),
                url: "vmux://sessions/codex/def".into(),
                pane_id: 8,
                tab_index: 1,
                is_active: false,
                location: "space-1 / pane 2".into(),
            },
        ];

        let items = PageRows::open_sessions(&tabs, &[]);

        assert_eq!(items.len(), 1, "the stack already on screen is not offered");
        assert!(matches!(
            &items[0],
            CommandBarResultItem::Stack { title, pane_id, .. }
                if title == "Docs" && *pane_id == 8
        ));
        assert!(PageRows::open_sessions(&[], &[]).is_empty());
    }
}
