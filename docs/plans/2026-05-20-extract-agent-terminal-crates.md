# Extract `agent` and `terminal` Domain Crates Implementation Plan

> **For agentic workers:** Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish steps 7-8 of the VMX-122 domain-crate extraction by moving `vmux_desktop::terminal` into `vmux_terminal` and `vmux_desktop::agent` into `vmux_agent`, without introducing a `vmux_agent ↔ vmux_terminal` cycle.

**Architecture:** Two PRs.

1. **PR-B — Decouple.** No files move between crates. Instead, the leaf types each side imports from the other (`TerminalLaunch`, `Pid`, `ServiceClient`, `build_agent_launch`, `space_dir`, `valid_cwd`, …) are pushed into their final-home crates (`vmux_terminal` / `vmux_agent`) one at a time. After this PR, `desktop::terminal` and `desktop::agent` no longer call each other through `crate::*` — they both import from sibling crates instead. The few residual direct calls become `Message` boundaries.
2. **PR-C — Move.** Now that the two files only depend on `vmux_terminal::*` / `vmux_agent::*` / `vmux_settings::*` / `vmux_layout::*`, move their bodies wholesale. Tiny diff inside each `*.rs` (just `crate::` → `super::`), large diff in `desktop::lib.rs` (drop modules, drop plugin wiring), and update every other desktop file that used `crate::terminal::` / `crate::agent::` to use the sibling-crate path.

`command_bar.rs` extraction (spec step 6), `spaces`/`processes_monitor` decoupling, and `layout_response.rs` cleanup are out of scope here and will get their own plan (PR-D).

**Tech Stack:** Rust 2024, Bevy 0.18, single Cargo workspace. Test runner: `cargo test`. Lint: `cargo clippy -- -D warnings`. Format: `cargo fmt`.

---

## Pre-flight

Reference inventory from `git log cab7400` and live grep on `refactor-plugins` branch (commit `a54050f`):

**`vmux_desktop::agent` (1667 LOC) imports from `crate::terminal::`:**

| Symbol | Lines | Final home |
|--------|-------|------------|
| `launch::TerminalLaunch` | 52, 318, 329, 426, 1120, 1231 | `vmux_terminal::launch` |
| `launch::TerminalKind` | 431, 1125 | `vmux_terminal::launch` |
| `Terminal` (component) | 435 | `vmux_terminal::component` (also used by browser, command_bar, os_menu, persistence, background_lifecycle) |
| `pid::PidToEntity` | 656, 853 | `vmux_terminal::pid` |
| `pid::focus_pane_entity` | 673, 738 | `vmux_terminal::pid` |
| `pid::Pid` | 998, 1011 | `vmux_terminal::pid` |
| `ServiceClient` | 868 | `vmux_terminal::service_client` |
| `handle_terminal_send_requests`, `handle_run_shell_requests` | 1135–1136 (system `.add_systems` wiring) | systems stay where defined; wire-up moves with `TerminalPlugin` |

**`vmux_desktop::terminal` (2856 LOC) imports from `crate::agent::`:**

| Symbol | Lines | Final home |
|--------|-------|------------|
| `space_dir` | 281 | `vmux_agent::cwd` |
| `build_agent_launch` | 378 | `vmux_agent::launch` |
| `AgentCommandRequest`, `AgentQueryRequest` | 789, 790, 1054, 1062 | `vmux_agent::events` |
| `parse_terminal_target`, `active_terminal_for_tab` | 2213, 2218, 2258 | `vmux_agent::target` |
| `shell_command_input` | 2256 | `vmux_agent::shell_input` |
| `valid_cwd` | 2264 | `vmux_agent::cwd` |
| `spawn_terminal_tab` | 2266 | message-driven (`TerminalSpawnRequest` already exists per VMX-122 design) |

**Out-of-scope cross-deps (handled in PR-D plan):**

- `terminal.rs:272,356` → `crate::spaces::{ActiveSpace, SpacesView}`
- `terminal.rs:988` → `crate::processes_monitor::ServiceProcessList`
- `agent.rs:617,643` → `crate::spaces::SpacesView`, `crate::processes_monitor::ProcessesMonitor`
- `agent.rs:855` → `crate::spaces::ActiveSpace`

