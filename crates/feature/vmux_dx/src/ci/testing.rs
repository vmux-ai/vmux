use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_ecs::prelude::*;
use serde_json::Value;
use vmux_ecs::cli::CliResult;

use super::{Arguments, CiPlugin, Emission, Input};
use crate::environment::Environment;

#[cfg(test)]
pub(super) struct Fixture {
    pub(super) directory: tempfile::TempDir,
    pub(super) environment: Environment,
}

#[cfg(test)]
impl Fixture {
    pub(super) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut environment = Environment::current();
        for (key, value) in [
            ("GITHUB_REPOSITORY", "fixture/repository"),
            ("GITHUB_RUN_ID", "999"),
            ("GITHUB_RUN_ATTEMPT", "1"),
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("RUNNER_OS", "Linux"),
            ("RUNNER_ARCH", "X64"),
            ("VMUX_CI_FAST_RUST", "1"),
            ("VMUX_CI_VALIDATION", "1"),
        ] {
            environment.set(key, value);
        }
        let fixture = Self {
            directory,
            environment,
        };
        fixture.git(&["init", "-q"]);
        fixture.git(&["config", "user.name", "Fixture"]);
        fixture.git(&["config", "user.email", "fixture@example.com"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture.write(
            ".gitignore",
            ".build-tools/\ntarget/\nevent.json\nbin/\nci-check-inputs.json\n",
        );
        fixture.write("Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = []\n[workspace.package]\nversion = \"1.0.0\"\n");
        fixture.write("rust-toolchain.toml", "[toolchain]\nchannel = \"stable\"\n");
        fixture.write("tool-versions.env", "CEF_VERSION=fixture\n");
        fixture.write(".github/workflows/ci.yml", "name: CI\non: pull_request\njobs:\n  ci:\n    name: Results\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n");
        fixture.commit();
        fixture
    }

    pub(super) fn root(&self) -> &Path {
        self.directory.path()
    }

    pub(super) fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.root().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    pub(super) fn execute(&self, program: &str, arguments: &[&str]) -> Output {
        let output = Command::new(program)
            .args(arguments)
            .current_dir(self.root())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    pub(super) fn git(&self, arguments: &[&str]) -> String {
        String::from_utf8(self.execute("git", arguments).stdout)
            .unwrap()
            .trim()
            .into()
    }

    pub(super) fn commit(&self) -> String {
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", "Fixture"]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub(super) fn app(&self, operation: &str, arguments: &[&str]) -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(Input {
            root: self.root().canonicalize().unwrap(),
            environment: self.environment.clone(),
        })
        .add_plugins(CiPlugin);
        let mut values = vec![OsString::from(operation)];
        for argument in arguments {
            values.push(OsString::from(argument));
        }
        let entity = app
            .world_mut()
            .spawn(Arguments::invocation(values).unwrap())
            .id();
        (app, entity)
    }

    pub(super) fn await_result(app: &mut App, entity: Entity) -> Result<String, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.update();
            if let Some(result) = app.world().get::<CliResult>(entity) {
                let result = result.0.clone();
                result?;
                let mut query = app
                    .world_mut()
                    .query::<(&super::Owner, &Emission, &vmux_ecs::Order)>();
                let mut emissions = Vec::new();
                for (owner, emission, order) in query.iter(app.world()) {
                    if owner.0 == entity {
                        emissions.push((order.0, emission.0.clone()));
                    }
                }
                emissions.sort_by_key(|(order, _)| *order);
                let mut bytes = Vec::new();
                for (_, emission) in emissions {
                    bytes.extend(emission);
                }
                return String::from_utf8(bytes).map_err(|error| error.to_string());
            }
            assert!(Instant::now() < deadline, "CI operation did not drain");
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn run(&self, operation: &str, arguments: &[&str]) -> Result<String, String> {
        let (mut app, entity) = self.app(operation, arguments);
        Self::await_result(&mut app, entity)
    }

