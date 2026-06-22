# Services page: realtime CPU + memory per process

## Goal

On the processes monitor (`vmux://services/`), each process card shows **realtime CPU%** and **memory (RSS)**, summed over the process's tree (the spawned pid + all descendants) so wrappers (node/python launchers) are counted.

Spawn time is **already shown** there as uptime (`format_uptime(uptime_secs)`), refreshed each 1s poll. This feature only adds CPU + memory.

This covers the original ask — seeing an agent's resource usage (e.g. Vibe) — because every agent process already appears as a card on this page (it has a pid).

## Approach (decided: B — sample on the desktop)

The vmux **service** (a launchd daemon) reports each process's OS `pid` and `created_at_secs` (uptime) to the desktop via `ServiceMessage::ProcessList { Vec<ProcessInfo> }`, cached in `ServiceProcessList`. We sample CPU/mem **on the desktop** with `sysinfo`, keyed on those pids.

No change to the desktop↔service wire protocol (`ProcessInfo`) or the service binary — this avoids version-skew risk between an updated desktop and an older running daemon. The only new wire field is on the host→webview event (`ProcessEntry`), which always ships in lockstep with the desktop.

Rejected — Approach A (sample in the service, add fields to `ProcessInfo`): changes the daemon wire protocol.

## Current state

- `crates/vmux_service/src/protocol.rs`: `ProcessInfo { id, shell, cwd, cols, rows, pid: u32, created_at_secs: u64 }` (service→desktop; **unchanged** by this feature).
- `crates/vmux_terminal/src/processes_monitor.rs`:
  - `ServiceProcessList` resource (cached `Vec<ProcessInfo>`).
  - `request_process_list`: sends `ClientMessage::ListProcesses` every 1s while a `ProcessesMonitor` webview exists.
  - `broadcast_to_monitors`: maps each `ProcessInfo` → `ProcessEntry` and pushes `ProcessesListEvent` to monitor webviews (only when `process_list.is_changed()`).
- `crates/vmux_service/src/event.rs`: `ProcessEntry { id, shell, cwd, cols, rows, pid, uptime_secs, attached, preview_lines }`, `ProcessesListEvent { connected, processes }` (host→webview).
- `crates/vmux_service/src/page.rs` (`wasm32`): `ProcessCard` renders uptime (row 1) + a `MetaRow` grid (PID, Size, CWD, Shell); has `format_uptime`.

## Design

### 1. Desktop sampler (`crates/vmux_terminal/src/processes_monitor.rs`)

Add `sysinfo` (native dep) and a usage resource:

```text
Resource ProcessUsage(HashMap<u32 /*pid*/, Usage>)
Usage { cpu_percent: f32, mem_bytes: u64 }
```

System `sample_process_usage`, gated identically to the poll (monitors present) on the existing 1s `ProcessesPollTimer`:

1. Persistent `sysinfo::System` (in a resource wrapper). Refresh processes + CPU each tick — CPU% is a delta between refreshes, so the 1s cadence yields valid values (first tick reads 0%, acceptable).
2. Build a parent→children map from sysinfo. For each `pid` in `ServiceProcessList`, sum `cpu_usage()` and `memory()` over `{pid} ∪ descendants(pid)`.
3. Write `ProcessUsage`; drop pids no longer present.

Order in `Update`: `request_process_list` → `sample_process_usage` → `broadcast_to_monitors`.

Pure, unit-testable helper:
`subtree_usage(root_pid, &procs) -> Usage` over a plain `{pid -> (parent_pid, cpu, mem)}` map (no sysinfo in the test).

`sample_process_usage` must also flag the broadcast: `broadcast_to_monitors` currently only emits when `ServiceProcessList.is_changed()`. CPU/mem change every tick without the list changing, so the gate becomes "`ServiceProcessList` changed **or** `ProcessUsage` changed" so realtime values are pushed each poll.

### 2. Event field (`crates/vmux_service/src/event.rs`)

Add to `ProcessEntry` (additive; both serde + rkyv derived):

```text
cpu_percent: f32,
mem_bytes: u64,
```

### 3. Fill on broadcast (`processes_monitor.rs::broadcast_to_monitors`)

Look up each process's pid in `ProcessUsage`; set `cpu_percent`/`mem_bytes` (default `0.0`/`0` when absent, e.g. just-spawned or unsampled).

### 4. Page (`crates/vmux_service/src/page.rs`)

- `ProcessCard`: show CPU + Memory. Add to row 1 beside uptime (e.g. `12% · 332 MB · 4m 03s`) or as two `MetaRow`s in the grid — default to the grid for consistency.
- Add pure `format_mem(u64) -> String` → `"332 MB"`, `"1.2 GB"`, and `"—"` when `0`.

## Cost / behavior

- Sampling, the poll, and broadcasts run **only while the services page is open**. Nothing runs when it is closed.
- Cadence 1s. No change to winit update mode; no idle CPU.

## Cross-platform

`sysinfo` covers macOS (primary) + Linux (CI); same-user pids are readable on both; process-tree via sysinfo parent pids. Gate the dep/imports for non-wasm if `vmux_terminal` ever targets wasm.

## Testing

- `subtree_usage` — pure: fake process tree; assert summed cpu+mem over a subtree, siblings/other trees excluded.
- `format_mem` — pure unit tests (0 → "—", <1 GB → MB, ≥1 GB → GB with one decimal).
- `broadcast_to_monitors` — seed `ServiceProcessList` + `ProcessUsage` + a `ProcessesMonitor` webview; assert the emitted `ProcessEntry` carries cpu/mem.
- Broadcast gate — `ProcessUsage` changing (list unchanged) still triggers a push.

## Out of scope

- Any change to the service binary or `ProcessInfo` wire protocol.
- Stats on the team page (`vmux://team/`).
- Historical graphs / sparklines; per-thread or GPU stats.
- Live sub-second uptime counting (uptime already updates each 1s poll).
