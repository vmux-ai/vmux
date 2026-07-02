# Auto-tidy agent file previews

Date: 2026-07-02
Status: Design (approved — ready for plan)

Decisions locked: trigger = `Streaming→Idle` gated by count; retention = changed
files + focused; confirm = **native `rfd` dialog** (3 buttons) mirroring the
existing close-confirm; dirtiness = `git status --porcelain` set membership
(no per-file `is_dirty`); close = plain despawn (focused always kept, so the
pane never empties → no collapse).

## Problem

When an agent reads/edits files, each opens a `file://` editor preview in its
follow-pane (`handle_agent_file_touch` → `OpenBesideRequest`). Over a session
these pile up — dozens of tabs, most of them one-off reference reads the agent
never touched again. The pane becomes noise. The signal the user actually cares
about is *files the agent changed*.

## Goal

Automatically close stale, clean file previews in the agent's follow-pane,
keeping only what matters: files with uncommitted diffs, plus whatever the user
is currently looking at. First run asks for confirmation with an
"always allow" escape hatch; after that it's silent.

## Non-goals

- No LRU / max-tab eviction as a general layout feature — this is scoped to the
  agent follow-pane only.
- No touching user-opened tabs in other panes / tabs / spaces.
- No pinned-tab concept (dirty-or-focused is the only retention signal).
- No closing of dirty files, ever, regardless of count.

## Behavior

### Trigger

Fire on the agent `Streaming → Idle` transition (turn end). Detected with the
established `LastRunStateKind` edge pattern (`vmux_agent/src/systems/surface_errors.rs:12-27`)
over `AgentRunState` (`vmux_agent/src/run_state.rs:3-14`), or by piggybacking the
existing `mark_agent_done` / `AgentAttention` flow (`vmux_agent/src/plugin.rs:562-627`).
Turn end is the natural rest moment — the user is reading, not mid-action.

### Gate (threshold)

Only run when the agent's follow-pane holds **more than `tidy_files_max`
(default 5)** `file://` previews. Below the cap → no-op, zero churn. This is the
"at some point it should be tidy" cutoff — small counts are left alone so a
couple of glanced-at reference files don't vanish every turn.

### Retention rule

When tidying, **keep**:

- any **changed** preview — its repo-relative path appears in
  `git status --porcelain=v2` output (Modified / Staged / StagedModified /
  Untracked / Deleted / Conflicted all list a path, so all are kept). "Clean" ==
  the path is absent from the porcelain set. No per-file `FileStatus`/`is_dirty`
  lookup is needed — set membership is the whole test.
- the **focused** preview (active `Stack` in that pane, by `LastActivatedAt` via
  `active_stack_in_pane`), even if clean — never yank the file under the user's
  eyes

**Close** everything else (clean && !focused). If nothing is closable (all
changed, or only the focused one is clean) → no-op, no prompt.

### Confirm-on-first-tidy

Gated by `agent.tidy_files_auto` (default `false`). The prompt is a **native
`rfd::MessageDialog`** (mirrors the existing "Close terminal?" confirm at
`vmux_layout/src/pane.rs:2578`), using `MessageButtons::YesNoCancelCustom` with
labels **"Tidy" / "Always tidy" / "Not now"**. The dialog is popped from an
exclusive system driven by a marker component (the `PendingStackClose` /
`process_pending_stack_closes` pattern at `stack.rs:36` / `pane.rs:2674`), so the
blocking `.show()` never runs inside a normal system.

- `auto == false` and closable set non-empty → tag the pane with a
  `PendingTidy { closable: Vec<Entity> }` marker; the exclusive system pops the
  dialog:
  - **Tidy** — close the tagged stacks; ask again next time
  - **Not now** — drop the marker, close nothing
  - **Always tidy** — persist `agent.tidy_files_auto = true`, then close
- `auto == true` → close silently, no dialog
- Only one `PendingTidy` outstanding per pane — a new Idle trigger while one is
  pending is ignored (no stacked dialogs)

### Close mechanism