These four call sites will be left as `vmux_desktop::*` direct calls after PR-C. That means `vmux_terminal` and `vmux_agent` will gain a temporary `vmux_desktop` dev-dep? **No** — that creates a cycle. Instead, **PR-C will gate these four sites behind already-existing message types or leave the systems that touch `Spaces*`/`ProcessesMonitor` *behind* in `vmux_desktop` as thin wrappers that listen for messages emitted by `vmux_terminal` / `vmux_agent`.** See PR-C Task 7.

---

## PR-B — Decouple agent ↔ terminal

**Branch:** `refactor/agent-terminal-decouple` (worktree `.worktrees/decouple-agent-terminal`).

**File structure after PR-B:**

```
crates/
├── vmux_terminal/
│   └── src/
│       ├── lib.rs            (re-exports new modules)
│       ├── plugin.rs         (existing — gains TerminalLaunch type registration)
│       ├── event.rs          (existing)
│       ├── render_model.rs   (existing)
│       ├── launch.rs         (NEW — TerminalLaunch, TerminalKind moved here)
│       ├── pid.rs            (NEW — Pid, PidToEntity, focus_pane_entity)
│       ├── service_client.rs (NEW — ServiceClient resource + wake helpers)
│       └── component.rs      (NEW — Terminal marker component)
├── vmux_agent/
│   └── src/
│       ├── lib.rs            (re-exports new modules)
│       ├── plugin.rs         (existing — gains nothing new here, PR-C wires AgentPlugin)
│       ├── cwd.rs            (NEW — space_dir, valid_cwd, default_space_dir)
│       ├── launch.rs         (NEW — build_agent_launch, AppAgent helpers)
│       ├── events.rs         (existing? merge — AgentCommandRequest, AgentQueryRequest)
│       ├── target.rs         (NEW — parse_terminal_target, active_terminal_for_tab, parse_pane_target)
│       └── shell_input.rs    (NEW — shell_command_input helper)
└── vmux_desktop/
    └── src/
        ├── terminal.rs       (still here; imports flip to vmux_terminal::* / vmux_agent::*)
        ├── agent.rs          (still here; imports flip to vmux_terminal::* / vmux_agent::*)
        └── ...
```

After PR-B: `grep -n "crate::terminal\|crate::agent" crates/vmux_desktop/src/{terminal,agent}.rs` returns **zero** matches.

---

### Task 1: Move `TerminalLaunch` + `TerminalKind` into `vmux_terminal::launch`

**Files:**
- Create: `crates/vmux_terminal/src/launch.rs`
- Modify: `crates/vmux_terminal/src/lib.rs` (add `pub mod launch;` and re-exports)
- Modify: `crates/vmux_terminal/Cargo.toml` (add `serde`, `bevy_reflect` features if needed for `#[derive(Reflect)]`)
- Modify: `crates/vmux_desktop/src/terminal.rs` — delete the inline `pub mod launch;` and its file, replace with `use vmux_terminal::launch::{TerminalLaunch, TerminalKind};`
- Delete: `crates/vmux_desktop/src/terminal/launch.rs`
- Modify: `crates/vmux_desktop/src/agent.rs` — replace `crate::terminal::launch::` with `vmux_terminal::launch::` (~6 sites)
- Modify: `crates/vmux_desktop/src/persistence.rs:77-78,195,222,458,461` — same replacement
- Modify: `crates/vmux_desktop/src/Cargo.toml` — confirm `vmux_terminal` is already a dep (it is)

- [ ] **Step 1: Read existing `crates/vmux_desktop/src/terminal/launch.rs`**

Run: `read_file path=crates/vmux_desktop/src/terminal/launch.rs`
Note the exact `TerminalLaunch` and `TerminalKind` definitions, derives, and any `impl` blocks. They must round-trip through `moonshine_save`, so `#[derive(Component, Reflect)]` plus `#[reflect(Component)]` must be preserved exactly.

- [ ] **Step 2: Create `crates/vmux_terminal/src/launch.rs`**

Verbatim copy of the file from step 1. Imports: only `bevy::prelude::*` and `serde::{Deserialize, Serialize}` should be needed — verify by reading. No `crate::` references; if any exist, resolve them by either (a) qualifying with `vmux_terminal::` (self) or (b) moving the referent here.

- [ ] **Step 3: Wire into `crates/vmux_terminal/src/lib.rs`**

Add at top of file (alphabetic):

```rust
pub mod launch;
```

Add re-export below existing re-exports:

```rust
pub use launch::{TerminalKind, TerminalLaunch};
```

- [ ] **Step 4: Register `Reflect` type in `TerminalPlugin::build`**

