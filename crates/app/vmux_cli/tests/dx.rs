#![cfg(feature = "dx")]

use assert_cmd::Command;
use predicates::prelude::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
