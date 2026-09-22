#[cfg(unix)]
#[test]
fn cargo_cache_wrapper_allows_nested_package_builds() {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, process::CommandExt},
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let target = temp.path().join("target");
    let locks = temp.path().join("locks");
    let cache = temp.path().join("cef-cache");
    let cargo = temp.path().join("cargo");
    let rustc = temp.path().join("rustc");
    let log = temp.path().join("cargo.log");
    fs::create_dir_all(&target).expect("create target");
    fs::write(
        &cargo,
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-V" ]]; then
    echo "cargo 1.0.0"
    exit 0
fi
printf '%s\n' "$*" >> "$FAKE_CARGO_LOG"
if [[ "${1:-}" == "packager" ]]; then
    "$CARGO_WRAPPER" build -p nested-package-build
fi
"#,
    )
    .expect("write fake cargo");
    fs::write(
        &rustc,
        "#!/usr/bin/env bash\necho 'rustc 1.0.0'\necho 'host: test-host'\n",
    )
    .expect("write fake rustc");
    for executable in [&cargo, &rustc] {
        let mut permissions = fs::metadata(executable)
            .expect("fake executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).expect("make fake executable runnable");
    }

    let wrapper = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../scripts/cargo-with-cef-cache.sh");
    let mut command = Command::new("bash");
    command
        .arg(&wrapper)
        .arg("packager")
        .env_remove("CI")
        .env_remove("VMUX_TARGET_LOCK_TARGET")
        .env_remove("VMUX_TARGET_LOCK_OWNER_PID")
        .env("CARGO_BIN", &cargo)
        .env("RUSTC", &rustc)
        .env("CARGO_TARGET_DIR", &target)
        .env("VMUX_TARGET_LOCK_ROOT", &locks)
        .env("VMUX_CEF_SDK_CACHE", &cache)
        .env("VMUX_DISABLE_SCCACHE", "1")
        .env("CARGO_WRAPPER", &wrapper)
        .env("FAKE_CARGO_LOG", &log)
        .process_group(0);
    let mut child = command.spawn().expect("run nested cargo wrapper");
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll nested cargo wrapper") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = Command::new("kill")
                .arg("-TERM")
                .arg(format!("-{}", child.id()))
                .status();
            let _ = child.wait();
            panic!("nested cargo wrapper deadlocked on the target cache lock");
        }
        thread::sleep(Duration::from_millis(100));
    };

    assert!(status.success());
    assert_eq!(
        fs::read_to_string(log).expect("read fake cargo log"),
        "packager\nbuild -p nested-package-build\n"
    );
}

#[test]
fn target_seed_key_resolves_a_symlinked_repo_root() {
    use std::os::unix::fs::symlink;
    use std::process::Command;

    let temp = tempfile::tempdir().expect("tempdir");
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root");
    let linked_repo = temp.path().join("repo");
    symlink(&repo, &linked_repo).expect("symlink repo");

    let run = |path: &std::path::Path| {
        Command::new("/bin/bash")
            .args([
                "-c",
                "cd \"$1\" && CARGO_BIN=/usr/bin/true RUSTC=/usr/bin/true scripts/target-seed-key.sh",
                "bash",
            ])
            .arg(path)
            .output()
            .expect("run target seed key")
    };
    let canonical = run(&repo);
    let linked = run(&linked_repo);

    for output in [&canonical, &linked] {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let canonical_key = String::from_utf8(canonical.stdout).expect("utf8 canonical key");
    let key = String::from_utf8(linked.stdout).expect("utf8 linked key");
    assert_eq!(key, canonical_key);
    assert_eq!(key.trim().len(), 40);
    assert!(key.trim().bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn cef_target_relocator_rewrites_only_cef_build_state() {
    use std::{fs, process::Command};

    let temp = tempfile::tempdir().expect("tempdir");
    let staging = temp.path().join("target");
    let source = temp.path().join("source-target");
    let original = temp.path().join("original-target");
    let destination = temp.path().join("destination-target");
    let cmake = staging.join("debug/build/cef-dll-sys-test/out/build/CMakeCache.txt");
    let fingerprint = staging
        .join("debug/.fingerprint/cef-dll-sys-test/run-build-script-build-script-build.json");
    let dep = staging.join("debug/deps/cef_dll_sys-test.d");
    let unrelated = staging.join("debug/build/other/output");

    for path in [&cmake, &fingerprint, &dep] {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, format!("{}\n", original.display())).expect("write fixture");
    }
    fs::write(
        &cmake,
        format!(
            "CMAKE_CACHEFILE_DIR:INTERNAL={}/debug/build/cef-dll-sys-test/out/build\n",
            original.display()
        ),
    )
    .expect("write CMake cache fixture");
    fs::create_dir_all(unrelated.parent().expect("parent")).expect("create unrelated parent");
    fs::write(&unrelated, format!("{}\n", source.display())).expect("write unrelated fixture");

    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../scripts/relocate-cef-target.sh");
    let status = Command::new("bash")
        .arg(script)
        .arg(&staging)
        .arg(&source)
        .arg(&destination)
        .status()
        .expect("run CEF target relocator");
    assert!(status.success());

    for path in [&cmake, &fingerprint, &dep] {
        assert!(
            fs::read_to_string(path)
                .expect("read relocated fixture")
                .contains(&destination.display().to_string())
        );
    }
    assert_eq!(
        fs::read_to_string(unrelated).expect("read unrelated fixture"),
        format!("{}\n", source.display())
    );
}

#[cfg(unix)]
#[test]
fn nightly_version_sorts_between_current_and_next_stable() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../scripts/nightly-version.sh");
    let output = std::process::Command::new("bash")
        .arg(script)
        .args(["0.0.34", "20260921", "42", "1"])
        .output()
        .expect("run nightly version script");

    assert!(output.status.success());
    let value = String::from_utf8(output.stdout).expect("nightly version is utf-8");
    let preview = semver::Version::parse(value.trim()).expect("nightly version is semver");
    assert_eq!(preview.to_string(), "0.0.35-nightly.20260921.42.1");
    assert!(preview > semver::Version::parse("0.0.34").unwrap());
    assert!(preview < semver::Version::parse("0.0.35").unwrap());
}