    pub(super) fn outputs(text: &str) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if let Some((key, delimiter)) = line.split_once("<<") {
                let mut value = Vec::new();
                let mut closed = false;
                for line in lines.by_ref() {
                    if line == delimiter {
                        closed = true;
                        break;
                    }
                    value.push(line);
                }
                assert!(closed, "Unclosed GitHub output: {key}");
                values.insert(key.into(), value.join("\n"));
            } else if let Some((key, value)) = line.split_once('=') {
                values.insert(key.into(), value.into());
            }
        }
        values
    }

    pub(super) fn json(&self, path: &str, value: &Value) {
        self.write(path, serde_json::to_vec(value).unwrap());
    }

    pub(super) fn cli(&self) {
        self.write("Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"crates/app/vmux_cli\"]\n[workspace.package]\nversion = \"1.0.0\"\n");
        self.write("crates/app/vmux_cli/Cargo.toml", "[package]\nname = \"vmux_cli\"\nversion = \"1.0.0\"\nedition = \"2021\"\n[features]\ndx = []\nci-syntax = []\n");
        self.write(
            "crates/app/vmux_cli/src/main.rs",
            "fn main() { println!(\"fixture\"); }\n",
        );
        self.execute("cargo", &["generate-lockfile", "--offline"]);
        self.commit();
    }

    pub(super) fn mock_gh(&mut self, source: &str) {
        self.write("bin/gh", source);
        std::fs::set_permissions(
            self.root().join("bin/gh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let original = self.environment.get("PATH").unwrap().to_string_lossy();
        self.environment.set(
            "PATH",
            format!("{}:{original}", self.root().join("bin").display()),
        );
    }
}

#[test]
fn unknown_operations_fail_without_waiting_for_a_job() {
    let fixture = Fixture::new();
    let started = Instant::now();
    assert!(
        fixture
            .run("unknown", &[])
            .unwrap_err()
            .contains("Unknown CI operation")
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn cancellation_wins_a_late_success_and_reparented_io_is_refused() {
    for reparent in [false, true] {
        let fixture = Fixture::new();
        let (mut app, entity) = fixture.app("fast-rust", &[]);
        app.update();
        if reparent {
            let mut query = app.world_mut().query::<(Entity, &super::Owner)>();
            let child = query
                .iter(app.world())
                .find(|(_, owner)| owner.0 == entity)
                .unwrap()
                .0;
            let other = app.world_mut().spawn_empty().id();
            app.world_mut().entity_mut(child).insert(ChildOf(other));
        } else {
            app.world_mut()
                .entity_mut(entity)
                .insert(vmux_ecs::OperationState::Canceled);
        }
        let result = Fixture::await_result(&mut app, entity).unwrap_err();
        assert!(
            result.contains(if reparent {
                "ownership changed"
            } else {
                "canceled"
            }),
            "{result}"
        );
        let mut query = app.world_mut().query::<(&super::Owner, &super::Flight)>();
        assert!(
            query
                .iter(app.world())
                .all(|(owner, flight)| owner.0 != entity || !flight.pending())
        );
    }
}

#[test]
fn fifo_inputs_are_rejected_without_waiting_for_a_writer() {
    let fixture = Fixture::new();
    fixture.execute("mkfifo", &["binary"]);
    let started = Instant::now();
    let result = fixture
        .run("artifact-publish", &["binary", "artifact"])
        .unwrap_err();
    assert!(result.contains("regular file"), "{result}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(test)]
struct IdentityFixture {
    fixture: Fixture,
}

#[cfg(test)]
impl IdentityFixture {
    fn new() -> Self {
        let fixture = Fixture::new();
        fixture.write("Cargo.toml", "[package]\nname = \"identity-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n");
        fixture.write(
            "build_git_env.rs",
            include_str!("../../../../build_git_env.rs"),
        );
        fixture.write(
            "build.rs",
            "mod build_git_env;\nfn main() { build_git_env::emit(); }\n",
        );
        let fixture = Self { fixture };
        fixture.source("initial");
        fixture.fixture.commit();
        fixture
    }

    fn source(&self, marker: &str) {
        self.fixture.write("src/main.rs", format!("fn main() {{ println!(\"{{}}|{{}}|{{}}|{marker}\", env!(\"VMUX_GIT_HASH\"), env!(\"VMUX_GIT_BRANCH\"), env!(\"VMUX_WORKTREE_ID\")); }}\n"));
    }

    fn build(
        &self,
        validation: bool,
        release: bool,
        profile: &str,
    ) -> (String, std::time::SystemTime) {
        let mut environment = self.fixture.environment.clone();
        for key in [
            "CARGO_BUILD_TARGET",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "VMUX_BUILD_PROFILE",
            "VMUX_HOST_NAME",
        ] {
            environment.remove(key);
        }
        environment.set(
            "CARGO_TARGET_DIR",
            self.fixture.root().join("target").as_os_str(),
        );
        environment.set("CARGO_INCREMENTAL", "0");
        environment.set("VMUX_CI_VALIDATION", if validation { "1" } else { "" });
        if !profile.is_empty() {
            environment.set("VMUX_BUILD_PROFILE", profile);
        }
        let mut command = environment.command("cargo", self.fixture.root());
        command.args(["build", "--offline", "--quiet"]);
        if release {
            command.arg("--release");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let binary = self.fixture.root().join(if release {
            "target/release/identity-fixture"
        } else {
            "target/debug/identity-fixture"
        });
        let output = Command::new(&binary).output().unwrap();
        assert!(output.status.success());
        (
            String::from_utf8(output.stdout).unwrap().trim().into(),
            std::fs::metadata(binary).unwrap().modified().unwrap(),
        )
    }
}

#[test]
fn validation_reuses_git_only_changes_and_product_identity_remains_current() {
    let fixture = IdentityFixture::new();
    let first = fixture.build(true, false, "");
    fixture
        .fixture
        .git(&["commit", "--allow-empty", "-qm", "Git only"]);
    let second = fixture.build(true, false, "");
    assert_eq!(first, second);
    fixture.source("changed");
    let changed = fixture.build(true, false, "");
    assert!(changed.0.ends_with("|changed"));
    assert_ne!(changed.1, second.1);
    let product = fixture.build(false, false, "");
    assert_eq!(
        product.0.split('|').next().unwrap(),
        fixture.fixture.git(&["rev-parse", "--short=7", "HEAD"])
    );
    assert_ne!(product.0, changed.0);
    fixture
        .fixture
        .git(&["commit", "--allow-empty", "-qm", "Product Git"]);
    let next = fixture.build(false, false, "");
    assert_eq!(
        next.0.split('|').next().unwrap(),
        fixture.fixture.git(&["rev-parse", "--short=7", "HEAD"])
    );
    assert_ne!(next.0, product.0);
}

#[test]
fn release_and_local_products_preserve_git_identity_with_validation_enabled() {
    for (release, profile) in [(true, "release"), (false, "local")] {
        let fixture = IdentityFixture::new();
        let validation = fixture.build(true, false, "");
        let product = fixture.build(true, release, profile);
        assert_eq!(
            product.0.split('|').next().unwrap(),
            fixture.fixture.git(&["rev-parse", "--short=7", "HEAD"])
        );
        assert_ne!(product.0, validation.0);
    }
}
