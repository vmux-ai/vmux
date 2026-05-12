# Terminal launch context as ECS state

**Date:** 2026-05-12
**Status:** Draft

## Summary

Reshape how terminal entities relate to service-side PTYs so that:

1. **`ProcessCreated` matches the requesting entity exactly**, by id, instead of the current FIFO-across-`AwaitingProcessCreated` matching that scrambles entity-to-process bindings under concurrent spawns.
2. **Service-restart recovery preserves launch context**, so a vibe pane whose service-side PTY went missing comes back as vibe (resumed if a SessionId is known), not as a bare nu shell.
3. **`vmux://vibe/` and `vmux://terminal/` URLs transition to `vmux://vibe/<session_id>` and `vmux://terminal/<pid>`** as a natural consequence of (1) and (2) — the existing `format_vibe_url` / `format_terminal_url` systems already do the right thing once `SessionId` and `Pid` reach the right entity.
4. **Vibe is spawned directly as the PTY's primary process** instead of through a `bash -lc '... vibe --trust; exec "${SHELL:-bash}"'` wrapper. Eliminates a class of shell-quoting bugs and the `PendingTerminalInput`-races-PTY-readiness path. On vibe exit, the entity's `TerminalLaunch.kind` flips to `Plain` and the user's shell is re-spawned in the same pane (preserving today's "ctrl+d twice in vibe drops you in a shell" UX).

The unifying primitive is a new `TerminalLaunch` component on every PTY-backed entity that records what should run there (executable + args + env + cwd). It survives across restart, persists into `space.ron`, and is the single source of truth consulted by every spawn/restart/respawn code path.

## Motivation

Three bugs surfaced together while testing the OSC-title work on this branch:

- **Empty-pane symptom.** A restored vibe pane comes back blank. Service log shows the desktop sent `CreateProcess shell=/opt/homebrew/bin/nu` but no `vibe --trust` launch command was ever piped in. The bash launch wrapper that the persistence path constructs (`crates/vmux_desktop/src/persistence.rs:312`) gets queued as `PendingTerminalInput`, but if the entity is the recipient of the wrong `ProcessCreated` (see next bug) the input is sent to the wrong PTY.
- **Race in `ProcessCreated` matching.** `terminal.rs:752` picks "any entity in `AwaitingProcessCreated`, in arbitrary order". Three concurrent persistence-path spawns produce three `CreateProcess` requests, three `ProcessCreated` replies, and three entity matches — but the entity-to-process_id assignment can scramble. We observed `awaiting_create.len=0` orphans in diagnostic logs, plus a `process not found` error indicating one entity tried to attach to a stale id.
- **Missing-process restart drops everything.** When the service has no record of a process_id (because it was restarted out from under the desktop), `missing_terminal_restart` (`terminal.rs:568`) issues a fresh `CreateProcess { shell: terminal_shell(settings), cwd: String::new(), env: Vec::new(), cols: 80, rows: 24 }` with no `PendingTerminalInput`. The original cwd is lost; for vibe panes, vibe never re-runs because the launch wrapper isn't piped in. The pane comes back as a bare nu in the user's home directory.
- **Bash-wrapper launch is fragile.** Vibe is launched via `bash -lc 'cd "$1" && VIBE_MCP_SERVERS="$2" "$3" --trust; exec "${SHELL:-bash}"' bash <cwd> <mcp_json> <vibe_path>` (see `crates/vmux_desktop/src/vibe.rs:240`). The wrapper exists because `ClientMessage::CreateProcess` only accepts a single shell binary — there's no way to pass args or per-process env vars to the PTY's primary process. So we spawn the user's shell, then pipe a complete bash invocation through `PendingTerminalInput`, and rely on `bash -lc` to set env, cd, run vibe, and fall back to a shell on exit. This wedges several concerns into a shell-escape-sensitive string and forces a "send bytes after PTY is ready" race.

The shared cause: the desktop has no per-entity record of what it asked the service for, and the wire protocol's "you can spawn a shell" abstraction is too narrow for "you can spawn an arbitrary command with args and env". The desktop generates a placeholder `ProcessId` at entity-creation time, sends `CreateProcess` without including it, and the service generates its OWN id and returns that. The desktop then hopes the matching loop pairs reply to entity correctly. Restart paths can't reproduce the original launch because nothing on the entity remembers the launch. Vibe-specific launch logic is wedged into a shell-escaping function.

