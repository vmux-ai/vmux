mod bookmark;
mod files;
mod knowledge;
mod param;
mod state;
mod visual;
mod workspace;

use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vmux_client::protocol::{AgentCommand, AgentQuery, ProcessId};

pub use param::McpParamTool;

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextToolOrder>()
            .configure_sets(
                Startup,
                (
                    ToolRegistrationSet::Generated,
                    ToolRegistrationSet::State,
                    ToolRegistrationSet::Workspace,
                    ToolRegistrationSet::Files,
                    ToolRegistrationSet::Knowledge,
                    ToolRegistrationSet::Visual,
                    ToolRegistrationSet::Bookmark,
                )
                    .chain(),
            )
            .add_systems(
                Startup,
                GeneratedTools::register.in_set(ToolRegistrationSet::Generated),
            )
            .add_systems(
                Update,
                (
                    GeneratedTools::dispatch_command,
                    GeneratedTools::dispatch_param,
                )
                    .in_set(ToolDispatchSet),
            )
            .add_plugins((
                state::StateToolsPlugin,
                workspace::WorkspaceToolsPlugin,
                files::FileToolsPlugin,
                knowledge::KnowledgeToolsPlugin,
                visual::VisualToolsPlugin,
                bookmark::BookmarkToolsPlugin,
            ));
    }
}

