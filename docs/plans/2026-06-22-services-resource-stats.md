# Services page: realtime CPU + memory — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show realtime CPU% and memory (RSS) per process on `vmux://services/`, summed over each process's tree.

**Architecture:** Desktop-side sampling (Approach B). A `sysinfo`-backed system in `vmux_terminal::processes_monitor` samples CPU/mem for the pids the service already reports (`ServiceProcessList`), stores them in a `ProcessUsage` resource, and `broadcast_to_monitors` attaches them to the existing `ProcessEntry` pushed to the services webview. No change to the service binary or the `ProcessInfo` wire protocol. Spawn time (uptime) already renders on that page.

**Tech Stack:** Rust, Bevy ECS, `sysinfo` 0.38, Dioxus (wasm page), rkyv/serde events.

---

## File Structure

- `crates/vmux_terminal/Cargo.toml` — add `sysinfo` native dep.
- `crates/vmux_terminal/src/processes_monitor.rs` — `Usage`/`ProcessUsage`/`SysinfoState` types, pure `subtree_usage`, pure `build_process_entries`, `sample_process_usage` system, plugin wiring, broadcast change.
- `crates/vmux_service/src/event.rs` — add `cpu_percent`/`mem_bytes` to `ProcessEntry`; add pure `format_mem`.
- `crates/vmux_service/src/page.rs` — render CPU + Memory in `ProcessCard`.

Note on test placement: `format_mem` lives in `event.rs` (compiled for native + wasm) so it is covered by `cargo test`; `page.rs` is wasm-only and not run by `cargo test`.

CEF note: `cargo test -p vmux_terminal` compiles bevy_cef and is slow on a cold worktree target. Budget for it.

---

### Task 1: Usage types + pure `subtree_usage` (+ sysinfo dep)

**Files:**
- Modify: `crates/vmux_terminal/Cargo.toml`
- Modify: `crates/vmux_terminal/src/processes_monitor.rs`

- [ ] **Step 1: Add the dependency**

In `crates/vmux_terminal/Cargo.toml`, under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, add (keep the list alph, after `rfd`):

```toml
sysinfo = "0.38"
```

- [ ] **Step 2: Write the failing test**

Append to the existing `#[cfg(test)] mod tests` in `processes_monitor.rs` (after `remove_process_from_cached_list_is_optimistic`):

```rust
    #[test]
    fn subtree_usage_sums_whole_tree() {
        let mut procs = std::collections::HashMap::new();
        procs.insert(1, ProcSample { parent: None, cpu: 5.0, mem: 100 });
        procs.insert(2, ProcSample { parent: Some(1), cpu: 10.0, mem: 200 });
        procs.insert(3, ProcSample { parent: Some(2), cpu: 1.0, mem: 50 });
        procs.insert(99, ProcSample { parent: None, cpu: 7.0, mem: 999 });
        let u = subtree_usage(1, &procs);
        assert_eq!(u.cpu_percent, 16.0);
        assert_eq!(u.mem_bytes, 350);
    }

    #[test]
    fn subtree_usage_missing_root_is_zero() {
        let procs = std::collections::HashMap::new();
        assert_eq!(subtree_usage(5, &procs), Usage::default());
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p vmux_terminal subtree_usage 2>&1 | tail -20`
Expected: FAIL — `cannot find type ProcSample` / `subtree_usage` not found.

- [ ] **Step 4: Add the types + function**

In `processes_monitor.rs`, add near the top (after the existing `use` lines) `use std::collections::HashMap;`, and add this block above `#[derive(Resource, Default)] pub struct ServiceProcessList`:

```rust
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Usage {
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

#[derive(Resource, Default)]
pub struct ProcessUsage(pub HashMap<u32, Usage>);

struct ProcSample {
    parent: Option<u32>,
    cpu: f32,
    mem: u64,
}

/// Sum cpu + memory over `root` and all its descendants in `procs`.
/// Returns `Usage::default()` if `root` is absent.
fn subtree_usage(root: u32, procs: &HashMap<u32, ProcSample>) -> Usage {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, s) in procs {
        if let Some(parent) = s.parent {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut total = Usage::default();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(s) = procs.get(&pid) {
            total.cpu_percent += s.cpu;
            total.mem_bytes += s.mem;
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
    }
    total
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_terminal subtree_usage 2>&1 | tail -20`
Expected: PASS (2 tests). `ProcSample`/`ProcessUsage` may warn as unused — resolved in later tasks.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/Cargo.toml crates/vmux_terminal/src/processes_monitor.rs
git commit -m "feat(services): add process subtree usage aggregation"
```

---

### Task 2: `ProcessEntry` fields + `format_mem`

**Files:**
- Modify: `crates/vmux_service/src/event.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_service/src/event.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mem_buckets() {
        assert_eq!(format_mem(0), "—");
        assert_eq!(format_mem(512 * 1024), "<1 MB");
        assert_eq!(format_mem(332 * 1024 * 1024), "332 MB");
        assert_eq!(format_mem(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_service format_mem 2>&1 | tail -20`
Expected: FAIL — `cannot find function format_mem`.

- [ ] **Step 3: Add fields + function**

In `event.rs`, add two fields to `ProcessEntry` (after `pub uptime_secs: u64,`):

```rust
    pub cpu_percent: f32,
    pub mem_bytes: u64,
```

Add this function at module scope (e.g. after the `PreviewLine` struct):

```rust
/// Human-readable RSS. `0` (unsampled) renders as an em dash.
pub fn format_mem(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if bytes == 0 {
        "—".to_string()
    } else if b < MB {
        "<1 MB".to_string()
    } else if b < GB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{:.1} GB", b / GB)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_service format_mem 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Verify no other `ProcessEntry { ... }` literal needs the new fields**

Run: `rg -n 'ProcessEntry \{' crates/`
Expected: only `crates/vmux_terminal/src/processes_monitor.rs` (handled in Task 4). If others appear, add `cpu_percent: 0.0, mem_bytes: 0,` to them.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/src/event.rs
git commit -m "feat(services): add cpu/mem fields + format_mem to ProcessEntry"
```

---

### Task 3: Pure `build_process_entries` (broadcast mapping)

**Files:**
- Modify: `crates/vmux_terminal/src/processes_monitor.rs`

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `processes_monitor.rs`:

```rust
    #[test]
    fn build_entries_attaches_usage() {
        let id = process_id(1);
        let mut usage = ProcessUsage::default();
        usage.0.insert(
            42,
            Usage { cpu_percent: 12.5, mem_bytes: 332 * 1024 * 1024 },
        );
        let entries =
            build_process_entries(&[process_info(id)], &usage, &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pid, 42);
        assert_eq!(entries[0].cpu_percent, 12.5);
        assert_eq!(entries[0].mem_bytes, 332 * 1024 * 1024);
        assert!(!entries[0].attached);
    }

    #[test]
    fn build_entries_defaults_usage_when_missing() {
        let entries = build_process_entries(
            &[process_info(process_id(1))],
            &ProcessUsage::default(),
            &Default::default(),
        );
        assert_eq!(entries[0].cpu_percent, 0.0);
        assert_eq!(entries[0].mem_bytes, 0);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_terminal build_entries 2>&1 | tail -20`
Expected: FAIL — `build_process_entries` not found.

- [ ] **Step 3: Add the function**

In `processes_monitor.rs`, add above `broadcast_to_monitors`:

```rust
fn build_process_entries(
    processes: &[vmux_service::protocol::ProcessInfo],
    usage: &ProcessUsage,
    attached_ids: &std::collections::HashSet<String>,
) -> Vec<ProcessEntry> {
    processes
        .iter()
        .map(|info| {
            let u = usage.0.get(&info.pid).copied().unwrap_or_default();
            ProcessEntry {
                id: info.id.to_string(),
                shell: info.shell.clone(),
                cwd: info.cwd.clone(),
                cols: info.cols,
                rows: info.rows,
                pid: info.pid,
                uptime_secs: info.created_at_secs,
                cpu_percent: u.cpu_percent,
                mem_bytes: u.mem_bytes,
                attached: attached_ids.contains(&info.id.to_string()),
                preview_lines: Vec::new(),
            }
        })
        .collect()
}
```

- [ ] **Step 4: Use it in `broadcast_to_monitors`**

Replace the `let processes: Vec<ProcessEntry> = process_list.processes.iter().map(...).collect();` block (currently lines ~132-146) with:

```rust
    let processes = build_process_entries(&process_list.processes, &usage, &attached_ids);
```

Add `usage: Res<ProcessUsage>,` to the `broadcast_to_monitors` parameter list (after `process_list: Res<ServiceProcessList>,`), and change the early-return guard (currently `if monitors.is_empty() || !process_list.is_changed()`) to:

```rust
    if monitors.is_empty() || !(process_list.is_changed() || usage.is_changed()) {
        return;
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_terminal build_entries 2>&1 | tail -20`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/processes_monitor.rs
git commit -m "feat(services): map sampled usage onto ProcessEntry on broadcast"
```

---

### Task 4: `sample_process_usage` system + plugin wiring

**Files:**
- Modify: `crates/vmux_terminal/src/processes_monitor.rs`

No new unit test (this system is sysinfo + timer glue; the pure pieces are tested in Tasks 1 & 3). Verify by compile + the manual runtime check in Task 6.

- [ ] **Step 1: Add the sysinfo state resource + timer**

In `processes_monitor.rs`, after `struct ProcessesPollTimer(Timer);` add:

```rust
#[derive(Resource)]
struct SysinfoPollTimer(Timer);

#[derive(Resource)]
struct SysinfoState(sysinfo::System);

impl Default for SysinfoState {
    fn default() -> Self {
        Self(sysinfo::System::new())
    }
}
```

(If the compiler reports `sysinfo::System` is not `Send`/`Sync`, change `SysinfoState` to a non-send resource: register with `app.init_non_send_resource::<SysinfoState>()` in Step 3 and take `mut sys: NonSendMut<SysinfoState>` in Step 2.)

- [ ] **Step 2: Add the system**

Add after `request_process_list`:

```rust
/// Sample CPU + memory for each service-managed pid (process tree) while the
/// services page is open. Runs on its own 1s cadence so cpu deltas are valid.
fn sample_process_usage(
    time: Res<Time>,
    mut timer: ResMut<SysinfoPollTimer>,
    monitors: Query<(), With<ProcessesMonitor>>,
    process_list: Res<ServiceProcessList>,
    mut sys: ResMut<SysinfoState>,
    mut usage: ResMut<ProcessUsage>,
) {
    if monitors.is_empty() {
        return;
    }
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    sys.0
        .refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let procs: HashMap<u32, ProcSample> = sys
        .0
        .processes()
        .iter()
        .map(|(pid, p)| {
            (
                pid.as_u32(),
                ProcSample {
                    parent: p.parent().map(|pp| pp.as_u32()),
                    cpu: p.cpu_usage(),
                    mem: p.memory(),
                },
            )
        })
        .collect();

    let mut map = HashMap::with_capacity(process_list.processes.len());
    for info in &process_list.processes {
        map.insert(info.pid, subtree_usage(info.pid, &procs));
    }
    usage.0 = map;
}
```

- [ ] **Step 3: Register resources + system in the plugin**

In `ProcessesMonitorPlugin::build`, change the builder chain to add the two resources and insert `sample_process_usage` between request and broadcast:

```rust
        app.init_resource::<ServiceProcessList>()
            .init_resource::<ProcessUsage>()
            .init_resource::<SysinfoState>()
            .insert_resource(ProcessesPollTimer(Timer::from_seconds(
                1.0,
                TimerMode::Repeating,
            )))
            .insert_resource(SysinfoPollTimer(Timer::from_seconds(
                1.0,
                TimerMode::Repeating,
            )))
            .add_plugins(BinEventEmitterPlugin::<(
                ProcessNavigateEvent,
                ProcessKillEvent,
                ProcessKillAllEvent,
            )>::for_hosts(&["services"]))
            .add_systems(
                Update,
                (
                    request_process_list,
                    sample_process_usage,
                    broadcast_to_monitors,
                )
                    .chain(),
            )
            .add_observer(on_process_navigate)
            .add_observer(on_process_kill)
            .add_observer(on_process_kill_all);
```

(If using the non-send fallback from Task 4 Step 1, replace `.init_resource::<SysinfoState>()` with `.init_non_send_resource::<SysinfoState>()`.)

- [ ] **Step 4: Build the crate**

Run: `cargo test -p vmux_terminal --no-run 2>&1 | tail -20`
Expected: compiles. Fix any sysinfo API mismatch (see Step 1 note and adjust `refresh_processes` args if the installed 0.38 minor differs).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_terminal/src/processes_monitor.rs
git commit -m "feat(services): sample per-process cpu/mem via sysinfo while page open"
```

---

### Task 5: Render CPU + Memory in `ProcessCard`

**Files:**
- Modify: `crates/vmux_service/src/page.rs`

- [ ] **Step 1: Add the rows**

In `ProcessCard` (the `// Row 2: metadata grid` block), add two `MetaRow`s after the `PID` row:

```rust
                MetaRow { label: "PID", value: process.pid.to_string() }
                MetaRow { label: "CPU", value: format!("{:.0}%", process.cpu_percent) }
                MetaRow { label: "Memory", value: format_mem(process.mem_bytes) }
                MetaRow { label: "Size", value: format!("{}x{}", process.cols, process.rows) }
```

`format_mem` is already in scope via the existing `use crate::event::*;`.

- [ ] **Step 2: Type-check the wasm page**

Run: `cargo check -p vmux_service --target wasm32-unknown-unknown 2>&1 | tail -20`
Expected: compiles. (Requires the `wasm32-unknown-unknown` target; `make ensure-mac-deps` installs it.)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_service/src/page.rs
git commit -m "feat(services): show CPU% and memory on process cards"
```

---

### Task 6: Verify

- [ ] **Step 1: Native tests**

Run: `cargo test -p vmux_terminal -p vmux_service 2>&1 | tail -30`
Expected: all pass (subtree_usage ×2, build_entries ×2, format_mem ×1, existing tests).

- [ ] **Step 2: Lint**

Run: `cargo clippy -p vmux_terminal -p vmux_service --all-targets 2>&1 | tail -20`
Expected: no warnings. Fix any.

Run: `cargo fmt -p vmux_terminal -p vmux_service`

- [ ] **Step 3: Manual runtime check (user)**

Build/run the app, open `vmux://services/`, confirm each card shows a CPU% that moves under load and a memory value; an agent (e.g. Vibe) shows nonzero memory; values refresh ~1s; closing the page stops updates. (Per project practice, the user runtime-tests UI.)

- [ ] **Step 4: Open PR**

```bash
git push -u origin feat/services-resource-stats
gh pr create --base main --title "feat(services): realtime CPU + memory per process" --body "..."
```

---

## Self-Review

- **Spec coverage:** CPU/mem sampling (Task 4), subtree sum (Task 1), ProcessEntry fields (Task 2), broadcast mapping + gate (Task 3), page render (Task 5), format_mem (Task 2), tests (Tasks 1–3), no service/protocol change (sampling is desktop-side). Spawn time already present — no task needed. ✓
- **Placeholders:** PR `--body "..."` is filled at PR time from the diff; all code steps are complete. ✓
- **Type consistency:** `Usage{cpu_percent,mem_bytes}`, `ProcessUsage(HashMap<u32,Usage>)`, `ProcSample{parent,cpu,mem}`, `subtree_usage(u32,&HashMap)->Usage`, `build_process_entries(&[ProcessInfo],&ProcessUsage,&HashSet<String>)->Vec<ProcessEntry>` used identically across tasks. `ProcessEntry` gains `cpu_percent: f32`, `mem_bytes: u64` (Task 2) consumed in Tasks 3 & 5. ✓
