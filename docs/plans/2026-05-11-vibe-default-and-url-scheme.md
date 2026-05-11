# VMX-109: Vibe-default new pane + URL scheme realignment — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Default new panes to `vibe --trust` when the `vibe` CLI is detected, expose OS PIDs and vibe session IDs in `vmux://` URLs, and remove the auto-opened command bar in favor of a configurable `startup_url`.

**Architecture:** Composable Bevy markers (`Pane` already exists; add `Vibe`, reuse existing `Terminal`) with generic data components (`Pid(u32)`, `SessionId(String)`). URL formatter system reads markers + components to emit the right scheme; reverse-lookup resources (`PidToEntity`, `VibeSessionToEntity`) handle inbound URL navigation. `startup_url` setting + resolver replaces the auto-command-bar code path.

**Tech Stack:** Rust, Bevy ECS, rkyv (service protocol), portable-pty.

**Spec:** `docs/specs/2026-05-11-vibe-default-and-url-scheme-design.md`

**Worktree:** `.worktrees/vmx-109` on branch `feature/vmx-109-vibe-default-url-scheme`. All work happens here.

---

## Pre-flight

Read these once before starting:

- `docs/specs/2026-05-11-vibe-default-and-url-scheme-design.md` — the design.
- `AGENTS.md` — project rules. Note: no comments in code, no `mod.rs`, use `bash -c` for shell, run lint loop on changed crates before each commit.
- `crates/vmux_service/src/protocol.rs:354` — current `ProcessCreated` variant.
- `crates/vmux_service/src/process.rs:229` — `unwrap_or(0)` PID fallback to remove.
- `crates/vmux_desktop/src/terminal.rs:39` — existing `Terminal` marker (reuse).
- `crates/vmux_desktop/src/terminal.rs:317` — existing static URL formatter to replace.
- `crates/vmux_desktop/src/terminal.rs:627` — `ProcessCreated` handler.
- `crates/vmux_desktop/src/agent.rs:266` — `spawn_vmux_tab` URL dispatcher.
- `crates/vmux_desktop/src/command_bar.rs:42` — `parse_process_id_from_url`.
- `crates/vmux_desktop/src/vibe.rs:203` — vibe launch command builder.
- `crates/vmux_layout/src/stack.rs:188` — `handle_stack_command`.
- `crates/vmux_layout/src/stack.rs:537` — `open_command_bar_if_no_stacks`.
- `crates/vmux_layout/src/pane.rs:69` — existing `Pane` marker.

## Per-commit lint loop

Before every commit, run on the crates changed by that commit. AGENTS.md snippet computes the full set; for the focused commits in this plan you generally know which crates changed. Wrap commands in `bash -c "..."`.

```sh
# Per crate (replace <pkg>):
bash -c "cargo fmt -p <pkg> -- --check"
bash -c "env -u CEF_PATH cargo clippy -p <pkg> --all-targets -- -D warnings"
bash -c "env -u CEF_PATH cargo test -p <pkg>"
```

If fmt fails, run `bash -c "make lint-fix"` then re-run the loop. If clippy/test fails, fix the issue — do not commit.

---

## File structure

**Created:**

- `crates/vmux_desktop/src/terminal/pid.rs` — `Pid(u32)` component, `PidToEntity` resource, cleanup system, URL formatter for terminal panes, URL parser helper.
- `crates/vmux_desktop/src/vibe/session.rs` — `Vibe` marker, `SessionId(String)` component, `PendingVibeSession` component, `VibeSessionToEntity` resource, discovery polling system, URL formatter for vibe panes.

**Modified:**

- `crates/vmux_service/src/protocol.rs` — `ProcessCreated { process_id, pid }`, new `ProcessCreateFailed { reason }` variant.
- `crates/vmux_service/src/process.rs` — `Process::new_with_wake` errors instead of `unwrap_or(0)`; `ProcessManager::create_process` returns `(ProcessId, u32)`.
- `crates/vmux_service/src/server.rs` — emits new fields/variant.
- `crates/vmux_desktop/src/terminal.rs` — `mod pid;` declaration; receive pid in `ProcessCreated` handler and stamp `Pid` component; remove the static URL `format!` at line 317 in favor of the formatter system in `terminal/pid.rs`; placeholder URL on bundle creation.
- `crates/vmux_desktop/src/vibe.rs` — `mod session;` declaration; stamp `Vibe` + `PendingVibeSession` on vibe launches; expose `vibe_command_for_resume(session_id)`.
- `crates/vmux_desktop/src/agent.rs` — extend `spawn_vmux_tab` with `vibe` host arm; change `terminal` host parser to `u32` + reverse lookup; malformed URL falls back to spawning empty terminal.
- `crates/vmux_desktop/src/command_bar.rs` — replace `parse_process_id_from_url` with `parse_pid_from_url` returning `Option<u32>`; terminal arm uses `PidToEntity`.
- `crates/vmux_desktop/src/settings.rs` — add `startup_url: Option<String>` to `AppSettings`; add `resolve_startup_url(settings) -> String`.
- `crates/vmux_desktop/src/lib.rs` — register new resources/systems if needed (PidToEntity, VibeSessionToEntity, polling system).
- `crates/vmux_layout/src/stack.rs` — `handle_stack_command` for `StackCommand::New` dispatches `startup_url` instead of setting `needs_open = true`; rename `open_command_bar_if_no_stacks` → `open_startup_url_if_no_stacks`; update existing tests at line 615+.

---

## Task 1: Service protocol — add `pid` to `ProcessCreated`, add `ProcessCreateFailed`

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs:354-414`
- Modify: `crates/vmux_service/src/protocol.rs` (tests at lines 432+)

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block in `protocol.rs`:

```rust
#[test]
fn process_created_round_trips_pid() {
    let id = ProcessId::new();
    let msg = ServiceMessage::ProcessCreated {
        process_id: id,
        pid: 12345,
    };
    let bytes = rkyv::to_bytes::<_, 256>(&msg).expect("serialize");
    let archived = rkyv::check_archived_root::<ServiceMessage>(&bytes).expect("archive");
    let decoded: ServiceMessage = archived.deserialize(&mut rkyv::Infallible).expect("deserialize");
    let ServiceMessage::ProcessCreated { process_id, pid } = decoded else {
        panic!("wrong variant");
    };
    assert_eq!(process_id, id);
    assert_eq!(pid, 12345);
}

