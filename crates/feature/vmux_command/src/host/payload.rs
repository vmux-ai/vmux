use crate::definition::CommandDefinition;
use crate::event::{CommandBarOpenEvent, OpenId};
use crate::open_target::OpenTarget;
use crate::snapshot::{
    CommandBarPagesSnapshot, CommandBarSpacesSnapshot, ContributedCommand, ContributedPages,
};
use bevy::prelude::{Query, default};
use vmux_api::command_bar::{
    CommandBarCommandEntry, CommandBarPage, CommandBarPick, CommandBarPickRow, CommandBarPicker,
    CommandBarSpace, CommandBarTab, SearchEngine,
};
use vmux_ui::i18n::{Locale, TranslationValue};

pub struct CommandBarEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

pub struct CommandBarPicks;

impl CommandBarPicks {
    pub fn for_picker(picker: CommandBarPicker, locale: &Locale) -> Vec<CommandBarPickRow> {
        match picker {
            CommandBarPicker::Space | CommandBarPicker::GotoLine => Vec::new(),
            CommandBarPicker::Indent => Self::indents(locale),
            CommandBarPicker::LineEnding => vec![
                Self::row("LF", CommandBarPick::LineEnding { crlf: false }),
                Self::row("CRLF", CommandBarPick::LineEnding { crlf: true }),
            ],
            CommandBarPicker::Encoding => vec![
                Self::row(
                    locale.translate(Self::label(CommandBarPicker::EncodingReopen)),
                    CommandBarPick::Picker(CommandBarPicker::EncodingReopen),
                ),
                Self::row(
                    locale.translate(Self::label(CommandBarPicker::EncodingSave)),
                    CommandBarPick::Picker(CommandBarPicker::EncodingSave),
                ),
            ],
            CommandBarPicker::EncodingReopen => Self::encodings(false),
            CommandBarPicker::EncodingSave => Self::encodings(true),
        }
    }

    fn indents(locale: &Locale) -> Vec<CommandBarPickRow> {
        let mut rows = Vec::with_capacity(6);
        for spaces in [true, false] {
            for width in [2u16, 4, 8] {
                let id = match spaces {
                    true => "editor-status-spaces",
                    false => "editor-status-tabs",
                };
                let label = locale
                    .translate_with(id, &[("width", TranslationValue::Number(i64::from(width)))]);
                rows.push(Self::row(label, CommandBarPick::Indent { spaces, width }));
            }
        }
        rows
    }

    fn encodings(save: bool) -> Vec<CommandBarPickRow> {
        let mut rows = Vec::with_capacity(vmux_core::event::FileEncoding::ALL.len());
        for encoding in vmux_core::event::FileEncoding::ALL {
            rows.push(Self::row(
                encoding.label(),
                CommandBarPick::Encoding {
                    label: encoding.label().to_string(),
                    save,
                },
            ));
        }
        rows
    }

    fn row(label: impl Into<String>, pick: CommandBarPick) -> CommandBarPickRow {
        CommandBarPickRow {
            label: label.into(),
            pick,
        }
    }

    const fn label(picker: CommandBarPicker) -> &'static str {
        match picker {
            CommandBarPicker::Space => "",
            CommandBarPicker::GotoLine => "editor-status-goto-title",
            CommandBarPicker::Indent => "editor-status-indent-title",
            CommandBarPicker::LineEnding => "editor-status-eol-title",
            CommandBarPicker::Encoding => "editor-status-encoding-title",
            CommandBarPicker::EncodingReopen => "editor-status-encoding-reopen",
            CommandBarPicker::EncodingSave => "editor-status-encoding-save",
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_command_bar_open_payload(
    open_id: OpenId,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces_snapshot: &CommandBarSpacesSnapshot,
    contributed_pages: &ContributedPages,
    contributed_commands: &Query<&ContributedCommand>,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &crate::snapshot::CommandBarWorkSnapshot,
    locale: &Locale,
    active_stack_count: usize,
    tabs: Vec<CommandBarTab>,
    target: Option<OpenTarget>,
    definitions: &[CommandDefinition],
) -> CommandBarOpenEvent {
    let mut contributed = Vec::new();
    for command in contributed_commands {
        let args: Vec<(&str, TranslationValue<'_>)> = command
            .args
            .iter()
            .map(|(name, value)| (name.as_str(), TranslationValue::String(value)))
            .collect();
        contributed.push(CommandBarEntry {
            id: command.id.clone(),
            name: locale.translate_with(&command.message_id, &args),
            shortcut: String::new(),
        });
    }
    let mut pages = Vec::with_capacity(pages_snapshot.pages.len());
    let mut superseded = Vec::new();
    for entry in &pages_snapshot.pages {
        let mut page = entry.page.clone();
        if let Some(message_id) = entry.title_message_id.as_deref() {
            page.title = locale.translate(message_id);
        }
        if let Some(command_id) = entry.replaces_command.as_deref() {
            page.shortcut = command_shortcut(command_id, definitions);
            superseded.push(command_id);
        }
        pages.push(page);
    }
    for entry in contributed_pages.sorted() {
        pages.push(entry.page);
    }
    let commands: Vec<CommandBarCommandEntry> =
        command_list(locale, contributed, &superseded, definitions)
            .into_iter()
            .map(|e| CommandBarCommandEntry {
                id: e.id,
                name: e.name,
                shortcut: e.shortcut,
            })
            .collect();
    let spaces = spaces_snapshot
        .spaces
        .iter()
        .map(|s| {
            let is_active = s.id == spaces_snapshot.active_space_id;
            CommandBarSpace {
                id: s.id.clone(),
                name: s.name.clone(),
                profile: s.profile.clone(),
                is_active,
                tab_count: if is_active {
                    active_stack_count as u32
                } else {
                    0
                },
            }
        })
        .collect();
    command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        url,
        spaces,
        tabs,
        commands,
        target,
        pages,
        work_snapshot.work_dirs.clone(),
        work_snapshot.recent_files.clone(),
        work_snapshot.search_engines.clone(),
        work_snapshot.projects.clone(),
    )
}

pub fn command_list(
    locale: &Locale,
    contributed: Vec<CommandBarEntry>,
    superseded: &[&str],
    definitions: &[CommandDefinition],
) -> Vec<CommandBarEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for definition in definitions {
        if definition.hidden
            || superseded.contains(&definition.id.as_str())
            || !seen.insert(definition.id.as_str())
        {
            continue;
        }
        entries.push(CommandBarEntry {
            id: definition.id.to_string(),
            name: definition.localized_name(locale.as_str()),
            shortcut: definition.shortcut_label(),
        });
    }
    entries.extend(contributed);
    entries
}

fn command_shortcut(id: &str, definitions: &[CommandDefinition]) -> String {
    definitions
        .iter()
        .find(|definition| definition.id == id)
        .map(CommandDefinition::shortcut_label)
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub fn command_bar_open_payload(
    open_id: OpenId,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces: Vec<CommandBarSpace>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    target: Option<crate::open_target::OpenTarget>,
    pages: Vec<CommandBarPage>,
    work_dirs: Vec<crate::event::CommandBarWorkDir>,
    recent_files: Vec<crate::event::CommandBarRecentFile>,
    search_engines: Vec<SearchEngine>,
    projects: Vec<String>,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        native_windowed,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        pages,
        work_dirs,
        recent_files,
        projects,
        search_engines,
        prompt_context: default(),
        agent_models: Vec::new(),
        agent_modes: Vec::new(),
        target,
        picker: None,
        picks: Vec::new(),
    }
}
