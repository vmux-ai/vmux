use bevy::prelude::*;
use vmux_api::JsonSchema;
use vmux_api::json::JsonValue;
use vmux_api::protocol::AgentCommandTool;
use vmux_ecs::JsonArguments;
use vmux_ecs::manifest::{self, FeatureManifest};
use vmux_ui::i18n::Locale;

use super::definition::{
    AgentAccess, CommandDefinition, CommandDispatch, CommandInvocation, CommandManifest,
    CommandMcp, CommandMessage, CommandShortcut, CommandToolbar, ShortcutDefinition,
};
use super::shortcut_driver::{Binding, KeyCombo, Shortcut, Source, When};

impl From<manifest::ShortcutKind> for ShortcutDefinition {
    fn from(shortcut: manifest::ShortcutKind) -> Self {
        match shortcut {
            manifest::ShortcutKind::Direct(value) => Self::Direct(value),
            manifest::ShortcutKind::Chord(value) => Self::Chord(value),
        }
    }
}

impl From<manifest::Shortcut> for CommandShortcut {
    fn from(shortcut: manifest::Shortcut) -> Self {
        Self {
            shortcut: shortcut.shortcut.into(),
            when: shortcut.when,
        }
    }
}

impl CommandManifest {
    pub fn for_feature<M: manifest::FeatureManifestSource>() -> Self {
        Self(FeatureManifest::of::<M>().commands)
    }

    pub fn from_feature_ron(source: &str) -> Self {
        Self(FeatureManifest::parse(source).commands)
    }

    pub fn into_vec(self) -> Vec<CommandDefinition> {
        self.0.into_iter().map(CommandDefinition::from).collect()
    }
}

impl From<manifest::CommandMcp> for CommandMcp {
    fn from(mcp: manifest::CommandMcp) -> Self {
        let definition = Self::new(
            mcp.description,
            mcp.input_schema.unwrap_or_else(JsonSchema::object),
        );
        if mcp.allow_agent {
            return definition.allow_agent();
        }
        definition
    }
}

impl CommandMcp {
    pub fn new(description: impl Into<String>, input_schema: JsonSchema) -> Self {
        Self {
            description: description.into(),
            input_schema,
            agent_access: AgentAccess::Denied,
        }
    }

    pub fn allow_agent(mut self) -> Self {
        self.agent_access = AgentAccess::Allowed;
        self
    }
}

impl From<manifest::Command> for CommandDefinition {
    fn from(command: manifest::Command) -> Self {
        Self {
            id: command.id,
            aliases: command.aliases,
            label: command.label,
            group: command.group,
            accelerator: command.accelerator,
            hidden: command.hidden,
            native_menu: command.native_menu,
            shortcut_label: command.shortcut_label,
            shortcuts: command.shortcuts.into_iter().map(Into::into).collect(),
            toolbar: command.toolbar.map(|toolbar| CommandToolbar {
                icon: toolbar.icon,
                rank: toolbar.rank,
            }),
            mcp: command.mcp.map(Into::into),
        }
    }
}

impl CommandDefinition {
    pub fn new(id: impl Into<String>, label: impl Into<String>, group: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            aliases: Vec::new(),
            label: label.into(),
            group: group.into(),
            accelerator: None,
            hidden: false,
            native_menu: true,
            shortcut_label: None,
            shortcuts: Vec::new(),
            toolbar: None,
            mcp: None,
        }
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    #[cfg(test)]
    pub(super) fn message<T>(self) -> (Self, CommandMessage)
    where
        T: Message + for<'a> TryFrom<&'a CommandInvocation>,
    {
        (self, CommandMessage::of::<T>())
    }

    pub fn agent_tool(&self) -> Option<AgentCommandTool> {
        let mcp = self.mcp.as_ref()?;
        Some(AgentCommandTool {
            name: self.id.clone(),
            description: mcp.description.clone(),
            input_schema: JsonValue::from(mcp.input_schema.to_json()),
        })
    }

    pub fn matches(&self, id: &str) -> bool {
        self.id == id || self.aliases.iter().any(|alias| alias == id)
    }

    pub fn user_invocation(
        &self,
        caller: Entity,
        arguments: serde_json::Value,
    ) -> Result<CommandInvocation, String> {
        self.validated_invocation(caller, arguments, false)
    }

    pub fn agent_invocation(
        &self,
        caller: Entity,
        arguments: serde_json::Value,
    ) -> Result<CommandInvocation, String> {
        self.validated_invocation(caller, arguments, true)
    }

