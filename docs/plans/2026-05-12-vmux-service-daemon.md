# vmux_service Daemon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote `vmux_service` to a first-class supervised daemon — independent lifecycle via launchd, per-profile resources, graceful upgrade, structured logging, CLI surface, in-terminal connect-error UI.

**Architecture:** One daemon binary per build profile, lazily kicked off by GUI/CLI clients, supervised by `launchd` (`KeepAlive.Crashed=true`, `RunAtLoad=false`). All PTYs and IPC live in `crates/vmux_service` (which absorbs the deleted `crates/vmux_process` webview). Identity-mismatch upgrades use a Shutdown handshake then SIGTERM/SIGKILL.

**Tech Stack:** Rust, tokio, rkyv IPC, `launchctl` (macOS), `tracing` + `tracing-appender`, `clap`, Bevy (host), Dioxus (webview).

**Spec:** `docs/specs/2026-05-12-vmux-service-daemon-design.md`

---

## File Structure (final state)

```
crates/vmux_service/
├── Cargo.toml                # bins: vmux_service (host), vmux_service_app (wasm); new deps
├── build.rs                  # webview build (moved from vmux_process)
├── src/
│   ├── lib.rs                # re-exports + per-profile path/identity helpers
│   ├── framing.rs            # unchanged
│   ├── protocol.rs           # +Status request, +StatusResponse message
│   ├── process.rs            # unchanged
│   ├── server.rs             # +tracing, +Status handler, +graceful Shutdown drain
│   ├── client.rs             # tracing, identity-mismatch delegates to supervisor
│   ├── service.rs            # pub fn run() — daemon entry (was main.rs)
│   ├── main.rs               # bin → service::run()
│   ├── launchd.rs            # macOS: plist gen, install/uninstall, kickstart, ensure_running
│   ├── supervisor.rs         # graceful shutdown handshake + SIGTERM/SIGKILL
│   ├── cli.rs                # status|start|stop|restart|logs|install|uninstall
│   ├── webview.rs            # `pub mod {event, plugin, app};`
│   └── webview/
│       ├── event.rs          # moved from vmux_process
│       ├── plugin.rs         # moved from vmux_process — ServicesPlugin
│       ├── app.rs            # moved from vmux_process
│       └── main.rs           # moved from vmux_process — wasm bin entry

crates/vmux_cli/src/
├── commands.rs               # +Service variant
├── commands/service.rs       # NEW: clap subcommand → vmux_service::cli
└── main.rs                   # route Service arm

crates/vmux_desktop/
├── Cargo.toml                # drop vmux_process dep
└── src/
    ├── main.rs               # drop service subcommand + run_service()
    ├── lib.rs                # ProcessesPlugin → ServicesPlugin
    ├── terminal.rs           # ensure_service_started → launchd::ensure_running;
    │                         # exp-backoff connect; ServiceUnavailableEvent emit
    ├── agent.rs              # update vmux::services URL constant import
    └── processes_monitor.rs  # update event imports

crates/vmux_process/          # DELETED entire crate

Cargo.toml (workspace)        # remove vmux_process from members
```

---

## Phase A: Per-profile foundations

### Task 1: Per-profile path + profile helpers in `vmux_service::lib`

**Files:**
- Modify: `crates/vmux_service/src/lib.rs`

- [ ] **Step 1: Write failing tests for new helpers**

Append to `crates/vmux_service/src/lib.rs` `tests` module:

```rust
    #[test]
    fn current_profile_is_compile_env() {
        // Set by build.rs; default in tests is "dev" because no VMUX_PROFILE override.
        // Just assert it's non-empty and is one of the known profiles.
        let p = current_profile();
        assert!(!p.is_empty());
        assert!(matches!(p, "release" | "local" | "dev"));
    }

    #[test]
    fn launchd_label_includes_profile() {
        assert_eq!(launchd_label("dev"), "ai.vmux.service.dev");
        assert_eq!(launchd_label("release"), "ai.vmux.service.release");
    }

    #[test]
    fn socket_path_includes_profile_suffix() {
        let s = socket_path();
        let name = s.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("vmux-"));
        assert!(name.ends_with(".sock"));
        assert!(name.contains(current_profile()));
    }

    #[test]
    fn pid_log_identity_paths_share_profile_suffix() {
        let suffix = format!("vmux-{}", current_profile());
        for p in [pid_path(), identity_path(), log_path()] {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with(&suffix), "expected {name} to start with {suffix}");
        }
    }

    #[test]
    fn plist_path_lives_in_user_launchagents() {
        let p = plist_path("dev");
        let s = p.to_string_lossy();
        assert!(s.contains("Library/LaunchAgents"));
        assert!(s.ends_with("ai.vmux.service.dev.plist"));
    }
```

- [ ] **Step 2: Run tests and confirm they fail**

```bash
env -u CEF_PATH cargo test -p vmux_service --lib current_profile_is_compile_env launchd_label_includes_profile socket_path_includes_profile_suffix pid_log_identity_paths_share_profile_suffix plist_path_lives_in_user_launchagents 2>&1 | tail -20
```

Expected: 5 failures with "cannot find function `current_profile`/`launchd_label`/`identity_path`/`log_path`/`plist_path` in this scope" (or similar).

- [ ] **Step 3: Add `VMUX_PROFILE` to `vmux_service` build script**

Create `crates/vmux_service/build.rs`:

```rust
fn main() {
    let profile = std::env::var("VMUX_PROFILE").unwrap_or_else(|_| {
        match std::env::var("PROFILE").as_deref() {
            Ok("release") => "release".to_string(),
            _ => "dev".to_string(),
        }
    });
    println!("cargo::rustc-env=VMUX_PROFILE={profile}");
    println!("cargo::rerun-if-env-changed=VMUX_PROFILE");
}
```

Add to `crates/vmux_service/Cargo.toml` package section:

```toml
build = "build.rs"
```

- [ ] **Step 4: Replace existing path helpers with profile-suffixed versions**

Edit `crates/vmux_service/src/lib.rs`. Replace the existing `socket_path`, `pid_path`, `service_identity_path` with profile-aware versions and add `launchd_label`, `plist_path`, `identity_path` (renamed), `log_path`, `current_profile`:

```rust
pub mod client;
pub mod framing;
pub mod process;
pub mod protocol;
pub mod server;

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Profile this build was compiled for ("release", "local", or "dev").
pub fn current_profile() -> &'static str {
    env!("VMUX_PROFILE")
}

/// Directory for service runtime files (socket, pid, log).
pub fn service_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home).join("Library/Application Support/Vmux/services")
}

/// Path to the per-profile Unix domain socket.
pub fn socket_path() -> PathBuf {
    service_dir().join(format!("vmux-{}.sock", current_profile()))
}

/// Path to the per-profile PID file.
pub fn pid_path() -> PathBuf {
    service_dir().join(format!("vmux-{}.pid", current_profile()))
}

/// Path to the per-profile service executable identity file.
pub fn identity_path() -> PathBuf {
    service_dir().join(format!("vmux-{}.identity", current_profile()))
}

/// Path to the per-profile service log file.
pub fn log_path() -> PathBuf {
    service_dir().join(format!("vmux-{}.log", current_profile()))
}

/// LaunchAgent label for the given profile.
pub fn launchd_label(profile: &str) -> String {
    format!("ai.vmux.service.{profile}")
}

/// Path to the LaunchAgent plist for the given profile.
pub fn plist_path(profile: &str) -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", launchd_label(profile)))
}

/// Identity for the current executable. Changes when the binary path, size,
/// or modification timestamp changes.
pub fn current_executable_identity() -> std::io::Result<String> {
    executable_identity_for_path(&std::env::current_exe()?)
}

/// Write the current executable identity for a service process.
pub fn write_service_identity() -> std::io::Result<()> {
    std::fs::write(identity_path(), current_executable_identity()?)
}

pub(crate) fn executable_identity_for_path(path: &Path) -> std::io::Result<String> {
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(format!(
        "{}\n{}\n{modified}",
        path.display(),
        metadata.len()
    ))
}

pub(crate) fn service_identity_matches(recorded: &str, current: &str) -> bool {
    recorded.trim() == current.trim()
}

// Backwards compat: keep the old name as a deprecated re-export so we can
// migrate call sites in a follow-up step (Task 2 deletes it after rename).
#[deprecated(note = "renamed to identity_path")]
pub fn service_identity_path() -> PathBuf {
    identity_path()
}
```

Delete the placeholder `profile_filename` lines from the snippet (they are dead). Final lib.rs should not contain that helper.

