use std::ffi::{OsStr, OsString};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::jobs::JobWake;
use vmux_ecs::{Cwd, OperationState, Order};

use crate::Settings;
use crate::debug::{Complete, Facts, Failed, Io, IoWriter, Root, Sequence, Stage};
use crate::debug_io::{
    CaptureOutput, CreateDirectory, DirectoryEntries, HashInput, InspectPath, PathMetadata,
    ProbeOutput, ProbeProcess, ReadDirectory, RemoveDirectory, RenamePath, RunProcess, ToolOutput,
    WriteFile,
};
use crate::execution::Failure;
use crate::repository::Checkout;
use crate::work::Outcome;

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                prepare,
                cache,
                probe_version,
                probe_compiler,
                execute,
                scan,
                versions,
                hashes,
                key,
                inspect,
                record,
                finish,
                restore,
            )
                .chain()
                .in_set(Stage::Decide),
        );
    }
}

#[derive(Component, Clone)]
#[require(crate::debug::Request, Phase)]
pub struct Run {
    pub arguments: Vec<OsString>,
}

impl Run {
    pub fn new(arguments: &[&str]) -> Self {
        let mut values = Vec::new();
        for argument in arguments {
            values.push(OsString::from(argument));
        }
        Self { arguments: values }
    }
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Start,
    Cache,
    VersionProbe,
    CompilerProbe,
    Execute,
    Scan,
    Versions,
    Hashes,
    Key,
    Inspect,
    Record,
    Done,
}

#[derive(Component)]
pub(crate) struct Slot;

#[derive(Component)]
struct ProbeSlot;

#[derive(Component)]
struct CargoVersionSlot;

#[derive(Component)]
struct RustcVersionSlot;

#[derive(Component)]
struct HashesSlot;

#[derive(Component)]
struct KeySlot;

#[derive(Component)]
struct MarkerSlot;

#[derive(Component)]
struct Original(Option<OsString>);

#[derive(Component)]
struct Directory;

#[derive(Component)]
struct Listed;

#[derive(Component)]
struct Scanned;

#[derive(Component)]
struct Input;

#[derive(Component)]
struct Bytes(Vec<u8>);

#[derive(Component)]
#[relationship(relationship_target = DirectoryReads)]
struct ForDirectory(Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = ForDirectory)]
struct DirectoryReads(Vec<Entity>);

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

