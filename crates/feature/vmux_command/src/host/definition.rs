use std::collections::HashSet;

use bevy::{ecs::system::SystemParam, prelude::*};
use vmux_api::JsonSchema;
use vmux_ecs::JsonArguments;
use vmux_ecs::manifest::{self, FeatureManifest};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WriteCommandRequests;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadCommandRequests;

pub struct CommandRuntimePlugin;

impl Plugin for CommandRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandInvocation>()
            .configure_sets(
                Startup,
                (
                    CommandStartupSet::Manifest,
                    CommandStartupSet::Flush,
                    BindCommands,
                )
                    .chain(),
            )
            .add_systems(
                Startup,
                (
                    spawn.before(BindCommands),
                    register.in_set(CommandStartupSet::Manifest),
                    ApplyDeferred.in_set(CommandStartupSet::Flush),
                ),
            )
            .configure_sets(
                Update,
                (
                    WriteCommandRequests,
                    DispatchCommandInvocations,
                    crate::host::snapshot::WriteCommandBarSnapshots,
                    ReadCommandRequests,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (validate, dispatch, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(DispatchCommandInvocations),
            );
        for binding in inventory::iter::<CommandBindingRegistration> {
            (binding.register)(app);
        }
        for message in inventory::iter::<CommandMessageRegistration> {
            (message.register)(app);
        }
    }
}

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

