#[cfg(test)]
use vmux_api::command_bar::CommandBarPicker;
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandPaletteAgent, CommandPaletteComposer, HistoryEntry, PaletteGlyph,
    PaletteMode, PathEntry,
};

use crate::CommandPaletteSurface;
use vmux_ui::i18n::translate;

pub(super) use self::decision::{PaletteDecision, PaletteState};
#[cfg(test)]
use self::decision::{RowText, TypedRow};
use self::decision::{SelectedAgentModels, SelectedAgentModes};
#[cfg(test)]
use self::file::ProjectPath;
pub(super) use self::file::{CompletionQuery, Completions, FileRows};
use self::results::{
    CommandBarResultItem, PageRows, PickerRows, SearchRows, SearchRowsInput, SlashRows, StartRows,
};

pub(super) use query::PaletteQuery;

mod decision;
mod file;
mod query;
mod results;

#[derive(Clone, Debug, Default)]
pub struct PaletteDraft {
    pub query: String,
    pub selected: usize,
    pub nav_mode: bool,
    pub target_url: String,
    pub completions: Vec<PathEntry>,
    pub completions_partial: bool,
    pub completions_total: usize,
    pub history: Vec<HistoryEntry>,
}

impl PaletteDraft {
    fn items(
        &self,
        state: &CommandBarOpenEvent,
        surface: CommandPaletteSurface,
        mode: &PaletteMode,
        start_prompt_mode: bool,
    ) -> Vec<CommandBarResultItem> {
        let query = self.query.as_str();
        let is_start = surface.is_start();
        if let PaletteMode::Picking(_) = mode {
            return PickerRows::filtered(state.picker_typed, &state.picks, query);
        }
        if matches!(mode, PaletteMode::Ex) {
            return Vec::new();
        }
        if mode == &PaletteMode::Slash {
            return SlashRows::for_query(query, state.prompt_context.slash_commands.as_slice());
        }
        if is_start && query.trim().is_empty() {
            return PageRows::open_sessions(&state.tabs, &state.pages);
        }
        if start_prompt_mode {
            let matched = StartRows::start(
                &state.pages,
                &state.work_dirs,
                &state.recent_files,
                &state.search_engines,
                query,
            );
            return self.with_completions(matched);
        }
        let matched = SearchRows::filter(SearchRowsInput {
            query,
            tabs: &state.tabs,
            commands: &state.commands,
            pages: &state.pages,
            history: &self.history,
            work_dirs: &state.work_dirs,
            recent_files: &state.recent_files,
        });
        let matched = self.with_completions(matched);
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
            let (CommandBarResultItem::Stack { url, .. } | CommandBarResultItem::Page { url, .. }) =
                &item
            else {
                kept.push(item);
                continue;
            };
            if startup_url == Some(url.trim_end_matches('/')) {
                continue;
            }
            kept.push(item);
        }
        kept
    }

    fn with_completions(&self, matched: Vec<CommandBarResultItem>) -> Vec<CommandBarResultItem> {
        FileRows::merge(
            &self.query,
            Completions::for_query(self, &self.query),
            matched,
        )
    }

    fn ghost(&self) -> String {
        if CompletionQuery::parse(&self.query).is_none() {
            return String::new();
        }
        let Some(first) = self.completions.first() else {
            return String::new();
        };
        let typed = self.query.trim();
        let full = &first.full_path;
        if !full.to_lowercase().starts_with(&typed.to_lowercase())
            || !full.is_char_boundary(typed.len())
        {
            return String::new();
        }
        full[typed.len()..].to_string()
    }
}