Edit `crates/vmux_terminal/src/plugin.rs`. Inside `impl Plugin for TerminalPlugin::build`, add (before the existing `app.add_message::<...>()` calls):

```rust
app.register_type::<launch::TerminalLaunch>()
    .register_type::<launch::TerminalKind>();
```

Then remove the equivalent registration from `crates/vmux_desktop/src/persistence.rs:77-78`. If `persistence.rs` was registering them, those lines now become unreachable (the plugin owns it).

- [ ] **Step 5: Delete `crates/vmux_desktop/src/terminal/launch.rs`**

Run: `git rm crates/vmux_desktop/src/terminal/launch.rs`

- [ ] **Step 6: Drop `pub(crate) mod launch;` from `crates/vmux_desktop/src/terminal.rs`**

Find line `pub(crate) mod launch;` near top (line 32) and delete it.

- [ ] **Step 7: Flip all `crate::terminal::launch::` callers**

In every file that referenced `crate::terminal::launch::TerminalLaunch` or `::TerminalKind`, replace with `vmux_terminal::launch::TerminalLaunch` / `::TerminalKind` (or add `use vmux_terminal::launch::{TerminalLaunch, TerminalKind};` to the top and bare-name them).

Files affected (from grep):
- `crates/vmux_desktop/src/terminal.rs` — 6 sites (lines 442, 447, 609, 996, 1001, 2158, 2163, 2171, 2292, 2297, 2688, 2804 — re-grep to confirm)
- `crates/vmux_desktop/src/agent.rs` — 6 sites (52, 318, 329, 426, 431, 1120, 1125, 1231)
- `crates/vmux_desktop/src/persistence.rs` — 5 sites (77, 78, 195, 222, 458, 461)

Use `grep -rn "crate::terminal::launch" crates/vmux_desktop/src/` to verify zero remaining after edits.

- [ ] **Step 8: Run changed-crate checks**

```bash
cd .worktrees/decouple-agent-terminal
for pkg in vmux_terminal vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_terminal vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_terminal vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all green. If `cargo test -p vmux_desktop` fails on a `moonshine_save` round-trip test, double-check that `TerminalLaunch` still has the same `#[type_path]` (it should, since serde + reflect path is based on the original module path — if it changed, add `#[type_path = "vmux_desktop::terminal::launch"]` as a temporary back-compat, with a TODO to drop after one release).

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor(terminal): move TerminalLaunch + TerminalKind into vmux_terminal"
```

---

### Task 2: Move `Terminal` marker component + `pid` submodule into `vmux_terminal`

**Files:**
- Create: `crates/vmux_terminal/src/component.rs` (Terminal, ProcessExited, PtyExited alias)
- Create: `crates/vmux_terminal/src/pid.rs` (Pid, PidToEntity, focus_pane_entity)
- Modify: `crates/vmux_terminal/src/lib.rs` (add modules + re-exports)
- Delete: `crates/vmux_desktop/src/terminal/pid.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs` — drop the `Terminal`/`ProcessExited` struct defs (lines ~44-50), drop `pub(crate) mod pid;`, replace with `use vmux_terminal::{Terminal, ProcessExited, PtyExited, pid::{Pid, PidToEntity, focus_pane_entity}};`
- Flip cross-callers (agent.rs, browser.rs, command_bar.rs, os_menu.rs, persistence.rs, background_lifecycle.rs, spaces.rs)

- [ ] **Step 1: Read source ranges**

`read_file path=crates/vmux_desktop/src/terminal.rs offset=42 limit=10` — capture `Terminal`, `ProcessExited`, `PtyExited` defs.

`read_file path=crates/vmux_desktop/src/terminal/pid.rs` — entire file.

- [ ] **Step 2: Create `crates/vmux_terminal/src/component.rs`**

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct Terminal;

#[derive(Component)]
pub struct ProcessExited;

pub type PtyExited = ProcessExited;
```

(Match exact derives/visibility from source — if `Terminal` had `Reflect` add it.)

- [ ] **Step 3: Create `crates/vmux_terminal/src/pid.rs`**

Copy contents of `crates/vmux_desktop/src/terminal/pid.rs`. Change line 1 from `use crate::terminal::Terminal;` to `use crate::component::Terminal;`.

- [ ] **Step 4: Wire into `vmux_terminal/src/lib.rs`**

```rust
pub mod component;
pub mod pid;

pub use component::{ProcessExited, PtyExited, Terminal};
```

