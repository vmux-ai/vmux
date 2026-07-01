# Tab-scoped `startup_dir` + per-tab worktrees — design

Date: 2026-07-01
Status: Draft (awaiting review)
Branch: `feat/tab-startup-dir`, **stacked on `feat/acp-host`** (both merge together)

## Problem

`startup_dir` today is a static, invisible config:

- The only way to set it is to tell an agent to edit `settings.ron`. Indirect, undiscoverable.
- Resolution is **per-space only**: `space override → global terminal.startup_dir → ~/.vmux/spaces/<id>` (`resolve_startup_dir`, `crates/vmux_setting/src/plugin/runtime.rs:167`). Every tab in a space shares one dir.
- The side sheet shows a space's dir but nothing about *where the value came from* or *what it scopes*.
- There is no worktree concept anywhere in the app, so a task can't be given an isolated branch/dir.

Users think in **tasks**. A task wants its own working directory — often its own git branch — and everything opened for that task (the agent *and* your own terminals) should land there. The unit that maps to a task in vmux is the **tab**.

## Context this design assumes

**ACP host (`feat/acp-host`).** vmux is becoming an ACP host (the Zed model): external agents run as ACP adapters (`claude-code-acp`, `codex-acp`, `vibe-acp`) driven by the `vmux_service` daemon. Each agent runs as an `AcpSession { agent_id, sid, cwd, anchor }` (`crates/vmux_agent/src/client/acp.rs:17`). The raw-PTY `Cli` path stays as a **fallback** until the ACP chat page matures, then Milestone D retires it. This design targets the ACP path and keeps the Cli path working (both cwd sites get the same one-line change).

**The boundary is native, not something we build.** Under ACP the working dir is the session `cwd`, and it is already enforced by two cooperating layers keyed to that one dir:

1. **The agent's own sandbox** — `codex-acp` / `claude-code-acp` run the real agent with its real sandbox scoped to `cwd`. We *utilize* it; we do **not** disable it. (The tool-disabling seen on the Cli path — claude `--disallowedTools Bash`, codex `DISABLED_FEATURES` — is Cli-only and retires with it.)
2. **vmux's ACP host fs handlers** — `resolve_in_cwd` (`crates/vmux_service/src/acp/driver.rs:312`) rejects any host-served read/write outside `cwd`; the subprocess spawns with `.current_dir(cwd)` (`driver.rs:83`); `request_permission` round-trips exist (`systems/approval.rs`). We *enhance* with host-side scoping.

So "boundary" = the tab's `cwd`, and setting it to a worktree gives isolation **and** enforcement for free.

**Unify terminal surface (Model A).** Agent and user share **one** terminal surface per tab (Cursor "agent is driving — click in to take over" model). Not an MCP loopback bridge (Model B) and not an agent-only ACP terminal panel (Model C). Consequence for this design: a tab has **one** working dir, used by both the ACP agent session and your own terminals in that tab. There is no separate "agent dir" vs "terminal dir".

## Goals

1. Add a **tab-level** `startup_dir` override. Resolution becomes `tab → space → global → builtin`.
2. Everything spawned in a tab — the ACP agent session **and** your terminals — shares that tab's dir (the Unify surface's single workspace).
3. Optional **per-tab git worktree**: one tab ↔ one worktree ↔ one branch, created lazily and opt-in. Because the worktree becomes the ACP `cwd`, the agent is natively isolated + sandboxed to its own branch.
4. A **visible boundary indicator** in the side sheet: effective dir, provenance, worktree/branch, whether it's ACP-sandboxed, and how many panes it scopes. Editable in place at tab and space level — replacing "tell the agent to edit settings".
5. A **guided "New Task"** entry that picks a folder up front.

## Non-goals

