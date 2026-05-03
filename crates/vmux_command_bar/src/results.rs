use crate::event::{CommandBarCommandEntry, CommandBarSession, CommandBarTab};

const SESSIONS_QUERY: &str = "vmux://sessions";
pub const SESSIONS_PAGE_URL: &str = "vmux://sessions/";
const SESSIONS_QUERY_PREFIX: &str = SESSIONS_PAGE_URL;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandBarResultItem {
    Terminal {
        path: String,
    },
    Tab {
        title: String,
        url: String,
        pane_id: u64,
        tab_index: usize,
    },
    Session {
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

fn session_result(session: &CommandBarSession) -> CommandBarResultItem {
    CommandBarResultItem::Session {
        id: session.id.clone(),
        name: session.name.clone(),
        profile: session.profile.clone(),
        is_active: session.is_active,
        tab_count: session.tab_count,
    }
}

fn session_matches(session: &CommandBarSession, search_lower: &str) -> bool {
    search_lower.is_empty()
        || session.name.to_lowercase().contains(search_lower)
        || session.id.to_lowercase().contains(search_lower)
        || session.profile.to_lowercase().contains(search_lower)
}

fn session_query(q: &str) -> Option<&str> {
    if q == SESSIONS_QUERY {
        Some("")
    } else {
        q.strip_prefix(SESSIONS_QUERY_PREFIX)
    }
}

fn session_results(
    sessions: &[CommandBarSession],
    search_lower: &str,
) -> Vec<CommandBarResultItem> {
    let mut results = Vec::new();
    if search_lower.is_empty() {
        results.push(CommandBarResultItem::Navigate {
            url: SESSIONS_PAGE_URL.to_string(),
        });
    }
    results.extend(
        sessions
            .iter()
            .filter(|session| session_matches(session, search_lower))
            .map(session_result),
    );
    results
}

pub fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    sessions: &[CommandBarSession],
    new_tab: bool,
) -> Vec<CommandBarResultItem> {
    let q = query.trim();
    if let Some(search) = session_query(q) {
        let search_lower = search.trim().to_lowercase();
        return session_results(sessions, &search_lower);
    }

    if q.is_empty() {
        let mut items: Vec<CommandBarResultItem> = Vec::new();
        items.push(CommandBarResultItem::Navigate { url: String::new() });
        if new_tab {
            items.push(CommandBarResultItem::Terminal {
                path: String::new(),
            });
        }
        items.extend(tabs.iter().map(|t| CommandBarResultItem::Tab {
            title: t.title.clone(),
            url: t.url.clone(),
            pane_id: t.pane_id,
            tab_index: t.tab_index,
        }));
        items.extend(commands.iter().map(|c| CommandBarResultItem::Command {
            id: c.id.clone(),
            name: c.name.clone(),
            shortcut: c.shortcut.clone(),
        }));
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
        items.extend(session_results(sessions, &search_lower));
    }

    if !starts_with_cmd || !search.is_empty() {
        for t in tabs {
            if search.is_empty()
                || t.title.to_lowercase().contains(&search_lower)
                || t.url.to_lowercase().contains(&search_lower)
            {
                items.push(CommandBarResultItem::Tab {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index,
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

    fn session(id: &str, name: &str, active: bool) -> CommandBarSession {
        CommandBarSession {
            id: id.to_string(),
            name: name.to_string(),
            profile: "default".to_string(),
            is_active: active,
            tab_count: if active { 3 } else { 0 },
        }
    }

    #[test]
    fn sessions_url_lists_all_sessions() {
        let sessions = vec![
            session("default", "Default", false),
            session("work", "Work", true),
        ];

        let results = filter_results(
            "vmux://sessions/",
            &[],
            &[] as &[CommandBarCommandEntry],
            &sessions,
            false,
        );

        assert_eq!(
            results,
            vec![
                CommandBarResultItem::Navigate {
                    url: SESSIONS_PAGE_URL.to_string(),
                },
                CommandBarResultItem::Session {
                    id: "default".to_string(),
                    name: "Default".to_string(),
                    profile: "default".to_string(),
                    is_active: false,
                    tab_count: 0,
                },
                CommandBarResultItem::Session {
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
    fn session_names_are_searchable() {
        let sessions = vec![
            session("default", "Default", false),
            session("client", "Client Work", false),
        ];
        let tabs: Vec<CommandBarTab> = Vec::new();

        let results = filter_results("client", &tabs, &[], &sessions, false);

        assert!(matches!(
            results.first(),
            Some(CommandBarResultItem::Session { id, .. }) if id == "client"
        ));
    }
}
