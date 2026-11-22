use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::Settings;
use crate::ci_io::fs::{
    Bytes, Print, ProbeRead, ProbedBytes, Publish, ReadFile, RemoveLink, Written,
};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::PathBuf;
use vmux_ecs::cli::CliInvocation;
#[derive(bevy_ecs :: system :: SystemParam)]
struct Results<'w, 's> {
    toolchain_slot_bytes: super::Reads<'w, 's, Bytes, ToolchainSlot>,
    binary_slot_bytes: super::Reads<'w, 's, Bytes, BinarySlot>,
    manifest_slot_bytes: super::Reads<'w, 's, Bytes, ManifestSlot>,
    binary_write_slot_written: super::Reads<'w, 's, Written, BinaryWriteSlot>,
    manifest_write_slot_written: super::Reads<'w, 's, Written, ManifestWriteSlot>,
    print_slot_written: super::Reads<'w, 's, Written, PrintSlot>,
    key_facts_slot_captured_sources:
        super::Reads<'w, 's, crate::ci_io::fs::CapturedSources, KeyFactsSlot>,
    key_write_slot_written: super::Reads<'w, 's, Written, KeyWriteSlot>,
    expected_slot_bytes: super::Reads<'w, 's, Bytes, ExpectedSlot>,
    manifest_slot_probed_bytes: super::Reads<'w, 's, ProbedBytes, ManifestSlot>,
    binary_slot_probed_bytes: super::Reads<'w, 's, ProbedBytes, BinarySlot>,
    reuse_print_slot_written: super::Reads<'w, 's, Written, ReusePrintSlot>,
    repair_directory_slot_written: super::Reads<'w, 's, Written, RepairDirectorySlot>,
    repair_binary_slot_written: super::Reads<'w, 's, Written, RepairBinarySlot>,
    repair_manifest_slot_written: super::Reads<'w, 's, Written, RepairManifestSlot>,
    tree_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, TreeSlot>,
    compiler_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, CompilerSlot>,
    files_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, FilesSlot>,
    cargo_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, CargoSlot>,
    graph_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, GraphSlot>,
}
pub(super) struct CiArtifactPlugin;
impl bevy_app::Plugin for CiArtifactPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                publish_start,
                install_start,
                identity,
                publish,
                install,
                finish,
                flags,
                key_start,
                key_capture,
                key_encode,
                key_finish,
                reuse_start,
                reuse_validate,
                reuse_finish,
                cache_publish_start,
                cache_publish_repair,
                cache_publish_write,
                cache_publish_finish,
            )
                .chain()
                .in_set(Stage::Step),
        );
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub(super) version: u8,
    pub(super) repository: String,
    pub(super) run: String,
    pub(super) tree: String,
    pub(super) toolchain: String,
    pub(super) os: String,
    pub(super) arch: String,
    pub(super) features: String,
    pub(super) compile: String,
    pub(super) build_identity: String,
}
impl Identity {
    fn from_inputs(settings: &Settings, tree: &str, toolchain: &[u8]) -> Result<Self, String> {
        let repository = Arguments::text(&settings.0, "GITHUB_REPOSITORY")?;
        let run = Arguments::text(&settings.0, "GITHUB_RUN_ID")?;
        let os = Arguments::text(&settings.0, "RUNNER_OS")?;
        let arch = Arguments::text(&settings.0, "RUNNER_ARCH")?;
        if repository.split('/').count() != 2
            || repository.split('/').any(str::is_empty)
            || !run.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("Invalid CI artifact provenance".into());
        }
        if !matches!(os.as_str(), "Linux" | "macOS") || !matches!(arch.as_str(), "X64" | "ARM64") {
            return Err("Unsupported CI artifact platform".into());
        }
        let tree = tree.trim().to_owned();
        if tree.len() != 40 || !tree.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Invalid artifact tree".into());
        }
        let source = std::str::from_utf8(toolchain).map_err(|error| error.to_string())?;
        let manifest: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        let channel = manifest
            .get("toolchain")
            .and_then(|value| value.get("channel"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing pinned Rust toolchain")?
            .to_owned();
        if channel.is_empty() {
            return Err("Missing pinned Rust toolchain".into());
        }
        Ok(Self {
            version: 2,
            repository,
            run,
            tree,
            toolchain: channel,
            os,
            arch,
            features: "dx,ci-syntax".into(),
            compile: if settings.0.get("VMUX_CI_FAST_RUST") == Some(OsStr::new("1")) {
                "opt1-cgu16".into()
            } else {
                "default".into()
            },
            build_identity: if settings.0.get("VMUX_CI_VALIDATION") == Some(OsStr::new("1")) {
                "validation".into()
            } else {
                "product".into()
            },
        })
    }
    pub(super) fn checksum(bytes: &[u8]) -> Result<String, String> {
        if bytes.is_empty() || bytes.len() > 256 * 1024 * 1024 {
            return Err("Invalid development CLI bytes".into());
        }
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    #[serde(flatten)]
    identity: Identity,
    sha256: String,
}
#[derive(Component)]
struct Current(Identity);
#[derive(Component)]
#[require(PublishPhase)]
struct Publisher {
    directory: PathBuf,
}
#[derive(Component)]
#[require(InstallPhase)]
struct Installer;
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum PublishPhase {
    #[default]
    Identity,
    Write,
    Done,
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum InstallPhase {
    #[default]
    Identity,
    Write,
    Done,
}
#[derive(Component)]
struct TreeSlot;
#[derive(Component)]
struct ToolchainSlot;
#[derive(Component)]
struct BinarySlot;
#[derive(Component)]
struct ManifestSlot;
#[derive(Component)]
struct BinaryWriteSlot;
#[derive(Component)]
struct ManifestWriteSlot;
#[derive(Component)]
struct PrintSlot;
fn publish_start(mut requests: PublishStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.artifact-publish") {
            continue;
        }
        let values = Arguments::values(invocation);
        if values.len() != 2 {
            io.fail(entity, "artifact-publish requires binary and directory");
            continue;
        }
        let directory = PathBuf::from(&values[1]);
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["rev-parse", "HEAD^{tree}"]),
            TreeSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location("rust-toolchain.toml"),
                limit: 4096,
            },
            ToolchainSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(PathBuf::from(&values[0])),
                limit: 256 * 1024 * 1024,
            },
            BinarySlot,
        );
        io.commands
            .entity(entity)
            .insert((Publisher { directory }, PublishPhase::Identity));
    }
}
fn install_start(mut requests: InstallStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.artifact-install") {
            continue;
        }
        let values = Arguments::values(invocation);
        if values.len() != 1 {
            io.fail(entity, "artifact-install requires directory");
            continue;
        }
        let directory = PathBuf::from(&values[0]);
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["rev-parse", "HEAD^{tree}"]),
            TreeSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location("rust-toolchain.toml"),
                limit: 4096,
            },
            ToolchainSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(directory.join("manifest.json")),
                limit: 32768,
            },
            ManifestSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(directory.join("vmux")),
                limit: 256 * 1024 * 1024,
            },
            BinarySlot,
        );
        io.commands
            .entity(entity)
            .insert((Installer, InstallPhase::Identity));
    }
}
fn identity(requests: IdentityRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, settings) in &requests {
        let Some(tree) = results
            .tree_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(toolchain) = results.toolchain_slot_bytes.get(entity) else {
            continue;
        };
        let identity =
            match tree.and_then(|tree| Identity::from_inputs(settings, tree, &toolchain.0)) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
        io.commands.entity(entity).insert(Current(identity));
    }
}
fn publish(mut requests: PublishRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, context, publisher, current, mut phase, mut sequence) in &mut requests {
        if *phase != PublishPhase::Identity {
            continue;
        }
        let Some(binary) = results.binary_slot_bytes.get(entity) else {
            continue;
        };
        let checksum = match Identity::checksum(&binary.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let bytes = binary.0.clone();
        let manifest = Manifest {
            identity: current.0.clone(),
            sha256: checksum,
        };
        let encoded = match serde_json::to_vec(&manifest) {
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
                location: context.location(publisher.directory.join("vmux")),
                bytes,
                executable: true,
            },
            BinaryWriteSlot,
        );
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: context.location(publisher.directory.join("manifest.json")),
                bytes: encoded,
                executable: false,
            },
            ManifestWriteSlot,
        );
        *phase = PublishPhase::Write;
    }
}
fn install(mut requests: InstallRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, context, current, mut phase, mut sequence) in &mut requests {
        if *phase != InstallPhase::Identity {
            continue;
        }
        let Some(encoded) = results.manifest_slot_bytes.get(entity) else {
            continue;
        };
        let Some(binary) = results.binary_slot_bytes.get(entity) else {
            continue;
        };
        let manifest: Manifest = match serde_json::from_slice(&encoded.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        if manifest.identity != current.0 {
            io.fail(entity, "CLI artifact provenance differs from this job");
            continue;
        }
        let checksum = match Identity::checksum(&binary.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if checksum != manifest.sha256 {
            io.fail(entity, "CLI artifact checksum mismatch");
            continue;
        }
        let bytes = binary.0.clone();
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: context.location(".vmux/bin/vmux"),
                bytes,
                executable: true,
            },
            BinaryWriteSlot,
        );
        *phase = InstallPhase::Write;
    }
}
fn finish(
    mut publishers: FinishPublishers<'_, '_>,
    mut installers: FinishInstallers<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, mut phase) in &mut publishers {
        if *phase == PublishPhase::Write
            && results.binary_write_slot_written.get(entity).is_some()
            && results.manifest_write_slot_written.get(entity).is_some()
        {
            *phase = PublishPhase::Done;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
    for (entity, mut phase) in &mut installers {
        if *phase == InstallPhase::Write && results.binary_write_slot_written.get(entity).is_some()
        {
            *phase = InstallPhase::Done;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
fn flags(mut requests: FlagsRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, invocation, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.fast-rust") {
            continue;
        }
        if results.print_slot_written.get(entity).is_some() {
            io.commands.entity(entity).insert(Completed(Ok(0)));
            continue;
        }
        if io.captures.iter().any(|(capture, parent, _, _)| {
            parent.parent() == entity && capture.contains::<PrintSlot>()
        }) {
            continue;
        }
        let mut values = BTreeMap::from([("CARGO_PROFILE_DEV_LTO", "off".to_owned())]);
        if let Some(encoded) = settings
            .0
            .get("CARGO_ENCODED_RUSTFLAGS")
            .and_then(OsStr::to_str)
        {
            let separator = if encoded.is_empty() { "" } else { "\x1f" };
            values.insert(
                "CARGO_ENCODED_RUSTFLAGS",
                format!("{encoded}{separator}-C\x1fopt-level=1\x1f-C\x1fcodegen-units=16"),
            );
        } else {
            let original = settings
                .0
                .get("RUSTFLAGS")
                .and_then(OsStr::to_str)
                .unwrap_or("");
            let separator = if original.is_empty() { "" } else { " " };
            values.insert(
                "RUSTFLAGS",
                format!("{original}{separator}-C opt-level=1 -C codegen-units=16"),
            );
        }
        let mut output = String::new();
        for (name, value) in values {
            if value.contains(['\r', '\n']) {
                io.fail(entity, "Invalid multiline Rust flags");
                output.clear();
                break;
            }
            output.push_str(&format!("{name}={value}\n"));
        }
        if !output.is_empty() {
            io.emit(entity, &mut sequence, Print(output.into_bytes()), PrintSlot);
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct CacheIdentity {
    version: u8,
    repository: String,
    toolchain: String,
    os: String,
    arch: String,
    features: String,
    compile: String,
    build_identity: String,
    input: String,
}
#[derive(Serialize, Deserialize)]
struct CachedManifest {
    #[serde(flatten)]
    identity: CacheIdentity,
    sha256: String,
}
#[derive(Component, Default)]
#[require(KeyPhase)]
struct Key {
    identity: Option<CacheIdentity>,
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum KeyPhase {
    #[default]
    Compiler,
    Probes,
    Capture,
    Publish,
    Done,
}
#[derive(Component)]
struct CompilerSlot;
#[derive(Component)]
struct CargoSlot;
#[derive(Component)]
struct GraphSlot;
#[derive(Component)]
struct FilesSlot;
#[derive(Component)]
struct KeyFactsSlot;
#[derive(Component)]
struct KeyWriteSlot;
fn key_start(mut requests: KeyStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cli-key") {
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["rev-parse", "HEAD^{tree}"]),
            TreeSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location("rust-toolchain.toml"),
                limit: 4096,
            },
            ToolchainSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.process(&settings.0, "rustc", &["-vV"]),
            CompilerSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["ls-files", "-z"]),
            FilesSlot,
        );
        io.commands
            .entity(entity)
            .insert((Key::default(), KeyPhase::Compiler));
    }
}
fn key_capture(mut requests: KeyCaptureRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, context, settings, mut phase, mut sequence) in &mut requests {
        if *phase == KeyPhase::Compiler {
            let Some(compiler) = results
                .compiler_slot_captured
                .get(entity)
                .map(crate::ci_io::process::Captured::text)
            else {
                continue;
            };
            let compiler = match compiler {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
            let Some(host) = compiler
                .lines()
                .find_map(|line| line.strip_prefix("host: "))
            else {
                io.fail(entity, "Missing Rust compiler host");
                continue;
            };
            io.request(
                entity,
                &mut sequence,
                context.process(&settings.0, "cargo", &["-V"]),
                CargoSlot,
            );
            io.request(
                entity,
                &mut sequence,
                context.process(
                    &settings.0,
                    "cargo",
                    &[
                        "tree",
                        "--locked",
                        "-p",
                        "vmux_cli",
                        "--no-default-features",
                        "--features",
                        "dx,ci-syntax",
                        "--target",
                        host,
                        "--edges",
                        "normal,build",
                        "--prefix",
                        "none",
                        "--format",
                        "{p}",
                    ],
                ),
                GraphSlot,
            );
            *phase = KeyPhase::Probes;
            continue;
        }
        if *phase != KeyPhase::Probes {
            continue;
        }
        let Some(listed) = results
            .files_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(graph) = results
            .graph_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let paths = match (listed, graph) {
            (Ok(listed), Ok(graph)) => Key::paths(listed, graph, context),
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        let paths = match paths {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        io.request(
            entity,
            &mut sequence,
            crate::ci_io::fs::SourceFacts {
                root: context.root.clone(),
                paths,
                target: None,
            },
            KeyFactsSlot,
        );
        *phase = KeyPhase::Capture;
    }
}
impl Key {
    fn excluded(name: &str) -> bool {
        name.starts_with("crates/feature/vmux_dx/lints/")
    }
    fn paths(listed: &str, graph: &str, context: &Context) -> Result<Vec<PathBuf>, String> {
        let directories = Self::directories(graph, context)?;
        let mut paths = Vec::new();
        for name in listed.split('\0') {
            if name.is_empty() || Self::excluded(name) {
                continue;
            }
            if matches!(
                name,
                "Cargo.toml"
                    | "Cargo.lock"
                    | "rust-toolchain.toml"
                    | "tool-versions.env"
                    | ".github/actions/setup-rust/action.yml"
            ) || name.starts_with(".cargo/")
                || name.starts_with("crates/build") && name.ends_with(".rs")
                || directories
                    .iter()
                    .any(|directory| name.starts_with(directory))
            {
                paths.push(PathBuf::from(name));
            }
        }
        Ok(paths)
    }
    fn directories(graph: &str, context: &Context) -> Result<BTreeSet<String>, String> {
        let mut directories = BTreeSet::new();
        for line in graph.lines() {
            let line = line.strip_suffix(" (*)").unwrap_or(line);
            let Some((_, path)) = line.rsplit_once(" (") else {
                continue;
            };
            let Some(path) = path.strip_suffix(')') else {
                continue;
            };
            if !path.starts_with('/') {
                continue;
            }
            let path = PathBuf::from(path)
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let relative = path
                .strip_prefix(&context.root)
                .map_err(|_| "CLI dependency escapes its checkout")?;
            if relative.as_os_str().is_empty() {
                return Err("CLI dependency escapes its checkout".into());
            }
            directories.insert(format!(
                "{}/",
                relative.to_str().ok_or("CLI path must be UTF-8")?
            ));
        }
        if !directories.contains("crates/app/vmux_cli/") {
            return Err("CLI dependency graph lacks its root".into());
        }
        Ok(directories)
    }
    fn settings(settings: &Settings, hash: &mut Sha256) {
        for name in settings.0.names() {
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(value) = settings.0.get(name).and_then(OsStr::to_str) else {
                continue;
            };
            if name.starts_with("CARGO_")
                || name.starts_with("RUSTC")
                || matches!(
                    name,
                    "CC" | "CXX"
                        | "CFLAGS"
                        | "CXXFLAGS"
                        | "AR"
                        | "VMUX_CI_VALIDATION"
                        | "VMUX_BUILD_PROFILE"
                        | "VMUX_HOST_NAME"
                        | "VMUX_TARGET_DESTINATION"
                        | "VMUX_UPDATE_PUBLIC_KEY"
                        | "ImageOS"
                        | "ImageVersion"
                )
            {
                hash.update(format!("{name}={value}\0"));
            }
        }
    }
    fn digest(
        current: Identity,
        settings: &Settings,
        facts: &crate::ci_io::fs::CapturedSources,
        compiler: &str,
        cargo: &str,
        graph: &str,
        context: &Context,
    ) -> Result<CacheIdentity, String> {
        let directories = Self::directories(graph, context)?;
        let mut identity = CacheIdentity {
            version: 2,
            repository: current.repository,
            toolchain: current.toolchain,
            os: current.os,
            arch: current.arch,
            features: current.features,
            compile: current.compile,
            build_identity: current.build_identity,
            input: String::new(),
        };
        let mut hash = Sha256::new();
        hash.update(serde_json::to_vec(&identity).map_err(|error| error.to_string())?);
        hash.update(compiler);
        hash.update(cargo);
        hash.update(graph.replace(
            context.root.to_str().ok_or("CI checkout must be UTF-8")?,
            "<checkout>",
        ));
        Self::settings(settings, &mut hash);
        let mut required = BTreeSet::from([
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "tool-versions.env",
        ]);
        if !facts.links.is_empty() {
            return Err("CLI compile inputs contain links".into());
        }
        for file in &facts.files {
            if Self::excluded(&file.name) {
                continue;
            }
            let shared = required.contains(file.name.as_str())
                || file.name.starts_with(".cargo/")
                || file.name.starts_with("crates/build") && file.name.ends_with(".rs")
                || file.name == ".github/actions/setup-rust/action.yml";
            if !shared
                && !directories
                    .iter()
                    .any(|directory| file.name.starts_with(directory))
            {
                continue;
            }
            hash.update(format!("{}\0{}\0{}\0", file.name, file.mode, file.hash));
            required.remove(file.name.as_str());
        }
        if !required.is_empty() {
            return Err("CLI compile inputs are missing".into());
        }
        identity.input = format!("{:x}", hash.finalize());
        Ok(identity)
    }
}
fn key_encode(mut requests: KeyEncodeRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, context, settings, mut key, mut phase, mut sequence) in &mut requests {
        if *phase != KeyPhase::Capture {
            continue;
        }
        let Some(facts) = results.key_facts_slot_captured_sources.get(entity) else {
            continue;
        };
        let Some(compiler) = results
            .compiler_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(cargo) = results
            .cargo_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(graph) = results
            .graph_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(tree) = results
            .tree_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(toolchain) = results.toolchain_slot_bytes.get(entity) else {
            continue;
        };
        let current =
            match tree.and_then(|tree| Identity::from_inputs(settings, tree, &toolchain.0)) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
        let identity = match (compiler, cargo, graph) {
            (Ok(compiler), Ok(cargo), Ok(graph)) => {
                Key::digest(current, settings, facts, compiler, cargo, graph, context)
            }
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
        };
        key.identity = match identity {
            Ok(value) => Some(value),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let identity = key.identity.as_ref().unwrap();
        let output = format!(
            "key=vmux-cli-v2-{}-{}-{}\n",
            identity.os, identity.arch, identity.input
        );
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: context.location(".vmux/cli-inputs.json"),
                bytes: serde_json::to_vec(identity).unwrap(),
                executable: false,
            },
            KeyWriteSlot,
        );
        io.emit(entity, &mut sequence, Print(output.into_bytes()), PrintSlot);
        *phase = KeyPhase::Publish;
    }
}
fn key_finish(mut requests: KeyFinishRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, mut phase) in &mut requests {
        if *phase == KeyPhase::Publish
            && results.key_write_slot_written.get(entity).is_some()
            && results.print_slot_written.get(entity).is_some()
        {
            *phase = KeyPhase::Done;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
#[require(ReusePhase)]
struct Reuse;
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum ReusePhase {
    #[default]
    Read,
    Install,
    Print,
    Done,
}
#[derive(Component)]
struct ExpectedSlot;
#[derive(Component)]
struct ReusePrintSlot;
fn reuse_start(mut requests: ReuseStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cli-reuse") {
            continue;
        }
        let values = Arguments::values(invocation);
        if values != [std::ffi::OsString::from(".vmux/cli-cache")] {
            io.fail(entity, "CLI cache must be runner-owned");
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(".vmux/cli-inputs.json"),
                limit: 32768,
            },
            ExpectedSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ProbeRead(ReadFile {
                location: context.location(".vmux/cli-cache/manifest.json"),
                limit: 32768,
            }),
            ManifestSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ProbeRead(ReadFile {
                location: context.location(".vmux/cli-cache/vmux"),
                limit: 256 * 1024 * 1024,
            }),
            BinarySlot,
        );
        io.commands.entity(entity).insert((Reuse, ReusePhase::Read));
    }
}
fn reuse_validate(mut requests: ReuseValidateRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, context, mut phase, mut sequence) in &mut requests {
        if *phase != ReusePhase::Read {
            continue;
        }
        let Some(expected) = results.expected_slot_bytes.get(entity) else {
            continue;
        };
        let Some(manifest) = results.manifest_slot_probed_bytes.get(entity) else {
            continue;
        };
        let Some(binary) = results.binary_slot_probed_bytes.get(entity) else {
            continue;
        };
        let expected: CacheIdentity = match serde_json::from_slice(&expected.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        let valid = match (&manifest.0, &binary.0) {
            (Ok(manifest), Ok(binary)) => {
                match serde_json::from_slice::<CachedManifest>(manifest) {
                    Ok(manifest) => {
                        manifest.identity == expected
                            && Identity::checksum(binary).ok().as_deref() == Some(&manifest.sha256)
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        };
        if valid {
            let bytes = binary.0.as_ref().unwrap().clone();
            io.request(
                entity,
                &mut sequence,
                Publish {
                    location: context.location(".vmux/bin/vmux"),
                    bytes,
                    executable: true,
                },
                BinaryWriteSlot,
            );
            *phase = ReusePhase::Install;
        } else {
            io.emit(
                entity,
                &mut sequence,
                Print(b"reused=false\n".to_vec()),
                ReusePrintSlot,
            );
            *phase = ReusePhase::Print;
        }
    }
}
fn reuse_finish(mut requests: ReuseFinishRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, mut phase, mut sequence) in &mut requests {
        if *phase == ReusePhase::Install && results.binary_write_slot_written.get(entity).is_some()
        {
            io.emit(
                entity,
                &mut sequence,
                Print(b"reused=true\n".to_vec()),
                ReusePrintSlot,
            );
            *phase = ReusePhase::Print;
        } else if *phase == ReusePhase::Print
            && results.reuse_print_slot_written.get(entity).is_some()
        {
            *phase = ReusePhase::Done;
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
#[require(CachePublishPhase)]
struct CachePublisher;
#[derive(Component)]
struct CachePublished;
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum CachePublishPhase {
    #[default]
    Directory,
    Leaves,
    Read,
}
#[derive(Component)]
struct RepairDirectorySlot;
#[derive(Component)]
struct RepairBinarySlot;
#[derive(Component)]
struct RepairManifestSlot;
fn cache_publish_start(mut requests: CachePublishStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cli-publish") {
            continue;
        }
        let values = Arguments::values(invocation);
        if values != [std::ffi::OsString::from(".vmux/cli-cache")] {
            io.fail(entity, "CLI cache must be runner-owned");
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(".vmux/cli-inputs.json"),
                limit: 32768,
            },
            ExpectedSlot,
        );
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(".vmux/bin/vmux"),
                limit: 256 * 1024 * 1024,
            },
            BinarySlot,
        );
        io.request(
            entity,
            &mut sequence,
            RemoveLink(context.location(".vmux/cli-cache")),
            RepairDirectorySlot,
        );
        io.commands
            .entity(entity)
            .insert((CachePublisher, CachePublishPhase::Directory));
    }
}
fn cache_publish_repair(
    mut requests: CachePublishRepairRequests<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, context, mut phase, mut sequence) in &mut requests {
        if *phase == CachePublishPhase::Directory
            && results.repair_directory_slot_written.get(entity).is_some()
        {
            io.request(
                entity,
                &mut sequence,
                RemoveLink(context.location(".vmux/cli-cache/vmux")),
                RepairBinarySlot,
            );
            io.request(
                entity,
                &mut sequence,
                RemoveLink(context.location(".vmux/cli-cache/manifest.json")),
                RepairManifestSlot,
            );
            *phase = CachePublishPhase::Leaves;
        } else if *phase == CachePublishPhase::Leaves
            && results.repair_binary_slot_written.get(entity).is_some()
            && results.repair_manifest_slot_written.get(entity).is_some()
        {
            *phase = CachePublishPhase::Read;
        }
    }
}
fn cache_publish_write(
    mut requests: CachePublishWriteRequests<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, context, phase, mut sequence) in &mut requests {
        if *phase != CachePublishPhase::Read {
            continue;
        }
        let Some(expected) = results.expected_slot_bytes.get(entity) else {
            continue;
        };
        let Some(binary) = results.binary_slot_bytes.get(entity) else {
            continue;
        };
        let identity: CacheIdentity = match serde_json::from_slice(&expected.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        let sha256 = match Identity::checksum(&binary.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let manifest = CachedManifest { identity, sha256 };
        let bytes = binary.0.clone();
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: context.location(".vmux/cli-cache/vmux"),
                bytes,
                executable: true,
            },
            BinaryWriteSlot,
        );
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: context.location(".vmux/cli-cache/manifest.json"),
                bytes: serde_json::to_vec(&manifest).unwrap(),
                executable: false,
            },
            ManifestWriteSlot,
        );
        io.commands.entity(entity).insert(CachePublished);
    }
}
fn cache_publish_finish(
    requests: CachePublishFinishRequests<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for entity in &requests {
        if results.binary_write_slot_written.get(entity).is_some()
            && results.manifest_write_slot_written.get(entity).is_some()
        {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}

type PublishStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Publisher>, Without<Completed>),
>;
type InstallStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Installer>, Without<Completed>),
>;
type IdentityRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Settings),
    (
        With<Root>,
        Or<(With<Publisher>, With<Installer>)>,
        Without<Current>,
        Without<Completed>,
    ),
