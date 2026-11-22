use std::ffi::{OsStr, OsString};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::jobs::JobWake;
use vmux_ecs::{OperationState, Order};

use crate::Settings;
use crate::compiler_lint_io::{Prepare, Read};
use crate::debug::{Complete, Facts, Failed, Io, IoWriter, Lease, Root, Sequence, Stage};
use crate::debug_io::{
    AcquireTarget, AcquiredLock, CaptureOutput, Done, FileBytes, RunProcess, ToolOutput,
};
use crate::execution::Failure;
use crate::policy::Policy;
use crate::repository::Checkout;
use crate::storage::Storage;
use crate::work::Outcome;

pub(crate) struct CompilerLintPlugin;

impl bevy_app::Plugin for CompilerLintPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                read, configure, prepare, guard, capacity, acquire, adopt, toolchain, install,
                fixtures, check, finish,
            )
                .chain()
                .in_set(Stage::Decide),
        );
    }
}

#[derive(Component, Clone)]
#[require(crate::debug::Request, Phase)]
pub(crate) struct Run {
    pub fixtures: bool,
    pub arguments: Option<Vec<OsString>>,
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Read,
    Configure,
    Prepare,
    GuardBefore,
    CapacityBefore,
    Acquire,
    Adopt,
    GuardAfter,
    CapacityAfter,
    Toolchain,
    Install,
    Fixtures,
    Check,
    Done,
}

#[derive(Component, Clone)]
struct Configuration {
    channel: String,
    version: String,
}

impl Configuration {
    fn parse(toolchain: &[u8], versions: &[u8]) -> Result<Self, Failure> {
        if toolchain.len() > 4096 || versions.len() > 4096 {
            return Err(Failure::message("Compiler lint metadata exceeds 4 KiB"));
        }
        let toolchain =
            std::str::from_utf8(toolchain).map_err(|error| Failure::message(error.to_string()))?;
        let versions =
            std::str::from_utf8(versions).map_err(|error| Failure::message(error.to_string()))?;
        let toolchain: toml::Value =
            toml::from_str(toolchain).map_err(|error| Failure::message(error.to_string()))?;
        let versions: toml::Value =
            toml::from_str(versions).map_err(|error| Failure::message(error.to_string()))?;
        let channel = toolchain
            .get("toolchain")
            .and_then(|value| value.get("channel"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| Failure::message("Missing compiler lint toolchain"))?;
        let version = versions
            .get("dylint")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| Failure::message("Missing Dylint version"))?;
        if !channel.starts_with("nightly-") {
            return Err(Failure::message(
                "Compiler lints require a dated nightly toolchain",
            ));
        }
        semver::Version::parse(version).map_err(|error| Failure::message(error.to_string()))?;
        Ok(Self {
            channel: channel.into(),
            version: version.into(),
        })
    }
}

#[derive(Component)]
struct ToolchainFile;
#[derive(Component)]
struct VersionFile;
#[derive(Component)]
struct Lock;
#[derive(Component)]
struct Directories;
#[derive(Component)]
struct CapacitySlot;
type Requests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Run,
        &'static Checkout,
        &'static mut Settings,
        &'static mut Phase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Io>, Without<Complete>, Without<Failed>),
>;

type Files<'w, 's, R> = Query<
    'w,
    's,
    (
        &'static FileBytes,
        &'static ChildOf,
        &'static Order,
        &'static OperationState,
    ),
    (With<Io>, With<R>),
>;
type Locks<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static AcquiredLock,
        &'static ChildOf,
        &'static Order,
        &'static OperationState,
    ),
    (With<Io>, With<Lock>),
>;

type Capacities<'w, 's> = Query<
    'w,
    's,
    (
        &'static ToolOutput,
        &'static ChildOf,
        &'static Order,
        &'static OperationState,
    ),
    (With<Io>, With<CapacitySlot>),
>;

type DirectoryReceipts<'w, 's> = Query<
    'w,
    's,
    (&'static ChildOf, &'static OperationState),
    (With<Io>, With<Directories>, With<Done>),
>;