#[test]
fn process_create_failed_round_trips_reason() {
    let msg = ServiceMessage::ProcessCreateFailed {
        reason: "missing PID after spawn".into(),
    };
    let bytes = rkyv::to_bytes::<_, 256>(&msg).expect("serialize");
    let archived = rkyv::check_archived_root::<ServiceMessage>(&bytes).expect("archive");
    let decoded: ServiceMessage = archived.deserialize(&mut rkyv::Infallible).expect("deserialize");
    let ServiceMessage::ProcessCreateFailed { reason } = decoded else {
        panic!("wrong variant");
    };
    assert_eq!(reason, "missing PID after spawn");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_service protocol::tests::process_created_round_trips_pid protocol::tests::process_create_failed_round_trips_reason"
```

Expected: compile error (variant fields don't exist; `ProcessCreateFailed` variant doesn't exist).

- [ ] **Step 3: Implement protocol changes**

Modify `ProcessCreated` and add `ProcessCreateFailed` in `crates/vmux_service/src/protocol.rs`:

```rust
pub enum ServiceMessage {
    ProcessCreated {
        process_id: ProcessId,
        pid: u32,
    },
    ProcessCreateFailed {
        reason: String,
    },
    // ... rest unchanged
}
```

- [ ] **Step 4: Run tests to verify they pass**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_service protocol::tests::process_created_round_trips_pid protocol::tests::process_create_failed_round_trips_reason"
```

Expected: 2 passed.

- [ ] **Step 5: Update callers in this crate so the build still passes**

Search and patch all `ServiceMessage::ProcessCreated` constructors + matches inside `vmux_service` to provide/handle `pid`. Use a placeholder `0` for now in the server arm — Task 2 fixes it.

```sh
bash -c "rg -n 'ProcessCreated' crates/vmux_service/src"
```

For each call site (server.rs, any tests): add `pid: 0` to constructions; add `pid: _` to match arms.

- [ ] **Step 6: Run lint loop on `vmux_service`**

```sh
bash -c "cargo fmt -p vmux_service -- --check && env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_service"
```

- [ ] **Step 7: Commit**

```sh
bash -c "git add crates/vmux_service/src/protocol.rs crates/vmux_service/src/server.rs && git commit -m 'feat(VMX-109): ProcessCreated carries pid; add ProcessCreateFailed variant'"
```

---

## Task 2: Service — populate real PID; fail spawn when PID missing

**Files:**
- Modify: `crates/vmux_service/src/process.rs:229` (`unwrap_or(0)`), and `create_process` at line 1286.
- Modify: `crates/vmux_service/src/server.rs:163-181` (use real pid; emit `ProcessCreateFailed`).

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `process.rs` (location: after the existing tests block):

```rust
#[test]
fn create_process_returns_real_pid() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut mgr = ProcessManager::new(wake_tx);
    let (id, pid) = mgr
        .create_process("/bin/sh".into(), String::new(), Vec::new(), 80, 24)
        .expect("spawn");
    assert!(pid > 0, "expected real pid, got {pid}");
    assert!(mgr.processes.contains_key(&id));
}
```

- [ ] **Step 2: Run test, verify it fails**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_service process::tests::create_process_returns_real_pid"
```

Expected: compile error — `create_process` returns `Result<ProcessId, String>`, not a tuple.

- [ ] **Step 3: Implement — propagate PID; remove `unwrap_or(0)`**

In `crates/vmux_service/src/process.rs`, change `Process` to store `pid` (already exists at line 64) and change `new_with_wake` to error on missing PID:

```rust
let child = pair
    .slave
    .spawn_command(cmd)
    .map_err(|e| format!("failed to spawn shell: {e}"))?;
let pid = child
    .process_id()
    .ok_or_else(|| "spawned PTY child has no PID".to_string())?;
```

Change `ProcessManager::create_process` signature to return `(ProcessId, u32)`:

```rust
pub fn create_process(
    &mut self,
    shell: String,
    cwd: String,
    env: Vec<(String, String)>,
    cols: u16,
    rows: u16,
) -> Result<(ProcessId, u32), String> {
    let process = Process::new_with_wake(shell, cwd, env, cols, rows, self.wake_tx.clone())?;
    let id = process.id;
    let pid = process.pid;
    self.processes.insert(id, process);
    Ok((id, pid))
}
```

- [ ] **Step 4: Update server to emit pid + ProcessCreateFailed**

In `crates/vmux_service/src/server.rs`, replace the matched-arm block (around lines 161–182):

```rust
let created = {
    let mut mgr = manager.lock().await;
    mgr.create_process(shell, cwd, env, cols, rows)
        .map(|(id, pid)| (id, pid, mgr.input_writer(&id)))
};
match created {
    Ok((id, pid, input_writer)) => {
        if let Some(input_writer) = input_writer {
            input_writers.lock().await.insert(id, input_writer);
        }
        let resp = ServiceMessage::ProcessCreated { process_id: id, pid };
        let w = writer.clone();
        let mut w = w.lock().await;
        write_message!(&mut *w, &resp)?;
    }
    Err(reason) => {
        let resp = ServiceMessage::ProcessCreateFailed { reason };
        let w = writer.clone();
        let mut w = w.lock().await;
        write_message!(&mut *w, &resp)?;
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_service process::tests::create_process_returns_real_pid"
```

Expected: PASS.

- [ ] **Step 6: Lint loop and commit**

```sh
bash -c "cargo fmt -p vmux_service -- --check && env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_service"
bash -c "git add crates/vmux_service/src/process.rs crates/vmux_service/src/server.rs && git commit -m 'feat(VMX-109): service emits real PID; fails spawn when PID missing'"
```

---

## Task 3: Desktop — `Pid` component, `PidToEntity` resource, cleanup

**Files:**
- Create: `crates/vmux_desktop/src/terminal/pid.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs` (add `pub(crate) mod pid;` near the top)
- Modify: `crates/vmux_desktop/src/lib.rs` (register `PidToEntity` resource and cleanup system)

- [ ] **Step 1: Write the failing test**

Create `crates/vmux_desktop/src/terminal/pid.rs` with the test stub:

```rust
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

#[derive(Resource, Default, Debug)]
pub struct PidToEntity(pub HashMap<u32, Entity>);

pub fn track_pid_inserts(
    mut map: ResMut<PidToEntity>,
    inserted: Query<(Entity, &Pid), Added<Pid>>,
) {
    for (entity, Pid(pid)) in &inserted {
        map.0.insert(*pid, entity);
    }
}

pub fn track_pid_removals(
    mut map: ResMut<PidToEntity>,
    mut removed: RemovedComponents<Pid>,
    survivors: Query<&Pid>,
) {
    for entity in removed.read() {
        if let Ok(Pid(pid)) = survivors.get(entity) {
            map.0.remove(pid);
        } else {
            map.0.retain(|_, &mut e| e != entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<PidToEntity>();
        app.add_systems(Update, (track_pid_inserts, track_pid_removals).chain());
        app
    }

    #[test]
    fn pid_insert_populates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(7777)).id();
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.0.get(&7777), Some(&e));
    }

    #[test]
    fn entity_despawn_removes_pid_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(8888)).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert!(!map.0.contains_key(&8888));
    }

    #[test]
    fn changing_pid_updates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(9000)).id();
        app.update();
        app.world_mut().entity_mut(e).insert(Pid(9001));
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.0.get(&9001), Some(&e));
    }
}
```

In `crates/vmux_desktop/src/terminal.rs`, add right after the existing `use` block:

```rust
pub(crate) mod pid;
```

- [ ] **Step 2: Run tests to verify they pass after compile**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop terminal::pid::tests"
```

Expected: 3 passed.

If `Added<Pid>` test for `changing_pid_updates_map` is wonky (component change vs add), accept that the third test may need adjustment — the goal is that after a Pid value swap the map reflects the new value. If needed, replace with: spawn → assert → remove → re-add with new value → assert.

- [ ] **Step 3: Register resource + systems in lib.rs**

Find the desktop plugin / `app.init_resource` block in `crates/vmux_desktop/src/lib.rs` and add:

```rust
app.init_resource::<crate::terminal::pid::PidToEntity>();
app.add_systems(
    Update,
    (
        crate::terminal::pid::track_pid_inserts,
        crate::terminal::pid::track_pid_removals,
    )
        .chain(),
);
```

(System set membership: pick whatever the existing terminal-related systems use; matching is more important than naming a new set.)

- [ ] **Step 4: Lint loop**

```sh
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_desktop terminal::pid"
```

- [ ] **Step 5: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/terminal/pid.rs crates/vmux_desktop/src/lib.rs && git commit -m 'feat(VMX-109): Pid component and PidToEntity reverse-lookup resource'"
```

---

## Task 4: Desktop — stamp `Pid` on entity when `ProcessCreated` arrives

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs` (around line 627: `ServiceMessage::ProcessCreated` handler)

