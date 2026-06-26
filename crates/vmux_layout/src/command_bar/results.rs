use vmux_command::event::{
    CommandBarCommandEntry, CommandBarPage, CommandBarSpace, CommandBarTab, HistoryEntry,
};
use vmux_core::PageIcon;

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
    },
    Navigate {
        url: String,
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
}

fn looks_like_path(s: &str) -> bool {
    if vmux_command::event::is_data_uri(s) {
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

pub fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    spaces: &[CommandBarSpace],
    pages: &[CommandBarPage],
    new_tab: bool,
    history: &[HistoryEntry],
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
        }));
        items.extend(page_results(pages, ""));
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

    if !starts_with_cmd && !is_path && new_tab && "terminal".contains(&search_lower) {
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
    use vmux_command::event::{CommandBarCommandEntry, CommandBarTab};

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
                icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Settings),
                shortcut: String::new(),
            },
            CommandBarPage {
                host: "spaces".into(),
                url: "vmux://spaces/".into(),
                title: "Spaces".into(),
                keywords: vec!["space".into()],
                icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Layers),
                shortcut: String::new(),
            },
            CommandBarPage {
                host: "history".into(),
                url: "vmux://history/".into(),
                title: "History".into(),
                keywords: vec!["recent".into()],
                icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Clock),
                shortcut: "\u{2318}Y".into(),
            },
            CommandBarPage {
                host: "agent".into(),
                url: "vmux://agent/vibe/".into(),
                title: "Vibe".into(),
                keywords: vec!["vibe".into(), "agent".into()],
                icon: vmux_core::PageIcon::None,
                shortcut: String::new(),
            },
        ]
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
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Layers),
            shortcut: String::new(),
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
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Layers),
            shortcut: String::new(),
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

        let results = filter_results("spaces", &[], &commands, &[], &sample_pages(), false, &[]);

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Layers),
            shortcut: String::new(),
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

        let results = filter_results("client", &tabs, &[], &spaces, &sample_pages(), false, &[]);

        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "client"
        )));
    }

    #[test]
    fn page_matched_by_keyword() {
        let results = filter_results("preferences", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Settings),
            shortcut: String::new(),
        }));
    }

    #[test]
    fn agent_page_matched_by_vmux_prefix_carries_favicon() {
        let results = filter_results("vmux://", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { url, icon, .. }
                if url == "vmux://agent/vibe/" && matches!(icon, vmux_core::PageIcon::None)
        )));
    }

    #[test]
    fn agent_page_matched_by_name() {
        let results = filter_results("vibe", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, icon, .. }
                if title == "Vibe" && matches!(icon, vmux_core::PageIcon::None)
        )));
    }

    #[test]
    fn settings_page_reachable_by_name() {
        let results = filter_results("setti", &[], &[], &[], &sample_pages(), false, &[]);
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

        let results = filter_results("", &[], &commands, &[], &sample_pages(), false, &[]);

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
        let results = filter_results("", &[], &[], &[], &sample_pages(), false, &[]);
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
        let results = filter_results("history", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, shortcut, .. }
                if title == "History" && shortcut == "\u{2318}Y"
        )));
    }

    #[test]
    fn command_prefix_excludes_pages() {
        let results = filter_results("> set", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(
            !results
                .iter()
                .any(|r| matches!(r, CommandBarResultItem::Page { .. }))
        );
    }
}