#[derive(SystemParam)]
struct Outputs<'w, 's> {
    toolchain: Files<'w, 's, ToolchainFile>,
    versions: Files<'w, 's, VersionFile>,
    locks: Locks<'w, 's>,
    capacities: Capacities<'w, 's>,
    directories: DirectoryReceipts<'w, 's>,
}

impl Outputs<'_, '_> {
    fn bytes<'a, R: Component>(&self, files: &'a Files<R>, owner: Entity) -> Option<&'a FileBytes> {
        let mut latest = None;
        for (value, parent, order, state) in files {
            if parent.parent() != owner || *state != OperationState::Succeeded {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|(_, previous)| order.0 > *previous)
            {
                latest = Some((value, order.0));
            }
        }
        Some(latest?.0)
    }

    fn lock(&self, owner: Entity) -> Option<(Entity, &AcquiredLock)> {
        let mut latest = None;
        for (entity, value, parent, order, state) in &self.locks {
            if parent.parent() != owner || *state != OperationState::Succeeded {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|(_, _, previous)| order.0 > *previous)
            {
                latest = Some((entity, value, order.0));
            }
        }
        let (entity, value, _) = latest?;
        Some((entity, value))
    }

    fn capacity(&self, owner: Entity) -> Option<&ToolOutput> {
        let mut latest = None;
        for (value, parent, order, state) in &self.capacities {
            if parent.parent() != owner || *state != OperationState::Succeeded {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|(_, previous)| order.0 > *previous)
            {
                latest = Some((value, order.0));
            }
        }
        Some(latest?.0)
    }

    fn prepared(&self, owner: Entity) -> bool {
        self.directories
            .iter()
            .any(|(parent, state)| parent.parent() == owner && *state == OperationState::Succeeded)
    }
}

fn read(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, _, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Read {
            continue;
        }
        let library = checkout.root.join("crates/feature/vmux_dx/lints");
        let mut previous = None;
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            Read(library.join("rust-toolchain.toml")),
            ToolchainFile,
            false,
        );
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            Read(library.join("tool-versions.toml")),
            VersionFile,
            false,
        );
        *phase = Phase::Configure;
        wake.0.wake_by_ref();
    }
}

fn configure(
    mut requests: Requests,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, mut settings, mut phase, _) in &mut requests {
        if *phase != Phase::Configure || facts.pending(entity) {
            continue;
        }
        let Some(toolchain) = outputs.bytes(&outputs.toolchain, entity) else {
            continue;
        };
        let Some(versions) = outputs.bytes(&outputs.versions, entity) else {
            continue;
        };
        let configuration = match Configuration::parse(&toolchain.0, &versions.0) {
            Ok(configuration) => configuration,
            Err(error) => {
                io.commands.entity(entity).insert(Failed(error));
                continue;
            }
        };
        for name in [
            "CARGO_BUILD_TARGET",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ] {
            settings.0.remove(name);
        }
        let tools = checkout.tools_root(&settings.0);
        let mut paths = vec![tools.join(".vmux/dylint/bin")];
        let path = match settings.0.required("PATH") {
            Ok(path) => path,
            Err(error) => {
                io.commands
                    .entity(entity)
                    .insert(Failed(Failure::from_error(error)));
                continue;
            }
        };
        paths.extend(std::env::split_paths(path));
        let path = match std::env::join_paths(paths) {
            Ok(path) => path,
            Err(error) => {
                io.commands
                    .entity(entity)
                    .insert(Failed(Failure::message(error.to_string())));
                continue;
            }
        };
        settings.0.set("PATH", path);
        settings
            .0
            .set("CARGO_TARGET_DIR", tools.join(".vmux/lints"));
        settings.0.set("CARGO_INCREMENTAL", "0");
        settings
            .0
            .set("DYLINT_DRIVER_PATH", tools.join(".vmux/dylint-drivers"));
        let policy = match Policy::configured(&settings.0) {
            Ok(policy) => policy,
            Err(error) => {
                io.commands
                    .entity(entity)
                    .insert(Failed(Failure::from_error(error)));
                continue;
            }
        };
        io.commands
            .entity(entity)
            .insert((configuration, Storage::new(policy)));
        *phase = Phase::Prepare;
        wake.0.wake_by_ref();
    }
}