- [ ] **Step 5: Drop defs from `crates/vmux_desktop/src/terminal.rs`**

Delete the three struct/type definitions (lines 42-50). Delete `pub(crate) mod pid;` (line 33). Add at top:

```rust
use vmux_terminal::{
    pid::{Pid, PidToEntity, focus_pane_entity},
    ProcessExited, PtyExited, Terminal,
};
```

- [ ] **Step 6: Delete `crates/vmux_desktop/src/terminal/pid.rs`**

```bash
git rm crates/vmux_desktop/src/terminal/pid.rs
rmdir crates/vmux_desktop/src/terminal 2>/dev/null || true
```

- [ ] **Step 7: Flip all `crate::terminal::Terminal` / `pid::` callers**

Grep first: `grep -rn "crate::terminal::\(Terminal\|ProcessExited\|PtyExited\|pid::\)" crates/vmux_desktop/src/`

Files affected: `browser.rs`, `command_bar.rs`, `os_menu.rs`, `persistence.rs`, `background_lifecycle.rs`, `spaces.rs`, `agent.rs`, `terminal.rs` self-refs.

For each, replace `crate::terminal::Terminal` → `vmux_terminal::Terminal`, `crate::terminal::pid::Pid` → `vmux_terminal::pid::Pid`, etc.

- [ ] **Step 8: Re-verify**

`grep -rn "crate::terminal::\(Terminal\|ProcessExited\|PtyExited\|pid\)" crates/vmux_desktop/src/` returns nothing.

- [ ] **Step 9: Run changed-crate checks**

```bash
for pkg in vmux_terminal vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_terminal vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_terminal vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 10: Commit**

```bash
git commit -am "refactor(terminal): move Terminal/ProcessExited components + pid module into vmux_terminal"
```

---

### Task 3: Move `ServiceClient` resource into `vmux_terminal::service_client`

**Files:**
- Create: `crates/vmux_terminal/src/service_client.rs`
- Modify: `crates/vmux_terminal/src/lib.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs` — drop the `ServiceClient` def, replace with import
- Flip cross-callers (agent.rs:868, browser.rs:1491, layout_response.rs:5)

- [ ] **Step 1: Locate `ServiceClient` def**

`grep -n "struct ServiceClient\|impl ServiceClient" crates/vmux_desktop/src/terminal.rs`

Read the full def and any `impl` block.

- [ ] **Step 2: Create `crates/vmux_terminal/src/service_client.rs`**

Copy verbatim. Imports likely include `bevy::prelude::Resource` and `vmux_service::client::{ServiceHandle, ServiceWake}`. Ensure `vmux_terminal/Cargo.toml` has `vmux_service` as a dep — check first with `grep "vmux_service" crates/vmux_terminal/Cargo.toml`. If missing, add `vmux_service = { path = "../vmux_service" }` under `[dependencies]`.

- [ ] **Step 3: Wire into `lib.rs`**

```rust
pub mod service_client;
pub use service_client::ServiceClient;
```

- [ ] **Step 4: Drop def from `terminal.rs`, add import**

```rust
use vmux_terminal::ServiceClient;
```

- [ ] **Step 5: Flip three external callers**

`agent.rs:868`, `browser.rs:1491`, `layout_response.rs:5`: change `crate::terminal::ServiceClient` → `vmux_terminal::ServiceClient`.

- [ ] **Step 6: Verify + checks + commit**

```bash
grep -rn "crate::terminal::ServiceClient" crates/vmux_desktop/src/
# expected: no matches

for pkg in vmux_terminal vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_terminal vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_terminal vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done

