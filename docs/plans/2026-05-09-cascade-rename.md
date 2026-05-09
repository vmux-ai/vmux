# Cascade Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cascade-rename `Session → Space → Tab → Stack` across the codebase, rework keybindings (`cmd+t`, `cmd+n`, `cmd+[`/`]`), and add a `cmd+shift+s` zen toggle that hides all chrome.

**Architecture:** Six sequential phases, each a single bisectable commit:
1. `Tab → Stack` (leaf, smallest blast radius)
2. `Space → Tab`
3. `Session → Space` (includes `vmux_session` crate, `vmux://sessions/` URL, `vmux_desktop/sessions.rs` file)
4. Old `~/.vmux/` `.ron` files moved to `.bak` on first launch
5. Keybinding rework (cmd+t/n/[/], remove `Ctrl+g` Space chord, remove `super+n` NewWindow accel)
6. `ZenCommand::Toggle`, delete individual chrome toggles, default all chrome visible

**Tech Stack:** Rust, Bevy 0.18 ECS, CEF (Chromium Embedded Framework via `bevy_cef`), `moonshine-save` (RON serialization), Dioxus (WASM UI), `vmux_macro` proc-macros (`DefaultShortcuts`, `OsMenu`, `CommandBar`).

**Spec:** `docs/specs/2026-05-09-cascade-rename-design.md`
**Linear:** [VMX-108](https://linear.app/vmux/issue/VMX-108/cascade-rename-sessionspacetab-spacetabstack-zen-toggle)

---

## Conventions

- All paths are relative to the worktree root: `/Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-108/`
- After every phase, run `make lint && make test` and only commit if both pass.
- For mechanical renames spanning many files, use `Grep` to find all callsites, then `Edit replace_all` per file.
- Preserve commit message format: `refactor(VMX-108): <phase summary>`.

---

## File Structure (after the rename)

| Path | Status |
|------|--------|
| `crates/vmux_layout/src/space.rs` + `space/` | **deleted** (was `Space` struct) |
| `crates/vmux_layout/src/tab.rs` + `tab/` | **renamed from** `space.rs` (now holds `Tab` struct, was `Space`) |
| `crates/vmux_layout/src/stack.rs` + `stack/` | **renamed from** `tab.rs` (now holds `Stack` struct, was `Tab`) |
| `crates/vmux_layout/src/zen.rs` | **new** (`ZenMode` resource + `ZenCommand` handler) |
| `crates/vmux_layout/src/header.rs` | modified (toggle handler deleted) |
| `crates/vmux_layout/src/footer.rs` | modified (toggle handler deleted) |
| `crates/vmux_layout/src/side_sheet.rs` | modified (toggle handlers deleted) |
| `crates/vmux_layout/src/window.rs` | modified (default Open on Header + Footer + all SideSheets) |
| `crates/vmux_layout/src/lib.rs` | modified (`HeaderState` / `SideSheetState` deleted, type_path tests deleted, plugin list adds `ZenPlugin`) |
| `crates/vmux_session/` | **renamed to** `crates/vmux_space/` |
| `crates/vmux_session/src/event.rs` (= `crates/vmux_space/src/event.rs`) | modified (URL + struct names) |
| `crates/vmux_session/src/model.rs` (= `crates/vmux_space/src/model.rs`) | modified (`SessionRecord → SpaceRecord` etc.) |
| `crates/vmux_desktop/src/sessions.rs` | **renamed to** `crates/vmux_desktop/src/spaces.rs` |
| `crates/vmux_desktop/src/persistence.rs` | modified (SceneFilter allow-list, startup `.bak` migration) |
| `crates/vmux_command/src/command.rs` | modified (type renames, accel/shortcut attrs reworked, individual chrome toggles deleted) |
| `crates/vmux_command/src/results.rs` | modified (`SESSIONS_PAGE_URL` → `SPACES_PAGE_URL`) |
| `crates/vmux_desktop/src/shortcut.rs` | modified (programmatic chord overrides updated) |

---

## Phase 0: Pre-flight

### Task 0.1: Verify clean baseline

**Files:** none

- [ ] **Step 1: Confirm clean working tree**

```bash
git status
```

Expected: `nothing to commit, working tree clean` (other than the spec doc, which is already committed). If dirty, stop and resolve.

- [ ] **Step 2: Run baseline lint**

```bash
make lint
```

Expected: exit 0. If it fails on `main` HEAD, stop — pre-existing breakage must be triaged first.

- [ ] **Step 3: Run baseline tests**

```bash
make test
```

Expected: exit 0.

- [ ] **Step 4: Record baseline test count**

Note the test count from the output (e.g. "test result: ok. 247 passed; 0 failed"). Store mentally so we can compare after each phase — drift in count is expected (we add/remove tests), but unexplained failures are not.

### Task 0.2: Delete legacy compat tests

The two tests in `crates/vmux_layout/src/lib.rs` that pin `#[type_path]` to old strings (`vmux_desktop::layout::space::Space` etc.) and round-trip embedded RON literals are pure backward-compat guarantees for old session files. Per spec, we are abandoning old `.ron` files; these tests block the rename and serve no purpose afterward.

**Files:**
- Modify: `crates/vmux_layout/src/lib.rs:192-313`

- [ ] **Step 1: Read the test block**

Read lines 180-320 of `crates/vmux_layout/src/lib.rs` to confirm the two test functions and the `#[cfg(test)] mod tests` boundary.

- [ ] **Step 2: Delete both test functions**

Remove the entire `persisted_type_paths_match_legacy_desktop_sessions` and `legacy_desktop_session_component_names_deserialize` tests. Keep the surrounding `#[cfg(test)] mod tests { ... }` block and any other tests in it.

- [ ] **Step 3: Verify file compiles**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_layout"
```

Expected: success.

- [ ] **Step 4: Run vmux_layout tests**

```bash
bash -c "env -u CEF_PATH cargo test -p vmux_layout"
```

Expected: success (with the two deleted tests now absent).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/lib.rs
git -c commit.gpgsign=false commit -m "refactor(VMX-108): delete legacy session.ron compat tests"
```

---

## Phase 1: Tab → Stack rename

Today's `Tab` (browser/terminal page inside a Pane) becomes `Stack`. Smallest blast radius — leaf of the hierarchy.

**Affected types:**
- `Tab` → `Stack` (struct in `vmux_layout/src/tab.rs`)
- `FocusedTab` → `FocusedStack` (resource)
- `ComputeFocusSet` → keep (generic name)
- `PendingTabClose` → `PendingStackClose`
- `CloseConfirmed` → keep (generic name)
- `TabCommandSet` → `StackCommandSet`
- `TabPlugin` → `StackPlugin`
- `NewTabContext` → `NewStackContext`
- `TabCommand` enum → `StackCommand`
- Helper fns: `active_tab_in_pane → active_stack_in_pane`, `first_tab_in_pane → first_stack_in_pane`, `collect_leaf_panes` keeps name

**Affected files (from grep):** all files matching `Tab` in `crates/`. Use `Grep pattern="\\bTab\\b" path="crates/"` to enumerate.

### Task 1.1: Rename files

**Files:**
- Move: `crates/vmux_layout/src/tab.rs` → `crates/vmux_layout/src/stack.rs`
- Move: `crates/vmux_layout/src/tab/` → `crates/vmux_layout/src/stack/` (if directory exists; check with `ls crates/vmux_layout/src/tab` first)

- [ ] **Step 1: Check whether `tab/` subdirectory exists**

```bash
bash -c "ls crates/vmux_layout/src/tab/ 2>/dev/null || echo 'no subdir'"
```

- [ ] **Step 2: Rename file (and subdir if present)**

```bash
bash -c "git mv crates/vmux_layout/src/tab.rs crates/vmux_layout/src/stack.rs"
```

If `tab/` subdir exists:
```bash
bash -c "git mv crates/vmux_layout/src/tab crates/vmux_layout/src/stack"
```

- [ ] **Step 3: Update `mod tab;` → `mod stack;` and `pub use tab::*;` → `pub use stack::*;` in `crates/vmux_layout/src/lib.rs`**

Use Grep to find all `mod tab` declarations:

```bash
bash -c "grep -n 'mod tab' crates/vmux_layout/src/lib.rs"
```

Then Edit each to `mod stack` / `pub use stack::*`. Note: only rename the `Tab` module, not unrelated identifiers.

### Task 1.2: Rename types in `stack.rs` (formerly `tab.rs`)

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs` (the renamed file)

- [ ] **Step 1: Update `#[type_path]` attribute**

Change:
```rust
#[type_path = "vmux_desktop::layout::tab"]
```
to:
```rust
#[type_path = "vmux_desktop::layout::stack"]
```

(The `vmux_desktop::layout::` prefix is kept for moonshine-save compat — only the leaf module name changes.)

- [ ] **Step 2: Rename struct + impls + types within the file**

Use `Edit replace_all`:
- `pub struct Tab` → `pub struct Stack`
- `pub struct FocusedTab` → `pub struct FocusedStack`
- `pub struct PendingTabClose` → `pub struct PendingStackClose`
- `pub struct TabCommandSet` → `pub struct StackCommandSet`
- `pub struct TabPlugin` → `pub struct StackPlugin`
- `impl Plugin for TabPlugin` → `impl Plugin for StackPlugin`
- `Resource(FocusedTab)` references → `FocusedStack`
- All `Tab` type uses inside fn signatures, queries, `With<Tab>` → `With<Stack>`
- `TabCommand::` → `StackCommand::`
- `active_tab_in_pane` → `active_stack_in_pane`
- `first_tab_in_pane` → `first_stack_in_pane`
- `pub mod tab` references inside the file (none expected)

Be careful: `TabCommandSet` shares the `Tab` prefix with `TabCommand`; rename it to `StackCommandSet`. The `ComputeFocusSet` and `CloseConfirmed` names stay (generic-sounding).

- [ ] **Step 3: Verify file compiles in isolation**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_layout 2>&1 | head -100"
```

Expect many errors elsewhere referencing the old names. That's fine — the next task fixes them.

### Task 1.3: Update all references in vmux_layout

**Files:**
- Modify: every `.rs` file under `crates/vmux_layout/src/` that imports or names `Tab`, `FocusedTab`, `TabCommand`, etc.

- [ ] **Step 1: Enumerate references**

```bash
bash -c "grep -rln '\\b\\(Tab\\|FocusedTab\\|TabCommand\\|TabPlugin\\|TabCommandSet\\|PendingTabClose\\|NewTabContext\\|active_tab_in_pane\\|first_tab_in_pane\\)\\b' crates/vmux_layout/src/"
```

- [ ] **Step 2: For each file in the list, apply renames via Edit replace_all**

For each file (one Edit per identifier per file is fine, or multiple replace_all calls):
- `Tab` → `Stack` (inside `vmux_layout` crate; be careful — `TabPlugin` becomes `StackPlugin`, etc.)
- `FocusedTab` → `FocusedStack`
- `TabCommand` → `StackCommand`
- `TabPlugin` → `StackPlugin`
- `TabCommandSet` → `StackCommandSet`
- `PendingTabClose` → `PendingStackClose`
- `NewTabContext` → `NewStackContext`
- `active_tab_in_pane` → `active_stack_in_pane`
- `first_tab_in_pane` → `first_stack_in_pane`
- `use crate::tab::` → `use crate::stack::`
- `crate::tab::` → `crate::stack::`

Watch out for: import lines like `use crate::tab::{Tab, FocusedTab, ...}` — both the module path and identifiers change.

- [ ] **Step 3: Update plugin registration in `lib.rs`**

```bash
bash -c "grep -n 'TabPlugin' crates/vmux_layout/src/lib.rs"
```

Change `TabPlugin` to `StackPlugin` in the plugin list.

- [ ] **Step 4: Compile-check**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_layout 2>&1 | tail -30"
```

Fix remaining errors iteratively.

### Task 1.4: Rename `TabCommand` → `StackCommand` in `vmux_command`

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (TabCommand enum definition + AppCommand variant)

- [ ] **Step 1: Read the existing `TabCommand` enum**

```bash
bash -c "grep -n 'TabCommand\\|AppCommand::Tab' crates/vmux_command/src/command.rs"
```

- [ ] **Step 2: Rename in command.rs**

In `crates/vmux_command/src/command.rs`:

- The enum declaration `pub enum TabCommand { ... }` → `pub enum StackCommand { ... }`
- Update inner variant menu IDs and labels:

| Old | New |
|-----|-----|
| `id = "tab_new"` | `id = "stack_new"` |
| `id = "tab_close"` | `id = "stack_close"` |
| `id = "tab_next"` | `id = "stack_next"` |
| `id = "tab_previous"` | `id = "stack_previous"` |
| `id = "tab_select_index_1"` | `id = "stack_select_index_1"` |
| ...etc through 8, last, reopen, duplicate, move_to_pane, swap_prev, swap_next | ditto |
| `label = "New Tab"` | `label = "New Stack"` |
| `label = "Close Tab"` | `label = "Close Stack"` |
| `label = "Next Tab"` | `label = "Next Stack"` |
| `label = "Previous Tab"` | `label = "Previous Stack"` |
| `label = "Reopen Closed Tab"` | `label = "Reopen Closed Stack"` |
| `label = "Duplicate Tab\\t<leader> d"` | `label = "Duplicate Stack\\t<leader> d"` |
| `label = "Move Tab to Pane\\t<leader> !"` | `label = "Move Stack to Pane\\t<leader> !"` |
| `label = "Move Tab Left\\t<leader> <"` | `label = "Move Stack Left\\t<leader> <"` |
| `label = "Move Tab Right\\t<leader> >"` | `label = "Move Stack Right\\t<leader> >"` |
| `accel = "super+t"` | **DEFER to Phase 5** — keep `super+t` for now (will rebind to new TabCommand) |
| `accel = "super+w"` | unchanged |

The `#[menu(label = "Tab")] Tab(TabCommand)` line in `AppCommand` becomes `#[menu(label = "Stack")] Stack(StackCommand)`.

NOTE: `super+t` MUST stay on `StackCommand::New` for now (after Phase 1 it's bound to "create new stack"). In Phase 5 we move it to the new TabCommand (was Space::New). Don't pre-optimize.

- [ ] **Step 3: Update all references to `AppCommand::Tab` and `TabCommand::` in handlers**

```bash
bash -c "grep -rln 'AppCommand::Tab\\b\\|TabCommand::' crates/"
```

Edit each file: `AppCommand::Tab(` → `AppCommand::Stack(`, `TabCommand::` → `StackCommand::`.

- [ ] **Step 4: Update `use vmux_command::TabCommand` imports across all crates**

```bash
bash -c "grep -rln 'use vmux_command::.*TabCommand' crates/"
```

Edit each file: `TabCommand` → `StackCommand` in the use statement.

### Task 1.5: Update SceneFilter allow-list in `persistence.rs`

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs` (`save_session_to_path`)

- [ ] **Step 1: Replace `allow::<Tab>()` with `allow::<Stack>()`**

Find the line:
```rust
.allow::<Tab>()
```

In `save_session_to_path` (search for `SceneFilter::deny_all`). Change to `.allow::<Stack>()`. Also update the `use` import at the top of the file.

### Task 1.6: Verify Phase 1 + commit

- [ ] **Step 1: Run lint**

```bash
make lint
```

Expected: exit 0.

- [ ] **Step 2: Run tests**

```bash
make test
```

Expected: exit 0.

- [ ] **Step 3: Stage and commit**

```bash
bash -c "git add -A && git -c commit.gpgsign=false commit -m 'refactor(VMX-108): rename Tab to Stack (browser page inside pane)'"
```

---

## Phase 2: Space → Tab rename

Today's `Space` (top-level workspace inside a Window/Session) becomes `Tab`.

**Affected types:**
- `Space` → `Tab`
- `SpaceCommand` enum → `TabCommand`
- `SpacePlugin` → `TabPlugin` (note: this collides with the OLD `TabPlugin` we just renamed to `StackPlugin`; the collision is now resolved)
- `SpaceCommandSet` → `TabCommandSet`
- `spawn_new_space` → `spawn_new_tab`
- `active_pane_in_space` → `active_pane_in_tab`

### Task 2.1: Rename files

**Files:**
- Move: `crates/vmux_layout/src/space.rs` → `crates/vmux_layout/src/tab.rs`
- Move: `crates/vmux_layout/src/space/` → `crates/vmux_layout/src/tab/` (if exists)

- [ ] **Step 1: Check whether space subdir exists**

```bash
bash -c "ls crates/vmux_layout/src/space/ 2>/dev/null || echo 'no subdir'"
```

- [ ] **Step 2: Rename**

```bash
bash -c "git mv crates/vmux_layout/src/space.rs crates/vmux_layout/src/tab.rs"
```

If subdir:
```bash
bash -c "git mv crates/vmux_layout/src/space crates/vmux_layout/src/tab"
```

- [ ] **Step 3: Update `mod space` → `mod tab` in `crates/vmux_layout/src/lib.rs`**

Edit the line.

### Task 2.2: Rename types in `tab.rs` (formerly `space.rs`)

**Files:**
- Modify: `crates/vmux_layout/src/tab.rs` (the renamed file)

- [ ] **Step 1: Update `#[type_path]`**

Change `#[type_path = "vmux_desktop::layout::space"]` → `#[type_path = "vmux_desktop::layout::tab"]`.

- [ ] **Step 2: Rename within the file**

`Edit replace_all`:
- `pub struct Space` → `pub struct Tab`
- `pub struct SpacePlugin` → `pub struct TabPlugin`
- `pub struct SpaceCommandSet` → `pub struct TabCommandSet`
- `impl Plugin for SpacePlugin` → `impl Plugin for TabPlugin`
- `With<Space>` → `With<Tab>` (and similar query usages)
- `Space::` → `Tab::`
- `SpaceCommand::` → `TabCommand::`
- `spawn_new_space` → `spawn_new_tab`
- `handle_space_commands` → `handle_tab_commands`
- `active_pane_in_space` → `active_pane_in_tab`
- inside string literals: `"Space {}"` → `"Tab {}"` (the auto-generated name)

### Task 2.3: Update all references in vmux_layout

- [ ] **Step 1: Enumerate references**

```bash
bash -c "grep -rln '\\b\\(Space\\|SpaceCommand\\|SpacePlugin\\|SpaceCommandSet\\|spawn_new_space\\|active_pane_in_space\\|handle_space_commands\\)\\b' crates/vmux_layout/src/"
```

- [ ] **Step 2: Apply renames per file** (same pattern as Phase 1.3)

| Old | New |
|-----|-----|
| `Space` | `Tab` |
| `SpaceCommand` | `TabCommand` |
| `SpacePlugin` | `TabPlugin` |
| `SpaceCommandSet` | `TabCommandSet` |
| `spawn_new_space` | `spawn_new_tab` |
| `active_pane_in_space` | `active_pane_in_tab` |
| `handle_space_commands` | `handle_tab_commands` |
| `use crate::space::` | `use crate::tab::` |
| `crate::space::` | `crate::tab::` |

Be careful: in `crates/vmux_layout/src/stack.rs` (renamed earlier), there's an import like `use crate::space::Space` — this becomes `use crate::tab::Tab`.

- [ ] **Step 3: Update plugin registration in `lib.rs`**

`SpacePlugin` → `TabPlugin` in the plugin list.

### Task 2.4: Rename `SpaceCommand` → `TabCommand` in `vmux_command`

**Files:**
- Modify: `crates/vmux_command/src/command.rs`

- [ ] **Step 1: Rename enum and AppCommand variant**

- `pub enum SpaceCommand` → `pub enum TabCommand`
- `#[menu(label = "Space")] Space(SpaceCommand)` → `#[menu(label = "Tab")] Tab(TabCommand)`
- All `SpaceCommand::` → `TabCommand::`
- Menu IDs: `new_space` → `new_tab`, `close_space` → `close_tab`, `next_space` → `next_tab`, `prev_space` → `prev_tab`, `rename_space` → `rename_tab`
- Menu labels: "New Space" → "New Tab", "Close Space" → "Close Tab", etc.
- Keybindings: keep `Ctrl+g, c/&/n/p/Comma` for now (Phase 5 removes them)

- [ ] **Step 2: Update all callsites**

```bash
bash -c "grep -rln 'AppCommand::Space\\b\\|SpaceCommand::' crates/"
```

Replace `AppCommand::Space(` → `AppCommand::Tab(`, `SpaceCommand::` → `TabCommand::`.

- [ ] **Step 3: Update `use vmux_command::SpaceCommand` imports**

```bash
bash -c "grep -rln 'use vmux_command::.*SpaceCommand' crates/"
```

Edit: `SpaceCommand` → `TabCommand` in use statements.

- [ ] **Step 4: Update `crates/vmux_desktop/src/shortcut.rs` programmatic chord overrides**

`prev_space` / `next_space` mentions remain valid as menu_id strings (we updated the IDs). Verify by reading the file:

```bash
bash -c "grep -n 'space\\|Space' crates/vmux_desktop/src/shortcut.rs"
```

Update string literals `"prev_space"` → `"prev_tab"`, `"next_space"` → `"next_tab"` to match the renamed menu IDs.

### Task 2.5: Update SceneFilter

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs`

- [ ] **Step 1: Replace `allow::<Space>()` with `allow::<Tab>()`**

In the `SceneFilter::deny_all().allow::<Save>().allow::<Tab>()...allow::<Space>()` chain, the `allow::<Space>()` line becomes `allow::<Tab>()`. (After Phase 1, `allow::<Tab>()` was already replaced with `allow::<Stack>()`.)

After Phase 2, the chain reads: `.allow::<Stack>() .allow::<Tab>() .allow::<Pane>() ...` — i.e., the new `Tab` (was `Space`) and `Stack` (was `Tab`).

Also update the `use` imports: `use vmux_layout::space::Space` → `use vmux_layout::tab::Tab` etc.

### Task 2.6: Verify Phase 2 + commit

- [ ] **Step 1: Lint + test**

```bash
make lint && make test
```

Expected: exit 0 for both.

- [ ] **Step 2: Commit**

```bash
bash -c "git add -A && git -c commit.gpgsign=false commit -m 'refactor(VMX-108): rename Space to Tab (top-level workspace)'"
```

---

## Phase 3: Session → Space (incl. crate, URL, file rename)

The biggest phase. Renames the `vmux_session` crate to `vmux_space`, the `SessionRecord`/`SessionRegistry`/`SessionCommand` types to `SpaceRecord`/`SpaceRegistry`/`SpaceCommand`, the `vmux://sessions/` URL scheme to `vmux://spaces/`, and the `vmux_desktop/src/sessions.rs` file to `spaces.rs`.

### Task 3.1: Rename the crate directory and Cargo.toml package name

**Files:**
- Move: `crates/vmux_session/` → `crates/vmux_space/`
- Modify: `crates/vmux_space/Cargo.toml` (package name)
- Modify: workspace `Cargo.toml` (`[profile.release.package.vmux_session]`)
- Modify: `crates/vmux_desktop/Cargo.toml` (path dep)

- [ ] **Step 1: Move the directory**

```bash
bash -c "git mv crates/vmux_session crates/vmux_space"
```

- [ ] **Step 2: Update package name in `crates/vmux_space/Cargo.toml`**

Edit:
```toml
[package]
name = "vmux_session"
```
to:
```toml
[package]
name = "vmux_space"
```

Also update the `description` line if it mentions "session":

```toml
description = "Session list webview app"
```
→
```toml
description = "Space list webview app"
```

- [ ] **Step 3: Update workspace `Cargo.toml`**

Find:
```toml
[profile.release.package.vmux_session]
```
Change to:
```toml
[profile.release.package.vmux_space]
```

- [ ] **Step 4: Update `crates/vmux_desktop/Cargo.toml`**

Find:
```toml
vmux_session = { path = "../vmux_session" }
```
Change to:
```toml
vmux_space = { path = "../vmux_space" }
```

- [ ] **Step 5: Update build/asset paths if any**

Check `crates/vmux_space/build.rs` for hardcoded `vmux_session` paths:

```bash
bash -c "grep -n 'vmux_session\\|sessions' crates/vmux_space/build.rs 2>/dev/null"
```

If it references the dist directory by `vmux_session` name, update.

Check `crates/vmux_space/assets/` and `crates/vmux_space/dist/` for any directory or file names with `sessions`. If the Dioxus app builds output into a `sessions` host directory, that needs to match the new URL host (`spaces`) — update in step 3.4.

- [ ] **Step 6: Compile-check workspace**

```bash
bash -c "env -u CEF_PATH cargo check --workspace 2>&1 | head -50"
```

Many errors expected. Continue with type renames in next task.

### Task 3.2: Rename Session types → Space types

**Files:**
- Modify: `crates/vmux_space/src/event.rs`
- Modify: `crates/vmux_space/src/model.rs`
- Modify: `crates/vmux_space/src/app.rs`
- Modify: `crates/vmux_desktop/src/sessions.rs` (renamed in Task 3.3)
- Modify: `crates/vmux_desktop/src/persistence.rs`
- Modify: `crates/vmux_desktop/src/command_bar.rs`
- Modify: `crates/vmux_command/src/command.rs` (the AppCommand `Session` variant)

**Type rename map:**

| Old | New |
|-----|-----|
| `SessionRecord` | `SpaceRecord` |
| `SessionRegistry` | `SpaceRegistry` |
| `SessionRow` | `SpaceRow` |
| `SessionsListEvent` | `SpacesListEvent` |
| `SessionCommandEvent` | `SpaceCommandEvent` |
| `SessionsPlugin` | `SpacesPlugin` |
| `SessionsView` | `SpacesView` |
| `ActiveSession` | `ActiveSpace` |
| `PendingSessionSwitch` | `PendingSpaceSwitch` |
| `SessionFilePresent` | `SpaceFilePresent` |
| `SessionCommand` | `SpaceCommand` (note: replaces the OLD `SpaceCommand` which we already renamed to `TabCommand` in Phase 2 — no collision now) |
| `DEFAULT_SESSION_ID` | `DEFAULT_SPACE_ID` |
| `DEFAULT_PROFILE_ID` | unchanged |
| Function names: `read_session_registry_from`, `write_session_registry_to`, `delete_session_record`, `delete_session_layout`, `session_layout_path_for`, `normalize_session_id`, `unique_session_id`, `save_session_to_path`, `save_session_on_default_event`, `load_session_on_startup` | replace `session` → `space` consistently |

- [ ] **Step 1: Rename in `vmux_space/src/event.rs`**

Use `Edit replace_all` for each identifier.

Also rename string constants:
- `pub const SESSIONS_WEBVIEW_URL: &str = "vmux://sessions/"` → `pub const SPACES_WEBVIEW_URL: &str = "vmux://spaces/"`
- `pub const SESSIONS_LIST_EVENT: &str = "..."` → `pub const SPACES_LIST_EVENT: &str = "..."` (check the IPC channel name; if changing breaks the Dioxus side, update both ends)

- [ ] **Step 2: Rename in `vmux_space/src/model.rs`**

Apply rename map. Update:
- `registry_path(root) -> root.join("sessions.ron")` → keep filename `sessions.ron` for now? OR rename to `spaces.ron`. **Decision: rename to `spaces.ron` for consistency.** (Old file is moved to `.bak` in Phase 4, so no compat concern.)
- `session_layout_path_for` returns paths like `root/profiles/{profile}/session.ron` and `root/profiles/{profile}/sessions/{id}/session.ron` — rename `session.ron` → `space.ron` and `sessions/` directory → `spaces/`.

Updated paths after rename:
- Default: `root/profiles/{profile}/space.ron`
- Named: `root/profiles/{profile}/spaces/{id}/space.ron`
- Registry: `root/spaces.ron`

- [ ] **Step 3: Rename in `vmux_space/src/app.rs`**

Apply rename map.

- [ ] **Step 4: Rename `vmux_session` import paths**

```bash
bash -c "grep -rln 'use vmux_session\\|vmux_session::' crates/"
```

Replace `vmux_session` → `vmux_space` in every use statement and path qualifier.

- [ ] **Step 5: Update `AppCommand` Session variant**

In `crates/vmux_command/src/command.rs`:
- `pub enum SessionCommand` → `pub enum SpaceCommand`
- `#[menu(label = "Session")] Session(SessionCommand)` → `#[menu(label = "Space")] Space(SpaceCommand)`
- `SessionCommand::Open` → `SpaceCommand::Open`
- Menu IDs: `session_open` → `space_open` (and any other `session_*` IDs)
- Menu labels: "Open Session" → "Open Space" etc.

### Task 3.3: Rename `vmux_desktop/src/sessions.rs` → `spaces.rs`

**Files:**
- Move: `crates/vmux_desktop/src/sessions.rs` → `crates/vmux_desktop/src/spaces.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (`mod sessions;` → `mod spaces;`)

- [ ] **Step 1: Rename**

```bash
bash -c "git mv crates/vmux_desktop/src/sessions.rs crates/vmux_desktop/src/spaces.rs"
```

- [ ] **Step 2: Update module declaration**

```bash
bash -c "grep -n 'mod sessions\\|sessions::' crates/vmux_desktop/src/lib.rs crates/vmux_desktop/src/main.rs"
```

Edit each to use `spaces`.

- [ ] **Step 3: Apply type renames within `spaces.rs`** (per the rename map in Task 3.2 — all `Session*` → `Space*` identifiers, `session` → `space` in fn names and string literals)

### Task 3.4: Rename `vmux://sessions/` → `vmux://spaces/`

**Files:**
- Modify: `crates/vmux_command/src/results.rs` (`SESSIONS_QUERY`, `SESSIONS_PAGE_URL`)
- Modify: `crates/vmux_desktop/src/command_bar.rs` (string literal at line 1918)
- Modify: `crates/vmux_space/src/plugin.rs` or wherever `WebviewAppConfig::with_custom_host("sessions")` is called (was `crates/vmux_desktop/src/sessions.rs:130` per exploration → now `crates/vmux_desktop/src/spaces.rs`)
- Modify: `crates/vmux_space/dist/` directory name (if Dioxus build output is `sessions/`, rename to `spaces/`; check `crates/vmux_space/Dioxus.toml` `out_dir` setting)

- [ ] **Step 1: Update constants in `vmux_command/src/results.rs`**

```rust
const SESSIONS_QUERY: &str = "vmux://sessions";
pub const SESSIONS_PAGE_URL: &str = "vmux://sessions/";
```

→

```rust
const SPACES_QUERY: &str = "vmux://spaces";
pub const SPACES_PAGE_URL: &str = "vmux://spaces/";
```

Also update the test strings within the same file:

```bash
bash -c "grep -n 'vmux://sessions' crates/vmux_command/src/results.rs"
```

Replace each match.

- [ ] **Step 2: Update `command_bar.rs`**

```bash
bash -c "grep -n 'vmux://sessions\\|SESSIONS_PAGE_URL' crates/vmux_desktop/src/command_bar.rs"
```

Replace matches.

- [ ] **Step 3: Update `WebviewAppConfig::with_custom_host` registration**

```bash
bash -c "grep -rn 'with_custom_host..sessions' crates/"
```

Change `with_custom_host("sessions")` → `with_custom_host("spaces")`.

- [ ] **Step 4: Update Dioxus app build output dir**

Check:

```bash
bash -c "cat crates/vmux_space/Dioxus.toml 2>/dev/null"
```

If `out_dir` or any host name is `sessions`, change to `spaces`. Also check whether the Dioxus app builds into a directory inside `crates/vmux_space/` — if so, update.

- [ ] **Step 5: Search for any remaining `sessions` literals tied to URL routing**

```bash
bash -c "grep -rn '\"sessions\"' crates/vmux_space/ crates/vmux_desktop/ | grep -v session_id | grep -v 'fn '"
```

Eyeball the matches; only update strings used for URL host routing or webview-app naming.

### Task 3.5: Update workspace dependents and verify

- [ ] **Step 1: Final compile**

```bash
bash -c "env -u CEF_PATH cargo check --workspace 2>&1 | tail -50"
```

Fix remaining errors.

- [ ] **Step 2: Lint + test**

```bash
make lint && make test
```

- [ ] **Step 3: Commit**

```bash
bash -c "git add -A && git -c commit.gpgsign=false commit -m 'refactor(VMX-108): rename Session to Space (crate, types, URL, file)'"
```

---

## Phase 4: Persistence cleanup (move old `.ron` files to `.bak`)

Adds a one-shot startup system that moves any pre-rename `~/Library/Application Support/Vmux/sessions.ron`, `*/session.ron`, and `sessions/` directories to `*.bak` so the new app boots fresh without parse errors.

### Task 4.1: Add startup migration system

**Files:**
- Create: `crates/vmux_space/src/migration.rs`
- Modify: `crates/vmux_space/src/lib.rs` (add `pub mod migration;`)
- Modify: `crates/vmux_desktop/src/persistence.rs` or `crates/vmux_desktop/src/lib.rs` (register the migration system at `Startup` BEFORE `LayoutStartupSet::Persistence`)

- [ ] **Step 1: Create the migration module**

Create `crates/vmux_space/src/migration.rs` with:

```rust
use bevy::prelude::*;
use std::fs;
use std::path::PathBuf;

const MARKER_FILENAME: &str = ".cascade_rename_done";

/// Move pre-rename files (sessions.ron, session.ron, sessions/) out of the way
/// once on first launch after the cascade rename. Old files become *.bak so
/// users can recover them manually if needed.
pub fn migrate_legacy_session_files(root: PathBuf) {
    let marker = root.join(MARKER_FILENAME);
    if marker.exists() {
        return;
    }
    if !root.exists() {
        return;
    }

    let mut moved = 0;

    // Top-level: sessions.ron -> sessions.ron.bak
    let registry_path = root.join("sessions.ron");
    if registry_path.exists() {
        let bak = root.join("sessions.ron.bak");
        if let Err(err) = fs::rename(&registry_path, &bak) {
            warn!("cascade-rename migration: could not move {:?}: {}", registry_path, err);
        } else {
            moved += 1;
        }
    }

    // Per-profile: profiles/*/session.ron -> session.ron.bak
    // Per-profile: profiles/*/sessions/ -> sessions.bak/
    let profiles_dir = root.join("profiles");
    if profiles_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let profile_dir = entry.path();
                if !profile_dir.is_dir() {
                    continue;
                }

                let session_ron = profile_dir.join("session.ron");
                if session_ron.exists() {
                    let bak = profile_dir.join("session.ron.bak");
                    if let Err(err) = fs::rename(&session_ron, &bak) {
                        warn!("cascade-rename migration: could not move {:?}: {}", session_ron, err);
                    } else {
                        moved += 1;
                    }
                }

                let sessions_dir = profile_dir.join("sessions");
                if sessions_dir.is_dir() {
                    let bak = profile_dir.join("sessions.bak");
                    if let Err(err) = fs::rename(&sessions_dir, &bak) {
                        warn!("cascade-rename migration: could not move {:?}: {}", sessions_dir, err);
                    } else {
                        moved += 1;
                    }
                }
            }
        }
    }

    if moved > 0 {
        info!(
            "cascade-rename migration: moved {} legacy file(s) to *.bak in {:?}. Old data preserved.",
            moved, root
        );
    }

    // Drop the marker so this never runs again.
    if let Err(err) = fs::write(&marker, b"done") {
        warn!("cascade-rename migration: could not write marker {:?}: {}", marker, err);
    }
}
```

- [ ] **Step 2: Register in lib.rs**

Edit `crates/vmux_space/src/lib.rs`:

```rust
pub mod event;
pub mod migration;
pub mod model;
```

- [ ] **Step 3: Wire as a Startup system**

In `crates/vmux_desktop/src/persistence.rs`, add a startup system that runs **before** `load_session_on_startup`:

```rust
use vmux_layout::profile::shared_data_dir;
use vmux_space::migration::migrate_legacy_session_files;

fn run_legacy_migration() {
    migrate_legacy_session_files(shared_data_dir());
}
```

Add to the `PersistencePlugin::build`:

```rust
.add_systems(Startup, run_legacy_migration.before(load_session_on_startup))
```

(Order matters — must run before `load_session_on_startup` reads `space.ron`.)

- [ ] **Step 4: Compile-check**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_desktop"
```

- [ ] **Step 5: Add a unit test**

In `crates/vmux_space/src/migration.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn moves_legacy_files_to_bak() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        fs::write(root.join("sessions.ron"), b"legacy").unwrap();
        let profile_dir = root.join("profiles/default");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("session.ron"), b"legacy").unwrap();
        fs::create_dir_all(profile_dir.join("sessions")).unwrap();

        migrate_legacy_session_files(root.clone());

        assert!(!root.join("sessions.ron").exists());
        assert!(root.join("sessions.ron.bak").exists());
        assert!(!profile_dir.join("session.ron").exists());
        assert!(profile_dir.join("session.ron.bak").exists());
        assert!(!profile_dir.join("sessions").exists());
        assert!(profile_dir.join("sessions.bak").exists());
        assert!(root.join(".cascade_rename_done").exists());
    }

    #[test]
    fn idempotent_after_marker() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        fs::write(root.join(".cascade_rename_done"), b"done").unwrap();
        fs::write(root.join("sessions.ron"), b"legacy").unwrap();

        migrate_legacy_session_files(root.clone());

        // Marker present, so migration is a no-op
        assert!(root.join("sessions.ron").exists());
        assert!(!root.join("sessions.ron.bak").exists());
    }
}
```

If `tempfile` is not already a dev-dependency of `vmux_space`, add it:

```bash
bash -c "grep -n 'tempfile' crates/vmux_space/Cargo.toml"
```

If absent, add to `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 6: Run tests**

