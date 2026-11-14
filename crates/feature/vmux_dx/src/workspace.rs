use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use bevy_app::{App, AppExit, PreStartup, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::cli::{
    CliArgumentManifest, CliCommandManifest, CliInvocation, CliManifest, CliResult, CliSet,
};
use vmux_ecs::manifest::FeatureManifest;
use vmux_ecs::{Active, Order};

pub use crate::workspace_io::Executable;
use crate::workspace_io::MetadataFile;

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Input>() {
            app.insert_resource(Input(std::env::args_os().skip(1).collect()));
        }
        app.init_resource::<Admission>()
            .init_resource::<Failure>()
            .add_systems(
                PreStartup,
                (
                    seed,
                    inspect,
                    choose,
                    admit,
                    resolve,
                    materialize,
                    validate,
                    project,
                    report,
                )
                    .chain(),
            )
            .add_systems(PreUpdate, route.after(CliSet::Parse));
    }
}

pub struct Host;

impl Host {
    pub fn selected(arguments: Vec<OsString>) -> bool {
        let mut app = App::new();
        app.insert_resource(Input(arguments))
            .insert_resource(Probe)
            .add_plugins(Plugin);
        app.world_mut()
            .spawn(FeatureManifest::of::<crate::Feature>());
        app.update();
        app.world().resource::<Admission>().0
    }
}

#[derive(Resource)]
struct Input(Vec<OsString>);

#[derive(Resource)]
struct Probe;

#[derive(Resource, Default)]
struct Admission(bool);

#[derive(Resource, Default)]
struct Failure(Option<String>);

#[derive(Component)]
struct Directory(PathBuf);

#[derive(Component)]
struct Configured;

#[derive(Component)]
struct GitBoundary;

#[derive(Component)]
struct InspectionError(String);

#[derive(Component)]
struct Chosen;

#[derive(Component)]
pub struct Provider {
    pub root: PathBuf,
    pub package: String,
    pub binary: String,
    pub cargo: PathBuf,
}

#[derive(Component)]
struct ManifestPath(PathBuf);

#[derive(Component)]
struct RawFeature;

#[derive(Component)]
struct DefaultCommand(Option<String>);

#[derive(Component)]
struct Command {
    id: String,
    about: Option<String>,
    hidden: bool,
    required: bool,
    provider: Entity,
}

#[derive(Component)]
struct Argument(CliArgumentManifest);

#[derive(Component)]
struct Validated;

#[derive(Component)]
pub struct Dispatch {
    pub provider: Entity,
    pub arguments: Vec<OsString>,
}

fn seed(input: Res<Input>, mut failure: ResMut<Failure>, mut commands: Commands) {
    let mut root = match std::env::current_dir() {
        Ok(root) => root,
        Err(error) => {
            failure.0 = Some(error.to_string());
            return;
        }
    };
    let mut arguments = input.0.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            break;
        }
        if argument == "--root" {
            let Some(path) = arguments.next() else {
                failure.0 = Some("--root requires a path".into());
                return;
            };
            root = path.into();
        } else if let Some(path) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--root="))
        {
            root = path.into();
        }
    }
    let mut root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            failure.0 = Some(error.to_string());
            return;
        }
    };
    let mut order = 0;
    loop {
        commands.spawn((Directory(root.clone()), Order(order)));
        if !root.pop() {
            break;
        }
        order += 1;
    }
}

fn inspect(directories: Query<(Entity, &Directory, &Order)>, mut commands: Commands) {
    let mut ordered = directories.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(_, _, order)| order.0);
    for (entity, directory, _) in ordered {
        let boundary = directory.0.join(".git").exists();
        if boundary {
            commands.entity(entity).insert(GitBoundary);
        }
        let cargo = directory.0.join("Cargo.toml");
        if cargo.is_file() {
            match MetadataFile(cargo).cargo() {
                Ok(Some(declaration)) => {
                    commands.entity(entity).insert(Configured);
                    if declaration.providers.len() > 16 {
                        commands.entity(entity).insert(InspectionError(
                            "Workspace declares too many CLI providers".into(),
                        ));
                        break;
                    }
                    for (index, definition) in declaration.providers.into_iter().enumerate() {
                        commands.spawn((
                            Name::new(definition.name),
                            Order(index as u32),
                            ChildOf(entity),
                            Provider {
                                root: directory.0.clone(),
                                package: definition.package,
                                binary: definition.binary,
                                cargo: definition.cargo.unwrap_or_else(|| "Cargo.toml".into()),
                            },
                            ManifestPath(definition.manifest),
                        ));
                    }
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(InspectionError(error.to_string()));
                    break;
                }
            }
        }
        if boundary {
            break;
        }
    }
}