- [ ] **Step 5: Run the tests; expect pass**

```bash
env -u CEF_PATH cargo test -p vmux_service --lib current_profile_is_compile_env launchd_label_includes_profile socket_path_includes_profile_suffix pid_log_identity_paths_share_profile_suffix plist_path_lives_in_user_launchagents 2>&1 | tail -10
```

Expected: 5 passed.

- [ ] **Step 6: Update existing call sites of `service_identity_path` → `identity_path`**

Search and replace:

```bash
rg -l 'service_identity_path' crates/
```

Update every match (currently `vmux_service/src/main.rs`, `vmux_service/src/client.rs`, `vmux_desktop/src/main.rs`, `vmux_desktop/src/terminal.rs`) to call `identity_path()`. Then remove the `#[deprecated]` shim from `lib.rs`.

- [ ] **Step 7: Run full crate test + clippy**

```bash
env -u CEF_PATH cargo test -p vmux_service 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_service -- --check
```

Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_service/
git add crates/vmux_desktop/src/main.rs crates/vmux_desktop/src/terminal.rs
git commit -m "feat(VMX-116): per-profile socket/pid/identity/log paths in vmux_service"
```

---

### Task 2: Add tracing dependencies, replace `eprintln!` with `tracing` macros in `server.rs` and `client.rs`

**Files:**
- Modify: `crates/vmux_service/Cargo.toml`
- Modify: `crates/vmux_service/src/server.rs`
- Modify: `crates/vmux_service/src/client.rs`

- [ ] **Step 1: Add tracing deps to `crates/vmux_service/Cargo.toml`**

Edit `[dependencies]`:

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-appender = "0.2"
```

- [ ] **Step 2: Replace `eprintln!` calls in `client.rs`**

Open `crates/vmux_service/src/client.rs`. Replace each `eprintln!("vmux-service: ...")` and `eprintln!("service ...")`:

| Old | New |
|---|---|
| `eprintln!("vmux-service: socket exists but no PID file, cleaning up");` | `tracing::warn!("socket exists but no PID file, cleaning up");` |
| `eprintln!("vmux-service: invalid PID file content: {:?}", pid_str.trim());` | `tracing::warn!(pid_file = ?pid_str.trim(), "invalid PID file content");` |
| `eprintln!("vmux-service: stale service (pid {pid}) — cleaning up");` | `tracing::warn!(pid, "stale service — cleaning up");` |
| `eprintln!("vmux-service: failed to identify current executable: {e}");` | `tracing::error!(error = %e, "failed to identify current executable");` |
| `eprintln!("vmux-service: service identity missing, cleaning up");` | `tracing::warn!("service identity missing, cleaning up");` |
| `eprintln!("vmux-service: service identity mismatch, cleaning up");` | `tracing::warn!("service identity mismatch, cleaning up");` |
| `eprintln!("service connect failed: {e}");` | `tracing::error!(error = %e, "service connect failed");` |
| `eprintln!("service connect timed out");` | `tracing::error!("service connect timed out");` |

Add at the top: nothing (tracing macros are at crate root).

- [ ] **Step 3: Replace `eprintln!` in `server.rs`**

```bash
rg 'eprintln!' crates/vmux_service/src/server.rs
```

For each match, convert with the same level mapping (info for normal lifecycle, warn for recoverable, error for failures, debug for per-message).

- [ ] **Step 4: Build to confirm**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 5: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_service 2>&1 | tail -10
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/Cargo.toml crates/vmux_service/src/server.rs crates/vmux_service/src/client.rs
git commit -m "refactor(VMX-116): replace eprintln! with tracing in vmux_service IPC"
```

---

## Phase B: Daemon entry rename + tracing init

### Task 3: Extract daemon entry into `service.rs`, leave thin `main.rs`, init tracing subscriber, rename binary

**Files:**
- Create: `crates/vmux_service/src/service.rs`
- Modify: `crates/vmux_service/src/main.rs`
- Modify: `crates/vmux_service/src/lib.rs` (add `pub mod service;`)
- Modify: `crates/vmux_service/Cargo.toml`

- [ ] **Step 1: Add `service` module to lib**

Append to `crates/vmux_service/src/lib.rs` (top, with other `pub mod` lines):

```rust
pub mod service;
```

- [ ] **Step 2: Create `crates/vmux_service/src/service.rs`**

```rust
use crate::{
    identity_path, log_path, pid_path, service_dir, socket_path, write_service_identity,
};
use std::time::Instant;
use tracing_subscriber::{EnvFilter, fmt};

/// Daemon entry point. Initializes logging, writes pid/identity, binds the socket,
/// installs SIGTERM/SIGINT handlers, and runs the IPC server until shutdown.
pub fn run() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    rt.block_on(async { run_async().await });
}

async fn run_async() {
    let dir = service_dir();
    std::fs::create_dir_all(&dir).expect("failed to create service dir");

    init_tracing();

    let pid = std::process::id();
    std::fs::write(pid_path(), pid.to_string()).expect("failed to write PID file");
    write_service_identity().expect("failed to write service identity file");

    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);
    let listener = tokio::net::UnixListener::bind(&sock).expect("failed to bind Unix socket");

    let started = Instant::now();
    tracing::info!(
        target: "vmux_service::startup",
        version = env!("CARGO_PKG_VERSION"),
        profile = crate::current_profile(),
        pid = pid,
        socket = %sock.display(),
        "vmux_service started"
    );

    let sock_cleanup = sock.clone();
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        tracing::info!("shutdown signal received, cleaning up");
        let _ = std::fs::remove_file(&sock_cleanup);
        let _ = std::fs::remove_file(pid_path());
        let _ = std::fs::remove_file(identity_path());
        std::process::exit(0);
    });

    crate::server::run_server(listener).await;

    let _uptime = started.elapsed();
}

fn init_tracing() {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!("vmux-{}", crate::current_profile()))
        .filename_suffix("log")
        .max_log_files(7)
        .build(service_dir())
        .expect("build rolling log appender");

    let (writer, guard) = tracing_appender::non_blocking(appender);
    // Leak the guard for the lifetime of the process so logs flush on drop.
    Box::leak(Box::new(guard));

    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_env("VMUX_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_target(false)
        .try_init();

    let _ = log_path(); // touches the path helper to keep it referenced
}
```

- [ ] **Step 3: Rewrite `crates/vmux_service/src/main.rs` to a thin entry**

Replace entire contents:

```rust
fn main() {
    vmux_service::service::run();
}
```

- [ ] **Step 4: Rename binary in `Cargo.toml`**

Edit `crates/vmux_service/Cargo.toml`:

```toml
[[bin]]
name = "vmux_service"   # was "vmux-service"
path = "src/main.rs"
```

- [ ] **Step 5: Build**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | tail -15
```

Expected: clean build with `vmux_service` binary at `target/debug/vmux_service`.

- [ ] **Step 6: Smoke test the binary**

```bash
./target/debug/vmux_service &
sleep 1
ls "$HOME/Library/Application Support/Vmux/services/" | rg vmux-dev
kill %1
```

Expected: `vmux-dev.sock`, `vmux-dev.pid`, `vmux-dev.identity`, `vmux-dev.YYYY-MM-DD.log` exist.

- [ ] **Step 7: Run tests + clippy**

```bash
env -u CEF_PATH cargo test -p vmux_service 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_service -- --check
```

Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_service/
git commit -m "feat(VMX-116): extract vmux_service daemon entry, init tracing subscriber"
```

---

## Phase C: Graceful shutdown + identity-mismatch killing

### Task 4: Server-side `Shutdown` drain handler

**Files:**
- Modify: `crates/vmux_service/src/server.rs`

- [ ] **Step 1: Inspect current Shutdown handling**

```bash
rg -n 'Shutdown' crates/vmux_service/src/server.rs
```

Note any existing arm. If `ClientMessage::Shutdown` is currently unhandled or merely ignored, we'll add a clean-drain path.

- [ ] **Step 2: Add a graceful drain path**

In `server.rs`, locate the per-connection request handler match arm. Add (or replace existing weak handling):

```rust
ClientMessage::Shutdown => {
    tracing::info!("Shutdown requested by client; draining and exiting");
    // Signal the top-level run loop to stop accepting new connections.
    shutdown_tx.send(()).await.ok();
    break;
}
```

The exact integration depends on whether `run_server` already plumbs a shutdown channel. If not, refactor `run_server` to accept a `tokio::sync::mpsc::Sender<()>` and break its accept loop when it fires. Show that refactor:

```rust
pub async fn run_server(listener: tokio::net::UnixListener) {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _addr)) => {
                        let tx = shutdown_tx.clone();
                        tokio::spawn(handle_client(stream, tx));
                    }
                    Err(e) => tracing::warn!(error = %e, "accept failed"),
                }
            }
            _ = shutdown_rx.recv() => {
                tracing::info!("server: drain signaled, closing listener");
                break;
            }
        }
    }
    // Allow in-flight handlers ~500ms to finish before the process exits.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}
