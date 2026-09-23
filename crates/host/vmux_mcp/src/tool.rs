mod application;
mod bookmark;
mod browser;
mod files;
mod knowledge;
mod layout;
mod setting;
mod space;
mod terminal;
mod visual;
mod workspace;

use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
#[cfg(test)]
use bevy_ecs::system::RunSystemOnce;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue, ProcessId};

use vmux_api::InputSchema;

pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextToolOrder>()
            .configure_sets(
                Update,
                (
                    ToolRequestSet,
                    ToolRequestFlush,
                    ToolDispatchSet,
                    ToolDispatchFlush,
                )
                    .chain(),
            )
            .configure_sets(
                Startup,
                (
                    ToolRegistrationSet::Application,
                    ToolRegistrationSet::Browser,
                    ToolRegistrationSet::Terminal,
                    ToolRegistrationSet::Layout,
                    ToolRegistrationSet::Setting,
                    ToolRegistrationSet::Space,
                    ToolRegistrationSet::Workspace,
                    ToolRegistrationSet::Files,
                    ToolRegistrationSet::Knowledge,
                    ToolRegistrationSet::Visual,
                    ToolRegistrationSet::Bookmark,
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
            )
            .add_plugins((
                application::ApplicationToolPlugin,
                browser::BrowserToolPlugin,
                terminal::TerminalToolPlugin,
                layout::LayoutToolPlugin,
                setting::SettingToolPlugin,
                space::SpaceToolPlugin,
                workspace::WorkspaceToolPlugin,
                files::FileToolPlugin,
                knowledge::KnowledgeToolPlugin,
                visual::VisualToolPlugin,
                bookmark::BookmarkToolPlugin,
            ));
    }
}

#[cfg(test)]
impl ToolPlugin {
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(Self);
        app.update();
        app
    }
}

#[derive(Component)]
pub struct McpToolRequest<T> {
    tool: T,
    call: ToolCall,
}

#[derive(Component)]
pub(super) struct ParsedToolCall<T> {
    call: ToolCall,
    args: T,
}

impl<T: Component> ParsedToolCall<T> {
    pub(super) fn args(&self) -> &T {
        &self.args
    }

    pub(super) fn anchor(&self) -> Option<ProcessId> {
        self.call.anchor
    }

    pub(super) fn host_shell(&self) -> &str {
        &self.call.host_shell
    }

    pub(super) fn require_anchor(&self) -> Result<ProcessId, String> {
        self.call.require_anchor(&self.call.name)
    }

    pub(super) fn serialized_args(&self) -> Result<Value, String>
    where
        T: Serialize,
    {
        ToolCall::serialize_arguments(&self.args)
    }

    pub(super) fn finish(
        &self,
        request: Entity,
        commands: &mut Commands,
        result: Result<DispatchTarget, String>,
    ) {
        commands.entity(request).remove::<Self>();
        self.call.finish_dispatch(request, commands, result);
    }

    pub(super) fn finish_execution(
        &self,
        request: Entity,
        commands: &mut Commands,
        result: Result<ToolExecution, String>,
    ) {
        commands.entity(request).remove::<Self>();
        self.call.finish(request, commands, result);
    }
}

impl<T: Component> McpToolRequest<T> {
    pub fn tool(&self) -> &T {
        &self.tool
    }

    pub fn name(&self) -> &str {
        &self.call.name
    }

    pub fn arguments(&self) -> &Value {
        &self.call.arguments
    }

    pub fn anchor(&self) -> Option<ProcessId> {
        self.call.anchor
    }

    pub fn host_shell(&self) -> &str {
        &self.call.host_shell
    }

    pub fn parse<A: serde::de::DeserializeOwned>(&self) -> Result<A, String> {
        self.call.parse(&self.call.name)
    }

    pub fn require_anchor(&self) -> Result<ProcessId, String> {
        self.call.require_anchor(&self.call.name)
    }

    pub fn finish(
        &self,
        request: Entity,
        commands: &mut Commands,
        result: Result<DispatchTarget, String>,
    ) {
        commands.entity(request).remove::<Self>();
        self.call.finish_dispatch(request, commands, result);
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
        if !app.is_plugin_added::<ToolPlugin>() {
            app.add_plugins(ToolPlugin);
        }
        app.insert_resource(McpToolManifest::<T>::new(self.manifest))
            .add_systems(
                Startup,
                register_mcp_tools::<T>.after(ToolRegistrationSet::Bookmark),
            )
            .add_systems(Update, route_mcp_tools::<T>.in_set(ToolRequestSet));
    }
}

#[derive(Resource)]
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

fn register_mcp_tools<T>(manifest: Res<McpToolManifest<T>>, mut tools: ToolSpawner)
where
    T: Component + serde::de::DeserializeOwned + Serialize,
{
    tools.spawn_manifest(ToolManifest::<T>::from_ron(manifest.source));
}

