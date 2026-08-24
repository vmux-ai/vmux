use crate::i18n::translate;
use vmux_wire::PageIcon;
use vmux_wire::command_bar::{
    CommandBarCommandEntry, CommandBarPage, CommandBarRecentFile, CommandBarSpace, CommandBarTab,
    CommandBarWorkDir, HistoryEntry, SearchEngine,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandBarResultItem {
    Terminal {
        path: String,
    },
    /// Read the path rather than `cd` to it.
    ///
    /// Distinct from [`Self::File`], which is one entry of a directory listing: this is the
    /// literal text typed, offered whether or not anything on disk answers to it.
    Editor {
        path: String,
    },
    Stack {
        title: String,
        url: String,
        icon: PageIcon,
        pane_id: u64,
        tab_index: usize,
        location: String,
    },
    Space {
        id: String,
        name: String,
        profile: String,
        is_active: bool,
        tab_count: usize,
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
    },
    Navigate {
        url: String,
    },
    Search {
        engine: SearchEngine,
        query: String,
    },
    File {
        path: String,
        is_dir: bool,
    },
    History {
        url: String,
        title: String,
        favicon_url: String,
        visit_count: u32,
        last_visited_at: i64,
    },
    WorkDir {
        path: String,
        is_dir: bool,
    },
    RecentFile {
        url: String,
        title: String,
    },
}

fn looks_like_path(s: &str) -> bool {
    if vmux_wire::command_bar::is_data_uri(s) {
        return false;
    }
    s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
        || s.contains('/') && !s.contains(' ') && !s.contains("://")
}

fn space_result(space: &CommandBarSpace) -> CommandBarResultItem {
    CommandBarResultItem::Space {
        id: space.id.clone(),
        name: space.name.clone(),
        profile: space.profile.clone(),
        is_active: space.is_active,
        tab_count: space.tab_count as usize,
    }
}

fn space_matches(space: &CommandBarSpace, search_lower: &str) -> bool {
    search_lower.is_empty()
        || space.name.to_lowercase().contains(search_lower)
        || space.id.to_lowercase().contains(search_lower)
        || space.profile.to_lowercase().contains(search_lower)
}

fn urls_match(a: &str, b: &str) -> bool {
    a == b || a.trim_end_matches('/') == b.trim_end_matches('/')
}

fn stack_icon_for(pages: &[CommandBarPage], url: &str) -> PageIcon {
    pages
        .iter()
        .find(|p| urls_match(&p.url, url))
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
        .filter(|page| page_matches(page, search_lower))
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
        })
        .collect()
}