>;
type PublishRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Publisher,
        &'static Current,
        &'static mut PublishPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type InstallRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Current,
        &'static mut InstallPhase,
        &'static mut Sequence,
    ),
    (With<Root>, With<Installer>, Without<Completed>),
>;
type FinishPublishers<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut PublishPhase),
    (With<Root>, With<Publisher>, Without<Completed>),
>;
type FinishInstallers<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut InstallPhase),
    (With<Root>, With<Installer>, Without<Completed>),
>;
type FlagsRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type KeyStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Key>, Without<Completed>),
>;
type KeyCaptureRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut KeyPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type KeyEncodeRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut Key,
        &'static mut KeyPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type KeyFinishRequests<'w, 's> =
    Query<'w, 's, (Entity, &'static mut KeyPhase), (With<Root>, With<Key>, Without<Completed>)>;
type ReuseStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Reuse>, Without<Completed>),
>;
type ReuseValidateRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static mut ReusePhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type ReuseFinishRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut ReusePhase, &'static mut Sequence),
    (With<Root>, With<Reuse>, Without<Completed>),
>;
type CachePublishStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static mut Sequence,
    ),
    (With<Root>, Without<CachePublisher>, Without<Completed>),
>;
type CachePublishRepairRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static mut CachePublishPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type CachePublishWriteRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static CachePublishPhase,
        &'static mut Sequence,
    ),
    (
        With<Root>,
        With<CachePublisher>,
        Without<CachePublished>,
        Without<Completed>,
    ),
