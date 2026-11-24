use std::collections::HashSet;

use bevy::prelude::*;
use bevy::prelude::{Added, Commands};
use vmux_api::JsonSchema;
use vmux_ecs::JsonArguments;
use vmux_feature::manifest::{self, FeatureId, FeatureManifest};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WriteCommandRequests;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadCommandRequests;

pub struct CommandRuntimePlugin;

impl Plugin for CommandRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandInvocation>()
            .add_systems(PreStartup, validate_bindings)
            .configure_sets(
                Startup,
                (RegisterManifests, FlushManifests, BindCommands).chain(),
            )
            .add_systems(
                Startup,
                (
                    spawn.before(BindCommands),
                    register.in_set(RegisterManifests),
                    ApplyDeferred.in_set(FlushManifests),
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
            .add_systems(Startup, (bind, bind_message).in_set(BindCommands))
            .add_systems(
                Update,
                (validate, dispatch, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(DispatchCommandInvocations),
            );
    }
}

fn validate_bindings() {
    for binding in inventory::iter::<CommandBindingRegistration> {
        binding
            .declaration
            .validate()
            .expect("valid command declaration binding");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandShortcut {
    pub chord: bool,
    pub key: String,
    pub when: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMcp {
    pub description: String,
    pub input_schema: JsonSchema,
    pub(super) allow_agent: bool,
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
    #[allow(unknown_lints, vmux_ecs_ownership)]
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
#[allow(unknown_lints, vmux_ecs_ownership)]
pub(super) struct CommandMessage(pub(super) fn(&CommandInvocation, &mut Commands));

pub trait CommandBinding: Bundle + Sized {
    fn for_command(id: &str) -> Option<Self>;
}

#[derive(Component, Clone, Copy)]
pub struct CommandBindingRegistration {
    declaration: vmux_feature::CommandBindingRegistration,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    attach: fn(Entity, &CommandDefinition, &mut Commands),
}

impl CommandBindingRegistration {
    pub const fn of<T: CommandBinding, M: manifest::FeatureManifestSource>(
        selector: Option<&'static str>,
    ) -> Self {
        Self {
            declaration: vmux_feature::CommandBindingRegistration::of::<M>(selector),
            attach: attach::<T>,
        }
    }
}

fn attach<T: CommandBinding>(
    entity: Entity,
    definition: &CommandDefinition,
    commands: &mut Commands,
) {
    if let Some(bundle) = T::for_command(&definition.id) {
        commands.entity(entity).insert(bundle);
    }
}

inventory::collect!(CommandBindingRegistration);

#[derive(Component, Clone, Copy)]
pub struct CommandMessageRegistration {
    #[allow(unknown_lints, vmux_ecs_ownership)]
    owner: fn() -> FeatureId,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    message: fn(&CommandDefinition) -> Option<CommandMessage>,
}

impl CommandMessageRegistration {
    pub const fn of<T, M: manifest::FeatureManifestSource>() -> Self
    where
        T: Message + for<'a> TryFrom<&'a CommandInvocation>,
    {
        Self {
            owner: FeatureId::of::<M>,
            message: Self::message::<T>,
        }
    }

    fn message<T>(definition: &CommandDefinition) -> Option<CommandMessage>
    where
        T: Message + for<'a> TryFrom<&'a CommandInvocation>,
    {
        let invocation = CommandInvocation::new(Entity::PLACEHOLDER, &definition.id);
        if T::try_from(&invocation).is_err() {
            return None;
        }
        Some(CommandMessage::of::<T>())
    }
}

inventory::collect!(CommandMessageRegistration);

fn bind(
    definitions: Query<(Entity, &CommandDefinition, &ChildOf)>,
    owners: Query<&FeatureId>,
    bindings: Query<&CommandBindingRegistration>,
    mut commands: Commands,
) {
    for (entity, definition, parent) in &definitions {
        let Ok(owner) = owners.get(parent.parent()) else {
            continue;
        };
        for binding in &bindings {
            if binding.declaration.owner() == *owner {
                (binding.attach)(entity, definition, &mut commands);
            }
        }
    }
}

fn bind_message(
    definitions: Query<(Entity, &CommandDefinition, &ChildOf)>,
    owners: Query<&FeatureId>,
    registrations: Query<&CommandMessageRegistration>,
    mut commands: Commands,
) {
    for (entity, definition, parent) in &definitions {
        let Ok(owner) = owners.get(parent.parent()) else {
            continue;
        };
        for registration in &registrations {
            if (registration.owner)() != *owner {
                continue;
            }
            if let Some(message) = (registration.message)(definition) {
                commands.entity(entity).insert(message);
            }
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchCommandInvocations;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindCommands;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct RegisterManifests;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct FlushManifests;

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command keymap"),
        crate::host::shortcut_driver::Keymap::default(),
    ));
}

fn register(manifests: Query<(Entity, &FeatureManifest)>, mut commands: Commands) {
    for binding in inventory::iter::<CommandBindingRegistration> {
        commands.spawn(*binding);
    }
    for message in inventory::iter::<CommandMessageRegistration> {
        commands.spawn(*message);
    }
    for (owner, manifest) in &manifests {
        for definition in CommandDefinition::from_manifest(manifest) {
            commands.spawn((definition, ChildOf(owner)));
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
    use vmux_feature::manifest::FeatureManifestOwner;

    struct Owner;
    struct Foreign;

    impl manifest::FeatureManifestSource for Owner {
        const SOURCE: &'static str =
            r#"(commands: [(id: "ownership.owner", label: "Owner", group: "Test")])"#;
    }

    impl manifest::FeatureManifestSource for Foreign {
        const SOURCE: &'static str =
            r#"(commands: [(id: "ownership.foreign", label: "Foreign", group: "Test")])"#;
    }

    #[derive(Component)]
    struct BroadBinding;

    impl CommandBinding for BroadBinding {
        fn for_command(_id: &str) -> Option<Self> {
            Some(Self)
        }
    }

    #[derive(Message)]
    struct BroadMessage;

    impl TryFrom<&CommandInvocation> for BroadMessage {
        type Error = ();

        fn try_from(_invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Ok(Self)
        }
    }

    inventory::submit! {
        CommandBindingRegistration::of::<BroadBinding, Owner>(None)
    }

    inventory::submit! {
        CommandMessageRegistration::of::<BroadMessage, Owner>()
    }

    #[test]
    fn registered_commands_bind_only_inside_their_source_lifetime() {
        let mut app = CommandTestApp::empty();
        app.add_plugins((
            manifest::FeaturePlugin::<Owner>::default(),
            manifest::FeaturePlugin::<Foreign>::default(),
        ))
        .add_message::<BroadMessage>();
        app.finish();
        app.update();
        let mut definitions = app.world_mut().query::<(Entity, &CommandDefinition)>();
        let mut owner = None;
        let mut foreign = None;
        for (entity, definition) in definitions.iter(app.world()) {
            match definition.id.as_str() {
                "ownership.owner" => owner = Some(entity),
                "ownership.foreign" => foreign = Some(entity),
                _ => {}
            }
        }
        let owner = owner.expect("owned command exists");
        let foreign = foreign.expect("foreign command exists");
        let owner_bound = app.world().get::<BroadBinding>(owner).is_some();
        let foreign_bound = app.world().get::<BroadBinding>(foreign).is_some();
        assert!(owner_bound);
        assert!(app.world().get::<CommandMessage>(owner).is_some());
        assert!(!foreign_bound);
        assert!(app.world().get::<CommandMessage>(foreign).is_none());
        app.world_mut()
            .write_message(CommandInvocation::new(owner, "ownership.owner"));
        app.world_mut()
            .write_message(CommandInvocation::new(foreign, "ownership.foreign"));
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<BroadMessage>>()
                .drain()
                .count(),
            1
        );
        let source = app
            .world_mut()
            .query_filtered::<Entity, With<FeatureManifestOwner<Owner>>>()
            .single(app.world())
            .unwrap();
        app.world_mut().despawn(source);
        app.update();
        assert!(app.world().get::<CommandDefinition>(owner).is_none());
        assert!(app.world().get::<CommandDefinition>(foreign).is_some());
    }

    #[derive(Message, Debug, PartialEq, Eq)]
    struct TestCommandMessage(String);

    impl TryFrom<&CommandInvocation> for TestCommandMessage {
        type Error = ();

        fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
            Ok(Self(invocation.id.clone()))
        }
    }

    struct CommandTestApp;

    impl CommandTestApp {
        fn empty() -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CommandRuntimePlugin));
            app
        }

        fn register(app: &mut App, definition: CommandDefinition) {
            app.add_message::<TestCommandMessage>();
            app.world_mut()
                .spawn(definition.message::<TestCommandMessage>());
        }

        fn test_toggle() -> App {
            let mut app = Self::empty();
            Self::register(
                &mut app,
                CommandDefinition::new("test_toggle", "Toggle", "Test").direct("Super+t"),
            );
            app
        }

        fn agent_commands() -> App {
            let mut app = Self::empty();
            Self::register(
                &mut app,
                CommandDefinition::new("agent_visible", "Visible", "Agent")
                    .mcp(CommandMcp::new("Visible", vmux_api::JsonSchema::object()).allow_agent()),
            );
            Self::register(
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

        let requests = app
            .world_mut()
            .resource_mut::<Messages<TestCommandMessage>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests, [TestCommandMessage("test_toggle".to_string())]);
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
        CommandTestApp::register(
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
            .resource_mut::<Messages<TestCommandMessage>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests, [TestCommandMessage("canonical".to_string())]);
    }

    #[test]
    #[should_panic(expected = "duplicate command id: duplicate")]
    fn duplicate_command_ids_are_rejected() {
        let mut app = CommandTestApp::empty();
        CommandTestApp::register(
            &mut app,
            CommandDefinition::new("duplicate", "First", "Test"),
        );
        CommandTestApp::register(
            &mut app,
            CommandDefinition::new("duplicate", "Second", "Test"),
        );
        app.update();
    }

    #[test]
    #[should_panic(expected = "duplicate command id: duplicate")]
    fn aliases_cannot_collide_with_command_ids() {
        let mut app = CommandTestApp::empty();
        CommandTestApp::register(
            &mut app,
            CommandDefinition::new("duplicate", "First", "Test"),
        );
        CommandTestApp::register(
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
        let requests = app
            .world_mut()
            .resource_mut::<Messages<TestCommandMessage>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests, [TestCommandMessage("agent_visible".to_string())]);
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