`TerminalLaunch` fixes the second half (entity remembers what it wants). Reusing the desktop-generated `ProcessId` as the request id on the wire fixes the first half (service echoes it back, exact-match becomes possible). Generalizing `CreateProcess` from `shell` to `command + args + env` lets vibe spawn directly without a wrapper.

## Architecture

### Identity model

The desktop generates a `ProcessId` (UUID) when an entity is spawned, stamps it onto the entity as a `ProcessId` component (no wrapper), and **sends it as the `process_id` field of `CreateProcess`**. The service uses the supplied id verbatim — it no longer generates one of its own. `ProcessCreated` echoes the id back. Matching the response to the entity is then a direct lookup by id.

This both fixes the matching race and removes one source of confusion (the placeholder-id-becomes-real-id swap that the current code performs).

`ProcessId` itself moves from `crates/vmux_service/src/protocol.rs` to `crates/vmux_core` (its description is "Shared component types for vmux", which is exactly its role here). It gains a `bevy_ecs::component::Component` derive and is used as a Component directly. The existing `ServiceProcessHandle { process_id: ProcessId }` newtype wrapper in `vmux_desktop` is deleted.

### Components

```rust
// New file: crates/vmux_desktop/src/terminal/launch.rs

#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct TerminalLaunch {
    pub command: String,                    // executable path, e.g. "/opt/homebrew/bin/nu" or "/Users/foo/.local/bin/vibe"
    pub args: Vec<String>,                  // e.g. ["--trust"] or ["--trust", "--resume", "<sid>"]
    pub cwd: String,                        // working directory; empty = unspecified
    pub env: Vec<(String, String)>,         // per-process env vars (TERM/COLORTERM/LANG/LC_CTYPE are added by service)
    pub kind: TerminalKind,
}

#[derive(Debug, Clone, Reflect, PartialEq, Eq)]
pub enum TerminalKind {
    Plain,
    Vibe,
}
```

`TerminalLaunch` describes a single executable invocation, not a shell session. For plain terminals: `command` is the user shell path, `args` is empty, `env` is empty. For vibe: `command` is the resolved vibe binary path, `args` is `["--trust"]` (or `["--trust", "--resume", "<sid>"]` when resuming), `env` carries `[("VIBE_MCP_SERVERS", json)]`.

Existing markers and components keep their roles unchanged:

| Component | Role |
|---|---|
| `Terminal` | marker — entity backed by a PTY |
| `Vibe` | marker — entity backed by vibe specifically (composes with `Terminal`) |
| `ProcessId([u8; 16])` | stable id for this entity's PTY across desktop ↔ service. **Moved from `vmux_service::protocol` to `vmux_core`** and gains `bevy_ecs::component::Component` derive. Used directly as a Component, no `ServiceProcessHandle` wrapper. |
| `Pid(u32)` | OS pid; stamped after `ProcessCreated` |
| `SessionId(String)` | vibe session id; stamped by discovery or persistence-restore |
| `PendingServiceCreate` | unit marker — needs `CreateProcess` sent |
| `AwaitingProcessCreated` | unit marker — `CreateProcess` sent, waiting for echo |
| `PendingTerminalInput { data }` | bytes to pipe to the PTY once attached |
| `PendingVibeSession { spawn_time, cwd, attempts }` | needs vibe-session discovery to find SessionId |

`PendingServiceCreate` and `AwaitingProcessCreated` become **field-less unit markers** (today they each carry duplicated state). All the launch info lives on `TerminalLaunch`; the request id is on the entity's `ProcessId` component.

`vmux_service`'s `Cargo.toml` gains `vmux_core = { path = "../vmux_core" }` as a dep and re-exports `ProcessId` from `vmux_core` for its protocol code (so the wire types in `protocol.rs` keep their existing `ProcessId` references with no source-level churn). All other consumers (`vmux_desktop`, `vmux_mcp`, `vmux_cli`) already use `ProcessId` via `vmux_service::protocol::ProcessId`; the import path remains valid.

### Wire protocol change

