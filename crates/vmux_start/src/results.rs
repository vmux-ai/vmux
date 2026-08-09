use vmux_ui::i18n::translate;
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
        /// Mirrors [`vmux_wire::command_bar::CommandBarPage::prompt_target`].
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

/// Pages a prompt can be sent to, in recent-first input order.
///
/// A matching query narrows the choices; text that matches none of them keeps every choice
/// visible, because that text is the prompt rather than a search for a target.
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

/// Whether the query should offer "Terminal".
///
/// Display and activation share this so a partially typed `ter` cannot list Terminal while
/// Enter quietly routes the text to a prompt target instead.
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
    if !vmux_wire::command_bar::is_start_prompt_query(query)
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
    // Terminal stays first when the query named it, so Enter still opens a terminal.
    let at = results
        .iter()
        .take_while(|item| matches!(item, CommandBarResultItem::Terminal { .. }))
        .count();
    results.splice(at..at, suggestions);
}

/// The launcher's resting state: every open session.
///
/// An empty query used to render nothing on the desktop launcher, so the sessions already open
/// were the one thing it could not show you. Both hosts now open on this list.
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
    // Terminal leads when the query names it, but never at the cost of the other options.
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
    if vmux_wire::command_bar::is_start_prompt_query(trimmed) {
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

/// Build the space-switcher result list: every space in snapshot order (filtered by
/// `query`), then a trailing "Manage spaces…" entry that opens the full spaces page.
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

/// Index of the active space, for pre-selecting the current space in the switcher.
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
        items.push(CommandBarResultItem::Terminal {
            path: search.to_string(),
        });
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
#[path = "results.test.rs"]
mod tests;
