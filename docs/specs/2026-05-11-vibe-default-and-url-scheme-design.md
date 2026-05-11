# Vibe-default new pane + URL scheme realignment

**Date:** 2026-05-11
**Status:** Draft

## Summary

Three coordinated changes to vmux:

1. When the `vibe` CLI is detected on disk, opening a new stack/tab/pane/space launches `vibe --trust` directly instead of opening the command bar.
2. Add a new URL route `vmux://vibe/<session_id>` that identifies a vibe pane by its vibe CLI session UUID.
3. Change the existing terminal URL form from `vmux://terminal/<process-id-uuid>` to `vmux://terminal/<os-pid>`.

The command bar remains reachable via its existing keybinding regardless of vibe availability.

## Motivation

`vibe` is the primary day-to-day surface for users who have it installed; making them dismiss the command bar to launch it is friction. Exposing OS PIDs and vibe session IDs in URLs makes panes addressable by the same identifiers users see in `ps` / `ls ~/.vibe/logs/session/`, which is what users (and external tooling like the MCP server) reach for when they want to point at a specific pane.

## Architecture

### Identity model

- **`ProcessId` (UUID, internal)** — unchanged. Stays as the entity-level handle across the desktop ↔ service protocol. All component lookups, `ServiceProcessHandle`, and protocol messages keep using `ProcessId`.
- **OS PID (`u32`, URL-only)** — exposed in URLs and via a reverse-lookup map. Stamped on the entity as a `ProcessPid` component once the service responds with `ProcessCreated`.
- **Vibe `session_id` (UUID string, URL-only)** — exposed in URLs and via a reverse-lookup map. Stamped on the entity as a `VibeSessionId` component once discovery succeeds.

The internal UUID is kept because (a) it exists at entity-creation time before the service has spawned anything, (b) it is stable across PTY respawn (preserves history/webview handle), and (c) it sidesteps PID-reuse correctness bugs at the protocol layer.

### Pane kind

Every terminal-style entity carries one of two marker components:

- `TerminalPane` — regular shell (existing behavior).
- `VibePane` — pane launched as `vibe --trust [...]`.

The URL formatter system reads the marker to decide which scheme to emit.

### Default-pane intent

A new resource is set once at startup based on `vibe::vibe_available()`:

```rust
enum DefaultPaneKind {
    Vibe,        // vibe binary found on PATH or in fallback dirs
    CommandBar,  // current behavior
}
```

The new-stack handler in `vmux_layout::stack` consults this resource. If `Vibe`, it bundles a vibe pane directly and skips `NewStackContext.needs_open` (which would otherwise open the command bar modal). The same check applies anywhere a new empty pane is auto-created (currently `open_command_bar_if_no_stacks` in `stack.rs`).

The check is evaluated at app startup only. Toggling vibe availability mid-session (e.g., installing/removing the binary) does not take effect until restart. This is acceptable because `vibe_available()` requires a filesystem scan that is too expensive to run per pane.

## Components

### Service protocol change

**File:** `crates/vmux_service/src/protocol.rs`

```rust
// Before
ProcessCreated { process_id: ProcessId },

// After
ProcessCreated { process_id: ProcessId, pid: u32 },
```

The service already tracks `Process::pid: u32`. Propagating it into the existing message is a one-field addition with no other protocol-shape changes.

If `child.process_id()` returns `None` at spawn time, the service emits `ProcessCreateFailed { reason }` instead of falling back to PID `0`. The `unwrap_or(0)` at `process.rs:229` becomes a hard error path. Defaulting to `0` would make `vmux://terminal/0` a possible URL, which would alias every spawn-failure pane.

### Desktop components & resources

**New file:** `crates/vmux_desktop/src/terminal/pid.rs` (filename-based module per project rules — no `mod.rs`).

```rust
#[derive(Component)]
pub struct ProcessPid(pub u32);

#[derive(Component)]
pub struct TerminalPane;

#[derive(Resource, Default)]
pub struct PidToEntity(pub HashMap<u32, Entity>);
```

**New file:** `crates/vmux_desktop/src/vibe/session.rs`.

```rust
#[derive(Component)]
pub struct VibePane;

#[derive(Component)]
pub struct VibeSessionId(pub String);

#[derive(Component)]
pub struct PendingVibeSession {
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
    pub attempts: u8,
}

#[derive(Resource, Default)]
pub struct VibeSessionToEntity(pub HashMap<String, Entity>);

#[derive(Resource)]
pub enum DefaultPaneKind {
    Vibe,
    CommandBar,
}
```

### URL formatter system

Replaces the static `format!` in `terminal.rs:317`. A Bevy system runs each frame over entities whose URL inputs changed:

- Entity has `TerminalPane`:
  - With `ProcessPid(pid)` → `PageMetadata.url = format!("vmux://terminal/{pid}")`.
  - Without → `PageMetadata.url = "vmux://terminal/"` (placeholder during spawn race).
- Entity has `VibePane`:
  - With `VibeSessionId(id)` → `PageMetadata.url = format!("vmux://vibe/{id}")`.
  - Without → `PageMetadata.url = "vmux://vibe/"` (placeholder during discovery).

Reverse-lookup maps stay in sync via `RemovedComponents<ProcessPid>` and `RemovedComponents<VibeSessionId>` cleanup systems running before any URL-resolution system in the same frame.

### Vibe session discovery

**File:** `crates/vmux_desktop/src/vibe/session.rs`.

A polling system runs every 200ms over entities with `PendingVibeSession`:

1. Read `~/.vibe/logs/session/` directory listing.
2. Filter to subdirs whose `meta.json` parses with:
   - `working_directory == entity.cwd` (string equality on canonical paths).
   - `start_time >= entity.spawn_time` (parse ISO-8601, compare as `SystemTime`).
   - `session_id` is **not** already present in `VibeSessionToEntity` (excludes sessions already claimed by other panes).
3. If multiple match, pick the earliest `start_time` (closest to this pane's spawn — more likely to be ours than a later concurrent spawn).
4. On match: read `session_id` field from `meta.json`, stamp `VibeSessionId` component, remove `PendingVibeSession`, insert into `VibeSessionToEntity`.
5. On no match: increment `attempts`. If `attempts >= 30` (~6s), log warning and remove `PendingVibeSession`. Pane stays functional with placeholder URL.

`meta.json` is written by vibe at session start (verified by inspecting an existing session: the file contains `session_id`, `start_time`, `environment.working_directory` from the moment vibe boots). End-of-session fields like `end_time` are added later but the start-time fields are present immediately.

### Vibe launch helpers

**File:** `crates/vmux_desktop/src/vibe.rs` — extend the existing launch builder at lines 203–211.

Two callers:

- **Fresh session** (default new pane, or `vmux://vibe/` no-path): `vibe --trust`. Discovery runs.
- **Resume** (`vmux://vibe/<id>` for unknown session): `vibe --trust --resume <id>`. Discovery skipped — session ID is already known and stamped on the entity at spawn time.

Both share the existing `bash -lc 'cd "$1" && VIBE_MCP_SERVERS="$2" exec "$3" --trust [...]' bash <cwd> <mcp_json> <vibe_path>` template.

### URL parser & dispatcher

**File:** `crates/vmux_desktop/src/agent.rs` — extend `spawn_vmux_tab` (lines 266–308).

Add `vibe` host arm. Updated routing table:

| Host | Path | Action |
|------|------|--------|
| `terminal` | empty | Spawn new terminal (existing) |
| `terminal` | `<u32>` | Look up `PidToEntity`. Hit → focus pane. Miss → log error, open command bar. |
| `vibe` | empty | Spawn new fresh vibe pane (same as default-pane flow) |
| `vibe` | `<session-id>` | Look up `VibeSessionToEntity`. Hit → focus pane. Miss → spawn new pane with `--resume <id>`, stamp `VibeSessionId(id)` immediately. |
| `sessions`, `services` | — | unchanged |

**File:** `crates/vmux_desktop/src/command_bar.rs` — `parse_process_id_from_url` at lines 42–45 becomes `parse_pid_from_url` returning `Option<u32>`. The `on_command_bar_action` handler's terminal arm uses `PidToEntity` for reattach lookups.

UUID-form URLs (`vmux://terminal/<uuid>`) no longer parse — hard cut.

## Data flow

### New vibe pane (vibe available)

```
User: Cmd+T
  → StackCommand::New
  → handler reads DefaultPaneKind::Vibe
  → bundle entity with:
       PendingServiceCreate { shell: vibe-cmd, cwd }
       VibePane
       PendingVibeSession { spawn_time: now(), cwd, attempts: 0 }
       PageMetadata.url = "vmux://vibe/" (placeholder)
  → skip NewStackContext.needs_open
  → poll_service_messages sends CreateProcess to service
  → service spawns PTY, returns ProcessCreated { process_id, pid }
  → desktop adds ServiceProcessHandle(process_id) + ProcessPid(pid)
  → discovery system polls ~/.vibe/logs/session/ each 200ms
  → on match: stamp VibeSessionId(id), remove PendingVibeSession,
     insert into VibeSessionToEntity
  → URL formatter updates PageMetadata.url to "vmux://vibe/<id>"
```

### URL navigation

```
vmux://terminal/12345
  → parse 12345 as u32
  → PidToEntity.get(&12345)
  → Some(entity): focus that pane's tab
  → None: log "no pane for pid 12345", open command bar

vmux://terminal/
  → spawn new terminal (existing flow)

vmux://vibe/ae724a54-c387-...
  → VibeSessionToEntity.get(&id)
  → Some(entity): focus that pane's tab
  → None: spawn new entity with vibe-cmd containing --resume <id>,
     mark VibePane, stamp VibeSessionId(id) at spawn time, skip discovery,
     insert into VibeSessionToEntity

vmux://vibe/
  → spawn fresh vibe (same path as default-pane flow)
```

## Error handling & edge cases

- **PID 0 / spawn failure** — service emits `ProcessCreateFailed`; entity gets a failure marker, URL stays empty. No `vmux://terminal/0` ever produced.
- **PID reuse** — `RemovedComponents<ProcessPid>` cleanup system runs before URL-nav resolution each frame. Worst case: same-frame reuse falls into the new entity, which is the correct behavior (the OS has decided that PID 12345 is now this new process).
- **Discovery race** — two vibe panes in the same cwd spawned within the same poll window. The "not already in `VibeSessionToEntity`" filter ensures each session_id is claimed by exactly one entity. The "earliest `start_time` ≥ spawn_time" tiebreak means the first entity polls and grabs the earlier-created dir; the second entity sees that dir already claimed and falls through to the later dir. Each entity ends up with its own session.
- **Discovery timeout** — pane stays usable, URL placeholder. Log warning. User can re-trigger by doing `Cmd+L` or whatever surfaces "copy URL" if they want a usable handle later.
- **Vibe binary disappears mid-session** — same handling as a regular shell exit. Out of scope.
- **Old `vmux://terminal/<uuid>` references** (e.g., MCP commands stored in vibe history) — fail to parse as `u32`, return error, log. Acceptable: pre-1.0, no public API contract.
- **`vibe_available()` flips at runtime** — not detected. Resolved on next vmux restart. Documented in user-facing changelog.
- **Multiple spaces** — `PidToEntity` and `VibeSessionToEntity` are global resources, not space-scoped. URL navigation can target a pane in any space and the existing pane-focus machinery handles cross-space switching.

## Testing

**Unit:**
- URL formatter for both schemes including placeholder cases (`vmux://terminal/` when no PID, `vmux://vibe/` when no session).
- URL parser for `vmux://terminal/<pid>`: numeric only, leading zeros, bounds (`u32::MAX`, overflow).
- URL parser for `vmux://vibe/<session>`: UUID string round-trip.
- `PidToEntity` insert on `ProcessPid` add, removal on entity despawn.
- `VibeSessionToEntity` same lifecycle.
- Vibe session discovery against a fixture `~/.vibe/logs/session/` directory: cwd match, start_time filter, multiple-match tiebreak.

**Integration:**
- Spawn vibe → assert URL transitions from `vmux://vibe/` to `vmux://vibe/<id>` within 6s. Test guarded with `#[cfg]` or skipped when `vibe` binary not present (matches existing pattern in vibe.rs).

**Manual (vibe-installed machine):**
- `Cmd+T` (or new-stack shortcut) launches vibe; no command bar appears.
- `Cmd+K` opens command bar.
- Open vibe pane, copy its URL, close pane, paste URL into command bar → spawns `vibe --resume <id>`.
- Spawn terminal, copy `vmux://terminal/<pid>`, switch tabs, paste URL → focuses original terminal.
- Uninstall vibe binary, restart vmux, `Cmd+T` opens command bar (fallback).

## Affected crates (lint/test scope per AGENTS.md)

- `vmux_service` — protocol message field added, spawn-failure path.
- `vmux_desktop` — terminal, vibe, agent, command_bar modules; new `terminal/pid.rs` and `vibe/session.rs` files (filename-based modules per project rules).
- `vmux_layout` — `stack.rs` consults `DefaultPaneKind` in new-stack handler and `open_command_bar_if_no_stacks`.

The lint loop in AGENTS.md will pick these up automatically based on `git diff main`.

## Out of scope

- Vibe `--continue` / interactive picker variants on default-pane spawn (locked: always fresh).
- UUID-form URL backwards compatibility (locked: hard cut).
- Replacing `ProcessId` UUID throughout the protocol (locked: keep internal; PID is URL-only).
- Per-space default-pane override (always global).
- Mid-session vibe-availability re-check (restart required).
- Dynamic URL update on PTY respawn (out of normal flow; respawn keeps same `ProcessId` so the existing flow already handles it).