```bash
bash -c "env -u CEF_PATH cargo test -p vmux_space migration::tests"
```

Expected: 2 tests pass.

### Task 4.2: Verify Phase 4 + commit

- [ ] **Step 1: Lint + test**

```bash
make lint && make test
```

- [ ] **Step 2: Commit**

```bash
bash -c "git add -A && git -c commit.gpgsign=false commit -m 'feat(VMX-108): one-shot legacy session.ron migration to .bak'"
```

---

## Phase 5: Keybinding rework

After Phases 1-3, types and modules are renamed but keybindings still match the OLD semantics:
- `super+t` fires `StackCommand::New` (was `TabCommand::New`)
- `Ctrl+g, c/&/n/p/Comma` fires `TabCommand::*` (was `SpaceCommand::*`)
- `super+n` fires `WindowCommand::NewWindow` (no handler)

Goal:
- `cmd+t` → `TabCommand::New` (the new Tab — was Space)
- `cmd+n` → `StackCommand::New` (the new Stack — was Tab)
- `cmd+shift+n` / `cmd+shift+p` → `StackCommand::Next` / `StackCommand::Previous`
- `cmd+shift+[` / `cmd+shift+]` → `TabCommand::Previous` / `TabCommand::Next`
- `cmd+1`..`cmd+9` → `TabCommand::SelectIndex1..8` / `TabCommand::SelectLast`
- `cmd+[` / `cmd+]` STAY bound to `BrowserCommand::PrevPage` / `NextPage` (unchanged)
- Remove `super+n` accel from `WindowCommand::NewWindow`
- Remove `Ctrl+g, c/&/n/p/Comma` chord bindings on `TabCommand` (was `SpaceCommand`)
- Remove programmatic chord overrides for `Ctrl+g, ArrowLeft/ArrowRight` in `shortcut.rs`