type Captures<'w, 's> = Query<
    'w,
    's,
    (
        EntityRef<'static>,
        &'static Order,
        &'static ChildOf,
        &'static OperationState,
    ),
    (
        With<Io>,
        With<ToolOutput>,
        With<Slot>,
        Or<(
            With<CargoVersionSlot>,
            With<RustcVersionSlot>,
            With<HashesSlot>,
            With<KeySlot>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct Outputs<'w, 's> {
    captures: Captures<'w, 's>,
}

impl Outputs<'_, '_> {
    fn text<T: Component>(&self, owner: Entity) -> Option<&str> {
        let mut latest = None;
        for (entity, order, parent, state) in &self.captures {
            if parent.parent() != owner
                || *state != OperationState::Succeeded
                || !entity.contains::<T>()
            {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|(_, previous)| order.0 > *previous)
            {
                latest = Some((entity, order.0));
            }
        }
        let (entity, _) = latest?;
        std::str::from_utf8(&entity.get::<ToolOutput>()?.0.stdout).ok()
    }
}

fn prepare(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, mut settings, mut phase, _) in &mut requests {
        if *phase != Phase::Start {
            continue;
        }
        io.commands.entity(entity).insert(Original(
            settings.0.get("CARGO_INCREMENTAL").map(OsStr::to_owned),
        ));
        settings
            .0
            .set("VMUX_TARGET_LOCK_TARGET", checkout.target.as_os_str());
        settings
            .0
            .set("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string());
        settings
            .0
            .set("VMUX_BUILD_STORAGE_TARGET", checkout.target.as_os_str());
        settings
            .0
            .set("VMUX_BUILD_STORAGE_ROOT", checkout.root.as_os_str());
        let executable = match std::env::current_exe() {
            Ok(executable) => executable,
            Err(error) => {
                io.commands
                    .entity(entity)
                    .insert(Failed(Failure::message(error.to_string())));
                continue;
            }
        };
        settings.0.set("VMUX_DX_BIN", executable.as_os_str());
        if let Some(binary) = settings.0.get("TAILWINDCSS_BIN").map(OsStr::to_owned) {
            settings.0.set("TAILWINDCSS", binary);
        }
        *phase = Phase::Cache;
        wake.0.wake_by_ref();
    }
}

fn cache(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, mut settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Cache {
            continue;
        }
        let cache = match settings.0.get("VMUX_CEF_SDK_CACHE") {
            Some(path) if !path.is_empty() => Some(std::path::PathBuf::from(path)),
            _ if settings.0.get("CI") != Some(OsStr::new("true")) => {
                let home = match settings.0.required("HOME") {
                    Ok(home) => std::path::PathBuf::from(home),
                    Err(error) => {
                        io.commands
                            .entity(entity)
                            .insert(Failed(Failure::from_error(error)));
                        continue;
                    }
                };
                Some(if cfg!(target_os = "macos") {
                    home.join("Library/Caches/Vmux/cef-sdk")
                } else {
                    let root = match settings.0.get("XDG_CACHE_HOME") {
                        Some(path) => std::path::PathBuf::from(path),
                        None => home.join(".cache"),
                    };
                    root.join("vmux/cef-sdk")
                })
            }
            _ => None,
        };
        settings.0.remove("CEF_PATH");
        let mut previous = None;
        if let Some(cache) = cache {
            io.request(
                &mut sequence,
                entity,
                &mut previous,
                CreateDirectory(cache.clone()),
                (),
                false,
            );
            settings.0.set("CEF_PATH", cache.as_os_str());
        }
        if settings.0.get("VMUX_DISABLE_SCCACHE") == Some(OsStr::new("1")) {
            *phase = Phase::Execute;
        } else {
            io.request(
                &mut sequence,
                entity,
                &mut previous,
                ProbeProcess {
                    environment: settings.0.clone(),
                    cwd: checkout.root.clone(),
                    program: "sccache".into(),
                    arguments: vec!["--version".into()],
                },
                (Slot, ProbeSlot),
                false,
            );
            *phase = Phase::VersionProbe;
        }
        wake.0.wake_by_ref();
    }
}

fn probe_version(mut requests: Requests, facts: Facts, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::VersionProbe || facts.pending(entity) {
            continue;
        }
        let Some(ProbeOutput(output)) = facts.get::<_, ProbeSlot>(entity) else {
            continue;
        };
        if output.as_ref().is_ok_and(|output| output.status.success()) {
            let compiler = settings
                .0
                .get("RUSTC")
                .unwrap_or(OsStr::new("rustc"))
                .to_owned();
            io.request(
                &mut sequence,
                entity,
                &mut None,
                ProbeProcess {
                    environment: settings.0.clone(),
                    cwd: checkout.root.clone(),
                    program: "sccache".into(),
                    arguments: vec![compiler, "-vV".into()],
                },
                (Slot, ProbeSlot),
                false,
            );
            *phase = Phase::CompilerProbe;
        } else {
            *phase = Phase::Execute;
        }
        wake.0.wake_by_ref();
    }
}

fn probe_compiler(mut requests: Requests, facts: Facts, wake: Res<JobWake>) {
    for (entity, _, _, mut settings, mut phase, _) in &mut requests {
        if *phase != Phase::CompilerProbe || facts.pending(entity) {
            continue;
        }
        let Some(ProbeOutput(output)) = facts.get::<_, ProbeSlot>(entity) else {
            continue;
        };
        if output.as_ref().is_ok_and(|output| output.status.success()) {
            for name in ["CMAKE_C_COMPILER_LAUNCHER", "CMAKE_CXX_COMPILER_LAUNCHER"] {
                if !settings.0.present(name) {
                    settings.0.set(name, "sccache");
                }
            }
            if settings.0.get("CI") != Some(OsStr::new("true"))
                && !settings.0.present("RUSTC_WRAPPER")
            {
                settings.0.set("RUSTC_WRAPPER", "sccache");
            }
        }
        *phase = Phase::Execute;
        wake.0.wake_by_ref();
    }
}

fn execute(mut requests: Requests, facts: Facts, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, request, checkout, mut settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Execute || facts.pending(entity) {
            continue;
        }
        let incremental = settings
            .0
            .incremental(request.arguments.first().map(OsString::as_os_str))
            .to_owned();
        settings.0.set("CARGO_INCREMENTAL", incremental);
        let program = settings
            .0
            .get("CARGO_BIN")
            .unwrap_or(OsStr::new("cargo"))
            .to_owned();
        io.request(
            &mut sequence,
            entity,
            &mut None,
            RunProcess::new(
                &settings.0,
                &checkout.root,
                program,
                request.arguments.clone(),
            ),
            (),
            false,
        );
        if matches!(
            request
                .arguments
                .first()
                .and_then(|argument| argument.to_str()),
            Some("bench" | "build" | "check" | "clippy" | "doc" | "rustc" | "test")
        ) && settings.0.get("CI") != Some(OsStr::new("true"))
        {
            io.commands.spawn((
                Name::new(checkout.root.display().to_string()),
                Cwd(checkout.root.clone()),
                Directory,
                ChildOf(entity),
            ));
            *phase = Phase::Scan;
        } else {
            *phase = Phase::Done;
        }
        wake.0.wake_by_ref();
    }
}

