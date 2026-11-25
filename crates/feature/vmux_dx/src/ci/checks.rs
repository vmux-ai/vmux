use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::Settings;
use crate::ci_io::fs::{Print, Publish, Written};
use crate::ci_io::process::ProbeCapture;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use globset::GlobBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use vmux_ecs::Order;
use vmux_ecs::cli::CliInvocation;
#[derive(bevy_ecs :: system :: SystemParam)]
struct Results<'w, 's> {
    print_slot_written: super::Reads<'w, 's, Written, PrintSlot>,
    record_write_slot_written: super::Reads<'w, 's, Written, RecordWriteSlot>,
    tree_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, TreeSlot>,
    record_tree_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, RecordTreeSlot>,
    runs_slot_probed_capture: super::Reads<'w, 's, crate::ci_io::process::ProbedCapture, RunsSlot>,
    jobs_slot_probed_capture: super::Reads<'w, 's, crate::ci_io::process::ProbedCapture, JobsSlot>,
    artifacts_slot_probed_capture:
        super::Reads<'w, 's, crate::ci_io::process::ProbedCapture, ArtifactsSlot>,
}
pub(super) struct CiChecksPlugin;
impl bevy_app::Plugin for CiChecksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                start,
                fingerprint,
                list_runs,
                next_run,
                jobs,
                artifacts,
                emit,
                printed,
                record_start,
                record_write,
                record_finish,
                gate,
                verify,
            )
                .chain()
                .in_set(Stage::Step),
        );
    }
}
#[derive(Serialize, Deserialize, Clone)]
struct MatrixRow {
    key: String,
    name: String,
    cache: String,
    dependencies: String,
    native: String,
    claim: String,
    reuse: String,
}
#[derive(Serialize, Deserialize, Default)]
struct Matrix {
    include: Vec<MatrixRow>,
}
#[derive(Component)]
struct Profile {
    key: String,
    name: String,
    paths: Vec<String>,
    scope: String,
    enabled: bool,
    matrix: Option<(String, String, String)>,
}
#[derive(Component, Default)]
struct Evidence {
    claim: String,
    reuse: String,
    settled: bool,
}
#[derive(Component)]
struct ProfileOwner(Entity);
#[derive(Component)]
#[require(Phase)]
struct Checks {
    base: String,
    tree: String,
    repository: String,
    branch: String,
    number: u64,
    run_id: u64,
    eligible: bool,
    cursor: u32,
    current: Option<u64>,
    attempt: Option<u64>,
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Tree,
    Runs,
    Candidate,
    Jobs,
    Artifacts,
    Emit,
    Printed,
}
#[derive(Component)]
struct Candidate {
    id: u64,
    attempt: u64,
}
#[derive(Component)]
struct CandidateOwner(Entity);
#[derive(Component)]
struct TreeSlot;
#[derive(Component)]
struct RunsSlot;
#[derive(Component)]
struct JobsSlot;
#[derive(Component)]
struct ArtifactsSlot;
#[derive(Component)]
struct PrintSlot;
impl Checks {
    fn boolean(settings: &Settings, key: &str) -> bool {
        settings.0.get(key) == Some(OsStr::new("true"))
    }
    fn encode(value: &str) -> String {
        let mut encoded = String::new();
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                encoded.push(byte as char);
            } else {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
        encoded
    }
    fn sha(value: &str) -> bool {
        value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
    fn settings(settings: &Settings) -> Result<Self, String> {
        let base = settings
            .0
            .get("VMUX_CHECK_BASE")
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_owned();
        let tree = settings
            .0
            .get("VMUX_CHECK_TREE")
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_owned();
        let repository = Arguments::text(&settings.0, "GITHUB_REPOSITORY")?;
        let branch = settings
            .0
            .get("VMUX_CHECK_BRANCH")
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_owned();
        let number = settings
            .0
            .get("VMUX_CHECK_PR")
            .and_then(OsStr::to_str)
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|error| error.to_string())?;
        let run_id = Arguments::text(&settings.0, "GITHUB_RUN_ID")?
            .parse()
            .map_err(|error: std::num::ParseIntError| error.to_string())?;
        let source_repository = settings
            .0
            .get("VMUX_CHECK_HEAD_REPOSITORY")
            .and_then(OsStr::to_str)
            .unwrap_or("");
        let eligible = settings.0.get("GITHUB_EVENT_NAME") == Some(OsStr::new("pull_request"))
            && source_repository == repository
            && settings.0.get("VMUX_CHECK_RELEASE") != Some(OsStr::new("true"));
        if eligible && (!Self::sha(&base) || !Self::sha(&tree) || number == 0 || branch.is_empty())
        {
            return Err("Invalid CI check provenance".into());
        }
        Ok(Self {
            base,
            tree,
            repository,
            branch,
            number,
            run_id,
            eligible,
            cursor: 0,
            current: None,
            attempt: None,
        })
    }
    fn matrix(profiles: &[(&Profile, &ProfileOwner, &Evidence, &Order)]) -> Matrix {
        let mut matrix = Matrix::default();
        for (profile, _, evidence, _) in profiles {
            if !profile.enabled {
                continue;
            }
            if let Some((cache, dependencies, native)) = &profile.matrix {
                matrix.include.push(MatrixRow {
                    key: profile.key.clone(),
                    name: profile.name.clone(),
                    cache: cache.clone(),
                    dependencies: dependencies.clone(),
                    native: native.clone(),
                    claim: evidence.claim.clone(),
                    reuse: evidence.reuse.clone(),
                });
            }
        }
        matrix
    }
    fn profiles(settings: &Settings) -> Result<Vec<Profile>, String> {
        let inputs = match settings
            .0
            .get("VMUX_CHECK_INPUTS")
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
        {
            Some(value) => serde_json::from_str(value).map_err(|error| error.to_string())?,
            None => BTreeMap::new(),
        };
        let lint = format!(
            "{};compiler={}",
            Profile::text(settings, "VMUX_LINT_SCOPE"),
            Self::boolean(settings, "VMUX_COMPILER_REQUIRED")
        );
        let rust = format!(
            "{};records={}",
            Profile::text(settings, "VMUX_TEST_SCOPE"),
            Self::boolean(settings, "VMUX_RECORDS")
        );
        let features = format!(
            "daemon,mcp,account-local,jobs;legacy={}",
            Self::boolean(settings, "VMUX_LEGACY_DESKTOP")
        );
        let mut profiles = vec![
            Profile::new(
                settings,
                &inputs,
                "lint",
                "Lint",
                "VMUX_COMPILER_REQUIRED",
                lint,
            ),
            Profile::new(
                settings,
                &inputs,
                "test-rust",
                "Test Rust",
                "VMUX_RUST_REQUIRED",
                rust,
            ),
            Profile::new(
                settings,
                &inputs,
                "test-desktop",
                "Test Desktop",
                "VMUX_DESKTOP_REQUIRED",
                "desktop-defaults".into(),
            ),
            Profile::new(
                settings,
                &inputs,
                "test-features",
                "Test Features",
                "VMUX_FEATURES_REQUIRED",
                features,
            ),
            Profile::new(
                settings,
                &inputs,
                "test-ecs",
                "Test ECS Composition",
                "VMUX_CODE_REQUIRED",
                "ownership+conventions".into(),
            ),
            Profile::new(
                settings,
                &inputs,
                "website",
                "Build Web",
                "",
                "website-release".into(),
            ),
        ];
        let (key, name) = super::sdk::Sdk::PROFILE;
        profiles.push(Profile::new(
            settings,
            &inputs,
            key,
            name,
            "VMUX_SDK_REQUIRED",
            key.to_string(),
        ));
        for profile in &profiles {
            if profile.paths.len() > 4096
                || profile.scope.len() > 65536
                || profile
                    .paths
                    .iter()
                    .any(|path| path.is_empty() || path.len() > 4096)
            {
                return Err("CI profile exceeds input bounds".into());
            }
        }
        Ok(profiles)
    }
}
impl Profile {
    fn text<'a>(settings: &'a Settings, key: &str) -> &'a str {
        settings.0.get(key).and_then(OsStr::to_str).unwrap_or("")
    }
    fn new(
        settings: &Settings,
        inputs: &BTreeMap<String, Vec<String>>,
        key: &str,
        name: &str,
        flag: &str,
        scope: String,
    ) -> Self {
        let mut execution = BTreeMap::new();
        for (name, key) in [
            ("os", "RUNNER_OS"),
            ("arch", "RUNNER_ARCH"),
            ("image", "ImageVersion"),
            ("flags", "RUSTFLAGS"),
            ("encoded", "CARGO_ENCODED_RUSTFLAGS"),
        ] {
            execution.insert(name, Self::text(settings, key));
        }
        let matrix = match key {
            "test-rust" => Some(("test", "test", "true")),
            "test-framework" => Some(("framework", "test", "true")),
            "test-desktop" => Some(("desktop", "test", "true")),
            "test-features" => Some(("headless", "test", "true")),
            "test-ecs" => Some(("ecs", "lint", "false")),
            _ => None,
        };
        Self {
            key: key.into(),
            name: name.into(),
            paths: Self::paths(settings, inputs, key),
            scope: format!("{};{scope}", serde_json::to_string(&execution).unwrap()),
            enabled: key == "website"
                || Checks::boolean(settings, "VMUX_CODE_REQUIRED")
                    && Checks::boolean(settings, flag),
            matrix: matrix.map(|(cache, dependencies, native)| {
                (cache.into(), dependencies.into(), native.into())
            }),
        }
    }
    fn paths(
        settings: &Settings,
        inputs: &BTreeMap<String, Vec<String>>,
        key: &str,
    ) -> Vec<String> {
        let key = if key == "test-framework" {
            "test-sdk"
        } else {
            key
        };
        let mut paths = Vec::new();
        for path in [
            ".github/**",
            ".cargo/**",
            ".gitattributes",
            ".env",
            ".env.*",
            "**/ecs-policy.ron",
            "cef/**",
            "packaging/**",
            "scripts/**",
            "Cargo.*",
            "rust-toolchain.toml",
            "tool-versions.env",
        ] {
            paths.push(path.into());
        }
        if matches!(key, "lint" | "test-ecs") {
            for path in ["crates/**", "patches/**", "examples/**", "website/**"] {
                paths.push(path.into());
            }
            if key == "test-ecs" {
                paths.extend([
                    "docs/architecture.md".into(),
                    "docs/dx/architecture.md".into(),
                ]);
            }
            return paths;
        }
        if key != "website" {
            match inputs.get(key) {
                Some(values) => paths.extend(values.iter().cloned()),
                None => paths.extend(["crates/**".into(), "patches/**".into()]),
            }
        }
        if key == "test-rust" && Checks::boolean(settings, "VMUX_RECORDS") {
            match inputs.get("records") {
                Some(values) => paths.extend(values.iter().cloned()),
                None => paths.extend(["crates/**".into(), "patches/**".into()]),
            }
        }
        if matches!(key, "test-sdk" | "website") {
            for path in [
                "crates/app/vmux_cli/**",
                "crates/feature/vmux_dx/**",
                "crates/util/vmux_ecs/**",
                "crates/util/vmux_api/**",
                "crates/util/vmux_macro/**",
                "crates/util/vmux_channel/**",
                "crates/util/vmux_watch/**",
                "crates/util/vmux_task/**",
                "crates/util/vmux_time/**",
                "crates/build_*.rs",
            ] {
                paths.push(path.into());
            }
            if key == "test-sdk" {
                paths.push("examples/**".into());
            } else {
                paths.extend(["website/**".into(), "docs/**".into()]);
            }
        }
        paths
    }
}
fn start(mut requests: StartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.checks-plan") {
            continue;
        }
        let checks = match Checks::settings(settings) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let profiles = match Checks::profiles(settings) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        for (index, profile) in profiles.into_iter().enumerate() {
            io.commands.spawn((
                Name::new("CI check"),
                vmux_ecs::CreatedAt::now(),
                ChildOf(entity),
                profile,
                Evidence::default(),
                ProfileOwner(entity),
                Order(index as u32),
            ));
        }
        if checks.eligible {
            io.request(
                entity,
                &mut sequence,
                context.git(&settings.0, &["ls-tree", "-r", "-z", &checks.tree]),
                TreeSlot,
            );
            io.commands.entity(entity).insert((checks, Phase::Tree));
        } else {
            io.commands.entity(entity).insert((checks, Phase::Emit));
        }
    }
}
fn fingerprint(
    mut roots: FingerprintRoots<'_, '_>,
    mut profiles: FingerprintProfiles<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, checks, mut phase) in &mut roots {
        if *phase != Phase::Tree {
            continue;
        }
        let Some(result) = results
            .tree_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let text = match result {
            Ok(value) => value.to_owned(),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        for (profile, owner, mut evidence) in &mut profiles {
            if owner.0 != entity || !profile.enabled {
                continue;
            }
            let mut matchers = Vec::new();
            for pattern in &profile.paths {
                match GlobBuilder::new(pattern)
                    .literal_separator(true)
                    .backslash_escape(true)
                    .build()
                {
                    Ok(glob) => matchers.push(glob.compile_matcher()),
                    Err(error) => {
                        io.fail(entity, error.to_string());
                    }
                }
            }
            let mut hash = Sha256::new();
            hash.update(format!(
                "v2\0{}\0{}\0{}\0{}\0{}\0{}\0",
                checks.repository,
                checks.number,
                checks.base,
                profile.key,
                profile.name,
                profile.scope
            ));
            hash.update(serde_json::to_vec(&profile.paths).unwrap());
            let mut eligible = true;
            let mut count = 0;
            for record in text.split('\0') {
                if record.is_empty() {
                    continue;
                }
                let Some((header, path)) = record.split_once('\t') else {
                    eligible = false;
                    break;
                };
                if !matchers.iter().any(|matcher| matcher.is_match(path)) {
                    continue;
                }
                count += 1;
                let mut fields = header.split_whitespace();
                let mode = fields.next().unwrap_or("");
                let kind = fields.next().unwrap_or("");
                let sha = fields.next().unwrap_or("");
                if !matches!(mode, "100644" | "100755") || kind != "blob" || !Checks::sha(sha) {
                    eligible = false;
                }
                hash.update(format!("\0{mode}\0{path}\0{sha}"));
            }
            if eligible && count > 0 {
                evidence.claim = format!(
                    "ci-check-v2-{}-{}-{}-{:x}",
                    checks.number,
                    checks.base,
                    profile.key,
                    hash.finalize()
                );
            }
        }
        *phase = Phase::Runs;
    }
}
fn list_runs(mut roots: ListRunsRoots<'_, '_>, mut io: Io) {
    for (entity, checks, context, settings, mut phase, mut sequence) in &mut roots {
        if *phase != Phase::Runs {
            continue;
        }
        let endpoint = format!(
            "repos/{}/actions/workflows/ci.yml/runs?event=pull_request&branch={}&per_page=20",
            checks.repository,
            Checks::encode(&checks.branch)
        );
        io.request(
            entity,
            &mut sequence,
            ProbeCapture(context.process(&settings.0, "gh", &["api", &endpoint])),
            RunsSlot,
        );
        *phase = Phase::Candidate;
    }
}
fn next_run(
    mut roots: NextRunRoots<'_, '_>,
    runs: NextRunRuns<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, mut checks, context, settings, mut phase, mut sequence) in &mut roots {
        if *phase != Phase::Candidate {
            continue;
        }
        let exists = runs.iter().any(|(_, owner, _)| owner.0 == entity);
        if !exists && checks.cursor == 0 {
            let Some(result) = results
                .runs_slot_probed_capture
                .get(entity)
                .map(crate::ci_io::process::ProbedCapture::text)
            else {
                continue;
            };
            let value: Value = match result
                .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
            {
                Ok(value) => value,
                Err(_) => {
                    *phase = Phase::Emit;
                    continue;
                }
            };
            let Some(list) = value["workflow_runs"].as_array() else {
                *phase = Phase::Emit;
                continue;
            };
            let mut candidates = Vec::new();
            for run in list.iter().take(20) {
                let Some(id) = run["id"].as_u64() else {
                    continue;
                };
                if id == checks.run_id
                    || run["event"].as_str() != Some("pull_request")
                    || run["head_branch"].as_str() != Some(&checks.branch)
                    || run.pointer("/repository/full_name").and_then(Value::as_str)
                        != Some(&checks.repository)
                    || run
                        .pointer("/head_repository/full_name")
                        .and_then(Value::as_str)
                        != Some(&checks.repository)
                    || run["path"].as_str() != Some(".github/workflows/ci.yml")
                {
                    continue;
                }
                if let Some(attempt) = run["run_attempt"].as_u64().filter(|value| *value > 0) {
                    candidates.push((id, attempt));
                }
            }
            candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
            if candidates.is_empty() {
                *phase = Phase::Emit;
                continue;
            }
            for (index, (id, attempt)) in candidates.into_iter().enumerate() {
                io.commands.spawn((
                    Candidate { id, attempt },
                    CandidateOwner(entity),
                    Order(index as u32),
                ));
            }
            continue;
        }
        let current = runs
            .iter()
            .find(|(_, owner, order)| owner.0 == entity && order.0 == checks.cursor);
        let Some((run, _, _)) = current else {
            *phase = Phase::Emit;
            continue;
        };
        checks.current = Some(run.id);
        checks.attempt = Some(run.attempt);
        let endpoint = format!(
            "repos/{}/actions/runs/{}/jobs?filter=latest&per_page=100",
            checks.repository, run.id
        );
        io.request(
            entity,
            &mut sequence,
            ProbeCapture(context.process(&settings.0, "gh", &["api", &endpoint])),
            JobsSlot,
        );
        *phase = Phase::Jobs;
    }
}
#[derive(Component)]
struct MatchedJob {
    attempt: u64,
}
fn jobs(
    mut roots: JobsRoots<'_, '_>,
    mut profiles: JobsProfiles<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, mut checks, context, settings, mut phase, mut sequence) in &mut roots {
        if *phase != Phase::Jobs {
            continue;
        }
        let Some(result) = results
            .jobs_slot_probed_capture
            .get(entity)
            .map(crate::ci_io::process::ProbedCapture::text)
        else {
            continue;
        };
        let value: Value = match result
            .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        {
            Ok(value) => value,
            Err(_) => {
                for (_, _, owner, mut evidence) in &mut profiles {
                    if owner.0 == entity {
                        evidence.reuse.clear();
                    }
                }
                *phase = Phase::Emit;
                continue;
            }
        };
        let Some(jobs) = value["jobs"].as_array() else {
            for (_, _, owner, mut evidence) in &mut profiles {
                if owner.0 == entity {
                    evidence.reuse.clear();
                }
            }
            *phase = Phase::Emit;
            continue;
        };
        if jobs.len() > 100 || value["total_count"].as_u64().unwrap_or(101) > 100 {
            for (_, _, owner, mut evidence) in &mut profiles {
                if owner.0 == entity {
                    evidence.reuse.clear();
                }
            }
            *phase = Phase::Emit;
            continue;
        }
        let mut matched = false;
        for (profile_entity, profile, owner, mut evidence) in &mut profiles {
            if owner.0 != entity || evidence.settled || evidence.claim.is_empty() {
                continue;
            }
            let entries: Vec<_> = jobs
                .iter()
                .filter(|job| job["name"].as_str() == Some(&profile.name))
                .collect();
            if entries.is_empty() {
                continue;
            }
            if entries.len() != 1 {
                evidence.settled = true;
                continue;
            }
            let job = entries[0];
            if job["conclusion"].as_str() == Some("skipped") {
                continue;
            }
            if job["status"].as_str() != Some("completed")
                || job["conclusion"].as_str() != Some("success")
            {
                evidence.settled = true;
                continue;
            }
            let attempt = job["run_attempt"].as_u64().or(checks.attempt).unwrap_or(0);
            if attempt == 0 || Some(attempt) != checks.attempt {
                evidence.settled = true;
                continue;
            }
            io.commands
                .entity(profile_entity)
                .insert(MatchedJob { attempt });
            matched = true;
        }
        if matched {
            let endpoint = format!(
                "repos/{}/actions/runs/{}/artifacts?per_page=100",
                checks.repository,
                checks.current.unwrap()
            );
            io.request(
                entity,
                &mut sequence,
                ProbeCapture(context.process(&settings.0, "gh", &["api", &endpoint])),
                ArtifactsSlot,
            );
            *phase = Phase::Artifacts;
        } else {
            checks.cursor += 1;
            *phase = Phase::Candidate;
        }
    }
}
fn artifacts(
    mut roots: ArtifactsRoots<'_, '_>,
    mut profiles: ArtifactsProfiles<'_, '_>,
    mut io: Io,
    results: Results,
) {
    for (entity, mut checks, mut phase) in &mut roots {
        if *phase != Phase::Artifacts {
            continue;
        }
        let Some(result) = results
            .artifacts_slot_probed_capture
            .get(entity)
            .map(crate::ci_io::process::ProbedCapture::text)
        else {
            continue;
        };
        let value: Value = match result
            .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        {
            Ok(value) => value,
            Err(_) => {
                for (_, owner, mut evidence, _) in &mut profiles {
                    if owner.0 == entity {
                        evidence.reuse.clear();
                    }
                }
                *phase = Phase::Emit;
                continue;
            }
        };
        let Some(artifacts) = value["artifacts"].as_array() else {
            for (_, owner, mut evidence, _) in &mut profiles {
                if owner.0 == entity {
                    evidence.reuse.clear();
                }
            }
            *phase = Phase::Emit;
            continue;
        };
        if artifacts.len() > 100 || value["total_count"].as_u64().unwrap_or(101) > 100 {
            for (_, owner, mut evidence, _) in &mut profiles {
                if owner.0 == entity {
                    evidence.reuse.clear();
                }
            }
            *phase = Phase::Emit;
            continue;
        }
        for (profile_entity, owner, mut evidence, matched) in &mut profiles {
            if owner.0 != entity {
                continue;
            }
            let Some(matched) = matched else {
                continue;
            };
            let name = format!("{}-{}", evidence.claim, matched.attempt);
            if artifacts.iter().any(|artifact| {
                artifact["expired"].as_bool() == Some(false)
                    && artifact["name"].as_str() == Some(&name)
            }) {
                evidence.reuse = checks.current.unwrap().to_string();
                evidence.settled = true;
            }
            io.commands.entity(profile_entity).remove::<MatchedJob>();
        }
        checks.cursor += 1;
        *phase = Phase::Candidate;
    }
}
fn emit(mut roots: EmitRoots<'_, '_>, profiles: EmitProfiles<'_, '_>, mut io: Io) {
    for (entity, mut phase, mut sequence) in &mut roots {
        if *phase != Phase::Emit {
            continue;
        }
        let mut values: Vec<_> = profiles
            .iter()
            .filter(|(_, owner, _, _)| owner.0 == entity)
            .collect();
        values.sort_by_key(|(_, _, _, order)| order.0);
        let mut output = String::new();
        let matrix = Checks::matrix(&values);
        for (profile, _, evidence, _) in values {
            let key = profile.key.replace('-', "_");
            output.push_str(&format!(
                "check_{key}={}\nreuse_{key}={}\n",
                evidence.claim, evidence.reuse
            ));
        }
        output.push_str(&format!(
            "tests={}\ntest_count={}\n",
            serde_json::to_string(&matrix).unwrap(),
            matrix.include.len()
        ));
        io.emit(entity, &mut sequence, Print(output.into_bytes()), PrintSlot);
        *phase = Phase::Printed;
    }
}
fn printed(roots: PrintedRoots<'_, '_>, mut io: Io, results: Results) {
    for (entity, phase) in &roots {
        if *phase == Phase::Printed && results.print_slot_written.get(entity).is_some() {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
struct Record {
    name: String,
    tree: String,
    run: String,
    attempt: String,
}
#[derive(Component)]
struct RecordWrite;
impl Record {
    fn valid_name(name: &str) -> bool {
        let Some(name) = name.strip_prefix("ci-check-v2-") else {
            return false;
        };
        let Some((number, name)) = name.split_once('-') else {
            return false;
        };
        let Some((base, name)) = name.split_once('-') else {
            return false;
        };
        let Some((key, digest)) = name.rsplit_once('-') else {
            return false;
        };
        number.parse::<u64>().is_ok_and(|value| value > 0)
            && Checks::sha(base)
            && matches!(
                key,
                "lint"
                    | "test-rust"
                    | "test-framework"
                    | "test-desktop"
                    | "test-features"
                    | "test-ecs"
                    | "website"
            )
            && digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }
}
#[derive(Component)]
struct RecordTreeSlot;
#[derive(Component)]
struct RecordWriteSlot;
fn record_start(mut requests: RecordStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.checks-record") {
            continue;
        }
        let name = match Arguments::text(&settings.0, "VMUX_CHECK_NAME") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let tree = match Arguments::text(&settings.0, "VMUX_CHECK_TREE") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let run = match Arguments::text(&settings.0, "GITHUB_RUN_ID") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let attempt = match Arguments::text(&settings.0, "GITHUB_RUN_ATTEMPT") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if !Record::valid_name(&name)
            || !Checks::sha(&tree)
            || !run.bytes().all(|byte| byte.is_ascii_digit())
            || attempt
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .is_none()
        {
            io.fail(entity, "Invalid CI check claim identity");
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["rev-parse", "HEAD^{tree}"]),
            RecordTreeSlot,
        );
        io.commands.entity(entity).insert(Record {
            name,
            tree,
            run,
            attempt,
        });
    }
}
fn record_write(mut requests: RecordWriteRequests<'_, '_>, mut io: Io, results: Results) {
    for (entity, context, record, mut sequence) in &mut requests {
        let Some(actual) = results
            .record_tree_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        match actual {
            Ok(value) if value.trim() == record.tree => {}
            Ok(_) => {
                io.fail(entity, "CI claim checkout differs from planned inputs");
                continue;
            }
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        }
        let value = serde_json :: json ! ({ "version" : 2 , "name" : record . name , "run" : record . run , "attempt" : record . attempt , "tree" : record . tree });
        io.request(
            entity,
            &mut sequence,
            Publish {
                location: context.location("ci-check-inputs.json"),
                bytes: serde_json::to_vec(&value).unwrap(),
                executable: false,
            },
            RecordWriteSlot,
        );
        io.commands.entity(entity).insert(RecordWrite);
    }
}
fn record_finish(requests: RecordFinishRequests<'_, '_>, mut io: Io, results: Results) {
    for entity in &requests {
        if results.record_write_slot_written.get(entity).is_some() {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
fn gate(requests: GateRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, settings) in &requests {
        if !invocation.is("dx.ci.checks-gate") {
            continue;
        }
        let results = match Arguments::text(&settings.0, "VMUX_CI_RESULTS").and_then(|value| {
            serde_json::from_str::<Value>(&value).map_err(|error| error.to_string())
        }) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let distribution = match Arguments::text(&settings.0, "VMUX_CI_DISTRIBUTION") {
            Ok(value) if value == "true" => true,
            Ok(value) if value == "false" => false,
            _ => {
                io.fail(entity, "Invalid distribution policy");
                continue;
            }
        };
        io.commands.entity(entity).insert(Verification {
            results,
            distribution,
        });
    }
}
fn verify(requests: VerifyRequests<'_, '_>, mut io: Io) {
    for (entity, verification) in &requests {
        match Verification::check(&verification.results, verification.distribution) {
            Ok(()) => {
                io.commands.entity(entity).insert(Completed(Ok(0)));
            }
            Err(error) => io.fail(entity, error),
        }
    }
}
#[derive(Component)]
struct Verification {
    results: Value,
    distribution: bool,
}
impl Verification {
    fn result(results: &Value, name: &str, required: bool, reused: bool) -> Result<(), String> {
        let expected = if required && !reused {
            "success"
        } else {
            "skipped"
        };
        if results[name]["result"].as_str() != Some(expected) {
            return Err(format!("CI {name}: expected {expected}"));
        }
        Ok(())
    }
    fn boolean(outputs: &Value, name: &str) -> Result<bool, String> {
        match outputs[name].as_str() {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(format!("Invalid CI selection: {name}")),
        }
    }
    fn check(results: &Value, distribution: bool) -> Result<(), String> {
        Self::result(results, "plan", true, false)?;
        Self::result(results, "changes", true, false)?;
        let outputs = &results["changes"]["outputs"];
        let code = Self::boolean(outputs, "code")?;
        let rust = Self::boolean(outputs, "rust")?;
        let sdk = Self::boolean(outputs, "contracts")?;
        let desktop = Self::boolean(outputs, "desktop")?;
        let headless = Self::boolean(outputs, "headless")?;
        let legacy = Self::boolean(outputs, "legacy_desktop")?;
        let website = Self::boolean(outputs, "website")?;
        let macos = Self::boolean(outputs, "macos")?;
        let ios = Self::boolean(outputs, "ios")?;
        let tooling = Self::boolean(outputs, "macos_tooling")?;
        let matrix: Matrix =
            serde_json::from_str(outputs["tests"].as_str().ok_or("Missing CI matrix")?)
                .map_err(|error| error.to_string())?;
        if matrix.include.len() > 5
            || outputs["test_count"].as_str() != Some(&matrix.include.len().to_string())
        {
            return Err("Invalid CI test matrix count".into());
        }
        let mut selected = BTreeSet::new();
        for row in &matrix.include {
            if !selected.insert(row.key.as_str()) {
                return Err("Duplicate CI matrix suite".into());
            }
        }
        for (key, required) in [
            ("test-rust", code && rust),
            ("test-desktop", code && desktop),
            ("test-features", code && (headless || legacy)),
            ("test-ecs", code),
            (super::sdk::Sdk::PROFILE.0, code && sdk),
        ] {
            if selected.contains(key) != required {
                return Err(format!("CI matrix differs from required scope: {key}"));
            }
            selected.remove(key);
        }
        if !selected.is_empty() {
            return Err("Unknown CI matrix suite".into());
        }
        Self::result(results, "tests", !matrix.include.is_empty(), false)?;
        Self::result(results, "lint", true, false)?;
        let compiler = Self::boolean(outputs, "compiler_lints")?;
        let reused = match outputs.get("reuse_lint") {
            None => "",
            Some(Value::String(value)) => value.as_str(),
            _ => return Err("Invalid lint reuse identity".into()),
        };
        Self::result(results, "lint-rust", code, !reused.is_empty())?;
        Self::result(results, "lint-ecs", code && compiler, !reused.is_empty())?;
        Self::result(
            results,
            "website",
            website,
            outputs["reuse_website"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
        )?;
        Self::result(
            results,
            "dx-macos",
            distribution && (macos || ios || tooling),
            false,
        )?;
        Self::result(results, "build-mac", distribution && macos, false)?;
        Self::result(results, "ios-build", ios, false)
    }
}

type StartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Checks>, Without<Completed>),
>;
type FingerprintRoots<'w, 's> =
    Query<'w, 's, (Entity, &'static Checks, &'static mut Phase), (With<Root>, Without<Completed>)>;
type FingerprintProfiles<'w, 's> = Query<
    'w,
    's,
    (
        &'static Profile,
        &'static ProfileOwner,
        &'static mut Evidence,
    ),
    Without<crate::ci_io::Owner>,
>;
type ListRunsRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Checks,
        &'static Context,
        &'static Settings,
        &'static mut Phase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type NextRunRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Checks,
        &'static Context,
        &'static Settings,
        &'static mut Phase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type NextRunRuns<'w, 's> =
    Query<'w, 's, (&'static Candidate, &'static CandidateOwner, &'static Order)>;
type JobsRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Checks,
        &'static Context,
        &'static Settings,
        &'static mut Phase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
type JobsProfiles<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Profile,
        &'static ProfileOwner,
        &'static mut Evidence,
    ),
    Without<crate::ci_io::Owner>,
>;
type ArtifactsRoots<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Checks, &'static mut Phase),
    (With<Root>, Without<Completed>),
>;
type ArtifactsProfiles<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ProfileOwner,
        &'static mut Evidence,
        Option<&'static MatchedJob>,
    ),
    Without<crate::ci_io::Owner>,
>;
type EmitRoots<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Phase, &'static mut Sequence),
    (With<Root>, With<Checks>, Without<Completed>),
>;
type EmitProfiles<'w, 's> = Query<
    'w,
    's,
    (
        &'static Profile,
        &'static ProfileOwner,
        &'static Evidence,
        &'static Order,
    ),
>;
type PrintedRoots<'w, 's> =
    Query<'w, 's, (Entity, &'static Phase), (With<Root>, Without<Completed>)>;
type RecordStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Record>, Without<Completed>),
>;
type RecordWriteRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Record,
        &'static mut Sequence,
    ),
    (With<Root>, Without<RecordWrite>, Without<Completed>),