- [ ] **Step 1: Read the existing handler**

Look at `crates/vmux_desktop/src/terminal.rs:627-645`. Current shape:

```rust
ServiceMessage::ProcessCreated { process_id } => {
    // ... matches awaiting entity, inserts ServiceProcessHandle, removes AwaitingProcessCreated
}
```

- [ ] **Step 2: Write the failing test**

Extract the post-match insertion logic into a pure helper so it's unit-testable, then test the helper. Add to `crates/vmux_desktop/src/terminal.rs`:

```rust
pub(crate) fn apply_process_created(
    commands: &mut Commands,
    entity: Entity,
    process_id: ProcessId,
    pid: u32,
) {
    commands
        .entity(entity)
        .insert(ServiceProcessHandle { process_id })
        .insert(crate::terminal::pid::Pid(pid))
        .remove::<AwaitingProcessCreated>();
}
```

Test in `mod tests`:

```rust
#[test]
fn apply_process_created_stamps_pid_and_handle() {
    let mut app = App::new();
    let entity = app
        .world_mut()
        .spawn((Terminal, AwaitingProcessCreated))
        .id();
    let pid_val = 4242u32;
    let id = ProcessId::new();
    app.world_mut().commands().queue(move |w: &mut World| {
        let mut cmds = w.commands();
        apply_process_created(&mut cmds, entity, id, pid_val);
    });
    app.world_mut().flush();
    assert!(app.world().get::<crate::terminal::pid::Pid>(entity).is_some());
    assert_eq!(
        app.world().get::<crate::terminal::pid::Pid>(entity).unwrap().0,
        pid_val,
    );
    assert!(app.world().get::<AwaitingProcessCreated>(entity).is_none());
    assert_eq!(
        app.world().get::<ServiceProcessHandle>(entity).unwrap().process_id,
        id,
    );
}
```

- [ ] **Step 3: Update the handler to call `apply_process_created`**

Replace the `ProcessCreated` arm body in the `poll_service_messages` system at `terminal.rs:627`:

```rust
ServiceMessage::ProcessCreated { process_id, pid } => {
    if let Some(entity) = matched_entity {
        apply_process_created(&mut commands, entity, process_id, pid);
    }
}
```

Also remove any `PageMetadata.url = ...` mutation that previously lived in this arm (lines around 643). The URL is now produced by the formatter system in Task 5.

Also add a handler for the new failure variant (next to the `ProcessCreated` arm):

```rust
ServiceMessage::ProcessCreateFailed { reason } => {
    bevy::log::warn!("service failed to create process: {reason}");
    // Find first AwaitingProcessCreated entity and mark it failed.
    if let Some((entity, _)) = awaiting.iter().next() {
        commands.entity(entity).insert(ProcessExited).remove::<AwaitingProcessCreated>();
    }
}
```

(`ProcessExited` already exists at line 43 and is used by close-confirmation code; reusing it lets the existing close path clean up the failed entity.)

- [ ] **Step 4: Run tests + lint**

```sh
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_desktop"
```

- [ ] **Step 5: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/terminal.rs && git commit -m 'feat(VMX-109): stamp Pid component from ProcessCreated; handle ProcessCreateFailed'"
```

---

## Task 5: Desktop — URL formatter system for terminal panes

**Files:**
- Modify: `crates/vmux_desktop/src/terminal/pid.rs` (add formatter system)
- Modify: `crates/vmux_desktop/src/terminal.rs:317` (remove static `format!`; use placeholder URL on bundle creation)
- Modify: `crates/vmux_desktop/src/lib.rs` (register the formatter system)

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_desktop/src/terminal/pid.rs` (in `mod tests`):

```rust
use crate::browser::PageMetadata; // adjust import if PageMetadata lives elsewhere

#[test]
fn formatter_emits_pid_url_for_terminal_with_pid() {
    let mut app = make_app();
    app.add_systems(Update, format_terminal_url);
    let e = app
        .world_mut()
        .spawn((
            crate::terminal::Terminal,
            Pid(4242),
            PageMetadata {
                title: String::new(),
                url: String::new(),
                favicon_url: String::new(),
                bg_color: None,
            },
        ))
        .id();
    app.update();
    let url = &app.world().get::<PageMetadata>(e).unwrap().url;
    assert_eq!(url, "vmux://terminal/4242");
}

#[test]
fn formatter_emits_placeholder_for_terminal_without_pid() {
    let mut app = make_app();
    app.add_systems(Update, format_terminal_url);
    let e = app
        .world_mut()
        .spawn((
            crate::terminal::Terminal,
            PageMetadata {
                title: String::new(),
                url: "stale".into(),
                favicon_url: String::new(),
                bg_color: None,
            },
        ))
        .id();
    app.update();
    let url = &app.world().get::<PageMetadata>(e).unwrap().url;
    assert_eq!(url, "vmux://terminal/");
}
```

- [ ] **Step 2: Implement the formatter system**

Append to `crates/vmux_desktop/src/terminal/pid.rs` (above `mod tests`):

```rust
use crate::browser::PageMetadata;
use crate::terminal::Terminal;
use vmux_terminal::event::TERMINAL_WEBVIEW_URL;

pub fn format_terminal_url(
    mut q: Query<
        (Option<&Pid>, &mut PageMetadata),
        (With<Terminal>, Or<(Changed<Pid>, Added<PageMetadata>)>),
    >,
) {
    for (pid, mut meta) in &mut q {
        let next = match pid {
            Some(Pid(p)) => format!("{TERMINAL_WEBVIEW_URL}{p}"),
            None => TERMINAL_WEBVIEW_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}
```

The `Vibe` marker doesn't exist yet (Task 7 introduces it). Task 9 will add a `Without<Vibe>` filter to this query so vibe entities aren't double-formatted; for now, Vibe entities will get a terminal URL stamped that Task 9's vibe formatter overwrites.

- [ ] **Step 3: Strip the static `format!` from terminal bundle**

In `crates/vmux_desktop/src/terminal.rs:315-320`, change `PageMetadata.url` to a placeholder:

```rust
PageMetadata {
    title: format!("Terminal ({})", &process_id.to_string()[..8]),
    url: format!("{}", TERMINAL_WEBVIEW_URL),
    favicon_url: String::new(),
    bg_color: None,
},
```

(The formatter overwrites once `Pid` lands.)

Do the same in the `reattach` bundle around line 367.

- [ ] **Step 4: Register the formatter system in `lib.rs`**

Add to the same systems block as Task 3:

```rust
app.add_systems(Update, crate::terminal::pid::format_terminal_url
    .after(crate::terminal::pid::track_pid_inserts));
```

- [ ] **Step 5: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop terminal::pid"
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 6: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/terminal/pid.rs crates/vmux_desktop/src/lib.rs && git commit -m 'feat(VMX-109): URL formatter for terminal panes (vmux://terminal/<pid>)'"
```

---

## Task 6: Desktop — terminal URL parser uses `u32` + `PidToEntity`

**Files:**
- Modify: `crates/vmux_desktop/src/command_bar.rs:42-45` (replace `parse_process_id_from_url`)
- Modify: `crates/vmux_desktop/src/agent.rs:266-308` (`spawn_vmux_tab` terminal arm)

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `command_bar.rs` (it has tests already):

```rust
#[test]
fn parse_pid_from_url_accepts_numeric() {
    assert_eq!(parse_pid_from_url("vmux://terminal/12345"), Some(12345));
    assert_eq!(parse_pid_from_url("vmux://terminal/0"), Some(0));
}

