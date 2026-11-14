#![cfg(feature = "dx")]

use assert_cmd::Command;
use predicates::prelude::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(not(feature = "runtime"))]
struct WorkspaceFixture {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
}

#[cfg(not(feature = "runtime"))]
impl WorkspaceFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("workspace with spaces");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let fixture = Self {
            _directory: directory,
            root,
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
        .stdout(predicate::str::contains("-b, --branch"));
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
        .timeout(std::time::Duration::from_secs(30))
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

#[cfg(unix)]
struct Fixture {
    directory: tempfile::TempDir,
    root: std::path::PathBuf,
    target: std::path::PathBuf,
}

#[cfg(unix)]
impl Fixture {
    fn new() -> Self {
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
            "packaging/macos/Vmux.icns",
            "packaging/macos/tray.png",
            "packaging/macos/dmg-background.png",
            "packaging/macos/crash_reporter.cfg",
            "packaging/macos/ai.vmux.service.plist",
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
case "$1" in
  -V|--version) printf 'cargo 1.90.0\n'; exit 0 ;;
  build|rustc)
    [ "${FAIL_BUILD:-0}" != 1 ] || exit 42
    case "$*" in *--release*) profile=release;; *cef-helper*) profile=cef-helper;; *) profile=debug;; esac
    output="$CARGO_TARGET_DIR/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}$profile"
    mkdir -p "$output"
    for binary in vmux vmux_service vmux_desktop bevy_cef_debug_render_process; do
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
        fixture.script("codesign", "#!/bin/sh\n[ \"${FAIL_SIGNATURE:-0}\" != 1 ] || exit 44\n[ \"$1\" != --display ] || printf certificate > codesign0\n");
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
    printf plist > "$contents/Info.plist"
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
    fixture
        .command()
        .args(["run", "-p", "local"])
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
        ["vmux", "vmux_desktop", "vmux service"]
            .into_iter()
            .map(String::from)
            .collect()
    );
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
        project.native_commit();
        let output = project.native(Some(&base));
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"native=false\n");

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
