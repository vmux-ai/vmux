use vmux_api::PageIcon;
use vmux_api::chat::{ResumableSessionEntry, SlashCommand, SlashCommandEntry};
use vmux_api::command_bar::{
    CommandBarCommandEntry, CommandBarPage, CommandBarPick, CommandBarPickRow, CommandBarPicker,
    CommandBarRecentFile, CommandBarSpace, CommandBarTab, CommandBarWorkDir, HistoryEntry,
    SearchEngine,
};
use vmux_core::chat_projection::ResumeRows;
use vmux_ui::i18n::translate;

pub use vmux_api::command_bar::CommandBarResultItem;

use super::{PaletteRows, query::PaletteQuery};

pub struct SlashRows;

impl SlashRows {
    const PENDING_ROWS: usize = 7;

    pub fn for_query(
        query: &str,
        commands: &[SlashCommandEntry],
        sessions: &[ResumableSessionEntry],
        pending: bool,
    ) -> Vec<CommandBarResultItem> {
        let held = PaletteQuery::new(query);
        let (name, rest) = match held.slash_token() {
            Some(parts) => parts,
            None if query.trim() == "/" => ("", ""),
            None => return Vec::new(),
        };
        let lowered = name.to_lowercase();
        let mut matching = Vec::new();
        for command in commands {
            if Self::name(command.command).starts_with(&lowered) {
                matching.push(command);
            }
        }
        let settled = match matching.as_slice() {
            [only] => Some(*only),
            _ => commands
                .iter()
                .find(|command| Self::name(command.command) == lowered),
        };
        if let Some(command) = settled
            && command.command == SlashCommand::Resume
        {
            if sessions.is_empty() && pending {
                return Self::pending();
            }
            return ResumeRows::filtered(rest, sessions);
        }
        let mut rows = Vec::new();
        for command in matching {
            rows.push(CommandBarResultItem::Slash {
                name: Self::name(command.command).to_string(),
                hint: if command.command == SlashCommand::Mcp {
                    translate("mcp-command-description")
                } else {
                    command.description.clone()
                },
            });
        }
        rows
    }

    pub(crate) const fn name(command: SlashCommand) -> &'static str {
        match command {
            SlashCommand::Upload => "upload",
            SlashCommand::Resume => "resume",
            SlashCommand::Mcp => "mcp",
            SlashCommand::Model => "model",
        }
    }

    fn pending() -> Vec<CommandBarResultItem> {
        let mut rows = Vec::new();
        for row in 0..Self::PENDING_ROWS {
            rows.push(CommandBarResultItem::ResumePending { row });
        }
        rows
    }
}

pub struct PickerRows;

impl PickerRows {
    pub fn filtered(
        picker: CommandBarPicker,
        picks: &[CommandBarPickRow],
        query: &str,
    ) -> Vec<CommandBarResultItem> {
        if Self::takes_typed_value(picker) {
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

    pub fn typed(picker: CommandBarPicker, input: &str) -> Option<CommandBarPick> {
        if !Self::takes_typed_value(picker) {
            return None;
        }
        let trimmed = input.trim();
        let digits = match trimmed.split_once(':') {
            Some((line, _)) => line.trim(),
            None => trimmed,
        };
        let line = digits.parse::<u32>().ok()?;
        Some(CommandBarPick::GotoLine {
            line: line.saturating_sub(1),
        })
    }

    pub const fn takes_typed_value(picker: CommandBarPicker) -> bool {
        matches!(picker, CommandBarPicker::GotoLine)
    }

    pub const fn placeholder(picker: CommandBarPicker) -> &'static str {
        match picker {
            CommandBarPicker::Space => "command-switch-space",
            CommandBarPicker::GotoLine => "editor-status-goto-placeholder",
            CommandBarPicker::Indent
            | CommandBarPicker::LineEnding
            | CommandBarPicker::Encoding
            | CommandBarPicker::EncodingReopen
            | CommandBarPicker::EncodingSave => "editor-status-pick-placeholder",
        }
    }
}

impl PaletteRows {
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

