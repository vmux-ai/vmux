use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use serde::Serialize;
use serde_json::Value;
use vmux_api::protocol::{AgentCommandTool, AgentRequest};
use vmux_ecs::{HostShell, JsonArguments, RegistrationOrder};
use vmux_feature::manifest::{FeatureManifest, ToolAvailability};

use vmux_api::JsonSchema;

pub struct ToolRegistryPlugin;

impl Plugin for ToolRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Startup,
            (
                ToolStartupRegistrySet,
                ToolStartupRegistryFlushSet,
                ToolStartupManifestSet,
                ToolStartupManifestFlushSet,
                ToolStartupBindingSet,
            )
                .chain(),
        )
        .add_systems(Startup, spawn.in_set(ToolStartupRegistrySet))
        .add_systems(
            Startup,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolStartupRegistryFlushSet),
        )
        .add_systems(Startup, register.in_set(ToolStartupManifestSet))
        .add_systems(
            Startup,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolStartupManifestFlushSet),
        )
        .add_systems(Startup, bind.in_set(ToolStartupBindingSet))
        .configure_sets(
            Update,
            (
                ToolResolveSet,
                ToolResolveFlush,
                ToolRequestSet,
                ToolRequestFlush,
                ToolDispatchSet,
                ToolDispatchFlush,
            )
                .chain(),
        )
        .add_systems(Update, (catalogs, invocations).in_set(ToolResolveSet))
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolResolveFlush),
        )
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolRequestFlush),
        )
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolDispatchFlush),
        );
    }
}

#[derive(Component, Clone, Copy)]
pub struct ToolInputRegistration {
    name: &'static str,
    parse: fn(Entity, &Name, &JsonArguments, &mut Commands),
}

impl ToolInputRegistration {
    pub const fn of<T: ToolInput>() -> Self {
        Self {
            name: T::NAME,
            parse: parse_input::<T>,
        }
    }
}

inventory::collect!(ToolInputRegistration);

