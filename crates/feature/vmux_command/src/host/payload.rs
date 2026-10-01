use crate::host::definition::CommandDefinition;
use crate::host::snapshot::{
    CommandBarPagesSnapshot, CommandBarProjectRoots, CommandBarSpacesSnapshot,
    ContributedAgentModels, ContributedAgentModes, ContributedCommand, ContributedPages,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::Query;
use vmux_api::command_bar::{
    AgentModels, AgentModes, CommandBarCommandEntry, CommandBarPick, CommandBarPickRow,
    CommandBarPicker, CommandBarSpace, CommandBarTab,
};
use vmux_api::command_bar::{CommandBarOpenEvent, OpenId};
use vmux_api::open_target::OpenTarget;
use vmux_ui::i18n::{Locale, TranslationValue};

struct CommandBarEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

impl CommandBarEntry {
    fn list(
        locale: &Locale,
        contributed: Vec<Self>,
        superseded: &[&str],
        definitions: &Query<&CommandDefinition>,
    ) -> Vec<Self> {
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for definition in definitions {
            if definition.hidden
                || superseded.contains(&definition.id.as_str())
                || !seen.insert(definition.id.as_str())
            {
                continue;
            }
            entries.push(Self {
                id: definition.id.to_string(),
                name: definition.localized_name(locale.as_str()),
                shortcut: definition.shortcut_label(),
            });
        }
        entries.extend(contributed);
        entries
    }

    fn shortcut(id: &str, definitions: &Query<&CommandDefinition>) -> String {
        definitions
            .iter()
            .find(|definition| definition.id == id)
            .map(CommandDefinition::shortcut_label)
            .unwrap_or_default()
    }
}

pub(super) struct CommandBarPicks;

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
        let mut rows = Vec::with_capacity(vmux_ecs::event::FileEncoding::ALL.len());
        for encoding in vmux_ecs::event::FileEncoding::ALL {
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

#[derive(SystemParam)]
pub struct CommandBarProjector<'w, 's> {
    pages: ContributedPages<'w, 's>,
    commands: Query<'w, 's, &'static ContributedCommand>,
    definitions: Query<'w, 's, &'static CommandDefinition>,
    agent_models: Query<'w, 's, &'static ContributedAgentModels>,
    agent_modes: Query<'w, 's, &'static ContributedAgentModes>,
}

pub struct CommandBarOpenProjection {
    pub open_id: OpenId,
    pub native_windowed: bool,
    pub space_name: String,
    pub url: String,
    pub spaces: CommandBarSpacesSnapshot,
    pub terminal_page_url: String,
    pub pages: CommandBarPagesSnapshot,
    pub projects: CommandBarProjectRoots,
    pub work: crate::host::snapshot::CommandBarWorkSnapshot,
    pub locale: Locale,
    pub active_stack_count: usize,
    pub tabs: Vec<CommandBarTab>,
    pub target: Option<OpenTarget>,
}

impl CommandBarProjector<'_, '_> {
    pub fn project(&self, projection: CommandBarOpenProjection) -> CommandBarOpenEvent {
        let mut contributed = Vec::new();
        for command in &self.commands {
            let mut args = Vec::new();
            for (name, value) in &command.args {
                args.push((name.as_str(), TranslationValue::String(value)));
            }
            contributed.push(CommandBarEntry {
                id: command.id.clone(),
                name: projection.locale.translate_with(&command.message_id, &args),
                shortcut: String::new(),
            });
        }

        let mut pages = Vec::with_capacity(projection.pages.pages.len());
        let mut superseded = Vec::new();
        for entry in &projection.pages.pages {
            let mut page = entry.page.clone();
            if let Some(message_id) = entry.title_message_id.as_deref() {
                page.title = projection.locale.translate(message_id);
            }
            if let Some(command_id) = entry.replaces_command.as_deref() {
                page.shortcut = CommandBarEntry::shortcut(command_id, &self.definitions);
                superseded.push(command_id);
            }
            pages.push(page);
        }
        for entry in self.pages.sorted() {
            pages.push(entry.page);
        }

        let entries = CommandBarEntry::list(
            &projection.locale,
            contributed,
            &superseded,
            &self.definitions,
        );
        let mut commands = Vec::new();
        for entry in entries {
            commands.push(CommandBarCommandEntry {
                id: entry.id,
                name: entry.name,
                shortcut: entry.shortcut,
            });
        }

        let mut spaces = Vec::new();
        for space in &projection.spaces.spaces {
            let is_active = space.id == projection.spaces.active_space_id;
            spaces.push(CommandBarSpace {
                id: space.id.clone(),
                name: space.name.clone(),
                profile: space.profile.clone(),
                is_active,
                tab_count: if is_active {
                    projection.active_stack_count as u32
                } else {
                    0
                },
            });
        }

        let mut agent_models = self
            .agent_models
            .iter()
            .map(|models| models.0.clone())
            .collect::<Vec<AgentModels>>();
        agent_models.sort_by(|left, right| left.agent_key.cmp(&right.agent_key));
        let mut agent_modes = self
            .agent_modes
            .iter()
            .map(|modes| modes.0.clone())
            .collect::<Vec<AgentModes>>();
        agent_modes.sort_by(|left, right| left.agent_key.cmp(&right.agent_key));

        CommandBarOpenEvent {
            open_id: projection.open_id,
            native_windowed: projection.native_windowed,
            caret_at_end: false,
            url: projection.url,
            space_name: projection.space_name,
            spaces,
            spaces_page_url: projection.spaces.spaces_page_url,
            tabs: projection.tabs,
            commands,
            pages,
            terminal_page_url: projection.terminal_page_url,
            work_dirs: projection.work.work_dirs,
            recent_files: projection.work.recent_files,
            projects: projection.projects.roots,
            search_engines: projection.work.search_engines,
            prompt_context: Default::default(),
            agent_models,
            agent_modes,
            target: projection.target,
            picker: None,
            picks: Vec::new(),
        }
    }
}