>;
type RecordFinishRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Root>, With<RecordWrite>, Without<Completed>)>;
type GateRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static CliInvocation, &'static Settings),
    (With<Root>, Without<Verification>, Without<Completed>),
>;
type VerifyRequests<'w, 's> =
    Query<'w, 's, (Entity, &'static Verification), (With<Root>, Without<Completed>)>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;
    #[cfg(test)]
    struct Runs {
        fixture: Fixture,
        base: String,
    }
    #[cfg(test)]
    impl Runs {
        fn new() -> Self {
            let mut fixture = Fixture::new();
            fixture.write("crates/leaf/lib.rs", "leaf");
            fixture.write("crates/ecs/lib.rs", "ecs");
            let base = fixture.commit();
            let tree = fixture.git(&["rev-parse", "HEAD^{tree}"]);
            for (key, value) in [
                ("VMUX_CHECK_BASE", base.as_str()),
                ("VMUX_CHECK_TREE", tree.as_str()),
                ("VMUX_CHECK_PR", "42"),
                ("VMUX_CHECK_BRANCH", "feature"),
                ("VMUX_CHECK_HEAD_REPOSITORY", "fixture/repository"),
                ("VMUX_CODE_REQUIRED", "true"),
                ("VMUX_RUST_REQUIRED", "true"),
                ("VMUX_RECORDS", "true"),
            ] {
                fixture.environment.set(key, value);
            }
            fixture
                .environment
                .set("VMUX_CHECK_INPUTS", r#"{"test-rust":["crates/leaf/**"]}"#);
            fixture . mock_gh ("#!/bin/sh\ncase \"$2\" in\n*/ci.yml/runs*) cat api-runs.json ;;\n*jobs*) cat api-jobs.json ;;\n*artifacts*) cat api-artifacts.json ;;\nesac\n") ;
            fixture.json("api-runs.json", &json ! ({ "workflow_runs" : [] }));
            Self { fixture, base }
        }
        fn plan(&self) -> std::collections::BTreeMap<String, String> {
            Fixture::outputs(&self.fixture.run("checks-plan", &[]).unwrap())
        }
        fn run_record(id: u64) -> Value {
            json ! ({ "id" : id , "event" : "pull_request" , "head_branch" : "feature" , "repository" : { "full_name" : "fixture/repository" } , "head_repository" : { "full_name" : "fixture/repository" } , "path" : ".github/workflows/ci.yml" , "run_attempt" : 1 })
        }
        fn evidence(&self, claim: &str, conclusion: &str) {
            self.fixture.json(
                "api-runs.json",
                &json ! ({ "workflow_runs" : [Self :: run_record (99)] }),
            );
            self . fixture . json ("api-jobs.json" , & json ! ({ "total_count" : 1 , "jobs" : [{ "name" : "Test Rust" , "status" : "completed" , "conclusion" : conclusion , "run_attempt" : 1 }] })) ;
            self . fixture . json ("api-artifacts.json" , & json ! ({ "total_count" : 1 , "artifacts" : [{ "name" : format ! ("{claim}-1") , "expired" : false }] })) ;
        }
    }
    #[test]
    fn records_fallback_invalidates_reuse_and_explicit_empty_inputs_are_preserved() {
        let mut runs = Runs::new();
        let claim = runs.plan()["check_test_rust"].clone();
        runs.evidence(&claim, "success");
        assert_eq!(runs.plan()["reuse_test_rust"], "99");
        runs.fixture.write("crates/ecs/lib.rs", "changed");
        runs.fixture.commit();
        runs.fixture.environment.set(
            "VMUX_CHECK_TREE",
            runs.fixture.git(&["rev-parse", "HEAD^{tree}"]),
        );
        assert_eq!(runs.plan()["reuse_test_rust"], "");
        runs.fixture.environment.set(
            "VMUX_CHECK_INPUTS",
            r#"{"test-rust":["crates/leaf/**"],"records":[]}"#,
        );
        let empty = runs.plan()["check_test_rust"].clone();
        runs.fixture.write("crates/ecs/lib.rs", "changed again");
        runs.fixture.commit();
        runs.fixture.environment.set(
            "VMUX_CHECK_TREE",
            runs.fixture.git(&["rev-parse", "HEAD^{tree}"]),
        );
        assert_eq!(empty, runs.plan()["check_test_rust"]);
        assert_eq!(
            runs.fixture
                .environment
                .get("VMUX_CHECK_BASE")
                .unwrap()
                .to_str()
                .unwrap(),
            runs.base
        );
    }
    #[test]
    fn latest_failed_jobs_wrong_attempts_and_incomplete_api_evidence_force_fresh_checks() {
        let runs = Runs::new();
        let claim = runs.plan()["check_test_rust"].clone();
        runs.evidence(&claim, "failure");
        assert_eq!(runs.plan()["reuse_test_rust"], "");
        runs.evidence(&claim, "success");
        runs . fixture . json ("api-jobs.json" , & json ! ({ "total_count" : 1 , "jobs" : [{ "name" : "Test Rust" , "status" : "completed" , "conclusion" : "success" , "run_attempt" : 2 }] })) ;
        assert_eq!(runs.plan()["reuse_test_rust"], "");
        runs.evidence(&claim, "success");
        runs.fixture.json(
            "api-artifacts.json",
            &json ! ({ "total_count" : 101 , "artifacts" : [] }),
        );
        assert_eq!(runs.plan()["reuse_test_rust"], "");
        let matrix: Value = serde_json::from_str(&runs.plan()["tests"]).unwrap();
        assert_eq!(matrix["include"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn a_later_api_failure_clears_already_granted_reuse() {
        for process_error in [false, true] {
            let mut runs = Runs::new();
            let claim = runs.plan()["check_test_rust"].clone();
            runs.evidence(&claim, "success");
            runs.fixture.json(
                "api-runs.json",
                &json ! ({ "workflow_runs" : [Runs :: run_record (99) , Runs :: run_record (98)] }),
            );
            let failure = if process_error { "exit 1" } else { "echo '{}'" };
            runs . fixture . mock_gh (& format ! ("#!/bin/sh\ncase \"$2\" in\n*/98/jobs*) {failure} ;;\n*/ci.yml/runs*) cat api-runs.json ;;\n*jobs*) cat api-jobs.json ;;\n*artifacts*) cat api-artifacts.json ;;\nesac\n")) ;
            assert_eq!(runs.plan()["reuse_test_rust"], "");
        }
    }
    #[test]
    fn canceling_a_process_keeps_ownership_until_pipes_and_children_drain() {
        let mut runs = Runs::new();
        runs.fixture
            .mock_gh("#!/bin/sh\necho $$ > owned-pid\nsleep 30 &\nwait\n");
        let (mut app, entity) = runs.fixture.app("checks-plan", &[]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let path = runs.fixture.root().join("owned-pid");
        while !path.exists() {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let pid = std::fs::read_to_string(path)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        app.world_mut()
            .entity_mut(entity)
            .insert(vmux_ecs::OperationState::Canceled);
        assert!(
            Fixture::await_result(&mut app, entity)
                .unwrap_err()
                .contains("canceled")
        );
        assert!(!crate::signals::Signals::alive(pid));
    }
    #[test]
    fn the_gate_rejects_failures_duplicates_unknown_suites_and_unneeded_jobs() {
        let mut fixture = Fixture::new();
        let matrix = json ! ({ "include" : [{ "key" : "test-rust" , "name" : "Test Rust" , "cache" : "test" , "dependencies" : "test" , "native" : "true" , "claim" : "" , "reuse" : "99" } , { "key" : "test-ecs" , "name" : "Test ECS Composition" , "cache" : "ecs" , "dependencies" : "lint" , "native" : "false" , "claim" : "" , "reuse" : "" }] });
        let mut outputs = json ! ({ "code" : "true" , "rust" : "true" , "contracts" : "false" , "desktop" : "false" , "headless" : "false" , "legacy_desktop" : "false" , "website" : "false" , "macos" : "false" , "ios" : "false" , "macos_tooling" : "false" , "tests" : matrix . to_string () , "test_count" : "2" , "reuse_website" : "", "compiler_lints": "true", "reuse_lint": "" });
        let mut results = json ! ({ "plan" : { "result" : "success" } , "changes" : { "result" : "success" , "outputs" : outputs } , "lint" : { "result" : "success" }, "lint-rust": {"result": "success"}, "lint-ecs": {"result": "success"}, "tests" : { "result" : "success" } , "website" : { "result" : "skipped" } , "dx-macos" : { "result" : "skipped" } , "build-mac" : { "result" : "skipped" } , "ios-build" : { "result" : "skipped" } });
        fixture.environment.set("VMUX_CI_DISTRIBUTION", "false");
        fixture
            .environment
            .set("VMUX_CI_RESULTS", results.to_string());
        fixture.run("checks-gate", &[]).unwrap();
        results["changes"]["outputs"]
            .as_object_mut()
            .unwrap()
            .remove("reuse_lint");
        fixture
            .environment
            .set("VMUX_CI_RESULTS", results.to_string());
        fixture.run("checks-gate", &[]).unwrap();
        results["changes"]["outputs"]["reuse_lint"] = json!(42);
        fixture
            .environment
            .set("VMUX_CI_RESULTS", results.to_string());
        assert!(fixture.run("checks-gate", &[]).is_err());
        results["changes"]["outputs"] = outputs.clone();
        for (name, result) in [
            ("tests", "failure"),
            ("tests", "cancelled"),
            ("tests", "skipped"),
            ("lint", "skipped"),
            ("lint-rust", "failure"),
            ("lint-ecs", "failure"),
            ("lint-ecs", "skipped"),
            ("website", "success"),
        ] {
            let original = results[name]["result"].clone();
            results[name]["result"] = json!(result);
            fixture
                .environment
                .set("VMUX_CI_RESULTS", results.to_string());
            assert!(fixture.run("checks-gate", &[]).is_err());
            results[name]["result"] = original;
        }
        for key in ["test-rust", "unknown"] {
            let mut changed = matrix.clone();
            changed["include"][1]["key"] = json!(key);
            outputs["tests"] = json!(changed.to_string());
            results["changes"]["outputs"] = outputs.clone();
            fixture
                .environment
                .set("VMUX_CI_RESULTS", results.to_string());
            assert!(fixture.run("checks-gate", &[]).is_err());
        }
        outputs["tests"] = json!(matrix.to_string());
        outputs["reuse_lint"] = json!("99");
        results["changes"]["outputs"] = outputs.clone();
        results["lint-rust"]["result"] = json!("skipped");
        results["lint-ecs"]["result"] = json!("skipped");
        fixture
            .environment
            .set("VMUX_CI_RESULTS", results.to_string());
        fixture.run("checks-gate", &[]).unwrap();
        results["lint-ecs"]["result"] = json!("success");
        fixture
            .environment
            .set("VMUX_CI_RESULTS", results.to_string());
        assert!(fixture.run("checks-gate", &[]).is_err());
    }

    #[test]
    fn framework_selection_requires_one_complete_validation_job() {
        let mut runs = Runs::new();
        runs.fixture.environment.set("VMUX_SDK_REQUIRED", "true");
        let outputs = runs.plan();
        let matrix: Value = serde_json::from_str(&outputs["tests"]).unwrap();
        let mut sdk = BTreeSet::new();
        for row in matrix["include"].as_array().unwrap() {
            if row["key"] == "test-framework" {
                sdk.insert(row["key"].as_str().unwrap().to_owned());
                runs.fixture
                    .environment
                    .set("VMUX_CHECK_NAME", row["claim"].as_str().unwrap());
                runs.fixture.run("checks-record", &[]).unwrap();
            }
        }
        assert_eq!(sdk, BTreeSet::from(["test-framework".to_string()]));
        let mut selected = json!({"code": "true", "rust": "true", "contracts": "true", "desktop": "false", "headless": "false", "legacy_desktop": "false", "website": "false", "macos": "false", "ios": "false", "macos_tooling": "false", "compiler_lints": "true", "reuse_lint": "", "reuse_website": "", "tests": matrix.to_string(), "test_count": matrix["include"].as_array().unwrap().len().to_string()});
        let mut results = json!({"plan": {"result": "success"}, "changes": {"result": "success", "outputs": selected}, "lint": {"result": "success"}, "lint-rust": {"result": "success"}, "lint-ecs": {"result": "success"}, "tests": {"result": "success"}, "website": {"result": "skipped"}, "dx-macos": {"result": "skipped"}, "build-mac": {"result": "skipped"}, "ios-build": {"result": "skipped"}});
        runs.fixture
            .environment
            .set("VMUX_CI_DISTRIBUTION", "false");
        runs.fixture
            .environment
            .set("VMUX_CI_RESULTS", results.to_string());
        runs.fixture.run("checks-gate", &[]).unwrap();
        for invalid in ["", "test-sdk-host", "test-sdk-lsp", "test-framework"] {
            let mut incomplete = matrix.clone();
            incomplete["include"]
                .as_array_mut()
                .unwrap()
                .retain(|row| row["key"] != "test-framework");
            if !invalid.is_empty() {
                let mut row = matrix["include"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["key"] == "test-framework")
                    .unwrap()
                    .clone();
                row["key"] = json!(invalid);
                incomplete["include"]
                    .as_array_mut()
                    .unwrap()
                    .push(row.clone());
                if invalid == "test-framework" {
                    incomplete["include"].as_array_mut().unwrap().push(row);
                }
            }
            selected["tests"] = json!(incomplete.to_string());
            selected["test_count"] =
                json!(incomplete["include"].as_array().unwrap().len().to_string());
            results["changes"]["outputs"] = selected.clone();
            runs.fixture
                .environment
                .set("VMUX_CI_RESULTS", results.to_string());
            assert!(runs.fixture.run("checks-gate", &[]).is_err());
        }
    }
}
