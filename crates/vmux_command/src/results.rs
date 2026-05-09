use crate::event::{CommandBarCommandEntry, CommandBarSpace, CommandBarTab};

const SPACES_QUERY: &str = "vmux://spaces";
pub const SPACES_PAGE_URL: &str = "vmux://spaces/";
const SPACES_QUERY_PREFIX: &str = SPACES_PAGE_URL;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandBarResultItem {
    Terminal {
        path: String,
    },
    Stack {
        title: String,
        url: String,
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
    Navigate {
        url: String,
    },
}

fn looks_like_path(s: &str) -> bool {
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

fn space_query(q: &str) -> Option<&str> {
    if q == SPACES_QUERY {
        Some("")
    } else {
        q.strip_prefix(SPACES_QUERY_PREFIX)
    }
}

fn spaces_page_matches(search_lower: &str) -> bool {
    search_lower.is_empty()
        || "spaces".contains(search_lower)
        || SPACES_PAGE_URL.contains(search_lower)
}

fn space_results(spaces: &[CommandBarSpace], search_lower: &str) -> Vec<CommandBarResultItem> {
    let mut results = Vec::new();
    if spaces_page_matches(search_lower) {
        results.push(CommandBarResultItem::Navigate {
            url: SPACES_PAGE_URL.to_string(),
        });
    }
    results.extend(
        spaces
            .iter()
            .filter(|space| space_matches(space, search_lower))
            .map(space_result),
    );
    results
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
    new_tab: bool,
) -> Vec<CommandBarResultItem> {
    let q = query.trim();
    if let Some(search) = space_query(q) {
        let search_lower = search.trim().to_lowercase();
        let mut items = space_results(spaces, &search_lower);
        if search_lower.is_empty() {
            items.extend(command_results(commands));
        }
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
            pane_id: t.pane_id,
            tab_index: t.tab_index as usize,
        }));
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
        items.extend(space_results(spaces, &search_lower));
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
                    pane_id: t.pane_id,
                    tab_index: t.tab_index as usize,
                });
            }
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
    use crate::event::{CommandBarCommandEntry, CommandBarTab};

    fn space(id: &str, name: &str, active: bool) -> CommandBarSpace {
        CommandBarSpace {
            id: id.to_string(),
            name: name.to_string(),
            profile: "default".to_string(),
            is_active: active,
            tab_count: if active { 3 } else { 0 },
        }
    }

    #[test]
    fn spaces_url_lists_all_spaces() {
        let spaces = vec![
            space("default", "Default", false),
            space("work", "Work", true),
        ];

        let results = filter_results(
            "vmux://spaces/",
            &[],
            &[] as &[CommandBarCommandEntry],
            &spaces,
            false,
        );

        assert_eq!(
            results,
            vec![
                CommandBarResultItem::Navigate {
                    url: SPACES_PAGE_URL.to_string(),
                },
                CommandBarResultItem::Space {
                    id: "default".to_string(),
                    name: "Default".to_string(),
                    profile: "default".to_string(),
                    is_active: false,
                    tab_count: 0,
                },
                CommandBarResultItem::Space {
                    id: "work".to_string(),
                    name: "Work".to_string(),
                    profile: "default".to_string(),
                    is_active: true,
                    tab_count: 3,
                },
            ]
        );
    }

    #[test]
    fn spaces_url_includes_normal_commands() {
        let commands = vec![CommandBarCommandEntry {
            id: "browser_open_command_bar".to_string(),
            name: "Command Bar".to_string(),
            shortcut: "super+k".to_string(),
        }];

        let results = filter_results("vmux://spaces/", &[], &commands, &[], false);

        assert!(results.contains(&CommandBarResultItem::Navigate {
            url: SPACES_PAGE_URL.to_string(),
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

        let results = filter_results("spaces", &[], &commands, &[], false);

        assert!(results.contains(&CommandBarResultItem::Navigate {
            url: SPACES_PAGE_URL.to_string(),
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
            space("default", "Default", false),
            space("client", "Client Work", false),
        ];
        let tabs: Vec<CommandBarTab> = Vec::new();

        let results = filter_results("client", &tabs, &[], &spaces, false);

        assert!(matches!(
            results.first(),
            Some(CommandBarResultItem::Space { id, .. }) if id == "client"
        ));
    }
}
