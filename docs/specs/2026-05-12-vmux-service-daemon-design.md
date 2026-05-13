# vmux_service: First-Class Supervised Daemon

**Linear:** [VMX-116](https://linear.app/vmux/issue/VMX-116/promote-vmux-service-to-a-first-class-supervised-daemon)
**Status:** Design — pending review
**Date:** 2026-05-12

## Goal

Make `vmux_service` a real daemon with an independent lifecycle: lazily started by clients, restarted on crash by `launchd`, controllable via `vmux service ...` CLI, with structured logs and clean upgrade semantics. Replace the current "subprocess of vmux_desktop, kept alive only by Unix orphan-on-parent-exit" pattern.

## Non-goals

- Linux `systemd --user` unit (separate ticket; same shape).
- Windows.
- Migrating away from Unix sockets.
- Building a generic toast/banner UI framework. We reuse the existing webview event channel for connect-error UX.

## Architecture summary

One canonical daemon binary per build profile, lazily kicked off by clients (GUI or CLI), supervised by `launchd`. tmux-shaped: server holds all PTYs; clients are thin.

```
+---------------------+        +---------------------+
| vmux_desktop (GUI)  |        | vmux (CLI)          |
+---------------------+        +---------------------+
       |   AF_UNIX                       |   AF_UNIX
       v                                  v
+--------------------------------------------------+
| vmux_service (daemon, one per profile)           |
|  - PTY host                                      |
|  - IPC server                                    |
|  - Agent command bus                             |
+--------------------------------------------------+
       ^
       | launchctl bootstrap / kickstart / bootout
+--------------------------------------------------+
| launchd LaunchAgent ai.vmux.service.{profile}    |
|  - RunAtLoad=false (lazy)                        |
|  - KeepAlive.Crashed=true (restart on crash)     |
|  - StandardOut/Err -> per-profile log            |
+--------------------------------------------------+
```

## Crate restructure

### Delete `crates/vmux_process/` entirely

The webview process monitor moves into `vmux_service`. Rationale: the daemon and its monitor UI are one logical unit. Dropping a crate boundary removes ~200 LOC of plumbing (separate `Cargo.toml`, separate `build.rs`, cross-crate event imports).

### `crates/vmux_service/` becomes the home of:

```
crates/vmux_service/
├── Cargo.toml                 # adds: clap, tracing, tracing-subscriber,
│                              #       tracing-appender, plus webview build deps;
│                              #       declares two [[bin]] targets
├── build.rs                   # NEW: runs WebviewAppBuilder for the monitor UI
├── src/
│   ├── lib.rs                 # pub mods + path/profile helpers
│   ├── framing.rs             # unchanged
│   ├── protocol.rs            # unchanged (Shutdown variant already exists)
│   ├── process.rs             # unchanged
│   ├── server.rs              # MODIFIED: tracing macros, Shutdown drain,
│   │                          #           structured startup record
│   ├── client.rs              # MODIFIED: identity-mismatch kill (not just unlink)
│   ├── service.rs             # NEW: pub fn run() — daemon entrypoint
│   ├── main.rs                # NEW: bin entry → service::run()
│   ├── launchd.rs             # NEW (cfg macos): plist gen, install/uninstall,
│   │                          #                  bootstrap/kickstart/bootout
│   ├── supervisor.rs          # NEW: graceful shutdown handshake
│   ├── cli.rs                 # NEW: status|start|stop|restart|logs|install|
│   │                          #      uninstall handlers
│   ├── webview.rs             # NEW: pub mod webview (no mod.rs per AGENTS.md)
│   └── webview/
│       ├── event.rs           # MOVED from vmux_process/src/event.rs
│       ├── plugin.rs          # MOVED from vmux_process/src/plugin.rs
│       ├── app.rs             # MOVED from vmux_process/src/app.rs (wasm component)
│       └── main.rs            # MOVED from vmux_process/src/main.rs (wasm bin)
```

`Cargo.toml` bin targets:

```toml
[[bin]]
name = "vmux_service"          # daemon (host)
path = "src/main.rs"

[[bin]]
name = "vmux_service_app"      # webview (wasm)
path = "src/webview/main.rs"
required-features = ["web"]
```

### `crates/vmux_desktop/` changes

| File | Change |
|---|---|
| `Cargo.toml` | drop `vmux_process` dep |
| `src/main.rs` | delete `service` subcommand check + `run_service()` |
| `src/lib.rs` | `vmux_process::ProcessesPlugin` → `vmux_service::webview::ServicesPlugin` |
| `src/terminal.rs` | `ensure_service_started()` → `vmux_service::launchd::ensure_running()`; replace fixed-interval connect retry with exp backoff |
| `src/processes_monitor.rs` | retarget imports from `vmux_process::event` → `vmux_service::webview::event` |
| `src/agent.rs` | retarget `vmux://services/` URL constant import |

### `crates/vmux_cli/` changes

| File | Change |
|---|---|
| `Cargo.toml` | add `vmux_service` dep |
| `src/commands.rs` | add `Command::Service(ServiceArgs)` variant |
| `src/commands/service.rs` | NEW: clap subcommand, dispatches to `vmux_service::cli` |
| `src/main.rs` | route `Service` arm |

## Per-profile resources

`VMUX_BUILD_PROFILE` env var (already populated by `crates/vmux_desktop/build.rs` and `crates/vmux_layout/build.rs`) drives every per-profile suffix. Possible values: `release`, `local`, `dev`, plus arbitrary user-set strings.

| Resource | Path |
|---|---|
| Plist | `~/Library/LaunchAgents/ai.vmux.service.{profile}.plist` |
| Label | `ai.vmux.service.{profile}` |
| Socket | `~/Library/Application Support/Vmux/services/vmux-{profile}.sock` |
| PID file | `~/Library/Application Support/Vmux/services/vmux-{profile}.pid` |
| Identity | `~/Library/Application Support/Vmux/services/vmux-{profile}.identity` |
| Log | `~/Library/Application Support/Vmux/services/vmux-{profile}.log` |

Helpers in `lib.rs`:

```rust
pub fn current_profile() -> &'static str { env!("VMUX_BUILD_PROFILE") }
pub fn launchd_label(profile: &str) -> String { format!("ai.vmux.service.{profile}") }
pub fn plist_path(profile: &str) -> PathBuf { /* ~/Library/LaunchAgents/<label>.plist */ }
pub fn socket_path() -> PathBuf { service_dir().join(format!("vmux-{}.sock", current_profile())) }
pub fn pid_path() -> PathBuf { service_dir().join(format!("vmux-{}.pid", current_profile())) }
pub fn identity_path() -> PathBuf { service_dir().join(format!("vmux-{}.identity", current_profile())) }
pub fn log_path() -> PathBuf { service_dir().join(format!("vmux-{}.log", current_profile())) }
```

## LaunchAgent plist (per profile)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>            <string>ai.vmux.service.{profile}</string>
  <key>ProgramArguments</key> <array>
    <string>{absolute-path-to-vmux_service-bin}</string>
  </array>
  <key>RunAtLoad</key>        <false/>
  <key>KeepAlive</key>        <dict>
    <key>Crashed</key>          <true/>
    <key>SuccessfulExit</key>   <false/>
  </dict>
  <key>ProcessType</key>      <string>Interactive</string>
  <key>EnvironmentVariables</key> <dict>
    <key>VMUX_BUILD_PROFILE</key>     <string>{profile}</string>
  </dict>
  <key>StandardOutPath</key>  <string>{log-path}</string>
  <key>StandardErrorPath</key><string>{log-path}</string>
</dict>
</plist>
```

**Why `RunAtLoad=false`**: tmux-shaped lazy start. Server doesn't run until the first client (GUI or `vmux service status`) kickstarts it. Crash recovery still works because `KeepAlive.Crashed=true` — once started, launchd keeps it up until an explicit `bootout`.

**Binary path resolution**:

| Profile | Path strategy |
|---|---|
| `release` | `<Vmux.app>/Contents/MacOS/vmux_service` resolved via `current_exe()` of GUI at install time |
| `local` | `cargo install --path crates/vmux_service` target, typically `~/.cargo/bin/vmux_service` |
| `dev` | `target/debug/vmux_service` resolved relative to repo root at install time |

## Lifecycle flows

### First GUI launch

```
GUI start
  ├─ if plist for current profile missing
  │    └─ generate plist → write ~/Library/LaunchAgents/ai.vmux.service.{profile}.plist
  │       launchctl bootstrap gui/$(id -u) <plist-path>
  ├─ launchctl kickstart gui/$(id -u)/ai.vmux.service.{profile}
  └─ connect_with_backoff(50ms, 100ms, 200ms, 400ms, 800ms, 1600ms)  # 6 tries
        ├─ success → ServiceClient resource ready
        └─ exhausted → emit ServiceUnavailableEvent to terminal webview
```

### Identity mismatch (upgrade case)

When `service_running()` detects the running daemon was built from a different binary than `current_exe()`:

```
1. ServiceConnection::connect to current socket
2. send ClientMessage::Shutdown
3. wait up to 2s for socket close (server drained PTYs cleanly)
4. if pid still alive → kill(pid, SIGTERM)
5. wait 500ms; if still alive → kill(pid, SIGKILL)
6. unlink socket/pid/identity files
7. launchctl kickstart (launchd respawns from updated plist binary)
```

Implemented as `vmux_service::supervisor::replace_running()`. Returns only when the old daemon is confirmed dead (or after SIGKILL timeout).

### Crash

`KeepAlive.Crashed=true` → launchd respawns within ~1s. New daemon writes new pid/identity. Existing GUI's `ServiceClient` will see EOF on its socket reader; reconnect path mirrors first-launch flow (without re-installing plist).

### `vmux service stop`

`launchctl bootout gui/$(id -u)/ai.vmux.service.{profile}` — disables KeepAlive, terminates daemon. Does NOT delete plist; subsequent `vmux service start` re-bootstraps.

### `vmux service uninstall`

`bootout` + `rm` plist + cleanup runtime files for this profile.

## CLI surface

`vmux service <subcommand>`:

| Subcommand | Behavior |
|---|---|
| `status` | Print human-readable table: pid, uptime, socket path, identity hash (short), # of live PTYs. Exit 0 if running, 1 if not. |
| `start` | `launchctl kickstart`; bootstrap first if plist missing. |
| `stop` | `launchctl bootout`. |
| `restart` | `bootout` then `kickstart`. |
| `logs [-f]` | `exec tail [-f] {log-path}`. Replaces current process. |
| `install` | Generate plist + bootstrap. Idempotent: rewrites plist if present (handles binary path drift). |
| `uninstall` | `bootout` + remove plist + remove runtime files for this profile. |

Status output example:

```
$ vmux service status
profile     dev
pid         54321
uptime      4h 12m 8s
socket      ~/Library/Application Support/Vmux/services/vmux-dev.sock
identity    a8f3c1...
processes   3
```

`status` reaches the live counts via a new `ClientMessage::Status` request returning `ServiceMessage::StatusResponse { uptime_secs, process_count }`. (`pid` and `identity` come from local files; `socket` is a constant.)

## Observability

### Tracing

Replace every `eprintln!` in `vmux_service::client`, `vmux_service::server`, and the new modules with `tracing` macros:
- `error!` for failures that affect a request
- `warn!` for recoverable anomalies (stale state cleanup, identity mismatch)
- `info!` for lifecycle (startup, shutdown, accept, drop)
- `debug!` for per-message detail
- `trace!` for byte-level framing

### Subscriber init (in `service::run`)

```rust
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt};