```

`handle_client` signature gains the shutdown sender; pass it down to wherever `Shutdown` is matched.

- [ ] **Step 3: Build**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 4: Add an integration-style unit test**

Add to `crates/vmux_service/src/server.rs` `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn shutdown_message_breaks_run_server() {
        let dir = std::env::temp_dir().join(format!("vmux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("test.sock");
        let _ = std::fs::remove_file(&sock);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(super::run_server(listener));

        // Connect and send Shutdown.
        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (_r, mut w) = stream.into_split();
        crate::framing::write_message_async(&mut w, &crate::protocol::ClientMessage::Shutdown)
            .await
            .unwrap();

        // run_server should return within 1s.
        let res = tokio::time::timeout(std::time::Duration::from_secs(2), server).await;
        assert!(res.is_ok(), "run_server did not exit after Shutdown");
        let _ = std::fs::remove_file(&sock);
    }
```

(If `framing::write_message_async` doesn't exist by that exact name, use whatever the existing async write helper is — `rg 'pub.*async.*write_message' crates/vmux_service/src/framing.rs` to find it.)

- [ ] **Step 5: Run the test**

```bash
env -u CEF_PATH cargo test -p vmux_service shutdown_message_breaks_run_server 2>&1 | tail -10
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/src/server.rs
git commit -m "feat(VMX-116): graceful Shutdown drain in vmux_service server"
```

---

### Task 5: `supervisor` module — graceful shutdown handshake with timeout escalation

**Files:**
- Create: `crates/vmux_service/src/supervisor.rs`
- Modify: `crates/vmux_service/src/lib.rs` (add `pub mod supervisor;`)

- [ ] **Step 1: Add module declaration**

In `crates/vmux_service/src/lib.rs`, add:

```rust
pub mod supervisor;
```

- [ ] **Step 2: Write failing tests in `supervisor.rs`**

Create `crates/vmux_service/src/supervisor.rs`:

```rust
//! Replace a running daemon: graceful Shutdown over the socket,
//! escalate to SIGTERM, finally SIGKILL.

use std::path::Path;
use std::time::{Duration, Instant};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const SIGTERM_GRACE: Duration = Duration::from_millis(500);

/// Outcome of attempting to terminate a running daemon.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplaceOutcome {
    /// Process exited after Shutdown handshake.
    GracefulShutdown,
    /// Process exited after SIGTERM.
    SigtermExit,
    /// Process exited after SIGKILL.
    SigkillExit,
    /// PID was already dead before any signal.
    AlreadyDead,
}

/// Block until `pid` is no longer alive, or `deadline` elapses.
pub fn wait_for_pid_exit(pid: i32, deadline: Instant) -> bool {
    while Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    !pid_alive(pid)
}

pub fn pid_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Send a signal to `pid` (Unix). Returns Ok(()) even if the process is gone.
pub fn send_signal(pid: i32, sig: i32) -> std::io::Result<()> {
    let r = unsafe { libc::kill(pid, sig) };
    if r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Replace the running daemon for the given pid. The caller must have already
/// determined the running pid is "wrong" (e.g., identity mismatch).
///
/// `send_shutdown` is a closure that performs the IPC Shutdown round-trip.
/// Returning `Ok` means the message was acknowledged or the connection closed.
/// Returning `Err` means we couldn't reach it (will fall through to SIGTERM).
pub fn replace_running<F>(pid: i32, send_shutdown: F) -> ReplaceOutcome
where
    F: FnOnce() -> std::io::Result<()>,
{
    if !pid_alive(pid) {
        return ReplaceOutcome::AlreadyDead;
    }

    if send_shutdown().is_ok() && wait_for_pid_exit(pid, Instant::now() + SHUTDOWN_GRACE) {
        tracing::info!(pid, "old daemon exited via Shutdown handshake");
        return ReplaceOutcome::GracefulShutdown;
    }

    tracing::warn!(pid, "Shutdown timed out, escalating to SIGTERM");
    let _ = send_signal(pid, libc::SIGTERM);
    if wait_for_pid_exit(pid, Instant::now() + SIGTERM_GRACE) {
        return ReplaceOutcome::SigtermExit;
    }

    tracing::warn!(pid, "SIGTERM timed out, escalating to SIGKILL");
    let _ = send_signal(pid, libc::SIGKILL);
    let _ = wait_for_pid_exit(pid, Instant::now() + Duration::from_millis(500));
    ReplaceOutcome::SigkillExit
}

/// Best-effort cleanup of stale runtime files for the current profile.
pub fn clean_runtime_files() {
    let _ = std::fs::remove_file(crate::socket_path());
    let _ = std::fs::remove_file(crate::pid_path());
    let _ = std::fs::remove_file(crate::identity_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_dead_pid_returns_alreadydead() {
        // PID 1 is always alive; pick a pid that's almost certainly free.
        // Use a high pid + verify it's not alive to avoid flakes.
        let mut pid = 999_999;
        while pid_alive(pid) {
            pid -= 1;
        }
        let outcome = replace_running(pid, || Ok(()));
        assert_eq!(outcome, ReplaceOutcome::AlreadyDead);
    }

    #[test]
    fn graceful_shutdown_when_send_succeeds_and_pid_exits() {
        // Spawn a sleep, kill it from the closure to simulate "the process
        // exited after Shutdown was delivered".
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id() as i32;

        let outcome = replace_running(pid, || {
            // Simulate: Shutdown handshake "succeeds" and the daemon exits.
            unsafe { libc::kill(pid, libc::SIGTERM) };
            Ok(())
        });
        let _ = child.wait();
        assert_eq!(outcome, ReplaceOutcome::GracefulShutdown);
    }

    #[test]
    fn escalates_to_sigterm_when_shutdown_send_fails() {
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id() as i32;

        let outcome = replace_running(pid, || {
            Err(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "no socket"))
        });
        let _ = child.wait();
        assert_eq!(outcome, ReplaceOutcome::SigtermExit);
    }
}
```

- [ ] **Step 3: Run tests; expect failures (compilation only — file is new)**

```bash
env -u CEF_PATH cargo test -p vmux_service supervisor 2>&1 | tail -20
```

Expected: build error referencing missing module if step 1 was skipped — fix; otherwise tests should compile and pass.

- [ ] **Step 4: Iterate until tests pass**

```bash
env -u CEF_PATH cargo test -p vmux_service supervisor 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 5: Wire `client.rs::service_running` to use supervisor on identity mismatch**

Edit `crates/vmux_service/src/client.rs`. Replace the existing identity-mismatch branch (currently `clean_service_files(&sock); return false;`) with a call to `supervisor::replace_running` followed by cleanup.

Locate this block (roughly `client.rs:125-129`):

```rust
        if !crate::service_identity_matches(&service_identity, &current_identity) {
            eprintln!("vmux-service: service identity mismatch, cleaning up");
            clean_service_files(&sock);
            return false;
        }
```

Replace with:

```rust
        if !crate::service_identity_matches(&service_identity, &current_identity) {
            tracing::warn!(pid, "service identity mismatch, replacing running daemon");
            let outcome = crate::supervisor::replace_running(pid, || {
                let stream = std::os::unix::net::UnixStream::connect(&sock)?;
                stream.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
                let mut stream = stream;
                crate::framing::write_message_blocking(
                    &mut stream,
                    &crate::protocol::ClientMessage::Shutdown,
                )
            });
            tracing::info!(?outcome, "replaced running daemon");
            crate::supervisor::clean_runtime_files();
            return false;
        }
```

(If `framing::write_message_blocking` doesn't exist by that name, locate the sync framing helper via `rg 'pub fn write_message' crates/vmux_service/src/framing.rs` and use that. If only async exists, add a small sync helper in `framing.rs` that uses the rkyv encoder + length prefix on a `std::io::Write`.)

- [ ] **Step 6: Run full crate tests + clippy**

```bash
env -u CEF_PATH cargo test -p vmux_service 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_service -- --check
```

Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_service/
git commit -m "feat(VMX-116): supervisor handles graceful shutdown + escalation on identity mismatch"
```

---

## Phase D: launchd integration

### Task 6: `launchd` module — plist generation, install/uninstall, kickstart, ensure_running

**Files:**
- Create: `crates/vmux_service/src/launchd.rs`
- Modify: `crates/vmux_service/src/lib.rs` (add module + re-exports)

- [ ] **Step 1: Add module declaration (gated to macOS)**

In `crates/vmux_service/src/lib.rs`:

```rust
#[cfg(target_os = "macos")]
pub mod launchd;
```

- [ ] **Step 2: Write `launchd.rs` with plist generation + golden test**

Create `crates/vmux_service/src/launchd.rs`:

```rust
//! macOS LaunchAgent integration for vmux_service.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Render the LaunchAgent plist XML for a profile.
pub fn generate_plist(profile: &str, binary_path: &Path, log_path: &Path) -> String {
    let label = crate::launchd_label(profile);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{binary}</string>
  </array>
  <key>RunAtLoad</key>
  <false/>
  <key>KeepAlive</key>
  <dict>
    <key>Crashed</key>
    <true/>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>VMUX_PROFILE</key>
    <string>{profile}</string>
  </dict>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#,
        label = label,
        binary = binary_path.display(),
        log = log_path.display(),
        profile = profile,
    )
}

/// Write the plist for `profile` pointing at `binary_path`.
pub fn install(profile: &str, binary_path: &Path) -> std::io::Result<PathBuf> {
    let plist = crate::plist_path(profile);
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(crate::service_dir())?;

    let xml = generate_plist(profile, binary_path, &crate::log_path());
    std::fs::write(&plist, xml)?;
    bootstrap(&plist)?;
    Ok(plist)
}

/// Remove the plist and unload from launchd.
pub fn uninstall(profile: &str) -> std::io::Result<()> {
    let plist = crate::plist_path(profile);
    if plist.exists() {
        let _ = bootout(profile);
        std::fs::remove_file(&plist)?;
    }
    Ok(())
}

/// `launchctl bootstrap gui/<uid> <plist>`.
pub fn bootstrap(plist: &Path) -> std::io::Result<()> {
    let uid = unsafe { libc::getuid() };
    let status = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}")])
        .arg(plist)
        .status()?;
    // Bootstrap returns nonzero if already loaded — that's fine.
    let _ = status;
    Ok(())
}

/// `launchctl bootout gui/<uid>/<label>`.
pub fn bootout(profile: &str) -> std::io::Result<()> {
    let uid = unsafe { libc::getuid() };
    let label = crate::launchd_label(profile);
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{label}")])
        .status()?;
    Ok(())
}

/// `launchctl kickstart -k gui/<uid>/<label>` — restart cleanly.
pub fn kickstart(profile: &str) -> std::io::Result<()> {
    let uid = unsafe { libc::getuid() };
    let label = crate::launchd_label(profile);
    let _ = Command::new("launchctl")
        .args(["kickstart", "-k", &format!("gui/{uid}/{label}")])
        .status()?;
    Ok(())
}

/// Make sure the daemon is installed and running. Idempotent.
/// `binary_path` is the daemon executable (resolved by the caller).
pub fn ensure_running(profile: &str, binary_path: &Path) -> std::io::Result<()> {
    let plist = crate::plist_path(profile);
    if !plist.exists() {
        install(profile, binary_path)?;
    }
    kickstart(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn generated_plist_contains_label_binary_log_profile() {
        let xml = generate_plist(
            "dev",
            &PathBuf::from("/usr/local/bin/vmux_service"),
            &PathBuf::from("/tmp/vmux-dev.log"),
        );
        assert!(xml.contains("<string>ai.vmux.service.dev</string>"));
        assert!(xml.contains("<string>/usr/local/bin/vmux_service</string>"));
        assert!(xml.contains("<string>/tmp/vmux-dev.log</string>"));
        assert!(xml.contains("<key>VMUX_PROFILE</key>"));
        assert!(xml.contains("<string>dev</string>"));
        assert!(xml.contains("<key>RunAtLoad</key>\n  <false/>"));
        assert!(xml.contains("<key>KeepAlive</key>"));
        assert!(xml.contains("<key>Crashed</key>\n    <true/>"));
    }
}
```

- [ ] **Step 3: Run the unit test**

```bash
env -u CEF_PATH cargo test -p vmux_service launchd::tests 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 4: Manual smoke test for install/uninstall**

```bash
cargo build -p vmux_service 2>&1 | tail -5
BIN="$PWD/target/debug/vmux_service"

# Use a tiny driver script
cat > /tmp/launchd-smoke.rs <<'EOF'
fn main() {
    let bin = std::env::args().nth(1).expect("binary path");
    vmux_service::launchd::install("dev", std::path::Path::new(&bin)).expect("install");
    println!("installed; sleeping 2s for launchd...");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let plist = vmux_service::plist_path("dev");
    println!("plist at: {}", plist.display());
    let pid = std::fs::read_to_string(vmux_service::pid_path()).unwrap_or_default();
    println!("pid file: {}", pid.trim());
    vmux_service::launchd::uninstall("dev").expect("uninstall");
    println!("uninstalled");
}
EOF

# Run via cargo --example or by hand: skip on CI, do manually before merge.
```

(This is exploratory — formal coverage is via the unit test + acceptance criteria. Document the manual run in the PR description.)

- [ ] **Step 5: Clippy + fmt**

```bash
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_service -- --check
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/
git commit -m "feat(VMX-116): launchd module — plist gen, install/uninstall, kickstart"
```

---

## Phase E: Status protocol + CLI surface

### Task 7: Add `Status` request + `StatusResponse` to protocol; server replies

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs`
- Modify: `crates/vmux_service/src/server.rs`

- [ ] **Step 1: Add new variants**

In `crates/vmux_service/src/protocol.rs`:

```rust
// in ClientMessage:
    Status,

// in ServiceMessage:
    StatusResponse {
        uptime_secs: u64,
        process_count: u32,
    },
```

- [ ] **Step 2: Add roundtrip test**

In `protocol.rs` `tests` module:

```rust
    #[test]
    fn status_response_roundtrips() {
        let msg = ServiceMessage::StatusResponse {
            uptime_secs: 42,
            process_count: 3,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(matches!(
            decoded,
            ServiceMessage::StatusResponse { uptime_secs: 42, process_count: 3 }
        ));
    }
```

- [ ] **Step 3: Implement server handler**

In `server.rs`, where `ClientMessage::ListProcesses` is handled, add a peer arm. The server must track its own start time. Add a constant near the top:

```rust
use std::sync::OnceLock;
use std::time::Instant;

static SERVICE_STARTED: OnceLock<Instant> = OnceLock::new();

/// Initialize the server start time (called by run_server before accept loop).
pub(crate) fn init_started_at() {
    SERVICE_STARTED.get_or_init(Instant::now);
}
```

In `run_server`, call `init_started_at()` before the accept loop.

In the message handler:

```rust
ClientMessage::Status => {
    let uptime_secs = SERVICE_STARTED
        .get()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);
    let process_count = process_table.len() as u32;  // adapt to whatever the per-handler state name is
    write_message!(&mut writer, &ServiceMessage::StatusResponse {
        uptime_secs,
        process_count,
    })?;
}
```

(Adapt `process_table.len()` to the actual server-side process collection; rg `processes:` or `HashMap<ProcessId` in `server.rs` to find the right field.)

- [ ] **Step 4: Build + tests**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | tail -10
env -u CEF_PATH cargo test -p vmux_service 2>&1 | tail -10
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_service/src/server.rs
git commit -m "feat(VMX-116): Status request + uptime/process_count response"
```

---

### Task 8: `cli` module + `format_status`

**Files:**
- Create: `crates/vmux_service/src/cli.rs`
- Modify: `crates/vmux_service/src/lib.rs` (add `pub mod cli;`)

- [ ] **Step 1: Add module decl**

In `crates/vmux_service/src/lib.rs`:

```rust
pub mod cli;
```

- [ ] **Step 2: Create cli.rs with `format_status` + dispatch surface**

```rust
//! Implementation of `vmux service ...` subcommands.

use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub profile: String,
    pub pid: Option<i32>,
    pub uptime: Option<Duration>,
    pub socket: std::path::PathBuf,
    pub identity_short: Option<String>,
    pub process_count: Option<u32>,
}

pub fn format_status(s: &StatusInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("profile     {}\n", s.profile));
    out.push_str(&format!(
        "pid         {}\n",
        s.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!(
        "uptime      {}\n",
        s.uptime.map(format_uptime).unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!("socket      {}\n", s.socket.display()));
    out.push_str(&format!(
        "identity    {}\n",
        s.identity_short.clone().unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!(
        "processes   {}\n",
        s.process_count.map(|c| c.to_string()).unwrap_or_else(|| "-".into())
    ));
    out
}

fn format_uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (h, rem) = (s / 3600, s % 3600);
    let (m, sec) = (rem / 60, rem % 60);
    if h > 0 { format!("{h}h {m}m {sec}s") }
    else if m > 0 { format!("{m}m {sec}s") }
    else { format!("{sec}s") }
}

fn read_pid() -> Option<i32> {
    std::fs::read_to_string(crate::pid_path()).ok()
        .and_then(|s| s.trim().parse().ok())
}

fn read_identity_short() -> Option<String> {
    std::fs::read_to_string(crate::identity_path()).ok().map(|s| {
        // Take first 8 chars of a hash of the identity string.
        let mut hash: u64 = 5381;
        for b in s.trim().bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(b as u64);
        }
        format!("{hash:016x}")[..8].to_string()
    })
}

/// Connect to the running daemon and ask for live counts. Returns None if not running.
fn live_status() -> Option<(u64, u32)> {
    use crate::protocol::{ClientMessage, ServiceMessage};
    let stream = std::os::unix::net::UnixStream::connect(crate::socket_path()).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut stream = stream;
    crate::framing::write_message_blocking(&mut stream, &ClientMessage::Status).ok()?;
    let mut reader = std::io::BufReader::new(&mut stream);
    let msg = crate::framing::read_message_blocking::<_, ServiceMessage>(&mut reader).ok()??;
    match msg {
        ServiceMessage::StatusResponse { uptime_secs, process_count } =>
            Some((uptime_secs, process_count)),
        _ => None,
    }
}

pub fn cmd_status() -> std::io::Result<i32> {
    let pid = read_pid();
    let live = live_status();
    let info = StatusInfo {
        profile: crate::current_profile().to_string(),
        pid,
        uptime: live.map(|(s, _)| Duration::from_secs(s)),
        socket: crate::socket_path(),
        identity_short: read_identity_short(),
        process_count: live.map(|(_, c)| c),
    };
    print!("{}", format_status(&info));
    Ok(if live.is_some() { 0 } else { 1 })
}

#[cfg(target_os = "macos")]
pub fn cmd_install(binary_path: &Path) -> std::io::Result<i32> {
    let profile = crate::current_profile();
    let plist = crate::launchd::install(profile, binary_path)?;
    println!("installed: {}", plist.display());
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_uninstall() -> std::io::Result<i32> {
    let profile = crate::current_profile();
    crate::launchd::uninstall(profile)?;
    println!("uninstalled: {}", crate::plist_path(profile).display());
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_start(binary_path: &Path) -> std::io::Result<i32> {
    let profile = crate::current_profile();
    crate::launchd::ensure_running(profile, binary_path)?;
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_stop() -> std::io::Result<i32> {
    let profile = crate::current_profile();
    crate::launchd::bootout(profile)?;
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_restart(binary_path: &Path) -> std::io::Result<i32> {
    let profile = crate::current_profile();
    let _ = crate::launchd::bootout(profile);
    crate::launchd::ensure_running(profile, binary_path)?;
    Ok(0)
}

pub fn cmd_logs(follow: bool) -> std::io::Result<i32> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new("tail");
    if follow {
        cmd.arg("-f");
    }
    cmd.arg(crate::log_path());
    let err = cmd.exec();
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn format_uptime_formats_segments() {
        assert_eq!(format_uptime(Duration::from_secs(0)), "0s");
        assert_eq!(format_uptime(Duration::from_secs(45)), "45s");
        assert_eq!(format_uptime(Duration::from_secs(75)), "1m 15s");
        assert_eq!(format_uptime(Duration::from_secs(3601)), "1h 0m 1s");
    }

    #[test]
    fn format_status_renders_all_fields() {
        let info = StatusInfo {
            profile: "dev".into(),
            pid: Some(12345),
            uptime: Some(Duration::from_secs(60)),
            socket: PathBuf::from("/tmp/vmux-dev.sock"),
            identity_short: Some("abcd1234".into()),
            process_count: Some(2),
        };
        let out = format_status(&info);
        assert!(out.contains("profile     dev"));
        assert!(out.contains("pid         12345"));
        assert!(out.contains("uptime      1m 0s"));
        assert!(out.contains("socket      /tmp/vmux-dev.sock"));
        assert!(out.contains("identity    abcd1234"));
        assert!(out.contains("processes   2"));
    }

    #[test]
    fn format_status_renders_dashes_when_unknown() {
        let info = StatusInfo {
            profile: "dev".into(),
            pid: None,
            uptime: None,
            socket: PathBuf::from("/tmp/vmux-dev.sock"),
            identity_short: None,
            process_count: None,
        };
        let out = format_status(&info);
        assert!(out.contains("pid         -"));
        assert!(out.contains("uptime      -"));
        assert!(out.contains("identity    -"));
        assert!(out.contains("processes   -"));
    }
}
```

- [ ] **Step 3: Run unit tests**

```bash
env -u CEF_PATH cargo test -p vmux_service cli:: 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 4: Build + clippy + fmt**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_service -- --check
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/
git commit -m "feat(VMX-116): vmux_service::cli — status formatter + subcommand handlers"
```

---

### Task 9: Wire `vmux service ...` clap subcommand in `vmux_cli`

**Files:**
- Modify: `crates/vmux_cli/Cargo.toml`
- Modify: `crates/vmux_cli/src/commands.rs`
- Create: `crates/vmux_cli/src/commands/service.rs`
- Modify: `crates/vmux_cli/src/main.rs`

- [ ] **Step 1: Add dep**

In `crates/vmux_cli/Cargo.toml` `[dependencies]`:

```toml
vmux_service = { path = "../vmux_service" }
```

- [ ] **Step 2: Add `Service` variant to `Command` enum in `commands.rs`**

```rust
// at the top
pub mod service;

// in Command enum:
    Service(service::ServiceArgs),
```

- [ ] **Step 3: Create `crates/vmux_cli/src/commands/service.rs`**

```rust
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: ServiceAction,
}

#[derive(Debug, Subcommand)]
pub enum ServiceAction {
    /// Print daemon status
    Status,
    /// Start (kickstart) the daemon
    Start,
    /// Stop the daemon (bootout)
    Stop,
    /// Restart the daemon
    Restart,
    /// Tail the service log file
    Logs {
        /// Follow new log lines as they arrive
        #[arg(short, long)]
        follow: bool,
    },
    /// Install the LaunchAgent plist for this profile
    Install,
    /// Uninstall the LaunchAgent plist for this profile
    Uninstall,
}

pub fn run(args: ServiceArgs) -> std::io::Result<i32> {
    let bin = current_service_binary()?;
    match args.action {
        ServiceAction::Status => vmux_service::cli::cmd_status(),
        ServiceAction::Start => {
            #[cfg(target_os = "macos")]
            { vmux_service::cli::cmd_start(&bin) }
            #[cfg(not(target_os = "macos"))]
            { let _ = bin; not_supported() }
        }
        ServiceAction::Stop => {
            #[cfg(target_os = "macos")]
            { vmux_service::cli::cmd_stop() }
            #[cfg(not(target_os = "macos"))]
            { not_supported() }
        }
        ServiceAction::Restart => {
            #[cfg(target_os = "macos")]
            { vmux_service::cli::cmd_restart(&bin) }
            #[cfg(not(target_os = "macos"))]
            { let _ = bin; not_supported() }
        }
        ServiceAction::Logs { follow } => vmux_service::cli::cmd_logs(follow),
        ServiceAction::Install => {
            #[cfg(target_os = "macos")]
            { vmux_service::cli::cmd_install(&bin) }
            #[cfg(not(target_os = "macos"))]
            { let _ = bin; not_supported() }
        }
        ServiceAction::Uninstall => {
            #[cfg(target_os = "macos")]
            { vmux_service::cli::cmd_uninstall() }
            #[cfg(not(target_os = "macos"))]
            { not_supported() }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn not_supported() -> std::io::Result<i32> {
    eprintln!("vmux service: launchd commands are macOS-only");
    Ok(2)
}

/// Resolve the path to the `vmux_service` daemon binary.
/// Strategy: same dir as the current `vmux` executable.
fn current_service_binary() -> std::io::Result<std::path::PathBuf> {
    let mut p = std::env::current_exe()?;
    p.pop();
    p.push("vmux_service");
    Ok(p)
}
```

- [ ] **Step 4: Route the variant in `main.rs`**

Edit `crates/vmux_cli/src/main.rs`:

```rust
use clap::Parser;

mod commands;

use commands::{Cli, Command, open::OpenAppLauncher};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Mcp) => commands::mcp::run().await,
        Some(Command::Service(args)) => {
            let code = commands::service::run(args)?;
            std::process::exit(code);
        }
        None => commands::open::run(&OpenAppLauncher),
    }
}
```

- [ ] **Step 5: Build + smoke**

```bash
env -u CEF_PATH cargo build -p vmux_cli 2>&1 | tail -10
./target/debug/vmux service --help 2>&1 | head -20
./target/debug/vmux service status 2>&1 | head -20
```

Expected: help shows subcommands; `status` prints either populated or all-dashes table (depending on whether daemon is up).

- [ ] **Step 6: Clippy + fmt**

```bash
env -u CEF_PATH cargo clippy -p vmux_cli --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_cli -- --check
```

Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_cli/
git commit -m "feat(VMX-116): vmux service CLI subcommand"
```

---

## Phase F: Webview merge — absorb vmux_process into vmux_service

### Task 10: Move `vmux_process` source into `vmux_service::webview`

**Files:**
- Move: `crates/vmux_process/src/event.rs` → `crates/vmux_service/src/webview/event.rs`
- Move: `crates/vmux_process/src/plugin.rs` → `crates/vmux_service/src/webview/plugin.rs`
- Move: `crates/vmux_process/src/app.rs` → `crates/vmux_service/src/webview/app.rs`
- Move: `crates/vmux_process/src/main.rs` → `crates/vmux_service/src/webview/main.rs`
- Move: `crates/vmux_process/build.rs` → merge into new `crates/vmux_service/build.rs`
- Create: `crates/vmux_service/src/webview.rs`
- Modify: `crates/vmux_service/src/lib.rs`
- Modify: `crates/vmux_service/Cargo.toml`
- Delete: `crates/vmux_process/` (whole crate)
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Inspect `vmux_process` build.rs and merge into the `vmux_service` build.rs**

Open `crates/vmux_process/build.rs` (already read in design phase). Update `crates/vmux_service/build.rs` (created in Task 1) to also run the webview builder:

```rust
use std::path::PathBuf;

fn main() {
    // VMUX_PROFILE plumbing (kept from Task 1)
    let profile = std::env::var("VMUX_PROFILE").unwrap_or_else(|_| {
        match std::env::var("PROFILE").as_deref() {
            Ok("release") => "release".to_string(),
            _ => "dev".to_string(),
        }
    });
    println!("cargo::rustc-env=VMUX_PROFILE={profile}");
    println!("cargo::rerun-if-env-changed=VMUX_PROFILE");

    // Webview build (merged from vmux_process/build.rs)
    use vmux_webview_app::build::{CefEmbeddedWebviewFinalize, WebviewAppBuilder};
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_service", "vmux_service_app")
        .track_manifest_rel_paths(&["tailwind.config.js", "../vmux_ui/assets/theme.css"])
        .dx_extra_args(&["--bin", "vmux_service_app", "--features", "web"])
        .cef_finalize(CefEmbeddedWebviewFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxs", "services-dxs"])
        .run("vmux_service");
}
```

Add `vmux_webview_app = { path = "../vmux_webview_app", features = ["build"] }` under `[build-dependencies]` in `crates/vmux_service/Cargo.toml`.

Also copy `crates/vmux_process/tailwind.config.js` → `crates/vmux_service/tailwind.config.js`. Confirm it exists with `ls crates/vmux_process/tailwind.config.js`.

- [ ] **Step 2: Move source files**

```bash
mkdir -p crates/vmux_service/src/webview
git mv crates/vmux_process/src/event.rs crates/vmux_service/src/webview/event.rs
git mv crates/vmux_process/src/plugin.rs crates/vmux_service/src/webview/plugin.rs
git mv crates/vmux_process/src/app.rs crates/vmux_service/src/webview/app.rs
git mv crates/vmux_process/src/main.rs crates/vmux_service/src/webview/main.rs
git mv crates/vmux_process/tailwind.config.js crates/vmux_service/tailwind.config.js
# Note: lib.rs of vmux_process was just `pub mod event; #[cfg(...)] include!("plugin.rs");` — discarded
```

- [ ] **Step 3: Create `crates/vmux_service/src/webview.rs`**

```rust
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::ServicesPlugin;
```

- [ ] **Step 4: Rename plugin in `webview/plugin.rs`**

Edit `crates/vmux_service/src/webview/plugin.rs`. Rename `ProcessesPlugin` → `ServicesPlugin`:

```rust
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

pub struct ServicesPlugin;

impl Plugin for ServicesPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("services"),
            );
    }
}
```

- [ ] **Step 5: Fix imports in moved files**

In `crates/vmux_service/src/webview/app.rs`, change:

```rust
use vmux_process::event::*;
```

to

```rust
use crate::webview::event::*;
```

Wait — this file is the wasm bin's source. It compiles as part of the `vmux_service_app` bin which runs in wasm. The wasm bin needs to import via `vmux_service::webview::event::*` if it's a separate compilation unit. Check: `webview/main.rs` is the bin entry; `webview/app.rs` is included how? In `vmux_process` it was a direct sibling module of `main.rs`:

```rust
// vmux_process/src/main.rs
mod app;

