use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;
use vmux_core::{HostShell, JsonArguments, RegistrationOrder};
use vmux_service::protocol::{AgentCommand, AgentQuery, JsonValue};

use vmux_api::InputSchema;

pub struct ToolRegistryPlugin;

impl Plugin for ToolRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Startup,
            (ToolStartupSet::Registry, ToolStartupSet::Manifest).chain(),
        )
        .add_systems(
            Startup,
            spawn_tool_registry.in_set(ToolStartupSet::Registry),
        )
        .configure_sets(
            Update,
            (
                ToolResolveSet,
                ToolRouteSet,
                ToolRequestSet,
                ToolRequestFlush,
                ToolDispatchSet,
                ToolDispatchFlush,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (resolve_tool_catalogs, resolve_tool_invocations).in_set(ToolResolveSet),
        )
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolRequestFlush),
        )
        .add_systems(Update, dispatch_command_calls.in_set(ToolDispatchSet))
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolDispatchFlush),
        );
    }
}

pub struct ToolManifestPlugin<T> {
    manifest: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> ToolManifestPlugin<T> {
    pub const fn new(manifest: &'static str) -> Self {
        Self {
            manifest,
            marker: PhantomData,
        }
    }
}

impl<T> Plugin for ToolManifestPlugin<T>
where
    T: Component + Clone + serde::de::DeserializeOwned + Serialize,
{
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ToolRegistryPlugin>() {
            app.add_plugins(ToolRegistryPlugin);
        }
        app.insert_resource(ToolManifestSource::<T>::new(self.manifest))
            .add_systems(
                Startup,
                register_tools::<T>.in_set(ToolStartupSet::Manifest),
            )
            .add_systems(Update, route_tools::<T>.in_set(ToolRouteSet));
    }
}