pub fn prompt_target_results(pages: &[CommandBarPage], query: &str) -> Vec<CommandBarResultItem> {
    let search_lower = query.trim().to_lowercase();
    let targets: Vec<_> = pages.iter().filter(|page| page.prompt_target).collect();
    let matches: Vec<_> = targets
        .iter()
        .copied()
        .filter(|page| page_matches(page, &search_lower))
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

pub fn prompt_target_matches_query(item: &CommandBarResultItem, query: &str) -> bool {
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

pub fn terminal_matches_query(query: &str) -> bool {
    let query = query.trim().to_lowercase();
    !query.is_empty() && "terminal".starts_with(&query)
}

pub fn prepend_prompt_targets(
    results: &mut Vec<CommandBarResultItem>,
    selected_target: Option<&CommandBarResultItem>,
    recent_targets: &[CommandBarResultItem],
    query: &str,
) {
    if !vmux_wire::command_bar::CommandBarQuery(query).is_start_prompt()
        || results.iter().any(|item| prompt_target_url(item).is_some())
    {
        return;
    }
    let mut suggestions = Vec::new();
    for target in selected_target.into_iter().chain(recent_targets) {
        let Some(url) = prompt_target_url(target) else {
            continue;
        };
        if suggestions
            .iter()
            .any(|existing| prompt_target_url(existing) == Some(url))
        {
            continue;
        }
        suggestions.push(target.clone());
        if suggestions.len() == 3 {
            break;
        }
    }
    let at = results
        .iter()
        .take_while(|item| matches!(item, CommandBarResultItem::Terminal { .. }))
        .count();
    results.splice(at..at, suggestions);
}

pub fn open_session_results(
    tabs: &[CommandBarTab],
    pages: &[CommandBarPage],
) -> Vec<CommandBarResultItem> {
    tabs.iter()
        .map(|tab| CommandBarResultItem::Stack {
            title: tab.title.clone(),
            url: tab.url.clone(),
            icon: stack_icon_for(pages, &tab.url),
            pane_id: tab.pane_id,
            tab_index: tab.tab_index as usize,
            location: tab.location.clone(),
        })
        .collect()
}

pub fn start_page_results(
    pages: &[CommandBarPage],
    work_dirs: &[CommandBarWorkDir],
    recent_files: &[CommandBarRecentFile],
    search_engines: &[SearchEngine],
    query: &str,
) -> Vec<CommandBarResultItem> {
    let search_lower = query.trim().to_lowercase();
    let mut results = Vec::new();
    if terminal_matches_query(query) {
        results.push(CommandBarResultItem::Terminal {
            path: String::new(),
        });
    }
    results.extend(
        prompt_target_results(pages, query)
            .into_iter()
            .filter(|item| prompt_target_matches_query(item, query)),
    );
    let mut app_pages: Vec<_> = pages
        .iter()
        .filter(|page| !page.prompt_target && page.host != "start" && page.host != "terminal")
        .filter(|page| page_matches(page, &search_lower))
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
            }),
    );
    results.extend(work_dir_results(work_dirs, &search_lower));
    results.extend(recent_file_results(recent_files, &search_lower));
    let trimmed = query.trim();
    if vmux_wire::command_bar::CommandBarQuery(trimmed).is_start_prompt() {
        let engines = if search_engines.is_empty() {
            SearchEngine::ALL.as_slice()
        } else {
            search_engines
        };
        results.extend(engines.iter().take(3).copied().map(|engine| {
            CommandBarResultItem::Search {
                engine,
                query: trimmed.to_string(),
            }
        }));
    } else if !trimmed.is_empty() {
        results.push(CommandBarResultItem::Navigate {
            url: trimmed.to_string(),
        });
    }
    results
}

fn work_dir_results(dirs: &[CommandBarWorkDir], search_lower: &str) -> Vec<CommandBarResultItem> {
    dirs.iter()
        .filter(|d| search_lower.is_empty() || d.path.to_lowercase().contains(search_lower))
        .map(|d| CommandBarResultItem::WorkDir {
            path: d.path.clone(),
            is_dir: d.is_dir,
        })
        .collect()
}

fn recent_file_results(
    files: &[CommandBarRecentFile],
    search_lower: &str,
) -> Vec<CommandBarResultItem> {
    files
        .iter()
        .filter(|f| {
            search_lower.is_empty()
                || f.title.to_lowercase().contains(search_lower)
                || f.url.to_lowercase().contains(search_lower)
        })
        .map(|f| CommandBarResultItem::RecentFile {
            url: f.url.clone(),
            title: f.title.clone(),
        })
        .collect()
}

fn space_list_items(spaces: &[CommandBarSpace], search_lower: &str) -> Vec<CommandBarResultItem> {
    spaces
        .iter()
        .filter(|space| space_matches(space, search_lower))
        .map(space_result)
        .collect()
}

pub fn space_switch_results(
    spaces: &[CommandBarSpace],
    pages: &[CommandBarPage],
    query: &str,
) -> Vec<CommandBarResultItem> {
    let search_lower = query.trim().to_lowercase();
    let mut items = space_list_items(spaces, &search_lower);
    if let Some(page) = pages.iter().find(|p| p.host == "spaces") {
        items.push(CommandBarResultItem::Page {
            url: page.url.clone(),
            title: translate("command-manage-spaces"),
            icon: page.icon.clone(),
            shortcut: String::new(),
            prompt_target: false,
        });
    }
    items
}

pub fn active_space_index(spaces: &[CommandBarSpace]) -> usize {
    spaces.iter().position(|s| s.is_active).unwrap_or(0)
}

fn query_targets_spaces_page(q: &str, pages: &[CommandBarPage]) -> bool {
    let Some(url) = pages
        .iter()
        .find(|p| p.host == "spaces")
        .map(|p| p.url.as_str())
    else {
        return false;
    };
    q == url || q == url.trim_end_matches('/') || q.starts_with(url)
}