type Directories<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Order,
        Option<&'static Configured>,
        Option<&'static GitBoundary>,
        Option<&'static InspectionError>,
    ),
>;

fn choose(
    directories: Directories,
    providers: Query<(Entity, &ChildOf), With<Provider>>,
    mut failure: ResMut<Failure>,
    mut commands: Commands,
) {
    if failure.0.is_some() {
        return;
    }
    let mut ordered = directories.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(_, order, _, _, _)| order.0);
    for (entity, _, configured, boundary, error) in ordered {
        if let Some(error) = error {
            failure.0 = Some(error.0.clone());
            return;
        }
        if configured.is_some() {
            commands.entity(entity).insert(Chosen);
            for (provider, parent) in &providers {
                if parent.parent() == entity {
                    commands.entity(provider).insert(Active);
                }
            }
            return;
        }
        if boundary.is_some() {
            return;
        }
    }
}

fn admit(
    input: Res<Input>,
    providers: Query<&Name, (With<Provider>, With<Active>)>,
    features: Query<&FeatureManifest>,
    mut admission: ResMut<Admission>,
) {
    let Some(name) = input.0.first() else {
        return;
    };
    for candidate in &providers {
        if std::ffi::OsStr::new(candidate.as_str()) == name {
            admission.0 = true;
            return;
        }
    }
    for feature in &features {
        if let Some(cli) = &feature.cli {
            for candidate in &cli.commands {
                if std::ffi::OsStr::new(&candidate.name) == name {
                    admission.0 = true;
                    return;
                }
            }
        }
    }
}

fn resolve(
    probe: Option<Res<Probe>>,
    mut providers: Query<(Entity, &mut Provider, &ManifestPath), With<Active>>,
    mut failure: ResMut<Failure>,
    mut commands: Commands,
) {
    if probe.is_some() || failure.0.is_some() {
        return;
    }
    for (entity, mut provider, manifest) in &mut providers {
        for value in [&provider.package, &provider.binary] {
            if value.is_empty()
                || value.len() > 128
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                failure.0 =
                    Some("Workspace provider package and binary must be Cargo names".into());
                return;
            }
        }
        let cargo = match provider.root.join(&provider.cargo).canonicalize() {
            Ok(path) => path,
            Err(error) => {
                failure.0 = Some(error.to_string());
                return;
            }
        };
        if !cargo.starts_with(&provider.root) || !cargo.is_file() {
            failure.0 = Some("Workspace provider Cargo manifest escapes its workspace".into());
            return;
        }
        let (path, feature) = match MetadataFile(provider.root.join(&manifest.0)).feature() {
            Ok(value) => value,
            Err(error) => {
                failure.0 = Some(error.to_string());
                return;
            }
        };
        if !path.starts_with(&provider.root) {
            failure.0 = Some("Workspace provider manifest escapes its workspace".into());
            return;
        }
        provider.cargo = cargo;
        commands
            .entity(entity)
            .insert((RawFeature, feature))
            .remove::<ManifestPath>();
    }
}