### Decision (locked)

Stack switching uses `cmd+shift+n` / `cmd+shift+p` (not `cmd+[`/`]`) so that browser back/forward stays on `cmd+[`/`]`. Confirmed by user 2026-05-09.

### Task 5.1: Rebind `super+t` and `super+n`

**Files:**
- Modify: `crates/vmux_command/src/command.rs`

- [ ] **Step 1: Move `accel = "super+t"` from `StackCommand::New` to `TabCommand::New`**

In `StackCommand::New` variant attribute (currently `accel = "super+t"`): drop the accel attribute or replace with a different one (TBD by user). For the spec target, **drop the `super+t` from `StackCommand::New`**.

In `TabCommand::New` variant attribute, add `accel = "super+t"`.

- [ ] **Step 2: Add `accel = "super+n"` to `StackCommand::New`**

In `StackCommand::New` attribute: `accel = "super+n"`.

- [ ] **Step 3: Remove `accel = "super+n"` from `WindowCommand::NewWindow`**

In `crates/vmux_command/src/command.rs` line 382 area:

```rust
#[menu(id = "new_window", label = "New Window", accel = "super+n", hidden)]
NewWindow,
```

Drop the `accel = "super+n"` (keep the variant; the variant has no handler anyway and is `hidden` from menu).