```rust
// crates/vmux_service/src/protocol.rs

ClientMessage::CreateProcess {
    process_id: ProcessId,            // NEW: client-assigned, service uses verbatim
    command: String,                  // RENAMED from `shell`; just the executable path now
    args: Vec<String>,                // NEW: argv tail, passed to the spawned program
    cwd: String,
    env: Vec<(String, String)>,
    cols: u16,
    rows: u16,
}

// ServiceMessage::ProcessCreated unchanged in shape; semantics: process_id echoes the request.
ServiceMessage::ProcessCreated { process_id: ProcessId, pid: u32 }
```

Three field-level changes on `CreateProcess`: `process_id` added, `shell` renamed to `command`, `args` added. `ProcessCreated`'s shape is unchanged — only its semantics shift. Service crate, desktop crate, and CLI are all rebuilt together by `make` (Makefile already enforces this).

Service-side plumbing: `vmux_service::server` reads `CreateProcess.{process_id, command, args, env, cwd, ...}` and passes them to `ProcessManager::create_process` (`crates/vmux_service/src/process.rs:1301`), which forwards them to `Process::new_with_wake` (line 204). Both function signatures change accordingly:

- Add `ProcessId` parameter to both; `Process::new_with_wake`'s internal `let id = ProcessId::new();` (line 212) is removed.
- Rename `shell: String` parameter to `command: String`.
- Add `args: Vec<String>` parameter; pass via `for a in &args { cmd.arg(a); }` after `CommandBuilder::new(&command)` (line 223).

`portable_pty::CommandBuilder` already supports `.arg()` and `.env()` and `.cwd()`, so no PTY-layer change is needed.

### Systems