>;
type CachePublishFinishRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Root>, With<CachePublished>, Without<Completed>)>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use std::os::unix::fs::symlink;

    #[test]
    fn cli_identity_waits_for_compiler_toolchain_installation_before_cargo_probes() {
        let mut fixture = Fixture::new();
        fixture.cli();
        let compiler = fixture.execute("rustc", &["-vV"]).stdout;
        fixture.write("compiler-version", compiler);
        let root = fixture.root().to_path_buf();
        fixture.environment.set("VMUX_TEST_ROOT", root.as_os_str());
        for (name, variable) in [("rustc", "VMUX_TEST_RUSTC"), ("cargo", "VMUX_TEST_CARGO")] {
            let output = fixture.execute("rustup", &["which", name]);
            let path = String::from_utf8(output.stdout).unwrap();
            fixture.environment.set(variable, path.trim());
        }
        fixture.mock_tool("rustc", "#!/bin/sh\nif [ \"$1\" = '-vV' ]; then\n  sleep 0.2\n  touch \"$VMUX_TEST_ROOT/compiler-ready\"\n  cat \"$VMUX_TEST_ROOT/compiler-version\"\nelse\n  exec \"$VMUX_TEST_RUSTC\" \"$@\"\nfi\n");
        fixture.mock_tool("cargo", "#!/bin/sh\nif [ ! -f \"$VMUX_TEST_ROOT/compiler-ready\" ]; then\n  echo 'Cargo started before compiler toolchain installation completed' >&2\n  exit 1\nfi\nexec \"$VMUX_TEST_CARGO\" \"$@\"\n");
        fixture.mock_tool("compiler-wrapper", "#!/bin/sh\ncd /\nexec \"$@\"\n");
        fixture.environment.set(
            "RUSTC_WRAPPER",
            fixture.root().join("bin/compiler-wrapper").as_os_str(),
        );
        fixture
            .run("cli-key", &[])
            .expect("Cargo probes must wait for compiler completion");
    }
    #[test]
    fn artifact_provenance_and_checksum_refusals_preserve_the_installed_binary() {
        let mut fixture = Fixture::new();
        fixture.write("binary", "new binary");
        fixture
            .run("artifact-publish", &["binary", "artifact"])
            .unwrap();
        fixture.run("artifact-install", &["artifact"]).unwrap();
        assert_eq!(
            std::fs::read(fixture.root().join(".vmux/bin/vmux")).unwrap(),
            b"new binary"
        );
        for (key, value) in [
            ("GITHUB_RUN_ID", "1000"),
            ("RUNNER_OS", "macOS"),
            ("RUNNER_ARCH", "ARM64"),
            ("VMUX_CI_FAST_RUST", "0"),
            ("VMUX_CI_VALIDATION", "0"),
        ] {
            let original = fixture.environment.get(key).unwrap().to_owned();
            fixture.environment.set(key, value);
            assert!(
                fixture
                    .run("artifact-install", &["artifact"])
                    .unwrap_err()
                    .contains("provenance")
            );
            fixture.environment.set(key, original);
        }
        fixture.write("artifact/vmux", "tampered");
        assert!(
            fixture
                .run("artifact-install", &["artifact"])
                .unwrap_err()
                .contains("checksum")
        );
        assert_eq!(
            std::fs::read(fixture.root().join(".vmux/bin/vmux")).unwrap(),
            b"new binary"
        );
    }
    #[test]
    fn corrupt_cache_symlinks_rebuild_without_touching_their_targets() {
        for directory_link in [false, true] {
            let fixture = Fixture::new();
            fixture.cli();
            fixture.run("cli-key", &[]).unwrap();
            fixture.write(".vmux/bin/vmux", "fresh binary");
            let external = tempfile::tempdir().unwrap();
            let target = external.path().join("target");
            std::fs::write(&target, "external").unwrap();
            if directory_link {
                symlink(external.path(), fixture.root().join(".vmux/cli-cache")).unwrap();
            } else {
                std::fs::create_dir(fixture.root().join(".vmux/cli-cache")).unwrap();
                for name in ["vmux", "manifest.json"] {
                    symlink(&target, fixture.root().join(".vmux/cli-cache").join(name)).unwrap();
                }
            }
            assert_eq!(
                Fixture::outputs(&fixture.run("cli-reuse", &[".vmux/cli-cache"]).unwrap())["reused"],
                "false"
            );
            fixture.run("cli-publish", &[".vmux/cli-cache"]).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"external");
            assert_eq!(
                Fixture::outputs(&fixture.run("cli-reuse", &[".vmux/cli-cache"]).unwrap())["reused"],
                "true"
            );
        }
    }
    #[test]
    fn cli_cache_inputs_follow_actual_dependencies_and_embedded_settings() {
        let mut fixture = Fixture::new();
        fixture.cli();
        let first = Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"].clone();
        fixture.write("unrelated/app.rs", "unrelated");
        fixture.commit();
        let second = Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"].clone();
        assert_eq!(first, second);
        symlink("missing", fixture.root().join("unrelated/link")).unwrap();
        fixture.commit();
        assert_eq!(
            first,
            Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"]
        );
        fixture.environment.set("VMUX_UPDATE_PUBLIC_KEY", "new key");
        assert_ne!(
            first,
            Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"]
        );
        fixture.environment.remove("VMUX_UPDATE_PUBLIC_KEY");
        fixture.write(
            "crates/app/vmux_cli/src/main.rs",
            "fn main() { println!(\"changed\"); }",
        );
        fixture.commit();
        assert_ne!(
            first,
            Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"]
        );
    }

    #[test]
    fn cli_cache_key_ignores_separate_dylint_workspace_under_vmux_dx() {
        let fixture = Fixture::new();
        fixture.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/app/vmux_cli\", \"crates/feature/vmux_dx\"]\n[workspace.package]\nversion = \"1.0.0\"\n",
        );
        fixture.write(
            "crates/feature/vmux_dx/Cargo.toml",
            "[package]\nname = \"vmux_dx\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
        );
        fixture.write(
            "crates/feature/vmux_dx/src/lib.rs",
            "pub fn marker() -> u8 { 1 }\n",
        );
        fixture.write(
            "crates/app/vmux_cli/Cargo.toml",
            "[package]\nname = \"vmux_cli\"\nversion = \"1.0.0\"\nedition = \"2021\"\n[features]\ndx = [\"dep:vmux_dx\"]\nci-syntax = []\n[dependencies]\nvmux_dx = { path = \"../../feature/vmux_dx\", optional = true }\n",
        );
        fixture.write(
            "crates/app/vmux_cli/src/main.rs",
            "fn main() { let _ = vmux_dx::marker(); }\n",
        );
        fixture.execute("cargo", &["generate-lockfile", "--offline"]);
        fixture.commit();
        let initial = Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"].clone();
        fixture.write(
            "crates/feature/vmux_dx/lints/src/lib.rs",
            "pub fn lint() {}\n",
        );
        fixture.commit();
        assert_eq!(
            initial,
            Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"]
        );
        fixture.write(
            "crates/feature/vmux_dx/src/lib.rs",
            "pub fn marker() -> u8 { 2 }\n",
        );
        fixture.commit();
        assert_ne!(
            initial,
            Fixture::outputs(&fixture.run("cli-key", &[]).unwrap())["key"]
        );
    }
}
