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

> **Revision (2026-06-16):** the original IPC-forwarding design (desktop →
> daemon single writer) proved fragile in this repo's multi-worktree dev setup:
> all builds/installs share one global per-profile socket
> (`~/Library/Application Support/Vmux/services/vmux-{profile}.sock`), and
> forwarded records are only persisted when the socket happens to be owned by a
> daemon built from a branch that knows `ClientMessage::Log`. When another
> worktree's daemon (or the release install) owns the socket, the forwarded
> records fail to deserialize and are silently dropped. The design below uses
> **direct file writes** instead — no IPC, no daemon dependency, no version
> skew.

Three components:

- **C1 — Unified application log.** Both the desktop and the daemon write
  **directly** to the same daily-rolled file
  `~/Library/Application Support/Vmux/logs/vmux-{profile}.{date}.log` via their
  own tracing-appender layers. `Rotation::DAILY` uses date-stamped filenames
  (not renames), so two processes appending to one file is safe — no rename
  race, just line-granular interleave.
- **C2 — Panic capture.** Desktop installs a panic hook that appends the panic
  (message + location + backtrace) **directly** to the same daily file
  (crash-safe; independent of the daemon).
- **C3 — CEF Crashpad.** Ship `crash_reporter.cfg` so CEF's bundled handler
  writes minidumps for native/Chromium crashes (dumps under `root_cache_path`;
  see C3 below).

Application logs live under `~/Library/Application Support/Vmux/logs/`; runtime
files (socket, pid, identity) stay in `…/Vmux/services/`.

```
desktop process                              daemon process
┌─────────────────────────────┐             ┌─────────────────────────────┐
│ tracing                     │             │ tracing                     │
│  ├─ default fmt → stdout    │             │  └─ rolling DAILY appender  │
│  └─ file DAILY appender ────┼───┐     ┌───┼──── (vmux_service)          │
│ panic hook ─ direct append ─┼─┐ │     │   └─────────────────────────────┘
└─────────────────────────────┘ ▼ ▼     ▼
        logs/vmux-{profile}.{date}.log   (one file, both append)
```

## C1 — Unified application log

### Daemon (`crates/vmux_service/src/service.rs`, `paths.rs`)

`init_tracing` builds a `tracing_appender::rolling` DAILY appender in
`log_dir()` (`~/Library/Application Support/Vmux/logs/`) with prefix
`vmux-{profile}`, suffix `log`, `max_log_files(7)` → writes
`vmux-{profile}.{date}.log`. `paths.rs` adds:

```rust
pub fn log_dir() -> PathBuf;          // …/Vmux/logs
pub fn current_log_file() -> PathBuf; // log_dir()/vmux-{profile}.{utc-date}.log
```

`current_log_file()` reproduces the appender's filename (UTC date, via `chrono`)
so the CLI and the panic hook can target the same file.

### Desktop (`crates/vmux_desktop/src/log_forward.rs`)

`file_log_layer(&mut App) -> Option<BoxedLayer>`, used as
`LogPlugin.custom_layer`, builds its **own** DAILY appender in the same
`log_dir()` with the same prefix and returns
`fmt::layer().with_writer(non_blocking).with_ansi(false)`. The `WorkerGuard` is
leaked to keep the writer alive. Bevy's default stdout layer is kept (additive),
so `cargo run` terminal output is unchanged.

Because `Rotation::DAILY` writes date-stamped files (it does **not** rename),
the desktop and daemon appenders both open `vmux-{profile}.{date}.log` in append
mode; concurrent appends interleave at line granularity (safe). No IPC, no
daemon dependency, no protocol version skew.

### CLI (`crates/vmux_service/src/cli.rs`)

`vmux logs [-f]` tails `current_log_file()` (previously it tailed the empty
`log_path()` stdout sink, which is why logs appeared "missing").

## C2 — Panic capture (crash-safe)

`crates/vmux_desktop/src/panic_hook.rs`; `install()` is called as the **first**
statement of `main()` so the earliest panics are captured:

- Chain the previous hook so stderr/abort behavior is preserved.
- Build a record from timestamp, thread name, panic payload
  (`downcast_ref::<&str>`/`String`), `info.location()`, and
  `Backtrace::force_capture()` — formatting factored into
  `format_crash_record(...)` for unit testing.
- Append it **directly** (blocking `OpenOptions::append`) to
  `vmux_service::current_log_file()` — the same unified file. Reliable
  regardless of daemon state. No tracing emit in the hook (avoids a duplicate
  via the file layer and the unreliable non-blocking flush at abort).

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
- `current_log_file()` lives in `log_dir()` (`…/logs`), starts with
  `vmux-{profile}.`, ends `.log` (paths.rs test).
- `file_log_layer` compiles into `LogPlugin.custom_layer` (desktop builds).
- Manual: run the app; confirm INFO lines appear in
  `logs/vmux-{profile}.{date}.log` (the same lines as stdout), alongside daemon
  lines.

### C2
- `format_crash_record(...)` contains the panic message and `file:line`.
- Manual: trigger a Rust panic; confirm the record is appended to
  `logs/vmux-{profile}.{date}.log`.

### C3
- `scripts/test-bundle-layout.sh`: assert
  `Contents/Resources/crash_reporter.cfg` exists in the built bundle.
- Manual: load `chrome://crash` in a webview; confirm a minidump appears under
  the `root_cache_path` Crashpad dir.

## Risks / notes

- **Two processes, one file:** desktop and daemon append concurrently to
  `vmux-{profile}.{date}.log`. `Rotation::DAILY` uses date-stamped names (no
  rename), so the only effect is occasional line interleave — acceptable.
- **`current_log_file()` couples to the appender's filename** (`{prefix}.{utc
  date}.{suffix}`). Both use UTC; a panic exactly at the UTC midnight boundary
  could land in the previous day's file. Negligible.
- **Stale files:** older `services/vmux-{profile}.log` (empty stdout sink) and
  dated files remain from the pre-move layout; they can be deleted.
- **CEF Crashpad dumps** land under `root_cache_path` (profile dir), not the
  `logs/` dir — CEF 148 has no setting to relocate them without moving the
  browser cache.

## Out of scope

- In-app "reveal logs" / "export report" UI (scope = persist only).
- Uploading crash reports to a server (`ServerURL` empty; local-only).
- Linux crash reporting specifics (primary target is macOS).