type Directories<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Cwd,
        &'static ChildOf,
        Has<Listed>,
        Option<&'static DirectoryReads>,
    ),
    (With<Directory>, Without<Scanned>),
>;

type Readbacks<'w, 's> = Query<
    'w,
    's,
    (
        &'static DirectoryEntries,
        &'static ForDirectory,
        &'static OperationState,
    ),
    With<Slot>,
>;

fn scan(
    mut requests: Requests,
    directories: Directories,
    readbacks: Readbacks,
    facts: Facts,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, _, _, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Scan || facts.pending(entity) {
            continue;
        }
        let mut remaining = false;
        let mut previous = None;
        for (directory, path, parent, listed, receipts) in &directories {
            if parent.parent() != entity {
                continue;
            }
            remaining = true;
            if !listed {
                io.request(
                    &mut sequence,
                    entity,
                    &mut previous,
                    ReadDirectory(path.0.clone()),
                    (Slot, ForDirectory(directory)),
                    false,
                );
                io.commands.entity(directory).insert(Listed);
                wake.0.wake_by_ref();
                continue;
            }
            let Some(receipts) = receipts else {
                continue;
            };
            let mut entries = None;
            for receipt in &receipts.0 {
                let Ok((data, owner, state)) = readbacks.get(*receipt) else {
                    continue;
                };
                if owner.0 == directory && *state == OperationState::Succeeded {
                    entries = Some(&data.0);
                    break;
                }
            }
            let Some(entries) = entries else {
                continue;
            };
            for (path, metadata) in entries {
                if matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(".git" | ".worktrees" | ".build-tools" | "target")
                ) {
                    continue;
                }
                if metadata.is_dir() {
                    io.commands.spawn((
                        Name::new(path.display().to_string()),
                        Cwd(path.clone()),
                        Directory,
                        ChildOf(entity),
                    ));
                } else if metadata.is_file()
                    && (matches!(
                        path.file_name().and_then(|name| name.to_str()),
                        Some(
                            "Cargo.toml" | "Cargo.lock" | "rust-toolchain" | "rust-toolchain.toml"
                        )
                    ) || path
                        .parent()
                        .is_some_and(|parent| parent.file_name() == Some(OsStr::new(".cargo")))
                        && matches!(
                            path.file_name().and_then(|name| name.to_str()),
                            Some("config" | "config.toml")
                        ))
                {
                    io.commands.spawn((
                        Name::new(path.display().to_string()),
                        Cwd(path.clone()),
                        Input,
                        ChildOf(entity),
                    ));
                }
            }
            io.commands.entity(directory).insert(Scanned);
            wake.0.wake_by_ref();
        }
        if !remaining {
            *phase = Phase::Versions;
            wake.0.wake_by_ref();
        }
    }
}

fn versions(mut requests: Requests, mut io: IoWriter, wake: Res<JobWake>) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Versions {
            continue;
        }
        let mut previous = None;
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            CaptureOutput::new(
                &settings.0,
                &checkout.root,
                settings
                    .0
                    .get("CARGO_BIN")
                    .unwrap_or(OsStr::new("cargo"))
                    .to_owned(),
                vec!["-V".into()],
            ),
            (Slot, CargoVersionSlot),
            false,
        );
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            CaptureOutput::new(
                &settings.0,
                &checkout.root,
                settings
                    .0
                    .get("RUSTC")
                    .unwrap_or(OsStr::new("rustc"))
                    .to_owned(),
                vec!["-vV".into()],
            ),
            (Slot, RustcVersionSlot),
            false,
        );
        *phase = Phase::Hashes;
        wake.0.wake_by_ref();
    }
}