#[cfg(test)]
impl PaletteDraft {
    pub fn typed(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    pub fn at(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn navigating(mut self) -> Self {
        self.nav_mode = true;
        self
    }

    pub fn targeting(mut self, target_url: impl Into<String>) -> Self {
        self.target_url = target_url.into();
        self
    }

    pub fn completing(mut self, completions: Vec<PathEntry>) -> Self {
        self.completions_total = completions.len();
        self.completions = completions;
        self
    }

    pub fn partially_completing(mut self, completions: Vec<PathEntry>) -> Self {
        self.completions_total = completions.len();
        self.completions = completions;
        self.completions_partial = true;
        self
    }

    pub fn out_of(mut self, total: usize) -> Self {
        self.completions_total = total;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteRows {
    pub items: Vec<CommandBarResultItem>,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub ghost: String,
    pub start_prompt_mode: bool,
    pub numbered: bool,
    pub mode: PaletteMode,
}

impl PaletteRows {
    pub fn build(
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: CommandPaletteSurface,
    ) -> Self {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        let slash_commands = state.prompt_context.slash_commands.as_slice();
        let mode = PaletteQuery::new(query).mode(state.picker.clone(), slash_commands);
        let prompt_targets = if is_start {
            PageRows::prompt_targets(&state.pages, "")
        } else {
            Vec::new()
        };
        let default_target = prompt_targets
            .iter()
            .find(|item| PageRows::prompt_target_url(item) == Some(draft.target_url.as_str()))
            .cloned()
            .or_else(|| prompt_targets.first().cloned());
        let start_prompt_mode = is_start && PaletteQuery::new(query).is_start_prompt();

        let mut items = FileRows::under_projects(
            draft.items(state, surface, &mode, start_prompt_mode),
            &state.projects,
        );
        if start_prompt_mode {
            StartRows::prepend_targets(&mut items, default_target.as_ref(), &prompt_targets, query);
        }
        for item in &mut items {
            let show_hint = start_prompt_mode
                && PageRows::prompt_target_url(item).is_some()
                && !PageRows::prompt_target_matches(item, query);
            if let CommandBarResultItem::Page { prompt_hint, .. } = item {
                *prompt_hint = show_hint;
            }
        }

        Self {
            items,
            prompt_targets,
            default_target,
            ghost: draft.ghost(),
            start_prompt_mode,
            numbered: state.picker_numbered,
            mode,
        }
    }

    pub fn selected(&self, stored: usize) -> usize {
        stored.min(self.items.len().saturating_sub(1))
    }

    #[cfg(test)]
    pub fn step(&self, from: usize, next: bool) -> usize {
        if next {
            (from + 1).min(self.items.len().saturating_sub(1))
        } else {
            from.saturating_sub(1)
        }
    }
}

struct Glyph;

impl Glyph {
    fn resolve(
        navigating: Option<&CommandBarResultItem>,
        mode: &PaletteMode,
        numbered: bool,
    ) -> Option<PaletteGlyph> {
        if matches!(mode, PaletteMode::Picking(_)) && !numbered {
            return None;
        }
        let Some(item) = navigating else {
            return Self::in_mode(mode, numbered);
        };
        let glyph = match item {
            CommandBarResultItem::Command { .. }
            | CommandBarResultItem::Slash { .. }
            | CommandBarResultItem::Pick { .. } => PaletteGlyph::Command,
            CommandBarResultItem::File { .. }
            | CommandBarResultItem::WorkDir { .. }
            | CommandBarResultItem::PartialIndex
            | CommandBarResultItem::MoreMatches { .. }
            | CommandBarResultItem::RecentFile { .. } => PaletteGlyph::Path,
            CommandBarResultItem::Stack { .. } | CommandBarResultItem::History { .. } => {
                PaletteGlyph::Url
            }
            CommandBarResultItem::Navigate { is_url, .. } => {
                if *is_url {
                    PaletteGlyph::Url
                } else {
                    PaletteGlyph::Search
                }
            }
            CommandBarResultItem::Page { .. } | CommandBarResultItem::Search { .. } => {
                PaletteGlyph::Search
            }
        };
        Some(glyph)
    }

    const fn in_mode(mode: &PaletteMode, numbered: bool) -> Option<PaletteGlyph> {
        match mode {
            PaletteMode::Command | PaletteMode::Ex | PaletteMode::Slash => {
                Some(PaletteGlyph::Command)
            }
            PaletteMode::Path => Some(PaletteGlyph::Path),
            PaletteMode::Url => Some(PaletteGlyph::Url),
            PaletteMode::Search => Some(PaletteGlyph::Search),
            PaletteMode::Picking(_) if numbered => Some(PaletteGlyph::Search),
            PaletteMode::Picking(_) => None,
        }
    }
}

pub struct ExLine;

impl ExLine {
    pub fn parse(query: &str) -> Option<String> {
        let body = query.strip_prefix(':')?.trim();
        (!body.is_empty()).then(|| body.to_string())
    }
}

struct Composer;

impl Composer {
    fn build(
        state: &CommandBarOpenEvent,
        prompt_targets: &[CommandBarResultItem],
        effective_target: Option<&CommandBarResultItem>,
    ) -> CommandPaletteComposer {
        let context = &state.prompt_context;
        let agent_url = effective_target
            .and_then(PageRows::prompt_target_url)
            .unwrap_or_default()
            .to_string();
        let agent_title = match effective_target {
            Some(CommandBarResultItem::Page { title, .. }) => title.clone(),
            _ => "Agent".to_string(),
        };
        let mut agents = Vec::new();
        for item in prompt_targets {
            let CommandBarResultItem::Page { url, title, .. } = item else {
                continue;
            };
            agents.push(CommandPaletteAgent {
                url: url.clone(),
                title: title.clone(),
            });
        }
        let models = SelectedAgentModels::find(&state.agent_models, &agent_url);
        let modes = SelectedAgentModes::find(&state.agent_modes, &agent_url);
        let active_project = context.projects.iter().find(|project| project.is_active);
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
            "Git repository".to_string()
        } else {
            format!("Branch {}", context.branch)
        };
        let worktree_title = if context.base_ref.is_empty() {
            "Linked worktree".to_string()
        } else {
            format!("Worktree from {}", context.base_ref)
        };

        CommandPaletteComposer {
            loading: state.pages.is_empty(),
            agents,
            agent_title,
            agent_url,
            model_name: SelectedAgentModels::name(models),
            model_options: models.map(|row| row.models.clone()).unwrap_or_default(),
            model_agent_key: models.map(|row| row.agent_key.clone()).unwrap_or_default(),
            model_current_id: models.map(|row| row.selected.clone()).unwrap_or_default(),
            permission_modes: modes.map(|row| row.modes.clone()).unwrap_or_default(),
            permission_agent_key: modes.map(|row| row.agent_key.clone()).unwrap_or_default(),
            permission_current_id: modes.map(|row| row.selected.clone()).unwrap_or_default(),
            permission_name: SelectedAgentModes::name(modes),
            permission_title: SelectedAgentModes::title(modes),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::{
        AgentModels, AgentModes, CommandBarCommandEntry, CommandBarPage, CommandBarPick,
        CommandBarPickRow, CommandBarPromptContext, CommandBarTab, ExRequest, OpenRequest,
        PickRequest, PromptRequest, SearchEngine, SwitchTabRequest,
    };
    use vmux_api::open_target::OpenTarget;
    use vmux_api::prompt_media::{ChatAttachment, ChatSubmitAttachment};
    use vmux_api::protocol::AcpModeOption;
    use vmux_api::room::ModelOptionEntry;
    use vmux_api::space::ProjectRow;

    fn search_engine(id: &str) -> SearchEngine {
        SearchEngine {
            id: id.to_string(),
            name: id.to_string(),
            hosts: vec![format!("{id}.example")],
            query_url: format!("https://{id}.example/?q={{query}}"),
        }
    }

    impl<'a> Completions<'a> {
        fn listing(entries: &'a [PathEntry]) -> Self {
            Self {
                entries,
                partial: false,
                total: entries.len(),
            }
        }

        fn partial(entries: &'a [PathEntry]) -> Self {
            Self {
                entries,
                partial: true,
                total: entries.len(),
            }
        }
    }

    impl FileRows {
        fn hits(paths: &[&str]) -> Vec<PathEntry> {
            let mut entries = Vec::new();
            for path in paths {
                entries.push(PathEntry {
                    name: (*path).to_string(),
                    is_dir: false,
                    full_path: format!("/root/{path}"),
                    project: "root".to_string(),
                });
            }
            entries
        }

        fn a_command() -> CommandBarResultItem {
            CommandBarResultItem::Command {
                id: "settings".to_string(),
                name: "Settings".to_string(),
                shortcut: String::new(),
            }
        }
    }

    struct Launcher;

    impl Launcher {
        const ENCODING: &str = "test.encoding";
        const ENCODING_REOPEN: &str = "test.encoding-reopen";
        const ENCODING_SAVE: &str = "test.encoding-save";
        const GOTO_LINE: &str = "test.goto-line";

        fn state() -> CommandBarOpenEvent {
            CommandBarOpenEvent {
                pages: vec![
                    CommandBarPage {
                        url: "vmux://settings/".into(),
                        title: "Settings".into(),
                        keywords: vec!["preferences".into()],
                        icon: vmux_api::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: false,
                        startup: false,
                    },
                    CommandBarPage {
                        url: "vmux://sessions/vibe/".into(),
                        title: "Vibe".into(),
                        keywords: vec!["vibe".into()],
                        icon: vmux_api::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: true,
                        startup: false,
                    },
                    CommandBarPage {
                        url: "vmux://sessions/codex/cli".into(),
                        title: "Codex".into(),
                        keywords: vec!["codex".into()],
                        icon: vmux_api::PageIcon::None,
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
                search_engines: vec![search_engine("google")],
                ..CommandBarOpenEvent::default()
            }
        }

        fn picking(picker: CommandBarPicker) -> CommandBarOpenEvent {
            let picks = if picker.is(Self::ENCODING) {
                vec![
                    CommandBarPickRow {
                        label: "Reopen with Encoding".to_string(),
                        pick: CommandBarPick::Picker(CommandBarPicker::new(Self::ENCODING_REOPEN)),
                    },
                    CommandBarPickRow {
                        label: "Save with Encoding".to_string(),
                        pick: CommandBarPick::Picker(CommandBarPicker::new(Self::ENCODING_SAVE)),
                    },
                ]
            } else if picker.is(Self::ENCODING_REOPEN) {
                let mut rows = Vec::new();
                for label in ["UTF-8", "Shift_JIS", "EUC-JP"] {
                    rows.push(CommandBarPickRow {
                        label: label.to_string(),
                        pick: CommandBarPick::Typed {
                            picker: picker.clone(),
                            value: label.to_string(),
                        },
                    });
                }
                rows
            } else {
                Vec::new()
            };
            CommandBarOpenEvent {
                picker: Some(picker.clone()),
                picks,
                picker_typed: picker.is(Self::GOTO_LINE),
                ..Self::state()
            }
        }

        fn with_open_stack() -> CommandBarOpenEvent {
            CommandBarOpenEvent {
                tabs: vec![CommandBarTab {
                    title: "Docs".into(),
                    url: "vmux://sessions/codex/def".into(),
                    pane_id: 8,
                    tab_index: 1,
                    is_active: false,
                    location: "space-1 / pane 2".into(),
                }],
                ..Self::state()
            }
        }
    }

    impl PaletteState {
        fn start(state: &CommandBarOpenEvent, draft: PaletteDraft) -> Self {
            Self::resolve(state, &draft, CommandPaletteSurface::Start)
        }

        fn modal(state: &CommandBarOpenEvent, draft: PaletteDraft) -> Self {
            Self::resolve(state, &draft, CommandPaletteSurface::Modal)
        }
    }

    #[test]
    fn a_bare_word_keeps_commands_above_the_files_it_also_matched() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "settings",
            Completions::listing(&hits),
            vec![FileRows::a_command()],
        );
        assert!(matches!(merged[0], CommandBarResultItem::Command { .. }));
        assert!(matches!(merged[1], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn a_typed_path_puts_its_files_first() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "~/src",
            Completions::listing(&hits),
            vec![FileRows::a_command()],
        );
        assert!(matches!(merged[0], CommandBarResultItem::File { .. }));
        assert!(matches!(merged[1], CommandBarResultItem::Command { .. }));
    }

    #[test]
    fn a_recent_file_row_for_an_already_listed_file_is_dropped() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "~/src",
            Completions::listing(&hits),
            vec![CommandBarResultItem::RecentFile {
                url: "file:///root/src/settings.rs".to_string(),
                title: "settings.rs".to_string(),
            }],
        );
        assert_eq!(merged.len(), 1);
        assert!(matches!(merged[0], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn every_ranked_completion_is_listed_rather_than_the_first_handful() {
        let paths: Vec<String> = (0..40).map(|at| format!("src/main_{at:02}.rs")).collect();
        let named: Vec<&str> = paths.iter().map(String::as_str).collect();
        let hits = FileRows::hits(&named);
        let merged = FileRows::merge("main.rs", Completions::listing(&hits), Vec::new());

        assert_eq!(merged.len(), 40);
        assert!(
            !merged
                .iter()
                .any(|row| matches!(row, CommandBarResultItem::MoreMatches { .. })),
            "nothing was withheld, so the palette must not claim otherwise"
        );
    }

    #[test]
    fn a_withheld_tail_is_counted_in_the_last_row() {
        let state = Launcher::state();
        let palette = PaletteState::modal(
            &state,
            PaletteDraft::typed("main.rs")
                .completing(FileRows::hits(&["src/main.rs", "src/other/main.rs"]))
                .out_of(14),
        );

        assert_eq!(
            palette.rows.last(),
            Some(&CommandBarResultItem::MoreMatches {
                shown: 2,
                total: 14
            })
        );
    }

    #[test]
    fn a_partial_index_owns_up_to_it_below_the_files_it_did_find() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "settings",
            Completions::partial(&hits),
            vec![FileRows::a_command()],
        );

        assert_eq!(merged.last(), Some(&CommandBarResultItem::PartialIndex));
        assert!(matches!(merged[1], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn a_partial_index_that_found_nothing_still_says_why() {
        let merged = FileRows::merge("settings", Completions::partial(&[]), Vec::new());

        assert_eq!(merged, vec![CommandBarResultItem::PartialIndex]);
    }

    #[test]
    fn the_partial_index_notice_does_nothing_and_leaves_the_typed_text_alone() {
        let state = Launcher::state();
        let palette = PaletteState::modal(
            &state,
            PaletteDraft::typed("settings")
                .partially_completing(FileRows::hits(&["src/settings.rs"]))
                .navigating(),
        );
        let at = palette
            .rows
            .iter()
            .position(|row| matches!(row, CommandBarResultItem::PartialIndex))
            .expect("the notice is listed");

        let submission = palette.activate(&palette.rows[at], &[]);
        assert_eq!(submission, PaletteDecision::Close);
        assert_eq!(
            RowText::over(Some(&CommandBarResultItem::PartialIndex), "settings"),
            None
        );
    }

    #[test]
    fn a_complete_index_says_nothing() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "settings",
            Completions::listing(&hits),
            vec![FileRows::a_command()],
        );

        assert!(
            !merged
                .iter()
                .any(|row| matches!(row, CommandBarResultItem::PartialIndex))
        );
    }

    #[test]
    fn a_bare_word_reaches_the_host_but_prose_and_urls_do_not() {
        assert_eq!(
            CompletionQuery::parse("handler").as_deref(),
            Some("handler")
        );
        assert_eq!(
            CompletionQuery::parse("https://example.com").as_deref(),
            None
        );
        assert_eq!(CompletionQuery::parse("file://~/x").as_deref(), Some("~/x"));
    }

    #[test]
    fn several_words_reach_the_host_so_a_path_can_be_narrowed_word_by_word() {
        assert_eq!(
            CompletionQuery::parse("mobile main").as_deref(),
            Some("mobile main")
        );
        assert_eq!(
            CompletionQuery::parse("desktop src/lib").as_deref(),
            Some("desktop src/lib")
        );
    }

    #[test]
    fn a_file_under_a_project_is_shown_against_that_project() {
        let projects = vec!["/code/dashboard".to_string(), "/code".to_string()];
        assert_eq!(
            ProjectPath::split("/code/dashboard/src/main.rs", &projects),
            Some(("dashboard".to_string(), "src/main.rs".to_string())),
            "the longest matching root wins, or a worktree is shown against its parent repo"
        );
        assert_eq!(ProjectPath::split("/elsewhere/main.rs", &projects), None);
    }

    #[test]
    fn the_start_surface_rests_on_open_stacks_and_hides_itself() {
        let mut state = Launcher::with_open_stack();
        state.pages.push(CommandBarPage {
            url: "vmux://start/".into(),
            startup: true,
            ..Default::default()
        });
        state.tabs.push(CommandBarTab {
            title: "Start".into(),
            url: "vmux://start".into(),
            pane_id: 9,
            tab_index: 2,
            is_active: false,
            location: String::new(),
        });

        let resting = PaletteState::start(&state, PaletteDraft::default());
        assert!(
            resting
                .rows
                .iter()
                .all(|row| matches!(row, CommandBarResultItem::Stack { .. })),
            "the empty start surface offers open stacks only: {:?}",
            resting.rows
        );

        let searched = PaletteState::start(&state, PaletteDraft::typed("vmux://"));
        assert!(
            !searched.rows.iter().any(|row| matches!(
                row,
                CommandBarResultItem::Stack { url, .. } | CommandBarResultItem::Page { url, .. }
                    if url.trim_end_matches('/') == "vmux://start"
            )),
            "the start surface never offers itself: {:?}",
            searched.rows
        );
    }

    #[test]
    fn typing_prose_on_start_leads_with_the_chosen_agent() {
        let state = Launcher::state();

        let defaulted = PaletteState::start(&state, PaletteDraft::typed("fix the failing test"));
        assert_eq!(
            PageRows::prompt_target_url(&defaulted.rows[0]),
            Some("vmux://sessions/vibe/")
        );

        let chosen = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://sessions/codex/cli"),
        );
        assert_eq!(
            PageRows::prompt_target_url(&chosen.rows[0]),
            Some("vmux://sessions/codex/cli")
        );
        assert_eq!(chosen.composer.agent_title, "Codex");
        assert_eq!(chosen.accent_agent.as_deref(), Some("codex"));
    }

    #[test]
    fn the_modal_surface_offers_no_agents_and_no_composer_agent() {
        let state = Launcher::state();
        let bar = PaletteState::modal(&state, PaletteDraft::typed("fix the failing test"));

        assert!(bar.prompt_targets.is_empty());
        assert!(bar.default_target.is_none());
        assert!(!bar.start_prompt_mode);
        assert_eq!(bar.composer.agent_title, "Agent");
    }

    #[test]
    fn a_command_with_nothing_to_show_yet_never_becomes_a_web_search() {
        let state = Launcher::state();
        let mut bar = PaletteState::start(&state, PaletteDraft::typed("/resume"));
        bar.mode = PaletteMode::Slash;
        bar.rows = Vec::new();

        let submitted = bar.submit_start(&[]);

        assert!(
            matches!(submitted, PaletteDecision::None),
            "the list is still loading, so Enter must wait rather than search the web for the command"
        );
    }

    #[test]
    fn navigation_overlays_the_highlighted_row_and_still_edits_the_typed_text() {
        let state = Launcher::state();

        let navigated =
            PaletteState::modal(&state, PaletteDraft::typed("setti").at(0).navigating());
        assert_eq!(navigated.row_text.as_deref(), Some("Settings"));
        assert_eq!(navigated.query, "setti");

        let prompting = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test")
                .at(0)
                .navigating(),
        );
        assert_eq!(prompting.row_text, None);
        assert_eq!(prompting.query, "fix the failing test");

        let path = RowText::over(
            Some(&CommandBarResultItem::File {
                path: "/Users/jun/projects/common/src/lib.rs".into(),
                is_dir: false,
                project: "vmx-198".into(),
                relative: "common/src/lib.rs".into(),
            }),
            "lib.rs",
        );
        assert_eq!(
            path, None,
            "the row already names the file and where it lives, so painting its path over the query only hides what was typed"
        );
    }

    #[test]
    fn the_input_glyph_follows_the_highlighted_row_then_the_typed_shape() {
        let state = Launcher::state();

        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("> close")).glyph,
            Some(PaletteGlyph::Command)
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("~/src")).glyph,
            Some(PaletteGlyph::Path)
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("example.com")).glyph,
            Some(PaletteGlyph::Url)
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("how do i")).glyph,
            Some(PaletteGlyph::Search)
        );