pub struct CommandManifest(pub(super) Vec<manifest::Command>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AgentAccess {
    Denied,
    Allowed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMcp {
    pub description: String,
    pub input_schema: JsonSchema,
    pub(super) agent_access: AgentAccess,
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
    pub toolbar: Option<CommandToolbar>,
    pub mcp: Option<CommandMcp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandToolbar {
    pub icon: vmux_api::BuiltinIcon,
    pub rank: i32,
}

#[derive(Message, Clone, Debug, PartialEq)]
pub struct CommandInvocation {
    pub caller: Entity,
    pub id: String,
    pub arguments: JsonArguments,
}

#[derive(Component, Clone, Copy)]
pub(super) struct CommandMessage(pub(super) fn(&CommandInvocation, &mut Commands));

pub trait CommandBinding: Bundle + Sized {
    fn for_command(id: &str) -> Option<Self>;
}

pub struct CommandBindingRegistration {
    register: fn(&mut App),
}

impl CommandBindingRegistration {
    pub const fn of<T: CommandBinding>() -> Self {
        Self {
            register: register_binding::<T>,
        }
    }
}

inventory::collect!(CommandBindingRegistration);

pub struct CommandMessageRegistration {
    register: fn(&mut App),
}

impl CommandMessageRegistration {
    pub const fn of<T>() -> Self
    where
        T: Message + for<'a> TryFrom<&'a CommandInvocation>,
    {
        Self {
            register: register_message::<T>,
        }
    }
}

inventory::collect!(CommandMessageRegistration);

fn register_binding<T: CommandBinding>(app: &mut App) {
    app.add_systems(Startup, bind::<T>.in_set(BindCommands));
}

fn bind<T: CommandBinding>(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<T>(&mut commands);
}

fn register_message<T>(app: &mut App)
where
    T: Message + for<'a> TryFrom<&'a CommandInvocation>,
{
    app.add_systems(Startup, bind_message::<T>.in_set(BindCommands));
}

fn bind_message<T>(registry: CommandRegistry, mut commands: Commands)
where
    T: Message + for<'a> TryFrom<&'a CommandInvocation>,
{
    registry.message::<T>(&mut commands);
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchCommandInvocations;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindCommands;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
enum CommandStartupSet {
    Manifest,
    Flush,
}

#[derive(SystemParam)]
pub struct CommandRegistry<'w, 's> {
    definitions: Query<'w, 's, (Entity, &'static CommandDefinition)>,
}

impl CommandRegistry<'_, '_> {
    pub fn bind<B: CommandBinding>(&self, commands: &mut Commands) {
        for (entity, definition) in &self.definitions {
            if let Some(bundle) = B::for_command(&definition.id) {
                commands.entity(entity).insert(bundle);
            }
        }
    }

    pub fn message<T>(&self, commands: &mut Commands)
    where
        T: Message + for<'a> TryFrom<&'a CommandInvocation>,
    {
        for (entity, definition) in &self.definitions {
            let invocation = CommandInvocation::new(Entity::PLACEHOLDER, &definition.id);
            if T::try_from(&invocation).is_ok() {
                commands.entity(entity).insert(CommandMessage::of::<T>());
            }
        }
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command keymap"),
        crate::host::shortcut_driver::Keymap::default(),
    ));
}

fn register(manifests: Query<&FeatureManifest>, mut commands: Commands) {
    for manifest in &manifests {
        for definition in manifest
            .commands
            .iter()
            .cloned()
            .map(CommandDefinition::from)
        {
            commands.spawn(definition);
        }
    }
}

fn validate(
    added: Query<(Entity, &CommandDefinition), Added<CommandDefinition>>,
    definitions: Query<(Entity, &CommandDefinition)>,
    mut commands: Commands,
) {
    for (entity, definition) in &added {
        let mut ids = Vec::with_capacity(definition.aliases.len() + 1);
        ids.push(definition.id.clone());
        ids.extend(definition.aliases.iter().cloned());
        let mut unique_ids = HashSet::with_capacity(ids.len());
        for id in &ids {
            assert!(unique_ids.insert(id.as_str()), "duplicate command id: {id}");
            for (other_entity, other) in &definitions {
                assert!(
                    other_entity == entity || !other.matches(id),
                    "duplicate command id: {id}"
                );
            }
        }
        commands
            .entity(entity)
            .insert(Name::new(definition.id.clone()));
    }
}

#[derive(EntityEvent)]
pub struct CommandDispatch {
    #[event_target]
    pub(super) command: Entity,
    pub(super) invocation: CommandInvocation,
}

fn dispatch(
    mut invocations: MessageReader<CommandInvocation>,
    definitions: Query<(Entity, &CommandDefinition, Option<&CommandMessage>)>,
    mut commands: Commands,
) {
    for invocation in invocations.read() {
        let Some((command, definition, message)) = definitions
            .iter()
            .find(|(_, definition, _)| definition.matches(&invocation.id))
        else {
            continue;
        };
        let mut invocation = invocation.clone();
        invocation.id.clone_from(&definition.id);
        if let Some(mcp) = &definition.mcp
            && let Err(error) = mcp.input_schema.validate_value(&invocation.arguments.0)
        {
            warn!(command = %definition.id, %error, "invalid command arguments");
            continue;
        }
        if let Some(message) = message {
            (message.0)(&invocation, &mut commands);
        }
        commands.trigger(CommandDispatch {
            command,
            invocation,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_from_feature_manifest() {
        let definitions = CommandManifest::from_feature_ron(
            r#"(
                commands: [(id: "test", label: "Test", group: "Test")],
                ignored: true,
            )"#,
        )
        .into_vec();

        assert_eq!(definitions[0].id, "test");
    }

    #[derive(Message)]
    struct TestToggleRequest;

    impl TryFrom<&CommandInvocation> for TestToggleRequest {
        type Error = ();

        fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            (invocation.id == "test_toggle").then_some(Self).ok_or(())
        }
    }

    #[derive(Message)]
    struct AgentVisibleRequest;

    impl TryFrom<&CommandInvocation> for AgentVisibleRequest {
        type Error = ();

        fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            (invocation.id == "agent_visible").then_some(Self).ok_or(())
        }
    }

    #[derive(Message)]
    struct UserOnlyRequest;

    impl TryFrom<&CommandInvocation> for UserOnlyRequest {
        type Error = ();

        fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            (invocation.id == "user_only").then_some(Self).ok_or(())
        }
    }

    #[derive(Message)]
    struct DuplicateCommandA;

    impl TryFrom<&CommandInvocation> for DuplicateCommandA {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Err(())
        }
    }

    #[derive(Message)]
    struct DuplicateCommandB;

    impl TryFrom<&CommandInvocation> for DuplicateCommandB {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Err(())
        }
    }

    #[derive(Message, Debug, PartialEq, Eq)]
    struct AliasedCommand(String);

    impl TryFrom<&CommandInvocation> for AliasedCommand {
        type Error = ();

        fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Ok(Self(invocation.id.clone()))
        }
    }

    #[derive(Message)]
    struct DuplicateAlias;

    impl TryFrom<&CommandInvocation> for DuplicateAlias {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Err(())
        }
    }

    struct CommandTestApp;

    impl CommandTestApp {
        fn empty() -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CommandRuntimePlugin));
            app
        }

        fn register<T>(app: &mut App, definition: CommandDefinition)
        where
            T: Message + for<'a> TryFrom<&'a CommandInvocation>,
        {
            app.add_message::<T>();
            app.world_mut().spawn(definition.message::<T>());
        }

        fn test_toggle() -> App {
            let mut app = Self::empty();
            Self::register::<TestToggleRequest>(
                &mut app,
                CommandDefinition::new("test_toggle", "Toggle", "Test").direct("Super+t"),
            );
            app
        }

        fn agent_commands() -> App {
            let mut app = Self::empty();
            Self::register::<AgentVisibleRequest>(
                &mut app,
                CommandDefinition::new("agent_visible", "Visible", "Agent")
                    .mcp(CommandMcp::new("Visible", vmux_api::JsonSchema::object()).allow_agent()),
            );
            Self::register::<UserOnlyRequest>(
                &mut app,
                CommandDefinition::new("user_only", "Only", "User")
                    .mcp(CommandMcp::new("User only", vmux_api::JsonSchema::object())),
            );
            app
        }
    }

    #[test]
    fn registered_command_dispatches_to_its_typed_message() {
        let mut app = CommandTestApp::test_toggle();
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
        let mut app = CommandTestApp::test_toggle();
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
        let mut app = CommandTestApp::empty();
        CommandTestApp::register::<AliasedCommand>(
            &mut app,
            CommandDefinition::new("canonical", "Canonical", "Test").alias("legacy"),
        );
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
        let mut app = CommandTestApp::empty();
        CommandTestApp::register::<DuplicateCommandA>(
            &mut app,
            CommandDefinition::new("duplicate", "First", "Test"),
        );
        CommandTestApp::register::<DuplicateCommandB>(
            &mut app,
            CommandDefinition::new("duplicate", "Second", "Test"),
        );
        app.update();
    }

    #[test]
    #[should_panic(expected = "duplicate command id: duplicate")]
    fn aliases_cannot_collide_with_command_ids() {
        let mut app = CommandTestApp::empty();
        CommandTestApp::register::<DuplicateCommandA>(
            &mut app,
            CommandDefinition::new("duplicate", "First", "Test"),
        );
        CommandTestApp::register::<DuplicateAlias>(
            &mut app,
            CommandDefinition::new("alias_owner", "Alias", "Test").alias("duplicate"),
        );
        app.update();
    }

    #[test]
    fn command_entities_expose_and_dispatch_the_registered_request_definition() {
        let mut app = CommandTestApp::agent_commands();
        app.update();

        let definitions = {
            let mut query = app.world_mut().query::<&CommandDefinition>();
            query.iter(app.world()).cloned().collect::<Vec<_>>()
        };
        let mut tools = definitions
            .iter()
            .filter_map(CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["agent_visible", "user_only"],
        );

        let caller = app.world_mut().spawn_empty().id();
        let invocation = definitions
            .iter()
            .find(|definition| definition.matches("agent_visible"))
            .unwrap()
            .agent_invocation(caller, serde_json::json!({}))
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
    fn command_entities_reject_unlisted_access_and_malformed_arguments() {
        let mut app = CommandTestApp::agent_commands();
        app.update();
        let caller = app.world_mut().spawn_empty().id();
        let definitions = {
            let mut query = app.world_mut().query::<&CommandDefinition>();
            query.iter(app.world()).cloned().collect::<Vec<_>>()
        };

        let denied = definitions
            .iter()
            .find(|definition| definition.matches("user_only"))
            .unwrap()
            .agent_invocation(caller, serde_json::json!({}))
            .unwrap_err();
        assert_eq!(
            denied,
            "focus-changing app command is disabled for agents".to_string(),
        );

        let malformed = definitions
            .iter()
            .find(|definition| definition.matches("agent_visible"))
            .unwrap()
            .agent_invocation(caller, serde_json::json!({"unexpected": true}))
            .unwrap_err();
        assert_eq!(
            malformed,
            "agent_visible: invalid arguments: unknown argument unexpected".to_string(),
        );
    }
}
