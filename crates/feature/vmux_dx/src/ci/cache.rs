use super::sources::Configuration;
use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::ci_io::cache::{RestoreFiles, StageFiles};
use crate::ci_io::fs::{
    Bytes, CapturedSources, DirectoryFact, DirectoryProbe, ReadFile, SourceFacts, Written,
};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use globset::GlobBuilder;
use std::path::PathBuf;
use vmux_ecs::cli::CliInvocation;

pub(super) struct CiCachePlugin;

impl bevy_app::Plugin for CiCachePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (start, load, probe, transfer, finish)
                .chain()
                .in_set(Stage::Step),
        );
    }
}

#[derive(Component)]
struct Transfer {
    partition: String,
    restore: bool,
    configuration: Option<Configuration>,
}

impl Transfer {
    fn files(&self, facts: &CapturedSources) -> Result<Vec<PathBuf>, String> {
        let configuration = self.configuration.as_ref().unwrap();
        let mut files = Vec::new();
        if self.restore {
            for entry in &facts.entries {
                if entry.directory {
                    continue;
                }
                let relative = entry
                    .name
                    .strip_prefix(&configuration.paths)
                    .map_err(|_| "Cache entry escapes its staging directory")?;
                if relative != std::path::Path::new(&configuration.snapshot)
                    && !relative.starts_with(&configuration.target)
                {
                    return Err("Cache entry is outside its compiler target".into());
                }
                files.push(relative.to_owned());
            }
            return Ok(files);
        }
        let mut patterns = Vec::new();
        for pattern in &configuration.artifacts {
            let excluded = pattern.starts_with('!');
            let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
            let direct = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| error.to_string())?
                .compile_matcher();
            let children = GlobBuilder::new(&format!("{pattern}/**"))
                .literal_separator(true)
                .build()
                .map_err(|error| error.to_string())?
                .compile_matcher();
            patterns.push((excluded, direct, children));
        }
        for entry in &facts.entries {
            if entry.directory {
                continue;
            }
            let mut included = false;
            for (excluded, direct, children) in &patterns {
                if direct.is_match(&entry.name) || children.is_match(&entry.name) {
                    included = !excluded;
                }
            }
            if included {
                files.push(entry.name.clone());
            }
        }
        files.push(PathBuf::from(&configuration.snapshot));
        Ok(files)
    }
}

#[derive(Component)]
struct ConfigurationSlot;
#[derive(Component)]
struct DirectorySlot;
#[derive(Component)]
struct FactsSlot;
#[derive(Component)]
struct TransferSlot;
#[derive(Component)]
struct Enumerating;
#[derive(Component)]
struct Scheduled;

fn start(mut requests: StartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, mut sequence) in &mut requests {
        let restore = match invocation.command.as_str() {
            "dx.ci.cache-stage" => false,
            "dx.ci.cache-unstage" => true,
            _ => continue,
        };
        let [partition] = Arguments::values(invocation) else {
            io.fail(entity, "Cache transfer requires one partition");
            continue;
        };
        let Some(partition) = partition.to_str() else {
            io.fail(entity, "Cache partition must be UTF-8");
            continue;
        };
        if partition.is_empty()
            || !partition
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            io.fail(entity, "Invalid cache transfer partition");
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            ReadFile {
                location: context.location(format!(".vmux/source-cache-{partition}.json.inputs")),
                limit: 8 * 1024 * 1024,
            },
            ConfigurationSlot,
        );
        io.commands.entity(entity).insert(Transfer {
            partition: partition.into(),
            restore,
            configuration: None,
        });
    }
}