Because tidy **always keeps the focused stack**, the follow-pane always retains
≥1 stack — so tidy never triggers pane-collapse or tab-teardown. Every closable
stack is a non-active sibling in a multi-stack pane. Closing one is therefore a
plain `commands.entity(stack).despawn()` (recursive in this Bevy version — takes
the child page/terminal with it, exactly as the `StackCommand::Close` arm does at
`stack.rs:340`). The big active-stack-centric teardown in `handle_stack_commands`
(`stack.rs:298-469`) does **not** need to be refactored or reused.

Seam: a new `CloseStackRequest { stack: Entity }` message in `vmux_layout`
(mirror `OpenBesideRequest`, `pane.rs:941`), written by the agent-side tidy
system and handled in `StackPlugin`. The handler despawns the stack, and clears
`NewStackContext` if it referenced that stack. Defensive guard: if the target is
somehow the last/active stack in its pane, skip it (tidy never selects it).

### Scope

Per agent, its own follow-pane only. Resolve via `AgentFileResolve` /
`file_page_for` (`vmux_agent/src/plugin.rs:774-833`); enumerate child `Stack`s
whose `PageMetadata.url` starts with `file:`.

## Diff signal — native git bridge (the real work)

Today git status runs **only in the editor WASM webview** (bevy_cef `BinReceive`
+ a detached thread → results routed back to a *webview entity*,
`vmux_git/src/plugin.rs:20-59`). There is **no native message-in/out pair** a
Bevy system could use, and no native caller of `runner::status` outside the
webview job pipeline. This feature adds a small native, Bevy-free helper:

- New `parse::changed_paths(porcelain: &str) -> Vec<String>` — same loop as
  `parse_porcelain_v2` (`parse.rs:32-87`) but collects the extracted path on
  every `1 `/`2 `/`u `/`? ` line instead of filtering to one `target_rel`
  (reuse `entry_path`, incl. the `2 ` tab-split for renames).
- New `runner::dirty_set(file: &Path) -> Result<(PathBuf /*repo_root*/, HashSet<String> /*repo-relative changed paths*/), GitError>`
  — `repo_root(file)?`, one `git status --porcelain=v2`, `changed_paths`.
- The tidy system calls `dirty_set` once per distinct repo root among the
  follow-pane's file paths, then tests each preview via `rel(root, path)` set
  membership.

Runs **synchronously** on the main thread at turn-end (Idle is not a hot frame;
typically one repo, a few ms). No `is_dirty`, no `JobKind`, no webview. A
preview whose path is absent from the set (incl. files outside any git repo →
`dirty_set` errors → treated as clean) is closable; untracked files appear as
`? path` → present → kept.

## Config

New keys under the `agent` section (absent == fallback; never auto-seeded per
project convention):

| Key | Default | Meaning |
|---|---|---|
| `agent.tidy_files` | `true` (when `follow_files` on) | feature enabled |
| `agent.tidy_files_max` | `5` | tidy only when previews exceed this |
| `agent.tidy_files_auto` | `false` | skip the confirm prompt |

`tidy_files_auto` is written only on explicit `[Always tidy]` — a real user
choice, not a default. Writer must **merge into the existing `agent` section**
(settings merge is per-section, not per-field — a partial write wipes sibling
fields), and readers use runtime fallback.

## Data flow

```
AgentRunState: Streaming → Idle   (vmux_agent tidy system)
  └─ agent.tidy_files on?
     └─ follow-pane file previews > tidy_files_max?
        └─ runner::dirty_set() per repo root → changed-path set
           └─ closable = previews where (path ∉ set) && !focused
              ├─ closable empty            → no-op
              ├─ tidy_files_auto == true   → CloseStackRequest per closable (→ despawn)
              └─ tidy_files_auto == false  → insert PendingTidy{closable} on the pane
                    └─ exclusive system pops rfd dialog:
                          ├─ Tidy        → CloseStackRequest per closable
                          ├─ Not now     → drop marker
                          └─ Always tidy → apply_settings_update("agent.tidy_files_auto",true)
                                           + SettingsWriteRequest, then CloseStackRequest per closable
```

## Edge cases