fn main() {
    dioxus::launch(app::App);
}
```

Translate to `crates/vmux_service/src/webview/main.rs`:

```rust
mod app;

fn main() {
    dioxus::launch(app::App);
}
```

And in `webview/app.rs`, the import becomes:

```rust
use vmux_service::webview::event::*;
```

But the wasm bin compiles `webview/main.rs` as a separate root with `mod app;`, so within `app.rs` `crate::` refers to the bin crate, not the lib. Use `vmux_service::webview::event::*` (lib re-exports).

Actually simpler: copy `event.rs` content as a sibling of the bin. The cleanest pattern matches existing crates: `event.rs` lives at top of crate (used by both lib and bin). Look at `vmux_process` for reference:

```bash
cat crates/vmux_process/src/lib.rs
# pub mod event;
# #[cfg(not(target_arch = "wasm32"))]
# include!("plugin.rs");
```

So `event.rs` was at crate root, accessible by both wasm bin (`vmux_process::event::*`) and host plugin. Mirror that:

Move `webview/event.rs` to `crates/vmux_service/src/webview/event.rs` AS PLANNED, but expose it from the lib at the same path. The wasm bin imports `vmux_service::webview::event::*`. That works as long as the lib is wasm-compatible. Check vmux_process — its lib is `pub mod event;` so the lib WAS compiled for wasm too.

Confirm by inspecting `crates/vmux_service/Cargo.toml` `[lib]`. Make sure no host-only deps gate `event.rs`.

`event.rs` has no host-specific deps (only serde/rkyv). It's safe under wasm.

But — `vmux_service/src/lib.rs` itself currently `pub mod client;` etc., which DO have host deps (tokio, libc). These break wasm. Solution: cfg-gate everything except `event.rs` and other wasm-safe modules.

Add to `crates/vmux_service/src/lib.rs`:

```rust
pub mod webview;