fn parse_input<T: ToolInput>(
    request: Entity,
    name: &Name,
    arguments: &JsonArguments,
    commands: &mut Commands,
) {
    match arguments.parse::<T>(name.as_str()) {
        Ok(arguments) => {
            commands.entity(request).insert(arguments);
        }
        Err(message) => {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

pub trait ToolInput: Component + serde::de::DeserializeOwned {
    const NAME: &'static str;
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Tool registry"), NextToolOrder::default()));
}

fn register(
    features: Query<&FeatureManifest>,
    mut commands: Commands,
    mut next_order: Single<&mut NextToolOrder>,
) {
    for feature in &features {
        for entry in feature.tools.iter().cloned() {
            let order = next_order.0;
            next_order.0 += 1;
            let mut entity = commands.spawn((
                RegisteredTool,
                Name::new(entry.name),
                ToolAliases(entry.aliases),
                ToolDescription(entry.description),
                ToolInputSchema(
                    serde_json::to_value(entry.input_schema)
                        .expect("tool manifest input schema serializes"),
                ),
                ToolAccess(entry.availability),
                RegistrationOrder(order),
            ));
            if entry.shell_aware {
                entity.insert(ShellAware);
            }
        }
    }
}

fn bind(tools: Query<(Entity, &Name), With<RegisteredTool>>, mut commands: Commands) {
    for input in inventory::iter::<ToolInputRegistration> {
        let Some((tool_entity, _)) = tools.iter().find(|(_, name)| name.as_str() == input.name)
        else {
            continue;
        };
        commands.entity(tool_entity).insert(*input);
    }
}

fn parse(
    calls: Query<(Entity, &Name, &JsonArguments, &ToolTarget), Added<ToolCall>>,
    tools: Query<&ToolInputRegistration>,
    mut commands: Commands,
) {
    for (request, name, arguments, target) in &calls {
        let Ok(binding) = tools.get(target.entity()) else {
            continue;
        };
        (binding.parse)(request, name, arguments, &mut commands);
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolResolveSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolResolveFlush;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolStartupRegistrySet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolStartupRegistryFlushSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolStartupManifestSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolStartupManifestFlushSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolStartupBindingSet;

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
    pub input_schema: JsonSchema,
}

type RegisteredTools<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        &'static ToolAliases,
        &'static ToolDescription,
        &'static ToolInputSchema,
        &'static ToolAccess,
        &'static RegistrationOrder,
        Option<&'static ShellAware>,
    ),
    With<RegisteredTool>,
>;

#[derive(Component, Clone, Copy)]
pub struct ToolCatalogRequest;

#[derive(Component, Clone, Debug)]
pub struct ToolCatalog(pub Vec<ToolDefinition>);

#[derive(Component, Clone, Copy)]
pub struct ToolInvocation;

#[derive(Component, Clone, Copy)]
pub struct ToolCommandFallback;

#[derive(Component, Clone, Copy)]
pub struct UnclaimedToolInvocation;

#[derive(Component, Clone, Copy)]
pub struct AcpSessionContext;

#[derive(Component, Clone, Copy)]
pub struct AcpTerminalContext;

fn catalogs(
    requests: Query<
        (
            Entity,
            Option<&HostShell>,
            Has<AcpSessionContext>,
            Has<AcpTerminalContext>,
        ),
        Added<ToolCatalogRequest>,
    >,
    tools: RegisteredTools,
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
                description.push_str(&ToolDescription::shell_note(
                    shell.map_or("", |shell| shell.0.as_str()),
                ));
            }
            definitions.push((
                order.0,
                ToolDefinition {
                    name: name.as_str().to_string(),
                    description,
                    input_schema: JsonSchema::try_from(schema.0.clone())
                        .expect("registered tool input schema is valid"),
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

fn invocations(
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
    tools: RegisteredTools,
    mut commands: Commands,
) {
    for (request_entity, requested_name, _, acp_session, acp_terminals, command_fallback) in
        &invocations
    {
        let normalized = requested_name
            .as_str()
            .strip_prefix("vmux_")
            .unwrap_or(requested_name.as_str());
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
                ToolTarget::new(tool_entity),
            ));
            matched = Some(());
            break;
        }
        if matched.is_some() {
            continue;
        }
        if command_fallback {
            commands.entity(request_entity).insert((
                Name::new(normalized.to_string()),
                ToolCall,
                UnclaimedToolInvocation,
            ));
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
        commands: Vec<AgentCommandTool>,
    ) -> Result<Vec<Self>, String> {
        for command in commands {
            let input_schema = Value::try_from(&command.input_schema)
                .map_err(|error| format!("invalid command input schema: {error}"))?;
            let input_schema = JsonSchema::try_from(input_schema)
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
pub struct ToolCommand(pub Result<AgentRequest, String>);

#[derive(Component, Clone, Debug)]
pub struct ToolQuery(pub Result<AgentRequest, String>);

#[derive(Component)]
struct RegisteredTool;

#[derive(Component)]
struct ToolAliases(pub(crate) Vec<String>);

#[derive(Component)]
struct ToolDescription(pub(crate) String);

impl ToolDescription {
    fn shell_note(shell: &str) -> String {
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

#[derive(Component)]
struct ToolInputSchema(pub(crate) Value);

#[derive(Component)]
struct ToolAccess(pub(crate) ToolAvailability);

#[derive(Component)]
struct ShellAware;

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

type ToolTarget = vmux_ecs::EntityTarget<RegisteredTool>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tools_from_feature_manifest() {
        let manifest = FeatureManifest::parse(
            r#"(
                tools: [(
                        name: "test",
                        description: "Test",
                        input_schema: (type: Object),
                    )],
                name: "Tools fixture",
            )"#,
        );

        assert_eq!(manifest.tools[0].name, "test");
    }
}