fn route_mcp_tools<T>(mut commands: Commands, calls: ToolCalls<T>)
where
    T: Component + Clone,
{
    for (request, call, tool) in calls.iter() {
        commands
            .entity(request)
            .remove::<ToolCall>()
            .insert(McpToolRequest {
                tool: tool.clone(),
                call: call.clone(),
            });
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub(crate) struct ToolRequestSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolRequestFlush;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub(super) enum ToolRegistrationSet {
    Application,
    Browser,
    Terminal,
    Layout,
    Setting,
    Space,
    Workspace,
    Files,
    Knowledge,
    Visual,
    Bookmark,
}

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

#[derive(bevy_ecs::system::SystemParam)]
pub struct ToolCatalog<'w, 's> {
    tools: RegisteredTools<'w, 's>,
}

type RegisteredTool<'w> = (
    Entity,
    &'w Name,
    &'w ToolAliases,
    &'w ToolDescription,
    &'w ToolInputSchema,
    &'w ToolAccess,
    &'w ToolOrder,
    Option<&'w ShellAware>,
);

type RegisteredTools<'w, 's> = Query<'w, 's, RegisteredTool<'static>, With<McpTool>>;

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

    #[cfg(test)]
    const fn strict(acp_session: bool, acp_terminals: bool) -> Self {
        Self {
            acp_session,
            acp_terminals,
            allow_command: false,
        }
    }
}