fn hashes(
    mut requests: Requests,
    files: Query<(&Cwd, &ChildOf), With<Input>>,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Hashes || facts.pending(entity) {
            continue;
        }
        let Some(cargo) = outputs.text::<CargoVersionSlot>(entity) else {
            continue;
        };
        let Some(rustc) = outputs.text::<RustcVersionSlot>(entity) else {
            continue;
        };
        let mut bytes = format!("cargo={}\nrustc={}\n", cargo.trim_end(), rustc.trim_end());
        if settings.0.get("VMUX_DX_PLATFORM") == Some(OsStr::new("web")) {
            bytes.push_str("vmux-dx-platform=web\n");
        }
        for (label, name) in [
            ("rustc-wrapper", "RUSTC_WRAPPER"),
            ("rustc-workspace-wrapper", "RUSTC_WORKSPACE_WRAPPER"),
            ("cargo-incremental", "CARGO_INCREMENTAL"),
            ("rustflags", "RUSTFLAGS"),
            ("cargo-encoded-rustflags", "CARGO_ENCODED_RUSTFLAGS"),
            ("cargo-build-target", "CARGO_BUILD_TARGET"),
            ("macosx-deployment-target", "MACOSX_DEPLOYMENT_TARGET"),
        ] {
            bytes.push_str(&format!(
                "{label}={}\n",
                settings
                    .0
                    .get(name)
                    .unwrap_or(OsStr::new(""))
                    .to_string_lossy()
            ));
        }
        let mut paths = Vec::new();
        for (path, parent) in &files {
            if parent.parent() == entity {
                paths.push(path.0.clone());
            }
        }
        paths.sort();
        let mut arguments = vec![OsString::from("hash-object")];
        for path in paths {
            let relative = path
                .strip_prefix(&checkout.root)
                .expect("scanned checkout input");
            bytes.push_str(&format!("file={}\n", relative.display()));
            arguments.push(path.into_os_string());
        }
        if arguments.len() == 1 {
            io.request(
                &mut sequence,
                entity,
                &mut None,
                HashInput {
                    environment: settings.0.clone(),
                    cwd: checkout.root.clone(),
                    bytes: bytes.into_bytes(),
                },
                (Slot, KeySlot),
                false,
            );
            *phase = Phase::Inspect;
        } else {
            io.commands.entity(entity).insert(Bytes(bytes.into_bytes()));
            io.request(
                &mut sequence,
                entity,
                &mut None,
                CaptureOutput::new(&settings.0, &checkout.root, "git".into(), arguments),
                (Slot, HashesSlot),
                false,
            );
            *phase = Phase::Key;
        }
        wake.0.wake_by_ref();
    }
}

fn key(
    mut requests: Requests,
    inputs: Query<&Bytes>,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, settings, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Key || facts.pending(entity) {
            continue;
        }
        let Some(hashes) = outputs.text::<HashesSlot>(entity) else {
            continue;
        };
        let Ok(input) = inputs.get(entity) else {
            continue;
        };
        let mut bytes = input.0.clone();
        bytes.extend_from_slice(hashes.as_bytes());
        io.request(
            &mut sequence,
            entity,
            &mut None,
            HashInput {
                environment: settings.0.clone(),
                cwd: checkout.root.clone(),
                bytes,
            },
            (Slot, KeySlot),
            false,
        );
        *phase = Phase::Inspect;
        wake.0.wake_by_ref();
    }
}

fn inspect(
    mut requests: Requests,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, _, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Inspect || facts.pending(entity) {
            continue;
        }
        let Some(key) = outputs.text::<KeySlot>(entity) else {
            continue;
        };
        let key = key.trim();
        if !matches!(key.len(), 40 | 64) || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            io.commands
                .entity(entity)
                .insert(Failed(Failure::message("Git returned an invalid seed key")));
            continue;
        }
        io.request(
            &mut sequence,
            entity,
            &mut None,
            InspectPath(checkout.target.join(".vmux-seed")),
            (Slot, MarkerSlot),
            false,
        );
        *phase = Phase::Record;
        wake.0.wake_by_ref();
    }
}

fn record(
    mut requests: Requests,
    facts: Facts,
    outputs: Outputs,
    mut io: IoWriter,
    wake: Res<JobWake>,
) {
    for (entity, _, checkout, _, mut phase, mut sequence) in &mut requests {
        if *phase != Phase::Record || facts.pending(entity) {
            continue;
        }
        let Some(PathMetadata(metadata)) = facts.get::<_, MarkerSlot>(entity) else {
            continue;
        };
        if metadata.as_ref().is_some_and(|metadata| !metadata.is_dir()) {
            io.commands
                .entity(entity)
                .insert(Failed(Failure::message("Seed marker must be a directory")));
            continue;
        }
        let Some(key) = outputs.text::<KeySlot>(entity) else {
            continue;
        };
        let marker = checkout.target.join(".vmux-seed");
        let staging = marker.join(format!(".pending-{}", entity.to_bits()));
        let mut previous = None;
        if metadata.is_some() {
            io.request(
                &mut sequence,
                entity,
                &mut previous,
                RemoveDirectory(marker.clone()),
                (),
                false,
            );
        }
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            CreateDirectory(marker.clone()),
            (),
            false,
        );
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            WriteFile(staging.clone(), Vec::new()),
            (),
            false,
        );
        io.request(
            &mut sequence,
            entity,
            &mut previous,
            RenamePath(staging, marker.join(key.trim())),
            (),
            false,
        );
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

type Restoring<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Original, &'static mut Settings),
    (With<Run>, Or<(With<Complete>, With<Failed>)>),
>;

fn restore(mut requests: Restoring, mut commands: Commands) {
    for (entity, original, mut settings) in &mut requests {
        match &original.0 {
            Some(value) => settings.0.set("CARGO_INCREMENTAL", value.clone()),
            None => settings.0.remove("CARGO_INCREMENTAL"),
        }
        commands.entity(entity).remove::<Original>();
    }
}