git commit -am "refactor(terminal): move ServiceClient resource into vmux_terminal"
```

---

### Task 4: Move `space_dir`, `valid_cwd`, `default_space_dir` into `vmux_agent::cwd`

**Files:**
- Create: `crates/vmux_agent/src/cwd.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent.rs` — drop the three fn defs, replace with re-export shim or direct import
- Flip cross-callers (terminal.rs:281, command_bar.rs:1489, anywhere else)

- [ ] **Step 1: Locate the three fns**

```bash
grep -n "fn space_dir\|fn valid_cwd\|fn default_space_dir" crates/vmux_desktop/src/agent.rs
```

Read each fn body (likely ~10-30 LOC each).

- [ ] **Step 2: Create `crates/vmux_agent/src/cwd.rs`**

Copy the three fn bodies. If they reference anything from `crate::agent::` siblings (helpers, constants), bring those along or change to `pub(crate)`. Verify imports — `std::path::{Path, PathBuf}` and possibly `vmux_layout::space::Space`.

- [ ] **Step 3: Wire into `vmux_agent/src/lib.rs`**

```rust
pub mod cwd;
pub use cwd::{default_space_dir, space_dir, valid_cwd};
```

- [ ] **Step 4: Drop fns from `desktop/agent.rs`**

Replace with `pub(crate) use vmux_agent::cwd::{default_space_dir, space_dir, valid_cwd};` so internal `agent.rs` callers still see them under the old name.

- [ ] **Step 5: Flip cross-callers**

`terminal.rs:281` and `command_bar.rs:1489` change `crate::agent::space_dir` / `default_space_dir` → `vmux_agent::cwd::space_dir` etc., or remove the `pub(crate) use` shim and update agent.rs callers too (cleaner — do that).

- [ ] **Step 6: Verify + checks + commit**

```bash
grep -rn "crate::agent::\(space_dir\|valid_cwd\|default_space_dir\)" crates/vmux_desktop/src/
# expected: no matches

for pkg in vmux_agent vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_agent vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_agent vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done

git commit -am "refactor(agent): move cwd helpers (space_dir/valid_cwd/default_space_dir) into vmux_agent"
```

---

### Task 5: Move `build_agent_launch` into `vmux_agent::launch`

**Files:**
- Create: `crates/vmux_agent/src/launch.rs` (or extend if exists)
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs:378`

- [ ] **Step 1: Read `fn build_agent_launch`**

`grep -n "fn build_agent_launch" crates/vmux_desktop/src/agent.rs` → line 318.

Read it. Its signature already returns `crate::terminal::launch::TerminalLaunch` — after Task 1 that's `vmux_terminal::launch::TerminalLaunch`.

- [ ] **Step 2: Add `vmux_terminal` dep to `vmux_agent`**

```bash
grep "vmux_terminal" crates/vmux_agent/Cargo.toml
```

If missing, add `vmux_terminal = { path = "../vmux_terminal" }` under `[dependencies]`. This creates `vmux_agent → vmux_terminal`. Confirm no reverse dep exists: `grep "vmux_agent" crates/vmux_terminal/Cargo.toml` → must be empty.

- [ ] **Step 3: Create `crates/vmux_agent/src/launch.rs`**

Move `build_agent_launch` here. Adjust imports — `AgentKind`, `AgentStrategies` etc. are siblings (`crate::AgentKind`, `crate::strategy::AgentStrategies`). `TerminalLaunch` becomes `vmux_terminal::launch::TerminalLaunch`.

- [ ] **Step 4: Wire into `vmux_agent/src/lib.rs`**

```rust
pub mod launch;
pub use launch::build_agent_launch;
```

- [ ] **Step 5: Drop fn from `desktop/agent.rs`, flip callers**

- `desktop/agent.rs` self-callers: change `build_agent_launch(...)` → `vmux_agent::build_agent_launch(...)` or add `use vmux_agent::build_agent_launch;` at top.
- `desktop/terminal.rs:378`: same.

- [ ] **Step 6: Verify + checks + commit**

```bash
grep -rn "crate::agent::build_agent_launch" crates/vmux_desktop/src/
# expected: no matches

for pkg in vmux_agent vmux_terminal vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_agent vmux_terminal vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_agent vmux_terminal vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done

git commit -am "refactor(agent): move build_agent_launch into vmux_agent"
```

---

### Task 6: Move `AgentCommandRequest`/`AgentQueryRequest` into `vmux_agent::events`

**Files:**
- Modify or create: `crates/vmux_agent/src/events.rs` (file already exists per `ls` output — extend it)
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent.rs` — drop defs (lines 29-39), wire registration via existing AgentPlugin
- Flip cross-callers (terminal.rs:789-790, 1054, 1062)

- [ ] **Step 1: Read existing `crates/vmux_agent/src/events.rs`**

Confirm it's a Bevy-events file. If yes, append the two `Message` types. If it conflicts (e.g. it's not events but something else), pick a new filename like `requests.rs`.

- [ ] **Step 2: Append `AgentCommandRequest` and `AgentQueryRequest`**

```rust
use bevy::prelude::Message;
use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentQuery, AgentRequestId};

#[derive(Message)]
pub struct AgentCommandRequest {
    pub request_id: AgentRequestId,
    pub command: ServiceAgentCommand,
}