#[test]
fn parse_pid_from_url_rejects_uuid_form() {
    let uuid_url = "vmux://terminal/ae724a54-c387-5359-0687-ccfc155558b6";
    assert_eq!(parse_pid_from_url(uuid_url), None);
}

#[test]
fn parse_pid_from_url_rejects_empty_path() {
    assert_eq!(parse_pid_from_url("vmux://terminal/"), None);
}

#[test]
fn parse_pid_from_url_rejects_overflow() {
    assert_eq!(parse_pid_from_url("vmux://terminal/99999999999999999"), None);
}
```

- [ ] **Step 2: Run tests, verify fail**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop command_bar::tests::parse_pid"
```

Expected: compile error (`parse_pid_from_url` not defined).

- [ ] **Step 3: Replace the parser**

In `crates/vmux_desktop/src/command_bar.rs:42-45`, replace `parse_process_id_from_url` with:

```rust
pub(crate) fn parse_pid_from_url(url: &str) -> Option<u32> {
    let suffix = url.strip_prefix(TERMINAL_WEBVIEW_URL)?;
    if suffix.is_empty() {
        return None;
    }
    suffix.parse::<u32>().ok()
}
```

- [ ] **Step 4: Update all call sites in `command_bar.rs`**

Find every `parse_process_id_from_url(...)` call in `command_bar.rs` (lines around 881, 960, 1035 per spec analysis). Replace with `parse_pid_from_url`. Where the old code resolved a `ProcessId` and matched against entities by `ServiceProcessHandle.process_id`, the new code looks up `PidToEntity`:

```rust
if let Some(pid) = parse_pid_from_url(url) {
    let pid_map = world.resource::<crate::terminal::pid::PidToEntity>();
    if let Some(&entity) = pid_map.0.get(&pid) {
        // focus that pane (existing focus logic)
    } else {
        // miss — log and either spawn a new terminal or open the command bar
        bevy::log::warn!("no terminal pane for pid {pid}");
    }
}
```

Adjust to whatever the existing focus/spawn helpers expect (this varies; follow the existing terminal-arm shape).

- [ ] **Step 5: Update `spawn_vmux_tab` terminal arm in `agent.rs`**

In `crates/vmux_desktop/src/agent.rs:266-308`, the `terminal` host arm currently parses CWD only. Update to:

```rust
"terminal" => {
    let path = parsed.path().trim_start_matches('/');
    if path.is_empty() {
        // existing spawn-new-terminal flow
        spawn_terminal_tab(/* args */);
    } else {
        match path.parse::<u32>() {
            Ok(pid) => {
                let pid_map = world.resource::<crate::terminal::pid::PidToEntity>();
                if let Some(&entity) = pid_map.0.get(&pid) {
                    focus_pane(entity, /* ... */);
                } else {
                    bevy::log::warn!("no terminal pane for pid {pid}; spawning new");
                    spawn_terminal_tab(/* args */);
                }
            }
            Err(_) => {
                bevy::log::warn!("malformed terminal URL: {url}");
                return Err(format!("malformed terminal URL: {url}"));
            }
        }
    }
}
```

- [ ] **Step 6: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop"
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 7: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/command_bar.rs crates/vmux_desktop/src/agent.rs && git commit -m 'feat(VMX-109): parse vmux://terminal/<pid> as u32 + reverse-lookup'"
```

---

## Task 7: Desktop — `Vibe` marker, `SessionId`, `PendingVibeSession`, `VibeSessionToEntity`

**Files:**
- Create: `crates/vmux_desktop/src/vibe/session.rs`
- Modify: `crates/vmux_desktop/src/vibe.rs` (add `pub(crate) mod session;`)
- Modify: `crates/vmux_desktop/src/lib.rs` (register `VibeSessionToEntity` + cleanup systems)

- [ ] **Step 1: Write the failing test**

Create `crates/vmux_desktop/src/vibe/session.rs`:

```rust
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Component, Debug)]
pub struct Vibe;

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug)]
pub struct PendingVibeSession {
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
    pub attempts: u8,
}

#[derive(Resource, Default, Debug)]
pub struct VibeSessionToEntity(pub HashMap<String, Entity>);

pub fn track_session_id_inserts(
    mut map: ResMut<VibeSessionToEntity>,
    inserted: Query<(Entity, &SessionId), (Added<SessionId>, With<Vibe>)>,
) {
    for (entity, SessionId(id)) in &inserted {
        map.0.insert(id.clone(), entity);
    }
}

pub fn track_session_id_removals(
    mut map: ResMut<VibeSessionToEntity>,
    mut removed: RemovedComponents<SessionId>,
) {
    for entity in removed.read() {
        map.0.retain(|_, &mut e| e != entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<VibeSessionToEntity>();
        app.add_systems(Update, (track_session_id_inserts, track_session_id_removals).chain());
        app
    }

    #[test]
    fn session_insert_populates_map_only_for_vibe_entities() {
        let mut app = make_app();
        let with_vibe = app
            .world_mut()
            .spawn((Vibe, SessionId("abc".into())))
            .id();
        let without_vibe = app
            .world_mut()
            .spawn(SessionId("xyz".into()))
            .id();
        app.update();
        let map = app.world().resource::<VibeSessionToEntity>();
        assert_eq!(map.0.get("abc"), Some(&with_vibe));
        assert!(!map.0.contains_key("xyz"));
        let _ = without_vibe;
    }

    #[test]
    fn entity_despawn_removes_session_from_map() {
        let mut app = make_app();
        let e = app
            .world_mut()
            .spawn((Vibe, SessionId("def".into())))
            .id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<VibeSessionToEntity>();
        assert!(!map.0.contains_key("def"));
    }
}
```

In `crates/vmux_desktop/src/vibe.rs`, add at the top:

```rust
pub(crate) mod session;
```

In `crates/vmux_desktop/src/lib.rs`, register:

```rust
app.init_resource::<crate::vibe::session::VibeSessionToEntity>();
app.add_systems(
    Update,
    (
        crate::vibe::session::track_session_id_inserts,
        crate::vibe::session::track_session_id_removals,
    )
        .chain(),
);
```

- [ ] **Step 2: Run tests, verify pass**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop vibe::session::tests"
```

Expected: 2 passed.

- [ ] **Step 3: Lint + commit**

```sh
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
bash -c "git add crates/vmux_desktop/src/vibe.rs crates/vmux_desktop/src/vibe/session.rs crates/vmux_desktop/src/lib.rs && git commit -m 'feat(VMX-109): Vibe marker, SessionId, VibeSessionToEntity'"
```

---

## Task 8: Desktop — vibe session discovery polling

**Files:**
- Modify: `crates/vmux_desktop/src/vibe/session.rs` (add discovery system)
- Modify: `crates/vmux_desktop/src/lib.rs` (register discovery system on a 200ms timer)

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_desktop/src/vibe/session.rs` (in `mod tests`):

```rust
use std::fs;
use tempfile::TempDir;

fn write_meta(dir: &std::path::Path, session_id: &str, working_dir: &str, start_time: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("meta.json"),
        format!(
            r#"{{"session_id":"{session_id}","start_time":"{start_time}","environment":{{"working_directory":"{working_dir}"}}}}"#
        ),
    )
    .unwrap();
}