| System (current name) | Change |
|---|---|
| `poll_service_messages` — pending-create slice | Read `(&ProcessId, &TerminalLaunch)` from each `PendingServiceCreate` entity. Send `CreateProcess { process_id: *pid, command: launch.command.clone(), args: launch.args.clone(), cwd: launch.cwd.clone(), env: launch.env.clone(), cols: 80, rows: 24 }`. Replace `PendingServiceCreate` with `AwaitingProcessCreated`. |
| `poll_service_messages` — `ServiceMessage::ProcessCreated` arm | For each reply, find the unique entity with both `AwaitingProcessCreated` and `ProcessId == reply.process_id`. Stamp `Pid`, drop `AwaitingProcessCreated`, send `AttachProcess`. Drop ambient `matched_entities` Vec — exact id match removes the need for it. Log a warning and drop the reply if no entity matches (real orphan). |
| `restart_missing_terminal` (rewrite of `missing_terminal_restart`) | Look up entity by stale `ProcessId`. Read its `TerminalLaunch`. Generate a fresh `ProcessId`, replace the `ProcessId` component, re-stamp `PendingServiceCreate`. If `kind == Vibe`: re-stamp `PendingVibeSession` (so discovery re-runs for the new session). No `PendingTerminalInput` is needed — the vibe binary is the PTY's primary process and `TerminalLaunch.command/args/env` already encode the resume vs. fresh case. |
| `respawn_shell_on_vibe_exit` (NEW) | On `ServiceMessage::ProcessExited` for an entity where `TerminalLaunch.kind == Vibe`: rewrite the entity's `TerminalLaunch` in place — `kind = Plain`, `command = user_shell_path()`, `args = vec![]`, `env = vec![]`, keep `cwd`. Drop `Vibe`, drop `SessionId` (if present), drop any `PendingVibeSession`. Generate fresh `ProcessId`, replace the `ProcessId` component, stamp `PendingServiceCreate`. The next `poll_service_messages` tick spawns the user shell in the same pane. |
| `format_terminal_url`, `format_vibe_url` | Unchanged. Existing `Changed<Pid>` / `Changed<SessionId>` filters fire when those components are correctly stamped on the right entity. (`format_vibe_url` correctly stops applying once `Vibe` is removed by `respawn_shell_on_vibe_exit`; `format_terminal_url`'s `Without<Vibe>` filter then takes over.) |
| `discover_pending_vibe_sessions_on_change` | Unchanged in logic. Drop the `[diag]` info-level instrumentation we added during debug. |
| `mark_vibe_session_dirty_on_change` | Drop `[diag]` instrumentation. |
| `discover_session_id_for` | Drop `[diag]` instrumentation. |
| `flush_pending_terminal_input` | **No longer used by the vibe spawn path.** Stays for legacy callers (e.g., MCP "send these bytes to a terminal" flows). The bash launch wrapper construction in `crates/vmux_desktop/src/vibe.rs` (`build_vibe_shell_command_fresh`, `build_vibe_shell_command_resume`, `build_bash_launch_command_resume`, `shell_quote`, `shell_quote_path`, `mcp_servers_env_value`'s shell-escape path) is **deleted**. The `--resume <sid>` decision moves into the `TerminalLaunch` builder (a small helper in `vibe.rs` that returns `(command, args, env)`). |

### Spawn-site updates

Every code path that constructs a Terminal/Vibe entity stamps `TerminalLaunch` in the bundle. The six known sites, all of which pre-resolve the executable path and assemble args/env directly (no shell wrapping):

1. `Terminal::new_with_cwd` (`crates/vmux_desktop/src/terminal.rs:391`) — `TerminalLaunch { command: <user shell path>, args: vec![], cwd, env: vec![], kind: Plain }`.
2. `spawn_vibe_pane` (`crates/vmux_desktop/src/terminal.rs:340`) — uses a new `vibe::build_terminal_launch(cwd, session_id: Option<&str>)` helper that returns `TerminalLaunch { command: <vibe path>, args: ["--trust"] or ["--trust", "--resume", sid], cwd, env: [("VIBE_MCP_SERVERS", json)], kind: Vibe }`. No `PendingTerminalInput` is inserted — the launch is fully encoded in `TerminalLaunch`.
3. `crates/vmux_desktop/src/agent.rs:171` and 240 — agent-provider spawn paths use the same `vibe::build_terminal_launch` helper.
4. Persistence restore for `vmux://terminal/` URLs (`persistence.rs:298`) — `TerminalLaunch { kind: Plain, ... }` from saved state.
5. Persistence restore for `vmux://vibe/...` URLs (`persistence.rs:303-345`) — `TerminalLaunch { kind: Vibe, ... }` from saved state, with `args` containing `--resume <sid>` if the URL had a session id.
6. `RestartPty` handler (`crates/vmux_desktop/src/terminal.rs:1840`) — uses the entity's existing `TerminalLaunch`. The handler stops mutating `meta.url` directly (line 1894) — `format_terminal_url` / `format_vibe_url` will set the URL once a fresh `Pid` arrives.

### Persistence

`PaneState` (in the persisted `space.ron` schema) gains one field:

```rust
pub struct PaneState {
    // ...existing fields
    pub launch: TerminalLaunch,
}
```

Plain `TerminalLaunch`, not `Option<TerminalLaunch>` — no backward compat with old `space.ron` files. The first save-after-this-change writes the new schema; old files that lack `launch` will fail to deserialize and the app will fall back to its empty-session bootstrap (the same fallback that fires today on a corrupt or first-run `space.ron`).

On restore:

- For each `Terminal`/`Vibe` pane, stamp `TerminalLaunch` from the saved value.
- Stamp a fresh `ProcessId` (never restore a persisted `process_id` — service was restarted; the old id is meaningless).
- For `Vibe` panes whose saved URL was `vmux://vibe/<sid>`, stamp `SessionId(sid)` directly. The launch-command builder will use it for `--resume <sid>`.

### URL transition path (Bug C) is no longer special-case work

Once (1) and (2) are in place:

- A fresh terminal pane: `Pid` lands on the right entity via id-matched `ProcessCreated` → `format_terminal_url`'s `Changed<Pid>` filter fires → URL becomes `vmux://terminal/<pid>`.
- A fresh vibe pane: vibe actually runs (because `PendingTerminalInput` carries the launch wrapper to the right PTY) → vibe writes `meta.json` → fs watcher fires → discovery finds it → `SessionId` stamped → `format_vibe_url`'s `Changed<SessionId>` filter fires → URL becomes `vmux://vibe/<sid>`.
- A restored vibe pane with saved `SessionId`: `SessionId` is stamped at restore time → `format_vibe_url`'s `Added<Vibe>` filter fires immediately → URL is correct from frame one.
- A missing-process-restart of a vibe pane: `restart_missing_terminal` re-issues `--resume <sid>` (or fresh) and re-stamps `PendingVibeSession` → discovery re-runs if needed.

If discovery still misbehaves after (1) and (2) are in place, the diagnostic infrastructure we added during this debug (uncommitted) is enough to identify the cause; address as a follow-up. The diagnostics are NOT included in this spec's deliverable — they get reverted as part of the implementation.

## Diagnostic cleanup

Revert the `[diag]` `eprintln!` / `info!` lines that were added during the debug session:

- `crates/vmux_service/src/process.rs` — none added; nothing to revert.
- `crates/vmux_service/src/server.rs` — `[diag] svc CreateProcess`, `[diag] svc spawned`, `[diag] svc CreateProcess failed`.
- `crates/vmux_service/src/client.rs` — `[diag] reader rx ...`, `[diag] reader: connection closed`, `[diag] reader: recv error`, `[diag] reader forward failed`.
- `crates/vmux_desktop/src/terminal.rs` — `[diag] sending CreateProcess`, `[diag] received ProcessCreated`, `[diag] matched ProcessCreated`, `[diag] flushing PendingTerminalInput`.
- `crates/vmux_desktop/src/vibe/session.rs` — `[diag] vibe watcher fired`, `[diag] discovery sweep`, `[diag] discovery scan`.

This is bookkeeping; the new system has its own logging at `warn!` for the genuinely surprising cases (orphan `ProcessCreated`, missing-process restart firing more than N times for the same entity, etc.).

## Edge cases & non-goals

- **Two desktops sharing one service.** UUIDv4 collisions are astronomically unlikely; we don't add an explicit collision-detection mechanism. If two desktops happen to spawn entities with the same `ProcessId`, behavior is undefined — same as today.
- **A `ProcessCreated` reply with no matching entity.** Log `warn!`, drop. Could happen if the entity was despawned mid-flight.
- **`restart_missing_terminal` racing with another restart attempt for the same entity.** Today's `restarted_missing_processes: Vec<ProcessId>` deduplication still applies, keyed by the *stale* process_id. After restart the entity has a new `ProcessId`, so a future "process not found" for the same entity would be for a different id and not deduplicated — fine.
- **`cwd` semantics in production.** The current `cwd: String` is whatever the spawn-site passed. We're not redesigning cwd semantics here (e.g., what `current_dir()` means for a packaged macOS app). `TerminalLaunch.cwd` faithfully stores whatever was passed; downstream cwd resolution is deferred to a future spec.
- **Old `space.ron` files.** No migration. First-run-after-upgrade falls back to bootstrap (same path as a corrupt file today).
- **Diagnostic logs are reverted as part of implementation, not preserved.** If the `format_vibe_url` chain misbehaves after this lands, we can re-add targeted instrumentation under a feature flag in a follow-up.
- **Vibe binary not found at spawn time.** `vibe::build_terminal_launch` resolves the vibe path via the existing `crate::vibe::find_executable("vibe")`. If lookup fails, the spawn site logs a warning and falls back to a `TerminalLaunch { kind: Plain, command: <user shell>, ... }` — same UX as today's "vibe not installed → bare shell" outcome.
- **Vibe exits with non-zero status (crash).** `respawn_shell_on_vibe_exit` ignores the exit code — any vibe exit triggers shell respawn, matching today's `; exec "${SHELL:-bash}"` behavior in the wrapper.
- **User shell not found.** `respawn_shell_on_vibe_exit` calls `default_shell()` (the existing `crates/vmux_desktop/src/terminal.rs:551` helper), which falls back to `/bin/zsh`. Matches today.
- **`PendingTerminalInput` flow elsewhere.** This system is preserved for non-spawn use cases (e.g., MCP "send these bytes to a terminal", restart-PTY paths that genuinely need to type something). The vibe spawn path stops using it.

Out of scope:

- Zombie service-process detection (`kill(pid, 0)` returning success for `<defunct>`). Tracked separately.
- A redesign of `cwd` semantics for the macOS-packaged app.
- Settings.ron-driven per-process favicon/bg map (the future of the OSC-title chain).
- Tab chips in the header.
- Per-shell startup files / login shell semantics. Plain terminals spawn the user shell as a non-login non-interactive process from the PTY; if a user wants a login shell they can adjust their shell config (same as iTerm2 default).

## Testing

- **Unit test, `vmux_service::protocol`** — round-trip `ClientMessage::CreateProcess { process_id, command, args, cwd, env, ... }` with rkyv to confirm the new shape serializes.
- **Unit test, `vmux_service::process`** — call `Process::new_with_wake` with a caller-supplied `ProcessId` and a non-shell `command + args` (e.g., `command = "/bin/echo", args = vec!["hello"]`); verify `Process.id == that id` and the spawned PTY actually runs `/bin/echo hello` (drain output for a few ms and check). (`Process::new_with_wake` currently generates its own `ProcessId::new()` at `crates/vmux_service/src/process.rs:212`; the change makes it accept the id as a parameter so the wire-supplied `CreateProcess.process_id` flows through `ProcessManager::create_process` into `Process`.)
- **Unit test, `vmux_desktop::terminal`** — three entities each with `PendingServiceCreate`, distinct `ProcessId` components, distinct `TerminalLaunch.command`s. Run `poll_service_messages` once, assert three distinct `CreateProcess` were queued, each with the expected `process_id` matching its entity's `ProcessId`. Then deliver three `ProcessCreated` replies in scrambled order; assert each `Pid` lands on the entity whose `ProcessId` matches the reply.
- **Unit test, `vmux_desktop::vibe::build_terminal_launch`** — given `cwd = "/tmp/x"` and `session_id = None`, expect `args = ["--trust"]` and `env` contains `VIBE_MCP_SERVERS`. Given `session_id = Some("abc")`, expect `args = ["--trust", "--resume", "abc"]`.
- **Unit test, `vmux_desktop::terminal::respawn_shell_on_vibe_exit`** — entity with `Vibe`, `SessionId("abc")`, `TerminalLaunch { kind: Vibe, ... }`. Deliver `ProcessExited`. Assert `Vibe` removed, `SessionId` removed, `TerminalLaunch.kind == Plain`, `TerminalLaunch.command == default_shell()`, `PendingServiceCreate` re-stamped, `ProcessId` changed (new UUID).
- **Unit test, `vmux_desktop::terminal::pid::format_terminal_url`** — already exists; confirm still passes.
- **Unit test, `vmux_desktop::vibe::session::format_vibe_url`** — already exists; confirm still passes.
- **Unit test, persistence restore** — given a `PaneState` with `launch: TerminalLaunch { command: "/path/to/vibe", args: vec!["--trust", "--resume", "<sid>"], cwd: "...", env: vec![...], kind: Vibe }` and URL `vmux://vibe/<sid>`, confirm the restored entity has `TerminalLaunch`, `SessionId(sid)`, and a fresh `ProcessId` (not whatever was saved).
- **Integration check, manual** — open a vibe pane, confirm address bar transitions `vmux://vibe/` → `vmux://vibe/<sid>` within a few seconds. Open a terminal pane, confirm `vmux://terminal/` → `vmux://terminal/<pid>`. Quit vmux, restart, confirm both panes come back with their original cwd; the vibe pane resumes the same session id (same `<sid>` in URL).
- **Integration check, manual — vibe exit drops to shell** — in a vibe pane, ctrl+d twice. Confirm vibe exits and the same pane shows the user shell prompt with the same cwd. Address bar transitions `vmux://vibe/<sid>` → `vmux://terminal/<new_pid>`.
- **Integration check, manual — service restart mid-session** — kill the service process while a vibe pane is open, observe the desktop fire `restart_missing_terminal`, confirm vibe re-launches via `--resume <sid>` in the same cwd.

## Migration / rollout

- Single workspace; service + desktop + CLI all rebuilt by `make`. No staged rollout.
- `space.ron` schema changes; no backward-compat. Users with an existing session may need to re-create a tab or two on first launch after this lands.
- No feature flag; the change is structural and fully replaces the old code paths.
- The shell-wrapper builders in `crates/vmux_desktop/src/vibe.rs` are deleted: `build_vibe_shell_command_fresh`, `build_vibe_shell_command_resume`, `build_bash_launch_command`, `build_bash_launch_command_resume`, `shell_quote`, `shell_quote_path`. They are replaced by a single `build_terminal_launch(cwd: &Path, session_id: Option<&str>) -> Result<TerminalLaunch, String>` helper that resolves the vibe binary path, assembles `args`, and serializes `VIBE_MCP_SERVERS` into the `env` vec without any shell escaping.
- `mcp_servers_env_value` stays — it produces the JSON payload for `VIBE_MCP_SERVERS`. The change is just where the JSON ends up (env var directly, instead of inside a single-quoted shell string).