### Task 5.2: Move Stack/Tab switching shortcuts

Today `super+shift+[` and `super+shift+]` are bound to `StackCommand::Previous`/`Next` (was old `TabCommand`). They MOVE up the hierarchy to the new `TabCommand`. Stack switching gets new `super+shift+n`/`super+shift+p` bindings.

- [ ] **Step 1: Move `accel = "super+shift+]"` and `super+shift+[` from `StackCommand::Next/Previous` to `TabCommand::Next/Previous`**

In `crates/vmux_command/src/command.rs`:
- `StackCommand::Next`: drop the `accel = "super+shift+]"`.
- `StackCommand::Previous`: drop the `accel = "super+shift+["`.
- `TabCommand::Next`: add `accel = "super+shift+]"`.
- `TabCommand::Previous`: add `accel = "super+shift+["`.

- [ ] **Step 2: Bind Stack switching to `super+shift+n` / `super+shift+p`**

In `crates/vmux_command/src/command.rs`:
- `StackCommand::Next`: add `accel = "super+shift+n"`.
- `StackCommand::Previous`: add `accel = "super+shift+p"`.

Verify these chords are unbound today:

```bash
bash -c "grep -n 'super+shift+n\\|super+shift+p' crates/vmux_command/src/command.rs"
```

Expected: no matches before this edit (they should be free).

