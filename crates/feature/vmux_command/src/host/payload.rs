use crate::host::definition::CommandDefinition;
use crate::host::snapshot::{
    CommandBarContextSnapshot, CommandBarPagesSnapshot, CommandBarProjectRoots,
    CommandBarWorkSnapshot, ContributedAgentModels, ContributedAgentModes, ContributedCommand,
    ContributedPages,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::Query;
use vmux_api::command_bar::{
    AgentModels, AgentModes, CommandBarCommandEntry, CommandBarControl, CommandBarOpenEvent,
    CommandBarTab, OpenId,
};
use vmux_api::open_target::OpenTarget;
use vmux_ui::i18n::{Locale, TranslationValue};

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
    pub context: CommandBarContextSnapshot,
    pub url: String,
    pub pages: CommandBarPagesSnapshot,
    pub projects: CommandBarProjectRoots,
    pub work: CommandBarWorkSnapshot,
    pub locale: Locale,
    pub tabs: Vec<CommandBarTab>,
    pub target: Option<OpenTarget>,
}

impl CommandBarProjector<'_, '_> {
    fn entries(&self, locale: &Locale, superseded: &[&str]) -> Vec<CommandBarCommandEntry> {
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for definition in &self.definitions {
            if definition.hidden
                || superseded.contains(&definition.id.as_str())
                || !seen.insert(definition.id.as_str())
            {
                continue;
            }
            entries.push(CommandBarCommandEntry {
                id: definition.id.clone(),
                name: definition.localized_name(locale.as_str()),
                shortcut: definition.shortcut_label(),
            });
        }
        for command in &self.commands {
            let mut args = Vec::new();
            for (name, value) in &command.args {
                args.push((name.as_str(), TranslationValue::String(value)));
            }
            entries.push(CommandBarCommandEntry {
                id: command.id.clone(),
                name: locale.translate_with(&command.message_id, &args),
                shortcut: String::new(),
            });
        }
        entries
    }

    fn shortcut(&self, id: &str) -> String {
        self.definitions
            .iter()
            .find(|definition| definition.id == id)
            .map(CommandDefinition::shortcut_label)
            .unwrap_or_default()
    }

    fn controls(&self, locale: &Locale) -> Vec<CommandBarControl> {
        let mut controls = Vec::new();
        for definition in &self.definitions {
            let Some(toolbar) = definition.toolbar else {
                continue;
            };
            let title = definition.localized_name(locale.as_str());
            let label = title
                .rsplit(" > ")
                .next()
                .unwrap_or(title.as_str())
                .to_string();
            let shortcut = definition.shortcut_label();
            let title = if shortcut.is_empty() {
                title
            } else {
                format!("{title} ({shortcut})")
            };
            controls.push((
                toolbar.rank,
                CommandBarControl {
                    id: definition.id.clone(),
                    label,
                    title,
                    icon: toolbar.icon,
                },
            ));
        }
        controls.sort_by(|(left_rank, left), (right_rank, right)| {
            left_rank
                .cmp(right_rank)
                .then_with(|| left.id.cmp(&right.id))
        });
        controls.into_iter().map(|(_, control)| control).collect()
    }

    fn agent_catalogs(&self) -> (Vec<AgentModels>, Vec<AgentModes>) {
        let mut agent_models: Vec<AgentModels> = self
            .agent_models
            .iter()
            .map(|models| models.0.clone())
            .collect();
        agent_models.sort_by(|left, right| left.agent_key.cmp(&right.agent_key));
        let mut agent_modes: Vec<AgentModes> = self
            .agent_modes
            .iter()
            .map(|modes| modes.0.clone())
            .collect();
        agent_modes.sort_by(|left, right| left.agent_key.cmp(&right.agent_key));
        (agent_models, agent_modes)
    }

    pub fn project(&self, projection: CommandBarOpenProjection) -> CommandBarOpenEvent {
        let mut pages = Vec::with_capacity(projection.pages.pages.len());
        let mut superseded = Vec::new();
        for entry in &projection.pages.pages {
            let mut page = entry.page.clone();
            if let Some(message_id) = entry.title_message_id.as_deref() {
                page.title = projection.locale.translate(message_id);
            }
            if let Some(command_id) = entry.replaces_command.as_deref() {
                page.shortcut = self.shortcut(command_id);
                superseded.push(command_id);
            }
            pages.push(page);
        }
        for entry in self.pages.sorted() {
            pages.push(entry.page);
        }
        let commands = self.entries(&projection.locale, &superseded);
        let controls = self.controls(&projection.locale);
        let (agent_models, agent_modes) = self.agent_catalogs();

        CommandBarOpenEvent {
            open_id: projection.open_id,
            native_windowed: projection.native_windowed,
            caret_at_end: false,
            url: projection.url,
            context_label: projection.context.label,
            tabs: projection.tabs,
            commands,
            controls,
            pages,
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
            picker_label: String::new(),
            picker_placeholder: String::new(),
            picker_typed: false,
            picker_numbered: false,
        }
    }
}