fn load(
    mut requests: LoadRequests<'_, '_>,
    configurations: super::Reads<Bytes, ConfigurationSlot>,
    mut io: Io,
) {
    for (entity, context, mut transfer, mut sequence) in &mut requests {
        if transfer.configuration.is_some() {
            continue;
        }
        let Some(bytes) = configurations.get(entity) else {
            continue;
        };
        let configuration: Configuration = match serde_json::from_slice(&bytes.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        if configuration.paths != format!(".vmux/workspace-cache-{}", transfer.partition)
            || configuration.snapshot != format!(".vmux/source-cache-{}.json", transfer.partition)
        {
            io.fail(entity, "Cache staging identity differs from its partition");
            continue;
        }
        if let Err(error) = context.location(&configuration.target).resolve() {
            io.fail(entity, error);
            continue;
        }
        if transfer.restore {
            io.request(
                entity,
                &mut sequence,
                DirectoryProbe(context.location(&configuration.paths)),
                DirectorySlot,
            );
        } else {
            io.request(
                entity,
                &mut sequence,
                SourceFacts {
                    root: context.root.clone(),
                    paths: Vec::new(),
                    target: Some(PathBuf::from(&configuration.target)),
                },
                FactsSlot,
            );
            io.commands.entity(entity).insert(Enumerating);
        }
        transfer.configuration = Some(configuration);
    }
}

fn probe(
    mut requests: ProbeRequests<'_, '_>,
    directories: super::Reads<DirectoryFact, DirectorySlot>,
    mut io: Io,
) {
    for (entity, context, transfer, mut sequence) in &mut requests {
        let Some(configuration) = &transfer.configuration else {
            continue;
        };
        let Some(directory) = directories.get(entity) else {
            continue;
        };
        match directory.0 {
            None => {
                io.commands.entity(entity).insert(Completed(Ok(0)));
            }
            Some(false) => io.fail(entity, "Compiler cache staging path must be a directory"),
            Some(true) => {
                io.request(
                    entity,
                    &mut sequence,
                    SourceFacts {
                        root: context.root.clone(),
                        paths: Vec::new(),
                        target: Some(PathBuf::from(&configuration.paths)),
                    },
                    FactsSlot,
                );
                io.commands.entity(entity).insert(Enumerating);
            }
        }
    }
}

fn transfer(
    mut requests: TransferRequests<'_, '_>,
    facts: super::Reads<CapturedSources, FactsSlot>,
    mut io: Io,
) {
    for (entity, context, transfer, mut sequence) in &mut requests {
        let Some(facts) = facts.get(entity) else {
            continue;
        };
        let files = match transfer.files(facts) {
            Ok(files) => files,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let configuration = transfer.configuration.as_ref().unwrap();
        let staging = context.location(&configuration.paths);
        if transfer.restore {
            io.request(
                entity,
                &mut sequence,
                RestoreFiles {
                    source: staging,
                    files,
                },
                TransferSlot,
            );
        } else {
            let mut locations = Vec::new();
            for file in files {
                locations.push(context.location(file));
            }
            io.request(
                entity,
                &mut sequence,
                StageFiles {
                    destination: staging,
                    files: locations,
                },
                TransferSlot,
            );
        }
        io.commands.entity(entity).insert(Scheduled);
    }
}

fn finish(
    requests: FinishRequests<'_, '_>,
    writes: super::Reads<Written, TransferSlot>,
    mut io: Io,
) {
    for entity in &requests {
        if writes.get(entity).is_some() {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}

type StartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Transfer>, Without<Completed>),
>;
type LoadRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static mut Transfer,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type ProbeRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Transfer,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Enumerating>, Without<Completed>),
>;
type TransferRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Transfer,
        &'static mut Sequence,
    ),
    (
        With<Root>,
        With<Enumerating>,
        Without<Scheduled>,
        Without<Completed>,
    ),
>;
type FinishRequests<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Root>,
        With<Transfer>,
        With<Scheduled>,
        Without<Completed>,
    ),