- [ ] **Step 3: Move `super+1..9` from `StackCommand::SelectIndex*` to `TabCommand::SelectIndex*`**

The `SelectIndex1..8` and `SelectLast` variants exist today on `TabCommand` (now `StackCommand`). They need to MOVE to the new `TabCommand` (was `SpaceCommand`).

Decision: does the new `TabCommand` (was Space) need `SelectIndex` variants? Today's `SpaceCommand` has Next/Previous but no SelectIndex. **Recommendation: add `SelectIndex1..8` and `SelectLast` variants to the new `TabCommand` and drop them from `StackCommand` (which will keep Next/Previous only).**

If the user disagrees on this, surface as a decision gate. For the plan default:
- Add `SelectIndex1..8`, `SelectLast` to the new `TabCommand` enum, with `accel = "super+1"` through `super+9`.
- Remove these variants from `StackCommand`. (Update handler in `stack.rs` to drop the dead match arms.)

### Task 5.3: Remove Ctrl+g chord bindings on TabCommand (was SpaceCommand)

**Files:**
- Modify: `crates/vmux_command/src/command.rs`
- Modify: `crates/vmux_desktop/src/shortcut.rs`

- [ ] **Step 1: Drop chord shortcut attrs from `TabCommand` (new) variants**

For each of `TabCommand::New`, `TabCommand::Close`, `TabCommand::Next`, `TabCommand::Previous`, `TabCommand::Rename` (these were the renamed SpaceCommand variants), remove the `#[shortcut(chord = "Ctrl+g, X")]` attribute.