fn command_results(
    commands: &[CommandBarCommandEntry],
) -> impl Iterator<Item = CommandBarResultItem> + '_ {
    commands.iter().map(|c| CommandBarResultItem::Command {
        id: c.id.clone(),
        name: c.name.clone(),
        shortcut: c.shortcut.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    spaces: &[CommandBarSpace],
    pages: &[CommandBarPage],
    new_tab: bool,
    history: &[HistoryEntry],
    work_dirs: &[CommandBarWorkDir],
    recent_files: &[CommandBarRecentFile],
) -> Vec<CommandBarResultItem> {
    let q = query.trim();

    if query_targets_spaces_page(q, pages) {
        let mut items = page_results(pages, &q.to_lowercase());
        items.extend(space_list_items(spaces, ""));
        items.extend(command_results(commands));
        return items;
    }

    if q.is_empty() {
        let mut items: Vec<CommandBarResultItem> = Vec::new();
        items.push(CommandBarResultItem::Navigate { url: String::new() });
        if new_tab {
            items.push(CommandBarResultItem::Terminal {
                path: String::new(),
            });
        }
        items.extend(tabs.iter().map(|t| CommandBarResultItem::Stack {
            title: t.title.clone(),
            url: t.url.clone(),
            icon: stack_icon_for(pages, &t.url),
            pane_id: t.pane_id,
            tab_index: t.tab_index as usize,
            location: t.location.clone(),
        }));
        items.extend(page_results(pages, ""));
        items.extend(work_dir_results(work_dirs, ""));
        items.extend(recent_file_results(recent_files, ""));
        items.extend(command_results(commands));
        return items;
    }

    let starts_with_cmd = q.starts_with('>');
    let search = if starts_with_cmd { q[1..].trim() } else { q };
    let search_lower = search.to_lowercase();

    let mut items = Vec::new();

    let is_path = looks_like_path(search);

    if !starts_with_cmd && is_path {
        // A trailing slash is the one thing the typed text says about what it names, and it
        // decides which of the two is the likelier intent: a directory is somewhere to work,
        // a file is something to read.
        let names_a_directory = search.ends_with('/');
        let editor = CommandBarResultItem::Editor {
            path: search.to_string(),
        };
        let terminal = CommandBarResultItem::Terminal {
            path: search.to_string(),
        };
        match names_a_directory {
            true => items.extend([terminal, editor]),
            false => items.extend([editor, terminal]),
        }
    }

    let terminal_label = translate("command-terminal").to_lowercase();
    if !starts_with_cmd
        && !is_path
        && new_tab
        && ("terminal".contains(&search_lower) || terminal_label.contains(&search_lower))
    {
        items.push(CommandBarResultItem::Terminal {
            path: String::new(),
        });
    }

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
        items.extend(page_results(pages, &search_lower));
        items.extend(space_list_items(spaces, &search_lower));
        items.extend(work_dir_results(work_dirs, &search_lower));
        items.extend(recent_file_results(recent_files, &search_lower));
    }

    if !starts_with_cmd || !search.is_empty() {
        for t in tabs {
            if search.is_empty()
                || t.title.to_lowercase().contains(&search_lower)
                || t.url.to_lowercase().contains(&search_lower)
            {
                items.push(CommandBarResultItem::Stack {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    icon: stack_icon_for(pages, &t.url),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index as usize,
                    location: t.location.clone(),
                });
            }
        }
    }

    if !starts_with_cmd {
        for h in history.iter().take(5) {
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
        });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::command_bar::{CommandBarCommandEntry, CommandBarTab};

    fn space(id: &str, name: &str, active: bool) -> CommandBarSpace {
        CommandBarSpace {
            id: id.to_string(),
            name: name.to_string(),
            profile: "Personal".to_string(),
            is_active: active,
            tab_count: if active { 3 } else { 0 },
        }
    }

    fn sample_pages() -> Vec<CommandBarPage> {
        vec![
            CommandBarPage {
                host: "settings".into(),
                url: "vmux://settings/".into(),
                title: "Settings".into(),
                keywords: vec!["preferences".into()],
                icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Settings),
                shortcut: String::new(),
                prompt_target: false,
            },
            CommandBarPage {
                host: "spaces".into(),
                url: "vmux://spaces/".into(),
                title: "Spaces".into(),
                keywords: vec!["space".into()],
                icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Layers),
                shortcut: String::new(),
                prompt_target: false,
            },
            CommandBarPage {
                host: "history".into(),
                url: "vmux://history/".into(),
                title: "History".into(),
                keywords: vec!["recent".into()],
                icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Clock),
                shortcut: "\u{2318}Y".into(),
                prompt_target: false,
            },
            CommandBarPage {
                host: "agent".into(),
                url: "vmux://agent/vibe/".into(),
                title: "Vibe".into(),
                keywords: vec!["vibe".into(), "agent".into()],
                icon: vmux_wire::PageIcon::None,
                shortcut: String::new(),
                prompt_target: true,
            },
        ]
    }

    #[test]
    fn space_switch_lists_spaces_in_order_then_manage() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("work", "Work", true),
        ];
        let results = space_switch_results(&spaces, &sample_pages(), "");
        assert!(matches!(&results[0], CommandBarResultItem::Space { id, .. } if id == "space-1"));
        assert!(matches!(&results[1], CommandBarResultItem::Space { id, .. } if id == "work"));
        assert!(matches!(
            results.last(),
            Some(CommandBarResultItem::Page { title, .. }) if title == "Manage spaces\u{2026}"
        ));
    }

    #[test]
    fn space_switch_filters_spaces_by_name() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("work", "Work", true),
        ];
        let results = space_switch_results(&spaces, &sample_pages(), "wor");
        let ids: Vec<_> = results
            .iter()
            .filter_map(|r| match r {
                CommandBarResultItem::Space { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["work".to_string()]);
        assert!(matches!(
            results.last(),
            Some(CommandBarResultItem::Page { title, .. }) if title == "Manage spaces\u{2026}"
        ));
    }

    #[test]
    fn active_space_index_finds_active_then_defaults_zero() {
        let with_active = vec![space("space-1", "S1", false), space("work", "Work", true)];
        assert_eq!(active_space_index(&with_active), 1);
        let none_active = vec![space("space-1", "S1", false)];
        assert_eq!(active_space_index(&none_active), 0);
    }

    #[test]
    fn spaces_url_lists_all_spaces() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("work", "Work", true),
        ];

        let results = filter_results(
            "vmux://spaces/",
            &[],
            &[] as &[CommandBarCommandEntry],
            &spaces,
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Layers),
            shortcut: String::new(),
            prompt_target: false,
        }));
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "space-1"
        )));
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "work"
        )));
    }

    #[test]
    fn spaces_url_includes_normal_commands() {
        let commands = vec![CommandBarCommandEntry {
            id: "browser_open_command_bar".to_string(),
            name: "Command Bar".to_string(),
            shortcut: "super+k".to_string(),
        }];

        let results = filter_results(
            "vmux://spaces/",
            &[],
            &commands,
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Layers),
            shortcut: String::new(),
            prompt_target: false,
        }));
        assert!(results.contains(&CommandBarResultItem::Command {
            id: "browser_open_command_bar".to_string(),
            name: "Command Bar".to_string(),
            shortcut: "super+k".to_string(),
        }));
    }

    #[test]
    fn spaces_query_includes_spaces_page_and_command() {
        let commands = vec![CommandBarCommandEntry {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }];

        let results = filter_results(
            "spaces",
            &[],
            &commands,
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Layers),
            shortcut: String::new(),
            prompt_target: false,
        }));
        assert!(results.contains(&CommandBarResultItem::Command {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }));
    }

    #[test]
    fn space_names_are_searchable() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("client", "Client Work", false),
        ];
        let tabs: Vec<CommandBarTab> = Vec::new();

        let results = filter_results(
            "client",
            &tabs,
            &[],
            &spaces,
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );

        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "client"
        )));
    }

    #[test]
    fn page_matched_by_keyword() {
        let results = filter_results(
            "preferences",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );
        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: vmux_wire::PageIcon::Builtin(vmux_wire::BuiltinIcon::Settings),
            shortcut: String::new(),
            prompt_target: false,
        }));
    }

    #[test]
    fn agent_page_matched_by_vmux_prefix_carries_favicon() {
        let results = filter_results(
            "vmux://",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { url, icon, .. }
                if url == "vmux://agent/vibe/" && matches!(icon, vmux_wire::PageIcon::None)
        )));
    }

    #[test]
    fn agent_page_matched_by_name() {
        let results = filter_results("vibe", &[], &[], &[], &sample_pages(), false, &[], &[], &[]);
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, icon, .. }
                if title == "Vibe" && matches!(icon, vmux_wire::PageIcon::None)
        )));
    }

    #[test]
    fn start_agent_pages_preserve_input_order_and_exclude_other_pages() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            host: "agent".into(),
            url: "vmux://agent/codex/cli".into(),
            title: "Codex (CLI)".into(),
            keywords: vec!["codex".into(), "agent".into()],
            icon: vmux_wire::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
        });

        let results = prompt_target_results(&pages, "");
        let urls: Vec<_> = results
            .iter()
            .filter_map(|result| match result {
                CommandBarResultItem::Page { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(urls, vec!["vmux://agent/vibe/", "vmux://agent/codex/cli"]);
    }

    #[test]
    fn start_agent_pages_filter_by_query() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            host: "agent".into(),
            url: "vmux://agent/codex/cli".into(),
            title: "Codex (CLI)".into(),
            keywords: vec!["codex".into(), "agent".into()],
            icon: vmux_wire::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
        });

        let results = prompt_target_results(&pages, "vibe");

        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            CommandBarResultItem::Page { url, .. } if url == "vmux://agent/vibe/"
        ));
    }

    #[test]
    fn start_agent_name_match_is_not_a_prompt() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            host: "agent".into(),
            url: "vmux://agent/codex-acp".into(),
            title: "Codex".into(),
            keywords: vec!["codex-acp".into(), "acp".into(), "agent".into()],
            icon: vmux_wire::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
        });
        let codex = prompt_target_results(&pages, "cod").remove(0);

        assert!(prompt_target_matches_query(&codex, "cod"));
        assert!(prompt_target_matches_query(&codex, "codex"));
        assert!(prompt_target_matches_query(&codex, "codex-acp"));
        assert!(!prompt_target_matches_query(&codex, "fix the failing test"));
    }

    #[test]
    fn start_prompt_text_keeps_all_agent_choices_visible() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            host: "agent".into(),
            url: "vmux://agent/codex/cli".into(),
            title: "Codex (CLI)".into(),
            keywords: vec!["codex".into(), "agent".into()],
            icon: vmux_wire::PageIcon::None,
            shortcut: String::new(),
            prompt_target: true,
        });

        let results = prompt_target_results(&pages, "show me something fun in terminal");
        let urls: Vec<_> = results.iter().filter_map(prompt_target_url).collect();

        assert_eq!(urls, vec!["vmux://agent/vibe/", "vmux://agent/codex/cli"]);
    }

    #[test]
    fn start_page_does_not_show_unmatched_agents() {
        let results = start_page_results(&sample_pages(), &[], &[], &[], "settings");
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
            SearchEngine::Kagi,
            SearchEngine::Google,
            SearchEngine::Bing,
            SearchEngine::DuckDuckGo,
        ];
        let results =
            start_page_results(&sample_pages(), &[], &[], &engines, "fix the failing test");
        let actual = results
            .iter()
            .filter_map(|result| match result {
                CommandBarResultItem::Search { engine, .. } => Some(*engine),
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
                host: "agent".into(),
                url: "vmux://agent/codex/cli".into(),
                title: "Codex".into(),
                keywords: vec!["codex".into(), "agent".into()],
                icon: vmux_wire::PageIcon::None,
                shortcut: String::new(),
                prompt_target: true,
            },
            CommandBarPage {
                host: "agent".into(),
                url: "vmux://agent/claude".into(),
                title: "Claude".into(),
                keywords: vec!["claude".into(), "agent".into()],
                icon: vmux_wire::PageIcon::None,
                shortcut: String::new(),
                prompt_target: true,
            },
        ]);
        let agents = prompt_target_results(&pages, "");
        let selected = agents[1].clone();
        let mut results = start_page_results(
            &pages,
            &[],
            &[],
            &[SearchEngine::Google, SearchEngine::Bing],
            "show me something fun",
        );

        prepend_prompt_targets(
            &mut results,
            Some(&selected),
            &agents,
            "show me something fun",
        );

        assert_eq!(
            prompt_target_url(&results[0]),
            Some("vmux://agent/codex/cli")
        );
        assert_eq!(prompt_target_url(&results[1]), Some("vmux://agent/vibe/"));
        assert_eq!(prompt_target_url(&results[2]), Some("vmux://agent/claude"));
        assert!(matches!(results[3], CommandBarResultItem::Search { .. }));
    }

    #[test]
    fn terminal_leads_but_agents_and_search_stay_available() {
        let agent = prompt_target_results(&sample_pages(), "").remove(0);
        let mut results = start_page_results(&sample_pages(), &[], &[], &[], "terminal");

        prepend_prompt_targets(&mut results, Some(&agent), &[], "terminal");

        assert!(
            matches!(results.first(), Some(CommandBarResultItem::Terminal { .. })),
            "terminal keeps the default selection: {results:?}"
        );
        assert!(
            results.iter().any(|item| prompt_target_url(item).is_some()),
            "asking an agent stays reachable: {results:?}"
        );
        assert!(
            results
                .iter()
                .any(|item| matches!(item, CommandBarResultItem::Search { .. })),
            "search stays reachable: {results:?}"
        );
    }

    #[test]
    fn a_terminal_prefix_offers_terminal_alongside_the_other_options() {
        let results = start_page_results(&sample_pages(), &[], &[], &[], "ter");
        assert!(matches!(
            results.first(),
            Some(CommandBarResultItem::Terminal { .. })
        ));
        assert!(results.len() > 1, "prefix match is not the only option");
    }

    #[test]
    fn terminal_query_predicate_matches_display_and_activation() {
        assert!(terminal_matches_query("t"));
        assert!(terminal_matches_query("ter"));
        assert!(terminal_matches_query("Terminal"));
        assert!(terminal_matches_query("  term  "));
        assert!(!terminal_matches_query(""));
        assert!(!terminal_matches_query("   "));
        assert!(!terminal_matches_query("terminals"));
        assert!(!terminal_matches_query("xterm"));
    }

    #[test]
    fn start_page_suggests_terminal_by_name() {
        let mut pages = sample_pages();
        pages.push(CommandBarPage {
            host: "terminal".into(),
            url: "vmux://terminal/".into(),
            title: "Terminal".into(),
            keywords: vec!["shell".into()],
            icon: vmux_wire::PageIcon::None,
            shortcut: String::new(),
            prompt_target: false,
        });
        let results = start_page_results(&pages, &[], &[], &[], "terminal");
        assert!(matches!(
            results.first(),
            Some(CommandBarResultItem::Terminal { .. })
        ));
        assert_eq!(
            results
                .iter()
                .filter(|item| matches!(item, CommandBarResultItem::Terminal { .. }))
                .count(),
            1,
            "an open terminal page must not duplicate the terminal action: {results:?}"
        );
        assert!(
            !results.iter().any(|item| matches!(
                item,
                CommandBarResultItem::Page { url, .. } if url.starts_with("vmux://terminal")
            )),
            "the terminal host is offered as the action, not as a page: {results:?}"
        );
    }

    #[test]
    fn start_page_searches_work_dirs_and_recent_files() {
        let work_dirs = vec![CommandBarWorkDir {
            path: "/work/vmux".into(),
            is_dir: true,
        }];
        let recent_files = vec![CommandBarRecentFile {
            url: "file:///work/vmux/README.md".into(),
            title: "README.md".into(),
        }];
        let dir_results =
            start_page_results(&sample_pages(), &work_dirs, &recent_files, &[], "vmux");
        assert!(dir_results.iter().any(|result| matches!(
            result,
            CommandBarResultItem::WorkDir { path, .. } if path == "/work/vmux"
        )));
        let file_results =
            start_page_results(&sample_pages(), &work_dirs, &recent_files, &[], "readme");
        assert!(file_results.iter().any(|result| matches!(
            result,
            CommandBarResultItem::RecentFile { url, .. }
                if url == "file:///work/vmux/README.md"
        )));
    }

    #[test]
    fn prompt_agent_url_only_accepts_agent_page_rows() {
        let agent = prompt_target_results(&sample_pages(), "").remove(0);
        let settings = CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: vmux_wire::PageIcon::None,
            shortcut: String::new(),
            prompt_target: false,
        };

        assert_eq!(prompt_target_url(&agent), Some("vmux://agent/vibe/"));
        assert_eq!(prompt_target_url(&settings), None);
    }

    #[test]
    fn settings_page_reachable_by_name() {
        let results = filter_results(
            "setti",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );
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

        let results = filter_results(
            "",
            &[],
            &commands,
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );

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
        let results = filter_results("", &[], &[], &[], &sample_pages(), false, &[], &[], &[]);
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
                "vmux://agent/vibe/",
                "vmux://history/",
                "vmux://settings/",
                "vmux://spaces/",
            ]
        );
    }

    #[test]
    fn page_carries_shortcut() {
        let results = filter_results(
            "history",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, shortcut, .. }
                if title == "History" && shortcut == "\u{2318}Y"
        )));
    }

    #[test]
    fn command_prefix_excludes_pages() {
        let results = filter_results(
            "> set",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
        );
        assert!(
            !results
                .iter()
                .any(|r| matches!(r, CommandBarResultItem::Page { .. }))
        );
    }

    fn sample_work_dirs() -> Vec<CommandBarWorkDir> {
        vec![CommandBarWorkDir {
            path: "/work/proj/main.rs".into(),
            is_dir: false,
        }]
    }

    fn sample_recent_files() -> Vec<CommandBarRecentFile> {
        vec![CommandBarRecentFile {
            url: "file:///work/proj/main.rs".into(),
            title: "main.rs".into(),
        }]
    }

    #[test]
    fn empty_query_puts_work_after_pages() {
        let results = filter_results(
            "",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &sample_work_dirs(),
            &sample_recent_files(),
        );
        let last_page = results
            .iter()
            .rposition(|r| matches!(r, CommandBarResultItem::Page { .. }))
            .expect("pages present");
        let first_work = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::WorkDir { .. }))
            .expect("work dir present");
        let first_recent = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::RecentFile { .. }))
            .expect("recent file present");
        assert!(last_page < first_work, "work dirs come after pages");
        assert!(first_work < first_recent, "dirs before recent files");
    }

    fn path_results(query: &str) -> Vec<CommandBarResultItem> {
        filter_results(query, &[], &[], &[], &sample_pages(), false, &[], &[], &[])
    }

    /// A file path in the bar used to mean one thing — spawn a shell next to it — so reading a
    /// file you could name outright meant navigating to its directory first.
    #[test]
    fn a_file_path_leads_with_the_editor_and_still_offers_the_terminal() {
        let results = path_results("/work/proj/main.rs");
        let editor = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::Editor { .. }))
            .expect("a path offers the editor");
        let terminal = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::Terminal { .. }))
            .expect("a path still offers the terminal");
        assert!(editor < terminal, "a file is likelier to be read than cd'd");
    }

    /// The other way round for a directory: it is somewhere to work, not something to read.
    #[test]
    fn a_directory_leads_with_the_terminal() {
        let results = path_results("/work/proj/");
        let editor = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::Editor { .. }))
            .expect("a directory still opens in the editor");
        let terminal = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::Terminal { .. }))
            .expect("a directory offers the terminal");
        assert!(terminal < editor, "a directory is a place to work");
    }

    /// The editor row carries what was typed, because that is what the accept path turns into a
    /// `file://` url — an empty one would open the page with no file.
    #[test]
    fn the_editor_row_carries_the_path_that_was_typed() {
        assert!(path_results("~/notes.md").iter().any(|r| matches!(
            r, CommandBarResultItem::Editor { path } if path == "~/notes.md"
        )));
    }

    /// `>` is the command prefix, and a command is not a path however it is spelled.
    #[test]
    fn a_command_is_never_offered_to_the_editor() {
        assert!(
            !path_results("> /work/proj/main.rs")
                .iter()
                .any(|r| matches!(r, CommandBarResultItem::Editor { .. }))
        );
    }

    #[test]
    fn work_dir_matched_by_query() {
        let results = filter_results(
            "proj",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &sample_work_dirs(),
            &sample_recent_files(),
        );
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::WorkDir { path, .. } if path == "/work/proj/main.rs"
        )));
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::RecentFile { title, .. } if title == "main.rs"
        )));
    }

    #[test]
    fn open_sessions_are_the_launcher_resting_state() {
        let tabs = vec![
            CommandBarTab {
                title: "Fun terminal demo".into(),
                url: "vmux://agent/claude/abc".into(),
                pane_id: 7,
                tab_index: 0,
                is_active: true,
                location: "space-1 / pane 1".into(),
            },
            CommandBarTab {
                title: "Docs".into(),
                url: "vmux://agent/codex/def".into(),
                pane_id: 8,
                tab_index: 1,
                is_active: false,
                location: "space-1 / pane 2".into(),
            },
        ];

        let items = open_session_results(&tabs, &[]);

        assert_eq!(items.len(), 2);
        assert!(matches!(
            &items[0],
            CommandBarResultItem::Stack { title, pane_id, .. }
                if title == "Fun terminal demo" && *pane_id == 7
        ));
        assert!(open_session_results(&[], &[]).is_empty());
    }
}
