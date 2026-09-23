use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;

use crate::shortcut::{Binding, KeyCombo, Modifiers, Shortcut, Source, When, resolve_key};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WriteCommandRequests;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadCommandRequests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutDefinition {
    Direct(String),
    Chord(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandShortcut {
    pub shortcut: ShortcutDefinition,
    pub when: Option<String>,
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
    pub fn register<T: Message>(
        app: &mut App,
        definitions: fn() -> Vec<Self>,
        request: fn(&CommandInvocation) -> Option<T>,
    ) {
        Self::install_runtime(app);
        let validator = RequestValidator(Arc::new(move |invocation| {
            request(invocation)
                .map(drop)
                .ok_or_else(|| format!("{} rejected its command arguments", invocation.id))
        }));
        app.add_message::<T>().add_systems(
            Startup,
            (move |mut index: ResMut<CommandIndex>, mut commands: Commands| {
                for definition in definitions() {
                    let mut ids = Vec::with_capacity(definition.aliases.len() + 1);
                    ids.push(definition.id.clone());
                    ids.extend(definition.aliases.iter().cloned());
                    let mut unique_ids = HashSet::with_capacity(ids.len());
                    for id in &ids {
                        assert!(
                            unique_ids.insert(id.as_str()) && !index.0.contains_key(id),
                            "duplicate command id: {id}"
                        );
                    }
                    let command = commands
                        .spawn((
                            Name::new(definition.id.clone()),
                            definition,
                            RequestParser(request),
                            validator.clone(),
                        ))
                        .observe(dispatch_request::<T>)
                        .id();
                    for id in ids {
                        index.0.insert(id, command);
                    }
                }
            })
            .in_set(RegisterCommandDefinitions),
        );
    }

    pub(crate) fn install_runtime(app: &mut App) {
        if app.world().contains_resource::<CommandRuntime>() {
            return;
        }
        app.insert_resource(CommandRuntime)
            .init_resource::<CommandIndex>()
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
                    dispatch_command_invocations,
                    bevy::ecs::schedule::ApplyDeferred,
                )
                    .chain()
                    .in_set(DispatchCommandInvocations),
            );
    }

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

    fn matches(&self, id: &str) -> bool {
        self.id == id || self.aliases.iter().any(|alias| alias == id)
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

#[derive(Resource, Default)]
struct CommandIndex(HashMap<String, Entity>);

#[derive(Resource)]
struct CommandRuntime;

#[derive(EntityEvent)]
struct InvokeCommand {
    #[event_target]
    command: Entity,
    invocation: CommandInvocation,
}

#[derive(Component)]
struct RequestParser<T: Message>(fn(&CommandInvocation) -> Option<T>);

type ValidateRequest = dyn Fn(&CommandInvocation) -> Result<(), String> + Send + Sync + 'static;

#[derive(Component, Clone)]
struct RequestValidator(Arc<ValidateRequest>);

#[derive(bevy::ecs::system::SystemParam)]
pub struct CommandCatalog<'w, 's> {
    definitions: Query<'w, 's, (&'static CommandDefinition, &'static RequestValidator)>,
    invocations: MessageWriter<'w, CommandInvocation>,
}

impl CommandCatalog<'_, '_> {
    pub fn tools(&self) -> Vec<vmux_api::protocol::AgentCommandTool> {
        let mut tools = Vec::new();
        for (definition, _) in &self.definitions {
            tools.extend(definition.agent_tool());
        }
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    pub fn invoke(
        &mut self,
        caller: Entity,
        id: &str,
        arguments: serde_json::Value,
    ) -> Result<(), String> {
        self.invoke_with_access(caller, id, arguments, false)
    }

    pub fn invoke_agent(
        &mut self,
        caller: Entity,
        id: &str,
        arguments: serde_json::Value,
    ) -> Result<(), String> {
        self.invoke_with_access(caller, id, arguments, true)
    }

    fn invoke_with_access(
        &mut self,
        caller: Entity,
        id: &str,
        arguments: serde_json::Value,
        require_agent_access: bool,
    ) -> Result<(), String> {
        let Some((definition, validator)) = self
            .definitions
            .iter()
            .find(|(definition, _)| definition.matches(id))
        else {
            return Err(format!("unknown app command: {id}"));
        };
        let Some(mcp) = &definition.mcp else {
            return Err(format!("unknown app command: {id}"));
        };
        if require_agent_access && mcp.agent_access != AgentAccess::Allowed {
            return Err("focus-changing app command is disabled for agents".to_string());
        }
        definition.validate_arguments(&arguments)?;
        let invocation = CommandInvocation::new(caller, &definition.id).with_arguments(arguments);
        validator.0(&invocation)?;
        self.invocations.write(invocation);
        Ok(())
    }
}

fn dispatch_command_invocations(
    mut invocations: MessageReader<CommandInvocation>,
    index: Res<CommandIndex>,
    definitions: Query<&CommandDefinition>,
    mut commands: Commands,
) {
    for invocation in invocations.read() {
        let Some(&command) = index.0.get(&invocation.id) else {
            continue;
        };
        let Ok(definition) = definitions.get(command) else {
            continue;
        };
        let mut invocation = invocation.clone();
        invocation.id.clone_from(&definition.id);
        if let Some(mcp) = &definition.mcp
            && let Err(error) = mcp.input_schema.validate_value(&invocation.arguments)
        {
            warn!(command = %definition.id, %error, "invalid command arguments");
            continue;
        }
        commands.trigger(InvokeCommand {
            command,
            invocation,
        });
    }
}

fn dispatch_request<T: Message>(
    trigger: On<InvokeCommand>,
    parsers: Query<&RequestParser<T>>,
    mut requests: MessageWriter<T>,
) {
    let Ok(parser) = parsers.get(trigger.command) else {
        return;
    };
    if let Some(request) = parser.0(&trigger.invocation) {
        requests.write(request);
    }
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
    use bevy::ecs::system::RunSystemOnce;

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

    impl DuplicateCommandA {
        fn register(app: &mut App) {
            CommandDefinition::register(app, Self::definitions, Self::from_invocation);
        }

        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("duplicate", "First", "Test")]
        }

        fn from_invocation(_invocation: &CommandInvocation) -> Option<Self> {
            None
        }
    }

    #[derive(Message)]
    struct DuplicateCommandB;

    impl DuplicateCommandB {
        fn register(app: &mut App) {
            CommandDefinition::register(app, Self::definitions, Self::from_invocation);
        }

        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("duplicate", "Second", "Test")]
        }

        fn from_invocation(_invocation: &CommandInvocation) -> Option<Self> {
            None
        }
    }

    #[derive(Message, Debug, PartialEq, Eq)]
    struct AliasedCommand(String);

    impl AliasedCommand {
        fn register(app: &mut App) {
            CommandDefinition::register(app, Self::definitions, Self::from_invocation);
        }

        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("canonical", "Canonical", "Test").alias("legacy")]
        }

        fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
            Some(Self(invocation.id.clone()))
        }
    }

    #[derive(Message)]
    struct DuplicateAlias;

    impl DuplicateAlias {
        fn register(app: &mut App) {
            CommandDefinition::register(app, Self::definitions, Self::from_invocation);
        }

        fn definitions() -> Vec<CommandDefinition> {
            vec![CommandDefinition::new("alias_owner", "Alias", "Test").alias("duplicate")]
        }

        fn from_invocation(_invocation: &CommandInvocation) -> Option<Self> {
            None
        }
    }

    #[test]
    fn registered_command_dispatches_to_its_typed_message() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        TestToggleRequest::register(&mut app);
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
        TestToggleRequest::register(&mut app);
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
        AliasedCommand::register(&mut app);
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
        DuplicateCommandA::register(&mut app);
        DuplicateCommandB::register(&mut app);
        app.update();
    }

    #[test]
    #[should_panic(expected = "duplicate command id: duplicate")]
    fn aliases_cannot_collide_with_command_ids() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        DuplicateCommandA::register(&mut app);
        DuplicateAlias::register(&mut app);
        app.update();
    }

    #[test]
    fn command_catalog_exposes_and_dispatches_the_registered_request_definition() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        AgentVisibleRequest::register(&mut app);
        UserOnlyRequest::register(&mut app);
        app.update();

        let tools = app
            .world_mut()
            .run_system_once(|catalog: CommandCatalog| catalog.tools())
            .unwrap();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["agent_visible", "user_only"],
        );

        let caller = app.world_mut().spawn_empty().id();
        let result = app
            .world_mut()
            .run_system_once(move |mut catalog: CommandCatalog| {
                catalog.invoke_agent(caller, "agent_visible", serde_json::json!({}))
            })
            .unwrap();
        assert_eq!(result, Ok(()));
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
        AgentVisibleRequest::register(&mut app);
        UserOnlyRequest::register(&mut app);
        app.update();
        let caller = app.world_mut().spawn_empty().id();

        let denied = app
            .world_mut()
            .run_system_once(move |mut catalog: CommandCatalog| {
                catalog.invoke_agent(caller, "user_only", serde_json::json!({}))
            })
            .unwrap();
        assert_eq!(
            denied,
            Err("focus-changing app command is disabled for agents".to_string()),
        );

        let malformed = app
            .world_mut()
            .run_system_once(move |mut catalog: CommandCatalog| {
                catalog.invoke_agent(
                    caller,
                    "agent_visible",
                    serde_json::json!({"unexpected": true}),
                )
            })
            .unwrap();
        assert_eq!(
            malformed,
            Err("agent_visible: invalid arguments: unknown argument unexpected".to_string()),
        );
    }
}
