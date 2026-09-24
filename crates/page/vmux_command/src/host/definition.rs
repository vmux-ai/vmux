use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use bevy::prelude::*;
use vmux_ui::i18n::Locale;

use crate::shortcut::{Binding, KeyCombo, Modifiers, Shortcut, Source, When, resolve_key};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WriteCommandRequests;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadCommandRequests;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum ShortcutDefinition {
    Direct(String),
    Chord(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct CommandShortcut {
    pub shortcut: ShortcutDefinition,
    pub when: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandManifestEntry<K> {
    kind: K,
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    label: String,
    group: String,
    #[serde(default)]
    accelerator: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default = "CommandManifestEntry::<K>::native_menu_default")]
    native_menu: bool,
    #[serde(default)]
    shortcut_label: Option<String>,
    #[serde(default)]
    shortcuts: Vec<CommandShortcut>,
}

impl<K> CommandManifestEntry<K> {
    fn native_menu_default() -> bool {
        true
    }

    fn into_command(self) -> (CommandDefinition, K) {
        (
            CommandDefinition {
                id: self.id,
                aliases: self.aliases,
                label: self.label,
                group: self.group,
                accelerator: self.accelerator,
                hidden: self.hidden,
                native_menu: self.native_menu,
                shortcut_label: self.shortcut_label,
                shortcuts: self.shortcuts,
                mcp: None,
            },
            self.kind,
        )
    }
}

pub struct CommandManifest<K>(Vec<CommandManifestEntry<K>>);

impl<K: serde::de::DeserializeOwned> CommandManifest<K> {
    pub fn from_ron(source: &str) -> Self {
        let entries =
            ron::from_str(source).expect("embedded command definitions must be valid RON");
        Self(entries)
    }

    pub fn into_commands(self) -> impl Iterator<Item = (CommandDefinition, K)> {
        self.0.into_iter().map(CommandManifestEntry::into_command)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentAccess {
    Denied,
    Allowed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMcp {
    pub description: String,
    pub input_schema: vmux_api::InputSchema,
    pub agent_access: AgentAccess,
}

impl CommandMcp {
    pub fn new(description: impl Into<String>, input_schema: vmux_api::InputSchema) -> Self {
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

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CommandDefinition {
    pub id: String,
    pub aliases: Vec<String>,
    pub label: String,
    pub group: String,
    pub accelerator: Option<String>,
    pub hidden: bool,
    pub native_menu: bool,
    pub shortcut_label: Option<String>,
    pub shortcuts: Vec<CommandShortcut>,
    pub mcp: Option<CommandMcp>,
}

impl CommandDefinition {
    pub fn inferred(module_path: &str, request_type: &str) -> Self {
        let crate_namespace = module_path
            .split("::")
            .next()
            .and_then(|name| name.strip_prefix("vmux_"))
            .unwrap_or(module_path);
        let action = request_type.strip_suffix("Request").unwrap_or(request_type);
        let action = Self::snake_case(action);
        let scope = Self::command_scope(module_path);
        let id = if let Some(scope) = scope {
            if action.split('_').any(|word| word == scope) {
                action.clone()
            } else {
                format!("{scope}_{action}")
            }
        } else if action.contains('_') {
            action.clone()
        } else {
            format!("{crate_namespace}_{action}")
        };
        let group = Self::inferred_group(&id);
        let group_name = group.rsplit(" > ").next().unwrap_or(&group);
        let group_key = Self::snake_case(group_name);
        let label_action = action
            .strip_prefix(&format!("{group_key}_"))
            .unwrap_or(&action);
        let label = if !action.contains('_') && id != action {
            format!("{} {group_name}", Self::title_case(label_action))
        } else {
            Self::title_case(label_action)
        };
        Self::new(id, label, group)
    }

    fn command_scope(module_path: &str) -> Option<&str> {
        let crate_name = module_path.split("::").next()?;
        module_path.rsplit("::").find(|segment| {
            *segment != crate_name
                && !matches!(
                    *segment,
                    "host"
                        | "command"
                        | "command_bar"
                        | "definition"
                        | "handler"
                        | "key"
                        | "plugin"
                        | "tests"
                        | "view"
                )
        })
    }

    fn inferred_group(id: &str) -> String {
        if id == "open_settings" || id.ends_with("_window") {
            return "Layout > Window".to_string();
        }
        if id == "toggle_layout" {
            return "Layout > Layout".to_string();
        }
        if id.starts_with("space_") {
            return "Layout > Space".to_string();
        }
        let namespace = id.split('_').next().unwrap_or(id);
        Self::title_case(namespace)
    }

    fn snake_case(value: &str) -> String {
        let mut result = String::new();
        for character in value.chars() {
            if character.is_ascii_uppercase() && !result.is_empty() {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        }
        result
    }

    fn title_case(value: &str) -> String {
        let mut words = Vec::new();
        for word in value.split('_') {
            let word = match word {
                "prev" => "Previous".to_string(),
                _ => {
                    let mut characters = word.chars();
                    let Some(first) = characters.next() else {
                        continue;
                    };
                    format!("{}{}", first.to_ascii_uppercase(), characters.as_str())
                }
            };
            words.push(word);
        }
        words.join(" ")
    }

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
            mcp: None,
        }
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    pub fn agent_tool(&self) -> Option<vmux_api::protocol::AgentCommandTool> {
        let mcp = self.mcp.as_ref()?;
        Some(vmux_api::protocol::AgentCommandTool {
            name: self.id.clone(),
            description: mcp.description.clone(),
            input_schema: vmux_api::json::JsonValue::from(mcp.input_schema.to_json()),
        })
    }

    pub fn mcp(mut self, definition: CommandMcp) -> Self {
        self.mcp = Some(definition);
        self
    }

    pub fn expose_to_mcp(mut self) -> Self {
        self.mcp = Some(CommandMcp::new(
            self.label.clone(),
            vmux_api::InputSchema::object(),
        ));
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

    pub fn extend_keymap(definitions: &[Self], keymap: &mut crate::shortcut::Keymap) {
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

#[derive(Message, Clone, Debug, PartialEq)]
pub struct CommandInvocation {
    pub caller: Entity,
    pub id: String,
    pub arguments: serde_json::Value,
}

impl CommandInvocation {
    pub fn new(caller: Entity, id: impl Into<String>) -> Self {
        Self {
            caller,
            id: id.into(),
            arguments: serde_json::json!({}),
        }
    }

    pub fn with_arguments(mut self, arguments: serde_json::Value) -> Self {
        self.arguments = arguments;
        self
    }

    pub fn argument<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        serde_json::from_value(self.arguments.get(name)?.clone()).ok()
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchCommandInvocations;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegisterCommandDefinitions;

pub trait CommandRequest: Message + for<'a> TryFrom<&'a CommandInvocation> {
    fn definitions() -> Vec<CommandDefinition>;
}

pub struct CommandTypePlugin<T>(PhantomData<fn() -> T>);

impl<T> Default for CommandTypePlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: CommandRequest> Plugin for CommandTypePlugin<T> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_message::<T>().add_systems(
            Startup,
            spawn_command_definitions::<T>.in_set(RegisterCommandDefinitions),
        );
    }
}

fn spawn_command_definitions<T: CommandRequest>(mut commands: Commands) {
    for definition in T::definitions() {
        commands.spawn(definition).observe(dispatch_request::<T>);
    }
}

pub struct CommandRuntimePlugin;

impl Plugin for CommandRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandCatalog>()
            .init_resource::<crate::shortcut::Keymap>()
            .add_message::<CommandInvocation>()
            .configure_sets(
                Update,
                (
                    WriteCommandRequests,
                    DispatchCommandInvocations,
                    crate::snapshot::WriteCommandBarSnapshots,
                    ReadCommandRequests,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    index_command_definitions,
                    dispatch_command_invocations,
                    bevy::ecs::schedule::ApplyDeferred,
                )
                    .chain()
                    .in_set(DispatchCommandInvocations),
            );
    }
}

#[derive(Clone)]
struct RegisteredCommand {
    definition: CommandDefinition,
}

#[derive(Resource, Default)]
pub struct CommandCatalog {
    ids: HashMap<String, Entity>,
    commands: HashMap<Entity, RegisteredCommand>,
}

fn index_command_definitions(
    definitions: Query<(Entity, &CommandDefinition), Added<CommandDefinition>>,
    mut catalog: ResMut<CommandCatalog>,
    mut commands: Commands,
) {
    for (entity, definition) in &definitions {
        let mut ids = Vec::with_capacity(definition.aliases.len() + 1);
        ids.push(definition.id.clone());
        ids.extend(definition.aliases.iter().cloned());
        let mut unique_ids = HashSet::with_capacity(ids.len());
        for id in &ids {
            assert!(
                unique_ids.insert(id.as_str()) && !catalog.ids.contains_key(id),
                "duplicate command id: {id}"
            );
        }
        for id in ids {
            catalog.ids.insert(id, entity);
        }
        catalog.commands.insert(
            entity,
            RegisteredCommand {
                definition: definition.clone(),
            },
        );
        commands
            .entity(entity)
            .insert(Name::new(definition.id.clone()));
    }
}

#[derive(EntityEvent)]
pub struct CommandDispatch {
    #[event_target]
    command: Entity,
    invocation: CommandInvocation,
}

impl CommandDispatch {
    pub fn command(&self) -> Entity {
        self.command
    }

    pub fn invocation(&self) -> &CommandInvocation {
        &self.invocation
    }
}

impl CommandCatalog {
    pub fn tools(&self) -> Vec<vmux_api::protocol::AgentCommandTool> {
        let mut tools = Vec::new();
        for command in self.commands.values() {
            tools.extend(command.definition.agent_tool());
        }
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    pub fn resolve(
        &self,
        caller: Entity,
        id: &str,
        arguments: serde_json::Value,
    ) -> Result<CommandInvocation, String> {
        self.resolve_with_access(caller, id, arguments, false)
    }

    pub fn resolve_agent(
        &self,
        caller: Entity,
        id: &str,
        arguments: serde_json::Value,
    ) -> Result<CommandInvocation, String> {
        self.resolve_with_access(caller, id, arguments, true)
    }

    fn resolve_with_access(
        &self,
        caller: Entity,
        id: &str,
        arguments: serde_json::Value,
        require_agent_access: bool,
    ) -> Result<CommandInvocation, String> {
        let Some(entity) = self.ids.get(id) else {
            return Err(format!("unknown app command: {id}"));
        };
        let command = self
            .commands
            .get(entity)
            .expect("indexed command entity must have a catalog entry");
        let definition = &command.definition;
        let Some(mcp) = &definition.mcp else {
            return Err(format!("unknown app command: {id}"));
        };
        if require_agent_access && mcp.agent_access != AgentAccess::Allowed {
            return Err("focus-changing app command is disabled for agents".to_string());
        }
        definition.validate_arguments(&arguments)?;
        Ok(CommandInvocation::new(caller, &definition.id).with_arguments(arguments))
    }
}

fn dispatch_command_invocations(
    mut invocations: MessageReader<CommandInvocation>,
    catalog: Res<CommandCatalog>,
    mut commands: Commands,
) {
    for invocation in invocations.read() {
        let Some(&command) = catalog.ids.get(&invocation.id) else {
            continue;
        };
        let definition = &catalog
            .commands
            .get(&command)
            .expect("indexed command entity must have a catalog entry")
            .definition;
        let mut invocation = invocation.clone();
        invocation.id.clone_from(&definition.id);
        if let Some(mcp) = &definition.mcp
            && let Err(error) = mcp.input_schema.validate_value(&invocation.arguments)
        {
            warn!(command = %definition.id, %error, "invalid command arguments");
            continue;
        }
        commands.trigger(CommandDispatch {
            command,
            invocation,
        });
    }
}

fn dispatch_request<T: CommandRequest>(
    trigger: On<CommandDispatch>,
    mut requests: MessageWriter<T>,
) {
    let Ok(request) = T::try_from(&trigger.invocation) else {
        warn!(command = %trigger.invocation.id, "command request rejected its registered definition");
        return;
    };
    requests.write(request);
}

impl KeyCombo {
    fn parse(value: &str) -> Option<Self> {
        let mut modifiers = Modifiers::default();
        let mut key = None;
        for part in value.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" | "option" => modifiers.alt = true,
                "super" | "cmd" | "command" | "meta" | "cmdorctrl" => {
                    modifiers.super_key = true;
                }
                _ => {
                    if key.is_some() {
                        return None;
                    }
                    let resolved = resolve_key(part)?;
                    key = Some(resolved.key);
                    modifiers.shift |= resolved.implicit_shift;
                }
            }
        }
        Some(Self {
            key: key?,
            modifiers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(vmux_macro::CommandBar)]
    #[shortcut(direct = "Super+t")]
    struct TestToggleRequest;

    #[derive(vmux_macro::CommandBar)]
    #[mcp(agent)]
    struct AgentVisibleRequest;

    #[derive(vmux_macro::CommandBar)]
    #[mcp(description = "User only")]
    struct UserOnlyRequest;

    #[derive(Message)]
    struct DuplicateCommandA;

    impl CommandRequest for DuplicateCommandA {
        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("duplicate", "First", "Test")]
        }
    }

    impl TryFrom<&CommandInvocation> for DuplicateCommandA {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Err(())
        }
    }

    #[derive(Message)]
    struct DuplicateCommandB;

    impl CommandRequest for DuplicateCommandB {
        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("duplicate", "Second", "Test")]
        }
    }

    impl TryFrom<&CommandInvocation> for DuplicateCommandB {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Err(())
        }
    }

    #[derive(Message, Debug, PartialEq, Eq)]
    struct AliasedCommand(String);

    impl CommandRequest for AliasedCommand {
        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("canonical", "Canonical", "Test").alias("legacy")]
        }
    }

    impl TryFrom<&CommandInvocation> for AliasedCommand {
        type Error = ();

        fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Ok(Self(invocation.id.clone()))
        }
    }

    #[derive(Message)]
    struct DuplicateAlias;

    impl CommandRequest for DuplicateAlias {
        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("alias_owner", "Alias", "Test").alias("duplicate")]
        }
    }

    impl TryFrom<&CommandInvocation> for DuplicateAlias {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Err(())
        }
    }

    #[test]
    fn registered_command_dispatches_to_its_typed_message() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandTypePlugin::<TestToggleRequest>::default());
        let caller = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(caller, "test_toggle"));

        app.update();

        let request_count = app
            .world_mut()
            .resource_mut::<Messages<TestToggleRequest>>()
            .drain()
            .count();
        assert_eq!(request_count, 1);
    }

    #[test]
    fn registered_command_contributes_metadata_and_shortcuts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandTypePlugin::<TestToggleRequest>::default());
        app.update();
        let mut query = app.world_mut().query::<&CommandDefinition>();
        let definitions = query.iter(app.world()).cloned().collect::<Vec<_>>();
        let definition = definitions
            .iter()
            .find(|definition| definition.id == "test_toggle")
            .unwrap();

        assert_eq!(definition.command_bar_name(), "Test > Toggle");
        assert_eq!(definition.shortcut_label(), "⌘T");
        assert_eq!(
            CommandDefinition::default_shortcuts(&definitions)[0].command,
            "test_toggle"
        );
    }

    #[test]
    fn alias_dispatches_with_the_canonical_id() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandTypePlugin::<AliasedCommand>::default());
        let caller = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(caller, "legacy"));

        app.update();

        let requests = app
            .world_mut()
            .resource_mut::<Messages<AliasedCommand>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests, [AliasedCommand("canonical".to_string())]);
    }

    #[test]
    #[should_panic(expected = "duplicate command id: duplicate")]
    fn duplicate_command_ids_are_rejected() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((
            CommandTypePlugin::<DuplicateCommandA>::default(),
            CommandTypePlugin::<DuplicateCommandB>::default(),
        ));
        app.update();
    }

    #[test]
    #[should_panic(expected = "duplicate command id: duplicate")]
    fn aliases_cannot_collide_with_command_ids() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((
            CommandTypePlugin::<DuplicateCommandA>::default(),
            CommandTypePlugin::<DuplicateAlias>::default(),
        ));
        app.update();
    }

    #[test]
    fn command_catalog_exposes_and_dispatches_the_registered_request_definition() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((
            CommandTypePlugin::<AgentVisibleRequest>::default(),
            CommandTypePlugin::<UserOnlyRequest>::default(),
        ));
        app.update();

        let tools = app.world().resource::<CommandCatalog>().tools();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["agent_visible", "user_only"],
        );

        let caller = app.world_mut().spawn_empty().id();
        let invocation = app
            .world()
            .resource::<CommandCatalog>()
            .resolve_agent(caller, "agent_visible", serde_json::json!({}))
            .unwrap();
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(invocation);
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<AgentVisibleRequest>>()
                .drain()
                .count(),
            1,
        );
    }

    #[test]
    fn command_catalog_rejects_unlisted_access_and_malformed_arguments() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((
            CommandTypePlugin::<AgentVisibleRequest>::default(),
            CommandTypePlugin::<UserOnlyRequest>::default(),
        ));
        app.update();
        let caller = app.world_mut().spawn_empty().id();

        let denied = app
            .world()
            .resource::<CommandCatalog>()
            .resolve_agent(caller, "user_only", serde_json::json!({}))
            .unwrap_err();
        assert_eq!(
            denied,
            "focus-changing app command is disabled for agents".to_string(),
        );

        let malformed = app
            .world()
            .resource::<CommandCatalog>()
            .resolve_agent(
                caller,
                "agent_visible",
                serde_json::json!({"unexpected": true}),
            )
            .unwrap_err();
        assert_eq!(
            malformed,
            "agent_visible: invalid arguments: unknown argument unexpected".to_string(),
        );
    }
}