- [ ] **Step 2: Drop programmatic chord overrides in `shortcut.rs`**

In `crates/vmux_desktop/src/shortcut.rs` (lines 44-58 area), remove the two extra entries for `Ctrl+g, ArrowLeft`/`ArrowRight` that bind to `prev_tab`/`next_tab` (after Phase 2 rename).

```bash
bash -c "grep -n 'Ctrl+g\\|ArrowLeft\\|ArrowRight' crates/vmux_desktop/src/shortcut.rs"
```

Delete those lines (keep the surrounding builder unchanged).

### Task 5.4: Verify Phase 5 + commit

- [ ] **Step 1: Lint + test**

```bash
make lint && make test
```

- [ ] **Step 2: Manual smoke-test (build + open app)**

```bash
bash -c "make"
```

(Or whatever the build command is — check the Makefile.)

Then run the app and verify:
- `cmd+t` opens a new Tab (top-level)
- `cmd+n` pushes a new Stack in the focused Pane
- `cmd+shift+[`/`]` cycles Tabs
- `cmd+1`..`cmd+9` jumps to Tab N
- Stack switching keys (per Task 5.0 decision) cycle Stacks
- `Ctrl+g, c/&/n/p/Comma` no longer do anything

If any of these fail, debug before committing.

- [ ] **Step 3: Commit**

