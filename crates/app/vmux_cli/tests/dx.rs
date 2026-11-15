#![cfg(feature = "dx")]

use assert_cmd::Command;
#[cfg(not(feature = "runtime"))]
use assert_cmd::assert::OutputAssertExt;
use predicates::prelude::*;
#[cfg(target_os = "macos")]
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
#[test]
fn cargo_children_receive_command_specific_incremental_defaults_and_overrides() {
    let fixture = Fixture::new();
    fixture.script("cargo", "#!/bin/sh\ncase \"$1\" in -V|--version) printf 'cargo 1.95.0\\n';; *) printf '%s' \"${CARGO_INCREMENTAL:-unset}\" > \"$TEST_INCREMENTAL_LOG\";; esac\n");
    let log = fixture.root.join("incremental.log");
    for (command, ci, override_value, expected) in [
        ("build", false, None, "1"),
        ("test", false, None, "0"),
        ("check", false, None, "0"),
        ("clippy", false, None, "0"),
        ("build", true, None, "0"),
        ("build", false, Some("0"), "0"),
        ("test", false, Some("1"), "1"),
    ] {
        let mut invocation = fixture.command();
        invocation
            .env("TEST_INCREMENTAL_LOG", &log)
            .env("VMUX_BUILD_PROFILE", "dev")
            .env_remove("CARGO_INCREMENTAL")
            .env_remove("CI")
            .args(["internal", "cargo", "--", command]);
        if ci {
            invocation.env("CI", "true");
        }
        if let Some(value) = override_value {
            invocation.env("CARGO_INCREMENTAL", value);
        }
        invocation.assert().success();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), expected);
    }
}

static FIXTURES: (std::sync::Mutex<u8>, std::sync::Condvar) =
    (std::sync::Mutex::new(0), std::sync::Condvar::new());

struct Permit;

impl Permit {
    fn acquire() -> Self {
        let mut active = FIXTURES.0.lock().unwrap();
        while *active >= 4 {
            active = FIXTURES.1.wait(active).unwrap();
        }
        *active += 1;
        Self
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut active = FIXTURES.0.lock().unwrap();
        *active -= 1;
        FIXTURES.1.notify_one();
    }
}

#[cfg(not(feature = "runtime"))]
struct WorkspaceFixture {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
    _permit: Permit,
}

#[cfg(not(feature = "runtime"))]
impl WorkspaceFixture {
    fn new() -> Self {
        let permit = Permit::acquire();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("workspace with spaces");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let fixture = Self {
            _directory: directory,
            root,
            _permit: permit,
        };
        std::fs::write(fixture.root.join("Cargo.toml"), "[workspace]\nmembers = []\n[workspace.metadata.vmux.dx]\nproviders = [{name = 'fixtures', package = 'fixture-provider', binary = 'provider-bin', manifest = 'provider.ron'}]\n").unwrap();
        fixture.manifest("fixtures", "fixture");
        fixture.tool("cargo", "target=\nhost=\nwhile [ $# -gt 0 ]; do case \"$1\" in --target-dir) shift; target=$1;; --target) shift; host=$1;; esac; shift; done\nmkdir -p \"$target/$host/debug\"\ncp \"$VMUX_WORKSPACE_TEST_ROOT/provider-template\" \"$target/$host/debug/provider-bin\"\nchmod 755 \"$target/$host/debug/provider-bin\"\nprintf 'compiled\\n' >> \"$VMUX_WORKSPACE_TEST_ROOT/compiled\"\n");
        std::fs::write(fixture.root.join("provider-template"), "#!/bin/sh\nif [ \"$2\" = hold ]; then printf '%s' \"$$\" > \"$VMUX_WORKSPACE_TEST_ROOT/started\"; exec sleep 60; fi\nprintf '%s\\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(
            fixture.root.join("provider-template"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fixture
    }

    fn manifest(&self, name: &str, id: &str) {
        std::fs::write(
            self.root.join("provider.ron"),
            format!(
                r#"(
            cli: Some((commands: [(
                id: "{id}", name: "{name}", subcommand_required: true,
                arguments: [(id: "root", long: Some("root"), global: true)],
                commands: [(
                    id: "{id}.echo", name: "echo",
                    arguments: [(id: "forward", index: Some(1), trailing: true)],
                ), (id: "{id}.hold", name: "hold")],
            )])),
        )"#
            ),
        )
        .unwrap();
    }

    fn tool(&self, name: &str, body: &str) {
        let path = self.root.join("bin").join(name);
        std::fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_vmux"));
        command
            .current_dir(&self.root)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("VMUX_WORKSPACE_TEST_ROOT", &self.root)
            .env("VMUX_BUILD_STORAGE_MIN_GIB", "0")
            .env("VMUX_BUILD_STORAGE_WARN_GIB", "0");
        command
    }

    fn run(&self, arguments: &[&str]) -> Command {
        let mut command = Command::from_std(self.command());
        command
            .args(arguments)
            .timeout(std::time::Duration::from_secs(30));
        command
    }
}

#[cfg(not(feature = "runtime"))]
struct WorkspaceChild(std::process::Child);

#[cfg(not(feature = "runtime"))]
impl Drop for WorkspaceChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGINT);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[cfg(not(feature = "runtime"))]
fn provider_forwarding_uses_positional_indexes_and_places_the_delimiter_last() {
    let fixture = WorkspaceFixture::new();
    std::fs::write(
        fixture.root.join("provider.ron"),
        r#"(cli:Some((commands:[(
        id:"fixture",name:"fixtures",commands:[(
            id:"fixture.echo",name:"echo",arguments:[
                (id:"second",index:Some(2)),
                (id:"forward",index:Some(3),trailing:true),
                (id:"option",long:Some("option")),
                (id:"first",index:Some(1)),
            ],
        )],subcommand_required:true,
    )])))"#,
    )
    .unwrap();
    fixture.run(&["fixtures", "echo", "first value", "second value", "--option", "option value", "--", "--literal", "tail value"])
        .assert().success()
        .stdout("fixtures\necho\n--option\noption value\nfirst value\nsecond value\n--\n--literal\ntail value\n");
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn provider_help_refuses_a_fifo_manifest_without_waiting_for_a_writer() {
    let fixture = WorkspaceFixture::new();
    let path = fixture.root.join("provider.ron");
    std::fs::remove_file(&path).unwrap();
    let output = std::process::Command::new("mkfifo")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    fixture
        .run(&["fixtures", "--help"])
        .timeout(std::time::Duration::from_secs(2))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Workspace provider manifest"));
    assert!(!fixture.root.join("compiled").exists());
}

