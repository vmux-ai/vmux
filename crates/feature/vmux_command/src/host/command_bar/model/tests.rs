use super::*;
use vmux_api::command_bar::{
    AgentModels, AgentModes, CommandBarCommandEntry, CommandBarPage, CommandBarPick,
    CommandBarPickRow, CommandBarPromptContext, CommandBarSpace, CommandBarTab, ExRequest,
    OpenRequest, PickRequest, PromptRequest, SearchEngine, SwitchSpaceRequest, SwitchTabRequest,
};
use vmux_api::open_target::OpenTarget;
use vmux_api::prompt_media::{ChatAttachment, ChatSubmitAttachment};
use vmux_api::protocol::AcpModeOption;
use vmux_api::room::ModelOptionEntry;
use vmux_api::space::ProjectRow;

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
            spaces_page_url: "vmux://spaces/".into(),
            terminal_page_url: "vmux://terminal/".into(),
            commands: vec![CommandBarCommandEntry {
                id: "close_tab".into(),
                name: "Close Tab".into(),
                shortcut: String::new(),
            }],
            search_engines: vec![SearchEngine::Google],
            ..CommandBarOpenEvent::default()
        }
    }

    fn switching_spaces() -> CommandBarOpenEvent {
        CommandBarOpenEvent {
            picker: Some(CommandBarPicker::Space),
            spaces: vec![
                CommandBarSpace {
                    id: "space-1".into(),
                    name: "Space 1".into(),
                    profile: "Personal".into(),
                    is_active: false,
                    tab_count: 0,
                },
                CommandBarSpace {
                    id: "work".into(),
                    name: "Work".into(),
                    profile: "Personal".into(),
                    is_active: true,
                    tab_count: 3,
                },
            ],
            ..Self::state()
        }
    }

    fn picking(picker: CommandBarPicker) -> CommandBarOpenEvent {
        let picks = match picker {
            CommandBarPicker::Encoding => vec![
                CommandBarPickRow {
                    label: "Reopen with Encoding".to_string(),
                    pick: CommandBarPick::Picker(CommandBarPicker::EncodingReopen),
                },
                CommandBarPickRow {
                    label: "Save with Encoding".to_string(),
                    pick: CommandBarPick::Picker(CommandBarPicker::EncodingSave),
                },
            ],
            CommandBarPicker::EncodingReopen => {
                let mut rows = Vec::new();
                for label in ["UTF-8", "Shift_JIS", "EUC-JP"] {
                    rows.push(CommandBarPickRow {
                        label: label.to_string(),
                        pick: CommandBarPick::Encoding {
                            label: label.to_string(),
                            save: false,
                        },
                    });
                }
                rows
            }
            _ => Vec::new(),
        };
        CommandBarOpenEvent {
            picker: Some(picker),
            picks,
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

struct ExNames;

impl ExNames {
    fn list(palette: &PaletteState) -> Vec<String> {
        let mut names = Vec::new();
        for row in &palette.rows {
            let CommandBarResultItem::Ex { name, .. } = row else {
                continue;
            };
            names.push(name.clone());
        }
        names
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
fn an_editor_row_for_an_already_listed_file_is_dropped() {
    let hits = FileRows::hits(&["src/settings.rs"]);
    let merged = FileRows::merge(
        "~/src",
        Completions::listing(&hits),
        vec![CommandBarResultItem::Editor {
            path: "/root/src/settings.rs".to_string(),
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

    let navigated = PaletteState::modal(&state, PaletteDraft::typed("setti").at(0).navigating());
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

    let navigated = PaletteState::modal(&state, PaletteDraft::typed("close").at(0).navigating());
    assert_eq!(
        navigated.glyph,
        Glyph::resolve(navigated.row(0), navigated.mode),
        "navigating reads the row, not the text"
    );
}

#[test]
fn a_picker_shows_no_input_glyph_because_its_chip_already_names_it() {
    assert_eq!(
        Glyph::resolve(None, PaletteMode::Picking(CommandBarPicker::Encoding)),
        None
    );
    assert_eq!(
        Glyph::resolve(None, PaletteMode::Picking(CommandBarPicker::Space)),
        Some(PaletteGlyph::Search),
        "the space switcher is a picker but reads as a search"
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
fn a_colon_offers_the_ex_commands_and_narrows_them_as_the_line_grows() {
    let state = Launcher::state();

    let offered = PaletteState::modal(&state, PaletteDraft::typed(":"));
    let names = ExNames::list(&offered);
    assert_eq!(names.len(), ExLine::COMMANDS.len(), "{names:?}");

    let narrowed = PaletteState::modal(&state, PaletteDraft::typed(":w"));
    assert_eq!(ExNames::list(&narrowed), vec!["w", "wq"]);

    let typed_out = PaletteState::modal(&state, PaletteDraft::typed(":%s/a/b/g"));
    assert!(
        typed_out.rows.is_empty(),
        "a line the catalog cannot complete offers nothing: {:?}",
        typed_out.rows
    );
}

#[test]
fn slash_commands_open_from_the_prefix_and_complete_mcp() {
    let mut state = Launcher::state();
    state.prompt_context.slash_commands = vec![
        vmux_api::chat::SlashCommandEntry {
            command: vmux_api::chat::SlashCommand::Upload,
            description: "Attach files".to_string(),
        },
        vmux_api::chat::SlashCommandEntry {
            command: vmux_api::chat::SlashCommand::Resume,
            description: "Resume a past session".to_string(),
        },
        vmux_api::chat::SlashCommandEntry {
            command: vmux_api::chat::SlashCommand::Mcp,
            description: String::new(),
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

    let mcp = PaletteState::start(&state, PaletteDraft::typed("/mcp"));
    assert!(matches!(
        mcp.rows.as_slice(),
        [CommandBarResultItem::Slash { name, .. }] if name == "mcp"
    ));
    assert_eq!(
        mcp.submit_start(&[]),
        PaletteDecision::Retype("/mcp ".to_string())
    );
}

#[test]
fn an_ex_line_runs_what_was_typed_unless_a_suggestion_is_highlighted() {
    let state = Launcher::state();

    let typed = PaletteState::modal(&state, PaletteDraft::typed(":noh"));
    assert_eq!(
        typed.submit_modal(&[]),
        PaletteDecision::Ex(ExRequest {
            line: "noh".to_string(),
        })
    );

    let picked = PaletteState::modal(&state, PaletteDraft::typed(":").at(1).navigating());
    assert_eq!(
        picked.submit_modal(&[]),
        PaletteDecision::Ex(ExRequest {
            line: ExLine::COMMANDS[1].name.to_string(),
        }),
        "an empty line still runs the row the user walked to: {:?}",
        picked.rows
    );
}

#[test]
fn an_asserted_picker_outranks_every_shape_the_typed_text_could_take() {
    let asserted = CommandBarPicker::EncodingReopen;
    for typed in [">", ":", "~/etc", "example.com", "how do i", ""] {
        assert_eq!(
            PaletteRows::infer_mode(typed, Some(asserted)),
            PaletteMode::Picking(asserted),
            "`{typed}` must not steal the picker the caller asked for"
        );
    }

    assert_eq!(
        PaletteRows::infer_mode("> close", None),
        PaletteMode::Command
    );
    assert_eq!(PaletteRows::infer_mode(":w", None), PaletteMode::Ex);
    assert_eq!(PaletteRows::infer_mode("~/src", None), PaletteMode::Path);
    assert_eq!(
        PaletteRows::infer_mode("example.com", None),
        PaletteMode::Url
    );
    assert_eq!(
        PaletteRows::infer_mode("how do i", None),
        PaletteMode::Search
    );
}

#[test]
fn a_picker_narrows_its_host_built_rows_and_submits_the_highlighted_one() {
    let state = Launcher::picking(CommandBarPicker::EncodingReopen);

    let offered = PaletteState::modal(&state, PaletteDraft::default());
    assert_eq!(offered.rows.len(), 3, "{:?}", offered.rows);

    let narrowed = PaletteState::modal(&state, PaletteDraft::typed("shift"));
    assert_eq!(narrowed.rows.len(), 1, "{:?}", narrowed.rows);
    assert_eq!(
        narrowed.submit_modal(&[]),
        PaletteDecision::Pick(PickRequest {
            pick: CommandBarPick::Encoding {
                label: "Shift_JIS".to_string(),
                save: false,
            },
        })
    );
}

#[test]
fn a_sub_list_row_asks_for_another_picker_rather_than_applying_anything() {
    let state = Launcher::picking(CommandBarPicker::Encoding);
    let palette = PaletteState::modal(&state, PaletteDraft::default());

    assert_eq!(
        palette.submit_modal(&[]),
        PaletteDecision::Pick(PickRequest {
            pick: CommandBarPick::Picker(CommandBarPicker::EncodingReopen),
        })
    );
}

#[test]
fn the_line_picker_reads_the_typed_number_instead_of_a_row() {
    let state = Launcher::picking(CommandBarPicker::GotoLine);

    for (input, line) in [("42", 41), ("  7  ", 6), ("12:5", 11), ("0", 0)] {
        let typed = PaletteState::modal(&state, PaletteDraft::typed(input));
        assert!(typed.rows.is_empty(), "{:?}", typed.rows);
        assert_eq!(
            typed.submit_modal(&[]),
            PaletteDecision::Pick(PickRequest {
                pick: CommandBarPick::GotoLine { line },
            }),
            "{input}"
        );
    }

    for input in ["", "abc", "-3", "3.5"] {
        let refused = PaletteState::modal(&state, PaletteDraft::typed(input));
        assert_eq!(refused.submit_modal(&[]), PaletteDecision::default());
    }
}

#[test]
fn a_seeded_prefix_is_typed_past_but_a_seeded_url_is_replaced() {
    for seed in [":", ">", "/"] {
        assert!(
            PaletteRows::opens_at_end(seed, None),
            "`{seed}` opens a mode, so the next keystroke must append to it"
        );
    }
    for seed in ["https://example.com", "", ":w"] {
        assert!(
            !PaletteRows::opens_at_end(seed, None),
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

    let mismatched = PaletteState::start(&state, PaletteDraft::typed("/other").completing(hits));
    assert!(mismatched.ghost.is_empty());
}

#[test]
fn selection_clamps_to_the_rows_that_exist() {
    let state = Launcher::state();
    let listed = PaletteState::start(&state, PaletteDraft::typed("fix the failing test").at(999));

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

    assert_eq!(rows.step(0, MenuDirection::Previous), 0);
    assert_eq!(rows.step(last, MenuDirection::Next), last);
    assert_eq!(rows.step(0, MenuDirection::Next), 1);
}

#[test]
fn a_space_digit_only_lands_on_a_space_row() {
    let state = Launcher::switching_spaces();
    let switching = PaletteState::start(&state, PaletteDraft::default());

    assert_eq!(switching.space_digit(0), Some(0));
    assert_eq!(switching.space_digit(1), Some(1));
    assert_eq!(
        switching.space_digit(2),
        None,
        "the manage-spaces page is not a space: {:?}",
        switching.rows
    );
}

#[test]
fn opening_the_space_switcher_preselects_the_active_space() {
    assert_eq!(
        PaletteState::opening_selection(&Launcher::switching_spaces()),
        1
    );
    assert_eq!(PaletteState::opening_selection(&Launcher::state()), 0);
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
fn switching_a_space_sends_the_space_id_from_the_highlighted_row() {
    let state = Launcher::switching_spaces();
    let palette = PaletteState::modal(&state, PaletteDraft::default().at(1));

    assert_eq!(
        palette.submit_modal(&[]),
        PaletteDecision::SwitchSpace(SwitchSpaceRequest {
            id: "work".to_string(),
        })
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
