# Unified Logging + Crash Capture (Electron model)

Date: 2026-06-16
Status: Draft — pending review

## Problem

The desktop (Bevy) app uses Bevy's default `LogPlugin`, which writes only to
stdout/stderr. In a bundled `.app` there is no attached terminal, so **desktop
logs vanish** and a Rust panic only goes to stderr. When the app crashes the
user has nothing to send.

The daemon (`vmux_service`) already writes a rolling daily log to
`~/Library/Application Support/Vmux/services/vmux-{profile}.log` (7-file
retention, `VMUX_LOG` filter). The desktop does not contribute to it.

Goal: surface desktop logs on disk so a user can send a report after a crash,
in **one unified log**, while still **reliably capturing crashes**.

## Constraints / prior art

The daemon and desktop are **two separate OS processes**. Two processes
daily-rotating one file race on the rename (lost lines, errors), and a desktop
panic is most reliably captured by writing **directly** to disk from the
panicking process — not by forwarding to another process that may not flush
before exit.

Electron resolves the same tension by splitting two concerns:

- **Unified text log** (`electron-log`): the main process is the sole file
  writer; renderers forward each log call over IPC. One writer, one file.
- **Crashes** (Crashpad via `crashReporter`): a separate out-of-process handler
  writes minidumps directly and survives the dying process. The logger is never
  relied on to capture a crash.

This design mirrors that split. A bug report is therefore (as in Electron) the
unified log file **plus** crash artifacts.

vmux already embeds CEF (`patches/bevy_cef-0.5.2`), which ships a Crashpad
handler; it is currently inert (no `crash_reporter.cfg`).

## Architecture overview

Three components:

- **C1 — Unified application log.** Desktop forwards log records over the
  existing Unix-domain IPC to the daemon, which is the **single writer** of
  `vmux-{profile}.log`.
- **C2 — Panic capture.** Desktop installs a panic hook that writes panics
  **directly** to a crash file (crash-safe; survives daemon-down) and
  best-effort forwards them through C1.
- **C3 — CEF Crashpad.** Ship `crash_reporter.cfg` so CEF's bundled handler
  writes minidumps for native/Chromium crashes.

All paths live under the existing service dir
`~/Library/Application Support/Vmux/services/` so report artifacts are
co-located.

```
desktop process                          daemon process (single writer)
┌───────────────────────────┐           ┌──────────────────────────────┐
│ tracing                   │           │ server.rs                     │
│  ├─ default fmt → stdout  │           │  ClientMessage::Log arm       │
│  └─ IpcLogLayer ──┐       │           │   → re-emit via tracing       │
│                   ▼       │  IPC      │   → vmux-{profile}.log (roll)  │
│  vmux-log-forward thread ─┼──────────▶│                               │
│   (blocking UnixStream,   │  socket   └──────────────────────────────┘
│    reconnect + ring buf)  │
│                           │           direct write (crash-safe)
│ panic hook ───────────────┼──────────▶ vmux-{profile}-crash.log
│ CEF (Settings.user_data_  │           CEF Crashpad handler
│  path) ───────────────────┼──────────▶ services/crashpad/ (minidumps)
└───────────────────────────┘
```

## C1 — Unified application log

### Protocol (`crates/vmux_service/src/protocol.rs`)

Add a fire-and-forget variant to `ClientMessage` (already
`rkyv::{Archive,Serialize,Deserialize}`):

```rust
Log {
    ts_ms: u64,      // desktop event time, unix epoch millis
    level: u8,       // 1=ERROR, 2=WARN, 3=INFO, 4=DEBUG, 5=TRACE
    target: String,
    message: String,
},
```

Add a pure level-mapping helper (testable, shared):

```rust
pub fn level_to_u8(level: tracing::Level) -> u8 { /* ERROR=1 .. TRACE=5 */ }
```

The desktop maps `tracing::Level → u8`; the daemon maps `u8 → const level`
(see below). No `ServiceMessage` response — the daemon never replies to `Log`.

### Desktop (`crates/vmux_desktop/src/log_forward.rs`, new)

Public entry used as `LogPlugin.custom_layer`:

```rust
pub fn ipc_log_layer(_app: &mut App) -> Option<BoxedLayer>;
```

`custom_layer` is a bare `fn(&mut App) -> Option<BoxedLayer>` (cannot capture),
so the function builds everything internally:

1. Read forward threshold from env `VMUX_LOG_FORWARD` (parse as a level),
   default `INFO`. Records at or above the threshold severity (e.g.
   `INFO`/`WARN`/`ERROR` for the default) are forwarded; more verbose records
   (`DEBUG`/`TRACE`) are skipped. Skipped records still reach stdout via Bevy's
   default layer.
2. Create a bounded `std::sync::mpsc::sync_channel::<LogRecord>(1024)`.
3. Spawn the `vmux-log-forward` OS thread (detached) owning the receiver:
   - Maintain a blocking `std::os::unix::net::UnixStream` to
     `vmux_service::socket_path()`.
   - Connect-retry with backoff. While disconnected, accumulate records in a
     bounded `VecDeque` (cap 1024, **drop-oldest**).
   - On connect: drain the buffer, then stream each record as
     `ClientMessage::Log` using `vmux_service::write_message_blocking!`.
   - On any write/connect error: mark disconnected and retry.
4. Return a boxed custom `tracing_subscriber::Layer` whose `on_event`:
   - Filters by level threshold.
   - Extracts `target()`, captures the event's `message` field (and appends
     other `key=value` fields) via a small `field::Visit` impl.
   - Builds `LogRecord { ts_ms, level, target, message }` and `try_send`s it
     (full channel → drop; counted, not fatal).

Bevy's default fmt layer is **kept** (additive `custom_layer`), so `cargo run`
terminal output is unchanged and dev workflow is unaffected.

This opens a **second** daemon connection dedicated to logs, independent of the
Bevy `ServiceHandle` terminal connection. The server already supports multiple
clients; the `Log` arm touches no process state.

### Daemon (`crates/vmux_service/src/server.rs`)

New arm in `handle_client`:

```rust
ClientMessage::Log { ts_ms, level, target, message } => {
    match level {
        1 => tracing::error!(source = "desktop", ts_ms, target = %target, "{message}"),
        2 => tracing::warn!( source = "desktop", ts_ms, target = %target, "{message}"),
        3 => tracing::info!( source = "desktop", ts_ms, target = %target, "{message}"),
        4 => tracing::debug!(source = "desktop", ts_ms, target = %target, "{message}"),
        _ => tracing::trace!(source = "desktop", ts_ms, target = %target, "{message}"),
    }
}
```

Re-emitting reuses the daemon's existing fmt subscriber, non-blocking appender,
daily rotation, and retention. The line carries the daemon's receive timestamp
in its prefix; the original desktop time is preserved as the `ts_ms` field.

Result: a single `vmux-{profile}.log` interleaving daemon and desktop lines,
one writer, no rotation race.

## C2 — Panic capture (crash-safe)

### Path helper (`crates/vmux_service/src/paths.rs`)

Add, mirroring `log_path()`:

```rust
/// Per-profile desktop crash log: vmux-{profile}-crash.log
pub fn crash_log_path() -> PathBuf { /* service_dir().join("vmux-{profile}-crash.log") */ }
```

### Hook (`crates/vmux_desktop/src/panic_hook.rs`, new)

```rust
pub fn install();
```

- Chain the previous hook: `let prev = std::panic::take_hook();` then
  `std::panic::set_hook(Box::new(move |info| { write_crash(info); prev(info); }))`
  so stderr / default abort behavior is preserved.
- Build a record string from: timestamp, thread name, panic payload
  (`downcast_ref::<&str>` / `String`), `info.location()`, and
  `std::backtrace::Backtrace::force_capture()`.
- **Guaranteed:** append the record to `vmux_service::crash_log_path()` with
  `OpenOptions::new().create(true).append(true)` and a plain blocking write (the
  process is still alive inside the hook; no tracing, minimal allocation).
- **Best-effort:** `tracing::error!(target: "vmux::panic", ...)` so the panic
  also flows through C1 into the unified file when the daemon is up.
- Factor the formatting into a pure `format_crash_record(...) -> String` for
  unit testing.

Call `panic_hook::install()` as the **first** statement in
`crates/vmux_desktop/src/main.rs::main()` (before the banner `println!`), so the
earliest panics are captured.

## C3 — CEF Crashpad (native/Chromium crashes)