type Sources<'w, 's> =
    Query<'w, 's, (Entity, &'static FeatureManifest), (With<RawFeature>, With<Active>)>;

fn materialize(sources: Sources, mut failure: ResMut<Failure>, mut commands: Commands) {
    if failure.0.is_some() {
        return;
    }
    for (provider, raw) in &sources {
        let Some(cli) = &raw.cli else {
            failure.0 = Some("Workspace provider requires a CLI manifest".into());
            return;
        };
        commands
            .entity(provider)
            .insert(DefaultCommand(cli.default.clone()))
            .remove::<(RawFeature, FeatureManifest)>();
        let mut pending = Vec::new();
        for (order, command) in cli.commands.iter().cloned().enumerate() {
            pending.push((provider, order as u32, command));
        }
        let mut count = 0;
        while let Some((parent, order, command)) = pending.pop() {
            count += 1;
            if count > 128 || command.arguments.len() > 64 {
                failure.0 = Some("Workspace provider CLI manifest exceeds its bounds".into());
                return;
            }
            let entity = commands
                .spawn((
                    Name::new(command.name),
                    Order(order),
                    ChildOf(parent),
                    Command {
                        id: command.id,
                        about: command.about,
                        hidden: command.hidden,
                        required: command.subcommand_required,
                        provider,
                    },
                ))
                .id();
            for (order, argument) in command.arguments.into_iter().enumerate() {
                commands.spawn((Argument(argument), Order(order as u32), ChildOf(entity)));
            }
            for (order, child) in command.commands.into_iter().enumerate() {
                pending.push((entity, order as u32, child));
            }
        }
    }
}

type Validating<'w, 's> =
    Query<'w, 's, (Entity, &'static Name, &'static DefaultCommand), (With<Provider>, With<Active>)>;

fn validate(
    probe: Option<Res<Probe>>,
    providers: Validating,
    declarations: Query<(&Command, &Name, &ChildOf)>,
    features: Query<&FeatureManifest>,
    mut failure: ResMut<Failure>,
    mut commands: Commands,
) {
    if probe.is_some() || failure.0.is_some() {
        return;
    }
    let mut names = BTreeSet::new();
    let mut identifiers = Vec::new();
    for feature in &features {
        if let Some(cli) = &feature.cli {
            let mut pending = cli.commands.iter().collect::<Vec<_>>();
            for command in &cli.commands {
                names.insert(command.name.as_str());
            }
            while let Some(command) = pending.pop() {
                identifiers.push(command.id.as_str());
                pending.extend(command.commands.iter());
            }
        }
    }
    let mut namespaces = Vec::<String>::new();
    let mut ids = BTreeSet::new();
    for (provider, name, default) in &providers {
        let roots = declarations
            .iter()
            .filter(|(_, _, parent)| parent.parent() == provider)
            .collect::<Vec<_>>();
        if default.0.is_some() || roots.len() != 1 {
            failure.0 = Some("Workspace provider must export one command and no default".into());
            return;
        }
        let (root, root_name, _) = roots[0];
        if root_name != name || !names.insert(name.as_str()) {
            failure.0 = Some("Workspace provider command differs from its declared name or duplicates another command".into());
            return;
        }
        if root.id.is_empty() || root.id == "dx" || root.id.starts_with("dx.") {
            failure.0 = Some("Workspace provider cannot use the built-in DX namespace".into());
            return;
        }
        let prefix = format!("{}.", root.id);
        for other in &namespaces {
            if root.id == *other
                || root.id.starts_with(&format!("{other}."))
                || other.starts_with(&prefix)
            {
                failure.0 = Some("Workspace CLI namespaces overlap".into());
                return;
            }
        }
        if identifiers
            .iter()
            .any(|id| *id == root.id || id.starts_with(&prefix))
        {
            failure.0 = Some("Workspace CLI namespace overlaps a registered command".into());
            return;
        }
        for (command, name, _) in &declarations {
            if command.provider != provider {
                continue;
            }
            if name.as_str().is_empty()
                || command.id.is_empty()
                || !ids.insert(command.id.as_str())
                || command.id != root.id && !command.id.starts_with(&prefix)
            {
                failure.0 =
                    Some("Workspace provider command escapes or duplicates its namespace".into());
                return;
            }
        }
        namespaces.push(root.id.clone());
    }
    for (provider, _, _) in &providers {
        commands.entity(provider).insert(Validated);
    }
}

#[derive(SystemParam)]
struct Catalog<'w, 's> {
    commands: Query<
        'w,
        's,
        (
            Entity,
            &'static Command,
            &'static Name,
            &'static Order,
            &'static ChildOf,
        ),
    >,
    arguments: Query<'w, 's, (&'static Argument, &'static Order, &'static ChildOf)>,
}

impl Catalog<'_, '_> {
    fn command(&self, entity: Entity) -> CliCommandManifest {
        let (_, command, name, _, _) = self.commands.get(entity).unwrap();
        let mut children = self
            .commands
            .iter()
            .filter(|(_, _, _, _, parent)| parent.parent() == entity)
            .collect::<Vec<_>>();
        children.sort_by_key(|(_, _, _, order, _)| order.0);
        let mut declarations = self
            .arguments
            .iter()
            .filter(|(_, _, parent)| parent.parent() == entity)
            .collect::<Vec<_>>();
        declarations.sort_by_key(|(_, order, _)| order.0);
        let mut arguments = Vec::new();
        for (argument, _, _) in declarations {
            arguments.push(argument.0.clone());
        }
        let mut commands = Vec::new();
        for (child, _, _, _, _) in children {
            commands.push(self.command(child));
        }
        CliCommandManifest {
            id: command.id.clone(),
            name: name.as_str().into(),
            about: command.about.clone(),
            hidden: command.hidden,
            subcommand_required: command.required,
            arguments,
            commands,
        }
    }
}

fn project(providers: Query<Entity, With<Validated>>, catalog: Catalog, mut commands: Commands) {
    for provider in &providers {
        let mut roots = catalog
            .commands
            .iter()
            .filter(|(_, _, _, _, parent)| parent.parent() == provider)
            .collect::<Vec<_>>();
        roots.sort_by_key(|(_, _, _, order, _)| order.0);
        let mut feature = FeatureManifest::parse("()");
        let mut cli: CliManifest = ron::from_str("(commands: [])").unwrap();
        for (root, _, _, _, _) in roots {
            cli.commands.push(catalog.command(root));
        }
        feature.cli = Some(cli);
        commands.entity(provider).insert(feature);
    }
}

fn report(probe: Option<Res<Probe>>, failure: Res<Failure>, mut exits: MessageWriter<AppExit>) {
    if probe.is_some() {
        return;
    }
    if let Some(error) = &failure.0 {
        eprintln!("vmux: workspace CLI metadata: {error}");
        exits.write(AppExit::error());
    }
}

type Incoming<'w, 's> =
    Query<'w, 's, (Entity, &'static CliInvocation), (Without<Dispatch>, Without<CliResult>)>;

fn route(
    requests: Incoming,
    providers: Query<Entity, With<Validated>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    for (entity, invocation) in &requests {
        let Some((selected, declaration, _, _, _)) =
            catalog.commands.iter().find(|(_, command, _, _, _)| {
                command.id == invocation.command && providers.contains(command.provider)
            })
        else {
            continue;
        };
        let provider = declaration.provider;
        let mut path = vec![selected];
        let mut current = selected;
        loop {
            let (_, _, _, _, parent) = catalog.commands.get(current).unwrap();
            if parent.parent() == provider {
                break;
            }
            current = parent.parent();
            path.push(current);
        }
        path.reverse();
        let mut arguments = Vec::new();
        let mut known = BTreeSet::new();
        for command in path {
            let (_, _, name, _, _) = catalog.commands.get(command).unwrap();
            arguments.push(OsString::from(name.as_str()));
            let mut declarations = catalog
                .arguments
                .iter()
                .filter(|(_, _, parent)| parent.parent() == command)
                .collect::<Vec<_>>();
            declarations.sort_by_key(|(_, order, _)| order.0);
            for (argument, _, _) in declarations {
                let argument = &argument.0;
                known.insert(argument.id.as_str());
                let Some(values) = invocation.arguments.get(&argument.id) else {
                    continue;
                };
                if argument.trailing {
                    if !values.is_empty() {
                        arguments.push("--".into());
                        arguments.extend(values.iter().cloned());
                    }
                    continue;
                }
                if argument.flag && invocation.value(&argument.id) != Some("true") {
                    continue;
                }
                if let Some(long) = &argument.long {
                    arguments.push(format!("--{long}").into());
                } else if let Some(short) = argument.short {
                    arguments.push(format!("-{short}").into());
                }
                if !argument.flag {
                    arguments.extend(values.iter().cloned());
                }
            }
        }
        if invocation
            .arguments
            .keys()
            .any(|key| !known.contains(key.as_str()))
        {
            commands.entity(entity).insert(CliResult(Err(
                "Workspace provider argument is not declared".into(),
            )));
            continue;
        }
        commands.entity(entity).insert(Dispatch {
            provider,
            arguments,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_reads_canonical_argument_entities_instead_of_the_manifest_projection() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Cargo.toml"), "[workspace]\nmembers = []\n[workspace.metadata.vmux.dx]\nproviders = [{name='tools',package='tools',binary='tools',manifest='cli.ron'}]\n").unwrap();
        std::fs::write(
            directory.path().join("cli.ron"),
            r#"(cli: Some((commands: [
                (id: "tools", name: "tools", commands: [
                    (id: "tools.echo", name: "echo", arguments: [
                        (id: "value", long: Some("original")),
                    ]),
                ]),
            ])))"#,
        )
        .unwrap();
        let mut app = App::new();
        app.insert_resource(Input(vec![
            "tools".into(),
            "--root".into(),
            directory.path().as_os_str().into(),
        ]))
        .add_plugins(Plugin);
        app.update();
        {
            let mut arguments = app.world_mut().query::<&mut Argument>();
            for mut argument in arguments.iter_mut(app.world_mut()) {
                if argument.0.id == "value" {
                    argument.0.long = Some("changed".into());
                }
            }
        }
        let entity = app
            .world_mut()
            .spawn(CliInvocation {
                command: "tools.echo".into(),
                arguments: std::collections::BTreeMap::from([(
                    "value".into(),
                    vec!["payload".into()],
                )]),
            })
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Dispatch>(entity).unwrap().arguments,
            vec![
                OsString::from("tools"),
                "echo".into(),
                "--changed".into(),
                "payload".into()
            ]
        );
    }
}