fn prepare(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Prepare {
            continue;
        }
        io.request(
            &mut sequence,
            entity,
            &mut None,
            Prepare {
                root: checkout.tools_root(&settings.0),
            },
            Directories,
            false,
        );
        *phase = Phase::GuardBefore;
        wake.0.wake_by_ref();
    }
}

fn guard(
    mut requests: Requests,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if !matches!(*phase, Phase::GuardBefore | Phase::GuardAfter) || facts.pending(entity) {
            continue;
        }
        if !outputs.prepared(entity) {
            continue;
        }
        let tools = checkout.tools_root(&settings.0);
        io.request(
            &mut sequence,
            entity,
            &mut None,
            CaptureOutput::new(
                &settings.0,
                &checkout.root,
                "df".into(),
                vec!["-Pk".into(), tools.join(".vmux/lints").into_os_string()],
            ),
            CapacitySlot,
            false,
        );
        *phase = if *phase == Phase::GuardBefore {
            Phase::CapacityBefore
        } else {
            Phase::CapacityAfter
        };
        wake.0.wake_by_ref();
    }
}

fn capacity(
    mut requests: Requests,
    facts: Facts,
    outputs: Outputs,
    storage: Query<&Storage>,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, _, _, mut phase, _) in &mut requests {
        if !matches!(*phase, Phase::CapacityBefore | Phase::CapacityAfter) || facts.pending(entity)
        {
            continue;
        }
        let (Some(output), Ok(storage)) = (outputs.capacity(entity), storage.get(entity)) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&output.0.stdout) else {
            io.commands
                .entity(entity)
                .insert(Failed(Failure::message("Invalid filesystem capacity")));
            continue;
        };
        let Some(blocks) = text
            .lines()
            .nth(1)
            .and_then(|line| line.split_whitespace().nth(3))
        else {
            io.commands
                .entity(entity)
                .insert(Failed(Failure::message("Invalid filesystem capacity")));
            continue;
        };
        let free = blocks
            .parse::<u64>()
            .ok()
            .and_then(|blocks| blocks.checked_mul(1024));
        let Some(free) = free else {
            io.commands
                .entity(entity)
                .insert(Failed(Failure::message("Invalid filesystem capacity")));
            continue;
        };
        if let Err(error) = storage.policy().check(free) {
            io.commands
                .entity(entity)
                .insert(Failed(Failure::from_error(error)));
            continue;
        }
        *phase = if *phase == Phase::CapacityBefore {
            Phase::Acquire
        } else {
            Phase::Toolchain
        };
        wake.0.wake_by_ref();
    }
}

fn acquire(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Acquire {
            continue;
        }
        let tools = checkout.tools_root(&settings.0);
        io.request(
            &mut sequence,
            entity,
            &mut None,
            AcquireTarget {
                root: checkout.root.clone(),
                target: tools.join(".vmux/lints"),
            },
            Lock,
            false,
        );
        *phase = Phase::Adopt;
        wake.0.wake_by_ref();
    }
}

fn adopt(
    mut requests: Requests,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, mut settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Adopt || facts.pending(entity) {
            continue;
        }
        let Some((child, lock)) = outputs.lock(entity) else {
            continue;
        };
        let tools = checkout.tools_root(&settings.0);
        io.commands
            .entity(entity)
            .insert(Lease(Some(lock.0.clone())));
        io.commands.entity(child).remove::<AcquiredLock>();
        settings
            .0
            .set("VMUX_TARGET_LOCK_TARGET", tools.join(".vmux/lints"));
        settings
            .0
            .set("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string());
        settings
            .0
            .set("VMUX_BUILD_STORAGE_TARGET", tools.join(".vmux/lints"));
        settings
            .0
            .set("VMUX_BUILD_STORAGE_ROOT", checkout.root.as_os_str());
        io.request(
            &mut sequence,
            entity,
            &mut None,
            Prepare { root: tools },
            Directories,
            false,
        );
        *phase = Phase::GuardAfter;
        wake.0.wake_by_ref();
    }
}