#[test]
#[cfg(not(feature = "runtime"))]
fn workspace_help_uses_the_declared_manifest_without_building_the_provider() {
    let fixture = WorkspaceFixture::new();
    fixture
        .run(&["fixtures", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"));
    assert!(!fixture.root.join("compiled").exists());
}

#[test]
#[cfg(not(feature = "runtime"))]
fn workspace_arguments_keep_spaces_and_forwarded_tool_options_without_shell_evaluation() {
    let fixture = WorkspaceFixture::new();
    let literal = "$(touch unexpected)";
    fixture
        .run(&[
            "fixtures",
            "--root",
            fixture.root.to_str().unwrap(),
            "echo",
            "--",
            "two words",
            literal,
            "--option",
            "--root",
            "/forwarded-path-must-not-select-a-provider",
        ])
        .assert()
        .success()
        .stdout(format!(
            "fixtures\n--root\n{}\necho\n--\ntwo words\n{literal}\n--option\n--root\n/forwarded-path-must-not-select-a-provider\n",
            fixture.root.display()
        ));
    assert!(!fixture.root.join("unexpected").exists());
}

#[test]
#[cfg(not(feature = "runtime"))]
fn workspace_compile_failure_preserves_its_status_and_prevents_invocation() {
    let fixture = WorkspaceFixture::new();
    fixture.tool("cargo", "exit 23\n");
    fixture
        .run(&["fixtures", "echo"])
        .assert()
        .code(23)
        .stdout("");
    assert!(!fixture.root.join("compiled").exists());
}

#[test]
#[cfg(not(feature = "runtime"))]
fn workspace_provider_cannot_claim_the_builtin_dx_namespace() {
    let fixture = WorkspaceFixture::new();
    fixture.manifest("fixtures", "dx.external");
    fixture.run(&["fixtures", "echo"]).assert().failure();
    assert!(!fixture.root.join("compiled").exists());
}

#[test]
#[cfg(not(feature = "runtime"))]
fn running_workspace_provider_does_not_hold_its_compile_cache_lock() {
    let fixture = WorkspaceFixture::new();
    let mut command = fixture.command();
    command.args(["fixtures", "hold"]);
    let mut child = WorkspaceChild(command.spawn().unwrap());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !fixture.root.join("started").exists() {
        if std::time::Instant::now() > deadline {
            child.0.kill().unwrap();
            panic!("provider never started");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    fixture
        .run(&["fixtures", "echo"])
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout("fixtures\necho\n");
    unsafe {
        libc::kill(child.0.id() as i32, libc::SIGINT);
    }
    let status = child.0.wait().unwrap();
    assert_eq!(status.code(), Some(130));
}

#[test]
#[cfg(not(feature = "runtime"))]
fn workspace_provider_interprets_its_own_dotenv_after_root_selection() {
    let fixture = WorkspaceFixture::new();
    std::fs::write(
        fixture.root.join(".env"),
        "VMUX_WORKSPACE_PRIVATE_VALUE=must-not-be-preloaded\n",
    )
    .unwrap();
    std::fs::write(
        fixture.root.join("provider-template"),
        "#!/bin/sh\nprintf '%s' \"${VMUX_WORKSPACE_PRIVATE_VALUE-absent}\"\n",
    )
    .unwrap();
    fixture
        .run(&["fixtures", "echo"])
        .env_remove("VMUX_WORKSPACE_PRIVATE_VALUE")
        .assert()
        .success()
        .stdout("absent");
}

#[test]
fn development_help_exposes_the_agreed_options_and_excludes_internal_integrations() {
    Command::cargo_bin("vmux")
        .unwrap()
        .args(["build", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-p, --profile"))
        .stdout(predicate::str::contains("-P, --platform"))
        .stdout(predicate::str::contains("[BRANCH]"))
        .stdout(predicate::str::contains("-b, --branch").not());
    Command::cargo_bin("vmux")
        .unwrap()
        .args(["dx", "test", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sdk"));
    Command::cargo_bin("vmux")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("internal").not());
}

#[test]
fn invalid_platforms_and_profiles_are_refused_before_repository_or_tool_access() {
    for args in [
        vec!["build", "-P", "invalid"],
        vec!["build", "-p", "invalid"],
        vec!["build", "--profile", "invalid"],
    ] {
        Command::cargo_bin("vmux")
            .unwrap()
            .args(args)
            .assert()
            .code(2);
    }
}

#[test]
#[cfg(unix)]
fn host_metadata_round_trips_native_arguments_and_refuses_invalid_identity() {
    let fixture = Fixture::new();
    let name = "Atlas \"Lab\" & $(touch unexpected)";
    let output = fixture
        .command()
        .args(["internal", "host-config"])
        .env("VMUX_HOST_NAME", name)
        .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(metadata["VMUX_HOST_NAME"], name);
    assert_eq!(
        metadata["VMUX_HOST_SERVICE_ID"],
        "org.example.atlas.service"
    );
    assert_eq!(metadata["VMUX_HOST_DATA_NAME"], "org.example.atlas");
    assert_eq!(metadata["VMUX_HOST_CAMERA"], "false");
    assert_eq!(metadata["VMUX_HOST_STABLE_ENDPOINT"], "");
    assert!(!fixture.root.join("unexpected").exists());
    for (key, value) in [
        ("VMUX_HOST_NAME", "Other"),
        ("VMUX_HOST_NAMESPACE", "org.example.other"),
        ("VMUX_HOST_DATA_NAME", "../Vmux"),
        ("VMUX_HOST_CAMERA", "yes"),
        ("VMUX_HOST_ICON", "missing-icon.icns"),
        ("VMUX_HOST_STABLE_ENDPOINT", "https://"),
        ("VMUX_HOST_STABLE_ENDPOINT", "https:///updates.json"),
        ("VMUX_HOST_STABLE_ENDPOINT", "https://?channel=stable"),
    ] {
        fixture
            .command()
            .args(["internal", "host-config"])
            .env(key, value)
            .assert()
            .failure();
    }
    assert!(!fixture.root.join("build.log").exists());
}

#[test]
#[cfg(unix)]
fn installed_tailwind_pin_is_recognized_when_ci_defaults_to_ansi_colors() {
    let fixture = Fixture::new();
    fixture.script("tailwindcss", "#!/bin/sh\nif [ -n \"${CI:-}\" ] && [ -z \"${NO_COLOR:-}\" ]; then\n  printf '\\033[34mv4.2.4\\033[39m\\n'\nelse\n  printf 'tailwindcss v4.2.4\\n'\nfi\n");
    fixture
        .command()
        .args(["internal", "install-tailwindcss"])
        .env("CI", "true")
        .env("GITHUB_ACTIONS", "true")
        .env_remove("NO_COLOR")
        .assert()
        .success();
    assert!(!fixture.root.join("build.log").exists());
}

#[test]
#[cfg(unix)]
fn cargo_cache_selection_never_combines_sccache_with_incremental_compilation() {
    for incremental in [None, Some("0"), Some("1")] {
        let fixture = Fixture::new();
        fixture.script("sccache", "#!/bin/sh\nexit 0\n");
        let mut command = fixture.command();
        command
            .args(["internal", "cargo", "--", "build"])
            .env_remove("VMUX_DISABLE_SCCACHE")
            .env_remove("RUSTC_WRAPPER")
            .env("CI", "false");
        match incremental {
            Some(value) => command.env("CARGO_INCREMENTAL", value),
            None => command.env_remove("CARGO_INCREMENTAL"),
        };
        command.assert().success();
    }
}

#[test]
#[cfg(target_os = "macos")]
fn custom_packages_preserve_product_identity_assets_and_service_namespace() {
    let fixture = Fixture::new();
    let name = "Atlas \"Lab\" & Co";
    std::fs::write(fixture.root.join("selected-tray.png"), "selected tray").unwrap();
    std::fs::write(fixture.root.join("selected-icon.icns"), "selected icon").unwrap();
    fixture
        .command()
        .args(["build", "-p", "local"])
        .env("VMUX_HOST_NAME", name)
        .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
        .env("VMUX_HOST_ICON", "selected-icon.icns")
        .env("VMUX_HOST_TRAY_ICON", "selected-tray.png")
        .assert()
        .success();
    let app = fixture.target.join(format!("release/{name} (9999999).app"));
    let info = plist::Value::from_file(app.join("Contents/Info.plist")).unwrap();
    let info = info.as_dictionary().unwrap();
    assert_eq!(
        info["CFBundleIdentifier"].as_string(),
        Some("org.example.atlas.desktop.9999999")
    );
    assert_eq!(
        info["CFBundleName"].as_string(),
        Some(format!("{name} (main)").as_str())
    );
    let launch = plist::Value::from_file(
        app.join("Contents/Library/LaunchAgents/org.example.atlas.service.plist"),
    )
    .unwrap();
    let launch = launch.as_dictionary().unwrap();
    assert_eq!(
        launch["Label"].as_string(),
        Some("org.example.atlas.service.9999999")
    );
    assert_eq!(
        launch["BundleProgram"].as_string(),
        Some(
            format!("Contents/Library/LoginItems/{name} Service.app/Contents/MacOS/{name} Service")
                .as_str()
        )
    );
    assert_eq!(
        launch["StandardOutPath"].as_string(),
        Some("/tmp/org-example-atlas-service.log")
    );
    assert_eq!(
        std::fs::read(app.join("Contents/Resources/tray.png")).unwrap(),
        b"selected tray"
    );
    assert_eq!(
        std::fs::read(app.join("Contents/Resources/Vmux.icns")).unwrap(),
        b"selected icon"
    );
}

#[test]
fn cli_update_preferences_work_outside_a_checkout_and_manual_checks_use_the_selected_channel() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("updates");
    let curl = directory.path().join("curl");
    std::fs::write(&curl, "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$TEST_CURL_LOG\"\nprintf '%s' '{\"version\":\"v9.0.0\",\"cli\":{}}'\n").unwrap();
    std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
    Command::cargo_bin("vmux")
        .unwrap()
        .current_dir(directory.path())
        .env("VMUX_CLI_UPDATE_ROOT", &root)
        .env("CURL_BIN", &curl)
        .env("TEST_CURL_LOG", directory.path().join("curl.log"))
        .args(["update", "-c", "preview", "--auto", "off", "--check"])
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("v9.0.0"));
    let preferences: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("update.json")).unwrap()).unwrap();
    assert_eq!(preferences["channel"], "preview");
    assert_eq!(preferences["enabled"], false);
    let requests = std::fs::read_to_string(directory.path().join("curl.log")).unwrap();
    assert!(requests.contains("/nightly/updates-preview.json"));
    Command::cargo_bin("vmux")
        .unwrap()
        .current_dir(directory.path())
        .env("VMUX_CLI_UPDATE_ROOT", &root)
        .env("CURL_BIN", &curl)
        .args(["update", "--auto", "on"])
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
    let preferences: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("update.json")).unwrap()).unwrap();
    assert_eq!(preferences["channel"], "preview");
    assert_eq!(preferences["enabled"], true);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("curl.log")).unwrap(),
        requests
    );
}

#[test]
fn invalid_cli_update_channels_are_refused_before_storage_access() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("updates");
    Command::cargo_bin("vmux")
        .unwrap()
        .env("VMUX_CLI_UPDATE_ROOT", &root)
        .args(["update", "--channel", "nightly"])
        .assert()
        .code(2);
    assert!(!root.exists());
}