#[derive(Message)]
pub struct AgentQueryRequest {
    pub request_id: AgentRequestId,
    pub query: AgentQuery,
}
```

Add `vmux_service` dep to `vmux_agent/Cargo.toml` if missing.

- [ ] **Step 3: Register in `vmux_agent` plugin or expect desktop to register**

Until PR-C moves `AgentPlugin` to `vmux_agent`, leave registration in `desktop::agent::AgentPlugin::build`. Just expose the types.

Update `vmux_agent/src/lib.rs`:

```rust
pub use events::{AgentCommandRequest, AgentQueryRequest};
```

- [ ] **Step 4: Drop defs from `desktop/agent.rs`**

Delete lines 29-39. Add `use vmux_agent::events::{AgentCommandRequest, AgentQueryRequest};`.

- [ ] **Step 5: Flip cross-callers in `desktop/terminal.rs`**

Lines 789, 790, 1054, 1062 — change `crate::agent::AgentCommandRequest` → `vmux_agent::AgentCommandRequest` (same for Query).

- [ ] **Step 6: Verify + checks + commit**

```bash
grep -rn "crate::agent::Agent\(Command\|Query\)Request" crates/vmux_desktop/src/
# expected: no matches

for pkg in vmux_agent vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_agent vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_agent vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done

git commit -am "refactor(agent): move AgentCommand/QueryRequest messages into vmux_agent"
```

---

### Task 7: Move `parse_terminal_target`, `active_terminal_for_tab`, `parse_pane_target` into `vmux_agent::target`

**Files:**
- Create: `crates/vmux_agent/src/target.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent.rs` — drop defs
- Flip cross-callers (terminal.rs:2213, 2218, 2258; browser.rs:1505, 1546, 1559)

- [ ] **Step 1: Locate fns**

```bash
grep -n "fn parse_terminal_target\|fn active_terminal_for_tab\|fn parse_pane_target\|fn active_webview_for_tab" crates/vmux_desktop/src/agent.rs
```

Read each. Their signatures take `Query<...>` and entity IDs — pure ECS helpers.

- [ ] **Step 2: Create `crates/vmux_agent/src/target.rs`**

Copy verbatim. Update imports — `Terminal` is now `vmux_terminal::Terminal`, `Browser` is `vmux_layout::Browser`. Add `vmux_terminal`/`vmux_layout` deps to Cargo.toml (likely already present after Task 5).

- [ ] **Step 3: Wire into lib + drop from desktop + flip callers**

```rust
// vmux_agent/src/lib.rs
pub mod target;
pub use target::{active_terminal_for_tab, active_webview_for_tab, parse_pane_target, parse_terminal_target};
```

`grep -rn "crate::agent::\(parse_terminal_target\|active_terminal_for_tab\|parse_pane_target\|active_webview_for_tab\)" crates/vmux_desktop/src/` → flip to `vmux_agent::*`.

- [ ] **Step 4: Verify + checks + commit**

```bash
for pkg in vmux_agent vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_agent vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_agent vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done

git commit -am "refactor(agent): move pane/terminal target helpers into vmux_agent"
```

---

### Task 8: Move `shell_command_input` into `vmux_agent::shell_input`

**Files:**
- Create: `crates/vmux_agent/src/shell_input.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent.rs` — drop def
- Flip caller (terminal.rs:2256)

- [ ] **Step 1: Locate**

```bash
grep -n "fn shell_command_input" crates/vmux_desktop/src/agent.rs
```

- [ ] **Step 2: Copy, wire, drop, flip — same pattern as Task 4-7**

- [ ] **Step 3: Verify + checks + commit**

```bash
git commit -am "refactor(agent): move shell_command_input helper into vmux_agent"
```

---

### Task 9: Replace residual `spawn_terminal_tab` direct call with existing `LayoutSpawnRequest`

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs:2266`

- [ ] **Step 1: Inspect call site**

Read terminal.rs:2240-2280. Understand what `spawn_terminal_tab(...)` does — likely spawns a terminal pane bundle at a target.

- [ ] **Step 2: Check if `LayoutSpawnRequest` already covers this**

```bash
grep -n "LayoutSpawnRequest\|TerminalSpawnRequest" crates/vmux_layout/src/*.rs crates/vmux_terminal/src/*.rs
```