fn toolchain(
    mut requests: Requests,
    configs: Query<&Configuration>,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Toolchain {
            continue;
        }
        let Ok(configuration) = configs.get(entity) else {
            continue;
        };
        let args = [
            "toolchain",
            "install",
            &configuration.channel,
            "--profile",
            "minimal",
            "--component",
            "rustc-dev,llvm-tools-preview,rustfmt",
            "--no-self-update",
        ]
        .map(OsString::from)
        .to_vec();
        io.request(
            &mut sequence,
            entity,
            &mut None,
            RunProcess::new(
                &settings.0,
                &checkout.root,
                settings
                    .0
                    .get("RUSTUP_BIN")
                    .unwrap_or(OsStr::new("rustup"))
                    .to_owned(),
                args,
            ),
            (),
            false,
        );
        *phase = Phase::Install;
        wake.0.wake_by_ref();
    }
}

fn install(
    mut requests: Requests,
    facts: Facts,
    configs: Query<&Configuration>,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Install || facts.pending(entity) {
            continue;
        }
        let Ok(configuration) = configs.get(entity) else {
            continue;
        };
        let tools = checkout.tools_root(&settings.0);
        let args = vec![
            format!("+{}", configuration.channel).into(),
            "install".into(),
            "cargo-dylint".into(),
            "dylint-link".into(),
            "--version".into(),
            configuration.version.clone().into(),
            "--locked".into(),
            "--root".into(),
            tools.join(".vmux/dylint").into_os_string(),
            "--target-dir".into(),
            tools.join(".vmux/dylint-cargo").into_os_string(),
        ];
        io.request(
            &mut sequence,
            entity,
            &mut None,
            RunProcess::new(
                &settings.0,
                &checkout.root,
                settings
                    .0
                    .get("CARGO_BIN")
                    .unwrap_or(OsStr::new("cargo"))
                    .to_owned(),
                args,
            ),
            (),
            false,
        );
        *phase = Phase::Fixtures;
        wake.0.wake_by_ref();
    }
}

fn fixtures(
    mut requests: Requests,
    facts: Facts,
    configs: Query<&Configuration>,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Fixtures || facts.pending(entity) {
            continue;
        }
        let Ok(configuration) = configs.get(entity) else {
            continue;
        };
        if request.fixtures {
            let args = vec![
                format!("+{}", configuration.channel).into(),
                "test".into(),
                "--locked".into(),
                "--lib".into(),
            ];
            io.request(
                &mut sequence,
                entity,
                &mut None,
                RunProcess::new(
                    &settings.0,
                    &checkout.root.join("crates/feature/vmux_dx/lints"),
                    settings
                        .0
                        .get("CARGO_BIN")
                        .unwrap_or(OsStr::new("cargo"))
                        .to_owned(),
                    args,
                ),
                (),
                false,
            );
        }
        *phase = Phase::Check;
        wake.0.wake_by_ref();
    }
}

fn check(
    mut requests: Requests,
    facts: Facts,
    configs: Query<&Configuration>,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Check || facts.pending(entity) {
            continue;
        }
        let Ok(configuration) = configs.get(entity) else {
            continue;
        };
        if let Some(arguments) = &request.arguments {
            let mut args = vec![
                format!("+{}", configuration.channel).into(),
                "dylint".into(),
                "--path".into(),
                checkout
                    .root
                    .join("crates/feature/vmux_dx/lints")
                    .into_os_string(),
                "--".into(),
            ];
            if arguments.is_empty() {
                args.extend(["--locked", "--workspace", "--all-targets"].map(OsString::from));
            } else {
                args.extend(arguments.iter().cloned());
            }
            io.request(
                &mut sequence,
                entity,
                &mut None,
                RunProcess::new(
                    &settings.0,
                    &checkout.root,
                    settings
                        .0
                        .get("CARGO_BIN")
                        .unwrap_or(OsStr::new("cargo"))
                        .to_owned(),
                    args,
                ),
                (),
                false,
            );
        }
        *phase = Phase::Done;
        wake.0.wake_by_ref();
    }
}

fn finish(mut requests: Requests, facts: Facts, mut io: IoWriter) {
    for (entity, _, _, _, phase, _) in &mut requests {
        if *phase == Phase::Done && !facts.pending(entity) {
            io.commands.entity(entity).insert(Complete(Outcome::Done));
        }
    }
}