impl ToolsPlugin {
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(Self);
        app.update();
        app
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub(super) enum ToolRegistrationSet {
    Generated,
    State,
    Workspace,
    Files,
    Knowledge,
    Visual,
    Bookmark,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub(crate) struct ToolDispatchSet;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolDefinition {
    fn all(world: &mut World, acp_session: bool, acp_terminals: bool, shell: &str) -> Vec<Self> {
        let mut query = world.query_filtered::<(
            &Name,
            &ToolDescription,
            &ToolInputSchema,
            &ToolAccess,
            &ToolOrder,
            Option<&ShellAware>,
        ), With<McpTool>>();
        let mut definitions = Vec::new();
        for (name, description, schema, access, order, shell_aware) in query.iter(world) {
            if !access.0.allows(acp_session, acp_terminals) {
                continue;
            }
            let mut description = description.0.clone();
            if shell_aware.is_some() {
                description.push_str(&ShellNote::for_shell(shell));
            }
            definitions.push((
                order.0,
                Self {
                    name: name.as_str().to_string(),
                    description,
                    input_schema: schema.0.clone(),
                },
            ));
        }
        definitions.sort_by_key(|(order, _)| *order);
        definitions
            .into_iter()
            .map(|(_, definition)| definition)
            .collect()
    }
}

#[derive(Debug)]
pub enum DispatchTarget {
    Command(AgentCommand),
    Query(AgentQuery),
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtocolTool {
    ReadFile,
    Grep,
    VaultStatus,
}

pub(crate) enum ToolExecution {
    Dispatch {
        target: DispatchTarget,
        name: String,
        arguments: Value,
        anchor: Option<ProcessId>,
    },
    Protocol {
        tool: ProtocolTool,
        arguments: Value,
        anchor: Option<ProcessId>,
    },
}

#[derive(Component)]
pub(crate) struct McpTool;

#[derive(Component)]
pub(crate) struct ToolAliases(pub(crate) Vec<String>);

#[derive(Component)]
pub(crate) struct ToolDescription(pub(crate) String);

#[derive(Component)]
pub(crate) struct ToolInputSchema(pub(crate) Value);

#[derive(Component)]
pub(crate) struct ToolAccess(pub(crate) ToolAvailability);

#[derive(Component)]
pub(crate) struct ToolOrder(pub(crate) u32);

#[derive(Component)]
pub(crate) struct ShellAware;

#[derive(Component)]
pub(crate) struct ToolOutcome(pub(crate) Result<ToolExecution, String>);

#[derive(Resource, Default)]
struct NextToolOrder(u32);

#[derive(Component)]
pub(super) struct ToolCall {
    pub(crate) tool: Entity,
    pub(crate) name: String,
    pub(crate) arguments: Value,
    pub(crate) anchor: Option<ProcessId>,
    pub(crate) host_shell: String,
}

impl ToolCall {
    fn parse<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        serde_json::from_value(self.arguments.clone())
            .map_err(|error| format!("{name}: invalid arguments: {error}"))
    }

    fn require_anchor(&self, name: &str) -> Result<ProcessId, String> {
        self.anchor.ok_or_else(|| {
            format!("{name} requires an agent anchor (not available to this client)")
        })
    }

    fn finish(
        &self,
        request: Entity,
        commands: &mut Commands,
        result: Result<ToolExecution, String>,
    ) {
        commands
            .entity(request)
            .remove::<Self>()
            .insert(ToolOutcome(result));
    }

    pub(super) fn finish_dispatch(
        &self,
        request: Entity,
        commands: &mut Commands,
        result: Result<DispatchTarget, String>,
    ) {
        let result = result.map(|target| ToolExecution::Dispatch {
            target,
            name: self.name.clone(),
            arguments: self.arguments.clone(),
            anchor: self.anchor,
        });
        self.finish(request, commands, result);
    }

    fn dispatch(
        app: &mut App,
        name: &str,
        arguments: Value,
        anchor: Option<ProcessId>,
        host_shell: &str,
        acp_session: bool,
        acp_terminals: bool,
    ) -> Result<ToolExecution, String> {
        let normalized = canonical_tool_name(name);
        let Some((tool, registered_name, availability)) = Self::find(app.world_mut(), normalized)
        else {
            return Err(format!("unknown tool: {normalized}"));
        };
        if !availability.allows(acp_session, acp_terminals) {
            return Err(format!("tool {normalized} is unavailable for ACP sessions"));
        }
        let request = app
            .world_mut()
            .spawn(Self {
                tool,
                name: registered_name,
                arguments,
                anchor,
                host_shell: host_shell.to_string(),
            })
            .id();
        app.update();
        let outcome = app
            .world_mut()
            .entity_mut(request)
            .take::<ToolOutcome>()
            .ok_or_else(|| format!("tool {normalized} did not produce an execution"))?;
        app.world_mut().despawn(request);
        outcome.0
    }

    fn find(world: &mut World, name: &str) -> Option<(Entity, String, ToolAvailability)> {
        let mut query =
            world.query_filtered::<(Entity, &Name, &ToolAliases, &ToolAccess), With<McpTool>>();
        for (entity, registered_name, aliases, access) in query.iter(world) {
            if registered_name.as_str() == name || aliases.0.iter().any(|alias| alias == name) {
                return Some((entity, registered_name.as_str().to_string(), access.0));
            }
        }
        None
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub(super) struct ToolCalls<'w, 's, T: Component> {
    calls: Query<'w, 's, (Entity, &'static ToolCall), Added<ToolCall>>,
    tools: Query<'w, 's, &'static T>,
}

impl<'w, 's, T: Component> ToolCalls<'w, 's, T> {
    pub(super) fn iter(&self) -> impl Iterator<Item = (Entity, &ToolCall, &T)> {
        self.calls.iter().filter_map(|(request, call)| {
            self.tools
                .get(call.tool)
                .ok()
                .map(|tool| (request, call, tool))
        })
    }

    pub(super) fn matching(&self, kind: T) -> impl Iterator<Item = (Entity, &ToolCall, &T)>
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
    input_schema: Value,
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
    input_schema: Value,
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

impl ToolSeed {
    pub(super) fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            description: description.into(),
            input_schema,
            availability: ToolAvailability::Always,
            shell_aware: false,
        }
    }
}

pub(super) struct ToolManifest<K>(Vec<ToolEntry<K>>);

impl<K> ToolManifest<K>
where
    K: Component + serde::de::DeserializeOwned + Serialize,
{
    pub(super) fn from_ron(source: &str) -> Self {
        Self(ron::from_str(source).expect("embedded MCP tool definitions must be valid RON"))
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub(super) struct ToolSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    next_order: ResMut<'w, NextToolOrder>,
}

impl ToolSpawner<'_, '_> {
    pub(super) fn spawn_manifest<K>(&mut self, manifest: ToolManifest<K>)
    where
        K: Component + Serialize,
    {
        for entry in manifest.0 {
            let (seed, kind) = entry.into_seed();
            self.spawn(seed, kind);
        }
    }

    fn spawn<T: Component>(&mut self, seed: ToolSeed, marker: T) {
        let order = self.next_order.0;
        self.next_order.0 += 1;
        let mut entity = self.commands.spawn((
            McpTool,
            Name::new(seed.name),
            ToolAliases(seed.aliases),
            ToolDescription(seed.description),
            ToolInputSchema(seed.input_schema),
            ToolAccess(seed.availability),
            ToolOrder(order),
            marker,
        ));
        if seed.shell_aware {
            entity.insert(ShellAware);
        }
    }
}

struct GeneratedTools;

#[derive(Component)]
struct GeneratedCommandTool;

#[derive(Component)]
struct ParamTool;

impl GeneratedTools {
    fn register(mut tools: ToolSpawner) {
        for (name, description, schema) in vmux_command_mcp::tool_entries() {
            tools.spawn(
                ToolSeed::new(name, description, schema),
                GeneratedCommandTool,
            );
        }
        for (name, description, schema) in McpParamTool::mcp_tool_entries() {
            tools.spawn(ToolSeed::new(name, description, schema), ParamTool);
        }
    }

    fn dispatch_command(mut commands: Commands, calls: ToolCalls<GeneratedCommandTool>) {
        fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
            if vmux_command_mcp::accepts_id(&call.name) {
                return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
                    id: call.name.clone(),
                    args_json: String::new(),
                }));
            }
            if vmux_command_mcp::accepts_call(&call.name, call.arguments.clone()) {
                let args_json = serde_json::to_string(&call.arguments).unwrap_or_default();
                return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
                    id: call.name.clone(),
                    args_json,
                }));
            }
            Err(format!("unknown tool: {}", call.name))
        }

        for (request, call, _) in calls.iter() {
            call.finish_dispatch(request, &mut commands, target(call));
        }
    }

    fn dispatch_param(mut commands: Commands, calls: ToolCalls<ParamTool>) {
        fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
            let parsed = McpParamTool::from_mcp_call(&call.name, call.arguments.clone())
                .ok_or_else(|| format!("unknown tool: {}", call.name))?;
            parsed
                .and_then(McpParamTool::to_agent_command)
                .map(DispatchTarget::Command)
        }

        for (request, call, _) in calls.iter() {
            call.finish_dispatch(request, &mut commands, target(call));
        }
    }
}