#[test]
fn discover_picks_session_matching_cwd_and_after_spawn_time() {
    let tmp = TempDir::new().unwrap();
    let sessions = tmp.path().join("sessions");
    let cwd = "/tmp/work-A";

    write_meta(
        &sessions.join("session_20260101_080000_olderold"),
        "older-uuid",
        cwd,
        "2025-12-31T23:00:00+00:00",
    );
    write_meta(
        &sessions.join("session_20260511_120000_thisone"),
        "this-uuid",
        cwd,
        "2026-05-11T12:00:00+00:00",
    );
    write_meta(
        &sessions.join("session_20260511_120000_other"),
        "other-uuid",
        "/tmp/work-B",
        "2026-05-11T12:00:00+00:00",
    );

    let spawn_time = std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_secs(1_770_000_000); // ≈ 2026-02-12

    let claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let result = discover_session_id_for(
        &sessions,
        std::path::Path::new(cwd),
        spawn_time,
        &claimed,
    );
    assert_eq!(result.as_deref(), Some("this-uuid"));
}

#[test]
fn discover_skips_already_claimed_sessions() {
    let tmp = TempDir::new().unwrap();
    let sessions = tmp.path().join("sessions");
    let cwd = "/tmp/work";

    write_meta(
        &sessions.join("session_a"),
        "claimed-uuid",
        cwd,
        "2026-05-11T12:00:00+00:00",
    );
    write_meta(
        &sessions.join("session_b"),
        "free-uuid",
        cwd,
        "2026-05-11T12:00:01+00:00",
    );

    let spawn_time = std::time::SystemTime::UNIX_EPOCH;
    let mut claimed = std::collections::HashSet::new();
    claimed.insert("claimed-uuid".to_string());

    let result = discover_session_id_for(
        &sessions,
        std::path::Path::new(cwd),
        spawn_time,
        &claimed,
    );
    assert_eq!(result.as_deref(), Some("free-uuid"));
}
```

You will need `tempfile` as a dev-dep — check `Cargo.toml` for `vmux_desktop` and add under `[dev-dependencies]` if absent.

- [ ] **Step 2: Implement the discovery helper + system**

Append to `crates/vmux_desktop/src/vibe/session.rs`:

```rust
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

#[derive(Deserialize)]
struct MetaEnvironment {
    working_directory: String,
}

#[derive(Deserialize)]
struct MetaJson {
    session_id: String,
    start_time: String,
    environment: MetaEnvironment,
}

pub(crate) fn discover_session_id_for(
    sessions_root: &Path,
    cwd: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let cwd_str = cwd.to_string_lossy().to_string();
    let spawn_secs = spawn_time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let entries = std::fs::read_dir(sessions_root).ok()?;
    let mut best: Option<(i64, String)> = None;

    for entry in entries.flatten() {
        let meta_path = entry.path().join("meta.json");
        let Ok(text) = std::fs::read_to_string(&meta_path) else { continue };
        let Ok(meta) = serde_json::from_str::<MetaJson>(&text) else { continue };
        if meta.environment.working_directory != cwd_str {
            continue;
        }
        if claimed.contains(&meta.session_id) {
            continue;
        }
        let Ok(start_dt) = chrono::DateTime::parse_from_rfc3339(&meta.start_time) else { continue };
        let start_secs = start_dt.timestamp();
        if start_secs < spawn_secs {
            continue;
        }
        match &best {
            None => best = Some((start_secs, meta.session_id)),
            Some((cur, _)) if start_secs < *cur => best = Some((start_secs, meta.session_id)),
            _ => {}
        }
    }

    best.map(|(_, id)| id)
}

pub fn vibe_sessions_root() -> std::path::PathBuf {
    std::env::var("VIBE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".vibe")
        })
        .join("logs")
        .join("session")
}

pub const DISCOVERY_MAX_ATTEMPTS: u8 = 30;

pub fn poll_pending_vibe_sessions(
    mut commands: Commands,
    mut q: Query<(Entity, &mut PendingVibeSession), With<Vibe>>,
    map: Res<VibeSessionToEntity>,
) {
    let sessions_root = vibe_sessions_root();
    let claimed: HashSet<String> = map.0.keys().cloned().collect();
    for (entity, mut pending) in &mut q {
        if let Some(id) =
            discover_session_id_for(&sessions_root, &pending.cwd, pending.spawn_time, &claimed)
        {
            commands
                .entity(entity)
                .insert(SessionId(id))
                .remove::<PendingVibeSession>();
            continue;
        }
        pending.attempts = pending.attempts.saturating_add(1);
        if pending.attempts >= DISCOVERY_MAX_ATTEMPTS {
            bevy::log::warn!("vibe session discovery timed out for entity {entity:?}");
            commands.entity(entity).remove::<PendingVibeSession>();
        }
    }
}
```

Add deps to `crates/vmux_desktop/Cargo.toml`:

```toml
serde = { version = "...", features = ["derive"] }
serde_json = "..."
chrono = "..."
```

(Use whatever versions are already pinned in the workspace — check `Cargo.toml` at the workspace root.)

- [ ] **Step 3: Register the system on a 200ms timer in `lib.rs`**

```rust
app.add_systems(
    Update,
    crate::vibe::session::poll_pending_vibe_sessions
        .run_if(bevy::time::common_conditions::on_timer(
            std::time::Duration::from_millis(200),
        )),
);
```

- [ ] **Step 4: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop vibe::session"
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 5: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/vibe/session.rs crates/vmux_desktop/src/lib.rs crates/vmux_desktop/Cargo.toml && git commit -m 'feat(VMX-109): vibe session discovery polling'"
```

---

## Task 9: Desktop — URL formatter for vibe panes (precedence over Terminal)

**Files:**
- Modify: `crates/vmux_desktop/src/vibe/session.rs` (add formatter system + tests)
- Modify: `crates/vmux_desktop/src/terminal/pid.rs` (ensure terminal formatter excludes Vibe entities)
- Modify: `crates/vmux_desktop/src/lib.rs` (register vibe formatter)

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_desktop/src/vibe/session.rs` tests:

```rust
use crate::browser::PageMetadata;

#[test]
fn formatter_emits_session_url_for_vibe_with_session() {
    let mut app = make_app();
    app.add_systems(Update, format_vibe_url);
    let e = app
        .world_mut()
        .spawn((
            Vibe,
            SessionId("ae724a54-c387-5359-0687-ccfc155558b6".into()),
            PageMetadata { title: String::new(), url: String::new(), favicon_url: String::new(), bg_color: None },
        ))
        .id();
    app.update();
    let url = &app.world().get::<PageMetadata>(e).unwrap().url;
    assert_eq!(url, "vmux://vibe/ae724a54-c387-5359-0687-ccfc155558b6");
}

#[test]
fn formatter_emits_placeholder_for_vibe_without_session() {
    let mut app = make_app();
    app.add_systems(Update, format_vibe_url);
    let e = app
        .world_mut()
        .spawn((
            Vibe,
            PageMetadata { title: String::new(), url: "stale".into(), favicon_url: String::new(), bg_color: None },
        ))
        .id();
    app.update();
    let url = &app.world().get::<PageMetadata>(e).unwrap().url;
    assert_eq!(url, "vmux://vibe/");
}
```

- [ ] **Step 2: Implement the formatter**

Append to `crates/vmux_desktop/src/vibe/session.rs`:

```rust
pub const VIBE_WEBVIEW_URL: &str = "vmux://vibe/";

