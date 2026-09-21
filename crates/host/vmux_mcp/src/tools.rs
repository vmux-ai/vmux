mod bookmark;
mod files;
mod knowledge;
mod param;
mod state;
mod visual;
mod workspace;

use bevy_app::{App, Plugin};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vmux_client::protocol::{AgentCommand, AgentQuery, ProcessId};

pub use param::McpParamTool;

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextToolOrder>();
        GeneratedTools::register(app);
        app.add_plugins((
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
        app
    }
}

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

#[derive(EntityEvent)]
pub(super) struct ToolCall {
    pub(crate) entity: Entity,
    pub(crate) request: Entity,
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

    fn finish(&self, commands: &mut Commands, result: Result<ToolExecution, String>) {
        commands.entity(self.request).insert(ToolOutcome(result));
    }

    fn dispatch(
        world: &mut World,
        name: &str,
        arguments: Value,
        anchor: Option<ProcessId>,
        host_shell: &str,
        acp_session: bool,
        acp_terminals: bool,
    ) -> Result<ToolExecution, String> {
        let normalized = canonical_tool_name(name);
        let Some((tool, registered_name, availability)) = Self::find(world, normalized) else {
            return Err(format!("unknown tool: {normalized}"));
        };
        if !availability.allows(acp_session, acp_terminals) {
            return Err(format!("tool {normalized} is unavailable for ACP sessions"));
        }
        let request = world.spawn_empty().id();
        world.trigger(Self {
            entity: tool,
            request,
            name: registered_name,
            arguments,
            anchor,
            host_shell: host_shell.to_string(),
        });
        world.flush();
        let outcome = world
            .entity_mut(request)
            .take::<ToolOutcome>()
            .ok_or_else(|| format!("tool {normalized} did not produce an execution"))?;
        world.despawn(request);
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

type ToolDispatch = fn(&ToolCall) -> Result<DispatchTarget, String>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolSeed {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    description: String,
    input_schema: Value,
    #[serde(default)]
    availability: ToolAvailability,
    #[serde(default)]
    shell_aware: bool,
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

    pub(super) fn local(self, app: &mut App, dispatch: ToolDispatch) {
        let entity = self.spawn(app);
        app.world_mut().entity_mut(entity).observe(
            move |trigger: On<ToolCall>, mut commands: Commands| {
                let result = dispatch(&trigger).map(|target| ToolExecution::Dispatch {
                    target,
                    name: trigger.name.clone(),
                    arguments: trigger.arguments.clone(),
                    anchor: trigger.anchor,
                });
                trigger.finish(&mut commands, result);
            },
        );
    }

    pub(super) fn protocol(self, app: &mut App, tool: ProtocolTool) {
        let entity = self.spawn(app);
        app.world_mut().entity_mut(entity).observe(
            move |trigger: On<ToolCall>, mut commands: Commands| {
                let execution = ToolExecution::Protocol {
                    tool,
                    arguments: trigger.arguments.clone(),
                    anchor: trigger.anchor,
                };
                trigger.finish(&mut commands, Ok(execution));
            },
        );
    }

    fn spawn(self, app: &mut App) -> Entity {
        let order = {
            let mut next = app.world_mut().resource_mut::<NextToolOrder>();
            let order = next.0;
            next.0 += 1;
            order
        };
        let mut entity = app.world_mut().spawn((
            McpTool,
            Name::new(self.name),
            ToolAliases(self.aliases),
            ToolDescription(self.description),
            ToolInputSchema(self.input_schema),
            ToolAccess(self.availability),
            ToolOrder(order),
        ));
        if self.shell_aware {
            entity.insert(ShellAware);
        }
        entity.id()
    }
}

pub(super) struct ToolManifest(Vec<ToolSeed>);

impl ToolManifest {
    pub(super) fn from_ron(source: &str) -> Self {
        Self(ron::from_str(source).expect("embedded MCP tool definitions must be valid RON"))
    }

    pub(super) fn local(&mut self, app: &mut App, name: &str, dispatch: ToolDispatch) {
        self.take(name).local(app, dispatch);
    }

    pub(super) fn protocol(&mut self, app: &mut App, name: &str, tool: ProtocolTool) {
        self.take(name).protocol(app, tool);
    }

    pub(super) fn finish(self) {
        assert!(
            self.0.is_empty(),
            "MCP tool definitions without handlers: {}",
            self.0
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn take(&mut self, name: &str) -> ToolSeed {
        let index = self
            .0
            .iter()
            .position(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("missing MCP tool definition: {name}"));
        self.0.remove(index)
    }
}

struct GeneratedTools;

impl GeneratedTools {
    fn register(app: &mut App) {
        for (name, description, schema) in vmux_command_mcp::tool_entries() {
            ToolSeed::new(name, description, schema).local(app, Self::dispatch_command);
        }
        for (name, description, schema) in McpParamTool::mcp_tool_entries() {
            ToolSeed::new(name, description, schema).local(app, Self::dispatch_param);
        }
    }

    fn dispatch_command(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    fn dispatch_param(call: &ToolCall) -> Result<DispatchTarget, String> {
        let parsed = McpParamTool::from_mcp_call(&call.name, call.arguments.clone())
            .ok_or_else(|| format!("unknown tool: {}", call.name))?;
        parsed
            .and_then(McpParamTool::to_agent_command)
            .map(DispatchTarget::Command)
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
    match ToolCall::dispatch(
        app.world_mut(),
        name,
        arguments,
        anchor,
        host_shell,
        false,
        false,
    )? {
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