let appender = rolling::daily(service_dir(), format!("vmux-{}.log", current_profile()));
let (writer, _guard) = tracing_appender::non_blocking(appender);
fmt()
  .with_env_filter(EnvFilter::try_from_env("VMUX_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
  .with_writer(writer)
  .with_target(false)
  .init();
```

`tracing-appender` daily rotation. Retention of 7 days handled by passing `Rotation::DAILY` and a custom max-files via `RollingFileAppender::builder()` (added in tracing-appender 0.2.3).

### Structured startup record

```rust
info!(
    target = "vmux_service::startup",
    version = env!("CARGO_PKG_VERSION"),
    git_hash = env!("VMUX_GIT_HASH"),
    profile = current_profile(),
    pid = std::process::id(),
    socket = %socket_path().display(),
    identity = %short_identity(),
    "vmux_service started"
);
```

## Connect-error UI

Connection failures surface inside the active terminal webview, not via a separate toast/banner system.

1. Existing `ProcessesListEvent { connected, processes }` already carries a `connected` flag — keep using it for the monitor webview.
2. NEW event for terminal webview: `ServiceUnavailableEvent { message: String }` in `vmux_service::webview::event`.
3. Bevy system in `vmux_desktop/terminal.rs`: when connect retries are exhausted, send the event to every active terminal webview entity. When `ServiceClient` is reinserted (reconnect succeeds), send `ServiceUnavailableEvent { message: String::new() }` to clear.
4. Dioxus app (`crates/vmux_terminal/src/app.rs`): subscribe to the event; when `message` is non-empty, render a centered overlay over the terminal area: "vmux service unavailable — run `vmux service logs` for details."

This reuses the existing `try_cef_bin_emit_rkyv` plumbing — no new IPC primitives.

## Migration

Old single-socket files (`vmux.sock`, `service.pid`, `service.identity`) are not migrated automatically.

Release notes block:

> If you upgrade from <0.0.5, after first launch run:
> ```
> rm -f ~/Library/Application\ Support/Vmux/services/{vmux.sock,service.pid,service.identity}
> ```
> The new daemon uses per-profile filenames; the old files are harmless but stale.

## Security / safety

- `setsid` is no longer needed: launchd starts the daemon directly with no controlling terminal, in its own process group.
- For dev `cargo run` startups before the plist is installed (the brief window during install), `pre_exec(setsid)` is still applied as a safety net so the dev daemon detaches from the cargo parent group.
- Plist is written with `0644` (world-readable, owner-writable) — standard for LaunchAgents in `~/Library/LaunchAgents/`.

## Acceptance criteria (from issue, refined)

- [ ] After `make install`, killing the GUI does not kill the daemon. `launchctl print gui/$(id -u)/ai.vmux.service.release` shows it running.
- [ ] `kill -9 $(cat ~/Library/Application\ Support/Vmux/services/vmux-release.pid)` → daemon respawns within 2s (allow 1s launchd debounce + startup).
- [ ] `cargo install --path crates/vmux_service` (after a code change) followed by relaunching GUI: old daemon receives `Shutdown`, exits cleanly within 2s, new daemon starts; no socket-bind error in log.
- [ ] `vmux service status` from a fresh shell with no GUI open: bootstraps + kickstarts + connects + prints status (or starts the daemon if needed).
- [ ] No `[diag]` or `eprintln!` from service crates remains. `service.log` is structured tracing output, readable with `vmux service logs`.
- [ ] `vmux_process` crate is gone. `vmux://services/` URL still works (now served by `vmux_service` crate).
- [ ] Connect failure shows the in-terminal overlay; recovery clears it without restart.

## Test plan

### Unit
- `vmux_service::launchd::generate_plist` — golden file per profile.
- `vmux_service::supervisor::shutdown_with_timeout` — mocked client returning EOF / hanging / dead.
- `vmux_service::cli::format_status` — fixture for table output.

### Integration
- Spawn daemon manually under `tokio::test`, exercise `Shutdown` → graceful drain.
- Identity mismatch: write fake `identity` file, verify replace_running flow.

### Manual smoke (record in PR description)
- Acceptance criteria 1–7 above, run on a clean macOS box for `release` profile.
- Dev coexistence: install both `release` + `dev` plists, confirm both daemons run side-by-side, GUI in each profile attaches to its own.

## Open questions

None blocking. To revisit during implementation:
- Whether `vmux service install` should refuse with a clear error if a plist for a *different* binary path already exists (vs. silently overwriting). Lean toward overwrite + warn for now.

## References

- `crates/vmux_service/src/lib.rs` — current path/identity helpers
- `crates/vmux_service/src/client.rs:71-130` — `service_running()` + identity mismatch (the cleanup-without-kill bug)
- `crates/vmux_desktop/src/main.rs:71-104` — current `run_service()` (to be deleted)
- `crates/vmux_desktop/src/terminal.rs:602-680` — current spawn/connect/retry plumbing
- `crates/vmux_process/` — entire crate to be merged into `vmux_service`
- Origin commit: `782e6d9` "feat: persistent terminal sessions via daemon"