    fn validated_invocation(
        &self,
        caller: Entity,
        arguments: serde_json::Value,
        require_agent_access: bool,
    ) -> Result<CommandInvocation, String> {
        let Some(mcp) = &self.mcp else {
            return Err(format!("unknown app command: {}", self.id));
        };
        if require_agent_access && mcp.agent_access != AgentAccess::Allowed {
            return Err("focus-changing app command is disabled for agents".to_string());
        }
        self.validate_arguments(&arguments)?;
        Ok(CommandInvocation::new(caller, &self.id).with_arguments(arguments))
    }

    pub fn mcp(mut self, definition: CommandMcp) -> Self {
        self.mcp = Some(definition);
        self
    }

    pub fn expose_to_mcp(mut self) -> Self {
        self.mcp = Some(CommandMcp::new(self.label.clone(), JsonSchema::object()));
        self
    }

    pub fn allow_agent(mut self) -> Self {
        let mcp = self
            .mcp
            .as_mut()
            .expect("agent access requires an MCP command definition");
        mcp.agent_access = AgentAccess::Allowed;
        self
    }

    fn validate_arguments(&self, arguments: &serde_json::Value) -> Result<(), String> {
        let Some(mcp) = &self.mcp else {
            return Err(format!("unknown app command: {}", self.id));
        };
        mcp.input_schema
            .validate_value(arguments)
            .map_err(|error| format!("{}: invalid arguments: {error}", self.id))
    }