- **No bespoke sandbox and no disabling of agent sandboxes.** Enforcement rides ACP (agent-native sandbox + host `resolve_in_cwd`). We only set the `cwd`. No OS seatbelt/landlock on the PTY (would also break plain human terminals and legit reads like `~/.cargo`, global gitconfig, `/tmp`).
- No new workspace crate (reuse existing crates; see touch map).
- No auto-worktree for every tab. Worktrees are opt-in per tab.
- No branch/PR automation beyond create/remove. Commit/push/rebase stay the user's job (editor git panel + terminal cover those).
- No change to the Unify surface itself (that is the acp-host work); this design only supplies the shared per-tab cwd/worktree it operates in.

## Terminology

Per repo memory: **space** (not "workspace"), **the layout** (UI shell), **page** (web content). Hierarchy: `Space → Tab → Pane (/PaneSplit tree) → Stack → page`. A **Tab** owns a pane tree and maps to a user's *task*. This design attaches dir/worktree state to the **Tab**.

## Overview

```
resolve_startup_dir_for_tab(settings, space_id, tab_dir):
    tab_dir  →  space override  →  global terminal.startup_dir  →  ~/.vmux/spaces/<id>
    (each candidate validated: trimmed, non-empty, existing dir; else cascade)

tab dir  ─┬─► ACP agent session cwd (AcpSession.cwd → NewSessionRequest → current_dir + resolve_in_cwd)
          └─► user terminal cwd (vmux_terminal spawn sites)
                = one shared workspace for the tab's Unify surface
```

- `Tab.startup_dir: Option<String>` is the single effective dir for a tab.
- `TabWorktree { repo_root, branch, base_ref }` annotates a tab whose dir is a vmux-managed worktree.
- Setting the tab dir to a worktree makes it the ACP `cwd` → agent isolated + sandboxed to its branch; your terminals in the tab share it.
- Worktrees are opt-in: offered once when agent work starts in an undecided git-repo tab, or via "New Task", or a manual side-sheet button.
- The side sheet renders a backend-computed `TabBoundary` DTO (dumb frontend) and exposes change-dir / isolate / remove-worktree actions.

## Detailed design

### 1. Resolution (`vmux_setting`)

Add to `crates/vmux_setting/src/plugin/runtime.rs`, keeping the crate ECS-free (pure functions):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirSource { Tab, Space, Global, Default }

/// tab_dir wins if valid; otherwise delegates to the existing space→global→builtin chain.
pub fn resolve_startup_dir_for_tab(
    settings: &AppSettings,
    space_id: &str,
    tab_dir: Option<&str>,
) -> std::path::PathBuf;

/// Same, but also reports which level supplied the value (for the boundary badge).
pub fn resolve_startup_dir_for_tab_with_source(
    settings: &AppSettings,
    space_id: &str,
    tab_dir: Option<&str>,
) -> (std::path::PathBuf, DirSource);
```

Existing `resolve_startup_dir` becomes the space→global→builtin tail (unchanged behavior; callers without a tab keep working). Validation reuses the current `pick` closure (trim, non-empty, `is_dir`).

### 2. State / components (`vmux_layout`, `vmux_core`)

`crates/vmux_layout/src/tab.rs` — extend the component:

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Tab {
    pub name: String,
    #[serde(default)]
    pub startup_dir: Option<String>,   // effective per-tab dir override
}
```

New components (`tab.rs`), both reflected + `Save`:

```rust
/// Present iff Tab.startup_dir points at a vmux-managed worktree.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabWorktree {
    pub repo_root: String,
    pub branch: String,
    pub base_ref: String,
}

/// Set once the workspace decision is made for a tab (isolate or work-here), so the offer never re-fires.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabDirDecided;
```

`Tab.startup_dir` auto-persists (Tab already in the `store.ron` allowlist). `TabWorktree` and `TabDirDecided` are new types → add to the moonshine save allowlist in `crates/vmux_desktop/src/persistence.rs:194`.

Shared messages/DTOs go in `vmux_core` (matching `agent.rs` / `event/space.rs` precedent) — see §4 and §7.

### 3. Spawn alignment (both actors, both paths)

Read the active tab via `ActiveTabParam` (`crates/vmux_layout/src/stack.rs:126`, already used cross-crate) and resolve `tab_dir = active_tab.startup_dir`. Swap every cwd site from space-scoped to tab-scoped:

| Actor / path | File:line (on `feat/acp-host`) | Change |
|---|---|---|
| **ACP agent session cwd** | `vmux_agent/src/plugin.rs:1764` (feeds `attach_acp_agent_to_stack` → `AcpSession.cwd`) | `resolve_startup_dir_for_tab(&settings, space_id, tab_dir)` |
| agent `RunTerminal`/service cmd cwd | `vmux_agent/src/plugin.rs:932`, `run_terminal_cwd` `:1222` | same (agent-opened terminals share the tab dir) |
| user terminal in new pane | `vmux_terminal/src/plugin.rs:485` | same |
| `vmux://terminal` open fallback | `vmux_terminal/src/plugin.rs:590` | same |

Because a tab's dir is fixed, the ACP agent and every terminal in the tab receive the **same** cwd → the Unify surface has one workspace. The daemon needs **no change** — `AcpSession.cwd` flows through `SpawnAcpAgent` → `current_dir(cwd)` + `resolve_in_cwd` exactly as today. Restore path (`persistence.rs:547`) still uses saved `TerminalLaunch.cwd`.

### 4. Worktree engine

**Git ops (`vmux_git`, new `crates/vmux_git/src/worktree.rs`).** `vmux_git` already shells out via the private `git(root, args)` helper (`runner.rs:11`). The existing API is file-centric and canonicalizes paths, so it can't create a not-yet-existing worktree dir; add root/path-based functions:

```rust
pub struct WorktreeInfo { pub path: PathBuf, pub branch: String, pub base_ref: String }
pub struct WorktreeStatus { pub uncommitted: u32, pub ahead: u32 }

pub fn repo_root_of(dir: &Path) -> Result<PathBuf, GitError>;          // rev-parse --show-toplevel from dir
pub fn worktree_add(root: &Path, path: &Path, branch: &str, base: &str)
    -> Result<WorktreeInfo, GitError>;                                  // worktree add <path> -b <branch> <base>
pub fn worktree_remove(root: &Path, path: &Path, force: bool)
    -> Result<(), GitError>;                                            // worktree remove [--force] + branch -D
pub fn worktree_status(path: &Path) -> Result<WorktreeStatus, GitError>;// status --porcelain + rev-list count
pub fn worktree_list(root: &Path) -> Result<Vec<PathBuf>, GitError>;    // worktree list --porcelain
pub fn head_ref(root: &Path) -> Result<String, GitError>;              // symbolic branch name, else short SHA
```

Reuse `parse.rs` porcelain parsing where possible. Pure/synchronous (block on `git`).