```bash
bash -c "git add -A && git -c commit.gpgsign=false commit -m 'feat(VMX-108): rebind cmd+t/n/[/] for new hierarchy'"
```

---

## Phase 6: Zen toggle + delete individual chrome toggles + default chrome visible

### Task 6.1: Add `ZenCommand`, `ZenMode` resource, and handler

**Files:**
- Create: `crates/vmux_layout/src/zen.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (add `pub mod zen;` + register `ZenPlugin`)
- Modify: `crates/vmux_command/src/command.rs` (add `ZenCommand` enum + `AppCommand::Zen` variant)

- [ ] **Step 1: Define `ZenCommand` enum in command.rs**

After existing enums in `crates/vmux_command/src/command.rs`, add:

```rust
#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZenCommand {
    #[default]
    #[menu(id = "zen_toggle", label = "Toggle Zen Mode", accel = "super+shift+s")]
    Toggle,
}
```

Add the variant to `AppCommand`:

```rust
#[menu(label = "Zen")]
Zen(ZenCommand),
```

- [ ] **Step 2: Create `crates/vmux_layout/src/zen.rs`**

```rust
use crate::{footer::Footer, header::Header, side_sheet::SideSheet};
use crate::Open;
use bevy::prelude::*;
use vmux_command::{AppCommand, ReadAppCommands, ZenCommand};

#[derive(Resource, Default, Debug)]
pub struct ZenMode {
    pub active: bool,
}

pub struct ZenPlugin;

impl Plugin for ZenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZenMode>()
            .add_systems(Update, handle_zen_toggle.in_set(ReadAppCommands));
    }
}

fn handle_zen_toggle(
    mut reader: MessageReader<AppCommand>,
    mut zen: ResMut<ZenMode>,
    header_q: Query<Entity, With<Header>>,
    footer_q: Query<Entity, With<Footer>>,
    sidesheet_q: Query<Entity, With<SideSheet>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        if !matches!(cmd, AppCommand::Zen(ZenCommand::Toggle)) {
            continue;
        }
        zen.active = !zen.active;

        if zen.active {
            // Hide all chrome
            for entity in header_q.iter().chain(footer_q.iter()).chain(sidesheet_q.iter()) {
                commands.entity(entity).remove::<Open>();
            }
        } else {
            // Show all chrome (no snapshot — restore = all visible)
            for entity in header_q.iter().chain(footer_q.iter()).chain(sidesheet_q.iter()) {
                commands.entity(entity).insert(Open);
            }
        }
    }
}
```

- [ ] **Step 3: Register module + plugin in `lib.rs`**

In `crates/vmux_layout/src/lib.rs`:
- Add `pub mod zen;` near the other module declarations.
- Add `ZenPlugin` to the plugin list inside `LayoutPlugin::build`.

- [ ] **Step 4: Re-export `ZenCommand`**

```bash
bash -c "grep -n 'pub use' crates/vmux_command/src/lib.rs"
```

Add `pub use command::ZenCommand;` if other commands are re-exported there (follow the existing pattern).

- [ ] **Step 5: Compile-check**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_layout"
```

### Task 6.2: Delete individual chrome toggle commands and handlers

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (delete enum variants)
- Modify: `crates/vmux_layout/src/header.rs` (delete handler + plugin system)
- Modify: `crates/vmux_layout/src/footer.rs` (delete `FooterCommand::Toggle` arm; keep `BrowserCommand::Find` arm)
- Modify: `crates/vmux_layout/src/side_sheet.rs` (delete handler + plugin system; SideSheet itself stays)

- [ ] **Step 1: Delete `HeaderCommand::Toggle` variant**

In `crates/vmux_command/src/command.rs`:

```rust
pub enum HeaderCommand {
    #[default]
    #[menu(id = "toggle_header", label = "Toggle Header", accel = "super+shift+h")]
    Toggle,
}
```

becomes:

```rust
pub enum HeaderCommand {}
```

If an empty enum causes compile errors with the derive macros (which expect a `Default`), delete the entire `HeaderCommand` enum AND remove the `Header(HeaderCommand)` variant from `AppCommand`.

- [ ] **Step 2: Delete `FooterCommand::Toggle` and the whole enum if empty**

Same treatment as `HeaderCommand`. If `FooterCommand` becomes empty after deleting `Toggle`, delete it entirely and remove `Footer(FooterCommand)` from `AppCommand`.

Note: there may be other variants on `FooterCommand` (search to confirm). If only `Toggle` exists, drop the whole enum.