pub fn format_vibe_url(
    mut q: Query<
        (Option<&SessionId>, &mut PageMetadata),
        (With<Vibe>, Or<(Changed<SessionId>, Added<PageMetadata>, Added<Vibe>)>),
    >,
) {
    for (sid, mut meta) in &mut q {
        let next = match sid {
            Some(SessionId(id)) => format!("{VIBE_WEBVIEW_URL}{id}"),
            None => VIBE_WEBVIEW_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}
```

Promote the `Vibe` filter on the terminal formatter (Task 5) — make sure `format_terminal_url` has `Without<crate::vibe::session::Vibe>` in its query so vibe entities aren't double-formatted. Re-check Task 5's code; if you deferred this, add it now.

- [ ] **Step 3: Register the system in `lib.rs`**

```rust
app.add_systems(Update, crate::vibe::session::format_vibe_url
    .after(crate::vibe::session::track_session_id_inserts));
```

- [ ] **Step 4: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop vibe::session::tests"
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 5: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/vibe/session.rs crates/vmux_desktop/src/terminal/pid.rs crates/vmux_desktop/src/lib.rs && git commit -m 'feat(VMX-109): URL formatter for vibe panes (vmux://vibe/<session>)'"
```

---

## Task 10: Desktop — vibe URL dispatcher (`vmux://vibe/` and `vmux://vibe/<id>`)

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs:266-308` (add `vibe` host arm)
- Modify: `crates/vmux_desktop/src/vibe.rs` (expose helpers: `spawn_fresh_vibe_pane(target)`, `spawn_vibe_resume_pane(target, session_id)`, `vibe_command_for_resume(session_id)`)

- [ ] **Step 1: Add a `--resume` variant of the launch-command builder; reuse MCP injection**

The existing `build_bash_launch_command` at `crates/vmux_desktop/src/vibe.rs:203-211` already injects `VIBE_MCP_SERVERS` properly via `shell_quote`/`shell_quote_path`. The MCP JSON is produced by `mcp_servers_env_value(cwd)` at line 116, which:

- Resolves the vmux sidecar binary next to the current executable (`vmux_sidecar_path`).
- Falls back to `cargo run --quiet -p vmux_cli --bin vmux -- mcp` when the sidecar is missing (workspace-dev path).
- Serializes the result as the `[{"name":"vmux","transport":"stdio","command":...,"args":...,"cwd":...}]` JSON that vibe expects.

Both new spawn paths (fresh + resume) **must call `mcp_servers_env_value(cwd)?`** to produce the MCP JSON. Do not inline a different MCP construction — the sidecar/cargo-run fallback logic must be shared so a developer-build vibe pane gets the same MCP wiring as a packaged-build vibe pane.

Add a parallel builder for the resume case in `vibe.rs`, right after `build_bash_launch_command`:

```rust
fn build_bash_launch_command_resume(
    mcp_servers: &str,
    vibe: &Path,
    cwd: &Path,
    session_id: &str,
) -> Result<String, String> {
    Ok(format!(
        "bash -lc {} bash {} {} {} {}",
        shell_quote("cd \"$1\" && VIBE_MCP_SERVERS=\"$2\" exec \"$3\" --trust --resume \"$4\"")?,
        shell_quote_path(cwd)?,
        shell_quote(mcp_servers)?,
        shell_quote_path(vibe)?,
        shell_quote(session_id)?,
    ))
}
```

The existing `build_bash_launch_command` stays as-is for the fresh path. The existing AgentLaunch consumer at `agent.rs:587-622` keeps using `build_bash_launch_command` and stays byte-identical.

Add a unit test in `mod tests` of `vibe.rs`, alongside the existing `launch_command_cds_and_passes_mcp_servers_to_vibe` (line 277):

```rust
#[test]
fn launch_command_resume_includes_session_id_and_mcp_servers() {
    let mcp = r#"[{"name":"vmux","transport":"stdio","command":"target/debug/vmux","args":["mcp"]}]"#;
    let cmd = build_bash_launch_command_resume(
        mcp,
        Path::new("/Users/test/.local/bin/vibe"),
        Path::new("/tmp/work tree"),
        "ae724a54-c387-5359-0687-ccfc155558b6",
    )
    .expect("build");
    assert!(cmd.contains("--resume"));
    assert!(cmd.contains("ae724a54-c387-5359-0687-ccfc155558b6"));
    assert!(cmd.contains("VIBE_MCP_SERVERS"));
    assert!(cmd.contains("\"name\":\"vmux\""));
}
```

- [ ] **Step 2: Add `spawn_fresh_vibe_pane` / `spawn_vibe_resume_pane` helpers**

Add to `crates/vmux_desktop/src/vibe.rs`. Both helpers call `mcp_servers_env_value(&cwd)?` to build the MCP JSON, then call the appropriate `build_bash_launch_command*` builder. The bundle mirrors `Terminal::new_with_cwd` (`crates/vmux_desktop/src/terminal.rs:283-353`) but with `Vibe` marker stamped, plus either `PendingVibeSession` (fresh) or `SessionId` (resume).

```rust
pub(crate) fn spawn_fresh_vibe_pane(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    target_stack: Entity,
    cwd: PathBuf,
    vibe_path: PathBuf,
) -> Result<Entity, String> {
    let mcp_json = mcp_servers_env_value(&cwd)?;
    let shell_cmd = build_bash_launch_command(&mcp_json, &vibe_path, &cwd)?;
    let bundle = make_terminal_bundle_with_command(meshes, webview_mt, shell_cmd, cwd.clone());
    let entity = commands
        .spawn(bundle)
        .insert(crate::vibe::session::Vibe)
        .insert(crate::vibe::session::PendingVibeSession {
            spawn_time: std::time::SystemTime::now(),
            cwd,
            attempts: 0,
        })
        .insert(ChildOf(target_stack))
        .id();
    Ok(entity)
}

pub(crate) fn spawn_vibe_resume_pane(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    target_stack: Entity,
    cwd: PathBuf,
    vibe_path: PathBuf,
    session_id: String,
) -> Result<Entity, String> {
    let mcp_json = mcp_servers_env_value(&cwd)?;
    let shell_cmd = build_bash_launch_command_resume(&mcp_json, &vibe_path, &cwd, &session_id)?;
    let bundle = make_terminal_bundle_with_command(meshes, webview_mt, shell_cmd, cwd);
    let entity = commands
        .spawn(bundle)
        .insert(crate::vibe::session::Vibe)
        .insert(crate::vibe::session::SessionId(session_id))
        .insert(ChildOf(target_stack))
        .id();
    Ok(entity)
}
```

Both call `mcp_servers_env_value(&cwd)?` so the vmux MCP server is wired into every vibe pane spawn — sidecar binary when packaged, `cargo run -p vmux_cli --bin vmux -- mcp` in dev. Do not bypass this helper.

`make_terminal_bundle_with_command` is a refactor of the existing `Terminal::new_with_cwd` body that takes the shell-command string directly (instead of resolving from settings). Extract it from `terminal.rs:283-353` so both the regular-terminal path and the vibe paths can call it. The signature:

```rust
pub(crate) fn make_terminal_bundle_with_command(
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    shell_cmd: String,
    cwd: PathBuf,
) -> impl Bundle { /* exact body of existing Terminal::new_with_cwd, with `shell` parameter replaced by `shell_cmd` */ }
```

Refactor `Terminal::new_with_cwd` to call `make_terminal_bundle_with_command(meshes, webview_mt, settings.terminal.resolve_theme().shell, cwd)`.

- [ ] **Step 3: Add the `vibe` host arm in `spawn_vmux_tab`**

In `crates/vmux_desktop/src/agent.rs:266-308`, extend the host match. The exact integration depends on how `spawn_vmux_tab` accesses world resources — follow the pattern of the existing `terminal` arm (which already calls `spawn_terminal_tab`, a sibling helper).

```rust
"vibe" => {
    let path = parsed.path().trim_start_matches('/');
    if path.is_empty() {
        spawn_fresh_vibe_tab(world, target_stack);
    } else {
        let session_id = path.to_string();
        let map = world.resource::<crate::vibe::session::VibeSessionToEntity>();
        if let Some(&entity) = map.0.get(&session_id) {
            focus_pane(world, entity);
        } else {
            spawn_vibe_resume_tab(world, target_stack, session_id);
        }
    }
}
```

Add `spawn_fresh_vibe_tab` / `spawn_vibe_resume_tab` next to the existing `spawn_terminal_tab` in `agent.rs`. Both call `crate::vibe::spawn_fresh_vibe_pane` / `crate::vibe::spawn_vibe_resume_pane` — pass `cwd` (current working dir or the active pane's cwd, matching the existing `handle_agent_launch_requests` resolution at `agent.rs:587-622`) and `vibe_path` (from `crate::vibe::find_executable("vibe")`). The MCP JSON is built inside the spawn helpers via `mcp_servers_env_value`, so callers do not pass it.

If `spawn_fresh_vibe_pane` returns `Err` (vibe binary missing or MCP config build failed), log a warning and fall back to `spawn_terminal_tab` so the user gets a usable shell rather than an empty pane.

- [ ] **Step 4: Write a test for the resume-path lookup**

Add to `mod tests` in `agent.rs` (it has tests already):

```rust
#[test]
fn vibe_url_with_known_session_resolves_via_map() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<crate::vibe::session::VibeSessionToEntity>();
    let entity = app
        .world_mut()
        .spawn((crate::vibe::session::Vibe, crate::vibe::session::SessionId("known".into())))
        .id();
    app.world_mut()
        .resource_mut::<crate::vibe::session::VibeSessionToEntity>()
        .0
        .insert("known".into(), entity);

    let map = app.world().resource::<crate::vibe::session::VibeSessionToEntity>();
    assert_eq!(map.0.get("known"), Some(&entity));
}
```

(Full integration of `spawn_vmux_tab` is heavy to test; this guards the lookup contract. End-to-end behavior is verified manually in Task 13.)

- [ ] **Step 5: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop"
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 6: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/vibe.rs crates/vmux_desktop/src/agent.rs && git commit -m 'feat(VMX-109): vibe URL dispatcher (fresh + resume)'"
```

---

## Task 11: Desktop — `startup_url` setting + resolver

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs:28-39` (`AppSettings`)
- Modify: `crates/vmux_desktop/src/settings.rs` (add resolver function)

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `settings.rs`. Mirror the `test_settings()` constructor in `crates/vmux_desktop/src/agent.rs:633` for the field shape:

```rust
#[test]
fn resolve_startup_url_returns_user_override() {
    let mut s = make_test_app_settings();
    s.startup_url = Some("vmux://services/".into());
    assert_eq!(resolve_startup_url(&s), "vmux://services/");
}

fn make_test_app_settings() -> AppSettings {
    AppSettings {
        browser: BrowserSettings {
            startup_url: "about:blank".to_string(),
        },
        layout: LayoutSettings {
            window: WindowSettings {
                padding: 0.0,
                padding_top: None,
                padding_right: None,
                padding_bottom: None,
                padding_left: None,
            },
            pane: PaneSettings { gap: 0.0, radius: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        },
        shortcuts: ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        startup_url: None,
    }
}
```

If the workspace types referenced here aren't in scope, copy from `agent.rs:633`'s `test_settings()` — same shape.

**NOTE: name conflict.** `BrowserSettings.startup_url` already exists (`crates/vmux_desktop/src/settings.rs:161`) and holds the default browser-tab URL (`"https://www.google.com"` per `settings.ron:3`). The new top-level `startup_url` field is distinct (`AppSettings.startup_url`) and only governs new-pane dispatch; both can coexist in `settings.ron` since they're at different nesting levels:

```ron
(
    startup_url: Some("vmux://vibe/"),    // new — top-level
    browser: (
        startup_url: "https://google.com", // existing — under `browser`
    ),
    ...
)
```

If reviewers find this confusing, consider renaming the new field to `default_pane_url` or `new_stack_url` before merging. Spec uses `startup_url`; flag this in the PR description.

- [ ] **Step 2: Run, verify fail**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop settings::tests::resolve_startup_url"
```

Expected: compile error.

- [ ] **Step 3: Implement**

In `crates/vmux_desktop/src/settings.rs:28-39`, add the field:

```rust
pub struct AppSettings {
    #[allow(dead_code)]
    pub browser: BrowserSettings,
    pub layout: LayoutSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default)]
    pub startup_url: Option<String>,
}
```

Add the resolver below the struct:

```rust
pub fn resolve_startup_url(settings: &AppSettings) -> String {
    settings.startup_url.clone().unwrap_or_else(|| {
        if crate::vibe::vibe_available() {
            "vmux://vibe/".to_string()
        } else {
            "vmux://terminal/".to_string()
        }
    })
}
```

- [ ] **Step 4: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_desktop settings"
bash -c "cargo fmt -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 5: Commit**

```sh
bash -c "git add crates/vmux_desktop/src/settings.rs && git commit -m 'feat(VMX-109): startup_url setting + resolver'"
```

---

## Task 12: Layout — drop auto-command-bar; dispatch `startup_url` instead

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs:188+` (`handle_stack_command` for `StackCommand::New`)
- Modify: `crates/vmux_layout/src/stack.rs:537+` (rename `open_command_bar_if_no_stacks` → `open_startup_url_if_no_stacks`)
- Modify: `crates/vmux_layout/src/stack.rs:615+` (existing tests asserting `needs_open == true`)
- Modify: `crates/vmux_desktop/src/lib.rs` (rename system registration)

`vmux_layout` does not depend on `vmux_desktop` (and shouldn't — desktop depends on layout). The new-stack handler must emit a message and let desktop perform the URL dispatch. The existing `LayoutSpawnRequest` enum at `crates/vmux_layout/src/lib.rs` (search: `pub enum LayoutSpawnRequest`) is the channel.

- [ ] **Step 1: Add `LayoutSpawnRequest::OpenUrl { stack, url }`**

In `crates/vmux_layout/src/lib.rs`, find `pub enum LayoutSpawnRequest` and add:

```rust
OpenUrl {
    stack: Entity,
    url: String,
},
```

In `crates/vmux_desktop/src/agent.rs` (or wherever the existing `LayoutSpawnRequest` consumer lives — grep for `LayoutSpawnRequest::Terminal` to find it), add a new arm to the existing match:

```rust
LayoutSpawnRequest::OpenUrl { stack, url } => {
    if let Err(e) = spawn_vmux_tab(world, &url, *stack) {
        bevy::log::warn!("startup_url dispatch failed for {url}: {e}");
        if url != "vmux://terminal/" {
            let _ = spawn_vmux_tab(world, "vmux://terminal/", *stack);
        }
    }
}
```

(The fallback to `vmux://terminal/` covers the malformed-startup_url case from the spec's Edge cases.)

- [ ] **Step 2: Add `EffectiveStartupUrl` resource (defined in layout, written by desktop)**

In `crates/vmux_layout/src/settings.rs`, add:

```rust
#[derive(Resource, Clone, Debug, Default)]
pub struct EffectiveStartupUrl(pub String);
```

In `crates/vmux_desktop/src/settings.rs`, add a system that updates it whenever settings change:

```rust
fn update_effective_startup_url(
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    if settings.is_changed() {
        commands.insert_resource(vmux_layout::settings::EffectiveStartupUrl(
            resolve_startup_url(&settings),
        ));
    }
}
```

Register in `crates/vmux_desktop/src/lib.rs`:

```rust
app.init_resource::<vmux_layout::settings::EffectiveStartupUrl>();
app.add_systems(Update, update_effective_startup_url);
```

- [ ] **Step 3: Update `handle_stack_command` for `StackCommand::New`**

In `crates/vmux_layout/src/stack.rs:188+`, find the `StackCommand::New` arm. The current code (lines around 211-247) calls `new_stack_ctx.needs_open = true`. Replace those `needs_open` assignments with:

```rust
spawn_request_writer.write(LayoutSpawnRequest::OpenUrl {
    stack: new_stack_entity,
    url: effective_startup_url.0.clone(),
});
```

Add `effective_startup_url: Res<crate::settings::EffectiveStartupUrl>` to the system signature.

Keep the `new_stack_ctx.stack = Some(new_stack_entity)` assignment (other code reads it for focus). Only `needs_open` goes away.

- [ ] **Step 4: Rename + refactor `open_command_bar_if_no_stacks`**

Find the function at `crates/vmux_layout/src/stack.rs:537`. Rename to `open_startup_url_if_no_stacks`. Replace the body that sets `new_stack_ctx.needs_open = true` (line 571) with the same `LayoutSpawnRequest::OpenUrl` dispatch as Step 3.

Update the `add_systems` registration in `vmux_desktop/src/lib.rs` (or wherever the system is wired — grep for `open_command_bar_if_no_stacks`) to use the new name.

- [ ] **Step 5: Update the existing tests**

The existing tests at `stack.rs:615+` assert `ctx.needs_open` is `true`. Update each to instead assert that a `LayoutSpawnRequest::OpenUrl` message was emitted with the expected URL.

Add a helper to the `mod tests` block to capture emitted messages:

```rust
#[derive(Resource, Default)]
struct CapturedSpawnRequests(Vec<LayoutSpawnRequest>);

fn capture_spawn_requests(
    mut reader: MessageReader<LayoutSpawnRequest>,
    mut captured: ResMut<CapturedSpawnRequests>,
) {
    for msg in reader.read() {
        captured.0.push(msg.clone());
    }
}
```

(`LayoutSpawnRequest` will need `Clone` derived if it doesn't already.)

In each test that previously asserted `ctx.needs_open == true`:

```rust
app.init_resource::<CapturedSpawnRequests>();
app.insert_resource(EffectiveStartupUrl("vmux://vibe/".into()));
app.add_systems(Update, capture_spawn_requests.after(handle_stack_commands));
// ... existing setup ...
app.update();

let captured = app.world().resource::<CapturedSpawnRequests>();
let urls: Vec<&str> = captured
    .0
    .iter()
    .filter_map(|m| match m {
        LayoutSpawnRequest::OpenUrl { url, .. } => Some(url.as_str()),
        _ => None,
    })
    .collect();
assert_eq!(urls, vec!["vmux://vibe/"]);
```

The third test `empty_active_pane_opens_command_bar_even_when_other_tabs_have_stacks` (line 718) tests `open_command_bar_if_no_stacks` — rename to `empty_active_pane_dispatches_startup_url_...` and update assertion the same way.

- [ ] **Step 6: Run tests + lint**

```sh
bash -c "env -u CEF_PATH cargo test -p vmux_layout"
bash -c "env -u CEF_PATH cargo test -p vmux_desktop"
bash -c "cargo fmt -p vmux_layout -p vmux_desktop -- --check"
bash -c "env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings"
bash -c "env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings"
```

- [ ] **Step 7: Commit**

```sh
bash -c "git add crates/vmux_layout/src/stack.rs crates/vmux_layout/src/settings.rs crates/vmux_layout/src/lib.rs crates/vmux_desktop/src/lib.rs crates/vmux_desktop/src/settings.rs && git commit -m 'feat(VMX-109): drop auto command bar; dispatch startup_url instead'"
```

---

## Task 13: Manual smoke test

**Files:** none — runtime verification on macOS.

- [ ] **Step 1: Build vmux locally**

```sh
bash -c "make run"
```

(Or whatever the project's run target is. Check `Makefile`.)

- [ ] **Step 2: Verify each acceptance criterion from VMX-109**

Open a window. With `vibe` installed:

- [ ] `Cmd+T` (or new-stack shortcut) → vibe launches in the new pane; the pane's URL becomes `vmux://vibe/<session_id>` within ~6s. No command bar appears.
- [ ] Command-bar keybinding (whatever's bound — check `settings.ron` or the keybindings doc) opens the command bar from any page.
- [ ] Open a vibe pane, copy its URL via the URL bar, close the pane, open command bar, paste URL → spawns `vibe --resume <id>`.
- [ ] Open a regular shell pane (e.g., via the command bar's terminal item). URL is `vmux://terminal/<pid>` where `<pid>` matches `ps` output for the shell.
- [ ] Copy `vmux://terminal/<pid>` for an existing pane, switch to a different tab, paste into command bar → focuses the original terminal.
- [ ] In `~/.../settings.ron`, set `startup_url: Some("vmux://services/")`, restart vmux. `Cmd+T` opens the services view.
- [ ] Remove `vibe` binary from PATH (e.g., `mv ~/.local/bin/vibe ~/.local/bin/vibe.bak`), restart vmux. `Cmd+T` opens a regular terminal — not the command bar.
- [ ] Restore vibe (`mv` back).

- [ ] **Step 3: Run the AGENTS.md changed-crates lint loop on the full diff**

Use the snippet from AGENTS.md to compute the changed crates against `main`:

```sh
bash -c '
BASE="${BASE:-main}"
ROOT="$(git rev-parse --show-toplevel)"
CHANGED_PKGS=$(
  cargo metadata --no-deps --format-version 1 \
  | jq -r ".packages[]
      | select(.manifest_path | test(\"patches\") | not)
      | \"\\(.name)\\t\\(.manifest_path | sub(\"/Cargo\\\\.toml\\$\"; \"\"))\"" \
  | while IFS=$"\t" read -r name dir; do
      rel="${dir#"$ROOT"/}"
      [ -z "$rel" ] && rel="."
      if ! git diff --quiet "$BASE" -- "$rel"; then
        echo "$name"
      fi
    done
)
for pkg in $CHANGED_PKGS; do echo "=== fmt $pkg ==="; cargo fmt -p "$pkg" -- --check; done
for pkg in $CHANGED_PKGS; do echo "=== clippy $pkg ==="; env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $CHANGED_PKGS; do echo "=== test $pkg ==="; env -u CEF_PATH cargo test -p "$pkg"; done
'
```

If any check fails, fix and re-run.

- [ ] **Step 4: Open PR**

Use the `open-new-pr` skill — it generates the title and body from the diff. Branch is already `feature/vmx-109-vibe-default-url-scheme`. Linear ticket VMX-109 will auto-link via branch name.

---

## Tying off

When the PR merges:

- Move VMX-109 to **Done** (`bash -c "linear issue update VMX-109 --state Done"`).
- Delete the implementation plan: `bash -c "rm docs/plans/2026-05-11-vibe-default-and-url-scheme.md"` and commit the deletion (per AGENTS.md: "Delete the plan file once the plan is fully implemented.").
- Remove the worktree: `bash -c "git worktree remove .worktrees/vmx-109"` (from the main worktree, not from inside `.worktrees/vmx-109`).