#[derive(Resource)]
struct ToolManifestSource<T> {
    source: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> ToolManifestSource<T> {
    fn new(source: &'static str) -> Self {
        Self {
            source,
            marker: PhantomData,
        }
    }
}

fn spawn_tool_registry(mut commands: Commands) {
    commands.spawn((Name::new("Tool registry"), NextToolOrder::default()));
}

fn register_tools<T>(
    manifest: Res<ToolManifestSource<T>>,
    mut commands: Commands,
    mut next_order: Single<&mut NextToolOrder>,
) where
    T: Component + serde::de::DeserializeOwned + Serialize,
{
    let manifest = ToolManifest::<T>::from_ron(manifest.source);
    for entry in manifest.0 {
        let (seed, kind) = entry.into_seed();
        let order = next_order.0;
        next_order.0 += 1;
        let mut entity = commands.spawn((
            RegisteredTool,
            Name::new(seed.name),
            ToolAliases(seed.aliases),
            ToolDescription(seed.description),
            ToolInputSchema(seed.input_schema),
            ToolAccess(seed.availability),
            RegistrationOrder(order),
            kind,
        ));
        if seed.shell_aware {
            entity.insert(ShellAware);
        }
    }
    commands.remove_resource::<ToolManifestSource<T>>();
}

fn route_tools<T>(
    mut commands: Commands,
    calls: Query<(Entity, &ToolTarget), Added<ToolCall>>,
    tools: Query<&T>,
) where
    T: Component + Clone,
{
    for (request, target) in &calls {
        let Ok(tool) = tools.get(target.0) else {
            continue;
        };
        commands.entity(request).insert(tool.clone());
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolResolveSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
enum ToolStartupSet {
    Registry,
    Manifest,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolRouteSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolRequestSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolRequestFlush;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolDispatchSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolDispatchFlush;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

type ToolEntity<'w> = (
    Entity,
    &'w Name,
    &'w ToolAliases,
    &'w ToolDescription,
    &'w ToolInputSchema,
    &'w ToolAccess,
    &'w RegistrationOrder,
    Option<&'w ShellAware>,
);

#[derive(Component, Clone, Copy)]
pub struct ToolCatalogRequest;

#[derive(Component, Clone, Debug)]
pub struct ToolCatalog(pub Vec<ToolDefinition>);

#[derive(Component, Clone, Copy)]
pub struct ToolInvocation;

#[derive(Component, Clone, Copy)]
pub struct ToolCommandFallback;

#[derive(Component, Clone, Copy)]
pub struct AcpSessionContext;

#[derive(Component, Clone, Copy)]
pub struct AcpTerminalContext;

fn resolve_tool_catalogs(
    requests: Query<
        (
            Entity,
            Option<&HostShell>,
            Has<AcpSessionContext>,
            Has<AcpTerminalContext>,
        ),
        Added<ToolCatalogRequest>,
    >,
    tools: Query<ToolEntity<'static>, With<RegisteredTool>>,
    mut commands: Commands,
) {
    for (request_entity, shell, acp_session, acp_terminals) in &requests {
        let mut definitions = Vec::new();
        for (_, name, _, description, schema, access, order, shell_aware) in &tools {
            if !access.0.allows(acp_session, acp_terminals) {
                continue;
            }
            let mut description = description.0.clone();
            if shell_aware.is_some() {
                description.push_str(&ShellNote::for_shell(
                    shell.map_or("", |shell| shell.0.as_str()),
                ));
            }
            definitions.push((
                order.0,
                ToolDefinition {
                    name: name.as_str().to_string(),
                    description,
                    input_schema: schema.0.to_json(),
                },
            ));
        }
        definitions.sort_by_key(|(order, _)| *order);
        let definitions = definitions
            .into_iter()
            .map(|(_, definition)| definition)
            .collect();
        commands
            .entity(request_entity)
            .insert(ToolCatalog(definitions));
    }
}

fn resolve_tool_invocations(
    invocations: Query<
        (
            Entity,
            &Name,
            &JsonArguments,
            Has<AcpSessionContext>,
            Has<AcpTerminalContext>,
            Has<ToolCommandFallback>,
        ),
        Added<ToolInvocation>,
    >,
    tools: Query<ToolEntity<'static>, With<RegisteredTool>>,
    mut commands: Commands,
) {
    for (request_entity, requested_name, _, acp_session, acp_terminals, command_fallback) in
        &invocations
    {
        let normalized = canonical_tool_name(requested_name.as_str());
        let mut matched = None;
        for (tool_entity, name, aliases, _, _, access, _, _) in &tools {
            if name.as_str() != normalized && !aliases.0.iter().any(|alias| alias == normalized) {
                continue;
            }
            if !access.0.allows(acp_session, acp_terminals) {
                commands
                    .entity(request_entity)
                    .insert(ToolDispatchError::new(format!(
                        "tool {normalized} is unavailable for ACP sessions"
                    )));
                matched = Some(());
                break;
            }
            commands.entity(request_entity).insert((
                Name::new(name.as_str().to_string()),
                ToolCall,
                ToolTarget(tool_entity),
            ));
            matched = Some(());
            break;
        }
        if matched.is_some() {
            continue;
        }
        if command_fallback {
            commands
                .entity(request_entity)
                .insert((Name::new(normalized.to_string()), ToolCall));
        } else {
            commands
                .entity(request_entity)
                .insert(ToolDispatchError::new(format!(
                    "unknown tool: {normalized}"
                )));
        }
    }
}

impl ToolDefinition {
    pub fn merge_commands(
        mut definitions: Vec<Self>,
        commands: Vec<vmux_service::protocol::AgentCommandTool>,
    ) -> Result<Vec<Self>, String> {
        for command in commands {
            let input_schema = Value::try_from(&command.input_schema)
                .map_err(|error| format!("invalid command input schema: {error}"))?;
            definitions.push(Self {
                name: command.name,
                description: command.description,
                input_schema,
            });
        }
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        for pair in definitions.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(format!("duplicate tool name: {}", pair[0].name));
            }
        }
        Ok(definitions)
    }
}

#[derive(Component, Clone, Debug)]
pub struct ToolCommand(pub Result<AgentCommand, String>);

#[derive(Component, Clone, Debug)]
pub struct ToolQuery(pub Result<AgentQuery, String>);

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolAvailability {
    #[default]
    Always,
    OutsideAcpSession,
    WithoutAcpTerminals,
}

impl ToolAvailability {
    pub(crate) fn allows(self, acp_session: bool, acp_terminals: bool) -> bool {
        match self {
            Self::Always => true,
            Self::OutsideAcpSession => !acp_session,
            Self::WithoutAcpTerminals => !acp_terminals,
        }
    }
}

#[derive(Component)]
pub(crate) struct RegisteredTool;

#[derive(Component)]
pub(crate) struct ToolAliases(pub(crate) Vec<String>);

#[derive(Component)]
pub(crate) struct ToolDescription(pub(crate) String);

#[derive(Component)]
pub(crate) struct ToolInputSchema(pub(crate) InputSchema);

#[derive(Component)]
pub(crate) struct ToolAccess(pub(crate) ToolAvailability);

#[derive(Component)]
pub(crate) struct ShellAware;

#[derive(Component, Clone, Debug)]
pub struct ToolDispatchError(pub(crate) String);

impl ToolDispatchError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

#[derive(Component, Default)]
struct NextToolOrder(u32);

#[derive(Clone, Copy, Component)]
pub struct ToolCall;

pub type AddedTool<T> = (With<ToolCall>, Added<T>);

#[derive(Clone, Copy, Component)]
pub struct ToolTarget(pub Entity);

type PendingCommandCalls<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Name, &'static JsonArguments),
    (Added<ToolCall>, Without<ToolTarget>, Without<ToolCommand>),
>;

fn dispatch_command_calls(mut commands: Commands, calls: PendingCommandCalls) {
    for (entity, name, arguments) in &calls {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::InvokeCommand {
                id: name.as_str().to_string(),
                args: JsonValue::from(arguments.0.clone()),
            })));
    }
}