#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod framing;
#[cfg(not(target_arch = "wasm32"))]
pub mod process;
pub mod protocol;        // wasm-safe (no host deps)
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod service;
#[cfg(not(target_arch = "wasm32"))]
pub mod supervisor;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod launchd;

// Path/identity helpers — wasm doesn't need them but they don't pull host deps.
#[cfg(not(target_arch = "wasm32"))]
mod paths;
#[cfg(not(target_arch = "wasm32"))]
pub use paths::*;
```

(Move the path helpers into `paths.rs` to keep the cfg-gating tidy. Don't forget to update `crate::socket_path()` etc. references — they're already at lib root via `pub use`.)

`protocol.rs` may import `vmux_terminal::event::TermLine` etc. — verify those are wasm-safe. If not, also cfg-gate `protocol`.

Check: `rg 'use vmux_' crates/vmux_service/src/protocol.rs`:

```bash
rg '^use ' crates/vmux_service/src/protocol.rs | head
```

If protocol uses host-only crates, add `#[cfg(not(target_arch = "wasm32"))]` to it too. (The webview only uses `event.rs`, not protocol, so this is fine either way.)

- [ ] **Step 6: Update `crates/vmux_service/Cargo.toml`**

```toml
[package]
name = "vmux_service"
version.workspace = true
edition.workspace = true
description = "Background service hosting persistent terminal processes (and its monitor webview)"
publish = false
build = "build.rs"

[features]
default = []
web = []

[[bin]]
name = "vmux_service"
path = "src/main.rs"

[[bin]]
name = "vmux_service_app"
path = "src/webview/main.rs"
required-features = ["web"]

[lib]
name = "vmux_service"
path = "src/lib.rs"

[build-dependencies]
vmux_webview_app = { path = "../vmux_webview_app", features = ["build"] }

[dependencies]
rkyv = { workspace = true }
serde = { workspace = true }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
libc = "0.2"
uuid = { workspace = true }
tokio = { workspace = true }
alacritty_terminal = { workspace = true }
portable-pty = { workspace = true }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-appender = "0.2"
vmux_core = { path = "../vmux_core" }
vmux_terminal = { path = "../vmux_terminal" }
vmux_webview_app = { path = "../vmux_webview_app" }

[target.'cfg(target_arch = "wasm32")'.dependencies]
dioxus = { workspace = true }
vmux_ui = { path = "../vmux_ui", default-features = false }
wasm-bindgen = { workspace = true }
web-sys = { version = "0.3", features = ["Window", "Document", "Element", "HtmlElement"] }
```