- **All previews dirty** → closable empty → no-op (legit WIP never yanked).
- **Focused preview clean** → kept.
- **File outside a git repo** → clean → closable.
- **Deleted file** → git reports Deleted → dirty → kept (stale preview is fine).
- **Multiple repos in one pane** → one status call per distinct repo root, per pass.
- **Rapid Idle transitions** → gate + single-prompt rule keep it idempotent;
  auto-close is a no-op once clean previews are gone.
- **Non-file stacks** in the pane → ignored (only `file:` URLs considered).

## Testing

Bevy system + message integration (per project convention: register message
types + systems in the plugin `build()`, send typed messages, run schedules,
assert ECS state).

- **Pure retention selector** — given `[(path, dirty, focused)]`, returns the
  correct close set. Table-driven.
- **Threshold gate** — count ≤ max → empty; count > max → evaluates.
- **Idle-edge detection** — fires once per `Streaming→Idle`, not on repeats.
- **Porcelain parse** — repo-wide variant over sample `--porcelain=v2` text →
  expected dirty set (incl. untracked `?`, renamed `2`, unmerged `u`).
- **Integration** — spawn agent session + follow-pane + N file `Stack`s, set
  `AgentRunState::Idle`, run schedule; with `auto=true` assert clean-non-focused
  stacks despawn while changed + focused survive.
- **Confirm gating** — `auto=false` → `PendingTidy` inserted, no
  `CloseStackRequest` yet, nothing despawned. (The `rfd` dialog itself is not
  unit-tested — it's a blocking native call; factor the button→action mapping
  into a pure fn `tidy_choice(result) -> TidyAction` and test that instead.)
- **CloseStackRequest handler** — send it for a non-active stack in a 3-stack
  pane → that stack despawns, siblings + active untouched.

## Implementation touchpoints

- `vmux_git/src/parse.rs` — `changed_paths(porcelain) -> Vec<String>` (repo-wide).
- `vmux_git/src/runner.rs` — `dirty_set(file) -> (PathBuf, HashSet<String>)`.
- `vmux_setting/src/plugin/runtime.rs` — add `tidy_files`, `tidy_files_max`,
  `tidy_files_auto` to `AgentSettings` + `default_agent_settings()`; embedded
  default in `settings.ron` (`agent:` block).
- `vmux_layout/src/stack.rs` — `CloseStackRequest { stack: Entity }` message +
  `handle_close_stack_requests` (plain despawn) registered in `StackPlugin`.
- `vmux_layout` (near `pane.rs` close-confirm) — `PendingTidy { closable }`
  marker + exclusive `process_pending_tidy` system popping the `rfd` dialog;
  pure `tidy_choice(rfd result) -> TidyAction`.
- `vmux_agent/src/plugin.rs` + `client/page/plugin.rs` — `LastTidyKind` component
  + attach system; `tidy_on_idle` system (Streaming→Idle edge, gate, resolve
  follow-pane via `AgentFileResolve`, `dirty_set`, decide). Add `vmux_git` dep to
  `vmux_agent` (verify no cycle — `vmux_git` depends only on core/cef).

`CloseStackRequest`/`PendingTidy` live in `vmux_layout` (native). The agent crate
writes `CloseStackRequest` — register the message in `StackPlugin` (owner) and
`.init_resource::<Messages<CloseStackRequest>>()` in `AgentPlugin` if load order
needs it (mirrors `OpenBesideRequest`, `plugin.rs:197`).

## Resolved implementation choices

1. **Native git execution** — **synchronous** `runner::dirty_set` at turn-end.
   Idle is not a hot frame; ~1 repo. Async job deferred (would need a new native
   request/response pair; current job pipeline only routes to webviews).
2. **Dirty-set lifetime** — **recomputed per tidy pass**. No cached `DirtyFiles`
   resource in v1.
3. **Prompt** — **native `rfd` dialog** (see Confirm section). In-app soft-glass
   overlay deferred (no precedent; all rich UI is CEF webviews).

## Out of scope

- General layout tab limits / LRU eviction.
- Pinned tabs.
- Tidying non-agent panes.
- Closing dirty files.
