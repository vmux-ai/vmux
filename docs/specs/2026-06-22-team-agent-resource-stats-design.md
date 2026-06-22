# Team page: agent spawn time + realtime CPU/memory

## Goal

On the Team page (`vmux://team/`), each **agent** row shows:

- **Uptime since spawn** — live elapsed since the agent process started (e.g. `↑ 4m`), counting up in realtime.
- **Realtime CPU%** — current CPU usage of the agent's process tree.
- **Realtime memory** — resident memory (RSS) of the agent's process tree.

Scope: **agents only**. The `You`/Personal row shows no stats. Page-agents (web, no local OS process) show no stats. CPU/mem are summed over the agent **process tree** (the spawned pid + all descendants) so wrapper processes (node/python launchers) are counted.

## Approach (decided: B — sample on the desktop)

The vmux **service** (a launchd daemon) spawns agent PTY processes and already reports each one's OS `pid` and `created_at_secs` (uptime) to the desktop via `ServiceMessage::ProcessList { Vec<ProcessInfo> }`, cached in the `ServiceProcessList` resource.

We sample CPU/mem **on the desktop** using `sysinfo`, keyed on those pids. This avoids any change to the desktop↔service wire protocol or the service binary, eliminating version-skew risk between an updated desktop and an older running daemon.

Rejected — Approach A (sample in the service, add fields to `ProcessInfo`): changes the wire protocol; desktop and daemon can run mismatched versions.

## Current state (what exists today)

- `crates/vmux_service/src/protocol.rs`: `ProcessInfo { id, shell, cwd, cols, rows, pid: u32, created_at_secs: u64 }`. `created_at_secs` is elapsed-since-spawn (uptime), recomputed per `ListProcesses`.
- `crates/vmux_terminal/src/processes_monitor.rs`: `ServiceProcessList` resource; `request_process_list` sends `ClientMessage::ListProcesses` every 1s **only while a `ProcessesMonitor` webview exists** (services page open).
- `crates/vmux_team/src/plugin.rs`: `build_team_members` builds `Vec<TeamMemberRow>` from agent entities for the active space; `emit_team` pushes `TeamEvent` (rkyv) to team + layout webviews on change. CLI agent entities carry a `ProcessId` (`vmux_service::protocol::ProcessId`, with `Terminal`); page-agents do not.
- `crates/vmux_core/src/event/team.rs`: `TeamEvent { members: Vec<TeamMemberRow> }`, `TeamMemberRow { id, name, initials, color, icon, url, title, sid, is_user, is_running }`. Shared by the native plugin and the wasm page.
- `crates/vmux_team/src/page.rs` (`#[cfg(target_arch = "wasm32")]`): renders rows.

## Design

### 1. Poll while the team page is open (`vmux_terminal`)

The `ListProcesses` poll is the source of pids. It must run while a team page is open, not only the services page.

- Define a marker `WantsProcessList` in `vmux_terminal::processes_monitor`.
- Change `request_process_list`'s gate from `Query<(), With<ProcessesMonitor>>` to also fire when any `WantsProcessList` webview exists (e.g. `Query<(), Or<(With<ProcessesMonitor>, With<WantsProcessList>)>>`).
- `vmux_team` inserts `WantsProcessList` on the team `Browser` entity it spawns in `handle_team_page_open`. (Despawns with the page → poll stops.)

Single poller, no duplicate requests, no new timer.

### 2. Desktop sampler (`crates/vmux_team/src/proc_stats.rs`, new)

A resource + system that turns pids into live CPU/mem:

```
Resource AgentStats(HashMap<ProcessId, AgentStat>)
AgentStat { pid: u32, cpu_percent: f32, mem_bytes: u64, spawn_epoch_ms: u64 }
```

System `sample_agent_stats` (in `Update`), gated on a 1s timer **and** a team page being open (skip work otherwise):