- [ ] **Step 7: Update consumer imports**

```bash
rg -l 'vmux_process' crates/ Cargo.toml
```

For each match:

| File | Change |
|---|---|
| `Cargo.toml` (workspace) | remove `crates/vmux_process` from `members` |
| `crates/vmux_desktop/Cargo.toml` | remove `vmux_process` line |
| `crates/vmux_desktop/src/lib.rs` | `vmux_process::ProcessesPlugin` → `vmux_service::webview::ServicesPlugin` (both occurrences) |
| `crates/vmux_desktop/src/terminal.rs:319` | `vmux_process::event::PROCESSES_WEBVIEW_URL` → `vmux_service::webview::event::PROCESSES_WEBVIEW_URL` |
| `crates/vmux_desktop/src/agent.rs:311` | same |
| `crates/vmux_desktop/src/processes_monitor.rs:7` | `use vmux_process::event::*` → `use vmux_service::webview::event::*` |

- [ ] **Step 8: Delete the empty `vmux_process` directory**

```bash
git rm -r crates/vmux_process/
```

- [ ] **Step 9: Build everything**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | tail -15
env -u CEF_PATH cargo build -p vmux_desktop 2>&1 | tail -15
```

Expected: clean. Fix any leftover import errors before proceeding.

- [ ] **Step 10: Tests + clippy + fmt for both crates**

```bash
env -u CEF_PATH cargo test -p vmux_service 2>&1 | tail -10
env -u CEF_PATH cargo test -p vmux_desktop 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_service -p vmux_desktop -- --check
```

Expected: green.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "refactor(VMX-116): merge vmux_process into vmux_service::webview, delete crate"
```