**Naming / placement.**
- `slug` = sanitize(`Tab.name`): lowercase, non-alnum → `-`, collapse repeats, trim, fallback `task`. Uniqueness: if branch `vmux/<slug>` or path exists, append `-2`, `-3`, …
- `path` = `<repo_root>/.worktrees/<slug>`.
- `branch` = `vmux/<slug>`.
- `base_ref` = `head_ref(repo_root)` (repo's current HEAD — least surprising for a general feature; the AGENTS.md "branch off origin/main" rule is a dev-workflow convention, not app behavior).
- Append `.worktrees/` to `<repo_root>/.git/info/exclude` (local, non-invasive) if not already ignored — never edit the repo's tracked `.gitignore`.

**Orchestration (`vmux_layout`, new `crates/vmux_layout/src/worktree.rs`).** `vmux_layout` gains a dep on `vmux_git` (no cycle). Messages defined in `vmux_core`:

```rust
#[derive(Message, Clone, Debug)]
pub struct CreateTabWorktreeRequest { pub tab: Entity, pub slug_hint: String, pub base_dir: PathBuf }
#[derive(Message, Clone, Debug)]
pub struct RemoveTabWorktreeRequest { pub tab: Entity, pub force: bool }
#[derive(Message, Clone, Debug)]
pub struct TabWorktreeReady { pub tab: Entity, pub info: WorktreeInfo }
#[derive(Message, Clone, Debug)]
pub struct TabWorktreeError { pub tab: Entity, pub message: String }
```

Git calls run **off the main thread** (reuse `vmux_git`'s `std::thread::spawn` + shared-mutex outbox drain, `plugin.rs:45`, or a Bevy `AsyncComputeTaskPool` task). On success the drain system writes `Tab.startup_dir = info.path`, inserts `TabWorktree` + `TabDirDecided`, emits `TabWorktreeReady`. On failure it emits `TabWorktreeError` (toast; tab keeps inherited dir).

### 5. Lazy opt-in offer — "isolate this task"

Reframed from "jail the agent" to *"give this task its own worktree"* (the whole shared surface lands on one branch).

Hook at ACP session creation — where `attach_acp_agent_to_stack` is about to run (`vmux_agent/src/plugin.rs`, the page-open handler around `:1761`). Before creating the `AcpSession`:

1. Resolve the target tab (ancestor `Tab` of the target stack).
2. If the tab has `TabDirDecided` (or already has `TabWorktree`) → create the session normally with the resolved cwd.
3. Else compute the would-be cwd and test `vmux_git::repo_root_of(cwd)`:
   - **Not a git repo** → insert `TabDirDecided`, create session (nothing to isolate).
   - **Git repo** → defer session creation and present an offer.

Offer UI (v1): `rfd::MessageDialog` mapped to **Isolate / Work here / Cancel** (`rfd` is already a dep; modal-from-system is an accepted pattern, `pane.rs:2069`), run on a main-thread-pinned system.

- **Isolate** → `CreateTabWorktreeRequest { tab, slug_hint: tab.name, base_dir: cwd }`; on `TabWorktreeReady`, create the `AcpSession` with `cwd = worktree`.
- **Work here** → insert `TabDirDecided`, create the session in the inherited dir.
- **Cancel** → abort.

Fires at most once per tab, only for git-repo dirs. Plain terminal use never triggers it. A small `PendingAgentSpawn` component/resource holds the deferred request (agent_id/sid/prompt) for re-emit.

*Polish (not v1):* replace the modal with an inline banner in the agent stack for a non-blocking feel.

### 6. Guided entry — "New Task" (`vmux_command`, `vmux_layout`, `vmux_desktop`)

- Add `TabCommand::New` (`crates/vmux_command/src/command.rs:304`) — a `Copy` variant (no payload; the path comes from the picker), surfaced in command bar + OS menu via the existing derives.
- A main-thread system reads it, opens `rfd::FileDialog::new().pick_folder()`, and on a pick writes `TabLayoutSpawnRequest { startup_dir: Some(picked), .. }`.
- Extend `TabLayoutSpawnRequest` (`crates/vmux_layout/src/lib.rs:190`) with `startup_dir: Option<String>`; `spawn_tab_scaffold_in_space` (`window.rs:460`) stamps it onto the new `Tab`. The tab is created with the dir set but **not** `TabDirDecided`, so the §5 isolate offer is the single isolation decision point when the user launches the agent.
- After spawning, open the ACP agent page so the user picks an agent.

### 7. Side sheet = boundary indicator (`vmux_core`, `vmux_space`/`vmux_layout`, WASM page)

Backend computes a DTO (frontend stays dumb — render + emit intents):

```rust
pub struct TabBoundary {
    pub effective_dir: String,     // ~-abbreviated for display
    pub source: DirSource,         // Tab | Space | Global | Default
    pub is_worktree: bool,
    pub branch: Option<String>,
    pub base_ref: Option<String>,
    pub uncommitted: u32,
    pub ahead: u32,
    pub pane_count: u32,           // leaf panes in the active tab
    pub sandboxed: bool,           // true when an ACP session is active in the tab (fs scoped to cwd)
}
```

Computed for the active tab (parallel to `space_rows_from_world`, `vmux_space/src/plugin.rs:184`) and sent to the layout webview alongside `SpaceRow`/`PaneNode`. `source` from `resolve_startup_dir_for_tab_with_source`; `sandboxed` true when the tab holds an `AcpSession`; git fields from `TabWorktree` + a cached `worktree_status`.

New section in `SideSheetView` (`crates/vmux_layout/src/page.rs:664`), above the per-pane stack list:

```
▸ auth refactor
   🌿 .worktrees/auth-refactor · vmux/auth-refactor ← main · 🔒 sandboxed
   3 panes aligned · ● 2 uncommitted
   [Change dir]  [Remove worktree…]
```
Inherited tab:
```
▸ scratch
   📁 ~/proj/app · from space
   [Change dir]  [Isolate as worktree]
```

The space header dir (`SideSheetSpaceRow`, `page.rs:707`) becomes clickable → `pick_folder()` → writes the space (or global) `startup_dir` to `settings.ron`. **Must write the whole `terminal`/`spaces` section** to avoid the known section-merge wipe.

Actions emit a new message (avoid overloading `SideSheetCommandEvent`):

```rust
#[derive(Message, Clone, Debug)]
pub struct BoundaryCommandEvent { pub tab: Entity, pub kind: BoundaryCmd, pub path: Option<String> }
pub enum BoundaryCmd { SetTabDir, ClearTabDir, Isolate, RemoveWorktree, SetSpaceDir }
```

Git status refreshed on: tab activation, after any worktree op, and debounced (~5s) while the side sheet is open — never every frame.

### 8. Teardown + restore reconcile

**Tab close never auto-removes** (keep + notify). On close of a tab with `TabWorktree`, run `worktree_status`:
- clean → keep on disk, info notice ("worktree kept: `<path>` (`<branch>`)").
- uncommitted/unpushed → keep + warn notice.

**Explicit removal** via `[Remove worktree…]` → `RemoveTabWorktreeRequest`:
- clean → `worktree_remove(root, path, force=false)` + delete branch.
- dirty → `rfd` confirm "discard N uncommitted changes?" → `worktree_remove(.., force=true)`.

**Restore reconcile** (in/after `rebuild_space_views`, `persistence.rs:374`): for each restored `TabWorktree`, verify `path` exists and appears in `worktree_list(repo_root)`. If gone → remove the `TabWorktree` component (the resolver's `is_dir` validation makes `Tab.startup_dir` cascade). Log at `warn!`.

## Data flow (isolate-on-agent-start, ACP)

```
user opens agent in tab T (undecided, dir in git repo)
  → page-open handler: repo detected → defer AcpSession creation, buffer request
  → rfd offer [Isolate / Work here / Cancel]
      Isolate → CreateTabWorktreeRequest{T, slug, base_dir}
              → worker thread: git worktree add .worktrees/<slug> -b vmux/<slug> <HEAD>
              → drain: Tab.startup_dir = path; insert TabWorktree, TabDirDecided; TabWorktreeReady
              → attach_acp_agent_to_stack(cwd = worktree)
                  → AcpSession.cwd → SpawnAcpAgent → daemon current_dir(cwd) + resolve_in_cwd
                  → agent natively isolated + sandboxed to its branch
      Work here → insert TabDirDecided → create session (inherited dir)
  → your terminals in T spawn with the same cwd (Unify surface, one workspace)
  → TabBoundary recomputed → side sheet shows worktree + 🔒 sandboxed
```

## Persistence

- `Tab.startup_dir` — auto-persists (field on already-saved `Tab`; confirm `serde(default)` so pre-existing `store.ron` loads).
- `TabWorktree`, `TabDirDecided` — add to allowlist (`persistence.rs:194`).
- No schema-version bump (additive reflected components/fields deserialize with defaults).

## Error handling

- Worktree create/remove failure → `TabWorktreeError` → toast; tab falls back to inherited dir (never blocks).
- Picked/worktree dir invalid or removed → resolver `is_dir` validation cascades; boundary `source` reflects the level used.
- Non-git dir → isolate actions hidden/disabled; offer never fires.
- Git binary missing → `repo_root_of` errors → treated as "not a repo" (session/terminal proceed in inherited dir).

## Testing

Assert via messages + ECS state, not ad-hoc helpers (repo rule).

- **vmux_setting**: `resolve_startup_dir_for_tab` — tab wins; invalid tab cascades to space→global→builtin; `_with_source` reports correct `DirSource`. (Unit.)
- **vmux_git**: `worktree_add`/`remove`/`status`/`list`/`head_ref` against a `tempfile` git repo; slug uniqueness; `remove` refuses dirty without force. (Unit.)
- **spawn alignment**: set active tab `startup_dir`; assert the ACP `AcpSession.cwd` (GUI-side, before `SpawnAcpAgent`) **and** a user terminal's `TerminalLaunch.cwd` both equal the tab dir. (Message + system.)
- **offer gate**: undecided git-repo tab defers + offers; "work here" sets `TabDirDecided` and creates the session; decided tab never re-offers; non-repo tab proceeds immediately.
- **teardown**: close with clean worktree → keep + notice; dirty → keep + warn; explicit remove clean vs dirty(force).
- **boundary DTO**: `source`/`is_worktree`/`pane_count`/`sandboxed` correct across tab/space/global/default and worktree + ACP-active cases.
- **persistence round-trip**: `Tab.startup_dir` + `TabWorktree` + `TabDirDecided` save/restore; missing-worktree reconcile drops the component and cascades.

Run `cargo test --workspace` before push. CI runs fmt + clippy + tests.

## Crate touch map

| Crate | Change |
|---|---|
| `vmux_setting` | `resolve_startup_dir_for_tab[_with_source]`, `DirSource` |
| `vmux_layout` | `Tab.startup_dir`; `TabWorktree`/`TabDirDecided`; `worktree.rs` orchestration; `TabBoundary` compute + `SideSheetView` section; `TabLayoutSpawnRequest.startup_dir`; dep on `vmux_git` |
| `vmux_git` | `worktree.rs` (add/remove/status/list/repo_root_of/head_ref) |
| `vmux_agent` | tab-scoped cwd at the ACP session site (`plugin.rs:1764`) + agent-terminal/Cli-fallback sites (`:932`, `:1222`); isolate-offer gate + buffered re-emit |
| `vmux_terminal` | tab-scoped cwd at the two user-terminal spawn sites |
| `vmux_command` | `TabCommand::New` |
| `vmux_core` | worktree messages, `TabBoundary`, `BoundaryCommandEvent` |
| `vmux_desktop` | persistence allowlist; "New Task" folder-picker system (main thread) |
| `vmux_service` | **none** — daemon already sandboxes to `cwd` (`resolve_in_cwd`, `current_dir`) |
| WASM page (`vmux_layout`) | render `TabBoundary`; emit `BoundaryCommandEvent` intents |

No new crates. `rfd` (existing dep) provides folder picker + confirm dialogs cross-platform (macOS + Linux/CI).

## Implementation order (single plan, incremental commits)

Each step compiles + tests green before the next:

1. `resolve_startup_dir_for_tab` + `DirSource` (+ tests).
2. `Tab.startup_dir` field + allowlist; thread through all cwd sites (ACP + terminals + agent-terminals) via `ActiveTabParam` (+ spawn-alignment tests).
3. `TabBoundary` DTO + side-sheet boundary section (read-only), incl. space/global dir editing via picker (+ DTO tests).
4. `vmux_git` worktree fns (+ tests).
5. `TabWorktree`/`TabDirDecided` + `vmux_layout` worktree orchestration (+ tests).
6. Isolate-offer gate at ACP session creation (+ gate tests).
7. `[Isolate]`/`[Remove worktree…]` actions + teardown + restore reconcile (+ teardown tests).
8. `TabCommand::New` guided entry.

User runtime-tests once at the end (per "finish then test").

## Open questions

None blocking. Deferred polish: inline-banner offer instead of `rfd` modal (§5); optional setting to auto-remove clean worktrees on tab close (currently keep-only).