    pub fn accelerator(mut self, accelerator: impl Into<String>) -> Self {
        self.accelerator = Some(accelerator.into());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn native_menu(mut self, native_menu: bool) -> Self {
        self.native_menu = native_menu;
        self
    }

    pub fn with_shortcut_label(mut self, shortcut_label: impl Into<String>) -> Self {
        self.shortcut_label = Some(shortcut_label.into());
        self
    }

    pub fn direct(self, shortcut: impl Into<String>) -> Self {
        self.direct_when(shortcut, None::<String>)
    }

    pub fn direct_when(
        mut self,
        shortcut: impl Into<String>,
        when: Option<impl Into<String>>,
    ) -> Self {
        self.shortcuts.push(CommandShortcut {
            shortcut: ShortcutDefinition::Direct(shortcut.into()),
            when: when.map(Into::into),
        });
        self
    }

    pub fn chord(self, shortcut: impl Into<String>) -> Self {
        self.chord_when(shortcut, None::<String>)
    }

    pub fn chord_when(
        mut self,
        shortcut: impl Into<String>,
        when: Option<impl Into<String>>,
    ) -> Self {
        self.shortcuts.push(CommandShortcut {
            shortcut: ShortcutDefinition::Chord(shortcut.into()),
            when: when.map(Into::into),
        });
        self
    }

    pub fn command_bar_name(&self) -> String {
        format!("{} > {}", self.group, self.label)
    }

    pub fn localized_name(&self, locale: &str) -> String {
        let locale = Locale::from(locale);
        let message_id = format!("command-{}", self.id.replace('_', "-"));
        let translated = locale.translate(&message_id);
        if translated == message_id {
            return self.command_bar_name();
        }
        let mut segments = translated
            .split(" > ")
            .map(str::to_string)
            .collect::<Vec<_>>();
        let group_count = self.group.split(" > ").count();
        if segments.len() <= group_count {
            return translated;
        }
        for (index, group) in self.group.split(" > ").enumerate() {
            let prefix = if index == 0 { "menu" } else { "command-group" };
            let group_id = format!("{prefix}-{}", Self::kebab_case(group));
            let localized = locale.translate(&group_id);
            if localized != group_id {
                segments[index] = localized;
            }
        }
        segments.join(" > ")
    }

    fn kebab_case(value: &str) -> String {
        value
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>()
            .join("-")
    }

    pub fn shortcut_label(&self) -> String {
        if let Some(label) = &self.shortcut_label {
            return label.clone();
        }
        self.bindings()
            .into_iter()
            .next()
            .map(|binding| binding.shortcut.display())
            .unwrap_or_default()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        let mut bindings = Vec::new();
        for definition in &self.shortcuts {
            let shortcut = match &definition.shortcut {
                ShortcutDefinition::Direct(value) => {
                    let Some(combo) = KeyCombo::parse(value) else {
                        continue;
                    };
                    Shortcut::Direct(combo)
                }
                ShortcutDefinition::Chord(value) => {
                    let Some((prefix, second)) = value.split_once(',') else {
                        continue;
                    };
                    let (Some(prefix), Some(second)) = (
                        KeyCombo::parse(prefix.trim()),
                        KeyCombo::parse(second.trim()),
                    ) else {
                        continue;
                    };
                    Shortcut::Chord(prefix, second)
                }
            };
            bindings.push(Binding {
                shortcut,
                command: self.id.to_string(),
                when: definition.when.as_deref().and_then(When::parse),
            });
        }
        if let Some(accelerator) = &self.accelerator
            && let Some(combo) = KeyCombo::parse(accelerator)
        {
            let shortcut = Shortcut::Direct(combo);
            if !bindings.iter().any(|binding| binding.shortcut == shortcut) {
                bindings.push(Binding {
                    shortcut,
                    command: self.id.to_string(),
                    when: None,
                });
            }
        }
        bindings
    }

    pub fn default_shortcuts(definitions: &[Self]) -> Vec<Binding> {
        let mut bindings = Vec::new();
        for definition in definitions {
            bindings.extend(definition.bindings());
        }
        bindings
    }

    pub fn extend_keymap(definitions: &[Self], keymap: &mut super::shortcut_driver::Keymap) {
        for definition in definitions {
            keymap.register(
                std::iter::once(definition.id.as_str())
                    .chain(definition.aliases.iter().map(String::as_str)),
            );
        }
        keymap.extend(Source::Default, Self::default_shortcuts(definitions));
    }

    pub fn append_native_menus(
        definitions: &[Self],
        menu: &mut muda::Menu,
    ) -> Result<(), muda::Error> {
        for definition in definitions {
            if definition.hidden || !definition.native_menu {
                continue;
            }
            let mut path = definition.group.split(" > ");
            let Some(root_name) = path.next() else {
                continue;
            };
            let existing = menu
                .items()
                .into_iter()
                .filter_map(|item| item.as_submenu().cloned())
                .find(|submenu| submenu.text() == root_name);
            let mut submenu = if let Some(existing) = existing {
                existing
            } else {
                let created = muda::Submenu::new(root_name, true);
                menu.append(&created)?;
                created
            };
            for name in path {
                let existing = submenu
                    .items()
                    .into_iter()
                    .filter_map(|item| item.as_submenu().cloned())
                    .find(|child| child.text() == name);
                submenu = if let Some(existing) = existing {
                    existing
                } else {
                    let created = muda::Submenu::new(name, true);
                    submenu.append(&created)?;
                    created
                };
            }
            let accelerator = definition
                .accelerator
                .as_deref()
                .or_else(|| {
                    definition.shortcuts.iter().find_map(|shortcut| {
                        if shortcut.when.is_some() {
                            return None;
                        }
                        let ShortcutDefinition::Direct(value) = &shortcut.shortcut else {
                            return None;
                        };
                        Some(value.as_str())
                    })
                })
                .and_then(|value| value.parse::<muda::accelerator::Accelerator>().ok());
            let item =
                muda::MenuItem::with_id(&definition.id, &definition.label, true, accelerator);
            submenu.append(&item)?;
        }
        Ok(())
    }
}

impl CommandMessage {
    pub(super) fn of<T>() -> Self
    where
        T: Message + for<'a> TryFrom<&'a CommandInvocation>,
    {
        Self(write_command_message::<T>)
    }
}

fn write_command_message<T>(invocation: &CommandInvocation, commands: &mut Commands)
where
    T: Message + for<'a> TryFrom<&'a CommandInvocation>,
{
    let Ok(message) = T::try_from(invocation) else {
        warn!(command = %invocation.id, "command message rejected its registered definition");
        return;
    };
    commands.write_message(message);
}

impl CommandInvocation {
    pub fn new(caller: Entity, id: impl Into<String>) -> Self {
        Self {
            caller,
            id: id.into(),
            arguments: JsonArguments(serde_json::json!({})),
        }
    }

    pub fn with_arguments(mut self, arguments: serde_json::Value) -> Self {
        self.arguments = JsonArguments(arguments);
        self
    }

    pub fn argument<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        serde_json::from_value(self.arguments.0.get(name)?.clone()).ok()
    }
}

impl CommandDispatch {
    pub fn command(&self) -> Entity {
        self.command
    }

    pub fn invocation(&self) -> &CommandInvocation {
        &self.invocation
    }
}