CEF enables Crashpad when a `crash_reporter.cfg` is present. On macOS it is read
from `<App>.app/Contents/Resources/crash_reporter.cfg`; the existing subprocess
(Helper) app is used as the handler (no `ExternalHandler` needed).
`disable_signal_handlers: true` (macOS) is fine — Crashpad uses Mach exception
ports, not POSIX signals.

**Dump location:** the `cef` 148 binding's `Settings` has **no `user_data_path`
field** (only `cache_path` / `root_cache_path`). CEF stores the Crashpad
database under the configured `root_cache_path`, which vmux already sets to the
profile dir via `CefPlugin.root_cache_path` (`cef_cache_path()` →
`~/Library/Application Support/Vmux/profiles/personal`). So **no Settings/plugin
wiring is needed** — shipping the cfg is sufficient, and dumps land under the
profile dir. (Relocating them next to the logs would require changing
`root_cache_path`, which also holds the browser cache — out of scope.)

### Config file (`packaging/macos/crash_reporter.cfg`, new)

```ini
# vmux CEF crash reporter config
[Config]
ProductName=
ProductVersion=
ServerURL=
RateLimitEnabled=false
MaxUploads=0
BrowserCrashForwardingEnabled=false
[CrashKeys]
```

- Empty `ProductName`/`ProductVersion` → pulled from `Info.plist`
  (`CFBundleName` / `CFBundleShortVersionString`).
- Empty `ServerURL` → reports stored **locally only** (no upload).
- `BrowserCrashForwardingEnabled=false` → no macOS crash dialog; dumps are kept
  under the `root_cache_path` profile dir. (Trade-off: set `true` to also surface
  native crashes in `~/Library/Logs/DiagnosticReports` with the standard system
  dialog. Chosen `false` for a quiet, self-contained location.)

### Bundle placement (`scripts/inject-cef.sh`)

`inject-cef.sh` already edits the built `.app` on the dmg pass. Add: copy
`packaging/macos/crash_reporter.cfg` → `$APP_BUNDLE/Contents/Resources/`. Must
run **before** the framework re-sign step in the existing pipeline.

No code/plugin wiring is required for the dump location (see "Dump location"
above): dumps land under the existing `root_cache_path`.

## Testing

### C1
- `ClientMessage::Log` rkyv round-trip (in `protocol.rs` tests).
- `level_to_u8` / `u8 → level` mapping is total and round-trips 1..=5.
- Field visitor extracts the `message` field (+ appends extra fields) — unit
  test the visitor on a synthetic event record.
- Bounded-buffer behavior: extract `enqueue_drop_oldest(&mut VecDeque, cap, rec)`
  and test it drops the oldest at capacity.

### C2
- `format_crash_record(...)` contains the panic message and `file:line`.
- `crash_log_path()` ends with `vmux-{profile}-crash.log` and shares the service
  dir (extend the existing `pid_log_identity_paths_share_profile_suffix` style
  test).

### C3
- `scripts/test-bundle-layout.sh`: assert
  `Contents/Resources/crash_reporter.cfg` exists in the built bundle.
- Source test in patched `bevy_cef` (like the existing
  `cef_global_background_is_transparent_for_windowed_glass`): assert
  `message_loop.rs` sets `user_data_path`.
- Manual: load `chrome://crash` in a webview; confirm a minidump appears under
  `services/crashpad/`. Trigger a Rust panic; confirm `vmux-{profile}-crash.log`
  is written and (daemon up) the panic also appears in `vmux-{profile}.log`.

## Risks / notes

- **Patched CEF crates change** (`bevy_cef`, and the `Settings` site). Per
  `AGENTS.md`, run the appropriate package checks for the patched crate.
- **Log volume:** default `INFO` forward threshold bounds IPC traffic;
  per-frame `DEBUG`/`TRACE` stay local on stdout only.
- **Daemon-down at crash time:** C1 may miss the last lines, but C2's direct
  crash file (and C3's minidump) are the guaranteed artifacts — this is the
  whole point of the Electron-style split.
- **Ordering/timestamps:** forwarded lines show the daemon's receive time in the
  fmt prefix; the desktop event time is preserved in the `ts_ms` field.
- **Second socket connection** from the desktop (logs) is intentional and
  independent of the Bevy terminal `ServiceHandle`.

## Out of scope

- In-app "reveal logs" / "export report" UI (scope = persist only).
- Uploading crash reports to a server (`ServerURL` empty; local-only).
- Linux crash reporting specifics (primary target is macOS).
