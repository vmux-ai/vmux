use crate::Settings;
use crate::ci_io::fs::{
    DirectoryProbe, Location, OptionalRead, Print, ProbeRead, Publish, ReadFile, RemoveDirectory,
    RemoveFile, RemoveLink, SetTimes, SourceFacts,
};
use crate::ci_io::process::{Capture, ProbeCapture};
use crate::ci_io::{Flight, IoJob, IoPlugin, Owner, Tracked};
use crate::environment::Environment;
use crate::signals::Signals;
use bevy_app::{App, PreStartup, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::Arc;
use std::task::Waker;
use std::thread;
use std::time::Duration;
use vmux_ecs::cli::{CliInvocation, CliResult, CliWake};
use vmux_ecs::jobs::{JobRequest, JobSet, JobWake};
use vmux_ecs::manifest::{FeatureManifest, FeatureManifestSource, FeaturePlugin};
use vmux_ecs::{CreatedAt, Cwd, Operation, OperationState, Order};
mod artifact;
mod checks;
mod plan;
mod sources;
#[cfg(test)]
mod testing;
mod workflow;
pub struct CiPlugin;
impl bevy_app::Plugin for CiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            IoPlugin::<Capture>::default(),
            IoPlugin::<ProbeCapture>::default(),
            IoPlugin::<ReadFile>::default(),
            IoPlugin::<Publish>::default(),
            IoPlugin::<DirectoryProbe>::default(),
            IoPlugin::<Print>::default(),
        ))
        .add_plugins((
            IoPlugin::<OptionalRead>::default(),
            IoPlugin::<ProbeRead>::default(),
            IoPlugin::<RemoveFile>::default(),
            IoPlugin::<RemoveLink>::default(),
            IoPlugin::<RemoveDirectory>::default(),
            IoPlugin::<SetTimes>::default(),
            IoPlugin::<SourceFacts>::default(),
        ))
        .add_plugins((
            plan::CiPlanPlugin,
            artifact::CiArtifactPlugin,
            sources::CiSourcesPlugin,
            checks::CiChecksPlugin,
        ))
        .configure_sets(
            Update,
            (Stage::Admit, Stage::Step).chain().before(JobSet::Submit),
        )
        .add_systems(PreStartup, initialize)
        .add_systems(Update, (admit, cancel).chain().in_set(Stage::Admit))
        .add_systems(
            Update,
            (cancel, failures, finish).chain().after(JobSet::Apply),
        );
    }
}
impl CiPlugin {
    pub fn invocation(arguments: Vec<OsString>) -> crate::Result<CliInvocation> {
        Arguments::invocation(arguments)
    }
    pub fn run(invocation: CliInvocation) -> crate::Result<u8> {
        let mut app = App::new();
        let wake = Arc::new(CliWake::current());
        app.insert_resource(Signals::install()?)
            .insert_resource(JobWake(Waker::from(wake)))
            .add_plugins(Self);
        let entity = app.world_mut().spawn(invocation).id();
        loop {
            app.update();
            if let Some(result) = app.world_mut().entity_mut(entity).take::<CliResult>() {
                return result.0.map_err(Into::into);
            }
            thread::park_timeout(Duration::from_millis(5));
        }
    }
}
pub(crate) struct Feature;
impl FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("ci/feature.ron");
}
impl Feature {
    fn recognizes(command: &str) -> bool {
        let manifest = FeatureManifest::of::<Self>();
        let Some(cli) = manifest.cli else {
            return false;
        };
        for root in cli.commands {
            for operation in root.commands {
                if operation.id == command {
                    return true;
                }
            }
        }
        false
    }
}
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stage {
    Admit,
    Step,
}
#[derive(Resource)]
struct Input {
    root: PathBuf,
    environment: Environment,
}
#[derive(Component)]
struct Root;
#[derive(Component, Clone)]
struct Context {
    root: PathBuf,
}
impl Context {
    fn location(&self, path: impl Into<PathBuf>) -> Location {
        Location {
            root: self.root.clone(),
            relative: path.into(),
        }
    }
    fn git(&self, environment: &Environment, arguments: &[&str]) -> Capture {
        let mut environment = environment.clone();
        for key in [
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG",
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_COUNT",
            "GIT_OBJECT_DIRECTORY",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_IMPLICIT_WORK_TREE",
            "GIT_GRAFT_FILE",
            "GIT_INDEX_FILE",
            "GIT_NO_REPLACE_OBJECTS",
            "GIT_REPLACE_REF_BASE",
            "GIT_PREFIX",
            "GIT_SHALLOW_FILE",
            "GIT_COMMON_DIR",
        ] {
            environment.remove(key);
        }
        self.process(&environment, "git", arguments)
    }
    fn process(&self, environment: &Environment, program: &str, arguments: &[&str]) -> Capture {
        let mut values = Vec::new();
        for argument in arguments {
            values.push(OsString::from(argument));
        }
        Capture {
            root: self.root.clone(),
            environment: environment.clone(),
            program: program.into(),
            arguments: values,
            input: None,
        }
    }
}
#[derive(Component, Default)]
struct Sequence(u32);
#[derive(Component)]
struct Completed(Result<u8, String>);
#[cfg(test)]
#[derive(Component)]
struct Emission(Vec<u8>);
#[derive(SystemParam)]
struct Reads<'w, 's, T: Component, S: Component> {
    captures: ReadsCaptures<'w, 's, T, S>,
}
impl<T: Component, S: Component> Reads<'_, '_, T, S> {
    fn pending(&self, root: Entity) -> bool {
        for (owner, parent, _, state, data) in &self.captures {
            if owner.0 == root
                && (parent.parent() != root
                    || *state != OperationState::Succeeded
                    || data.is_none())
            {
                return true;
            }
        }
        false
    }
    fn get(&self, root: Entity) -> Option<&T> {
        let mut latest = None;
        for (owner, parent, order, state, data) in &self.captures {
            if owner.0 != root {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|(previous, _, _, _)| order.0 > *previous)
            {
                latest = Some((order.0, parent, state, data));
            }
        }
        let (_, parent, state, data) = latest?;
        if parent.parent() != root || *state != OperationState::Succeeded {
            return None;
        }
        data
    }
}
#[derive(SystemParam)]
struct Io<'w, 's> {
    commands: Commands<'w, 's>,
    captures: IoCaptures<'w, 's>,
}
impl Io<'_, '_> {
    fn request<J: IoJob, T: Component>(
        &mut self,
        root: Entity,
        sequence: &mut Sequence,
        job: J,
        slot: T,
    ) -> Entity {
        sequence.0 += 1;
        let (job, flight) = Tracked::new(job);
        self.commands
            .spawn((
                JobRequest(job),
                flight,
                Owner(root),
                ChildOf(root),
                Order(sequence.0),
                slot,
                Operation::bundle("dx.ci.io").unwrap().running(),
            ))
            .id()
    }
    fn emit<T: Component>(&mut self, root: Entity, sequence: &mut Sequence, job: Print, slot: T) {
        #[cfg(test)]
        let emission = Emission(job.0.clone());
        let _entity = self.request(root, sequence, job, slot);
        #[cfg(test)]
        self.commands.entity(_entity).insert(emission);
    }
    fn fail(&mut self, root: Entity, error: impl Into<String>) {
        self.commands
            .entity(root)
            .insert(Completed(Err(error.into())));
    }
}
struct Arguments;
impl Arguments {
    fn invocation(mut arguments: Vec<OsString>) -> crate::Result<CliInvocation> {
        if arguments.is_empty() {
            return Err("Usage: vmux-ci <operation> [arguments]".into());
        }
        let name = arguments
            .remove(0)
            .into_string()
            .map_err(|_| "CI operation must be UTF-8")?;
        if name.starts_with('-')
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        {
            return Err("Invalid CI operation".into());
        }
        let mut values = BTreeMap::new();
        values.insert("forward".into(), arguments);
        Ok(CliInvocation {
            command: format!("dx.ci.{name}"),
            arguments: values,
        })
    }
    fn values(invocation: &CliInvocation) -> &[OsString] {
        invocation
            .arguments
            .get("forward")
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    fn text(environment: &Environment, key: &str) -> Result<String, String> {
        environment
            .get(key)
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("Missing {key}"))
    }
}
fn initialize(input: Option<Res<Input>>, mut commands: Commands) {
    if input.is_some() {
        return;
    }
    if let Ok(root) = std::env::current_dir() {
        commands.insert_resource(Input {
            root,
            environment: Environment::current(),
        });
    }
}
fn admit(invocations: AdmitInvocations<'_, '_>, input: Res<Input>, mut commands: Commands) {
    for (entity, invocation) in &invocations {
        if !invocation.command.starts_with("dx.ci.") {
            continue;
        }
        if !Feature::recognizes(&invocation.command) {
            commands
                .entity(entity)
                .insert(CliResult(Err("Unknown CI operation".into())));
            continue;
        }
        let root = invocation
            .value_os("root")
            .map(PathBuf::from)
            .unwrap_or_else(|| input.root.clone());
        commands.entity(entity).insert((
            Root,
            Context { root: root.clone() },
            Settings(input.environment.clone()),
            Sequence::default(),
            Cwd(root),
            CreatedAt::now(),
            Name::new(invocation.command.clone()),
            Operation::bundle(&invocation.command).unwrap().running(),
        ));
    }
}
fn cancel(roots: CancelRoots<'_, '_>, children: CancelChildren<'_, '_>, mut commands: Commands) {
    for (root, state, complete) in &roots {
        let canceled = *state == OperationState::Canceled || Signals::pending().is_some();
        let failed = complete.is_some_and(|complete| complete.0.is_err());
        if !canceled && !failed {
            continue;
        }
        if canceled {
            commands
                .entity(root)
                .insert(Completed(Err("CI operation canceled".into())));
        }
        for (entity, owner, state) in &children {
            if owner.0 == root && matches!(state, OperationState::Pending | OperationState::Running)
            {
                commands.entity(entity).insert(OperationState::Canceled);
            }
        }
    }
}
fn failures(
    failures: FailuresFailures<'_, '_>,
    roots: FailuresRoots<'_, '_>,
    mut commands: Commands,
) {
    for (owner, parent, failure, state) in &failures {
        match roots.get(owner.0) {
            Ok(Some(complete)) if complete.0.is_err() => continue,
            Err(_) => continue,
            _ => {}
        }
        if parent.map(ChildOf::parent) != Some(owner.0) {
            commands
                .entity(owner.0)
                .insert(Completed(Err("CI IO ownership changed".into())));
        } else if let Some(failure) = failure {
            commands
                .entity(owner.0)
                .insert(Completed(Err(failure.0.clone())));
        } else if *state == OperationState::Canceled {
            commands
                .entity(owner.0)
                .insert(Completed(Err("CI IO canceled".into())));
        }
    }
}
fn finish(roots: FinishRoots<'_, '_>, flights: FinishFlights<'_, '_>, mut commands: Commands) {
    for (root, complete) in &roots {
        if flights
            .iter()
            .any(|(owner, flight)| owner.0 == root && flight.pending())
        {
            continue;
        }
        let state = if complete.0.is_ok() {
            OperationState::Succeeded
        } else {
            OperationState::Failed
        };
        commands
            .entity(root)
            .insert((CliResult(complete.0.clone()), state));
    }
}
type ReadsCaptures<'w, 's, T, S> = Query<
    'w,
    's,
    (
        &'static Owner,
        &'static ChildOf,
        &'static Order,
        &'static OperationState,
        Option<&'static T>,
    ),
    (With<S>, Without<Root>),