>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    #[test]
    fn staged_archives_preserve_selected_artifacts_and_links_without_duplicating_registry_dependencies()
     {
        let mut fixture = Fixture::new();
        fixture.cli();
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
        let outputs = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        fixture.run("cache-unstage", &["fixture"]).unwrap();
        fixture.write(&outputs["snapshot"], "snapshot bytes");
        fixture.write(
            "target/debug/deps/libvmux_cli-cafe.rlib",
            "workspace artifact",
        );
        fixture.write("target/debug/deps/libserde-cafe.rlib", "registry artifact");
        fixture.write(
            "target/debug/.fingerprint/vmux_cli-cafe/dep-lib-vmux_cli",
            "fingerprint",
        );
        fixture.write(
            "target/debug/build/vmux_cli-cafe/out/native",
            "native artifact",
        );
        fixture.write(
            "target/debug/build/vmux_cli-cafe/out/package.tar.gz",
            "distribution",
        );
        fixture.write(
            "target/debug/incremental/vmux_cli-cafe/cache.bin",
            "incremental",
        );
        fixture.write(
            "target/release/vmux_cli.app/Contents/MacOS/vmux_cli",
            "bundle",
        );
        let artifact = fixture
            .root()
            .join("target/debug/deps/libvmux_cli-cafe.rlib");
        std::fs::set_permissions(&artifact, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mtime = filetime::FileTime::from_unix_time(1_234_567_890, 0);
        filetime::set_file_mtime(&artifact, mtime).unwrap();
        std::os::unix::fs::symlink(
            "native",
            fixture
                .root()
                .join("target/debug/build/vmux_cli-cafe/out/link"),
        )
        .unwrap();
        fixture.run("cache-stage", &["fixture"]).unwrap();
        let staging = fixture.root().join(&outputs["paths"]);
        assert_eq!(
            std::fs::read(staging.join("target/debug/deps/libvmux_cli-cafe.rlib")).unwrap(),
            b"workspace artifact"
        );
        assert!(
            !staging
                .join("target/debug/deps/libserde-cafe.rlib")
                .exists()
        );
        assert!(
            !staging
                .join("target/debug/incremental/vmux_cli-cafe/cache.bin")
                .exists()
        );
        assert!(
            !staging
                .join("target/debug/build/vmux_cli-cafe/out/package.tar.gz")
                .exists()
        );
        assert!(!staging.join("target/release/vmux_cli.app").exists());
        assert_eq!(
            std::fs::metadata(&artifact).unwrap().ino(),
            std::fs::metadata(staging.join("target/debug/deps/libvmux_cli-cafe.rlib"))
                .unwrap()
                .ino()
        );
        std::fs::remove_dir_all(fixture.root().join("target")).unwrap();
        std::fs::remove_file(fixture.root().join(&outputs["snapshot"])).unwrap();
        fixture.run("cache-unstage", &["fixture"]).unwrap();
        assert_eq!(std::fs::read(&artifact).unwrap(), b"workspace artifact");
        assert_eq!(std::fs::metadata(&artifact).unwrap().mtime(), 1_234_567_890);
        assert_eq!(
            std::fs::metadata(&artifact).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::read(fixture.root().join(&outputs["snapshot"])).unwrap(),
            b"snapshot bytes"
        );
        assert_eq!(
            std::fs::read_link(
                fixture
                    .root()
                    .join("target/debug/build/vmux_cli-cafe/out/link")
            )
            .unwrap(),
            std::path::PathBuf::from("native")
        );
        assert!(
            !fixture
                .root()
                .join("target/debug/deps/libserde-cafe.rlib")
                .exists()
        );
        fixture.write(
            &format!("{}/.git/foreign", outputs["paths"]),
            "untrusted entry",
        );
        assert!(
            fixture
                .run("cache-unstage", &["fixture"])
                .unwrap_err()
                .contains("outside its compiler target")
        );
    }

    #[test]
    fn consumer_targets_are_cached_without_incremental_files_and_symlinked_roots_are_refused() {
        let mut fixture = Fixture::new();
        fixture.cli();
        for (key, value) in [
            ("VMUX_CACHE_PARTITION", "framework"),
            ("VMUX_CACHE_TARGET", "target"),
            ("VMUX_CACHE_SAVE", "true"),
        ] {
            fixture.environment.set(key, value);
        }
        fixture
            .environment
            .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
        let output = Fixture::outputs(&fixture.run("cache-inputs", &[]).unwrap());
        fixture.write(&output["snapshot"], "snapshot");
        for consumer in [
            "framework-consumer",
            "host-consumer",
            "framework-git-consumer",
            "host-git-consumer",
        ] {
            fixture.write(
                &format!("target/{consumer}/debug/deps/libdependency.rlib"),
                "consumer dependency",
            );
            fixture.write(
                &format!("target/{consumer}/debug/incremental/cache.bin"),
                "incremental",
            );
        }
        fixture.run("cache-stage", &["framework"]).unwrap();
        let staging = fixture.root().join(&output["paths"]);
        for consumer in ["framework-consumer", "host-consumer"] {
            assert_eq!(
                std::fs::read(
                    staging.join(format!("target/{consumer}/debug/deps/libdependency.rlib"))
                )
                .unwrap(),
                b"consumer dependency"
            );
            assert!(
                !staging
                    .join(format!("target/{consumer}/debug/incremental/cache.bin"))
                    .exists()
            );
        }
        for consumer in ["framework-git-consumer", "host-git-consumer"] {
            assert!(
                !staging
                    .join(format!("target/{consumer}/debug/deps/libdependency.rlib"))
                    .exists()
            );
        }
        std::fs::remove_dir_all(&staging).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), &staging).unwrap();
        assert!(
            fixture
                .run("cache-unstage", &["framework"])
                .unwrap_err()
                .contains("symbolic links")
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