struct ToolSeed {
    name: String,
    aliases: Vec<String>,
    description: String,
    input_schema: InputSchema,
    availability: ToolAvailability,
    shell_aware: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolEntry<K> {
    kind: K,
    #[serde(default)]
    aliases: Vec<String>,
    description: String,
    input_schema: InputSchema,
    #[serde(default)]
    availability: ToolAvailability,
    #[serde(default)]
    shell_aware: bool,
}

impl<K: Component + Serialize> ToolEntry<K> {
    fn into_seed(self) -> (ToolSeed, K) {
        let Value::String(name) =
            serde_json::to_value(&self.kind).expect("tool kind must serialize")
        else {
            panic!("tool kind must serialize as a string")
        };
        (
            ToolSeed {
                name,
                aliases: self.aliases,
                description: self.description,
                input_schema: self.input_schema,
                availability: self.availability,
                shell_aware: self.shell_aware,
            },
            self.kind,
        )
    }
}

struct ToolManifest<K>(Vec<ToolEntry<K>>);

impl<K> ToolManifest<K>
where
    K: Component + serde::de::DeserializeOwned + Serialize,
{
    fn from_ron(source: &str) -> Self {
        let entries: Vec<ToolEntry<K>> =
            ron::from_str(source).expect("embedded tool definitions must be valid RON");
        for entry in &entries {
            entry
                .input_schema
                .validate()
                .expect("embedded tool input schemas must be valid");
        }
        Self(entries)
    }
}

pub fn canonical_tool_name(name: &str) -> &str {
    name.strip_prefix("vmux_").unwrap_or(name)
}

pub struct ShellNote;

impl ShellNote {
    pub fn for_shell(shell: &str) -> String {
        let base = shell
            .rsplit('/')
            .next()
            .unwrap_or(shell)
            .trim()
            .to_ascii_lowercase();
        if base.is_empty() {
            return String::new();
        }
        let differences = match base.as_str() {
            "nu" | "nushell" => concat!(
                " Write nushell, not POSIX: redirect both streams with `out+err>` (`2>&1` is a parse error),",
                " substitute with `(cmd)` not `$(cmd)`, set variables with `$env.NAME = \"value\"` not `export`,",
                " and use `| ignore` rather than `> /dev/null` to discard output."
            ),
            "fish" => concat!(
                " Write fish, not POSIX: redirect both streams with `&>`, set variables with `set -x NAME value`",
                " not `export`, and note that `&&` and `||` are `; and` and `; or`."
            ),
            _ => "",
        };
        if differences.is_empty() {
            return format!(" The shell is {base}.");
        }
        format!(
            " The shell is {base}.{differences} To run a POSIX script instead, invoke `bash -c \"...\"` as the command."
        )
    }
}
