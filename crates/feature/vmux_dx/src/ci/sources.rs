use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::Settings;
use crate::ci_io::fs::{
    CapturedSources, DirectoryFact, DirectoryProbe, OptionalBytes, OptionalRead, Print, Publish,
    ReadFile, RemoveDirectory, RemoveFile, SetTimes, SourceFacts, Written,
};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use vmux_ecs::cli::CliInvocation;
#[derive(bevy_ecs :: system :: SystemParam)]
struct Results<'w, 's> {
    validation: super::Reads<'w, 's, DirectoryFact, ValidationSlot>,
    timestamps: super::Reads<'w, 's, Written, TimestampSlot>,
    facts_slot_captured_sources: super::Reads<'w, 's, CapturedSources, FactsSlot>,
    workspace_manifest_slot_bytes:
        super::Reads<'w, 's, crate::ci_io::fs::Bytes, WorkspaceManifestSlot>,
    configuration_slot_written: super::Reads<'w, 's, Written, ConfigurationSlot>,
    print_slot_written: super::Reads<'w, 's, Written, PrintSlot>,
    snapshot_slot_optional_bytes: super::Reads<'w, 's, OptionalBytes, SnapshotSlot>,
    receipt_slot_written: super::Reads<'w, 's, Written, ReceiptSlot>,
    checkpoint_configuration_slot_bytes:
        super::Reads<'w, 's, crate::ci_io::fs::Bytes, CheckpointConfigurationSlot>,
    checkpoint_receipt_slot_optional_bytes:
        super::Reads<'w, 's, OptionalBytes, CheckpointReceiptSlot>,
    checkpoint_write_slot_written: super::Reads<'w, 's, Written, CheckpointWriteSlot>,
    checkpoint_remove_slot_written: super::Reads<'w, 's, Written, CheckpointRemoveSlot>,
    listed_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, ListedSlot>,
    metadata_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, MetadataSlot>,
    compiler_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, CompilerSlot>,
}
pub(super) struct CiSourcesPlugin;
impl bevy_app::Plugin for CiSourcesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                inputs_start,
                inputs_capture,
                inputs_encode,
                inputs_finish,
                restore_start,
                restore_capture,
                restore_apply,
                restore_record,
                restore_finish,
                checkpoint_start,
                checkpoint_capture,
                checkpoint_write,
                checkpoint_finish,
                validate,
                validated,
            )
                .chain()
                .in_set(Stage::Step),
        );
    }
}
#[derive(Serialize, Deserialize, Clone)]
pub(super) struct Configuration {
    pub(super) artifacts: Vec<String>,
    prefix: String,
    restore_prefix: String,
    key: String,
    pub(super) paths: String,
    pub(super) snapshot: String,
    pub(super) target: String,
    receipt: String,
    workspace: String,
    save: bool,
}
impl Configuration {
    fn direct(settings: &Settings, workspace: &str, snapshot: &str) -> Self {
        let target = settings
            .0
            .get("INPUT_TARGET")
            .and_then(OsStr::to_str)
            .unwrap_or("target")
            .to_owned();
        let receipt = settings
            .0
            .get("INPUT_RECEIPT")
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{snapshot}.restore"));
        Self {
            artifacts: Vec::new(),
            prefix: String::new(),
            restore_prefix: String::new(),
            key: String::new(),
            paths: String::new(),
            snapshot: snapshot.into(),
            target,
            receipt,
            workspace: workspace.into(),
            save: settings.0.get("INPUT_SAVE") == Some(OsStr::new("true")),
        }
    }
    fn output(&self) -> String {
        format!(
            "prefix={}\nrestore-prefix={}\nkey={}\npaths<<VMUX_CACHE_PATHS\n{}\nVMUX_CACHE_PATHS\nsnapshot={}\ntarget={}\nreceipt={}\nworkspace={}\nsave={}\n",
            self.prefix,
            self.restore_prefix,
            self.key,
            self.paths,
            self.snapshot,
            self.target,
            self.receipt,
            self.workspace,
            self.save
        )
    }
}
#[derive(Serialize, Deserialize, Clone)]
struct FileStamp {
    hash: String,
    mode: u32,
    mtime: String,
}
#[derive(Serialize, Deserialize, Clone)]
struct DirectoryStamp {
    hash: Option<String>,
    mtime: String,
}
#[derive(Serialize, Deserialize, Clone)]
struct Snapshot {
    version: u8,
    files: BTreeMap<String, FileStamp>,
    #[serde(default)]
    directories: BTreeMap<String, DirectoryStamp>,
}
impl Snapshot {
    fn now() -> i128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i128
    }
    fn compiled(facts: &CapturedSources) -> i128 {
        let mut compiled = 0;
        for entry in &facts.entries {
            if !entry.directory
                && entry
                    .name
                    .file_name()
                    .is_some_and(|name| name == "invoked.timestamp")
            {
                compiled = compiled.max(entry.modified);
            }
        }
        compiled
    }
    fn empty() -> Self {
        Self {
            version: 2,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
        }
    }
    fn read(bytes: Option<&Vec<u8>>) -> Self {
        let Some(bytes) = bytes else {
            return Self::empty();
        };
        match serde_json::from_slice::<Self>(bytes) {
            Ok(value) if value.version == 2 => value,
            _ => Self::empty(),
        }
    }
    fn capture(facts: &CapturedSources) -> Self {
        let mut snapshot = Self::empty();
        let mut directory_hashes: BTreeMap<String, Sha256> = BTreeMap::new();
        for file in &facts.files {
            snapshot.files.insert(
                file.name.clone(),
                FileStamp {
                    hash: file.hash.clone(),
                    mode: file.mode,
                    mtime: file.modified.to_string(),
                },
            );
            let mut parent = std::path::Path::new(&file.name).parent();
            while let Some(path) = parent {
                if path.as_os_str().is_empty() {
                    break;
                }
                let name = path.to_str().unwrap().to_owned();
                directory_hashes
                    .entry(name)
                    .or_default()
                    .update(format!("{}\0{}\0{}\0", file.name, file.hash, file.mode));
                parent = path.parent();
            }
        }
        for (name, directory) in &facts.directories {
            let complete = directory.children.iter().all(|child| {
                child.file && snapshot.files.contains_key(&child.name)
                    || child.directory && directory_hashes.contains_key(&child.name)
            });
            let hash = if complete {
                directory_hashes
                    .get(name)
                    .map(|hash| format!("{:x}", hash.clone().finalize()))
            } else {
                None
            };
            snapshot.directories.insert(
                name.clone(),
                DirectoryStamp {
                    hash,
                    mtime: directory.modified.to_string(),
                },
            );
        }
        snapshot
    }
    fn time(value: &str) -> Option<i128> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        value.parse().ok()
    }
    fn publication(&mut self, receipt: &Self, facts: &CapturedSources) {
        let changed = Snapshot::now()
            .max(Snapshot::compiled(facts) + 1_000_000_000)
            .to_string();
        for (name, file) in &mut self.files {
            file.mtime = match receipt.files.get(name) {
                Some(previous) if previous.hash == file.hash && previous.mode == file.mode => {
                    previous.mtime.clone()
                }
                _ => changed.clone(),
            };
        }
        for (name, directory) in &mut self.directories {
            directory.mtime = match receipt.directories.get(name) {
                Some(previous) if directory.hash.is_some() && previous.hash == directory.hash => {
                    previous.mtime.clone()
                }
                _ => changed.clone(),
            };
        }
    }
}
#[derive(Component)]
#[require(InputPhase)]
struct Inputs {
    partition: String,
    workspace: String,
    target: PathBuf,
    scope: Context,
}
impl Inputs {
    fn manifests(metadata: &Value, context: &Context) -> Result<Vec<PathBuf>, String> {
        let members = metadata["workspace_members"]
            .as_array()
            .ok_or("Missing Cargo workspace members")?;
        if members.is_empty() {
            return Err("No workspace packages to cache".into());
        }
        let packages = metadata["packages"]
            .as_array()
            .ok_or("Missing Cargo workspace packages")?;
        let mut paths = Vec::new();
        for package in packages {
            if !members.contains(&package["id"]) {
                continue;
            }
            let path = PathBuf::from(
                package["manifest_path"]
                    .as_str()
                    .ok_or("Missing Cargo manifest path")?,
            );
            let relative = path
                .strip_prefix(&context.root)
                .map_err(|_| "Cargo manifest escapes source cache workspace")?;
            context.location(relative).resolve()?;
            paths.push(relative.into());
        }
        let workspace = PathBuf::from(
            metadata["workspace_root"]
                .as_str()
                .ok_or("Missing Cargo workspace root")?,
        );
        let relative = workspace
            .strip_prefix(&context.root)
            .map_err(|_| "Cargo workspace escapes checkout")?;
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            ".cargo/config",
            ".cargo/config.toml",
        ] {
            paths.push(relative.join(name));
        }
        Ok(paths)
    }
    fn profiles(bytes: &[u8]) -> Result<BTreeSet<String>, String> {
        let source = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let manifest: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        let mut profiles = BTreeSet::from(["debug".into(), "release".into()]);
        if let Some(table) = manifest.get("profile").and_then(toml::Value::as_table) {
            for name in table.keys() {
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
                {
                    return Err("Invalid Cargo profile name".into());
                }
                profiles.insert(
                    match name.as_str() {
                        "dev" | "test" => "debug",
                        "bench" => "release",
                        name => name,
                    }
                    .into(),
                );
            }
        }
        Ok(profiles)
    }
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum InputPhase {
    #[default]
    Probes,
    Facts,
    Publish,
    Printed,
}
#[derive(Component)]
struct ListedSlot;
#[derive(Component)]
struct MetadataSlot;
#[derive(Component)]
struct CompilerSlot;
#[derive(Component)]
struct WorkspaceManifestSlot;
#[derive(Component)]
struct FactsSlot;
#[derive(Component)]
struct ConfigurationSlot;
#[derive(Component)]
struct PrintSlot;
impl Context {
    fn scoped(&self, workspace: &str) -> Result<Self, String> {
        if workspace == "." {
            return Ok(self.clone());
        }
        let path = self.location(workspace).resolve()?;
        if !path.is_dir() {
            return Err("CI workspace must be a directory".into());
        }
        Ok(Self { root: path })
    }
    fn paths(text: &str) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for name in text.split('\0') {
            if !name.is_empty() {
                paths.push(PathBuf::from(name));
            }
        }
        paths
    }
}
fn inputs_start(mut requests: InputsStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cache-inputs") {
            continue;
        }
        let partition = match Arguments::text(&settings.0, "VMUX_CACHE_PARTITION") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if !partition
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            io.fail(entity, "Invalid CI cache partition");
            continue;
        }
        let workspace = settings
            .0
            .get("VMUX_CACHE_WORKSPACE")
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(".")
            .to_owned();
        let manifest_scope = match context.scoped(&workspace) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let scope = context.clone();
        let target = settings
            .0
            .get("VMUX_CACHE_TARGET")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target"));
        if let Err(error) = scope.location(target.clone()).resolve() {
            io.fail(entity, error);
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: manifest_scope.location("Cargo.toml"),
                limit: 1024 * 1024,
            },
            WorkspaceManifestSlot,
        );
        io.request(
            entity,
            &mut sequence,
            scope.git(&settings.0, &["ls-files", "-z"]),
            ListedSlot,
        );
        io.request(
            entity,
            &mut sequence,
            scope.process(
                &settings.0,
                "cargo",
                &[
                    "metadata",
                    "--locked",
                    "--no-deps",
                    "--format-version=1",
                    "--manifest-path",
                    manifest_scope.root.join("Cargo.toml").to_str().unwrap(),
                ],
            ),
            MetadataSlot,
        );
        io.request(
            entity,
            &mut sequence,
            scope.process(&settings.0, "rustc", &["-vV"]),
            CompilerSlot,
        );
        io.commands.entity(entity).insert((
            Inputs {
                partition,
                workspace,
                target,
                scope,
            },
            InputPhase::Probes,
        ));
    }
}
fn inputs_capture(mut requests: InputsCaptureRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, inputs, mut phase, mut sequence) in &mut requests {
        if *phase != InputPhase::Probes {
            continue;
        }
        let Some(listed) = results
            .listed_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let mut paths = match listed {
            Ok(value) => Context::paths(value),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let Some(metadata) = results
            .metadata_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let metadata: Value = match metadata
            .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let extra = match Inputs::manifests(&metadata, &inputs.scope) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        paths.extend(extra);
        paths.sort();
        paths.dedup();
        io.request(
            entity,
            &mut sequence,
            SourceFacts {
                root: inputs.scope.root.clone(),
                paths,
                target: Some(inputs.target.clone()),
            },
            FactsSlot,
        );
        *phase = InputPhase::Facts;
    }
}
impl Inputs {
    fn digest(
        &self,
        settings: &Settings,
        facts: &CapturedSources,
        compiler: &str,
    ) -> Result<(String, String), String> {
        let mut hash = Sha256::new();
        hash.update(compiler);
        hash.update(format!(
            "workspace={}\0target={}\0",
            self.workspace,
            self.target.display()
        ));
        for name in settings.0.names() {
            let Some(key) = name.to_str() else {
                continue;
            };
            let Some(value) = settings.0.get(key).and_then(OsStr::to_str) else {
                continue;
            };
            if key == "RUSTFLAGS"
                || key == "CARGO_ENCODED_RUSTFLAGS"
                || key.starts_with("CARGO_BUILD_")
                || key.starts_with("CARGO_TARGET_")
                || key.starts_with("CARGO_PROFILE_")
                || key.starts_with("RUSTC")
                || matches!(
                    key,
                    "VMUX_CI_VALIDATION"
                        | "VMUX_BUILD_PROFILE"
                        | "VMUX_HOST_NAME"
                        | "CC"
                        | "CXX"
                        | "CFLAGS"
                        | "CXXFLAGS"
                        | "AR"
                )
            {
                hash.update(format!("{key}={value}\0"));
            }
        }
        let mut compatibility = hash.clone();
        for file in &facts.files {
            if file.name.ends_with("Cargo.toml")
                || file.name.ends_with("Cargo.lock")
                || file.name.ends_with(".cargo/config")
                || file.name.ends_with(".cargo/config.toml")
                || matches!(
                    file.name.as_str(),
                    "rust-toolchain.toml" | "tool-versions.env"
                )
            {
                hash.update(format!("{}\0{}\0{}\0", file.name, file.mode, file.hash));
                if !file.name.ends_with("Cargo.toml") && !file.name.ends_with("Cargo.lock") {
                    compatibility.update(format!("{}\0{}\0{}\0", file.name, file.mode, file.hash));
                }
            }
        }
        let os = Arguments::text(&settings.0, "RUNNER_OS")?;
        let arch = Arguments::text(&settings.0, "RUNNER_ARCH")?;
        let restore_prefix = format!(
            "vmux-source-v4-{os}-{arch}-{}-{:x}",
            self.partition,
            compatibility.finalize()
        );
        Ok((
            format!("{restore_prefix}-{:x}", hash.finalize()),
            restore_prefix,
        ))
    }
    fn artifacts(
        &self,
        metadata: &Value,
        profiles: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, String> {
        let members = metadata["workspace_members"]
            .as_array()
            .ok_or("Missing Cargo workspace members")?;
        let packages = metadata["packages"]
            .as_array()
            .ok_or("Missing Cargo workspace packages")?;
        let target = self
            .target
            .to_str()
            .ok_or("Cargo cache target must be UTF-8")?;
        let mut paths = BTreeSet::new();
        let mut count = 0;
        for package in packages {
            if !members.contains(&package["id"]) {
                continue;
            }
            let name = package["name"]
                .as_str()
                .ok_or("Missing Cargo package name")?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            {
                return Err("Invalid workspace cache package".into());
            }
            count += 1;
            for profile in profiles {
                for output in [
                    format!("{target}/{profile}"),
                    format!("{target}/*/{profile}"),
                ] {
                    paths.insert(format!("{output}/.fingerprint/{name}-*"));
                    paths.insert(format!("{output}/build/{name}-*"));
                    Self::targets(package, &output, &mut paths)?;
                }
            }
        }
        if count == 0 {
            return Err("No workspace packages to cache".into());
        }
        for (partition, consumer) in [
            ("sdk-framework", "framework-consumer"),
            ("sdk-host", "host-consumer"),
        ] {
            if self.partition == partition {
                paths.insert(format!("{target}/{consumer}/**"));
            }
        }
        paths.insert(format!("!{target}/**/incremental"));
        paths.insert(format!("!{target}/**/incremental/**"));
        paths.insert(format!("!{target}/**/.cargo-lock"));
        for extension in ["app", "dmg", "zip", "tar", "tar.*"] {
            paths.insert(format!("!{target}/**/*.{extension}"));
            paths.insert(format!("!{target}/**/*.{extension}/**"));
        }
        Ok(paths)
    }
    fn targets(package: &Value, output: &str, paths: &mut BTreeSet<String>) -> Result<(), String> {
        let targets = package["targets"]
            .as_array()
            .ok_or("Missing Cargo package targets")?;
        for target in targets {
            let name = target["name"].as_str().ok_or("Missing Cargo target name")?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            {
                return Err("Invalid Cargo target name".into());
            }
            let stem = name.replace('-', "_");
            for path in [
                format!("{output}/deps/{stem}-*"),
                format!("{output}/deps/lib{stem}-*"),
                format!("{output}/{name}*"),
                format!("{output}/lib{stem}*"),
            ] {
                paths.insert(path);
            }
            if target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("example")))
            {
                paths.insert(format!("{output}/examples/{stem}*"));
                paths.insert(format!("{output}/examples/lib{stem}*"));
            }
        }
        Ok(())
    }
    fn configuration(
        &self,
        settings: &Settings,
        prefix: String,
        restore_prefix: String,
        metadata: &Value,
        manifest: &[u8],
    ) -> Result<Configuration, String> {
        let sha = Arguments::text(&settings.0, "GITHUB_SHA")?;
        if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Invalid cache Git identity".into());
        }
        let profiles = Self::profiles(manifest)?;
        let paths = self.artifacts(metadata, &profiles)?;
        let snapshot = format!(".build-tools/source-cache-{}.json", self.partition);
        Ok(Configuration {
            key: format!("{prefix}-{sha}"),
            prefix,
            restore_prefix,
            artifacts: Self::ordered(paths).lines().map(str::to_owned).collect(),
            paths: format!(".build-tools/workspace-cache-{}", self.partition),
            receipt: format!("{snapshot}.{}.restore", Snapshot::now()),
            snapshot,
            target: self
                .target
                .to_str()
                .ok_or("Cargo target must be UTF-8")?
                .into(),
            workspace: self.workspace.clone(),
            save: settings.0.get("VMUX_CACHE_SAVE") == Some(OsStr::new("true")),
        })
    }
}
fn inputs_encode(mut requests: InputsEncodeRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, inputs, settings, mut phase, mut sequence) in &mut requests {
        if *phase != InputPhase::Facts {
            continue;
        }
        let Some(facts) = results.facts_slot_captured_sources.get(entity) else {
            continue;
        };
        let Some(metadata) = results.metadata_slot_captured.get(entity) else {
            continue;
        };
        let Some(manifest) = results.workspace_manifest_slot_bytes.get(entity) else {
            continue;
        };
        let metadata: Value = match metadata
            .text()
            .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        {
            Ok(metadata) => metadata,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let Some(compiler) = results
            .compiler_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let (prefix, restore_prefix) =
            match compiler.and_then(|compiler| inputs.digest(settings, facts, compiler)) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
        let configuration =
            match inputs.configuration(settings, prefix, restore_prefix, &metadata, &manifest.0) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
        let output = configuration.output().into_bytes();
        let bytes = match serde_json::to_vec(&configuration) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: inputs
                    .scope
                    .location(format!("{}.inputs", configuration.snapshot)),
                bytes,
                executable: false,
            },
            ConfigurationSlot,
        );
        io.emit(entity, &mut sequence, Print(output), PrintSlot);
        *phase = InputPhase::Publish;
    }
}
fn inputs_finish(mut requests: InputsFinishRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, mut phase) in &mut requests {
        if *phase == InputPhase::Publish
            && results.configuration_slot_written.get(entity).is_some()
            && results.print_slot_written.get(entity).is_some()
        {
            *phase = InputPhase::Printed;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
#[require(RestorePhase)]
struct Restore {
    scope: Context,
    snapshot: String,
    target: PathBuf,
    receipt: String,
    save: bool,
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum RestorePhase {
    #[default]
    Listed,
    Facts,
    Applying,
    Recording,
    Done,
}
#[derive(Component)]
struct SnapshotSlot;
#[derive(Component)]
struct TimestampSlot;
#[derive(Component)]
struct ReceiptSlot;
fn restore_start(mut requests: RestoreStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cache-restore") {
            continue;
        }
        let workspace = settings
            .0
            .get("INPUT_WORKSPACE")
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(".");
        let scope = match context.scoped(workspace) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let snapshot = match Arguments::text(&settings.0, "INPUT_SNAPSHOT") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let target = match Arguments::text(&settings.0, "INPUT_TARGET") {
            Ok(value) => PathBuf::from(value),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let receipt = settings
            .0
            .get("INPUT_RECEIPT")
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{snapshot}.restore"));
        let save = settings.0.get("INPUT_SAVE") == Some(OsStr::new("true"));
        io.request(
            entity,
            &mut sequence,
            scope.git(&settings.0, &["ls-files", "-z"]),
            ListedSlot,
        );
        io.request(
            entity,
            &mut sequence,
            OptionalRead {
                location: scope.location(&snapshot),
                limit: 8 * 1024 * 1024,
            },
            SnapshotSlot,
        );
        io.commands.entity(entity).insert((
            Restore {
                scope,
                snapshot,
                target,
                receipt,
                save,
            },
            RestorePhase::Listed,
        ));
    }
}
fn restore_capture(mut requests: RestoreCaptureRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, restore, mut phase, mut sequence) in &mut requests {
        if *phase != RestorePhase::Listed {
            continue;
        }
        let Some(listed) = results
            .listed_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let paths = match listed {
            Ok(value) => Context::paths(value),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        io.request(
            entity,
            &mut sequence,
            SourceFacts {
                root: restore.scope.root.clone(),
                paths,
                target: Some(restore.target.clone()),
            },
            FactsSlot,
        );
        *phase = RestorePhase::Facts;
    }
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    version: u8,
    snapshot: String,
    target: String,
    workspace: String,
    restored: Snapshot,
}
impl Receipt {
    fn decode(
        bytes: &[u8],
        context: &Context,
        snapshot: &str,
        target: &str,
    ) -> Result<Snapshot, String> {
        let receipt: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if receipt.version != 1
            || receipt.snapshot != snapshot
            || receipt.target != target
            || receipt.workspace != context.root.to_str().ok_or("CI workspace must be UTF-8")?
            || receipt.restored.version != 2
        {
            return Err("Source cache restore receipt identity differs".into());
        }
        Ok(receipt.restored)
    }
}
#[derive(Component)]
struct RestoredSnapshot(Snapshot);
fn restore_apply(mut requests: RestoreApplyRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, restore, mut phase, mut sequence) in &mut requests {
        if *phase != RestorePhase::Facts {
            continue;
        }
        let Some(facts) = results.facts_slot_captured_sources.get(entity) else {
            continue;
        };
        let Some(saved) = results.snapshot_slot_optional_bytes.get(entity) else {
            continue;
        };
        let facts = facts.clone();
        let saved = Snapshot::read(saved.0.as_ref());
        let changed = Snapshot::now().max(Snapshot::compiled(&facts) + 1_000_000_000);
        let mut restored = Snapshot::capture(&facts);
        for file in &facts.files {
            let time = match saved.files.get(&file.name) {
                Some(previous) if previous.hash == file.hash && previous.mode == file.mode => {
                    Snapshot::time(&previous.mtime).unwrap_or(changed)
                }
                _ => changed,
            };
            restored.files.get_mut(&file.name).unwrap().mtime = time.to_string();
            io.request(
                entity,
                &mut sequence,
                SetTimes {
                    location: restore.scope.location(&file.name),
                    accessed: file.accessed,
                    modified: time,
                },
                TimestampSlot,
            );
        }
        for (name, directory) in &facts.directories {
            let current = &restored.directories[name];
            let time = match saved.directories.get(name) {
                Some(previous) if current.hash.is_some() && previous.hash == current.hash => {
                    Snapshot::time(&previous.mtime).unwrap_or(changed)
                }
                _ => changed,
            };
            restored.directories.get_mut(name).unwrap().mtime = time.to_string();
            io.request(
                entity,
                &mut sequence,
                SetTimes {
                    location: restore.scope.location(name),
                    accessed: directory.accessed,
                    modified: time,
                },
                TimestampSlot,
            );
        }
        for reference in &facts.git_references {
            io.request(
                entity,
                &mut sequence,
                SetTimes {
                    location: restore.scope.location(&reference.name),
                    accessed: reference.accessed,
                    modified: changed,
                },
                TimestampSlot,
            );
        }
        if !facts.links.is_empty() {
            for entry in &facts.entries {
                if entry.directory
                    && entry
                        .name
                        .file_name()
                        .is_some_and(|name| name == ".fingerprint")
                {
                    io.request(
                        entity,
                        &mut sequence,
                        RemoveDirectory(restore.scope.location(entry.name.clone())),
                        TimestampSlot,
                    );
                }
            }
        }
        io.commands
            .entity(entity)
            .insert(RestoredSnapshot(restored));
        *phase = RestorePhase::Applying;
    }
}
fn restore_record(mut requests: RestoreRecordRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, restore, restored, mut phase, mut sequence) in &mut requests {
        if *phase != RestorePhase::Applying {
            continue;
        }
        if results.timestamps.pending(entity) {
            continue;
        }
        if restore.save {
            let receipt = Receipt {
                version: 1,
                snapshot: restore.snapshot.clone(),
                target: restore.target.to_str().unwrap().into(),
                workspace: restore.scope.root.to_str().unwrap().into(),
                restored: restored.0.clone(),
            };
            let bytes = match serde_json::to_vec(&receipt) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error.to_string());
                    continue;
                }
            };
            io.request(
                entity,
                &mut sequence,
                Publish {
                    location: restore.scope.location(&restore.receipt),
                    bytes,
                    executable: false,
                },
                ReceiptSlot,
            );
            *phase = RestorePhase::Recording;
        } else {
            *phase = RestorePhase::Done;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
fn restore_finish(mut requests: RestoreFinishRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, mut phase) in &mut requests {
        if *phase == RestorePhase::Recording && results.receipt_slot_written.get(entity).is_some() {
            *phase = RestorePhase::Done;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
#[require(CheckpointPhase)]
struct Checkpoint {
    scope: Context,
    configuration: Option<Configuration>,
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum CheckpointPhase {
    #[default]
    Configuration,
    Listed,
    Facts,
    Publish,
    Remove,
    Done,
}
#[derive(Component)]
struct CheckpointConfigurationSlot;
#[derive(Component)]
struct CheckpointReceiptSlot;
#[derive(Component)]
struct CheckpointWriteSlot;
#[derive(Component)]
struct CheckpointRemoveSlot;
fn checkpoint_start(mut requests: CheckpointStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cache-checkpoint") {
            continue;
        }
        let workspace = if settings.0.present("INPUT_SNAPSHOT") {
            settings
                .0
                .get("INPUT_WORKSPACE")
                .and_then(OsStr::to_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(".")
        } else {
            "."
        };
        let scope = match context.scoped(workspace) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if let Some(snapshot) = settings.0.get("INPUT_SNAPSHOT").and_then(OsStr::to_str) {
            let configuration = Configuration::direct(settings, workspace, snapshot);
            if !configuration.save {
                io.emit(
                    entity,
                    &mut sequence,
                    Print(configuration.output().into_bytes()),
                    PrintSlot,
                );
                io.commands.entity(entity).insert((
                    Checkpoint {
                        scope,
                        configuration: Some(configuration),
                    },
                    CheckpointPhase::Done,
                ));
                continue;
            }
            io.request(
                entity,
                &mut sequence,
                scope.git(&settings.0, &["ls-files", "-z"]),
                ListedSlot,
            );
            io.request(
                entity,
                &mut sequence,
                OptionalRead {
                    location: scope.location(&configuration.receipt),
                    limit: 8 * 1024 * 1024,
                },
                CheckpointReceiptSlot,
            );
            io.commands.entity(entity).insert((
                Checkpoint {
                    scope,
                    configuration: Some(configuration),
                },
                CheckpointPhase::Listed,
            ));
        } else {
            let partition = match Arguments::text(&settings.0, "VMUX_CACHE_PARTITION") {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
            if !partition
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            {
                io.fail(entity, "Invalid checkpoint partition");
                continue;
            }
            io.request(
                entity,
                &mut sequence,
                ReadFile {
                    location: scope
                        .location(format!(".build-tools/source-cache-{partition}.json.inputs")),
                    limit: 1024 * 1024,
                },
                CheckpointConfigurationSlot,
            );
            io.commands.entity(entity).insert((
                Checkpoint {
                    scope,
                    configuration: None,
                },
                CheckpointPhase::Configuration,
            ));
        }
    }
}
fn checkpoint_capture(
    mut requests: CheckpointCaptureRequests<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, settings, mut checkpoint, mut phase, mut sequence) in &mut requests {
        if *phase == CheckpointPhase::Configuration {
            let Some(bytes) = results.checkpoint_configuration_slot_bytes.get(entity) else {
                continue;
            };
            let configuration: Configuration = match serde_json::from_slice(&bytes.0) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error.to_string());
                    continue;
                }
            };
            if !configuration.save {
                io.emit(
                    entity,
                    &mut sequence,
                    Print(configuration.output().into_bytes()),
                    PrintSlot,
                );
                checkpoint.configuration = Some(configuration);
                *phase = CheckpointPhase::Done;
                continue;
            }
            io.request(
                entity,
                &mut sequence,
                checkpoint.scope.git(&settings.0, &["ls-files", "-z"]),
                ListedSlot,
            );
            io.request(
                entity,
                &mut sequence,
                OptionalRead {
                    location: checkpoint.scope.location(&configuration.receipt),
                    limit: 8 * 1024 * 1024,
                },
                CheckpointReceiptSlot,
            );
            checkpoint.configuration = Some(configuration);
            *phase = CheckpointPhase::Listed;
        } else if *phase == CheckpointPhase::Listed {
            let Some(listed) = results
                .listed_slot_captured
                .get(entity)
                .map(crate::ci_io::process::Captured::text)
            else {
                continue;
            };
            let paths = match listed {
                Ok(value) => Context::paths(value),
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
            let configuration = checkpoint.configuration.as_ref().unwrap();
            io.request(
                entity,
                &mut sequence,
                SourceFacts {
                    root: checkpoint.scope.root.clone(),
                    paths,
                    target: Some(configuration.target.clone().into()),
                },
                FactsSlot,
            );
            *phase = CheckpointPhase::Facts;
        }
    }
}
fn checkpoint_write(mut requests: CheckpointWriteRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, checkpoint, mut phase, mut sequence) in &mut requests {
        if *phase != CheckpointPhase::Facts {
            continue;
        }
        let Some(facts) = results.facts_slot_captured_sources.get(entity) else {
            continue;
        };
        let Some(receipt) = results.checkpoint_receipt_slot_optional_bytes.get(entity) else {
            continue;
        };
        let Some(receipt) = receipt.0.as_ref() else {
            io.fail(entity, "Missing source cache restore receipt");
            continue;
        };
        let configuration = checkpoint.configuration.as_ref().unwrap();
        let receipt = match Receipt::decode(
            receipt,
            &checkpoint.scope,
            &configuration.snapshot,
            &configuration.target,
        ) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let mut snapshot = Snapshot::capture(facts);
        snapshot.publication(&receipt, facts);
        let bytes = match serde_json::to_vec(&snapshot) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: checkpoint.scope.location(&configuration.snapshot),
                bytes,
                executable: false,
            },
            CheckpointWriteSlot,
        );
        io.emit(
            entity,
            &mut sequence,
            Print(configuration.output().into_bytes()),
            PrintSlot,
        );
        *phase = CheckpointPhase::Publish;
    }
}
fn checkpoint_finish(mut requests: CheckpointFinishRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, checkpoint, mut phase, mut sequence) in &mut requests {
        if *phase == CheckpointPhase::Publish
            && results.checkpoint_write_slot_written.get(entity).is_some()
            && results.print_slot_written.get(entity).is_some()
        {
            let configuration = checkpoint.configuration.as_ref().unwrap();
            io.request(
                entity,
                &mut sequence,
                RemoveFile(checkpoint.scope.location(&configuration.receipt)),
                CheckpointRemoveSlot,
            );
            *phase = CheckpointPhase::Remove;
        } else if *phase == CheckpointPhase::Remove
            && results.checkpoint_remove_slot_written.get(entity).is_some()
            || *phase == CheckpointPhase::Done && results.print_slot_written.get(entity).is_some()
        {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
struct Validating;
impl Inputs {
    fn ordered(paths: BTreeSet<String>) -> String {
        let mut positives = Vec::new();
        let mut exclusions = Vec::new();
        for path in paths {
            if path.starts_with('!') {
                exclusions.push(path);
            } else {
                positives.push(path);
            }
        }
        positives.extend(exclusions);
        positives.join("\n")
    }
}
#[derive(Component)]
struct ValidationSlot;
fn validate(mut requests: ValidateRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cache-validate") {
            continue;
        }
        let mut paths = BTreeSet::from([
            "target".to_owned(),
            ".build-tools/cargo".into(),
            ".build-tools/ci-cargo".into(),
            ".build-tools/lints".into(),
        ]);
        if let Some(target) = settings.0.get("VMUX_CACHE_TARGET").and_then(OsStr::to_str)
            && !target.is_empty()
        {
            paths.insert(target.into());
        }
        for path in paths {
            io.request(
                entity,
                &mut sequence,
                DirectoryProbe(context.location(path)),
                ValidationSlot,
            );
        }
        io.commands.entity(entity).insert(Validating);
    }
}
fn validated(requests: ValidatedRequests<'_, '_>, mut io: Io, results: Results) {
    for entity in &requests {
        if results.validation.pending(entity) {
            continue;
        }
        let mut valid = true;
        for (owner, _, _, _, data) in &results.validation.captures {
            if owner.0 != entity {
                continue;
            }
            if data.is_some_and(|fact| fact.0 == Some(false)) {
                valid = false;
                break;
            }
        }
        if valid {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        } else {
            io.fail(entity, "CI cache target must be a directory");
        }
    }
}

type InputsStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Inputs>, Without<Completed>),
>;
type InputsCaptureRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Inputs,
        &'static mut InputPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type InputsEncodeRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Inputs,
        &'static Settings,
        &'static mut InputPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type InputsFinishRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut InputPhase),
    (With<Root>, With<Inputs>, Without<Completed>),
>;
type RestoreStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Restore>, Without<Completed>),
>;
type RestoreCaptureRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Restore,
        &'static mut RestorePhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type RestoreApplyRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Restore,
        &'static mut RestorePhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type RestoreRecordRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Restore,
        &'static RestoredSnapshot,
        &'static mut RestorePhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type RestoreFinishRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut RestorePhase),
    (With<Root>, With<Restore>, Without<Completed>),
>;
type CheckpointStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Checkpoint>, Without<Completed>),
>;
type CheckpointCaptureRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Settings,
        &'static mut Checkpoint,
        &'static mut CheckpointPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type CheckpointWriteRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Checkpoint,
        &'static mut CheckpointPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type CheckpointFinishRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Checkpoint,
        &'static mut CheckpointPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type ValidateRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Validating>, Without<Completed>),
>;
type ValidatedRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Root>, With<Validating>, Without<Completed>)>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use std::os::unix::fs::MetadataExt;
    #[test]
    fn restored_sources_reuse_only_matching_bytes_and_modes_and_keep_receipts_on_failure() {
        let mut fixture = Fixture::new();
        fixture.write("src/input.rs", "initial");
        fixture.commit();
        for (key, value) in [
            ("INPUT_SNAPSHOT", ".build-tools/snapshot.json"),
            ("INPUT_TARGET", "target"),
            ("INPUT_SAVE", "true"),
        ] {
            fixture.environment.set(key, value);
        }
        fixture.run("cache-restore", &[]).unwrap();
        fixture.run("cache-checkpoint", &[]).unwrap();
        let saved = std::fs::metadata(fixture.root().join("src/input.rs"))
            .unwrap()
            .mtime_nsec();
        let old = filetime::FileTime::from_unix_time(1, 0);
        filetime::set_file_mtime(fixture.root().join("src/input.rs"), old).unwrap();
        fixture.run("cache-restore", &[]).unwrap();
        assert_eq!(
            std::fs::metadata(fixture.root().join("src/input.rs"))
                .unwrap()
                .mtime_nsec(),
            saved
        );
        fixture.run("cache-checkpoint", &[]).unwrap();
        fixture.write("src/input.rs", "changed");
        filetime::set_file_mtime(fixture.root().join("src/input.rs"), old).unwrap();
        fixture.run("cache-restore", &[]).unwrap();
        assert!(
            std::fs::metadata(fixture.root().join("src/input.rs"))
                .unwrap()
                .mtime()
                > 1
        );
        fixture.write(".build-tools/snapshot.json.restore", "corrupt");
        assert!(fixture.run("cache-checkpoint", &[]).is_err());
        assert!(
            fixture
                .root()
                .join(".build-tools/snapshot.json.restore")
                .exists()
        );
        fixture.run("cache-restore", &[]).unwrap();
        fixture.environment.set("INPUT_TARGET", "other-target");
        assert!(
            fixture
                .run("cache-checkpoint", &[])
                .unwrap_err()
                .contains("identity")
        );
    }
    #[test]
    fn cache_partitions_include_generated_lockfiles_and_custom_profiles() {
        let mut fixture = Fixture::new();
        fixture.cli();
        for (key, value) in [
            ("VMUX_CACHE_PARTITION", "fixture"),
            ("VMUX_CACHE_TARGET", "target"),
        ] {
            fixture.environment.set(key, value);
        }
        fixture
            .environment
            .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
        let first = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        let manifest = std::fs::read_to_string(fixture.root().join("Cargo.toml")).unwrap();
        fixture.write(
            "Cargo.toml",
            format!("{manifest}\n[profile.custom]\ninherits = \"dev\"\n"),
        );
        let output = fixture.run("cache-inputs", &[]).unwrap();
        assert_ne!(first["prefix"], Fixture::outputs(&output)["prefix"]);
        assert_eq!(
            first["restore-prefix"],
            Fixture::outputs(&output)["restore-prefix"]
        );
        assert_eq!(first["paths"], Fixture::outputs(&output)["paths"]);
        fixture.git(&["rm", "--cached", "Cargo.lock"]);
        let before = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        let lock = std::fs::read_to_string(fixture.root().join("Cargo.lock")).unwrap();
        fixture.write("Cargo.lock", format!("{lock}\n"));
        let after = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        assert_ne!(before["prefix"], after["prefix"]);
        assert_eq!(before["restore-prefix"], after["restore-prefix"]);
        assert_eq!(before["paths"], after["paths"]);
    }

    #[test]
    fn adding_workspace_members_and_targets_keeps_the_cache_archive_version_stable() {
        let mut fixture = Fixture::new();
        fixture.cli();
        fixture.environment.set("VMUX_CACHE_PARTITION", "fixture");
        fixture.environment.set("VMUX_CACHE_TARGET", "target");
        fixture
            .environment
            .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
        let first = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        fixture.write(
            "crates/util/added/Cargo.toml",
            "[package]\nname = \"added\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
        );
        fixture.write(
            "crates/util/added/src/lib.rs",
            "pub fn value() -> u32 { 1 }\n",
        );
        let manifest = std::fs::read_to_string(fixture.root().join("Cargo.toml")).unwrap();
        fixture.write(
            "Cargo.toml",
            manifest.replace(
                "members = [\"crates/app/vmux_cli\"]",
                "members = [\"crates/app/vmux_cli\", \"crates/util/added\"]",
            ),
        );
        fixture.execute("cargo", &["generate-lockfile", "--offline"]);
        fixture.commit();
        let added = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        assert_ne!(first["key"], added["key"]);
        assert_eq!(first["restore-prefix"], added["restore-prefix"]);
        assert_eq!(first["paths"], added["paths"]);
        let manifest =
            std::fs::read_to_string(fixture.root().join("crates/app/vmux_cli/Cargo.toml")).unwrap();
        fixture.write(
            "crates/app/vmux_cli/Cargo.toml",
            format!("{manifest}\n[[bin]]\nname = \"extra\"\npath = \"src/extra.rs\"\n"),
        );
        fixture.write("crates/app/vmux_cli/src/extra.rs", "fn main() {}\n");
        fixture.commit();
        let extra = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        assert_ne!(added["key"], extra["key"]);
        assert_eq!(added["restore-prefix"], extra["restore-prefix"]);
        assert_eq!(added["paths"], extra["paths"]);
    }

    #[test]
    fn fallback_cache_partitions_refuse_different_compiler_settings() {
        let mut fixture = Fixture::new();
        fixture.cli();
        fixture.environment.set("VMUX_CACHE_PARTITION", "fixture");
        fixture.environment.set("VMUX_CACHE_TARGET", "target");
        fixture
            .environment
            .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
        let original = fixture.environment.clone();
        let first = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        for (key, value) in [
            ("RUSTFLAGS", "-C opt-level=0"),
            ("CARGO_ENCODED_RUSTFLAGS", "-C\u{1f}opt-level=0"),
            ("CARGO_PROFILE_DEV_LTO", "thin"),
            ("CARGO_BUILD_TARGET", "aarch64-apple-darwin"),
            ("RUNNER_OS", "macOS"),
            ("RUNNER_ARCH", "ARM64"),
            ("VMUX_CI_VALIDATION", "0"),
            ("VMUX_CACHE_PARTITION", "other"),
            ("VMUX_CACHE_TARGET", "other-target"),
        ] {
            fixture.environment = original.clone();
            fixture.environment.set(key, value);
            let changed = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
            assert_ne!(first["restore-prefix"], changed["restore-prefix"], "{key}");
        }
        fixture.environment = original;
        fixture.write(
            ".cargo/config.toml",
            "[build]\nrustflags = [\"-C\", \"opt-level=0\"]\n",
        );
        fixture.commit();
        let changed = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        assert_ne!(first["restore-prefix"], changed["restore-prefix"]);
    }

    #[test]
    fn fallback_restore_keeps_unchanged_dependencies_fresh_and_rebuilds_changed_sources() {
        let mut fixture = Fixture::new();
        fixture.cli();
        fixture.write(
            "crates/util/cache_dependency/Cargo.toml",
            "[package]\nname = \"cache_dependency\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
        );
        fixture.write(
            "crates/util/cache_dependency/src/lib.rs",
            "pub fn value() -> u32 { 7 }\n",
        );
        let manifest =
            std::fs::read_to_string(fixture.root().join("crates/app/vmux_cli/Cargo.toml")).unwrap();
        fixture.write("crates/app/vmux_cli/Cargo.toml", format!("{manifest}\n[dependencies]\ncache_dependency = {{ path = \"../../util/cache_dependency\" }}\n"));
        fixture.write(
            "crates/app/vmux_cli/src/main.rs",
            "fn main() { println!(\"{}\", cache_dependency::value()); }\n",
        );
        fixture.execute("cargo", &["generate-lockfile", "--offline"]);
        fixture.commit();
        for (key, value) in [
            ("VMUX_CACHE_PARTITION", "fixture"),
            ("VMUX_CACHE_TARGET", "target"),
            ("VMUX_CACHE_SAVE", "true"),
        ] {
            fixture.environment.set(key, value);
        }
        fixture
            .environment
            .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
        let first = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        for (input, output) in [
            ("INPUT_SNAPSHOT", "snapshot"),
            ("INPUT_TARGET", "target"),
            ("INPUT_RECEIPT", "receipt"),
        ] {
            fixture.environment.set(input, &first[output]);
        }
        fixture.environment.set("INPUT_SAVE", "true");
        fixture.run("cache-restore", &[]).unwrap();
        fixture.execute("cargo", &["build", "--offline", "--quiet"]);
        fixture.run("cache-checkpoint", &[]).unwrap();
        let mut dependency = None;
        for entry in std::fs::read_dir(fixture.root().join("target/debug/deps")).unwrap() {
            let path = entry.unwrap().path();
            if path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libcache_dependency-")
                && path
                    .extension()
                    .is_some_and(|extension| extension == "rlib")
            {
                dependency = Some(path);
                break;
            }
        }
        let dependency = dependency.unwrap();
        let compiled = std::fs::metadata(&dependency).unwrap().modified().unwrap();
        fixture.write("crates/app/vmux_cli/Cargo.toml", format!("{manifest}\n[dependencies]\ncache_dependency = {{ path = \"../../util/cache_dependency\" }}\n[package.metadata.fixture]\nrevision = 2\n"));
        let lock = std::fs::read_to_string(fixture.root().join("Cargo.lock")).unwrap();
        fixture.write("Cargo.lock", format!("{lock}\n"));
        fixture.write(
            "crates/app/vmux_cli/src/main.rs",
            "fn main() { println!(\"{}\", cache_dependency::value() + 1); }\n",
        );
        let changed = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        assert_ne!(first["key"], changed["key"]);
        assert_eq!(first["restore-prefix"], changed["restore-prefix"]);
        for (input, output) in [
            ("INPUT_SNAPSHOT", "snapshot"),
            ("INPUT_TARGET", "target"),
            ("INPUT_RECEIPT", "receipt"),
        ] {
            fixture.environment.set(input, &changed[output]);
        }
        fixture.run("cache-restore", &[]).unwrap();
        fixture.execute("cargo", &["build", "--offline", "--quiet"]);
        assert_eq!(
            compiled,
            std::fs::metadata(dependency).unwrap().modified().unwrap()
        );
        let output = fixture.execute("target/debug/vmux_cli", &[]);
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "8");
        fixture.run("cache-checkpoint", &[]).unwrap();
    }
}
