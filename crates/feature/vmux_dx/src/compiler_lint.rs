use std::ffi::{OsStr, OsString};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::jobs::JobWake;
use vmux_ecs::{OperationState, Order};

use crate::Settings;
use crate::compiler_lint_io::{Prepare, Read};
use crate::debug::{Complete, Facts, Failed, Io, IoWriter, Lease, Root, Sequence, Stage};
use crate::debug_io::{AcquiredLock, FileBytes, RunProcess};
use crate::execution::Failure;
use crate::policy::Policy;
use crate::repository::Checkout;
use crate::work::Outcome;

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                read, configure, acquire, adopt, toolchain, install, fixtures, check, finish,
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
    Acquire,
    Adopt,
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

#[derive(SystemParam)]
struct Outputs<'w, 's> {
    toolchain: Files<'w, 's, ToolchainFile>,
    versions: Files<'w, 's, VersionFile>,
    locks: Locks<'w, 's>,
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
        let mut paths = vec![checkout.root.join(".build-tools/dylint/bin")];
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
            .set("CARGO_TARGET_DIR", checkout.root.join(".build-tools/lints"));
        settings.0.set("CARGO_INCREMENTAL", "0");
        settings.0.set(
            "DYLINT_DRIVER_PATH",
            checkout.root.join(".build-tools/dylint-drivers"),
        );
        io.commands.entity(entity).insert(configuration);
        *phase = Phase::Acquire;
        wake.0.wake_by_ref();
    }
}

fn acquire(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Acquire {
            continue;
        }
        let policy = match Policy::configured(&settings.0) {
            Ok(policy) => policy,
            Err(error) => {
                io.commands
                    .entity(entity)
                    .insert(Failed(Failure::from_error(error)));
                continue;
            }
        };
        io.request(
            &mut sequence,
            entity,
            &mut None,
            Prepare {
                root: checkout.root.clone(),
                policy,
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
    for (entity, _, checkout, mut settings, mut phase, _) in &mut requests {
        if *phase != Phase::Adopt || facts.pending(entity) {
            continue;
        }
        let Some((child, lock)) = outputs.lock(entity) else {
            continue;
        };
        io.commands
            .entity(entity)
            .insert(Lease(Some(lock.0.clone())));
        io.commands.entity(child).remove::<AcquiredLock>();
        settings.0.set(
            "VMUX_TARGET_LOCK_TARGET",
            checkout.root.join(".build-tools/lints"),
        );
        settings
            .0
            .set("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string());
        settings.0.set(
            "VMUX_BUILD_STORAGE_TARGET",
            checkout.root.join(".build-tools/lints"),
        );
        settings
            .0
            .set("VMUX_BUILD_STORAGE_ROOT", checkout.root.as_os_str());
        *phase = Phase::Toolchain;
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
        let args = vec![
            format!("+{}", configuration.channel).into(),
            "install".into(),
            "cargo-dylint".into(),
            "dylint-link".into(),
            "--version".into(),
            configuration.version.clone().into(),
            "--locked".into(),
            "--root".into(),
            checkout.root.join(".build-tools/dylint").into_os_string(),
            "--target-dir".into(),
            checkout
                .root
                .join(".build-tools/dylint-cargo")
                .into_os_string(),
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