>;
type IoCaptures<'w, 's> = Query<
    'w,
    's,
    (
        EntityRef<'static>,
        &'static ChildOf,
        &'static Order,
        &'static OperationState,
    ),
    (With<Owner>, Without<Root>),
>;
type AdmitInvocations<'w, 's> =
    Query<'w, 's, (Entity, &'static CliInvocation), (Without<Root>, Without<CliResult>)>;
type CancelRoots<'w, 's> =
    Query<'w, 's, (Entity, &'static OperationState, Option<&'static Completed>), With<Root>>;
type CancelChildren<'w, 's> = Query<'w, 's, (Entity, &'static Owner, &'static OperationState)>;
type FailuresFailures<'w, 's> = Query<
    'w,
    's,
    (
        &'static Owner,
        Option<&'static ChildOf>,
        Option<&'static crate::ci_io::Failed>,
        &'static OperationState,
    ),
>;
type FailuresRoots<'w, 's> =
    Query<'w, 's, Option<&'static Completed>, (With<Root>, Without<CliResult>)>;
type FinishRoots<'w, 's> =
    Query<'w, 's, (Entity, &'static Completed), (With<Root>, Without<CliResult>)>;
type FinishFlights<'w, 's> = Query<'w, 's, (&'static Owner, &'static Flight)>;