- [ ] **Step 3: Delete `SideSheetCommand::Toggle/ToggleRight/ToggleBottom`**

In `crates/vmux_command/src/command.rs`, delete all three variants. If `SideSheetCommand` becomes empty, delete it and remove `SideSheet(SideSheetCommand)` from `AppCommand`.

- [ ] **Step 4: Delete `handle_header_toggle` from `header.rs`**

In `crates/vmux_layout/src/header.rs`:
- Delete the `handle_header_toggle` function entirely.
- Remove `app.add_systems(Update, handle_header_toggle.in_set(ReadAppCommands))` from `HeaderLayoutPlugin::build`. If the plugin becomes empty, leave it as a no-op stub or delete it from the plugin list (delete from `lib.rs` plugin tuple).
- Drop unused imports.

- [ ] **Step 5: Update `handle_footer_toggle` (or rename)**

In `crates/vmux_layout/src/footer.rs`:
- Delete the `AppCommand::Footer(FooterCommand::Toggle) => Some(!is_open)` arm in `footer_open_after_command`.
- The `AppCommand::Browser(BrowserCommand::Find) if is_open => Some(false)` arm STAYS (escape-find behavior).
- Rename `handle_footer_toggle` → `handle_footer_browser_find` (or similar) to reflect its new sole purpose.
- Drop the `FooterCommand` import if no longer used.

- [ ] **Step 6: Delete `handle_side_sheet_toggle` from `side_sheet.rs`**

Delete the entire `handle_side_sheet_toggle` function, remove its system registration from `SideSheetLayoutPlugin::build`, drop unused imports (`SideSheetState`, `SideSheetCommand`).

- [ ] **Step 7: Compile-check**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_layout -p vmux_command 2>&1 | tail -30"
```

### Task 6.3: Delete `HeaderState` and `SideSheetState`

**Files:**
- Modify: `crates/vmux_layout/src/lib.rs` (struct definitions, plugin registrations)
- Modify: `crates/vmux_desktop/src/persistence.rs` (allow-list, imports)

- [ ] **Step 1: Find all uses**

```bash
bash -c "grep -rn 'HeaderState\\|SideSheetState' crates/"
```

- [ ] **Step 2: Delete struct definitions**

In `crates/vmux_layout/src/lib.rs` (lines 110, 118 area), delete the `HeaderState` and `SideSheetState` struct definitions and their `register_type` calls.

- [ ] **Step 3: Drop from SceneFilter**

In `crates/vmux_desktop/src/persistence.rs`, delete `.allow::<HeaderState>()` and `.allow::<SideSheetState>()` from the `save_session_to_path` filter chain.

Drop the `HeaderState` / `SideSheetState` imports.

- [ ] **Step 4: Drop persistence-state spawn logic**

```bash
bash -c "grep -rn 'HeaderState\\|SideSheetState' crates/vmux_desktop/src/"
```

If `ensure_layout_state_entities` or `apply_persisted_layout_state` references these types, drop the relevant code blocks. The chrome state is no longer per-session.

- [ ] **Step 5: Compile-check**

```bash
bash -c "env -u CEF_PATH cargo check --workspace 2>&1 | tail -30"
```

### Task 6.4: Default all chrome to visible at spawn

**Files:**
- Modify: `crates/vmux_layout/src/window.rs` (lines 225-243 SideSheet, 262-275 Header, 289-302 Footer)

- [ ] **Step 1: Update Header spawn**

Find the Header spawn block (around line 262-275). Currently it has `Visibility::Hidden` and no `Open`. Add `crate::Open` to the bundle and change `Visibility::Hidden` → `Visibility::Inherited`. Also remove `Display::None` from the initial node if present (let the sync system handle it).

- [ ] **Step 2: Update SideSheet spawn (Left)**

Find the SideSheet spawn block (around line 225-243). Same treatment: add `crate::Open`, set `Visibility::Inherited`, remove `Display::None`.

If there are spawns for Right and Bottom positions too, apply the same change (search for `SideSheetPosition::Right` / `Bottom`).

- [ ] **Step 3: Footer spawn already has `Open` — verify**

Check that the Footer spawn block (around line 289-302) still has `crate::Open` in its bundle. If it was lost during refactoring, restore it.

- [ ] **Step 4: Compile-check**

```bash
bash -c "env -u CEF_PATH cargo check -p vmux_layout"
```

### Task 6.5: Verify Phase 6 + commit

- [ ] **Step 1: Lint + test**

```bash
make lint && make test
```

- [ ] **Step 2: Manual smoke-test**

Build and run. Verify:
- App launches with Header + Footer + SideSheet all visible
- `cmd+shift+s` hides all three
- `cmd+shift+s` again shows all three
- Old `super+s`, `super+shift+h`, `super+shift+f` no longer do anything

- [ ] **Step 3: Commit**

```bash
bash -c "git add -A && git -c commit.gpgsign=false commit -m 'feat(VMX-108): zen mode toggle, drop individual chrome toggles'"
```

---

## Phase 7: Open PR

### Task 7.1: Push and create PR

- [ ] **Step 1: Push branch**

```bash
bash -c "git push -u origin feature/vmx-108-cascade-rename"
```

- [ ] **Step 2: Open PR via Linear**

```bash
bash -c "linear issue pr"
```

(`linear issue pr` auto-detects branch and links to VMX-108.)

If `linear issue pr` doesn't take a body, use `gh pr create` with a body summarizing the spec.

- [ ] **Step 3: Delete the plan file**

Per AGENTS.md: "Delete the plan file once the plan is fully implemented."

```bash
bash -c "git rm docs/plans/2026-05-09-cascade-rename.md && git -c commit.gpgsign=false commit -m 'chore(VMX-108): drop completed plan' && git push"
```

---

## Risks and Notes

- **Phase 1-3 are largely mechanical.** A subagent should be able to execute each via grep + Edit replace_all. Watch out for false-positive matches: `Tab`, `Space`, `Session` are common English words — when in doubt, narrow the regex (e.g. `\\bTabCommand\\b` not `Tab`).

- **Phase 5 has a UX decision gate (cmd+[/])** — pause for user input before proceeding.

- **Phase 6 (zen + chrome cleanup) has the most behavioral change.** Manual smoke test mandatory before commit.

- **bisect-ability:** every commit must build and pass tests on its own. Don't combine phases or split a phase across commits.

- **Empty-enum risk:** if `HeaderCommand`/`FooterCommand`/`SideSheetCommand` become empty after deletion, the `OsSubMenu` / `DefaultShortcuts` / `CommandBar` derive macros may not handle empty enums. Test compile early in Task 6.2 — if errors appear, delete the enum entirely and remove the corresponding `AppCommand` variant.

- **Crate rename Cargo.lock churn:** after Phase 3, `Cargo.lock` will have major churn (`vmux_session` → `vmux_space`). Stage and commit it as part of the Phase 3 commit.

- **CEF embedded asset path:** the `vmux://spaces/` URL relies on `WebviewAppRegistry::register` matching the new host name AND the embedded `dist/` directory being either renamed or properly aliased. Test by running the app post-Phase-3 and navigating to `vmux://spaces/` to confirm it resolves.