1. Persistent `sysinfo::System` (kept in a `NonSend`/resource wrapper); refresh process list + cpu each tick. CPU% is a delta between refreshes, so the persistent System across 1s ticks yields valid values (first tick reads 0%, acceptable).
2. Build a parent→children map from sysinfo; for each `ProcessInfo` in `ServiceProcessList`, sum `cpu_percent` and `memory()` over `{pid} ∪ descendants(pid)`.
3. `spawn_epoch_ms`: captured once per `ProcessId` the first time it is seen (`now_ms - created_at_secs*1000`) and held stable thereafter, so live uptime counts smoothly and doesn't jitter with integer-second polls.
4. Write `AgentStats`. Drop entries whose `ProcessId` is no longer in `ServiceProcessList`.

Pure, unit-testable helper: `subtree_usage(root_pid, &procs) -> (f32, u64)` over a plain `{pid -> (parent_pid, cpu, mem)}` map.

### 3. Row data (`vmux_core/src/event/team.rs`)

Add to `TeamMemberRow` (additive; `0` means "no process / not applicable"):

```
pid: u32,
cpu_percent: f32,
mem_bytes: u64,
spawn_epoch_ms: u64,
```

Add pure formatters here (native + wasm, with tests; the page module is wasm-only and untested by `cargo test`):

- `fmt_bytes(u64) -> String` → `"332 MB"`, `"1.2 GB"`.
- `fmt_uptime(secs: u64) -> String` → `"4m"`, `"1h 03m"`, `"45s"`.

### 4. Join (`vmux_team/src/plugin.rs`)

- `build_team_members` gains `Res<AgentStats>` and the agent's `Option<&ProcessId>`.
- For each CLI agent with a `ProcessId` present in `AgentStats`, fill `pid`/`cpu_percent`/`mem_bytes`/`spawn_epoch_ms`. Otherwise leave them `0`.
- User row and page-agents: `0` (no stats).

`emit_team` is unchanged structurally; cpu/mem change ~1×/s so `TeamEvent` re-emits ~1×/s while the page is visible. `spawn_epoch_ms` is stable, so it does not add churn.

### 5. Page (`vmux_team/src/page.rs`)

- A 1s local interval signal drives live uptime: `uptime = fmt_uptime((now_ms - spawn_epoch_ms)/1000)`.
- Agent rows with `spawn_epoch_ms != 0` render a muted stat line beneath the sid, e.g.:

  `↑ 4m · 12% · 332 MB`

  (uptime from local tick; cpu = `{cpu_percent:.0}%`; mem = `fmt_bytes(mem_bytes)`).
- Rows without a process (user, page-agents) render no stat line — unchanged appearance.

## Cost / behavior

- Sampling, the `ListProcesses` poll, and `TeamEvent` re-emits happen **only while a team or services page is open**. Nothing runs when closed.
- Cadence 1s. No change to winit update mode; no idle CPU when no page is open.

## Cross-platform

`sysinfo` covers macOS (primary) and Linux (CI). Same-user pids are readable on both. Process-tree discovery via sysinfo parent pids.

## Testing

- `subtree_usage` — pure: tree of fake processes, assert summed cpu+mem over a subtree; sibling/other trees excluded.
- `fmt_bytes` / `fmt_uptime` — pure unit tests (boundaries: <1KB, MB, GB; 0s, seconds, minutes, hours).
- `build_team_members` — seed `AgentStats` + an agent entity with a `ProcessId`; assert the row carries stats; assert user row / no-ProcessId agent carry `0`.
- Poll gate — a webview with `WantsProcessList` (no `ProcessesMonitor`) causes `request_process_list` to send `ListProcesses`.
- `spawn_epoch_ms` stability — same `ProcessId` across two ticks keeps the first epoch.

## Out of scope

- Stats for the user/Personal row or a whole-vmux total.
- Historical graphs / sparklines.
- Per-thread or GPU stats.
- Any change to the service binary or wire protocol.