Per VMX-122 design, `TerminalSendRequest`/`RunShellRequest` are scaffolded but a `TerminalSpawnRequest` may or may not exist. If a terminal-spawn message exists, write that instead of calling the helper. If not, the simplest decoupling is to keep `spawn_terminal_tab` callable via `vmux_agent::spawn_terminal_tab` (move it in Task 5-7 fashion).

**Decision per executor:** if `LayoutSpawnRequest::OpenUrl` with `vmux_layout::event::TERMINAL_WEBVIEW_URL` already triggers a terminal spawn in `desktop::terminal`, replace this direct call with a `writer.write(LayoutSpawnRequest::OpenUrl { url: TERMINAL_WEBVIEW_URL.into(), target })`. Otherwise, move `spawn_terminal_tab` into `vmux_agent::launch` alongside `build_agent_launch`.

- [ ] **Step 3: Verify + checks + commit**

```bash
grep -rn "crate::agent::" crates/vmux_desktop/src/terminal.rs
grep -rn "crate::terminal::" crates/vmux_desktop/src/agent.rs
# both: expected zero matches

for pkg in vmux_agent vmux_terminal vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_agent vmux_terminal vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_agent vmux_terminal vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done

git commit -am "refactor(terminal): drop direct spawn_terminal_tab call; route through layout message"
```

---

### Task 10: PR-B open

- [ ] **Step 1: Final cross-check**

```bash
grep -rn "crate::terminal\|crate::agent" crates/vmux_desktop/src/{terminal,agent}.rs
# expected: zero matches (other than self-references to submodules that are now empty)
```

- [ ] **Step 2: Run full per-changed-crate sweep**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 3: Push + open PR-B**

```bash
git push -u origin refactor/agent-terminal-decouple
```

PR title: `Decouple agent ↔ terminal direct calls (prerequisite for VMX-122 steps 7-8)`.

PR description bullets: "Pushes 9 leaf types/functions into their final-home crates, no file moves between crates yet, no behavior change. Sets up PR-C to drop `desktop/{terminal,agent}.rs` wholesale into `vmux_terminal` / `vmux_agent`."

---

## PR-C — Move file bodies

**Branch:** `refactor/agent-terminal-move` (depends on PR-B merged or rebased on top).

After PR-B, `desktop/terminal.rs` and `desktop/agent.rs` only reference:

- `vmux_terminal::*` (now contains everything that was in `crate::terminal::launch/pid/service_client/component`)
- `vmux_agent::*` (now contains cwd/launch/events/target/shell_input helpers)
- `vmux_settings::AppSettings`
- `vmux_layout::*`
- `vmux_service::*`
- `vmux_history`, `vmux_core`, `vmux_webview_app`
- `crate::browser::Browser` *(re-export of `vmux_layout::Browser` — easy)*
- `crate::command::*` *(stays in desktop until PR-D)*
- `crate::processes_monitor::*` *(stays in desktop — only used in 1 site per file)*
- `crate::spaces::*` *(stays in desktop — used in 3 sites total)*

**Strategy for the remaining desktop deps:**

- `crate::browser::Browser` → re-export via `vmux_layout::Browser` directly. Trivial.
- `crate::command::*` → these are message types declared in `vmux_command::command`. Re-route imports to `vmux_command`.
- `crate::processes_monitor::*` and `crate::spaces::*` → **leave the system functions that touch them inside `vmux_desktop`**. Specifically: any free `fn` in `terminal.rs` or `agent.rs` whose body needs `ActiveSpace`/`SpacesView`/`ProcessesMonitor` stays in `vmux_desktop::shell_systems` (new file), and the corresponding `app.add_systems(...)` line moves with it. Most of these are spawn helpers — they can stay behind as thin wrappers that read messages emitted by the new `vmux_terminal::TerminalPlugin` / `vmux_agent::AgentPlugin`.

This is the largest design choice in PR-C and needs an extra read-through before splitting.

### Task 1: Inventory residual desktop-only call sites

- [ ] **Step 1: List every system fn in `agent.rs` / `terminal.rs` that touches `spaces` / `processes_monitor`**

```bash
grep -B5 "crate::\(spaces\|processes_monitor\)::" crates/vmux_desktop/src/agent.rs crates/vmux_desktop/src/terminal.rs
```

For each match, note the surrounding `fn` name. Build a list — likely 4-6 systems total.

- [ ] **Step 2: For each, classify**