---

## Phase G: vmux_desktop integration

### Task 11: Drop `service` subcommand and `run_service()` from `vmux_desktop/main.rs`

**Files:**
- Modify: `crates/vmux_desktop/src/main.rs`

- [ ] **Step 1: Remove the early subcommand check + run_service**

Edit `crates/vmux_desktop/src/main.rs`. Delete:

```rust
    // Check for `service` subcommand before any GUI/Bevy initialization.
    if std::env::args().nth(1).as_deref() == Some("service") {
        run_service();
        return;
    }
```

And the entire `run_service()` function (lines ~71-105 in current file).

- [ ] **Step 2: Build**

```bash
env -u CEF_PATH cargo build -p vmux_desktop 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/main.rs
git commit -m "refactor(VMX-116): drop vmux_desktop service subcommand (canonical bin is vmux_service)"
```

---

### Task 12: Replace `ensure_service_started` with `launchd::ensure_running` + exp-backoff connect

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Locate the daemon path resolution helper**

The GUI needs to know where the `vmux_service` binary lives. Strategy: same directory as the GUI binary (`Vmux.app/Contents/MacOS/vmux_service` for release, `target/debug/vmux_service` for dev).

Add near the existing `ensure_service_started`:

```rust
fn vmux_service_binary() -> std::io::Result<std::path::PathBuf> {
    let mut p = std::env::current_exe()?;
    p.pop();
    p.push("vmux_service");
    Ok(p)
}
```

- [ ] **Step 2: Replace `ensure_service_started` body**

Replace the entire function body:

```rust
fn ensure_service_started() {
    if ServiceHandle::service_running() {
        tracing::info!("service already running");
        return;
    }
    let binary = match vmux_service_binary() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "could not locate vmux_service binary");
            return;
        }
    };
    #[cfg(target_os = "macos")]
    {
        let profile = vmux_service::current_profile();
        if let Err(e) = vmux_service::launchd::ensure_running(profile, &binary) {
            tracing::error!(error = %e, "launchd ensure_running failed");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Linux/CI fallback: spawn directly with setsid for now.
        use std::os::unix::process::CommandExt;
        let log_dir = vmux_service::service_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let stderr_cfg = std::fs::File::create(vmux_service::log_path())
            .map(std::process::Stdio::from)
            .unwrap_or(std::process::Stdio::null());
        unsafe {
            std::process::Command::new(&binary)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(stderr_cfg)
                .pre_exec(|| { libc::setsid(); Ok(()) })
                .spawn()
                .ok();
        }
    }
}
```

Add `use vmux_service` import at top of file if not already present. Add `tracing` import.

Add `tracing = "0.1"` to `crates/vmux_desktop/Cargo.toml` `[dependencies]` if not present.

- [ ] **Step 3: Replace fixed retry with exponential backoff in `try_connect_service`**

Find `try_connect_service` (around line 644). Replace the timer-based retry resource with one that doubles its interval. First, change the `ServiceConnectRetry` resource definition:

```bash
rg -n 'ServiceConnectRetry' crates/vmux_desktop/src/
```

Modify the resource:

```rust
#[derive(Resource)]
struct ServiceConnectRetry {
    timer: Timer,
    next_delay_ms: u64,
    remaining_attempts: u32,
}

impl ServiceConnectRetry {
    fn new() -> Self {
        Self {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            next_delay_ms: 50,
            remaining_attempts: 6,
        }
    }
}
```

In the system, after a failed connect, schedule the next attempt with doubled delay (capped):

```rust
if retry.timer.just_finished() {
    // ... existing connect attempt ...
    // on failure:
    retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
    retry.timer = Timer::new(
        std::time::Duration::from_millis(retry.next_delay_ms),
        TimerMode::Once,
    );
    retry.remaining_attempts -= 1;
}
```

Update the resource constructor call site to use `ServiceConnectRetry::new()`.

- [ ] **Step 4: Build**

```bash
env -u CEF_PATH cargo build -p vmux_desktop 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 5: Clippy + fmt**

```bash
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_desktop -- --check
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/
git commit -m "feat(VMX-116): GUI uses launchd ensure_running + exp-backoff connect"
```

---

## Phase H: Connect-error overlay in terminal webview

### Task 13: `ServiceUnavailableEvent` + Bevy emitter + Dioxus overlay

**Files:**
- Modify: `crates/vmux_service/src/webview/event.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs` (or the relevant connect-failure system)
- Modify: `crates/vmux_terminal/src/event.rs`
- Modify: `crates/vmux_terminal/src/app.rs`

- [ ] **Step 1: Add the event type**

In `crates/vmux_terminal/src/event.rs` (so the terminal webview consumes it directly without crossing crate boundaries):

```rust
/// Event name for service availability errors (host -> terminal webview).
pub const SERVICE_UNAVAILABLE_EVENT: &str = "service_unavailable";

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ServiceUnavailableEvent {
    /// Empty string clears the overlay.
    pub message: String,
}
```

(Make sure `serde::{Serialize, Deserialize}` is in scope.)

- [ ] **Step 2: Emit from the GUI when retries exhausted**

In `crates/vmux_desktop/src/terminal.rs`, locate the branch where the retry loop gives up (around line 660-680). After the existing `tracing::error!` log, broadcast the event to all terminal webviews:

```rust
fn broadcast_service_unavailable(
    cef_emit: &CefBinEmit,                      // existing wiring; rg the actual type
    terminals: &Query<&CefWebview, With<Terminal>>,
    message: String,
) {
    use vmux_terminal::event::{SERVICE_UNAVAILABLE_EVENT, ServiceUnavailableEvent};
    let evt = ServiceUnavailableEvent { message };
    for webview in terminals.iter() {
        cef_emit.emit_rkyv(webview, SERVICE_UNAVAILABLE_EVENT, &evt);
    }
}
```

Adapt `CefBinEmit`/`CefWebview`/`Terminal` to whatever the existing types are — look at how `processes_monitor.rs` emits to its webview for the exact pattern. Reuse the same primitive.

In `try_connect_service` after final retry fails:

```rust
broadcast_service_unavailable(
    &cef_emit,
    &terminal_webviews,
    "vmux service unavailable — run `vmux service logs` for details.".into(),
);
```

In the success path (when `ServiceClient` is inserted after a reconnect), broadcast an empty message to clear:

```rust
broadcast_service_unavailable(&cef_emit, &terminal_webviews, String::new());
```

- [ ] **Step 3: Render overlay in dioxus app**

In `crates/vmux_terminal/src/app.rs`, near other `use_signal` calls, add:

```rust
let mut service_error = use_signal(String::new);