    pub(super) fn prompt_targets(
        pages: &[CommandBarPage],
        query: &str,
    ) -> Vec<CommandBarResultItem> {
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

    pub(super) fn prompt_target_url(item: &CommandBarResultItem) -> Option<&str> {
        match item {
            CommandBarResultItem::Page {
                url,
                prompt_target: true,
                ..
            } => Some(url),
            _ => None,
        }
    }

    pub(super) fn prompt_target_matches(item: &CommandBarResultItem, query: &str) -> bool {
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

    pub(super) fn terminal_matches(query: &str) -> bool {
        let query = query.trim().to_lowercase();
        !query.is_empty() && "terminal".starts_with(&query)
    }

    pub(super) fn prepend_targets(
        results: &mut Vec<CommandBarResultItem>,
        selected_target: Option<&CommandBarResultItem>,
        recent_targets: &[CommandBarResultItem],
        query: &str,
    ) {
        if !PaletteQuery::new(query).is_start_prompt()
            || results
                .iter()
                .any(|item| PaletteRows::prompt_target_url(item).is_some())
        {
            return;
        }
        let mut suggestions = Vec::new();
        for target in selected_target.into_iter().chain(recent_targets) {
            let Some(url) = PaletteRows::prompt_target_url(target) else {
                continue;
            };
            if suggestions
                .iter()
                .any(|existing| PaletteRows::prompt_target_url(existing) == Some(url))
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

    pub(super) fn open_sessions(
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

    pub(super) fn start(
        pages: &[CommandBarPage],
        work_dirs: &[CommandBarWorkDir],
        recent_files: &[CommandBarRecentFile],
        search_engines: &[SearchEngine],
        terminal_page_url: &str,
        query: &str,
    ) -> Vec<CommandBarResultItem> {
        let search_lower = query.trim().to_lowercase();
        let mut results = Vec::new();
        if PaletteRows::terminal_matches(query) {
            results.push(CommandBarResultItem::Terminal {
                path: String::new(),
            });
        }
        results.extend(
            PaletteRows::prompt_targets(pages, query)
                .into_iter()
                .filter(|item| PaletteRows::prompt_target_matches(item, query)),
        );
        let trimmed = query.trim();
        if PaletteQuery::new(trimmed).is_start_prompt() {
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
        }
        let mut app_pages: Vec<_> = pages
            .iter()
            .filter(|page| {
                !page.prompt_target
                    && !page.startup
                    && !Self::urls_match(&page.url, terminal_page_url)
            })
            .filter(|page| Self::page_matches(page, &search_lower))
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
        results.extend(Self::work_dir_results(work_dirs, &search_lower));
        results.extend(Self::recent_file_results(recent_files, &search_lower));
        if !PaletteQuery::new(trimmed).is_start_prompt() && !trimmed.is_empty() {
            results.push(CommandBarResultItem::Navigate {
                url: trimmed.to_string(),
                is_url: PaletteQuery::new(trimmed).looks_like_url(),
            });
        }
        results
    }

    fn work_dir_results(
        dirs: &[CommandBarWorkDir],
        search_lower: &str,
    ) -> Vec<CommandBarResultItem> {
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

    fn space_list_items(
        spaces: &[CommandBarSpace],
        search_lower: &str,
    ) -> Vec<CommandBarResultItem> {
        spaces
            .iter()
            .filter(|space| Self::space_matches(space, search_lower))
            .map(Self::space_result)
            .collect()
    }

    pub(super) fn space_switch(
        spaces: &[CommandBarSpace],
        pages: &[CommandBarPage],
        spaces_page_url: &str,
        query: &str,
    ) -> Vec<CommandBarResultItem> {
        let search_lower = query.trim().to_lowercase();
        let mut items = Self::space_list_items(spaces, &search_lower);
        if let Some(page) = pages
            .iter()
            .find(|page| Self::urls_match(&page.url, spaces_page_url))
        {
            items.push(CommandBarResultItem::Page {
                url: page.url.clone(),
                title: translate("command-manage-spaces"),
                icon: page.icon.clone(),
                shortcut: String::new(),
                prompt_target: false,
                prompt_hint: false,
            });
        }
        items
    }

    fn query_targets_spaces_page(q: &str, spaces_page_url: &str) -> bool {
        if spaces_page_url.is_empty() {
            return false;
        }
        q == spaces_page_url
            || q == spaces_page_url.trim_end_matches('/')
            || q.starts_with(spaces_page_url)
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
    pub(super) fn filter(
        query: &str,
        tabs: &[CommandBarTab],
        commands: &[CommandBarCommandEntry],
        spaces: &[CommandBarSpace],
        pages: &[CommandBarPage],
        new_tab: bool,
        history: &[HistoryEntry],
        work_dirs: &[CommandBarWorkDir],
        recent_files: &[CommandBarRecentFile],
        spaces_page_url: &str,
    ) -> Vec<CommandBarResultItem> {
        let q = query.trim();

        if Self::query_targets_spaces_page(q, spaces_page_url) {
            let mut items = Self::page_results(pages, &q.to_lowercase());
            items.extend(Self::space_list_items(spaces, ""));
            items.extend(Self::command_results(commands));
            return items;
        }

        if q.is_empty() {
            let mut items: Vec<CommandBarResultItem> = Vec::new();
            items.push(CommandBarResultItem::Navigate {
                url: String::new(),
                is_url: false,
            });
            if new_tab {
                items.push(CommandBarResultItem::Terminal {
                    path: String::new(),
                });
            }
            items.extend(tabs.iter().filter(|t| !t.is_active).map(|t| {
                CommandBarResultItem::Stack {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    icon: Self::stack_icon_for(pages, &t.url),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index as usize,
                    location: t.location.clone(),
                }
            }));
            items.extend(Self::page_results(pages, ""));
            items.extend(Self::work_dir_results(work_dirs, ""));
            items.extend(Self::recent_file_results(recent_files, ""));
            items.extend(Self::command_results(commands));
            return items;
        }

        let starts_with_cmd = q.starts_with('>');
        let search = if starts_with_cmd { q[1..].trim() } else { q };
        let search_lower = search.to_lowercase();

        let mut items = Vec::new();

        let is_path = PaletteQuery::new(search).looks_like_path();

        if !starts_with_cmd && is_path {
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
            items.extend(Self::page_results(pages, &search_lower));
            items.extend(Self::space_list_items(spaces, &search_lower));
            items.extend(Self::work_dir_results(work_dirs, &search_lower));
            items.extend(Self::recent_file_results(recent_files, &search_lower));
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
                        icon: Self::stack_icon_for(pages, &t.url),
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

    const SPACES_PAGE_URL: &str = "vmux://spaces/";
    const TERMINAL_PAGE_URL: &str = "vmux://terminal/";

    fn resume(title: &str, agent: &str, project: &str, branch: &str) -> ResumableSessionEntry {
        ResumableSessionEntry {
            title: title.into(),
            agent_name: agent.into(),
            project: project.into(),
            branch: branch.into(),
            ..Default::default()
        }
    }

    #[test]
    fn resume_rows_group_sessions_under_shared_context() {
        let rows = ResumeRows::all(&[
            resume("first", "Codex", "vmux", "main"),
            resume("second", "Claude", "vmux", "main"),
            resume("third", "Codex", "vmux", "main"),
        ]);

        let CommandBarResultItem::Resume {
            entry: first,
            section: first_section,
        } = &rows[0]
        else {
            panic!("first resume row");
        };
        let CommandBarResultItem::Resume {
            entry: second,
            section: second_section,
        } = &rows[1]
        else {
            panic!("second resume row");
        };
        let CommandBarResultItem::Resume {
            entry: third,
            section: third_section,
        } = &rows[2]
        else {
            panic!("third resume row");
        };

        assert_eq!(first.title, "first");
        assert_eq!(second.title, "third");
        assert_eq!(third.title, "second");
        assert_eq!(first_section.as_ref().unwrap().agent, "Codex");
        assert!(second_section.is_none());
        assert_eq!(third_section.as_ref().unwrap().agent, "Claude");
    }

    #[test]
    fn resume_search_matches_section_context() {
        let rows = ResumeRows::filtered(
            "feature-x",
            &[
                resume("one", "Codex", "vmux", "feature-x"),
                resume("two", "Claude", "dashboard", "main"),
            ],
        );

        assert_eq!(rows.len(), 1);
        assert!(matches!(
            &rows[0],
            CommandBarResultItem::Resume { entry, .. } if entry.title == "one"
        ));
    }

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
    fn space_switch_lists_spaces_in_order_then_manage() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("work", "Work", true),
        ];
        let results = PaletteRows::space_switch(&spaces, &sample_pages(), SPACES_PAGE_URL, "");
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
        let results = PaletteRows::space_switch(&spaces, &sample_pages(), SPACES_PAGE_URL, "wor");
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
    fn spaces_url_lists_all_spaces() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("work", "Work", true),
        ];

        let results = PaletteRows::filter(
            "vmux://spaces/",
            &[],
            &[] as &[CommandBarCommandEntry],
            &spaces,
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Layers),
            shortcut: String::new(),
            prompt_target: false,
            prompt_hint: false,
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

        let results = PaletteRows::filter(
            "vmux://spaces/",
            &[],
            &commands,
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Layers),
            shortcut: String::new(),
            prompt_target: false,
            prompt_hint: false,
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

        let results = PaletteRows::filter(
            "spaces",
            &[],
            &commands,
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );

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
    fn space_names_are_searchable() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("client", "Client Work", false),
        ];
        let tabs: Vec<CommandBarTab> = Vec::new();

        let results = PaletteRows::filter(
            "client",
            &tabs,
            &[],
            &spaces,
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );

        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "client"
        )));
    }

    #[test]
    fn page_matched_by_keyword() {
        let results = PaletteRows::filter(
            "preferences",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );
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
        let results = PaletteRows::filter(
            "vmux://",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { url, icon, .. }
                if url == "vmux://sessions/vibe/" && matches!(icon, vmux_api::PageIcon::None)
        )));
    }