        let navigated =
            PaletteState::modal(&state, PaletteDraft::typed("close").at(0).navigating());
        assert_eq!(
            navigated.glyph,
            Glyph::resolve(navigated.row(0), &navigated.mode, navigated.numbered,),
            "navigating reads the row, not the text"
        );
    }

    #[test]
    fn a_picker_shows_no_input_glyph_because_its_chip_already_names_it() {
        assert_eq!(
            Glyph::resolve(
                None,
                &PaletteMode::Picking(CommandBarPicker::new(Launcher::ENCODING)),
                false,
            ),
            None
        );
        assert_eq!(
            Glyph::resolve(
                None,
                &PaletteMode::Picking(CommandBarPicker::new("numbered")),
                true,
            ),
            Some(PaletteGlyph::Search),
            "a numbered picker reads as a search"
        );
    }

    #[test]
    fn a_highlighted_file_wins_over_reading_its_name_as_a_hostname() {
        let row = CommandBarResultItem::File {
            path: "/repo/ts/packages/csp/src/index.ts".into(),
            is_dir: false,
            project: "dashboard".into(),
            relative: "ts/packages/csp/src".into(),
        };

        assert!(TypedRow::beats_a_guessed_url(Some(&row), "index.ts"));
        assert!(TypedRow::beats_a_guessed_url(Some(&row), " Index.TS "));
        assert!(
            !TypedRow::beats_a_guessed_url(Some(&row), "csp"),
            "a partial match is still a search, not an open"
        );
        assert!(
            !TypedRow::beats_a_guessed_url(Some(&row), "https://index.ts"),
            "an explicit scheme means the user typed a URL"
        );
    }

    #[test]
    fn a_highlighted_file_never_hijacks_a_real_domain() {
        let row = CommandBarResultItem::File {
            path: "/repo/docs/google.com".into(),
            is_dir: false,
            project: "dashboard".into(),
            relative: "docs".into(),
        };
        let directory = CommandBarResultItem::File {
            path: "/repo/example.com".into(),
            is_dir: true,
            project: "dashboard".into(),
            relative: "".into(),
        };

        assert!(
            TypedRow::beats_a_guessed_url(Some(&row), "google.com"),
            "a file that really is named google.com is still the highlighted row"
        );
        assert!(
            !TypedRow::beats_a_guessed_url(Some(&directory), "example.com"),
            "a directory is not something Enter opens over a URL"
        );
        assert!(!TypedRow::beats_a_guessed_url(None, "google.com"));
    }

    #[test]
    fn ex_rows_are_feature_contributions() {
        let state = Launcher::state();

        let offered = PaletteState::modal(&state, PaletteDraft::typed(":"));
        assert!(offered.rows.is_empty());

        let narrowed = PaletteState::modal(&state, PaletteDraft::typed(":w"));
        assert!(narrowed.rows.is_empty());

        let typed_out = PaletteState::modal(&state, PaletteDraft::typed(":%s/a/b/g"));
        assert!(typed_out.rows.is_empty());
    }

    #[test]
    fn slash_commands_open_from_the_prefix_and_complete_mcp() {
        let mut state = Launcher::state();
        state.prompt_context.slash_commands = vec![
            vmux_api::chat::SlashCommandEntry {
                command: vmux_api::chat::SlashCommand::Upload,
                description: "Attach files".to_string(),
                delegated: false,
            },
            vmux_api::chat::SlashCommandEntry {
                command: vmux_api::chat::SlashCommand::Resume,
                description: "Resume a past session".to_string(),
                delegated: true,
            },
            vmux_api::chat::SlashCommandEntry {
                command: vmux_api::chat::SlashCommand::Mcp,
                description: String::new(),
                delegated: true,
            },
        ];

        let palette = PaletteState::start(&state, PaletteDraft::typed("/"));
        let names = palette
            .rows
            .iter()
            .filter_map(|row| match row {
                CommandBarResultItem::Slash { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(palette.mode, PaletteMode::Slash);
        assert_eq!(names, ["upload", "resume", "mcp"]);

        let mcp = PaletteState::start(&state, PaletteDraft::typed("/m"));
        assert!(matches!(
            mcp.rows.as_slice(),
            [CommandBarResultItem::Slash { name, .. }] if name == "mcp"
        ));
        assert_eq!(
            mcp.submit_start(&[]),
            PaletteDecision::Retype("/mcp ".to_string())
        );

        let delegated = PaletteState::start(&state, PaletteDraft::typed("/mcp"));
        assert!(delegated.rows.is_empty());
    }

    #[test]
    fn an_ex_line_runs_what_was_typed() {
        let state = Launcher::state();

        let typed = PaletteState::modal(&state, PaletteDraft::typed(":noh"));
        assert_eq!(
            typed.submit_modal(&[]),
            PaletteDecision::Ex(ExRequest {
                line: "noh".to_string(),
            })
        );
    }

    #[test]
    fn an_asserted_picker_outranks_every_shape_the_typed_text_could_take() {
        let asserted = CommandBarPicker::new(Launcher::ENCODING_REOPEN);
        for typed in [">", ":", "~/etc", "example.com", "how do i", ""] {
            assert_eq!(
                PaletteQuery::new(typed).mode(Some(asserted.clone()), &[]),
                PaletteMode::Picking(asserted.clone()),
                "`{typed}` must not steal the picker the caller asked for"
            );
        }

        assert_eq!(
            PaletteQuery::new("> close").mode(None, &[]),
            PaletteMode::Command
        );
        assert_eq!(PaletteQuery::new(":w").mode(None, &[]), PaletteMode::Ex);
        assert_eq!(
            PaletteQuery::new("~/src").mode(None, &[]),
            PaletteMode::Path
        );
        assert_eq!(
            PaletteQuery::new("example.com").mode(None, &[]),
            PaletteMode::Url
        );
        assert_eq!(
            PaletteQuery::new("how do i").mode(None, &[]),
            PaletteMode::Search
        );
    }

    #[test]
    fn a_picker_narrows_its_host_built_rows_and_submits_the_highlighted_one() {
        let state = Launcher::picking(CommandBarPicker::new(Launcher::ENCODING_REOPEN));

        let offered = PaletteState::modal(&state, PaletteDraft::default());
        assert_eq!(offered.rows.len(), 3, "{:?}", offered.rows);

        let narrowed = PaletteState::modal(&state, PaletteDraft::typed("shift"));
        assert_eq!(narrowed.rows.len(), 1, "{:?}", narrowed.rows);
        assert_eq!(
            narrowed.submit_modal(&[]),
            PaletteDecision::Pick(PickRequest {
                pick: CommandBarPick::Typed {
                    picker: CommandBarPicker::new(Launcher::ENCODING_REOPEN),
                    value: "Shift_JIS".to_string(),
                },
            })
        );
    }

    #[test]
    fn a_sub_list_row_asks_for_another_picker_rather_than_applying_anything() {
        let state = Launcher::picking(CommandBarPicker::new(Launcher::ENCODING));
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        assert_eq!(
            palette.submit_modal(&[]),
            PaletteDecision::Pick(PickRequest {
                pick: CommandBarPick::Picker(CommandBarPicker::new(Launcher::ENCODING_REOPEN)),
            })
        );
    }

    #[test]
    fn typed_picker_delegates_raw_value_to_its_feature() {
        let state = Launcher::picking(CommandBarPicker::new(Launcher::GOTO_LINE));

        for (input, value) in [
            ("42", "42"),
            ("  7  ", "7"),
            ("12:5", "12:5"),
            ("abc", "abc"),
        ] {
            let typed = PaletteState::modal(&state, PaletteDraft::typed(input));
            assert!(typed.rows.is_empty(), "{:?}", typed.rows);
            assert_eq!(
                typed.submit_modal(&[]),
                PaletteDecision::Pick(PickRequest {
                    pick: CommandBarPick::Typed {
                        picker: CommandBarPicker::new(Launcher::GOTO_LINE),
                        value: value.to_string(),
                    },
                }),
                "{input}"
            );
        }
    }

    #[test]
    fn a_seeded_prefix_is_typed_past_but_a_seeded_url_is_replaced() {
        for seed in [":", ">", "/"] {
            assert!(
                PaletteQuery::new(seed).opens_at_end(None),
                "`{seed}` opens a mode, so the next keystroke must append to it"
            );
        }
        for seed in ["https://example.com", "", ":w"] {
            assert!(
                !PaletteQuery::new(seed).opens_at_end(None),
                "`{seed}` is a value, so the next keystroke must replace it"
            );
        }
    }

    #[test]
    fn the_ghost_completes_a_typed_path_but_never_prose() {
        let state = Launcher::state();
        let hits = FileRows::hits(&["src/main.rs"]);

        let path = PaletteState::start(
            &state,
            PaletteDraft::typed("/root/src").completing(hits.clone()),
        );
        assert_eq!(path.ghost, "/main.rs");

        let prose = PaletteState::start(
            &state,
            PaletteDraft::typed("how do i").completing(hits.clone()),
        );
        assert!(prose.ghost.is_empty());

        let mismatched =
            PaletteState::start(&state, PaletteDraft::typed("/other").completing(hits));
        assert!(mismatched.ghost.is_empty());
    }

    #[test]
    fn selection_clamps_to_the_rows_that_exist() {
        let state = Launcher::state();
        let listed =
            PaletteState::start(&state, PaletteDraft::typed("fix the failing test").at(999));

        assert_eq!(listed.selected, listed.rows.len() - 1);

        let single = PaletteState::modal(&state, PaletteDraft::typed("zzzz").at(4));
        assert_eq!(single.rows.len(), 1, "{:?}", single.rows);
        assert_eq!(single.selected, 0);
    }

    #[test]
    fn arrow_keys_stop_at_both_ends_of_the_list() {
        let state = Launcher::state();
        let rows = PaletteRows::build(
            &state,
            &PaletteDraft::typed("fix the failing test"),
            CommandPaletteSurface::Start,
        );
        let last = rows.items.len() - 1;

        assert_eq!(rows.step(0, false), 0);
        assert_eq!(rows.step(last, true), last);
        assert_eq!(rows.step(0, true), 1);
    }

    #[test]
    fn prose_on_start_prompts_the_agent() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix the failing test"));

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: "fix the failing test".to_string(),
                    target_url: Some("vmux://sessions/vibe/".to_string()),
                    attachments: Vec::new(),
                },
            }
        );
    }

    #[test]
    fn a_cli_agent_is_prompted() {
        let state = Launcher::state();
        let palette = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://sessions/codex/cli"),
        );

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: "fix the failing test".to_string(),
                    target_url: Some("vmux://sessions/codex/cli".to_string()),
                    attachments: Vec::new(),
                },
            }
        );
    }

    #[test]
    fn naming_an_agent_opens_it_instead_of_prompting_it() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("vibe"));

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted,
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "vmux://sessions/vibe/".to_string(),
                    open: palette.open_target,
                },
            }
        );
    }

    #[test]
    fn an_attachment_alone_prompts_the_default_agent() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::default());
        let attached = [ChatAttachment {
            path: "/tmp/a.png".into(),
            name: "a.png".into(),
            mime_type: "image/png".into(),
            size: 12,
            preview_data_url: String::new(),
        }];

        let submitted = palette.submit_start(&attached);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: String::new(),
                    target_url: Some("vmux://sessions/vibe/".to_string()),
                    attachments: vec![ChatSubmitAttachment::from(&attached[0])],
                },
            }
        );
    }

    #[test]
    fn an_attachment_with_no_agent_still_reaches_the_host() {
        let state = CommandBarOpenEvent::default();
        let palette = PaletteState::start(&state, PaletteDraft::default());
        let attached = [ChatAttachment {
            path: "/tmp/a.png".into(),
            name: "a.png".into(),
            mime_type: "image/png".into(),
            size: 12,
            preview_data_url: String::new(),
        }];

        let submitted = palette.submit_start(&attached);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: false,
                request: PromptRequest {
                    text: String::new(),
                    target_url: None,
                    attachments: vec![ChatSubmitAttachment::from(&attached[0])],
                },
            },
            "the composer keeps its draft on screen"
        );
    }

    #[test]
    fn a_typed_url_opens_in_place_unless_a_matching_page_is_highlighted() {
        let mut state = Launcher::state();
        state.target = Some(OpenTarget::InPlace);

        let typed = PaletteState::modal(&state, PaletteDraft::typed("https://example.com"));
        assert_eq!(
            typed.submit_modal(&[]),
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "https://example.com".to_string(),
                    open: Some(OpenTarget::InPlace),
                },
            }
        );

        let page = PaletteState::modal(&state, PaletteDraft::typed("vmux://settings"));
        let opened = page.submit_modal(&[]);
        assert_eq!(
            opened,
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "vmux://settings/".to_string(),
                    open: Some(OpenTarget::InPlace),
                },
            },
            "the page row wins over the raw text: {:?}",
            page.rows
        );
    }

    #[test]
    fn a_highlighted_stack_switches_tab_rather_than_opening_a_url() {
        let state = Launcher::with_open_stack();
        let palette = PaletteState::start(&state, PaletteDraft::default());

        assert_eq!(
            palette.submit_start(&[]),
            PaletteDecision::SwitchTab(SwitchTabRequest { pane: 8, index: 1 })
        );
    }

    #[test]
    fn a_file_row_opens_through_the_file_scheme() {
        let state = Launcher::state();
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        let opened = palette.activate(
            &CommandBarResultItem::File {
                path: "/work/main.rs".into(),
                is_dir: false,
                project: String::new(),
                relative: String::new(),
            },
            &[],
        );

        assert_eq!(
            opened,
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "file:///work/main.rs".to_string(),
                    open: palette.open_target,
                },
            }
        );
    }

    #[test]
    fn an_empty_navigate_row_closes_without_asking_the_host_for_anything() {
        let state = Launcher::state();
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        let submitted = palette.activate(
            &CommandBarResultItem::Navigate {
                url: String::new(),
                is_url: false,
            },
            &[],
        );

        assert_eq!(submitted, PaletteDecision::Close);
    }

    #[test]
    fn the_send_button_prompts_the_composer_agent_when_no_row_answers_the_text() {
        let state = Launcher::state();
        let palette = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://sessions/codex/cli"),
        );

        assert_eq!(
            palette.submit_start(&[]),
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: "fix the failing test".to_string(),
                    target_url: Some("vmux://sessions/codex/cli".to_string()),
                    attachments: Vec::new(),
                },
            }
        );
    }

    #[test]
    fn the_send_button_does_nothing_on_an_empty_composer() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::default());
        let empty = PaletteState {
            rows: Vec::new(),
            ..palette
        };

        assert_eq!(empty.submit_start(&[]), PaletteDecision::default());
    }

    #[test]
    fn the_composer_reads_the_model_of_the_targeted_agent_only() {
        let mut state = Launcher::state();
        state.agent_models = vec![AgentModels {
            agent_key: "vibe".into(),
            url: "vmux://sessions/vibe/".into(),
            selected: "big".into(),
            models: vec![
                ModelOptionEntry {
                    id: "big".into(),
                    name: "Big".into(),
                    ..ModelOptionEntry::default()
                },
                ModelOptionEntry {
                    id: "small".into(),
                    name: "Small".into(),
                    ..ModelOptionEntry::default()
                },
            ],
        }];

        let vibe = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        assert_eq!(vibe.composer.model_name, "Big");
        assert_eq!(vibe.composer.model_agent_key, "vibe");
        assert_eq!(vibe.composer.model_options.len(), 2);

        let codex = PaletteState::start(
            &state,
            PaletteDraft::typed("fix it").targeting("vmux://sessions/codex/cli"),
        );
        assert!(codex.composer.model_name.is_empty());
        assert!(codex.composer.model_options.is_empty());
    }

    #[test]
    fn the_composer_matches_permission_modes_across_a_trailing_slash() {
        let mut state = Launcher::state();
        state.agent_modes = vec![AgentModes {
            agent_key: "vibe".into(),
            url: "vmux://sessions/vibe".into(),
            selected: "agent".into(),
            modes: vec![AcpModeOption {
                id: "agent".into(),
                name: "Agent".into(),
                description: None,
            }],
        }];

        let vibe = PaletteState::start(&state, PaletteDraft::typed("fix it"));

        assert_eq!(vibe.composer.permission_agent_key, "vibe");
        assert_eq!(vibe.composer.permission_current_id, "agent");
        assert_eq!(vibe.composer.permission_modes.len(), 1);
    }

    #[test]
    fn the_composer_prefers_the_active_project_over_the_working_directory() {
        let mut state = Launcher::state();
        state.prompt_context = CommandBarPromptContext {
            cwd: "/tmp/scratch".into(),
            workspace_name: "scratch".into(),
            projects: vec![
                ProjectRow {
                    path: "/work/one".into(),
                    label: "one".into(),
                    is_active: false,
                    ..ProjectRow::default()
                },
                ProjectRow {
                    path: "/work/two".into(),
                    label: "two".into(),
                    is_active: true,
                    ..ProjectRow::default()
                },
            ],
            ..CommandBarPromptContext::default()
        };
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        assert_eq!(palette.composer.project, "/work/two");
        assert_eq!(palette.composer.workspace_label, "two");
        assert_eq!(
            palette.composer.workspace_title,
            "Choose project · /work/two"
        );

        state.prompt_context = CommandBarPromptContext {
            cwd: "/tmp/scratch".into(),
            ..CommandBarPromptContext::default()
        };
        let unrooted = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        assert_eq!(unrooted.composer.project, "/tmp/scratch");
    }

    #[test]
    fn the_composer_lists_every_agent_the_launcher_knows() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        let urls: Vec<_> = palette
            .composer
            .agents
            .iter()
            .map(|agent| agent.url.as_str())
            .collect();

        assert_eq!(
            urls,
            vec!["vmux://sessions/vibe/", "vmux://sessions/codex/cli"]
        );
    }
}