(a) Pure spawn — convert to `Message` emitter (system stays behind in `vmux_desktop`, listens on the new message, calls `SpacesView::new` itself).
(b) Pure read — change `Res<ActiveSpace>` to a message-carried value.
(c) Mixed — split fn into two halves.

Document the classification inline in this plan (extend Task 2-N) before coding.

### Task 2: For each system identified, split or message-ify

(Plan section deferred: the inventory in Task 1 produces the per-system task list. Each follows the same TDD-style decouple pattern shown in PR-B Tasks 4-8.)

### Task 3: Move `crates/vmux_desktop/src/agent.rs` → `crates/vmux_agent/src/desktop_systems.rs`

- [ ] **Step 1: `git mv crates/vmux_desktop/src/agent.rs crates/vmux_agent/src/desktop_systems.rs`** (rename for clarity since the file mixes UI + ECS; can be renamed to just `runtime.rs` if cleaner.)

- [ ] **Step 2: Edit moved file: change `use crate::` to `use vmux_desktop::` for residual desktop deps (browser, command, processes_monitor, spaces if any survived Task 2)**

This creates a `vmux_agent → vmux_desktop` edge. **Forbidden — cycle.** If Task 2 didn't eliminate all such deps, do **not** proceed with this step; loop back and finish Task 2.

- [ ] **Step 3: Drop `pub(crate) mod agent;` from `vmux_desktop/src/lib.rs`. Drop `app.add_plugins(AgentPlugin);` since `vmux_agent::AgentPlugin` now exposes it.**

- [ ] **Step 4: Wire `AgentPlugin` from `vmux_agent::lib.rs`**

- [ ] **Step 5: Run checks + commit**

### Task 4: Same for `terminal.rs` → `vmux_terminal::runtime.rs`

(Mirror of Task 3 — see PR-B Task 1 pattern.)

### Task 5: Drop `vmux_desktop::layout_response.rs`

Per spec step 9 — only uses `crate::terminal::ServiceClient` which is now `vmux_terminal::ServiceClient`. Decide:

- If the file is small (~50 LOC) and only does message-relay between `vmux_service` and `vmux_terminal`, fold it into `vmux_terminal::service_client`.
- Otherwise, move it to `vmux_terminal::layout_response`.

### Task 6: PR-C open

Same checks as PR-B Task 10.

---

## Risks & Open Questions

1. **`moonshine_save` `#[type_path]` for `TerminalLaunch`.** Changing the module path may invalidate persisted layouts on user disks. Audit: run the app once with an old layout file, observe whether load still works. If not, add an explicit `#[type_path = "vmux_desktop::terminal::launch"]` annotation on the moved type for one release cycle.

2. **`AgentPlugin` startup order.** `AgentPlugin` currently runs after `TerminalPlugin` because they're both added in `vmux_desktop::lib.rs` in that order. After PR-C, both come from sibling crates; the order in `desktop::lib.rs::build` must remain `TerminalPlugin → AgentPlugin` to preserve current `Startup` ordering (or add an explicit `.before(...)` if the dep is real).

3. **`vmux_agent → vmux_terminal` dep edge.** Created in PR-B Task 5. Verify the reverse (`vmux_terminal → vmux_agent`) stays empty after every commit by `grep -n "vmux_agent" crates/vmux_terminal/Cargo.toml`. If any cross-call sneaks in from `vmux_terminal` back into `vmux_agent`, route it through a new `Message` instead.

4. **`command_bar.rs` extraction** is still blocked after this plan; it depends on `crate::spaces`, `crate::processes_monitor`, `crate::browser`. Separate PR-D plan needed once PR-C lands.

5. **Behavior change risk.** None expected — this is pure code motion. Smoke test after each commit by `cargo run -p vmux_desktop` and exercising: open terminal, spawn agent (vibe/claude/codex), open settings, open command bar.

---

## Definition of Done

PR-B:
- `grep -rn "crate::\(terminal\|agent\)::" crates/vmux_desktop/src/{terminal,agent}.rs` returns zero matches.
- All changed-crate checks green.
- App smoke-tested manually.

PR-C:
- `crates/vmux_desktop/src/agent.rs` and `crates/vmux_desktop/src/terminal.rs` no longer exist.
- `vmux_desktop::lib.rs` no longer adds `AgentPlugin` / `TerminalPlugin` (or only registers them by re-export from sibling crates).
- `make lint` and `make test` pass workspace-wide.
- VMX-122 follow-up Linear issue updated with the remaining `command_bar` extraction (PR-D plan to be written).