#[test]
#[cfg(not(feature = "runtime"))]
fn automatic_cli_check_keeps_machine_readable_command_output_on_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("bin/vmux");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::copy(assert_cmd::cargo::cargo_bin("vmux"), &executable).unwrap();
    let curl = directory.path().join("curl");
    std::fs::write(
        &curl,
        format!(
            "#!/bin/sh\nprintf '%s' '{{\"version\":\"v{}\",\"cli\":{{}}}}'\n",
            env!("CARGO_PKG_VERSION")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new(executable);
    command
        .env("VMUX_CLI_UPDATE_ROOT", directory.path().join("updates"))
        .env("VMUX_UPDATE_PUBLIC_KEY", "fixture")
        .env("CURL_BIN", curl)
        .args([
            "internal",
            "nightly-version",
            "--",
            "0.0.34",
            "20261114",
            "1",
            "1",
        ])
        .timeout(std::time::Duration::from_secs(30));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let output = loop {
        match command.output() {
            Ok(output) => break output,
            Err(error)
                if error.raw_os_error() == Some(26) && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => panic!("CLI fixture could not start: {error}"),
        }
    };
    output
        .assert()
        .success()
        .stdout("0.0.35-nightly.20261114.1.1\n")
        .stderr(predicate::str::contains("CLI release:"));
}

#[test]
fn cli_update_lock_excludes_another_process_until_its_owner_releases_it() {
    let directory = tempfile::tempdir().unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(directory.path().join("update.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("vmux"))
        .env("VMUX_CLI_UPDATE_ROOT", directory.path())
        .args(["update", "--auto", "off"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(child.try_wait().unwrap().is_none());
    assert!(!directory.path().join("update.json").exists());
    lock.unlock().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("CLI update did not resume after lock release");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(directory.path().join("update.json").is_file());
}

#[test]
fn cli_update_cancellation_terminates_its_download_and_releases_the_installation_lock() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("updates");
    let curl = directory.path().join("curl");
    let started = directory.path().join("started");
    std::fs::write(
        &curl,
        "#!/bin/sh\nprintf '%s\\n' $$ > \"$TEST_STARTED\"\nsleep 60\nprintf '%s' '{}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("vmux"))
        .env("VMUX_CLI_UPDATE_ROOT", &root)
        .env("CURL_BIN", &curl)
        .env("TEST_STARTED", &started)
        .args(["update", "--check"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !started.is_file() {
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("CLI update never started its network request");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let status = std::process::Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(143));
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("CLI update ignored cancellation");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Command::cargo_bin("vmux")
        .unwrap()
        .env("VMUX_CLI_UPDATE_ROOT", &root)
        .args(["update", "--auto", "off"])
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
}

#[test]
fn both_branch_option_forms_refuse_a_missing_branch_without_changing_checkout() {
    for option in ["-b", "--branch"] {
        Command::cargo_bin("vmux")
            .unwrap()
            .args(["cache", option, "missing-dx-fixture-branch", "status"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Branch has no worktree"));
    }
}

#[test]
fn desktop_help_exposes_the_branch_without_launching() {
    Command::cargo_bin("vmux")
        .unwrap()
        .args(["desktop", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[BRANCH]"))
        .stdout(predicate::str::contains("current checkout"));
}

#[test]
#[cfg(unix)]
fn positional_branches_refuse_a_missing_worktree_before_building_or_launching() {
    let fixture = Fixture::new();
    for command in ["build", "run", "desktop", "serve"] {
        fixture
            .command()
            .args([command, "missing-dx-fixture-branch"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Branch has no worktree"));
    }
    assert!(!fixture.root.join("open.log").exists());
}

#[test]
#[cfg(unix)]
fn serve_uses_the_selected_website_and_development_profile() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.root.join("website")).unwrap();
    std::fs::write(fixture.root.join("website/tailwind.input.css"), "fixture").unwrap();
    fixture.script("dx", r#"#!/bin/sh
if [ "$1" = --version ]; then printf 'dx 0.7.9\n'; exit 0; fi
printf '%s\n%s\n%s\n%s\n' "$PWD" "$*" "$VMUX_BUILD_PROFILE" "$VMUX_DX_PLATFORM" > "$TEST_ROOT/serve.log"
"#);
    fixture.script(
        "../dx-home/tools/binaryen-127/bin/wasm-opt",
        "#!/bin/sh\nprintf 'wasm-opt version 127\\n'\n",
    );
    fixture
        .command()
        .args(["serve", "main"])
        .env("DX_BIN", fixture.root.join("bin/dx"))
        .env("DX_HOME", fixture.root.join("dx-home"))
        .env("TAILWINDCSS_BIN", fixture.root.join("bin/tailwindcss"))
        .env("TEST_ROOT", &fixture.root)
        .env("VMUX_BUILD_PROFILE", "release")
        .assert()
        .success();
    let output = std::fs::read_to_string(fixture.root.join("serve.log")).unwrap();
    assert_eq!(
        output,
        format!(
            "{}\nserve --platform web\ndev\nweb\n",
            fixture
                .root
                .join("website")
                .canonicalize()
                .unwrap()
                .display()
        )
    );
    assert!(!fixture.root.join("open.log").exists());
}

#[cfg(unix)]
struct Fixture {
    directory: tempfile::TempDir,
    root: std::path::PathBuf,
    target: std::path::PathBuf,
    _permit: Permit,
}

#[cfg(unix)]
impl Fixture {
    fn new() -> Self {
        let permit = Permit::acquire();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("checkout with spaces");
        let target = directory.path().join("target with spaces");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "tool-versions.env",
            "cef/manifest.toml",
            "crates/app/vmux_desktop/Cargo.toml",
            "packaging/macos/Info.plist",
            "packaging/macos/usage-descriptions.plist",
            "packaging/macos/VmuxDev.entitlements",
            "packaging/macos/Vmux.entitlements",
            "crates/util/vmux_vm/hypervisor.entitlements",
            "packaging/macos/Vmux.icns",
            "packaging/macos/tray.png",
            "packaging/macos/dmg-background.png",
            "packaging/macos/crash_reporter.cfg",
            "packaging/macos/ai.vmux.service.plist",
            "packaging/ios/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png",
            "packaging/ios/Assets.xcassets/AppIcon.appiconset/Contents.json",
            "packaging/ios/Info.plist",
            "packaging/ios/LaunchScreen.storyboard",
            "packaging/ios/PrivacyInfo.xcprivacy",
            "packaging/ios/Vmux.entitlements",
        ] {
            let destination = root.join(name);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(source.join(name), destination).unwrap();
        }
        std::fs::create_dir_all(root.join("crates/app/vmux_cli")).unwrap();
        std::fs::write(
            root.join("crates/app/vmux_cli/Cargo.toml"),
            "[package]\nname = \"vmux_cli\"\nversion = \"0.0.35\"\n",
        )
        .unwrap();
        let fixture = Self {
            directory,
            root,
            target,
            _permit: permit,
        };
        let git = std::process::Command::new("git")
            .arg("-C")
            .arg(&fixture.root)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        assert!(git.status.success());
        fixture.script("git", "#!/bin/sh\ncase \"$*\" in *'rev-parse --short=7 HEAD'*) printf '9999999\\n';; *) exec /usr/bin/git \"$@\";; esac\n");
        fixture.script("cargo", r#"#!/bin/sh
[ "${RUSTC_WRAPPER:-}" != sccache ] || [ "${CARGO_INCREMENTAL:-0}" != 1 ] || exit 73
case "$1" in
  -V|--version) printf 'cargo 1.90.0\n'; exit 0 ;;
  build|rustc)
    [ "${FAIL_BUILD:-0}" != 1 ] || exit 42
    if [ -n "${TEST_BUILD_ENV:-}" ]; then printf '%s' "${CEF_PATH:-}" > "$TEST_BUILD_ENV"; fi
    case "$*" in *--release*) profile=release;; *cef-helper*) profile=cef-helper;; *) profile=debug;; esac
    output="$CARGO_TARGET_DIR/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}$profile"
    mkdir -p "$output"
    for binary in vmux vmux_vm vmux_service vmux_desktop bevy_cef_debug_render_process; do
      printf '#!/bin/sh\nexit 0\n' > "$output/$binary"
      chmod +x "$output/$binary"
    done
    printf '%s\n' "$*" >> "$TEST_BUILD_LOG"
    ;;
  packager)
    [ "${FAIL_PACKAGE:-0}" != 1 ] || exit 43
    printf '%s\n' "$*" >> "$TEST_BUILD_LOG"
    "$VMUX_DX_BIN" internal package-binaries || exit $?
    python3 - "$3" <<'PY'
import json, pathlib, plistlib, sys
config = json.loads(sys.argv[1])
app = pathlib.Path(config["out-dir"]) / (config["product-name"] + ".app")
(app / "Contents/MacOS").mkdir(parents=True)
(app / "Contents/Frameworks").mkdir()
for binary in config["binaries"]:
    name = binary["path"]
    (app / "Contents/MacOS" / name).write_text("fixture executable " + name)
(app / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":config["identifier"], "CFBundleDisplayName":config["product-name"], "CFBundleName":config["product-name"]}))
PY
    ;;
  *) exit 0 ;;
esac
"#);
        fixture.script("rustup", "#!/bin/sh\nprintf 'rustup 1.28.0\\n'\n");
        fixture.script("rustc", "#!/bin/sh\nprintf 'rustc 1.90.0 fixture\\n'\n");
        fixture.script("curl", "#!/bin/sh\nexit 99\n");
        fixture.script("tailwindcss", "#!/bin/sh\nprintf 'tailwindcss v4.2.4\\n'\n");
        fixture.script("lipo", "#!/bin/sh\nprintf 'arm64\\n'\n");
        fixture.script("otool", "#!/bin/sh\nprintf 'fixture (compatibility version 1520.0.6, current version 1520.0.6)\\n'\n");
        fixture.script(
            "nm",
            "#!/bin/sh\nprintf '000 T _cef_set_os_crypt_keys\\n'\n",
        );
        fixture.script("file", "#!/bin/sh\nprintf 'Mach-O fixture\\n'\n");
        fixture.script("install_name_tool", "#!/bin/sh\nexit 0\n");
        fixture.script("codesign", "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$TEST_SIGN_LOG\"\n[ \"${FAIL_SIGNATURE:-0}\" != 1 ] || exit 44\n[ \"$1\" != --display ] || printf certificate > codesign0\n");
        fixture.script(
            "security",
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$TEST_SECURITY_LOG"
case "$1" in
  list-keychains) [ "${4:-}" = -s ] || printf '"login.keychain-db"\n"system.keychain"\n' ;;
  find-identity) printf '1) fixture "fixture"\n' ;;
  import) [ "${FAIL_IMPORT:-0}" != 1 ] || exit 45 ;;
esac
"#,
        );
        fixture.script("open", "#!/bin/sh\nfor arg do app=$arg; done\ntest -f \"$app/Contents/MacOS/vmux_desktop\" || exit 1\nprintf '%s\\n' \"$app\" >> \"$TEST_OPEN_LOG\"\n");
        fixture.script("lsof", "#!/bin/sh\nprintf 'p%s\\nn/\\n' $$\n");
        fixture.script("ps", "#!/bin/sh\nprintf '%s fixture\\n' $$\n");
        fixture.script(
            "cargo-packager",
            "#!/bin/sh\nprintf 'cargo-packager 0.11.9\\n'\n",
        );
        fixture.script("bevy_cef_bundle_app", r#"#!/bin/sh
if [ "$1" = --version ]; then printf 'bevy_cef_bundle_app 0.8.1\n'; exit 0; fi
while [ "$#" -gt 0 ]; do case "$1" in --app) app=$2; shift;; --cef-framework) framework=$2; shift;; esac; shift; done
cp -R "$framework" "$app/Contents/Frameworks/"
for helper in "vmux_desktop Helper" "vmux_desktop Helper (Renderer)"; do
    contents="$app/Contents/Frameworks/$helper.app/Contents"
    mkdir -p "$contents/MacOS"
    printf '%s' "$helper" > "$contents/MacOS/$helper"
    printf '%s' '<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>fixture.helper</string></dict></plist>' > "$contents/Info.plist"
done
"#);
        let framework = fixture
            .directory
            .path()
            .join("Chromium Embedded Framework.framework");
        std::fs::create_dir_all(framework.join("Resources/en.lproj")).unwrap();
        std::fs::create_dir_all(framework.join("Libraries")).unwrap();
        std::fs::write(framework.join("Chromium Embedded Framework"), "fixture").unwrap();
        std::fs::set_permissions(
            framework.join("Chromium Embedded Framework"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        for name in [
            "bevy_cef_core-0.5.2",
            "bevy_cef_debug_render_process-0.5.2",
            "bevy_window-0.19.0",
        ] {
            std::fs::create_dir_all(fixture.root.join("patches").join(name)).unwrap();
        }
        let dist = fixture.root.join("crates/util/vmux_ui/dist");
        let mut stamp = String::new();
        for name in [
            "assets/index.css",
            "assets/theme.css",
            "assets/favicons/tools.svg",
            "assets/tool-logos/acp.png",
            "assets/tool-logos/homebrew.svg",
            "assets/tool-logos/mcp.png",
            "assets/tool-logos/npm.svg",
        ] {
            let path = dist.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, name).unwrap();
            let output = std::process::Command::new("shasum")
                .args(["-a", "256"])
                .arg(path)
                .output()
                .unwrap();
            assert!(output.status.success());
            let hash = std::str::from_utf8(&output.stdout)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap();
            stamp.push_str(&format!("{hash}  {name}\n"));
        }
        std::fs::write(dist.join(".bundle-stamp"), stamp).unwrap();
        fixture.script("plist-buddy", "#!/bin/sh\nexit 0\n");
        fixture
    }

    fn script(&self, name: &str, source: &str) {
        let path = self.root.join("bin").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(any(target_os = "macos", not(feature = "runtime")))]
    fn native(&self, program: &str) -> std::process::Command {
        let mut command = std::process::Command::new(program);
        command.current_dir(&self.root).env(
            "PATH",
            format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
        );
        command
    }

    fn bootstrap(&self) {
        for (group, name) in [
            ("feature", "vmux_dx"),
            ("app", "vmux_cli"),
            ("util", "vmux_ecs"),
            ("util", "vmux_api"),
            ("util", "vmux_task"),
            ("util", "vmux_macro"),
        ] {
            let path = self.root.join("crates").join(group).join(name);
            std::fs::create_dir_all(path.join("src")).unwrap();
            std::fs::write(
                path.join("Cargo.toml"),
                format!("[package]\nname = '{name}'\nversion = '0.0.0'\n"),
            )
            .unwrap();
            std::fs::write(path.join("src/lib.rs"), "pub struct BootstrapFixture;\n").unwrap();
        }
        self.script(
            "cargo",
            r#"#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do case "$1" in --target-dir) target=$2; shift;; esac; shift; done
mkdir -p "$target/debug"
cp "$BOOTSTRAP_ROOT/handoff" "$target/debug/vmux"
chmod 755 "$target/debug/vmux"
printf compiled > "$BOOTSTRAP_ROOT/bootstrap-compiled"
"#,
        );
        std::fs::write(
            self.root.join("handoff"),
            r#"#!/bin/sh
set -eu
for lock in "$VMUX_TARGET_LOCK_ROOT"/*.lock; do [ ! -s "$lock" ] || exit 29; done
printf '%s\n' "$VMUX_DX_HANDOFF" "$BOOTSTRAP_ORIGINAL" "$@"
"#,
        )
        .unwrap();
    }

    #[cfg(target_os = "macos")]
    fn ios_tools(&self) {
        self.script(
            "plist-buddy",
            "#!/bin/sh\nexec /usr/libexec/PlistBuddy \"$@\"\n",
        );
        self.script("xcrun", r#"#!/usr/bin/env python3
import pathlib,plistlib,shutil,sys
args=sys.argv[1:]
if '--show-sdk-version' in args:
    print('18.0'); raise SystemExit
if '--show-sdk-build-version' in args:
    print('22A'); raise SystemExit
output=pathlib.Path(args[args.index('--compile')+1])
if args[0]=='actool':
    output.mkdir(parents=True,exist_ok=True)
    selected=pathlib.Path(args[1])/'AppIcon.appiconset/AppIcon-1024.png'
    shutil.copyfile(selected,output/'Assets.car')
    shutil.copyfile(selected,output/'AppIcon60x60@2x.png')
    with open(args[args.index('--output-partial-info-plist')+1],'wb') as target:
        plistlib.dump({'CFBundleIcons':{'CFBundlePrimaryIcon':{'CFBundleIconName':'AppIcon'}}},target)
elif args[0]=='ibtool':
    shutil.copyfile(args[1],output)
else:
    raise SystemExit('unsupported xcrun fixture arguments')
"#);
        self.script(
            "xcodebuild",
            "#!/bin/sh\nprintf 'Xcode 16.1\\nBuild version 16B40\\n'\n",
        );
        self.script("sw_vers", "#!/bin/sh\nprintf '24A\\n'\n");
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("vmux").unwrap();
        command.timeout(std::time::Duration::from_secs(30));
        command
            .current_dir(&self.root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("CARGO_TARGET_DIR", &self.target)
            .env(
                "CEF_FRAMEWORK_DIR",
                self.directory
                    .path()
                    .join("Chromium Embedded Framework.framework"),
            )
            .env(
                "VMUX_CEF_HELPER_CACHE",
                self.directory.path().join("helper-cache"),
            )
            .env("VMUX_CEF_SDK_CACHE", self.directory.path().join("cef-sdk"))
            .env("APPLE_SIGNING_IDENTITY", "fixture")
            .env("PLIST_BUDDY", self.root.join("bin/plist-buddy"))
            .env("VMUX_BUILD_STORAGE_MIN_GIB", "0")
            .env("VMUX_BUILD_STORAGE_WARN_GIB", "0")
            .env("VMUX_BUILD_STORAGE_CACHE_GIB", "1000000")
            .env("VMUX_DISABLE_SCCACHE", "1")
            .env("SKIP_NOTARIZE", "1")
            .env("TEST_BUILD_LOG", self.root.join("build.log"))
            .env("TEST_SECURITY_LOG", self.root.join("security.log"))
            .env("TEST_SIGN_LOG", self.root.join("signing.log"))
            .env("TEST_OPEN_LOG", self.root.join("open.log"));
        for key in [
            "VMUX_TARGET_LOCK_OWNER_PID",
            "VMUX_TARGET_LOCK_TARGET",
            "VMUX_BUILD_STORAGE_ROOT",
            "VMUX_BUILD_STORAGE_TARGET",
            "CARGO_BUILD_TARGET",
            "VMUX_DATA_DIR",
            "APPLE_CERTIFICATE",
            "APPLE_CERTIFICATE_PASSWORD",
        ] {
            command.env_remove(key);
        }
        command
    }
}

#[test]
#[cfg(unix)]
fn cleanup_preview_reports_the_preserved_cache_usage() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.target.join("debug/.fingerprint")).unwrap();
    std::fs::write(
        fixture.target.join("debug/.fingerprint/current"),
        "warm cache",
    )
    .unwrap();
    std::fs::write(
        fixture.target.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55",
    )
    .unwrap();
    fixture.script(
        "du",
        "#!/bin/sh\nshift\nfor path do printf '1048576\\t%s\\n' \"$path\"; done\n",
    );
    let before = fixture
        .command()
        .args(["cache", "status"])
        .output()
        .unwrap();
    assert!(before.status.success());
    let before = String::from_utf8(before.stdout).unwrap();
    let summary = before
        .lines()
        .find(|line| line.contains("GiB of compile caches"))
        .unwrap();
    let (_, usage) = summary.split_once(", ").unwrap();
    fixture
        .command()
        .env_remove("VMUX_BUILD_STORAGE_APPLY")
        .args(["cache", "clean"])
        .assert()
        .success()
        .stdout(predicate::str::contains(usage));
    assert_eq!(
        std::fs::read_to_string(fixture.target.join("debug/.fingerprint/current")).unwrap(),
        "warm cache"
    );
}

#[test]
#[cfg(unix)]
fn unsafe_cargo_target_is_refused_before_seed_or_tool_execution() {
    let fixture = Fixture::new();
    let target = fixture.directory.path().join("target-link");
    std::os::unix::fs::symlink(&fixture.target, &target).unwrap();
    fixture
        .command()
        .env("CARGO_TARGET_DIR", &target)
        .args(["internal", "cargo", "--", "build"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "dedicated, nonsymlink Cargo target directory",
        ));
    assert!(!fixture.root.join("build.log").exists());
    assert!(!fixture.root.join(".vmux-seed").exists());
    assert!(fixture.root.join("Cargo.toml").is_file());
}

#[test]
#[cfg(unix)]
fn bootstrap_releases_its_lock_and_preserves_original_environment_and_arguments() {
    let fixture = Fixture::new();
    fixture.bootstrap();
    std::fs::write(
        fixture.root.join(".env"),
        "BOOTSTRAP_ORIGINAL=from-dotenv\n",
    )
    .unwrap();
    fixture
        .command()
        .env_remove("VMUX_DX_HANDOFF")
        .env("BOOTSTRAP_ROOT", &fixture.root)
        .env("BOOTSTRAP_ORIGINAL", "from-os")
        .env(
            "VMUX_TARGET_LOCK_ROOT",
            fixture.directory.path().join("bootstrap-locks"),
        )
        .args(["doctor", "--root", fixture.root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "1\nfrom-os\ndoctor\n--root\n{}\n",
            fixture.root.display()
        )));
    assert!(fixture.root.join("bootstrap-compiled").is_file());
}

#[test]
#[cfg(unix)]
fn bootstrap_refuses_fifo_compiler_inputs_without_waiting_for_a_writer() {
    let fixture = Fixture::new();
    fixture.bootstrap();
    let lock = fixture.root.join("Cargo.lock");
    std::fs::remove_file(&lock).unwrap();
    let path = std::ffi::CString::new(lock.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    fixture
        .command()
        .env_remove("VMUX_DX_HANDOFF")
        .args(["doctor", "--root", fixture.root.to_str().unwrap()])
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "CLI compiler input must be a regular file",
        ));
    assert!(!fixture.root.join("bootstrap-compiled").exists());
}

#[test]
#[cfg(unix)]
fn bootstrap_rechecks_capacity_under_its_lock_before_compilation() {
    let fixture = Fixture::new();
    fixture.bootstrap();
    fixture.script("df", r#"#!/bin/sh
if [ -e "$BOOTSTRAP_ROOT/capacity-checked" ]; then free=0; else
    touch "$BOOTSTRAP_ROOT/capacity-checked"; free=1000000000
fi
printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\nfixture 1000000000 0 %s 0%% /\n' "$free"
"#);
    fixture
        .command()
        .env_remove("VMUX_DX_HANDOFF")
        .env("BOOTSTRAP_ROOT", &fixture.root)
        .env("VMUX_BUILD_STORAGE_MIN_GIB", "1")
        .env("VMUX_BUILD_STORAGE_WARN_GIB", "1")
        .env(
            "VMUX_TARGET_LOCK_ROOT",
            fixture.directory.path().join("bootstrap-locks"),
        )
        .args(["doctor", "--root", fixture.root.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Insufficient build space"));
    assert!(!fixture.root.join("bootstrap-compiled").exists());
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn bootstrap_compile_cancellation_drains_before_releasing_the_lock() {
    let fixture = Fixture::new();
    fixture.bootstrap();
    fixture.script(
        "cargo",
        r#"#!/bin/sh
printf '%s' "$$" > "$BOOTSTRAP_ROOT/bootstrap-started"
trap '' INT TERM
sleep 60
"#,
    );
    let mut command = fixture.native(env!("CARGO_BIN_EXE_vmux"));
    command
        .env_remove("VMUX_DX_HANDOFF")
        .env("BOOTSTRAP_ROOT", &fixture.root)
        .env(
            "VMUX_TARGET_LOCK_ROOT",
            fixture.directory.path().join("bootstrap-locks"),
        )
        .args(["doctor", "--root", fixture.root.to_str().unwrap()]);
    let mut child = WorkspaceChild(command.spawn().unwrap());
    let marker = fixture.root.join("bootstrap-started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "bootstrap exited before compile"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "bootstrap compile did not start"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pid: i32 = std::fs::read_to_string(marker).unwrap().parse().unwrap();
    unsafe {
        libc::kill(child.0.id() as i32, libc::SIGINT);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "bootstrap did not drain"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(130));
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert!(!fixture.root.join("bootstrap-compiled").exists());
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn git_configuration_failures_do_not_count_as_missing_core_worktree() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["cache", "-b", "main", "status"])
        .assert()
        .success();
    fixture.script(
        "git",
        r#"#!/bin/sh
case "$*" in
  'config --local --get core.worktree') exit 73 ;;
  *) exec /usr/bin/git "$@" ;;
esac
"#,
    );
    fixture
        .command()
        .args(["cache", "-b", "main", "status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Git repository read failed"))
        .stderr(predicate::str::contains("73"));
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn resolved_launch_cancellation_drains_a_tool_that_ignores_the_signal() {
    let fixture = Fixture::new();
    let app = fixture
        .target
        .join("release/Vmux (fixture).app/Contents/MacOS");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("vmux_desktop"), "fixture").unwrap();
    fixture.script(
        "codesign",
        r#"#!/bin/sh
trap '' TERM INT
printf '%s' $$ > "$TEST_ROOT/tool-started"
sleep 60 &
printf '%s' "$!" > "$TEST_ROOT/tool-child.pending"
mv "$TEST_ROOT/tool-child.pending" "$TEST_ROOT/tool-child"
wait
"#,
    );
    let mut child = WorkspaceChild(
        std::process::Command::new(env!("CARGO_BIN_EXE_vmux"))
            .current_dir(&fixture.root)
            .args(["run", "-p", "local"])
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", fixture.root.join("bin").display()),
            )
            .env("CARGO_TARGET_DIR", &fixture.target)
            .env_remove("CARGO_BUILD_TARGET")
            .env("TEST_ROOT", &fixture.root)
            .env("TEST_OPEN_LOG", fixture.root.join("open.log"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !fixture.root.join("tool-child").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "signature verification never started"
        );
        assert!(child.0.try_wait().unwrap().is_none());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pid: i32 = std::fs::read_to_string(fixture.root.join("tool-started"))
        .unwrap()
        .parse()
        .unwrap();
    let _cleanup = OwnedGroup(pid);
    let descendant: i32 = std::fs::read_to_string(fixture.root.join("tool-child"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGTERM) }, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(143));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "CLI did not finish cancellation"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    while unsafe { libc::kill(descendant, 0) } == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "owned tool survived CLI exit"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!fixture.root.join("open.log").exists());
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn workspace_capacity_is_rechecked_before_compilation() {
    let fixture = WorkspaceFixture::new();
    fixture.tool("df", r#"
if [ -e "$VMUX_WORKSPACE_TEST_ROOT/capacity-first" ]; then free=0; else
    touch "$VMUX_WORKSPACE_TEST_ROOT/capacity-first"
    free=2097152
fi
printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\nfixture 2097152 0 %s 0%% /\n' "$free"
"#);
    fixture
        .run(&["fixtures", "echo"])
        .env("VMUX_BUILD_STORAGE_MIN_GIB", "1")
        .env("VMUX_BUILD_STORAGE_WARN_GIB", "1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Insufficient build space"));
    assert!(!fixture.root.join("compiled").exists());
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn workspace_capacity_probe_cancels_and_drains_before_exit() {
    let fixture = WorkspaceFixture::new();
    fixture.tool(
        "df",
        r#"
trap '' TERM INT
printf '%s' $$ > "$VMUX_WORKSPACE_TEST_ROOT/probe.pending"
mv "$VMUX_WORKSPACE_TEST_ROOT/probe.pending" "$VMUX_WORKSPACE_TEST_ROOT/probe-started"
exec sleep 60
"#,
    );
    let mut command = fixture.command();
    command
        .args(["fixtures", "echo"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = WorkspaceChild(command.spawn().unwrap());
    let marker = fixture.root.join("probe-started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "capacity probe never started"
        );
        assert!(child.0.try_wait().unwrap().is_none());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pid: i32 = std::fs::read_to_string(marker).unwrap().parse().unwrap();
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGINT) }, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(130));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "capacity probe did not drain"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    assert!(!fixture.root.join("compiled").exists());
}

#[test]
#[cfg(all(target_os = "macos", not(feature = "runtime")))]
fn debug_fifo_inputs_are_refused_before_launching() {
    for name in [".vmux-dev-ready", "vmux"] {
        let fixture = Fixture::new();
        fixture.command().args(["build"]).assert().success();
        let path = fixture.target.join("debug").join(name);
        std::fs::remove_file(&path).unwrap();
        let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        fixture
            .command()
            .args(["run"])
            .timeout(std::time::Duration::from_secs(10))
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "Debug input must be a regular file",
            ));
        assert!(!fixture.root.join("open.log").exists());
    }
}

#[cfg(all(unix, not(feature = "runtime")))]
struct OwnedGroup(i32);

#[cfg(all(unix, not(feature = "runtime")))]
impl Drop for OwnedGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

#[test]
#[cfg(all(unix, not(feature = "runtime")))]
fn repository_read_cancellation_preserves_the_signal_and_prevents_following_stages() {
    let fixture = Fixture::new();
    fixture.script(
        "git",
        r#"#!/bin/sh
case "$*" in
  'worktree list --porcelain -z')
    printf '%s' $$ > "$TEST_ROOT/git-started"
    sleep 60 &
    printf '%s' "$!" > "$TEST_ROOT/git-descendant.pending"
    mv "$TEST_ROOT/git-descendant.pending" "$TEST_ROOT/git-descendant"
    wait
    ;;
  *)
    printf '%s\n' "$*" >> "$TEST_ROOT/git-following"
    exec /usr/bin/git "$@"
    ;;
esac
"#,
    );
    let mut child = WorkspaceChild(
        std::process::Command::new(env!("CARGO_BIN_EXE_vmux"))
            .current_dir(&fixture.root)
            .args(["cache", "-b", "main", "status"])
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", fixture.root.join("bin").display()),
            )
            .env("CARGO_TARGET_DIR", &fixture.target)
            .env("TEST_ROOT", &fixture.root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !fixture.root.join("git-descendant").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "Git read never started"
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "CLI exited before its Git read"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let descendant: i32 = std::fs::read_to_string(fixture.root.join("git-descendant"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGTERM) }, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(143));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "CLI did not process cancellation"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    while unsafe { libc::kill(descendant, 0) } == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "Git descendant survived cancellation"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let following = fixture.root.join("git-following");
    if following.exists() {
        let commands = std::fs::read_to_string(following).unwrap();
        assert!(!commands.contains("--git-common-dir"));
        assert!(!commands.contains("core.worktree"));
    }
}

#[test]
#[cfg(target_os = "macos")]
fn signing_identity_provisioning_verifies_creation_and_stops_after_import_failure() {
    for fail in [false, true] {
        let fixture = Fixture::new();
        fixture.script(
            "openssl",
            r#"#!/bin/sh
next=
for arg do
  if [ -n "$next" ]; then printf fixture > "$arg"; next=; continue; fi
  case "$arg" in -out|-keyout) next=yes;; esac
done
"#,
        );
        fixture.script("security", r#"#!/bin/sh
case "$1" in
  find-identity) if [ -f "$TEST_ROOT/created" ]; then printf '1) fixture "Fixture Identity"\n'; fi ;;
  import) [ "${FAIL_IMPORT:-0}" != 1 ] || exit 45; test -f "$2" || exit 46; touch "$TEST_ROOT/created" ;;
  add-trusted-cert) test -f "$TEST_ROOT/created" || exit 47; touch "$TEST_ROOT/trusted" ;;
esac
"#);
        let mut command = fixture.command();
        command
            .args(["internal", "ensure-signing"])
            .env_remove("APPLE_SIGNING_IDENTITY")
            .env("VMUX_LOCAL_SIGNING_IDENTITY", "Fixture Identity")
            .env(
                "VMUX_LOCAL_CODESIGN_KEYCHAIN",
                fixture.root.join("fixture.keychain-db"),
            )
            .env("TEST_ROOT", &fixture.root)
            .env("FAIL_IMPORT", if fail { "1" } else { "0" });
        if fail {
            command.assert().code(45);
            assert!(!fixture.root.join("trusted").exists());
        } else {
            command
                .assert()
                .success()
                .stdout(predicate::str::contains("Fixture Identity"));
            assert!(fixture.root.join("created").is_file());
            assert!(fixture.root.join("trusted").is_file());
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn debug_bundle_preserves_metadata_and_reuses_signatures_until_inputs_change() {
    let fixture = Fixture::new();
    let debug = fixture.target.join("debug");
    std::fs::create_dir_all(&debug).unwrap();
    for name in ["vmux", "vmux_vm", "vmux_service", "vmux_desktop"] {
        std::fs::write(debug.join(name), format!("#!/bin/sh\nprintf '{name}'\n")).unwrap();
        std::fs::set_permissions(debug.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fixture.script(
        "plist-buddy",
        "#!/bin/sh\nexec /usr/libexec/PlistBuddy \"$@\"\n",
    );
    fixture.script("rustc", "#!/bin/sh\nprintf '%s\\n' \"$TEST_STDLIB\"\n");
    fixture.script(
        "codesign",
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$TEST_SIGN_LOG\"\n",
    );
    let stdlib = fixture.directory.path().join("stdlib with spaces");
    std::fs::create_dir_all(&stdlib).unwrap();
    let log = fixture.root.join("signatures.log");
    for _ in 0..2 {
        fixture
            .command()
            .args(["internal", "debug-bundle"])
            .env("TEST_STDLIB", &stdlib)
            .env("TEST_SIGN_LOG", &log)
            .assert()
            .success();
    }
    let app = debug.join("Vmux Dev.app");
    let plist = plist::Value::from_file(app.join("Contents/Info.plist")).unwrap();
    let dictionary = plist.as_dictionary().unwrap();
    assert_eq!(
        dictionary["CFBundleIdentifier"].as_string(),
        Some("ai.vmux.desktop.dev")
    );
    assert_eq!(
        dictionary["CFBundleDisplayName"].as_string(),
        Some("Vmux Dev (main)")
    );
    let service = app.join("Contents/Library/LoginItems/Vmux Service.app");
    let plist = plist::Value::from_file(service.join("Contents/Info.plist")).unwrap();
    let dictionary = plist.as_dictionary().unwrap();
    assert_eq!(
        dictionary["CFBundleExecutable"].as_string(),
        Some("Vmux Service")
    );
    assert_eq!(dictionary["LSUIElement"].as_boolean(), Some(true));
    assert_eq!(
        std::fs::read(app.join("Contents/MacOS/vmux_desktop")).unwrap(),
        std::fs::read(debug.join("vmux_desktop")).unwrap()
    );
    let first = std::fs::read_to_string(&log).unwrap();
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("--force"))
            .count(),
        2
    );
    std::fs::write(debug.join("vmux_desktop"), "#!/bin/sh\nprintf 'rebuilt'\n").unwrap();
    fixture
        .command()
        .args(["internal", "debug-bundle"])
        .env("TEST_STDLIB", &stdlib)
        .env("TEST_SIGN_LOG", &log)
        .assert()
        .success();
    let after = std::fs::read_to_string(&log).unwrap();
    assert_eq!(
        after
            .lines()
            .filter(|line| line.starts_with("--force"))
            .count(),
        3
    );
    assert_eq!(
        std::fs::read(app.join("Contents/MacOS/vmux_desktop")).unwrap(),
        std::fs::read(debug.join("vmux_desktop")).unwrap()
    );
}

#[test]
#[cfg(target_os = "macos")]
fn native_launch_refusal_keeps_the_tool_diagnostic_and_exit_status() {
    let fixture = Fixture::new();
    let app = fixture
        .target
        .join("release/Vmux (fixture).app/Contents/MacOS");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("vmux_desktop"), "fixture").unwrap();
    fixture.script(
        "codesign",
        "#!/bin/sh\nprintf 'fixture signature permission denied\\n' >&2\nexit 44\n",
    );
    fixture
        .command()
        .args(["run", "-p", "local"])
        .assert()
        .code(44)
        .stderr(predicate::str::contains(
            "fixture signature permission denied",
        ));
    assert!(!fixture.root.join("open.log").exists());
}

#[test]
#[cfg(target_os = "macos")]
fn local_run_opens_the_newest_completed_product_bundle_and_forwards_test_environment() {
    let fixture = Fixture::new();
    let release = fixture.target.join("release");
    std::fs::create_dir_all(&release).unwrap();
    for (name, timestamp) in [
        ("Vmux (older).app", 1),
        ("Vmux (newer).app", 2),
        ("Vmux.app", 3),
        ("Other (newest).app", 4),
    ] {
        let app = release.join(name);
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/MacOS/vmux_desktop"), "fixture").unwrap();
        std::fs::File::open(&app)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp),
            ))
            .unwrap();
    }
    std::os::unix::fs::symlink(release.join("Vmux.app"), release.join("Vmux (symlink).app"))
        .unwrap();
    fixture.script(
        "open",
        "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$TEST_OPEN_LOG\"\n",
    );
    let data = fixture.directory.path().join("copied store");
    std::fs::write(fixture.root.join(".env"), "VMUX_PROFILE=from-file\nVMUX_DATA_DIR=/file-data-must-not-win\nVMUX_REMOTE_RELAY_URL=https://production.invalid:443\n").unwrap();
    for command in ["run", "desktop"] {
        fixture
            .command()
            .args([command, "main", "-p", "local"])
            .env("VMUX_PROFILE", "work")
            .env_remove("VMUX_TEST")
            .env("VMUX_DATA_DIR", &data)
            .env("VMUX_REMOTE_RELAY_URL", "https://localhost:8787")
            .assert()
            .success();
        let output = std::fs::read(fixture.root.join("open.log")).unwrap();
        let actual = output
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
            .map(|value| std::str::from_utf8(value).unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                "--env".to_owned(),
                "VMUX_PROFILE=work".into(),
                "--env".into(),
                format!("VMUX_DATA_DIR={}", data.display()),
                "--env".into(),
                "VMUX_REMOTE_RELAY_URL=https://localhost:8787".into(),
                release
                    .join("Vmux (newer).app")
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string(),
            ]
        );
        assert!(!fixture.root.join("build.log").exists());
    }
}

#[test]
#[cfg(target_os = "macos")]
fn packaged_artifacts_are_verified_before_publication_and_failed_builds_preserve_existing_outputs()
{
    let fixture = Fixture::new();
    let app = fixture.target.join("release/Vmux (9999999).app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("sentinel"), "previous artifact").unwrap();
    for failure in ["FAIL_PACKAGE", "FAIL_SIGNATURE"] {
        fixture
            .command()
            .args(["build", "-p", "local"])
            .env(failure, "1")
            .assert()
            .failure();
        assert_eq!(
            std::fs::read_to_string(app.join("sentinel")).unwrap(),
            "previous artifact"
        );
    }
    fixture
        .command()
        .args(["build", "-p", "local"])
        .assert()
        .success();
    assert!(!app.join("sentinel").exists());
    assert!(app.join("Contents/MacOS/vmux_vm").is_file());
    let signing = std::fs::read_to_string(fixture.root.join("signing.log")).unwrap();
    assert!(
        signing
            .lines()
            .any(|line| line.contains("hypervisor.entitlements")
                && line.ends_with("Contents/MacOS/vmux_vm"))
    );
    assert!(
        app.join("Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service")
            .is_file()
    );
    let build_log = std::fs::read(fixture.root.join("build.log")).unwrap();
    fixture
        .command()
        .args(["run", "-p", "local"])
        .assert()
        .success();
    assert_eq!(
        std::fs::read(fixture.root.join("build.log")).unwrap(),
        build_log
    );
}

#[test]
#[cfg(target_os = "macos")]
fn debug_vm_helpers_are_verified_and_rebundling_preserves_open_executables() {
    let fixture = Fixture::new();
    fixture.command().args(["build"]).assert().success();
    let debug = fixture.target.join("debug");
    let app = debug.join("Vmux Dev.app");
    for (source, relative) in [
        ("vmux_vm", "Contents/MacOS/vmux_vm"),
        (
            "vmux_service",
            "Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
        ),
        ("vmux_desktop", "Contents/MacOS/vmux_desktop"),
    ] {
        let bundled = app.join(relative);
        let previous = std::fs::read(&bundled).unwrap();
        let mut mapped = std::fs::File::open(&bundled).unwrap();
        let changed = format!("changed {source}\n");
        std::fs::write(debug.join(source), changed.as_bytes()).unwrap();
        fixture
            .command()
            .args(["internal", "debug-sign"])
            .assert()
            .success();
        fixture
            .command()
            .args(["internal", "debug-bundle"])
            .assert()
            .success();
        let mut retained = Vec::new();
        mapped.read_to_end(&mut retained).unwrap();
        assert_eq!(retained, previous);
        assert_eq!(std::fs::read(bundled).unwrap(), changed.as_bytes());
    }
    let missing = Fixture::new();
    missing.command().args(["build"]).assert().success();
    std::fs::remove_file(
        missing
            .target
            .join("debug/Vmux Dev.app/Contents/MacOS/vmux_vm"),
    )
    .unwrap();
    missing.command().args(["run"]).assert().failure();
    assert!(!missing.root.join("open.log").exists());
}

#[test]
#[cfg(target_os = "macos")]
fn signing_keychain_is_restored_even_when_certificate_import_fails() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["build", "-p", "local"])
        .env("APPLE_CERTIFICATE", "QUJD")
        .env("APPLE_CERTIFICATE_PASSWORD", "fixture-password")
        .env("FAIL_IMPORT", "1")
        .assert()
        .code(45);
    let log = std::fs::read_to_string(fixture.root.join("security.log")).unwrap();
    assert!(log.contains("list-keychains -d user -s login.keychain-db system.keychain"));
    assert!(log.lines().last().unwrap().starts_with("delete-keychain "));
    assert!(!fixture.root.join("build.log").exists());
}

#[test]
#[cfg(target_os = "macos")]
fn package_callbacks_prepare_the_configured_cef_sdk_cache_for_cargo() {
    let fixture = Fixture::new();
    let log = fixture.root.join("cargo-environment");
    fixture
        .command()
        .args(["internal", "package-binaries"])
        .env("TEST_BUILD_ENV", &log)
        .env_remove("CEF_PATH")
        .assert()
        .success();
    let cache = fixture.directory.path().join("cef-sdk");
    assert_eq!(
        std::fs::read_to_string(log).unwrap(),
        cache.display().to_string()
    );
    assert!(cache.is_dir());
}

#[test]
#[cfg(target_os = "macos")]
fn package_callbacks_refuse_missing_bundle_paths_before_mutation() {
    let fixture = Fixture::new();
    let sentinel = fixture
        .root
        .join("Contents/Resources/webview-apps/sentinel");
    std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    std::fs::write(&sentinel, "keep").unwrap();
    for task in ["before-package", "finalize-package"] {
        fixture
            .command()
            .args(["internal", task])
            .env("CARGO_PACKAGER_FORMAT", "dmg")
            .env_remove("VMUX_APP_BUNDLE")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Missing VMUX_APP_BUNDLE"));
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "keep");
    }
    fixture
        .command()
        .args(["internal", "verify-bundle"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing app bundle path"));
}

#[test]
#[cfg(target_os = "macos")]
fn package_rejects_corrupted_copied_web_assets_and_preserves_previous_release() {
    let fixture = Fixture::new();
    fixture.script(
        "cp",
        r#"#!/bin/sh
/bin/cp "$@" || exit $?
for destination do :; done
case "$destination" in
  */webview-apps/_shared) printf corrupt > "$destination/assets/index.css" ;;
esac
"#,
    );
    let app = fixture.target.join("release/Vmux (9999999).app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("sentinel"), "previous artifact").unwrap();
    fixture
        .command()
        .args(["build", "-p", "local"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Stylesheet checksum or file coverage mismatch",
        ));
    assert_eq!(
        std::fs::read_to_string(app.join("sentinel")).unwrap(),
        "previous artifact"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn bundle_verification_refuses_a_missing_vm_helper() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["build", "-p", "local"])
        .assert()
        .success();
    let app = fixture.target.join("release/Vmux (9999999).app");
    fixture
        .command()
        .args(["internal", "verify-bundle", "--"])
        .arg(&app)
        .assert()
        .success();
    std::fs::remove_file(app.join("Contents/MacOS/vmux_vm")).unwrap();
    fixture
        .command()
        .args(["internal", "verify-bundle", "--"])
        .arg(&app)
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "macos")]
fn fast_ci_packages_publish_verified_signed_apps_with_native_resources() {
    for (event, reference, preflight) in [
        ("pull_request", "refs/pull/1/merge", ""),
        ("push", "refs/heads/feature", "true"),
    ] {
        let fixture = Fixture::new();
        fixture
            .command()
            .args(["build", "-p", "release"])
            .env("VMUX_CI_FAST_PACKAGE", "1")
            .env("GITHUB_EVENT_NAME", event)
            .env("GITHUB_REF", reference)
            .env("VMUX_CI_PREFLIGHT", preflight)
            .env("IS_RELEASE_PR", "0")
            .assert()
            .success();
        let app = fixture.target.join("release/Vmux.app");
        assert!(
            app.join("Contents/Frameworks/Chromium Embedded Framework.framework")
                .is_dir()
        );
        assert!(
            app.join("Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service")
                .is_file()
        );
        assert!(
            app.join("Contents/Library/LaunchAgents/ai.vmux.service.plist")
                .is_file()
        );
        let log = std::fs::read_to_string(fixture.root.join("build.log")).unwrap();
        assert!(
            log.lines()
                .any(|line| line.starts_with("packager ") && line.ends_with("--formats app"))
        );
        fixture
            .command()
            .args(["build", "-p", "release"])
            .env("VMUX_CI_FAST_PACKAGE", "1")
            .env("GITHUB_EVENT_NAME", event)
            .env("GITHUB_REF", reference)
            .env("VMUX_CI_PREFLIGHT", preflight)
            .env("IS_RELEASE_PR", "0")
            .env("FAIL_SIGNATURE", "1")
            .assert()
            .failure();
        assert!(
            app.join("Contents/Library/LaunchAgents/ai.vmux.service.plist")
                .is_file()
        );
    }
}

#[test]
#[cfg(unix)]
fn storage_refusal_happens_before_native_prerequisites_or_build_processes() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["build"])
        .env("VMUX_BUILD_STORAGE_MIN_GIB", "1000000")
        .env("VMUX_BUILD_STORAGE_WARN_GIB", "1000000")
        .assert()
        .failure();
    assert!(!fixture.root.join("build.log").exists());
    assert!(!fixture.root.join("security.log").exists());
}

#[test]
#[cfg(unix)]
fn nightly_versions_sort_between_the_current_and_next_stable_release() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args([
            "internal",
            "nightly-version",
            "--",
            "0.0.34",
            "20260921",
            "42",
            "1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value = std::str::from_utf8(&output.stdout).unwrap().trim();
    let nightly = semver::Version::parse(value).unwrap();
    assert!(nightly > semver::Version::parse("0.0.34").unwrap());
    assert!(nightly < semver::Version::parse("0.0.35").unwrap());
    for args in [
        ["0.0.34", "20260921", "042", "1"],
        ["0.0.34", "date", "42", "1"],
        ["not-semver", "20260921", "42", "1"],
    ] {
        fixture
            .command()
            .args(["internal", "nightly-version", "--"])
            .args(args)
            .assert()
            .failure();
    }
}

#[test]
#[cfg(unix)]
fn target_seed_identity_is_independent_of_a_symlinked_checkout_path() {
    let fixture = Fixture::new();
    let link = fixture.directory.path().join("linked checkout");
    std::os::unix::fs::symlink(&fixture.root, &link).unwrap();
    let first = fixture
        .command()
        .args(["internal", "target-seed-key"])
        .output()
        .unwrap();
    let second = fixture
        .command()
        .current_dir(link)
        .args(["internal", "target-seed-key"])
        .output()
        .unwrap();
    assert!(first.status.success() && second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
#[cfg(unix)]
fn target_relocation_updates_cef_metadata_without_rewriting_other_build_products() {
    let fixture = Fixture::new();
    let staging = fixture.directory.path().join("staging");
    let source = fixture.directory.path().join("source");
    let original = fixture.directory.path().join("original");
    let destination = fixture.directory.path().join("destination");
    let cache = staging.join("debug/build/cef-dll-sys-test/out/build/CMakeCache.txt");
    let fingerprint = staging
        .join("debug/.fingerprint/cef-dll-sys-test/run-build-script-build-script-build.json");
    let dep = staging.join("debug/deps/cef_dll_sys-test.d");
    let other = staging.join("debug/build/other/output");
    for path in [&cache, &fingerprint, &dep, &other] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("{}\n", original.display())).unwrap();
    }
    std::fs::write(
        &cache,
        format!(
            "CMAKE_CACHEFILE_DIR:INTERNAL={}/debug/build/cef-dll-sys-test/out/build\n",
            original.display()
        ),
    )
    .unwrap();
    let mut command = fixture.command();
    command
        .args(["internal", "relocate-target", "--"])
        .arg(&staging)
        .arg(&source)
        .arg(&destination)
        .assert()
        .success();
    for path in [&cache, &fingerprint, &dep] {
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains(&destination.display().to_string())
        );
    }
    assert_eq!(
        std::fs::read_to_string(other).unwrap(),
        format!("{}\n", original.display())
    );
}

#[test]
fn package_executables_have_one_main_binary_and_no_case_insensitive_name_collision() {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vmux_desktop/Cargo.toml");
    let manifest: toml::Table = std::fs::read_to_string(source).unwrap().parse().unwrap();
    let binaries = manifest["package"]["metadata"]["packager"]["binaries"]
        .as_array()
        .unwrap();
    let mut names = std::collections::BTreeSet::new();
    let mut main = None;
    for binary in binaries {
        let path = binary["path"].as_str().unwrap();
        assert!(
            names.insert(path.to_ascii_lowercase()),
            "case insensitive executable collision"
        );
        if binary.get("main").and_then(toml::Value::as_bool) == Some(true) {
            assert!(main.replace(path).is_none(), "multiple main executables");
        }
    }
    assert_eq!(main, Some("vmux_desktop"));
    assert_eq!(
        names,
        ["vmux", "vmux_desktop", "vmux_vm", "vmux service"]
            .into_iter()
            .map(String::from)
            .collect()
    );
}

#[test]
#[cfg(target_os = "macos")]
fn mobile_signing_matches_product_identity_and_cleans_up_after_refusal_or_failure() {
    for (matching_profile, signing_failure) in [(true, false), (false, false), (true, true)] {
        let project = Fixture::new();
        project.script(
            "security",
            r#"#!/usr/bin/env python3
import json,os,pathlib,plistlib,sys
args=sys.argv[1:]
with pathlib.Path('security.jsonl').open('a') as output:
    output.write(json.dumps(args)+'\n')
if args[0]=='list-keychains' and '-s' not in args:
    print('"/fixture/login.keychain-db"')
elif args[0]=='cms':
    plistlib.dump({'Entitlements':{'application-identifier':os.environ['FIXTURE_PROFILE_ID']}},sys.stdout.buffer)
elif args[0]=='find-identity':
    print('1) FIXTURE "Fixture Distribution"')
"#,
        );
        project.script(
            "dx",
            r#"#!/usr/bin/env python3
import os,pathlib,plistlib
app=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'dx/vmux_mobile/release/ios/VmuxMobile.app'
app.mkdir(parents=True,exist_ok=True)
(app/'vmux_mobile').write_text('binary')
(app/'_CodeSignature').mkdir(exist_ok=True)
with (app/'Info.plist').open('wb') as output:
    plistlib.dump({},output)
pathlib.Path('dx-built').write_text('built')
"#,
        );
        project.ios_tools();
        project.script(
            "codesign",
            r#"#!/usr/bin/env python3
import json,os,pathlib,plistlib,sys
args=sys.argv[1:]
if '--entitlements' in args:
    with open(args[args.index('--entitlements')+1],'rb') as source:
        entitlements=plistlib.load(source)
    pathlib.Path('signed.json').write_text(json.dumps({'args':args,'entitlements':entitlements}))
    if os.environ['FIXTURE_SIGN_FAILURE']=='1':
        raise SystemExit(47)
"#,
        );
        project.script(
            "ditto",
            r#"#!/usr/bin/env python3
import pathlib,sys,zipfile
with zipfile.ZipFile(sys.argv[-1],'w') as archive:
    for file in pathlib.Path('Payload').rglob('*'):
        if file.is_file():
            archive.write(file,str(file))
"#,
        );
        std::fs::write(
            project.root.join("custom.entitlements"),
            r#"<?xml version="1.0"?><plist version="1.0"><dict>
<key>get-task-allow</key><false/>
<key>nested</key><dict><key>identity</key><string>$(TeamIdentifierPrefix){{BUNDLE_ID}}</string></dict>
<key>keychain-access-groups</key><array><string>$(AppIdentifierPrefix)$(PRODUCT_BUNDLE_IDENTIFIER)</string><string>{{TEAM_ID}}.{{BUNDLE_ID}}</string></array>
</dict></plist>"#,
        )
        .unwrap();
        let output = project
            .command()
            .args(["internal", "ios-release"])
            .env("VMUX_HOST_NAME", "Atlas & Co")
            .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
            .env(
                "VMUX_HOST_MOBILE_ENTITLEMENTS",
                project.root.join("custom.entitlements"),
            )
            .env("APPLE_TEAM_ID", "FIXTURETEAM")
            .env("APPLE_IOS_SIGNING_IDENTITY", "Fixture Distribution")
            .env("APPLE_IOS_CERTIFICATE", "QUJD")
            .env("APPLE_IOS_CERTIFICATE_PASSWORD", "fixture-password")
            .env("APPLE_IOS_PROVISIONING_PROFILE", "QUJD")
            .env("DX_BIN", project.root.join("bin/dx"))
            .env("VMUX_IOS_UPLOAD", "0")
            .env(
                "FIXTURE_PROFILE_ID",
                if matching_profile {
                    "FIXTURETEAM.org.example.atlas.mobile"
                } else {
                    "FIXTURETEAM.ai.vmux.mobile"
                },
            )
            .env(
                "FIXTURE_SIGN_FAILURE",
                if signing_failure { "1" } else { "0" },
            )
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            matching_profile && !signing_failure,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(project.root.join("dx-built").exists(), matching_profile);
        assert_eq!(project.root.join("signed.json").exists(), matching_profile);
        let verification = project.native("python3").arg("-c").arg(r#"
import json,pathlib,sys,zipfile,plistlib
root=pathlib.Path(sys.argv[1]); target=pathlib.Path(sys.argv[2]); signed=sys.argv[3]=='true'; successful=sys.argv[4]=='true'
calls=[json.loads(line) for line in (root/'security.jsonl').read_text().splitlines()]
created=next(args[-1] for args in calls if args[0]=='create-keychain')
assert any(args==['delete-keychain',created] for args in calls)
assert calls[-2]==['list-keychains','-d','user','-s','/fixture/login.keychain-db']
assert not pathlib.Path(created).parent.exists()
if signed:
    sign=json.loads((root/'signed.json').read_text())
    identity='FIXTURETEAM.org.example.atlas.mobile'
    assert sign['entitlements']['application-identifier']==identity
    assert sign['entitlements']['com.apple.developer.team-identifier']=='FIXTURETEAM'
    assert sign['entitlements']['keychain-access-groups']==[identity,identity]
    assert sign['entitlements']['get-task-allow'] is False
    assert sign['entitlements']['nested']['identity']==identity
    assert '--keychain' in sign['args'] and created in sign['args']
archives=list((target/'dx/vmux_mobile/release/ios/ipa').glob('*.ipa'))
assert len(archives)==int(successful)
if successful:
    assert archives[0].name.startswith('Atlas & Co_')
    with zipfile.ZipFile(archives[0]) as archive:
        info=plistlib.loads(archive.read('Payload/VmuxMobile.app/Info.plist'))
        assert info['CFBundleIdentifier']=='org.example.atlas.mobile'
        assert info['CFBundleDisplayName']=='Atlas & Co'
"#).arg(&project.root).arg(&project.target).arg(matching_profile.to_string()).arg((matching_profile && !signing_failure).to_string()).output().unwrap();
        assert!(
            verification.status.success(),
            "{}",
            String::from_utf8_lossy(&verification.stderr)
        );
    }
}

#[test]
#[cfg(target_os = "macos")]
fn mobile_resource_projection_uses_selected_identity_and_assets_without_changing_templates() {
    for custom in [false, true] {
        let project = Fixture::new();
        project.ios_tools();
        let app = project.root.join("input.app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("vmux_mobile"), "fixture mobile binary").unwrap();
        std::fs::copy(
            project.root.join("packaging/ios/Info.plist"),
            app.join("Info.plist"),
        )
        .unwrap();
        let original_icon = std::fs::read(
            project
                .root
                .join("packaging/ios/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png"),
        )
        .unwrap();
        let icon = project.root.join("custom-icon.png");
        let output = project.native("python3").arg("-c").arg(r#"
import pathlib,struct,sys,zlib
def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
row=b'\0'+bytes([240,80,20,255])*1024
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>2I5B',1024,1024,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(row*1024))+chunk(b'IEND',b'')
pathlib.Path(sys.argv[1]).write_bytes(png)
"#).arg(&icon).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let name = if custom { "Atlas \"Lab\" & Co" } else { "Vmux" };
        let namespace = if custom {
            "org.example.atlas"
        } else {
            "ai.vmux"
        };
        let selected_icon = if custom {
            icon
        } else {
            project
                .root
                .join("packaging/ios/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png")
        };
        let launch = project.root.join("selected.storyboard");
        let privacy = project.root.join("selected.xcprivacy");
        let template = project.root.join("selected.plist");
        std::fs::write(&launch, "selected launch screen").unwrap();
        std::fs::write(&privacy, "selected privacy manifest").unwrap();
        let mut selected =
            plist::Value::from_file(project.root.join("packaging/ios/Info.plist")).unwrap();
        selected
            .as_dictionary_mut()
            .unwrap()
            .insert("SelectedTemplate".into(), "selected template".into());
        selected.to_file_xml(&template).unwrap();
        let mut command = project.command();
        if custom {
            command
                .env("VMUX_HOST_MOBILE_LAUNCH", &launch)
                .env("VMUX_HOST_MOBILE_PRIVACY", &privacy)
                .env("VMUX_HOST_MOBILE_PLIST", &template);
        }
        let output = command
            .args(["internal", "ios-resources"])
            .env("VMUX_IOS_APP_BUNDLE", &app)
            .env("VMUX_HOST_NAME", name)
            .env("VMUX_HOST_NAMESPACE", namespace)
            .env("VMUX_HOST_MOBILE_ICON", &selected_icon)
            .env("VMUX_IOS_BUILD_NUMBER", "19")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let layout = project
            .command()
            .args(["internal", "ios-verify"])
            .env("VMUX_IOS_APP_BUNDLE", &app)
            .env("VMUX_HOST_NAME", name)
            .env("VMUX_HOST_NAMESPACE", namespace)
            .env("VMUX_HOST_MOBILE_ICON", &selected_icon)
            .output()
            .unwrap();
        assert!(
            layout.status.success(),
            "{}",
            String::from_utf8_lossy(&layout.stderr)
        );
        assert_eq!(
            std::fs::read(app.join("Assets.car")).unwrap(),
            std::fs::read(&selected_icon).unwrap()
        );
        assert_eq!(
            std::fs::read(
                project
                    .root
                    .join("packaging/ios/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png")
            )
            .unwrap(),
            original_icon
        );
        if custom {
            assert_eq!(
                std::fs::read(app.join("LaunchScreen.storyboardc")).unwrap(),
                std::fs::read(&launch).unwrap()
            );
            assert_eq!(
                std::fs::read(app.join("PrivacyInfo.xcprivacy")).unwrap(),
                std::fs::read(&privacy).unwrap()
            );
            let info = plist::Value::from_file(app.join("Info.plist")).unwrap();
            assert_eq!(
                info.as_dictionary().unwrap()["SelectedTemplate"].as_string(),
                Some("selected template")
            );
        }
        let output = project
            .native("python3")
            .arg("-c")
            .arg(
                r#"
import pathlib,plistlib,sys
app=pathlib.Path(sys.argv[1]); name=sys.argv[2]; identifier=sys.argv[3]+'.mobile'
with (app/'Info.plist').open('rb') as source: info=plistlib.load(source)
assert info['CFBundleIdentifier']==identifier
assert info['CFBundleName']==name and info['CFBundleDisplayName']==name
assert info['CFBundleVersion']=='19'
assert info['CFBundleURLTypes'][0]['CFBundleURLName']==identifier
assert info['DTPlatformName']=='iphoneos'
assert (app/'PrivacyInfo.xcprivacy').is_file()
assert (app/'LaunchScreen.storyboardc').is_file()
"#,
            )
            .arg(&app)
            .arg(name)
            .arg(namespace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(all(unix, feature = "ci-syntax"))]
mod ci_native {
    use std::process::{Command, Output};

    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    struct Project {
        directory: tempfile::TempDir,
        root: PathBuf,
    }

    impl Project {
        fn create() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("checkout with spaces");
            std::fs::create_dir_all(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let project = Self { directory, root };
            project.script(
                "crates/app/vmux_cli/Cargo.toml",
                "[package]\nname = \"vmux_cli\"\nversion = \"0.0.35\"\n",
            );
            project
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn script(&self, name: &str, contents: &str) {
            let path = self.root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn command(&self, program: &str) -> Command {
            let mut command = Command::new(program);
            command.current_dir(&self.root);
            for name in [
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_INDEX_FILE",
                "GIT_COMMON_DIR",
                "CARGO_BUILD_TARGET",
            ] {
                command.env_remove(name);
            }
            command.env("CARGO_TARGET_DIR", self.directory.path().join("target"));
            command
        }
    }

    impl Project {
        fn native_fixture() -> Self {
            let project = Self::create();
            project.script(
            "crates/feature/sample/src/lib.rs",
            "pub fn live() {} #[cfg(test)] mod tests { #[test] fn check() { assert!(true); } }\n",
        );
            project.script("docs/architecture.md", "fixture documentation\n");
            project.script("ecs-policy.ron", "fixture policy\n");
            project.script(
                "crates/feature/sample/Cargo.toml",
                "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
            );
            project.script("bin/cargo", &format!(r#"#!/bin/sh
cat <<'METADATA'
{{"packages":[{{"manifest_path":"{root}/crates/feature/sample/Cargo.toml","targets":[{{"kind":["lib"],"src_path":"{root}/crates/feature/sample/src/lib.rs"}}]}}]}}
METADATA
"#, root = project.root().display()));
            project.native_git(&["init", "-b", "main"]);
            project.native_commit();
            project
        }

        fn native_git(&self, args: &[&str]) -> String {
            let mut command = Command::new("/usr/bin/git");
            command
                .args(args)
                .current_dir(self.root())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null");
            for key in [
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_INDEX_FILE",
                "GIT_COMMON_DIR",
            ] {
                command.env_remove(key);
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{:?}", output);
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        }

        fn native_commit(&self) {
            self.native_git(&["add", "."]);
            self.native_git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "fixture",
            ]);
        }

        fn native(&self, base: Option<&str>) -> Output {
            let mut command = self.command(env!("CARGO_BIN_EXE_vmux"));
            command
                .args(["ci-native", "--root", self.root().to_str().unwrap(), "--"])
                .env("CARGO_BIN", self.root().join("bin/cargo"))
                .env_remove("VMUX_CI_BASE");
            if let Some(base) = base {
                command.args(["--base", base]);
            }
            command.output().unwrap()
        }
    }

    #[test]
    fn portable_tests_and_their_policy_updates_skip_native_builds_but_production_edits_do_not() {
        let project = Project::native_fixture();
        let base = project.native_git(&["rev-parse", "HEAD"]);
        project.script(
        "crates/feature/sample/src/lib.rs",
        "pub fn live() {} #[cfg(test)] mod tests { #[test] fn check() { assert_eq!(1, 1); } }\n",
    );
        project.script("docs/architecture.md", "updated fixture documentation\n");
        project.script("ecs-policy.ron", "updated fixture policy\n");
        for path in [
            "crates/feature/sample/ecs-policy.ron",
            "crates/ecs-policy.ron",
            "website/ecs-policy.ron",
            "scripts/ecs-policy.ron",
            "patches/bevy_window-0.19.0/ecs-policy.ron",
        ] {
            project.script(path, "updated local fixture policy\n");
        }
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=false\n");

        project.script(
            "crates/feature/sample/src/ecs-policy.ron",
            "misplaced fixture policy\n",
        );
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=true\n");
        std::fs::remove_file(
            project
                .root()
                .join("crates/feature/sample/src/ecs-policy.ron"),
        )
        .unwrap();

        project.script(
        "crates/feature/sample/src/lib.rs",
        "pub fn live() { changed(); } #[cfg(test)] mod tests { #[test] fn check() { assert_eq!(1, 1); } }\n",
    );
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=true\n");
    }

    #[test]
    fn missing_history_and_unparseable_or_build_inputs_cannot_disable_native_checks() {
        let project = Project::native_fixture();
        let base = project.native_git(&["rev-parse", "HEAD"]);
        let output = project.native(None);
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=true\n");
        let output = project.native(Some("missing-commit"));
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());

        for (path, contents) in [
            ("crates/feature/sample/src/lib.rs", "fn invalid(\n"),
            (
                "crates/feature/sample/build.rs",
                "#[cfg(test)] fn fixture() {}\n",
            ),
            (
                "crates/build_platform_cfg.rs",
                "#[cfg(test)] fn fixture() {}\n",
            ),
            (
                "crates/feature/sample/Cargo.toml",
                "[package]\nname = \"sample\"\n",
            ),
            (
                "crates/feature/vmux_dx/src/lib.rs",
                "#[cfg(test)] fn fixture() {}\n",
            ),
        ] {
            project.native_git(&["reset", "--hard", &base]);
            project.native_git(&["clean", "-fd"]);
            project.script(path, contents);
            project.native_commit();
            let output = project.native(Some(&base));
            assert!(output.status.success(), "{path}: {:?}", output);
            assert_eq!(output.stdout, b"native=true\n", "{path}");
        }
    }

    #[test]
    fn source_mode_changes_retain_native_validation() {
        let project = Project::native_fixture();
        let base = project.native_git(&["rev-parse", "HEAD"]);
        project.native_git(&[
            "update-index",
            "--chmod=-x",
            "crates/feature/sample/src/lib.rs",
        ]);
        project.native_git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "source mode",
        ]);
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=true\n");
    }

    #[test]
    fn native_parent_modules_cannot_hide_changed_child_tests_from_linux() {
        let project = Project::native_fixture();
        project.script(
            "crates/feature/sample/src/lib.rs",
            "#[cfg(target_os = \"macos\")] mod native;\n",
        );
        project.script(
            "crates/feature/sample/src/native.rs",
            "#[cfg(test)] mod tests { #[test] fn check() { assert!(true); } }\n",
        );
        project.native_commit();
        let base = project.native_git(&["rev-parse", "HEAD"]);
        project.script(
            "crates/feature/sample/src/native.rs",
            "#[cfg(test)] mod tests { #[test] fn check() { assert!(false); } }\n",
        );
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=true\n");
    }

    #[test]
    fn inherited_coverage_is_proven_or_native_validation_is_retained() {
        for (declaration, source, native) in [
            ("mod native;", "native.rs", false),
            ("#[cfg(host)] mod native;", "native.rs", false),
            ("#[cfg(unix)] mod native;", "native.rs", false),
            ("#[cfg(not(host))] mod native;", "native.rs", true),
            (
                "#[cfg(feature = \"package\")] mod native;",
                "native.rs",
                true,
            ),
            (
                "#[cfg_attr(unix, cfg(target_os = \"macos\"))] mod native;",
                "native.rs",
                true,
            ),
            (
                "mod outer { #[cfg(target_arch = \"aarch64\")] mod native; }",
                "outer/native.rs",
                true,
            ),
            ("#[path = \"native.rs\"] mod alias;", "native.rs", true),
            ("", "native.rs", true),
        ] {
            let project = Project::native_fixture();
            let path = format!("crates/feature/sample/src/{source}");
            project.script("crates/feature/sample/src/lib.rs", declaration);
            project.script(
                &path,
                "#[cfg(test)] mod tests { #[test] fn check() { assert!(true); } }\n",
            );
            project.native_commit();
            let base = project.native_git(&["rev-parse", "HEAD"]);
            project.script(
                &path,
                "#[cfg(test)] mod tests { #[test] fn check() { assert!(false); } }\n",
            );
            project.native_commit();
            let output = project.native(Some(&base));
            assert!(output.status.success(), "{declaration}: {:?}", output);
            assert_eq!(
                output.stdout,
                format!("native={native}\n").as_bytes(),
                "{declaration}"
            );
        }
    }
}
