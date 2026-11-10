use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub struct Project {
    root: PathBuf,
    base: PathBuf,
    target: PathBuf,
    repo: PathBuf,
}

impl Project {
    pub fn create() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir().join(format!(
            "vmux-builder-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let root = base.join("checkout with spaces");
        let target = base.join("target with spaces");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&target).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .to_path_buf();
        let project = Self {
            root,
            base,
            target,
            repo,
        };
        for file in [
            "Makefile",
            "tool-versions.env",
            "Cargo.lock",
            "scripts/vmux-builder",
            "scripts/cargo-target-paths.sh",
            "scripts/build-mac-release.sh",
        ] {
            let destination = project.root.join(file);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(project.repo.join(file), destination).unwrap();
        }
        project.write_target(
            "CACHEDIR.TAG",
            "Signature: 8a477f597d28d172789f06886806bc55",
        );
        project.script("bin/ps", "#!/bin/sh\nprintf '%s fixture\\n' $$\n");
        project.script("bin/lsof", "#!/bin/sh\nprintf 'p%s\\nn/\\n' $$\n");
        project.script("bin/security", "#!/bin/sh\nexit 0\n");
        project.script("bin/cargo", "#!/bin/sh\nprintf '{\"packages\":[]}\\n'\n");
        project.script(
            "bin/git",
            r#"#!/bin/sh
case "$*" in
  *"rev-parse --short=7 HEAD"*) printf '9999999\n' ;;
  *) exec /usr/bin/git "$@" ;;
esac
"#,
        );
        project.script(
            "bin/codesign",
            r#"#!/bin/sh
[ "${FAIL_SIGNATURE:-0}" != 1 ] || exit 43
[ "$1" != --display ] || printf 'certificate\n' > codesign0
"#,
        );
        project.script(
            "bin/open",
            r#"#!/bin/sh
test -f "$1/Contents/MacOS/vmux_desktop" || exit 1
printf '%s\n' "$1" >> "$TEST_OPEN_LOG"
"#,
        );
        project.script(
            "scripts/package.sh",
            r#"#!/bin/bash
set -eu
printf 'package\n' >> "$TEST_BUILD_LOG"
env > "$TEST_ENV_LOG"
[[ "${FAIL_PACKAGE:-0}" != 1 ]] || exit 42
source ./scripts/cargo-target-paths.sh
directory="$(vmux_cargo_profile_dir "$PWD" release)"
if [[ "$1" == local ]]; then name='Vmux (9999999).app'; else name='Vmux.app'; fi
mkdir -p "$directory/$name/Contents/MacOS" "$directory/deps"
printf 'signed\n' > "$directory/$name/Contents/MacOS/vmux_desktop"
printf 'cache\n' > "$directory/deps/cache.o"
[[ "$1" != release ]] || printf 'distribution\n' > "$directory/Vmux.dmg"
"#,
        );
        project.script(
            "scripts/fixture-dev",
            r#"#!/bin/bash
set -eu
printf 'dev\n' >> "$TEST_BUILD_LOG"
mkdir -p "$CARGO_TARGET_DIR/debug/Vmux Dev.app/Contents/MacOS" "$CARGO_TARGET_DIR/debug/deps"
printf 'prepared\n' > "$CARGO_TARGET_DIR/debug/Vmux Dev.app/Contents/MacOS/vmux_desktop"
printf 'cache\n' > "$CARGO_TARGET_DIR/debug/deps/cache.o"
printf 'library\n' > "$CARGO_TARGET_DIR/debug/deps/current.dylib"
"#,
        );
        let mut makefile = fs::read_to_string(project.root.join("Makefile")).unwrap();
        makefile.push_str("\nensure-native-deps ensure-codesign-deps install-debug-render-process ensure-package-deps:\n\t@printf 'prerequisite\\n' >> \"$$TEST_BUILD_LOG\"\n_build-dev:\n\t@./scripts/fixture-dev\n");
        fs::write(project.root.join("Makefile"), makefile).unwrap();
        project
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    pub fn write_target(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.target.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    pub fn script(&self, name: &str, contents: &str) {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        fs::set_permissions(path, Permissions::from_mode(0o755)).unwrap();
    }

    pub fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command.current_dir(&self.root);
        for key in [
            "VMUX_TARGET_LOCK_OWNER_PID",
            "VMUX_TARGET_LOCK_TARGET",
            "VMUX_BUILD_STORAGE_TARGET",
            "VMUX_BUILD_STORAGE_ROOT",
            "CARGO_BUILD_TARGET",
            "RUSTC",
        ] {
            command.env_remove(key);
        }
        command
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("VMUX_BUILDER_BIN", env!("CARGO_BIN_EXE_vmux_builder"))
            .env("VMUX_TARGET_LOCK_ROOT", self.root.join("locks"))
            .env("CARGO_TARGET_DIR", &self.target)
            .env("APPLE_SIGNING_IDENTITY", "fixture")
            .env("VMUX_BUILD_STORAGE_MIN_GIB", "0")
            .env("VMUX_BUILD_STORAGE_WARN_GIB", "0")
            .env("VMUX_BUILD_STORAGE_CACHE_GIB", "0")
            .env("TEST_BUILD_LOG", self.root.join("build.log"))
            .env("TEST_ENV_LOG", self.root.join("build.env"))
            .env("TEST_OPEN_LOG", self.root.join("open.log"));
        command
    }

    pub fn make(&self, goal: &str) -> Output {
        let output = self.command("make").arg(goal).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    pub fn opened(&self) -> PathBuf {
        let log = fs::read_to_string(self.root.join("open.log")).unwrap();
        PathBuf::from(log.lines().last().unwrap())
    }

    pub fn wait_for(&self, path: &Path) {
        let start = Instant::now();
        while !path.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "fixture did not become ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_child(&self, child: &mut Child) -> std::process::ExitStatus {
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            if start.elapsed() >= Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("builder did not stop");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_output(&self, path: &Path) {
        let start = Instant::now();
        while fs::metadata(path).unwrap().len() == 0 {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "fixture produced no output"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

pub struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    pub fn new(child: Child) -> Self {
        Self { child }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}
