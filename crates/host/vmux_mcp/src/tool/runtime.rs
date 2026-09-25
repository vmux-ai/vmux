use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue, ProcessId};

use vmux_api::InputSchema;

pub struct ToolRuntimePlugin;

impl Plugin for ToolRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .spawn((Name::new("MCP tool registry"), NextToolOrder::default()));
        app.configure_sets(
            Update,
            (
                ToolRequestSet,
                ToolRequestFlush,
                ToolDispatchSet,
                ToolDispatchFlush,
            )
                .chain(),
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

pub struct McpToolPlugin<T> {
    manifest: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> McpToolPlugin<T> {
    pub const fn new(manifest: &'static str) -> Self {
        Self {
            manifest,
            marker: PhantomData,
        }
    }
}

impl<T> Plugin for McpToolPlugin<T>
where
    T: Component + Clone + serde::de::DeserializeOwned + Serialize,
{
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ToolRuntimePlugin>() {
            app.add_plugins(ToolRuntimePlugin);
        }
        app.world_mut()
            .spawn(McpToolManifest::<T>::new(self.manifest));
        app.add_systems(Startup, register_mcp_tools::<T>.in_set(RegisterTools))
            .add_systems(Update, route_mcp_tools::<T>.in_set(ToolRequestSet));
    }
}

#[derive(Component)]
struct McpToolManifest<T> {
    source: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> McpToolManifest<T> {
    fn new(source: &'static str) -> Self {
        Self {
            source,
            marker: PhantomData,
        }
    }
}

fn register_mcp_tools<T>(
    manifests: Query<(Entity, &McpToolManifest<T>)>,
    mut commands: Commands,
    mut next_order: Single<&mut NextToolOrder>,
) where
    T: Component + serde::de::DeserializeOwned + Serialize,
{
    for (manifest_entity, manifest) in &manifests {
        let manifest = ToolManifest::<T>::from_ron(manifest.source);
        for entry in manifest.0 {
            let (seed, kind) = entry.into_seed();
            let order = next_order.0;
            next_order.0 += 1;
            let mut entity = commands.spawn((
                McpTool,
                Name::new(seed.name),
                ToolAliases(seed.aliases),
                ToolDescription(seed.description),
                ToolInputSchema(seed.input_schema),
                ToolAccess(seed.availability),
                ToolOrder(order),
                kind,
            ));
            if seed.shell_aware {
                entity.insert(ShellAware);
            }
        }
        commands.entity(manifest_entity).despawn();
    }
}

fn route_mcp_tools<T>(mut commands: Commands, calls: ToolCalls<T>)
where
    T: Component + Clone,
{
    for (request, _, tool) in calls.iter() {
        commands.entity(request).insert(tool.clone());
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolRequestSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolRequestFlush;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct RegisterTools;

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
    &'w ToolOrder,
    Option<&'w ShellAware>,
);

#[derive(bevy_ecs::system::SystemParam)]
pub struct ToolRegistry<'w, 's> {
    tools: Query<'w, 's, ToolEntity<'static>, With<McpTool>>,
}

#[derive(Clone, Copy)]
pub struct ToolCallPolicy {
    acp_session: bool,
    acp_terminals: bool,
    allow_command: bool,
}

impl ToolCallPolicy {
    pub const fn mcp(acp_session: bool, acp_terminals: bool) -> Self {
        Self {
            acp_session,
            acp_terminals,
            allow_command: true,
        }
    }

    pub const fn agent() -> Self {
        Self {
            acp_session: false,
            acp_terminals: false,
            allow_command: true,
        }
    }

    pub const fn registered(acp_session: bool, acp_terminals: bool) -> Self {
        Self {
            acp_session,
            acp_terminals,
            allow_command: false,
        }
    }
}

impl ToolRegistry<'_, '_> {
    pub fn definitions(
        &self,
        acp_session: bool,
        acp_terminals: bool,
        shell: &str,
    ) -> Vec<ToolDefinition> {
        let mut definitions = Vec::new();
        for (_, name, _, description, schema, access, order, shell_aware) in &self.tools {
            if !access.0.allows(acp_session, acp_terminals) {
                continue;
            }
            let mut description = description.0.clone();
            if shell_aware.is_some() {
                description.push_str(&ShellNote::for_shell(shell));
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
        definitions
            .into_iter()
            .map(|(_, definition)| definition)
            .collect()
    }

    pub fn call(
        &self,
        name: &str,
        arguments: Value,
        anchor: Option<ProcessId>,
        host_shell: &str,
        policy: ToolCallPolicy,
    ) -> Result<ToolCall, String> {
        let normalized = canonical_tool_name(name);
        for (entity, name, aliases, _, _, access, _, _) in &self.tools {
            if name.as_str() != normalized && !aliases.0.iter().any(|alias| alias == normalized) {
                continue;
            }
            if !access.0.allows(policy.acp_session, policy.acp_terminals) {
                return Err(format!("tool {normalized} is unavailable for ACP sessions"));
            }
            return Ok(ToolCall {
                tool: Some(entity),
                name: name.as_str().to_string(),
                arguments,
                anchor,
                host_shell: host_shell.to_string(),
            });
        }
        if policy.allow_command {
            return Ok(ToolCall {
                tool: None,
                name: normalized.to_string(),
                arguments,
                anchor,
                host_shell: host_shell.to_string(),
            });
        }
        Err(format!("unknown tool: {normalized}"))
    }
}

impl ToolDefinition {
    pub fn merge_commands(
        mut definitions: Vec<Self>,
        commands: Vec<vmux_client::protocol::AgentCommandTool>,
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
pub(crate) struct McpTool;

#[derive(Component)]
pub(crate) struct ToolAliases(pub(crate) Vec<String>);

#[derive(Component)]
pub(crate) struct ToolDescription(pub(crate) String);

#[derive(Component)]
pub(crate) struct ToolInputSchema(pub(crate) InputSchema);

#[derive(Component)]
pub(crate) struct ToolAccess(pub(crate) ToolAvailability);

#[derive(Component)]
pub(crate) struct ToolOrder(pub(crate) u32);

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
pub(super) struct NextToolOrder(u32);

#[derive(Clone, Component)]
pub struct ToolCall {
    tool: Option<Entity>,
    pub(crate) name: String,
    pub(crate) arguments: Value,
    pub(crate) anchor: Option<ProcessId>,
    pub(crate) host_shell: String,
}

impl ToolCall {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn anchor(&self) -> Option<ProcessId> {
        self.anchor
    }

    pub fn host_shell(&self) -> &str {
        &self.host_shell
    }

    pub fn arguments(&self) -> &Value {
        &self.arguments
    }

    pub fn tool(&self) -> Option<Entity> {
        self.tool
    }

    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(self.arguments.clone())
            .map_err(|error| format!("{}: invalid arguments: {error}", self.name))
    }

    pub fn require_anchor(&self) -> Result<ProcessId, String> {
        self.anchor.ok_or_else(|| {
            format!(
                "{} requires an agent anchor (not available to this client)",
                self.name
            )
        })
    }
}

type PendingCommandCalls<'w, 's> =
    Query<'w, 's, (Entity, &'static ToolCall), (Added<ToolCall>, Without<ToolCommand>)>;

fn dispatch_command_calls(mut commands: Commands, calls: PendingCommandCalls) {
    for (entity, call) in &calls {
        if call.tool.is_some() {
            continue;
        }
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::InvokeCommand {
                id: call.name.clone(),
                args: JsonValue::from(call.arguments.clone()),
            })));
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub struct ToolCalls<'w, 's, T: Component> {
    calls: Query<'w, 's, (Entity, &'static ToolCall), Added<ToolCall>>,
    tools: Query<'w, 's, &'static T>,
}

impl<'w, 's, T: Component> ToolCalls<'w, 's, T> {
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &ToolCall, &T)> {
        self.calls.iter().filter_map(|(request, call)| {
            let tool = call.tool?;
            self.tools.get(tool).ok().map(|tool| (request, call, tool))
        })
    }

    pub fn matching(&self, kind: T) -> impl Iterator<Item = (Entity, &ToolCall, &T)>
    where
        T: Copy + PartialEq,
    {
        self.iter().filter(move |(_, _, tool)| **tool == kind)
    }
}

pub(super) struct ToolSeed {
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
            serde_json::to_value(&self.kind).expect("MCP tool kind must serialize")
        else {
            panic!("MCP tool kind must serialize as a string")
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

pub(super) struct ToolManifest<K>(Vec<ToolEntry<K>>);

impl<K> ToolManifest<K>
where
    K: Component + serde::de::DeserializeOwned + Serialize,
{
    pub(super) fn from_ron(source: &str) -> Self {
        let entries: Vec<ToolEntry<K>> =
            ron::from_str(source).expect("embedded MCP tool definitions must be valid RON");
        for entry in &entries {
            entry
                .input_schema
                .validate()
                .expect("embedded MCP tool input schemas must be valid");
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
