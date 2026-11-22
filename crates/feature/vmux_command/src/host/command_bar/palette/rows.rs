use super::{PaletteDraftInput, PaletteOpen, PaletteSnapshot};
use crate::CommandPaletteSurface;
use bevy::prelude::Component;
use vmux_api::chat::SlashCommandEntry;
use vmux_api::command_bar::{
    AgentModels, AgentModes, CommandBarCommandEntry, CommandBarOpenEvent, CommandBarPage,
    CommandBarPick, CommandBarPickRow, CommandBarPicker, CommandBarResultItem, CommandBarTab,
    CommandPaletteAgent, CommandPaletteComposer, CommandPaletteProjection, HistoryEntry,
    PaletteGlyph, PaletteMode, PathEntry, SearchEngine,
};
use vmux_api::open_target::OpenTarget;
use vmux_api::space::ProjectRow;
use vmux_api::{BuiltinIcon, PageIcon, VmuxRoute};
use vmux_path::{NavigationText, UrlPresentation};
use vmux_ui::favicon::FaviconSource;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PaletteRowKind {
    Pick,
    Stack,
    Command,
    Page,
    Navigate,
    Search,
    File,
    History,
    Slash,
    #[default]
    Disabled,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PaletteRow {
    pub(super) kind: PaletteRowKind,
    pub(super) display: CommandBarResultItem,
    pub(super) pick: Option<CommandBarPick>,
    pub(super) tab: Option<(u64, usize)>,
    pub(super) command_id: String,
    pub(super) command_name: String,
    pub(super) slash_name: String,
    pub(super) search_query: String,
    pub(super) page_title: String,
    pub(super) page_icon: PageIcon,
    pub(super) page_shortcut: String,
    pub(super) prompt_target: bool,
    pub(super) prompt_hint: bool,
    pub(super) is_url: bool,
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub(super) struct PaletteRowsSnapshot {
    pub(super) items: Vec<PaletteRow>,
    pub(super) prompt_targets: Vec<PaletteRow>,
    pub(super) default_target: Option<PaletteRow>,
    pub(super) ghost: String,
    pub(super) start_prompt_mode: bool,
    pub(super) numbered: bool,
    pub(super) mode: PaletteMode,
}

impl PaletteRowsSnapshot {
    pub(super) fn build(
        opened: &PaletteOpen,
        draft: &PaletteDraftInput,
        snapshot: &PaletteSnapshot,
        query: &str,
    ) -> Self {
        let state = &opened.0;
        let is_start = draft.surface.is_start();
        let mode = opened.mode(query);
        let prompt_targets = Self::prompt_targets(&state.pages, "");
        let default_target = prompt_targets
            .iter()
            .find(|item| Self::prompt_target_url(item) == Some(draft.target_url.as_str()))
            .cloned()
            .or_else(|| prompt_targets.first().cloned());
        let prompt_mode =
            matches!(mode, PaletteMode::Search) && PaletteDraftInput::is_prompt(query);
        let start_prompt_mode = is_start && prompt_mode;

        let mut items = Self::collect_items(
            state,
            snapshot,
            query,
            draft.surface,
            &mode,
            start_prompt_mode,
        );
        if start_prompt_mode {
            Self::prepend_targets(&mut items, default_target.as_ref(), &prompt_targets, query);
        } else if prompt_mode {
            Self::insert_target(&mut items, default_target.as_ref(), query);
        }
        for item in &mut items {
            let show_hint = prompt_mode
                && Self::prompt_target_url(item).is_some()
                && !Self::prompt_target_matches(item, query);
            if item.kind == PaletteRowKind::Page && item.prompt_hint != show_hint {
                Self::apply_prompt_hint(item, show_hint);
            }
        }

        Self {
            items,
            prompt_targets,
            default_target,
            ghost: String::new(),
            start_prompt_mode,
            numbered: state.picker_numbered,
            mode,
        }
    }

    pub(super) fn selected(&self, stored: usize) -> usize {
        stored.min(self.items.len().saturating_sub(1))
    }

    pub(super) fn effective_target(&self, selected: usize) -> Option<&PaletteRow> {
        self.items
            .get(selected)
            .filter(|item| Self::prompt_target_url(item).is_some())
            .or(self.default_target.as_ref())
    }

    pub(super) fn project_ui(
        &self,
        opened: &PaletteOpen,
        input: &PaletteDraftInput,
    ) -> (CommandPaletteProjection, CommandPaletteComposer) {
        let selected = self.selected(input.selected);
        let active = self.items.get(selected);
        let navigating = if input.navigating { active } else { None };
        let effective_target = self.effective_target(selected);
        let composer = opened.build_composer(&self.prompt_targets, effective_target);
        let accent_agent = Self::accent_agent(&composer);
        let row_text = if self.start_prompt_mode {
            None
        } else {
            Self::row_text_over(navigating, &input.query)
        };
        let mut rows = Vec::with_capacity(self.items.len());
        for item in &self.items {
            rows.push(item.display.clone());
        }
        let mut prompt_targets = Vec::with_capacity(self.prompt_targets.len());
        for item in &self.prompt_targets {
            prompt_targets.push(item.display.clone());
        }
        let projection = CommandPaletteProjection {
            query: input.query.clone(),
            rows,
            selected: selected as u32,
            navigating: input.navigating,
            row_text,
            placeholder: opened.placeholder(&self.mode, input.surface),
            glyph: Self::glyph_for(navigating, &self.mode, self.numbered),
            numbered: self.numbered,
            numbered_count: 0,
            context_label: opened.0.context_label.clone(),
            accent_agent,
            composer: composer.clone(),
            prompt_targets,
            default_target: self.default_target.as_ref().map(|row| row.display.clone()),
            ghost: self.ghost.clone(),
            start_prompt_mode: self.start_prompt_mode,
            mode: self.mode.clone(),
            mode_label: opened.0.picker_label.clone(),
            ..Default::default()
        };
        (projection, composer)
    }

    pub(super) fn accepts_typed(navigating: bool, item: &PaletteRow, query: &str) -> bool {
        navigating || Self::prompt_target_matches(item, query)
    }

    pub(super) fn should_activate_selected(
        &self,
        opened: &PaletteOpen,
        input: &PaletteDraftInput,
        clamped: usize,
    ) -> bool {
        let Some(selected_row) = self.items.get(clamped) else {
            return false;
        };
        if input.surface.is_start() {
            if self.mode == PaletteMode::Slash && self.items.is_empty() {
                return false;
            }
            if input.query.trim().is_empty() {
                return false;
            }
            if self.numbered {
                return true;
            }
            if self.start_prompt_mode {
                return Self::accepts_typed(input.navigating, selected_row, &input.query);
            }
        } else if matches!(self.mode, PaletteMode::Picking(_)) {
            return !opened.0.picker_typed;
        } else if matches!(self.mode, PaletteMode::Ex) {
            return input.navigating;
        }
        Self::row_beats_guessed_url(Some(selected_row), &input.query)
            || !PaletteDraftInput::opens_typed_url_on_enter(
                &input.query,
                opened.0.target,
                input.navigating,
            )
    }

    pub(super) fn row_beats_guessed_url(row: Option<&PaletteRow>, query: &str) -> bool {
        let query = query.trim();
        let Some(row) = row else {
            return false;
        };
        match row.kind {
            PaletteRowKind::Page => {
                query.starts_with("vmux://") && row.display.url.starts_with(query)
            }
            PaletteRowKind::File => {
                if row.display.directory || query.contains("://") {
                    return false;
                }
                let name = row
                    .display
                    .file_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&row.display.file_path);
                name.eq_ignore_ascii_case(query)
            }
            _ => false,
        }
    }

    pub(super) fn default_prompt_target(&self) -> &str {
        self.default_target
            .as_ref()
            .and_then(Self::prompt_target_url)
            .unwrap_or_default()
    }

    pub(super) fn prompt_target_url(item: &PaletteRow) -> Option<&str> {
        if item.kind == PaletteRowKind::Page && item.prompt_target {
            Some(item.display.url.as_str())
        } else {
            None
        }
    }

    pub(super) fn prompt_target_matches(item: &PaletteRow, query: &str) -> bool {
        if item.kind != PaletteRowKind::Page || !item.prompt_target {
            return false;
        }
        let search_lower = query.trim().to_lowercase();
        !search_lower.is_empty()
            && (item.page_title.to_lowercase().contains(&search_lower)
                || item.display.url.to_lowercase().contains(&search_lower))
    }

    fn collect_items(
        state: &CommandBarOpenEvent,
        snapshot: &PaletteSnapshot,
        query: &str,
        surface: CommandPaletteSurface,
        mode: &PaletteMode,
        start_prompt_mode: bool,
    ) -> Vec<PaletteRow> {
        let is_start = surface.is_start();
        if let PaletteMode::Picking(_) = mode {
            return Self::picker_rows(state.picker_typed, &state.picks, query);
        }
        if matches!(mode, PaletteMode::Ex) {
            return Vec::new();
        }
        if mode == &PaletteMode::Slash {
            return Self::slash_rows(query, &state.prompt_context.slash_commands);
        }
        if PaletteDraftInput::only_files(query) {
            return Self::merge_file_rows(
                query,
                &snapshot.completions,
                snapshot.completions_partial,
                snapshot.completions_total as usize,
                Vec::new(),
            );
        }
        if is_start && query.trim().is_empty() {
            return Vec::new();
        }
        if start_prompt_mode {
            return Self::prompt_rows(&state.pages, &state.search_engines, query);
        }
        let matched = Self::search_rows(
            query,
            &state.tabs,
            &state.commands,
            &state.pages,
            &snapshot.history,
        );
        if !is_start {
            return matched;
        }
        let startup_url = state
            .pages
            .iter()
            .find(|page| page.startup)
            .map(|page| page.url.trim_end_matches('/'));
        let mut kept = Vec::with_capacity(matched.len());
        for item in matched {
            if matches!(item.kind, PaletteRowKind::Stack | PaletteRowKind::Page)
                && startup_url == Some(item.display.url.trim_end_matches('/'))
            {
                continue;
            }
            kept.push(item);
        }
        kept
    }

    fn pick_row(label: String, pick: CommandBarPick) -> PaletteRow {
        let display = CommandBarResultItem {
            title: label,
            trailing: "\u{21b5}".to_string(),
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Pick,
            display,
            pick: Some(pick),
            ..Default::default()
        }
    }

    fn stack_row(tab: &CommandBarTab, icon: PageIcon) -> PaletteRow {
        let trailing = if tab.location.is_empty() {
            translate("command-stack")
        } else {
            tab.location.clone()
        };
        let display = CommandBarResultItem {
            title: tab.title.clone(),
            subtitle: tab.url.clone(),
            url: tab.url.clone(),
            icon,
            trailing,
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Stack,
            display,
            tab: Some((tab.pane_id, tab.tab_index as usize)),
            ..Default::default()
        }
    }

    fn command_row(command: &CommandBarCommandEntry) -> PaletteRow {
        let display = CommandBarResultItem {
            leading: ">_".to_string(),
            title: command.name.clone(),
            trailing: command.shortcut.clone(),
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Command,
            display,
            command_id: command.id.clone(),
            command_name: command.name.clone(),
            ..Default::default()
        }
    }

    fn page_row(page: &CommandBarPage, prompt_target: bool) -> PaletteRow {
        let trailing = if page.shortcut.is_empty() {
            translate("command-new-tab")
        } else {
            page.shortcut.clone()
        };
        let display = CommandBarResultItem {
            title: page.title.clone(),
            subtitle: page.url.clone(),
            url: page.url.clone(),
            icon: page.icon.clone(),
            trailing,
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Page,
            display,
            page_title: page.title.clone(),
            page_icon: page.icon.clone(),
            page_shortcut: page.shortcut.clone(),
            prompt_target,
            prompt_hint: false,
            ..Default::default()
        }
    }

    fn apply_prompt_hint(row: &mut PaletteRow, prompt_hint: bool) {
        row.prompt_hint = prompt_hint;
        if prompt_hint {
            row.display.title = format!("Ask {}", row.page_title);
            row.display.subtitle.clear();
            row.display.trailing = translate("command-prompt");
        } else {
            row.display.title.clone_from(&row.page_title);
            row.display.subtitle.clone_from(&row.display.url);
            row.display.trailing = if row.page_shortcut.is_empty() {
                translate("command-new-tab")
            } else {
                row.page_shortcut.clone()
            };
        }
    }

    fn navigate_row(url: String, is_url: bool) -> PaletteRow {
        let icon = PageIcon::Builtin(if is_url {
            BuiltinIcon::Globe
        } else {
            BuiltinIcon::Search
        });
        let title = if url.is_empty() {
            translate("command-search")
        } else if is_url {
            translate_with(
                "command-open-value",
                &[(
                    "value",
                    TranslationValue::String(UrlPresentation::new(&url).text()),
                )],
            )
        } else {
            translate_with(
                "command-search-value",
                &[("value", TranslationValue::String(&url))],
            )
        };
        let trailing = if url.is_empty() {
            String::new()
        } else {
            "\u{21b5}".to_string()
        };
        let display = CommandBarResultItem {
            title,
            url,
            icon,
            trailing,
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Navigate,
            display,
            is_url,
            ..Default::default()
        }
    }

    fn search_row(engine: &SearchEngine, query: &str) -> PaletteRow {
        let url = engine.query_url(query);
        let favicon_url = FaviconSource::resolve("", &url).unwrap_or_default();
        let display = CommandBarResultItem {
            title: format!("Search with {}", engine.name),
            url,
            icon: PageIcon::Builtin(BuiltinIcon::Globe),
            favicon_url,
            trailing: "\u{21b5}".to_string(),
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Search,
            display,
            search_query: query.to_string(),
            ..Default::default()
        }
    }

    fn file_row(entry: &PathEntry) -> PaletteRow {
        let title = entry
            .full_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&entry.full_path)
            .to_string();
        let shown = if entry.project.is_empty() {
            entry.full_path.as_str()
        } else {
            entry.name.as_str()
        };
        let subtitle = shown
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default();
        let trailing = if entry.is_dir {
            String::new()
        } else {
            "\u{21b5}".to_string()
        };
        let display = CommandBarResultItem {
            title,
            subtitle,
            badge: entry.project.clone(),
            file_path: entry.full_path.clone(),
            directory: entry.is_dir,
            trailing,
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::File,
            display,
            ..Default::default()
        }
    }

    fn history_row(entry: &HistoryEntry) -> PaletteRow {
        let title = if entry.title.is_empty() {
            entry.url.clone()
        } else {
            entry.title.clone()
        };
        let display = CommandBarResultItem {
            title,
            subtitle: entry.url.clone(),
            url: entry.url.clone(),
            favicon_url: entry.favicon_url.clone(),
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::History,
            display,
            ..Default::default()
        }
    }

    fn slash_row(name: String, hint: String) -> PaletteRow {
        let display = CommandBarResultItem {
            leading: "/".to_string(),
            title: name.clone(),
            subtitle: hint,
            trailing: "\u{21b5}".to_string(),
            ..Default::default()
        };
        PaletteRow {
            kind: PaletteRowKind::Slash,
            display,
            slash_name: name,
            ..Default::default()
        }
    }

    fn partial_index_row() -> PaletteRow {
        PaletteRow {
            kind: PaletteRowKind::Disabled,
            display: CommandBarResultItem {
                leading: "!".to_string(),
                title: translate("command-partial-index"),
                disabled: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn more_matches_row(shown: usize, total: usize) -> PaletteRow {
        PaletteRow {
            kind: PaletteRowKind::Disabled,
            display: CommandBarResultItem {
                leading: "+".to_string(),
                title: translate_with(
                    "command-more-matches",
                    &[
                        ("shown", TranslationValue::Number(shown as i64)),
                        ("total", TranslationValue::Number(total as i64)),
                    ],
                ),
                disabled: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn slash_rows(query: &str, commands: &[SlashCommandEntry]) -> Vec<PaletteRow> {
        let (name, _) = match PaletteDraftInput::slash_token(query) {
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
            rows.push(Self::slash_row(
                command.command.name().to_string(),
                command.description.clone(),
            ));
        }
        rows
    }

    fn picker_rows(typed: bool, picks: &[CommandBarPickRow], query: &str) -> Vec<PaletteRow> {
        if typed {
            return Vec::new();
        }
        let needle = query.trim().to_lowercase();
        let mut rows = Vec::with_capacity(picks.len());
        for row in picks {
            if !needle.is_empty() && !row.label.to_lowercase().contains(&needle) {
                continue;
            }
            rows.push(Self::pick_row(row.label.clone(), row.pick.clone()));
        }
        rows
    }

    fn merge_file_rows(
        query: &str,
        entries: &[PathEntry],
        partial: bool,
        total: usize,
        matched: Vec<PaletteRow>,
    ) -> Vec<PaletteRow> {
        if PaletteDraftInput::parse_completion_query(query).is_none() {
            return matched;
        }
        if entries.is_empty() && !partial {
            return matched;
        }
        let trimmed = query.trim();
        let leads = NavigationText::new(trimmed).looks_like_path()
            || PaletteDraftInput::names_a_file(trimmed);
        let mut files = Vec::with_capacity(entries.len());
        let mut listed = Vec::with_capacity(entries.len());
        for entry in entries {
            files.push(Self::file_row(entry));
            listed.push(entry.full_path.as_str());
        }
        let mut rest = Vec::with_capacity(matched.len());
        for item in matched {
            let duplicate = item.kind == PaletteRowKind::History
                && item
                    .display
                    .url
                    .strip_prefix("file://")
                    .is_some_and(|path| listed.contains(&path));
            if !duplicate {
                rest.push(item);
            }
        }
        let mut merged = if leads {
            files.extend(rest);
            files
        } else {
            let mut ahead = Vec::with_capacity(rest.len());
            let mut fallbacks = Vec::new();
            for item in rest {
                if item.kind == PaletteRowKind::Search {
                    fallbacks.push(item);
                } else {
                    ahead.push(item);
                }
            }
            ahead.extend(files);
            ahead.extend(fallbacks);
            ahead
        };
        if partial {
            merged.push(Self::partial_index_row());
        }
        if total > entries.len() {
            merged.push(Self::more_matches_row(entries.len(), total));
        }
        merged
    }

    fn page_matches(page: &CommandBarPage, search_lower: &str) -> bool {
        search_lower.is_empty()
            || page.title.to_lowercase().contains(search_lower)
            || page.url.to_lowercase().contains(search_lower)
            || page
                .keywords
                .iter()
                .any(|keyword| keyword.to_lowercase().contains(search_lower))
    }

    fn page_results(pages: &[CommandBarPage], search_lower: &str) -> Vec<PaletteRow> {
        let mut matched: Vec<&CommandBarPage> = pages
            .iter()
            .filter(|page| Self::page_matches(page, search_lower))
            .collect();
        matched.sort_by_key(|page| page.url.to_lowercase());
        matched
            .into_iter()
            .map(|page| Self::page_row(page, false))
            .collect()
    }

    fn prompt_targets(pages: &[CommandBarPage], query: &str) -> Vec<PaletteRow> {
        let search_lower = query.trim().to_lowercase();
        let targets: Vec<&CommandBarPage> =
            pages.iter().filter(|page| page.prompt_target).collect();
        let matches: Vec<&CommandBarPage> = targets
            .iter()
            .copied()
            .filter(|page| Self::page_matches(page, &search_lower))
            .collect();
        let visible = if matches.is_empty() { targets } else { matches };
        visible
            .into_iter()
            .map(|page| Self::page_row(page, true))
            .collect()
    }

    fn insert_target(
        results: &mut Vec<PaletteRow>,
        selected_target: Option<&PaletteRow>,
        query: &str,
    ) {
        let Some(target) = selected_target else {
            return;
        };
        if Self::prompt_target_matches(target, query) {
            return;
        }
        let at = results
            .iter()
            .position(|item| matches!(item.kind, PaletteRowKind::Navigate | PaletteRowKind::Search))
            .unwrap_or(results.len());
        results.insert(at, target.clone());
    }

    fn prepend_targets(
        results: &mut Vec<PaletteRow>,
        selected_target: Option<&PaletteRow>,
        recent_targets: &[PaletteRow],
        query: &str,
    ) {
        if !PaletteDraftInput::is_prompt(query)
            || results
                .iter()
                .any(|item| Self::prompt_target_url(item).is_some())
        {
            return;
        }
        let mut suggestions = Vec::new();
        for target in selected_target.into_iter().chain(recent_targets) {
            let Some(url) = Self::prompt_target_url(target) else {
                continue;
            };
            if suggestions
                .iter()
                .any(|existing| Self::prompt_target_url(existing) == Some(url))
            {
                continue;
            }
            suggestions.push(target.clone());
            if suggestions.len() == 3 {
                break;
            }
        }
        let mut leading = Vec::new();
        let mut rest = Vec::new();
        for target in suggestions {
            if leading.is_empty() {
                leading.push(target);
            } else {
                rest.push(target);
            }
        }
        let mut after = leading.len();
        results.splice(0..0, leading);
        for (index, item) in results.iter().enumerate() {
            if item.kind == PaletteRowKind::File {
                after = index + 1;
            }
        }
        let tail = results.split_off(after);
        results.extend(rest);
        results.extend(tail);
    }

    fn prompt_rows(
        pages: &[CommandBarPage],
        search_engines: &[SearchEngine],
        query: &str,
    ) -> Vec<PaletteRow> {
        let search_lower = query.trim().to_lowercase();
        let mut results = Vec::new();
        for target in Self::prompt_targets(pages, query) {
            if Self::prompt_target_matches(&target, query) {
                results.push(target);
            }
        }
        let trimmed = query.trim();
        if PaletteDraftInput::is_prompt(trimmed) {
            for engine in search_engines.iter().take(3) {
                results.push(Self::search_row(engine, trimmed));
            }
        }
        let mut app_pages: Vec<&CommandBarPage> = pages
            .iter()
            .filter(|page| !page.prompt_target && !page.startup)
            .filter(|page| Self::page_matches(page, &search_lower))
            .collect();
        app_pages.sort_by_cached_key(|page| page.url.to_lowercase());
        for page in app_pages {
            results.push(Self::page_row(page, false));
        }
        if !PaletteDraftInput::is_prompt(trimmed) && !trimmed.is_empty() {
            results.push(Self::navigate_row(
                trimmed.to_string(),
                NavigationText::new(trimmed).looks_like_url(),
            ));
        }
        results
    }

    fn stack_icon_for(pages: &[CommandBarPage], url: &str) -> PageIcon {
        let trimmed = url.trim_end_matches('/');
        pages
            .iter()
            .find(|page| page.url == url || page.url.trim_end_matches('/') == trimmed)
            .map(|page| page.icon.clone())
            .unwrap_or_default()
    }

    fn search_rows(
        query: &str,
        tabs: &[CommandBarTab],
        commands: &[CommandBarCommandEntry],
        pages: &[CommandBarPage],
        history: &[HistoryEntry],
    ) -> Vec<PaletteRow> {
        let q = query.trim();
        if q.is_empty() {
            return Self::empty_search_rows(tabs, commands, pages);
        }
        let starts_with_cmd = q.starts_with('>');
        let search = if starts_with_cmd { q[1..].trim() } else { q };
        let search_lower = search.to_lowercase();
        let mut items = Vec::new();
        if starts_with_cmd {
            Self::push_matching_commands(&mut items, commands, search, &search_lower);
        } else if !NavigationText::new(search).looks_like_path() {
            items.extend(Self::page_results(pages, &search_lower));
        }
        if !starts_with_cmd || !search.is_empty() {
            Self::push_matching_tabs(&mut items, tabs, pages, search, &search_lower);
        }
        if !starts_with_cmd {
            for entry in history.iter().take(5) {
                if !entry.url.starts_with("file://") {
                    items.push(Self::history_row(entry));
                }
            }
            Self::push_matching_commands(&mut items, commands, search, &search_lower);
        }
        if !search.is_empty() {
            items.push(Self::navigate_row(
                search.to_string(),
                NavigationText::new(search).looks_like_url(),
            ));
        }
        items
    }

    fn empty_search_rows(
        tabs: &[CommandBarTab],
        commands: &[CommandBarCommandEntry],
        pages: &[CommandBarPage],
    ) -> Vec<PaletteRow> {
        let mut items = vec![Self::navigate_row(String::new(), false)];
        for tab in tabs {
            if !tab.is_active {
                items.push(Self::stack_row(tab, Self::stack_icon_for(pages, &tab.url)));
            }
        }
        items.extend(Self::page_results(pages, ""));
        for command in commands {
            items.push(Self::command_row(command));
        }
        items
    }

    fn push_matching_commands(
        items: &mut Vec<PaletteRow>,
        commands: &[CommandBarCommandEntry],
        search: &str,
        search_lower: &str,
    ) {
        for command in commands {
            if search.is_empty()
                || command.name.to_lowercase().contains(search_lower)
                || command.id.contains(search_lower)
            {
                items.push(Self::command_row(command));
            }
        }
    }

    fn push_matching_tabs(
        items: &mut Vec<PaletteRow>,
        tabs: &[CommandBarTab],
        pages: &[CommandBarPage],
        search: &str,
        search_lower: &str,
    ) {
        for tab in tabs {
            if tab.is_active {
                continue;
            }
            if search.is_empty()
                || tab.title.to_lowercase().contains(search_lower)
                || tab.url.to_lowercase().contains(search_lower)
            {
                items.push(Self::stack_row(tab, Self::stack_icon_for(pages, &tab.url)));
            }
        }
    }

    fn glyph_for(
        navigating: Option<&PaletteRow>,
        mode: &PaletteMode,
        numbered: bool,
    ) -> Option<PaletteGlyph> {
        if matches!(mode, PaletteMode::Picking(_)) && !numbered {
            return None;
        }
        let Some(item) = navigating else {
            return match mode {
                PaletteMode::Command | PaletteMode::Ex | PaletteMode::Slash => {
                    Some(PaletteGlyph::Command)
                }
                PaletteMode::Path => Some(PaletteGlyph::Path),
                PaletteMode::Url => Some(PaletteGlyph::Url),
                PaletteMode::Search => Some(PaletteGlyph::Search),
                PaletteMode::Picking(_) if numbered => Some(PaletteGlyph::Search),
                PaletteMode::Picking(_) => None,
            };
        };
        let glyph = match item.kind {
            PaletteRowKind::Command | PaletteRowKind::Slash | PaletteRowKind::Pick => {
                PaletteGlyph::Command
            }
            PaletteRowKind::File | PaletteRowKind::Disabled => PaletteGlyph::Path,
            PaletteRowKind::Stack | PaletteRowKind::History => PaletteGlyph::Url,
            PaletteRowKind::Navigate => {
                if item.is_url {
                    PaletteGlyph::Url
                } else {
                    PaletteGlyph::Search
                }
            }
            PaletteRowKind::Page | PaletteRowKind::Search => PaletteGlyph::Search,
        };
        Some(glyph)
    }

    fn row_text_over(item: Option<&PaletteRow>, query: &str) -> Option<String> {
        let item = item?;
        if item.kind == PaletteRowKind::File {
            return None;
        }
        let text = match item.kind {
            PaletteRowKind::Command => format!("> {}", item.command_name),
            PaletteRowKind::Slash => format!("/{} ", item.slash_name),
            PaletteRowKind::Pick => item.display.title.clone(),
            PaletteRowKind::Navigate | PaletteRowKind::Stack => item.display.url.clone(),
            PaletteRowKind::Search => item.search_query.clone(),
            PaletteRowKind::Page => item.page_title.clone(),
            PaletteRowKind::History => item.display.title.clone(),
            PaletteRowKind::File => item.display.file_path.clone(),
            PaletteRowKind::Disabled => query.to_string(),
        };
        if text == query { None } else { Some(text) }
    }

    fn accent_agent(composer: &CommandPaletteComposer) -> Option<String> {
        if !composer.model_agent_key.is_empty() {
            return Some(composer.model_agent_key.clone());
        }
        if !composer.permission_agent_key.is_empty() {
            return Some(composer.permission_agent_key.clone());
        }
        let route = VmuxRoute::parse(&composer.agent_url)?;
        url::form_urlencoded::parse(route.query()?.as_bytes()).find_map(|(key, value)| {
            (key == "agent" && !value.is_empty()).then(|| value.into_owned())
        })
    }
}

impl PaletteOpen {
    pub(crate) fn opens_at_end(url: &str, asserted: Option<&CommandBarPicker>) -> bool {
        if asserted.is_none() && url.trim() == "@" {
            return true;
        }
        let prefix = match Self::mode_for(url, asserted.cloned(), &[]) {
            PaletteMode::Ex => ":",
            PaletteMode::Command => ">",
            PaletteMode::Path | PaletteMode::Slash => "/",
            PaletteMode::Search | PaletteMode::Url | PaletteMode::Picking(_) => "",
        };
        !prefix.is_empty() && url == prefix
    }

    pub(super) fn parse_ex_line(query: &str) -> Option<String> {
        let body = query.strip_prefix(':')?.trim();
        (!body.is_empty()).then(|| body.to_string())
    }

    pub(super) fn mode(&self, query: &str) -> PaletteMode {
        Self::mode_for(
            query,
            self.0.picker.clone(),
            &self.0.prompt_context.slash_commands,
        )
    }

    fn mode_for(
        query: &str,
        asserted: Option<CommandBarPicker>,
        slash_commands: &[SlashCommandEntry],
    ) -> PaletteMode {
        if let Some(picker) = asserted {
            return PaletteMode::Picking(picker);
        }
        if query.starts_with(':') {
            return PaletteMode::Ex;
        }
        let trimmed = query.trim();
        if trimmed.starts_with('@') {
            return PaletteMode::Path;
        }
        if trimmed.starts_with('>') {
            return PaletteMode::Command;
        }
        if Self::names_slash_command(query, slash_commands) {
            return PaletteMode::Slash;
        }
        if trimmed.starts_with('/') || trimmed.starts_with('~') {
            return PaletteMode::Path;
        }
        if trimmed.contains("://") || (trimmed.contains('.') && !trimmed.contains(' ')) {
            return PaletteMode::Url;
        }
        PaletteMode::Search
    }

    fn names_slash_command(query: &str, slash_commands: &[SlashCommandEntry]) -> bool {
        if query.trim() == "/" {
            return !slash_commands.is_empty();
        }
        let Some((name, _)) = PaletteDraftInput::slash_token(query) else {
            return false;
        };
        let lowered = name.to_lowercase();
        slash_commands
            .iter()
            .any(|command| command.command.name().starts_with(&lowered))
    }

    pub(super) fn placeholder(&self, mode: &PaletteMode, surface: CommandPaletteSurface) -> String {
        if matches!(mode, PaletteMode::Picking(_)) {
            return self.0.picker_placeholder.clone();
        }
        if matches!(mode, PaletteMode::Ex) {
            return translate("command-ex-placeholder");
        }
        match surface {
            CommandPaletteSurface::Start => translate("command-search-ask"),
            CommandPaletteSurface::Modal => {
                if matches!(self.0.target, Some(OpenTarget::InNewStack)) {
                    translate("command-new-tab-placeholder")
                } else {
                    translate("command-placeholder")
                }
            }
        }
    }

    pub(super) fn build_composer(
        &self,
        prompt_targets: &[PaletteRow],
        effective_target: Option<&PaletteRow>,
    ) -> CommandPaletteComposer {
        let state = &self.0;
        let context = &state.prompt_context;
        let agent_url = effective_target
            .and_then(PaletteRowsSnapshot::prompt_target_url)
            .unwrap_or_default()
            .to_string();
        let agent_title = match effective_target {
            Some(row) if row.kind == PaletteRowKind::Page => row.page_title.clone(),
            _ => "Agent".to_string(),
        };
        let mut agents = Vec::new();
        for item in prompt_targets {
            if item.kind != PaletteRowKind::Page {
                continue;
            }
            agents.push(CommandPaletteAgent {
                url: item.display.url.clone(),
                title: item.page_title.clone(),
                icon: item.page_icon.clone(),
            });
        }
        let models = Self::find_agent_models(&state.agent_models, &agent_url);
        let modes = Self::find_agent_modes(&state.agent_modes, &agent_url);
        let active_project = ProjectRow::for_directory(&context.projects, &context.cwd)
            .or_else(|| context.projects.iter().find(|project| project.is_active));
        let workspace_label = if let Some(project) = active_project {
            project.label.clone()
        } else if !context.workspace_name.is_empty() {
            context.workspace_name.clone()
        } else {
            translate("agent-project-select")
        };
        let workspace_path = active_project
            .map(|project| project.path.as_str())
            .filter(|path| !path.is_empty())
            .unwrap_or(&context.cwd);
        let workspace_title = if workspace_path.is_empty() {
            translate("agent-project-choose")
        } else {
            format!(
                "{} \u{00b7} {}",
                translate("agent-project-choose"),
                workspace_path
            )
        };
        let branch_label = if context.branch.is_empty() {
            "Git".to_string()
        } else {
            context.branch.clone()
        };
        let branch_title = if context.branch.is_empty() {
            translate("composer-git-repository")
        } else {
            translate_with(
                "composer-branch-name",
                &[("branch", TranslationValue::String(&context.branch))],
            )
        };
        let worktree_title = if context.base_ref.is_empty() {
            "Linked worktree".to_string()
        } else {
            format!("Worktree from {}", context.base_ref)
        };

        CommandPaletteComposer {
            loading: state.pages.is_empty() && state.commands.is_empty(),
            agents,
            agent_title,
            agent_url,
            model_name: Self::selected_model_name(models),
            model_options: models.map(|row| row.models.clone()).unwrap_or_default(),
            model_agent_key: models.map(|row| row.agent_key.clone()).unwrap_or_default(),
            model_current_id: models.map(|row| row.selected.clone()).unwrap_or_default(),
            permission_modes: modes.map(|row| row.modes.clone()).unwrap_or_default(),
            permission_agent_key: modes.map(|row| row.agent_key.clone()).unwrap_or_default(),
            permission_current_id: modes.map(|row| row.selected.clone()).unwrap_or_default(),
            permission_name: Self::selected_mode_name(modes),
            permission_title: Self::selected_mode_title(modes),
            workspace_label,
            workspace_title,
            branch_label,
            branch_title,
            worktree_title,
            project: workspace_path.to_string(),
            projects: context.projects.clone(),
            cwd: context.cwd.clone(),
            is_git_repo: context.is_git_repo,
            is_worktree: context.is_worktree,
            uncommitted: context.uncommitted,
            ahead: context.ahead,
        }
    }

    fn find_agent_models<'a>(rows: &'a [AgentModels], target_url: &str) -> Option<&'a AgentModels> {
        if target_url.is_empty() {
            return None;
        }
        let target = target_url.trim_end_matches('/');
        rows.iter()
            .find(|row| row.url.trim_end_matches('/') == target)
    }

    fn selected_model_name(row: Option<&AgentModels>) -> String {
        let Some(row) = row else {
            return String::new();
        };
        for model in &row.models {
            if model.id == row.selected {
                return model.name.clone();
            }
        }
        String::new()
    }

    fn find_agent_modes<'a>(rows: &'a [AgentModes], target_url: &str) -> Option<&'a AgentModes> {
        if target_url.is_empty() {
            return None;
        }
        let target = target_url.trim_end_matches('/');
        rows.iter()
            .find(|row| row.url.trim_end_matches('/') == target)
    }

    fn selected_mode_name(row: Option<&AgentModes>) -> String {
        let Some(row) = row else {
            return String::new();
        };
        row.modes
            .iter()
            .find(|mode| mode.id == row.selected)
            .map(|mode| mode.name.clone())
            .unwrap_or_else(|| row.selected.clone())
    }

    fn selected_mode_title(row: Option<&AgentModes>) -> String {
        row.and_then(|row| row.modes.iter().find(|mode| mode.id == row.selected))
            .and_then(|mode| mode.description.clone())
            .filter(|description| !description.is_empty())
            .unwrap_or_else(|| translate("composer-permission-change"))
    }
}

impl PaletteDraftInput {
    pub(super) fn only_files(input: &str) -> bool {
        input.trim_start().starts_with('@')
    }

    pub(super) fn parse_completion_query(input: &str) -> Option<String> {
        if !Self::only_files(input) {
            return None;
        }
        let mut trimmed = input.trim();
        trimmed = trimmed[1..].trim_start();
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Some(rest.to_string());
        }
        if NavigationText::new(trimmed).looks_like_path() {
            return Some(trimmed.to_string());
        }
        if trimmed.is_empty() {
            return Some(String::new());
        }
        if trimmed.contains("://") || NavigationText::new(trimmed).is_data_uri() {
            return None;
        }
        Some(trimmed.to_string())
    }

    pub(super) fn names_a_file(value: &str) -> bool {
        for term in value.split_whitespace() {
            if term.contains('/') {
                return true;
            }
            let Some((stem, extension)) = term.rsplit_once('.') else {
                continue;
            };
            if stem.is_empty() || extension.is_empty() || extension.len() > 5 {
                continue;
            }
            if extension.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                return true;
            }
        }
        false
    }

    pub(super) fn slash_token(input: &str) -> Option<(&str, &str)> {
        let rest = input.trim_start().strip_prefix('/')?;
        let (name, tail) = match rest.find(char::is_whitespace) {
            Some(at) => (&rest[..at], rest[at..].trim_start()),
            None => (rest, ""),
        };
        if name.is_empty() {
            return None;
        }
        let named = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
        named.then_some((name, tail))
    }

    pub(super) fn is_prompt(input: &str) -> bool {
        let query = input.trim();
        !query.is_empty()
            && !query.starts_with('@')
            && !query.starts_with('>')
            && !NavigationText::new(query).looks_like_url()
            && !(query.starts_with('/')
                || query.starts_with('~')
                || query.starts_with("./")
                || query.starts_with("../"))
    }

    pub(super) fn opens_typed_url_on_enter(
        input: &str,
        open_target: Option<OpenTarget>,
        navigating: bool,
    ) -> bool {
        let query = input.trim();
        matches!(open_target, Some(OpenTarget::InPlace))
            && !navigating
            && !query.is_empty()
            && !query.starts_with('@')
            && !input.trim_start().starts_with('>')
            && NavigationText::new(query).looks_like_url()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::CommandBarPromptContext;
    use vmux_api::protocol::AcpModeOption;
    use vmux_api::session::ModelOptionEntry;

    impl PaletteRowsSnapshot {
        fn engine(id: &str) -> SearchEngine {
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
                    icon: PageIcon::Builtin(BuiltinIcon::Settings),
                    shortcut: String::new(),
                    prompt_target: false,
                    startup: false,
                },
                CommandBarPage {
                    url: "vmux://spaces/".into(),
                    title: "Spaces".into(),
                    keywords: vec!["space".into()],
                    icon: PageIcon::Builtin(BuiltinIcon::Planet),
                    shortcut: String::new(),
                    prompt_target: false,
                    startup: false,
                },
                CommandBarPage {
                    url: "vmux://history/".into(),
                    title: "History".into(),
                    keywords: vec!["recent".into()],
                    icon: PageIcon::Builtin(BuiltinIcon::Clock),
                    shortcut: "\u{2318}Y".into(),
                    prompt_target: false,
                    startup: false,
                },
                CommandBarPage {
                    url: "vmux://sessions/vibe/".into(),
                    title: "Vibe".into(),
                    keywords: vec!["vibe".into(), "agent".into()],
                    icon: PageIcon::None,
                    shortcut: String::new(),
                    prompt_target: true,
                    startup: false,
                },
            ]
        }

        fn sample_open() -> PaletteOpen {
            PaletteOpen(CommandBarOpenEvent {
                pages: vec![
                    CommandBarPage {
                        url: "vmux://settings/".into(),
                        title: "Settings".into(),
                        keywords: vec!["preferences".into()],
                        icon: PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: false,
                        startup: false,
                    },
                    CommandBarPage {
                        url: "vmux://sessions/?agent=vibe".into(),
                        title: "Vibe".into(),
                        keywords: vec!["vibe".into()],
                        icon: PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: true,
                        startup: false,
                    },
                    CommandBarPage {
                        url: "vmux://sessions/?agent=codex".into(),
                        title: "Codex".into(),
                        keywords: vec!["codex".into()],
                        icon: PageIcon::Favicon("https://example.com/codex.png".into()),
                        shortcut: String::new(),
                        prompt_target: true,
                        startup: false,
                    },
                ],
                commands: vec![CommandBarCommandEntry {
                    id: "close_tab".into(),
                    name: "Close Tab".into(),
                    shortcut: String::new(),
                }],
                search_engines: vec![Self::engine("google")],
                ..CommandBarOpenEvent::default()
            })
        }

        fn hits(paths: &[&str]) -> Vec<PathEntry> {
            paths
                .iter()
                .map(|path| PathEntry {
                    name: (*path).to_string(),
                    is_dir: false,
                    full_path: format!("/root/{path}"),
                    project: "root".to_string(),
                })
                .collect()
        }
    }

    #[test]
    fn slash_token_is_a_bare_name_and_never_a_path() {
        assert_eq!(
            PaletteDraftInput::slash_token("/resume"),
            Some(("resume", ""))
        );
        assert_eq!(
            PaletteDraftInput::slash_token("/resume yesterday"),
            Some(("resume", "yesterday"))
        );
        assert_eq!(
            PaletteDraftInput::slash_token("  /model"),
            Some(("model", ""))
        );
        assert_eq!(PaletteDraftInput::slash_token("/Users/jun/notes.md"), None);
        assert_eq!(PaletteDraftInput::slash_token("/"), None);
        assert_eq!(PaletteDraftInput::slash_token("resume"), None);
    }

    #[test]
    fn prompt_rejects_navigation_and_commands() {
        assert!(PaletteDraftInput::is_prompt("fix the failing test"));
        assert!(PaletteDraftInput::is_prompt("codex"));
        for query in [
            "https://example.com",
            "example.com",
            "vmux://settings/",
            "/tmp/file",
            "~/project",
            "./src",
            "../repo",
            "> close tab",
            "@main.rs",
        ] {
            assert!(!PaletteDraftInput::is_prompt(query), "{query}");
        }
    }

    #[test]
    fn typed_url_open_requires_in_place_without_navigation() {
        assert!(PaletteDraftInput::opens_typed_url_on_enter(
            "https://example.com",
            Some(OpenTarget::InPlace),
            false
        ));
        assert!(!PaletteDraftInput::opens_typed_url_on_enter(
            "https://example.com",
            Some(OpenTarget::InPlace),
            true
        ));
        assert!(!PaletteDraftInput::opens_typed_url_on_enter(
            "> close",
            Some(OpenTarget::InPlace),
            false
        ));
        assert!(!PaletteDraftInput::opens_typed_url_on_enter(
            "@main.rs",
            Some(OpenTarget::InPlace),
            false
        ));
    }

    #[test]
    fn web_rows_project_synchronous_builtin_icons() {
        let search = PaletteRowsSnapshot::navigate_row("question".to_string(), false).display;
        let url = PaletteRowsSnapshot::navigate_row("example.com".to_string(), true).display;
        let engine =
            PaletteRowsSnapshot::search_row(&PaletteRowsSnapshot::engine("google"), "question")
                .display;

        assert_eq!(search.icon, PageIcon::Builtin(BuiltinIcon::Search));
        assert_eq!(url.icon, PageIcon::Builtin(BuiltinIcon::Globe));
        assert_eq!(engine.icon, PageIcon::Builtin(BuiltinIcon::Globe));
    }

    #[test]
    fn spaces_query_includes_spaces_page_and_command() {
        let commands = vec![CommandBarCommandEntry {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }];
        let results = PaletteRowsSnapshot::search_rows(
            "spaces",
            &[],
            &commands,
            &PaletteRowsSnapshot::sample_pages(),
            &[],
        );
        assert!(results.iter().any(|row| {
            row.kind == PaletteRowKind::Page && row.display.url == "vmux://spaces/"
        }));
        assert!(
            results.iter().any(|row| {
                row.kind == PaletteRowKind::Command && row.command_id == "space_open"
            })
        );
    }

    #[test]
    fn page_matched_by_keyword_and_sorted_alphabetically() {
        let pages = PaletteRowsSnapshot::sample_pages();
        let by_keyword = PaletteRowsSnapshot::search_rows("preferences", &[], &[], &pages, &[]);
        assert!(by_keyword.iter().any(|row| {
            row.kind == PaletteRowKind::Page && row.display.url == "vmux://settings/"
        }));

        let all = PaletteRowsSnapshot::search_rows("", &[], &[], &pages, &[]);
        let urls: Vec<&str> = all
            .iter()
            .filter(|row| row.kind == PaletteRowKind::Page)
            .map(|row| row.display.url.as_str())
            .collect();
        assert_eq!(
            urls,
            [
                "vmux://history/",
                "vmux://sessions/vibe/",
                "vmux://settings/",
                "vmux://spaces/",
            ]
        );
    }

    #[test]
    fn command_prefix_excludes_pages_and_file_history_hidden_without_at_prefix() {
        let pages = PaletteRowsSnapshot::sample_pages();
        let commands_only = PaletteRowsSnapshot::search_rows("> set", &[], &[], &pages, &[]);
        assert!(
            commands_only
                .iter()
                .all(|row| row.kind != PaletteRowKind::Page)
        );

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
        let results = PaletteRowsSnapshot::search_rows("main", &[], &[], &pages, &history);
        assert!(results.iter().all(|row| {
            !(row.kind == PaletteRowKind::History && row.display.url.starts_with("file://"))
        }));
        assert!(results.iter().any(|row| {
            row.kind == PaletteRowKind::History && row.display.url == "https://example.com/main"
        }));
    }

    #[test]
    fn file_completion_ordering_dedup_partial_and_more_matches() {
        let hits = PaletteRowsSnapshot::hits(&["src/settings.rs"]);
        let cmd = PaletteRowsSnapshot::command_row(&CommandBarCommandEntry {
            id: "settings".to_string(),
            name: "Settings".to_string(),
            shortcut: String::new(),
        });

        let by_name = PaletteRowsSnapshot::merge_file_rows(
            "@settings",
            &hits,
            false,
            hits.len(),
            vec![cmd.clone()],
        );
        assert_eq!(by_name[0].kind, PaletteRowKind::Command);
        assert_eq!(by_name[1].kind, PaletteRowKind::File);

        let by_path =
            PaletteRowsSnapshot::merge_file_rows("@~/src", &hits, false, hits.len(), vec![cmd]);
        assert_eq!(by_path[0].kind, PaletteRowKind::File);
        assert_eq!(by_path[1].kind, PaletteRowKind::Command);

        let dedup = PaletteRowsSnapshot::merge_file_rows(
            "@~/src",
            &hits,
            false,
            hits.len(),
            vec![PaletteRowsSnapshot::history_row(&HistoryEntry {
                url_entity_bits: 1,
                url: "file:///root/src/settings.rs".into(),
                title: "settings.rs".into(),
                favicon_url: String::new(),
                visit_created_at: 1,
                visit_count: 1,
                last_visited_at: 1,
            })],
        );
        assert_eq!(dedup.len(), 1);
        assert_eq!(dedup[0].kind, PaletteRowKind::File);

        let partial = PaletteRowsSnapshot::merge_file_rows("@settings", &hits, true, 14, vec![]);
        assert_eq!(partial.len(), 3);
        assert_eq!(partial[1].kind, PaletteRowKind::Disabled);
        assert_eq!(partial[1].display.leading, "!");
        assert_eq!(partial[2].kind, PaletteRowKind::Disabled);
        assert_eq!(partial[2].display.leading, "+");
    }

    #[test]
    fn only_at_prefixed_queries_reach_file_search() {
        for query in ["handler", "mobile main", "/root/src", "file://~/x"] {
            assert_eq!(
                PaletteDraftInput::parse_completion_query(query),
                None,
                "{query}"
            );
        }
        assert_eq!(
            PaletteDraftInput::parse_completion_query("@main.rs").as_deref(),
            Some("main.rs")
        );
        assert_eq!(
            PaletteDraftInput::parse_completion_query("@file://~/x").as_deref(),
            Some("~/x")
        );
        assert_eq!(
            PaletteDraftInput::parse_completion_query("@").as_deref(),
            Some("")
        );
    }

    #[test]
    fn asserted_picker_outranks_typed_prefixes_and_seeded_prefix_opens_at_end() {
        let asserted = CommandBarPicker::new("test.encoding-reopen");
        let open = PaletteOpen(CommandBarOpenEvent {
            picker: Some(asserted.clone()),
            ..Default::default()
        });
        for typed in [">", ":", "~/etc", "example.com", "how do i", ""] {
            assert_eq!(open.mode(typed), PaletteMode::Picking(asserted.clone()));
        }
        for seed in [":", ">", "/", "@"] {
            assert!(PaletteOpen::opens_at_end(seed, None));
        }
        for seed in ["https://example.com", "", ":w"] {
            assert!(!PaletteOpen::opens_at_end(seed, None));
        }
    }

    #[test]
    fn highlighted_file_beats_guessed_url_but_not_explicit_scheme_or_directory() {
        let file = PaletteRowsSnapshot::file_row(&PathEntry {
            name: "ts/packages/csp/src".into(),
            is_dir: false,
            full_path: "/repo/ts/packages/csp/src/index.ts".into(),
            project: "dashboard".into(),
        });
        let dir = PaletteRowsSnapshot::file_row(&PathEntry {
            name: String::new(),
            is_dir: true,
            full_path: "/repo/example.com".into(),
            project: "dashboard".into(),
        });
        assert!(PaletteRowsSnapshot::row_beats_guessed_url(
            Some(&file),
            "index.ts"
        ));
        assert!(PaletteRowsSnapshot::row_beats_guessed_url(
            Some(&file),
            " Index.TS "
        ));
        assert!(!PaletteRowsSnapshot::row_beats_guessed_url(
            Some(&file),
            "https://index.ts"
        ));
        assert!(!PaletteRowsSnapshot::row_beats_guessed_url(
            Some(&dir),
            "example.com"
        ));
    }

    #[test]
    fn composer_projects_targeted_agent_models_modes_and_working_directory() {
        let mut opened = PaletteRowsSnapshot::sample_open();
        opened.0.agent_models = vec![AgentModels {
            agent_key: "vibe".into(),
            url: "vmux://sessions/?agent=vibe".into(),
            selected: "big".into(),
            models: vec![
                ModelOptionEntry {
                    id: "big".into(),
                    name: "Big".into(),
                    ..Default::default()
                },
                ModelOptionEntry {
                    id: "small".into(),
                    name: "Small".into(),
                    ..Default::default()
                },
            ],
        }];
        opened.0.agent_modes = vec![AgentModes {
            agent_key: "vibe".into(),
            url: "vmux://sessions/?agent=vibe/".into(),
            selected: "agent".into(),
            modes: vec![AcpModeOption {
                id: "agent".into(),
                name: "Agent".into(),
                description: None,
            }],
        }];
        opened.0.prompt_context = CommandBarPromptContext {
            cwd: "/work/one/src".into(),
            workspace_name: "scratch".into(),
            projects: vec![
                ProjectRow {
                    path: "/work/one".into(),
                    label: "one".into(),
                    is_active: false,
                    ..Default::default()
                },
                ProjectRow {
                    path: "/work/two".into(),
                    label: "two".into(),
                    is_active: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let draft = PaletteDraftInput {
            surface: CommandPaletteSurface::Start,
            query: "fix it".into(),
            ..Default::default()
        };
        let rows =
            PaletteRowsSnapshot::build(&opened, &draft, &PaletteSnapshot::default(), &draft.query);
        let (projection, composer) = rows.project_ui(&opened, &draft);
        assert_eq!(composer.model_name, "Big");
        assert_eq!(composer.permission_current_id, "agent");
        assert_eq!(composer.project, "/work/one");
        assert_eq!(composer.workspace_label, "one");
        assert_eq!(projection.accent_agent.as_deref(), Some("vibe"));
        assert_eq!(composer.agents.len(), 2);
    }

    #[test]
    fn start_surface_stays_empty_until_typed_and_leads_with_selected_agent_for_prose() {
        let mut opened = PaletteRowsSnapshot::sample_open();
        opened.0.pages.push(CommandBarPage {
            url: "vmux://start/".into(),
            startup: true,
            ..Default::default()
        });
        let resting_draft = PaletteDraftInput {
            surface: CommandPaletteSurface::Start,
            ..Default::default()
        };
        let resting =
            PaletteRowsSnapshot::build(&opened, &resting_draft, &PaletteSnapshot::default(), "");
        assert!(resting.items.is_empty());

        let prose_draft = PaletteDraftInput {
            surface: CommandPaletteSurface::Start,
            query: "fix the failing test".into(),
            target_url: "vmux://sessions/?agent=codex".into(),
            ..Default::default()
        };
        let prose = PaletteRowsSnapshot::build(
            &opened,
            &prose_draft,
            &PaletteSnapshot::default(),
            &prose_draft.query,
        );
        assert_eq!(
            PaletteRowsSnapshot::prompt_target_url(&prose.items[0]),
            Some("vmux://sessions/?agent=codex")
        );
        assert_eq!(prose.items[0].display.title, "Ask Codex");
    }
}