let _err_listener = use_bin_event_listener::<ServiceUnavailableEvent, _>(
    SERVICE_UNAVAILABLE_EVENT,
    move |evt| service_error.set(evt.message),
);
```

In the rsx tree, add an overlay sibling to the terminal grid (top of the visible component):

```rust
{
    let msg = service_error.read().clone();
    if !msg.is_empty() {
        rsx! {
            div {
                class: "absolute inset-0 z-50 flex items-center justify-center bg-term-bg/80 text-ansi-1",
                div {
                    class: "rounded-md border border-ansi-1 bg-term-bg px-4 py-2 text-sm",
                    "{msg}"
                }
            }
        }
    } else {
        rsx! {}
    }
}
```

(Tailwind utility classes — they're scanned by the existing `_TW_SAFELIST` if needed; otherwise add `bg-term-bg/80`, `text-ansi-1`, etc. to the safelist.)

Add the import at the top:

```rust
use vmux_terminal::event::{SERVICE_UNAVAILABLE_EVENT, ServiceUnavailableEvent};
```

- [ ] **Step 4: Build everything**

```bash
env -u CEF_PATH cargo build -p vmux_terminal -p vmux_desktop 2>&1 | tail -15
```

Expected: clean.

- [ ] **Step 5: Tests + clippy + fmt**

```bash
env -u CEF_PATH cargo test -p vmux_terminal -p vmux_desktop 2>&1 | tail -10
env -u CEF_PATH cargo clippy -p vmux_terminal -p vmux_desktop --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt -p vmux_terminal -p vmux_desktop -- --check
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/ crates/vmux_desktop/
git commit -m "feat(VMX-116): in-terminal overlay for service-unavailable state"
```

---

## Phase I: Acceptance + cleanup

### Task 14: Manual acceptance smoke + README/migration note

**Files:**
- Modify: `README.md` (release-notes section, if present)
- Verification only otherwise

- [ ] **Step 1: Build a debug binary**

```bash
env -u CEF_PATH cargo build -p vmux_service -p vmux_desktop -p vmux_cli 2>&1 | tail -5
```

- [ ] **Step 2: Acceptance criterion 1 — daemon survives GUI exit**

```bash
# Start fresh
launchctl bootout gui/$(id -u)/ai.vmux.service.dev 2>/dev/null || true
rm -f "$HOME/Library/LaunchAgents/ai.vmux.service.dev.plist"
rm -f "$HOME/Library/Application Support/Vmux/services/vmux-dev."*

# Launch GUI (will install plist + kickstart)
./target/debug/vmux_desktop &
GUI=$!
sleep 3

# Daemon should be alive
launchctl print gui/$(id -u)/ai.vmux.service.dev | head -5

# Kill the GUI
kill $GUI
sleep 2

# Daemon should still be running
launchctl print gui/$(id -u)/ai.vmux.service.dev | head -5
```

Expected: daemon present after GUI death.

- [ ] **Step 3: Acceptance criterion 2 — launchd respawns on crash**

```bash
PID=$(cat "$HOME/Library/Application Support/Vmux/services/vmux-dev.pid")
kill -9 $PID
sleep 2
NEW_PID=$(cat "$HOME/Library/Application Support/Vmux/services/vmux-dev.pid")
[ "$PID" != "$NEW_PID" ] && echo "respawn OK ($PID -> $NEW_PID)" || echo "FAILED"
```

Expected: `respawn OK ...`.

- [ ] **Step 4: Acceptance criterion 3 — identity-mismatch upgrade**

```bash
# Touch the binary so identity changes
touch -m target/debug/vmux_service
# Open GUI; it should kill old daemon and start a new one
./target/debug/vmux_desktop &
GUI=$!
sleep 4
# Look at log for "identity mismatch" + "exited via Shutdown handshake"
tail -20 "$HOME/Library/Application Support/Vmux/services/vmux-dev."*.log
kill $GUI
```

Expected: log shows identity mismatch and graceful shutdown.

- [ ] **Step 5: Acceptance criterion 4 — `vmux service status` from fresh shell**

```bash
launchctl bootout gui/$(id -u)/ai.vmux.service.dev 2>/dev/null
rm -f "$HOME/Library/Application Support/Vmux/services/vmux-dev.pid"
./target/debug/vmux service status
./target/debug/vmux service start
./target/debug/vmux service status
```

Expected: first call shows pid `-` and processes `-`; after start, populated values.

- [ ] **Step 6: Acceptance criterion 5 — no eprintln, structured log**

```bash
rg 'eprintln!' crates/vmux_service/src/ crates/vmux_desktop/src/main.rs crates/vmux_desktop/src/terminal.rs
./target/debug/vmux service logs | head -20
```

Expected: no `eprintln!` matches in service files; log output is structured (level, target, fields).

- [ ] **Step 7: Acceptance criterion 6 — `vmux_process` gone**

```bash
ls crates/vmux_process 2>&1
rg 'vmux_process' crates/ Cargo.toml
```

Expected: directory gone, no remaining references.

- [ ] **Step 8: Acceptance criterion 7 — connect-error overlay**

Manual: stop the daemon while the GUI is running, check that the terminal webview shows the overlay; restart and confirm overlay clears.

```bash
./target/debug/vmux_desktop &
sleep 3
launchctl bootout gui/$(id -u)/ai.vmux.service.dev
# Visually verify overlay appears in any open terminal tab
sleep 5
./target/debug/vmux service start
# Visually verify overlay clears
```

- [ ] **Step 9: Add migration note to README**

In `README.md`, find the Releases / Upgrade section (or add one near the top of any user-facing notes). Append:

```markdown
### Upgrade from <0.0.5

The service daemon now uses per-profile filenames. After your first launch on the new build, you can remove the legacy files:

```sh
rm -f "$HOME/Library/Application Support/Vmux/services/"{vmux.sock,service.pid,service.identity}
```
```

- [ ] **Step 10: Run full pre-commit checks on all changed crates**

Per `AGENTS.md`:

```bash
BASE="${BASE:-main}"
ROOT="$(git rev-parse --show-toplevel)"
CHANGED_PKGS=$(
  cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[]
      | select(.manifest_path | test("patches") | not)
      | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
  | while IFS=$'\t' read -r name dir; do
      rel="${dir#"$ROOT"/}"
      [ -z "$rel" ] && rel="."
      if ! git diff --quiet "$BASE" -- "$rel"; then
        echo "$name"
      fi
    done
)
for pkg in $CHANGED_PKGS; do
  cargo fmt -p "$pkg" -- --check
done
for pkg in $CHANGED_PKGS; do
  env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings
done
for pkg in $CHANGED_PKGS; do
  env -u CEF_PATH cargo test -p "$pkg"
done
```

Expected: all green.

- [ ] **Step 11: Commit migration note**

```bash
git add README.md
git commit -m "docs(VMX-116): upgrade migration note for per-profile service files"
```

- [ ] **Step 12: Open PR**

Use the open-new-pr skill.

---

## Self-review checklist (run before requesting review)

- [ ] All 7 acceptance criteria from the spec have a verifying step in Task 14.
- [ ] No `eprintln!` remains in `crates/vmux_service/src/` or in the daemon path of `crates/vmux_desktop/src/`.
- [ ] `crates/vmux_process/` is gone; workspace `Cargo.toml` no longer lists it.
- [ ] Both `[[bin]]` targets build (`vmux_service` for host, `vmux_service_app` for wasm via `--features web`).
- [ ] Per-profile resources (sock/pid/identity/log/plist/label) all use the same suffix from `current_profile()`.
- [ ] `RunAtLoad=false`, `KeepAlive.Crashed=true` in generated plist.
- [ ] Identity-mismatch path goes through `supervisor::replace_running` (not just file unlink).
- [ ] `vmux service status` returns exit 0 when running, 1 when not.
- [ ] Connect-error overlay appears and clears via `ServiceUnavailableEvent`.