pub(crate) fn canonical_tool_name(name: &str) -> &str {
    name.strip_prefix("vmux_").unwrap_or(name)
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    tool_definitions_filtered(false, false, "")
}

pub fn tool_definitions_filtered(
    acp_session: bool,
    acp_terminals: bool,
    shell: &str,
) -> Vec<ToolDefinition> {
    let mut app = ToolsPlugin::app();
    ToolDefinition::all(app.world_mut(), acp_session, acp_terminals, shell)
}

pub fn dispatch_from_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    dispatch_with_anchor(name, arguments, None)
}

pub fn dispatch_with_anchor(
    name: &str,
    arguments: Value,
    anchor: Option<ProcessId>,
) -> Result<DispatchTarget, String> {
    dispatch_in_shell(name, arguments, anchor, "")
}

pub fn dispatch_in_shell(
    name: &str,
    arguments: Value,
    anchor: Option<ProcessId>,
    host_shell: &str,
) -> Result<DispatchTarget, String> {
    let mut app = ToolsPlugin::app();
    match ToolCall::dispatch(&mut app, name, arguments, anchor, host_shell, false, false)? {
        ToolExecution::Dispatch { target, .. } => Ok(target),
        ToolExecution::Protocol { .. } => Err(format!(
            "tool {} requires MCP protocol context",
            canonical_tool_name(name)
        )),
    }
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

#[cfg(test)]
mod tests;