    #[test]
    fn agent_page_matched_by_name() {
        let results = PaletteRows::filter(
            "vibe",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );
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

        let results = PaletteRows::prompt_targets(&pages, "");
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

        let results = PaletteRows::prompt_targets(&pages, "vibe");

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
        let codex = PaletteRows::prompt_targets(&pages, "cod").remove(0);

        assert!(PaletteRows::prompt_target_matches(&codex, "cod"));
        assert!(PaletteRows::prompt_target_matches(&codex, "codex"));
        assert!(PaletteRows::prompt_target_matches(&codex, "codex-acp"));
        assert!(!PaletteRows::prompt_target_matches(
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

        let results = PaletteRows::prompt_targets(&pages, "show me something fun in terminal");
        let urls: Vec<_> = results
            .iter()
            .filter_map(PaletteRows::prompt_target_url)
            .collect();

        assert_eq!(
            urls,
            vec!["vmux://sessions/vibe/", "vmux://sessions/codex/cli"]
        );
    }

    #[test]
    fn start_page_does_not_show_unmatched_agents() {
        let results = PaletteRows::start(
            &sample_pages(),
            &[],
            &[],
            &[],
            TERMINAL_PAGE_URL,
            "settings",
        );
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
        let results = PaletteRows::start(
            &sample_pages(),
            &[],
            &[],
            &engines,
            TERMINAL_PAGE_URL,
            "fix the failing test",
        );
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
    fn start_page_puts_web_search_before_matching_files() {
        let work_dirs = [CommandBarWorkDir {
            path: "/work/failing test".into(),
            is_dir: true,
        }];
        let recent_files = [CommandBarRecentFile {
            url: "file:///work/failing%20test.txt".into(),
            title: "failing test.txt".into(),
        }];
        let results = PaletteRows::start(
            &sample_pages(),
            &work_dirs,
            &recent_files,
            &[SearchEngine::Google],
            TERMINAL_PAGE_URL,
            "failing test",
        );
        let search = results
            .iter()
            .position(|item| matches!(item, CommandBarResultItem::Search { .. }))
            .unwrap();
        let work_dir = results
            .iter()
            .position(|item| matches!(item, CommandBarResultItem::WorkDir { .. }))
            .unwrap();
        let recent_file = results
            .iter()
            .position(|item| matches!(item, CommandBarResultItem::RecentFile { .. }))
            .unwrap();

        assert!(search < work_dir);
        assert!(search < recent_file);
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
        let agents = PaletteRows::prompt_targets(&pages, "");
        let selected = agents[1].clone();
        let mut results = PaletteRows::start(
            &pages,
            &[],
            &[],
            &[SearchEngine::Google, SearchEngine::Bing],
            TERMINAL_PAGE_URL,
            "show me something fun",
        );

        PaletteRows::prepend_targets(
            &mut results,
            Some(&selected),
            &agents,
            "show me something fun",
        );

        assert_eq!(
            PaletteRows::prompt_target_url(&results[0]),
            Some("vmux://sessions/codex/cli")
        );
        assert_eq!(
            PaletteRows::prompt_target_url(&results[1]),
            Some("vmux://sessions/vibe/")
        );
        assert_eq!(
            PaletteRows::prompt_target_url(&results[2]),
            Some("vmux://sessions/claude")
        );
        assert!(matches!(results[3], CommandBarResultItem::Search { .. }));
    }

    #[test]
    fn terminal_leads_but_agents_and_search_stay_available() {
        let agent = PaletteRows::prompt_targets(&sample_pages(), "").remove(0);
        let mut results = PaletteRows::start(
            &sample_pages(),
            &[],
            &[],
            &[],
            TERMINAL_PAGE_URL,
            "terminal",
        );

        PaletteRows::prepend_targets(&mut results, Some(&agent), &[], "terminal");

        assert!(
            matches!(results.first(), Some(CommandBarResultItem::Terminal { .. })),
            "terminal keeps the default selection: {results:?}"
        );
        assert!(
            results
                .iter()
                .any(|item| PaletteRows::prompt_target_url(item).is_some()),
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
        let results = PaletteRows::start(&sample_pages(), &[], &[], &[], TERMINAL_PAGE_URL, "ter");
        assert!(matches!(
            results.first(),
            Some(CommandBarResultItem::Terminal { .. })
        ));
        assert!(results.len() > 1, "prefix match is not the only option");
    }

    #[test]
    fn terminal_query_predicate_matches_display_and_activation() {
        assert!(PaletteRows::terminal_matches("t"));
        assert!(PaletteRows::terminal_matches("ter"));
        assert!(PaletteRows::terminal_matches("Terminal"));
        assert!(PaletteRows::terminal_matches("  term  "));
        assert!(!PaletteRows::terminal_matches(""));
        assert!(!PaletteRows::terminal_matches("   "));
        assert!(!PaletteRows::terminal_matches("terminals"));
        assert!(!PaletteRows::terminal_matches("xterm"));
    }

    #[test]
    fn start_page_suggests_terminal_by_name() {
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
        let results = PaletteRows::start(&pages, &[], &[], &[], TERMINAL_PAGE_URL, "terminal");
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
        let dir_results = PaletteRows::start(
            &sample_pages(),
            &work_dirs,
            &recent_files,
            &[],
            TERMINAL_PAGE_URL,
            "vmux",
        );
        assert!(dir_results.iter().any(|result| matches!(
            result,
            CommandBarResultItem::WorkDir { path, .. } if path == "/work/vmux"
        )));
        let file_results = PaletteRows::start(
            &sample_pages(),
            &work_dirs,
            &recent_files,
            &[],
            TERMINAL_PAGE_URL,
            "readme",
        );
        assert!(file_results.iter().any(|result| matches!(
            result,
            CommandBarResultItem::RecentFile { url, .. }
                if url == "file:///work/vmux/README.md"
        )));
    }

    #[test]
    fn prompt_agent_url_only_accepts_agent_page_rows() {
        let agent = PaletteRows::prompt_targets(&sample_pages(), "").remove(0);
        let settings = CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: vmux_api::PageIcon::None,
            shortcut: String::new(),
            prompt_target: false,
            prompt_hint: false,
        };

        assert_eq!(
            PaletteRows::prompt_target_url(&agent),
            Some("vmux://sessions/vibe/")
        );
        assert_eq!(PaletteRows::prompt_target_url(&settings), None);
    }

    #[test]
    fn settings_page_reachable_by_name() {
        let results = PaletteRows::filter(
            "setti",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
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

        let results = PaletteRows::filter(
            "",
            &[],
            &commands,
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
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
        let results = PaletteRows::filter(
            "",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );
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
        let results = PaletteRows::filter(
            "history",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        );
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, shortcut, .. }
                if title == "History" && shortcut == "\u{2318}Y"
        )));
    }

    #[test]
    fn command_prefix_excludes_pages() {
        let results = PaletteRows::filter(
            "> set",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
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
        let results = PaletteRows::filter(
            "",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &sample_work_dirs(),
            &sample_recent_files(),
            SPACES_PAGE_URL,
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
        PaletteRows::filter(
            query,
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &[],
            &[],
            SPACES_PAGE_URL,
        )
    }

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

    #[test]
    fn the_editor_row_carries_the_path_that_was_typed() {
        assert!(path_results("~/notes.md").iter().any(|r| matches!(
            r, CommandBarResultItem::Editor { path } if path == "~/notes.md"
        )));
    }

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
        let results = PaletteRows::filter(
            "proj",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &sample_work_dirs(),
            &sample_recent_files(),
            SPACES_PAGE_URL,
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

        let items = PaletteRows::open_sessions(&tabs, &[]);

        assert_eq!(items.len(), 1, "the stack already on screen is not offered");
        assert!(matches!(
            &items[0],
            CommandBarResultItem::Stack { title, pane_id, .. }
                if title == "Docs" && *pane_id == 8
        ));
        assert!(PaletteRows::open_sessions(&[], &[]).is_empty());
    }
}