impl ToolCatalog<'_, '_> {
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
        for (tool, registered_name, aliases, _, _, access, _, _) in &self.tools {
            if registered_name.as_str() != normalized
                && !aliases.0.iter().any(|alias| alias == normalized)
            {
                continue;
            }
            if !access.0.allows(policy.acp_session, policy.acp_terminals) {
                return Err(format!("tool {normalized} is unavailable for ACP sessions"));
            }
            return Ok(ToolCall {
                tool: Some(tool),
                name: registered_name.as_str().to_string(),
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
    #[cfg(test)]
    fn all(world: &mut World, acp_session: bool, acp_terminals: bool, shell: &str) -> Vec<Self> {
        let shell = shell.to_string();
        world
            .run_system_once(move |tools: ToolCatalog| {
                tools.definitions(acp_session, acp_terminals, &shell)
            })
            .expect("tool catalog system must run")
    }

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
pub enum DispatchTarget {
    Command(AgentCommand),
    Query(AgentQuery),
}

impl DispatchTarget {
    fn from_execution(
        call: &ToolCall,
        outcome: &Result<ToolExecution, String>,
    ) -> Result<Self, String> {
        match outcome {
            Ok(ToolExecution::Dispatch { target, .. }) => Ok(target.clone()),
            Ok(ToolExecution::Command {
                name, arguments, ..
            }) => Ok(Self::Command(AgentCommand::InvokeCommand {
                id: name.clone(),
                args: JsonValue::from(arguments.clone()),
            })),
            Ok(ToolExecution::Protocol { .. }) => {
                Err(format!("tool {} requires MCP protocol context", call.name))
            }
            Ok(ToolExecution::List { .. }) => {
                Err("tool listing is not a dispatchable tool".to_string())
            }
            Err(message) => Err(message.clone()),
        }
    }
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

#[derive(Clone)]
pub(crate) enum ToolExecution {
    List {
        definitions: Vec<ToolDefinition>,
    },
    Command {
        name: String,
        arguments: Value,
        anchor: Option<ProcessId>,
    },
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
pub(crate) struct ToolInputSchema(pub(crate) InputSchema);

#[derive(Component)]
pub(crate) struct ToolAccess(pub(crate) ToolAvailability);

#[derive(Component)]
pub(crate) struct ToolOrder(pub(crate) u32);

#[derive(Component)]
pub(crate) struct ShellAware;

#[derive(Component)]
pub(crate) struct ToolOutcome(pub(crate) Result<ToolExecution, String>);

#[derive(Component, Clone, Debug)]
pub struct ToolDispatchError(String);

impl ToolDispatchError {
    pub fn message(&self) -> &str {
        &self.0
    }
}

#[derive(Resource, Default)]
struct NextToolOrder(u32);

#[derive(Clone, Component)]
pub struct ToolCall {
    tool: Option<Entity>,
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

    pub(super) fn parse_into<T>(&self, request: Entity, commands: &mut Commands)
    where
        T: Component + serde::de::DeserializeOwned,
    {
        match self.parse::<T>(&self.name) {
            Ok(args) => {
                commands
                    .entity(request)
                    .remove::<Self>()
                    .insert(ParsedToolCall {
                        call: self.clone(),
                        args,
                    });
            }
            Err(message) => self.finish_dispatch(request, commands, Err(message)),
        }
    }

    fn serialize_arguments<T: Serialize>(arguments: T) -> Result<Value, String> {
        serde_json::to_value(arguments)
            .map_err(|error| format!("MCP tool arguments must serialize: {error}"))
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
        let dispatch = DispatchTarget::from_execution(self, &result);
        let mut entity = commands.entity(request);
        entity.remove::<Self>().insert(ToolOutcome(result));
        match dispatch {
            Ok(target) => {
                entity.insert(target);
            }
            Err(message) => {
                entity.insert(ToolDispatchError(message));
            }
        }
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

    #[cfg(test)]
    fn dispatch(
        app: &mut App,
        name: &str,
        arguments: Value,
        anchor: Option<ProcessId>,
        host_shell: &str,
        acp_session: bool,
        acp_terminals: bool,
    ) -> Result<ToolExecution, String> {
        let normalized = canonical_tool_name(name).to_string();
        let name = name.to_string();
        let host_shell = host_shell.to_string();
        let call = app
            .world_mut()
            .run_system_once(move |tools: ToolCatalog| {
                tools.call(
                    &name,
                    arguments.clone(),
                    anchor,
                    &host_shell,
                    ToolCallPolicy::strict(acp_session, acp_terminals),
                )
            })
            .map_err(|error| error.to_string())??;
        let request = app.world_mut().spawn(call).id();
        app.update();
        let outcome = app
            .world_mut()
            .entity_mut(request)
            .take::<ToolOutcome>()
            .ok_or_else(|| format!("tool {normalized} did not produce an execution"))?;
        app.world_mut().despawn(request);
        outcome.0
    }

    #[cfg(test)]
    fn find(world: &mut World, name: &str) -> Option<(Entity, String, ToolAvailability)> {
        let name = name.to_string();
        world
            .run_system_once(move |tools: ToolCatalog| {
                tools
                    .call(
                        &name,
                        Value::Null,
                        None,
                        "",
                        ToolCallPolicy::strict(false, false),
                    )
                    .ok()
                    .and_then(|call| {
                        call.tool
                            .map(|tool| (tool, call.name, ToolAvailability::Always))
                    })
            })
            .ok()
            .flatten()
    }
}

type PendingCommandCalls<'w, 's> =
    Query<'w, 's, (Entity, &'static ToolCall), (Added<ToolCall>, Without<ToolOutcome>)>;

fn dispatch_command_calls(mut commands: Commands, calls: PendingCommandCalls) {
    for (entity, call) in &calls {
        if call.tool.is_some() {
            continue;
        }
        call.finish(
            entity,
            &mut commands,
            Ok(ToolExecution::Command {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            }),
        );
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
            let tool = call.tool?;
            self.tools.get(tool).ok().map(|tool| (request, call, tool))
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

pub(crate) fn canonical_tool_name(name: &str) -> &str {
    name.strip_prefix("vmux_").unwrap_or(name)
}

#[cfg(test)]
fn tool_definitions() -> Vec<ToolDefinition> {
    tool_definitions_filtered(false, false, "")
}

#[cfg(test)]
fn tool_definitions_filtered(
    acp_session: bool,
    acp_terminals: bool,
    shell: &str,
) -> Vec<ToolDefinition> {
    let mut app = ToolPlugin::app();
    let shell = shell.to_string();
    app.world_mut()
        .run_system_once(move |tools: ToolCatalog| {
            tools.definitions(acp_session, acp_terminals, &shell)
        })
        .expect("tool catalog system must run")
}

#[cfg(test)]
fn dispatch_from_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    dispatch_with_anchor(name, arguments, None)
}

#[cfg(test)]
fn dispatch_agent_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    let normalized = canonical_tool_name(name);
    match dispatch_from_tool_call(normalized, arguments.clone()) {
        Ok(target) => Ok(target),
        Err(message) if message == format!("unknown tool: {normalized}") => {
            Ok(DispatchTarget::Command(AgentCommand::InvokeCommand {
                id: normalized.to_string(),
                args: JsonValue::from(arguments),
            }))
        }
        Err(message) => Err(message),
    }
}

#[cfg(test)]
fn dispatch_with_anchor(
    name: &str,
    arguments: Value,
    anchor: Option<ProcessId>,
) -> Result<DispatchTarget, String> {
    dispatch_in_shell(name, arguments, anchor, "")
}

#[cfg(test)]
fn dispatch_in_shell(
    name: &str,
    arguments: Value,
    anchor: Option<ProcessId>,
    host_shell: &str,
) -> Result<DispatchTarget, String> {
    let mut app = ToolPlugin::app();
    match ToolCall::dispatch(&mut app, name, arguments, anchor, host_shell, false, false)? {
        ToolExecution::Dispatch { target, .. } => Ok(target),
        ToolExecution::Protocol { .. }
        | ToolExecution::List { .. }
        | ToolExecution::Command { .. } => Err(format!(
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
