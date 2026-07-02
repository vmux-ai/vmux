# Auto-tidy agent file previews

Date: 2026-07-02
Status: Design (approved for spec)

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

- any preview whose file `is_dirty` — Modified / Staged / StagedModified /
  Untracked / Deleted / Conflicted (`vmux_git/src/event.rs:66-75`,
  `vmux_git/src/ui.rs:2`)
- the **focused** preview (active `Stack` in that pane, by `LastActivatedAt`),
  even if clean — never yank the file under the user's eyes

**Close** everything else (clean && !focused). If nothing is closable (all
dirty, or only the focused one is clean) → no-op, no prompt.

### Confirm-on-first-tidy

Gated by `agent.tidy_files_auto` (default `false`):

- `auto == false` and closable set non-empty → show a soft-glass confirm
  affordance near the follow-pane:
  **"Tidy N clean previews?"** → `[Tidy]` · `[Not now]` · `[Always tidy]`
  - `[Tidy]` — close now; ask again next time
  - `[Not now]` — skip this pass
  - `[Always tidy]` — persist `agent.tidy_files_auto = true`, then close; silent
    from then on
- `auto == true` → close silently, no prompt
- Only one prompt outstanding at a time — a new Idle trigger while a prompt is
  pending is ignored (no stacking)

### Close mechanism

Route each close through the existing `StackCommand::Close` teardown
(`vmux_layout/src/stack.rs:298-469`) so pane/split collapse and neighbor
re-activation stay correct, and `NewStackContext` is cleared. `StackCommand::Close`
targets the *active* stack, so this needs a **new message that closes a specific
`Stack` entity** funneling into the same collapse logic — not raw `despawn`.

### Scope

Per agent, its own follow-pane only. Resolve via `AgentFileResolve` /
`file_page_for` (`vmux_agent/src/plugin.rs:774-833`); enumerate child `Stack`s
whose `PageMetadata.url` starts with `file:`.

## Diff signal — native git bridge (the real work)

Today git status runs **only in the editor WASM webview** (bevy_cef `BinReceive`
+ a detached thread, `vmux_git/src/plugin.rs:20-59`). The native world that owns
the panes has **no dirty signal**. This feature must add one:

- On tidy, collect the distinct repo roots among the follow-pane's file paths.
- For each repo, run `git status --porcelain=v2` once and build a
  **repo-wide dirty-path set** — extend `parse_porcelain_v2`
  (`vmux_git/src/parse.rs:32-87`) with a variant that collects *all* entry paths
  instead of filtering to a single `target_rel` (the loop already visits them).
- Test each preview path against the set via `is_dirty`.
- Files outside any git repo → treated as clean (closable). Untracked files
  inside a repo → dirty (kept).

Runs native. Prefer an async `JobKind::Status` job (`vmux_git/src/plugin.rs:56`)
over a main-thread `git` spawn to avoid stalling the schedule; a synchronous
`runner::status` call per repo is an acceptable MVP if kept off the hot path.

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
AgentRunState: Streaming → Idle
  └─ agent.tidy_files on?
     └─ follow-pane file previews > tidy_files_max?
        └─ git status --porcelain per repo root → dirty-path set
           └─ closable = previews where !dirty && !focused
              ├─ closable empty            → no-op
              ├─ tidy_files_auto == true   → close each (CloseStack → StackCommand::Close teardown)
              └─ tidy_files_auto == false  → show confirm prompt
                    ├─ [Tidy]        → close each
                    ├─ [Not now]     → skip
                    └─ [Always tidy] → set tidy_files_auto=true (merge), close each
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
  `AgentRunState::Idle`, run schedule; assert clean-non-focused stacks close via
  the teardown path while dirty + focused survive.
- **Confirm flow** — `auto=false` → prompt message emitted, nothing closed;
  simulate `[Always tidy]` → `tidy_files_auto` set (merged) + closed;
  `auto=true` → closed directly, no prompt.

## Implementation touchpoints

- `vmux_agent/src/plugin.rs` — tidy trigger system near `mark_agent_done`; reuse
  `AgentFileResolve`.
- `vmux_git/src/parse.rs` — repo-wide dirty-set variant of `parse_porcelain_v2`.
- `vmux_git/src/plugin.rs` / `runner.rs` — native status path (`JobKind::Status`
  or direct `runner::status`).
- `vmux_layout/src/stack.rs` — new "close specific `Stack`" message → existing
  collapse/teardown.
- `vmux_layout` — soft-glass confirm affordance (native layout UI, **not** a
  webview — avoids bin-listener/page-console pitfalls) + its action messages.
- settings — `agent.tidy_files*` keys + section-merging writer for
  `tidy_files_auto`.

New message types stay native (`vmux_agent` / `vmux_layout`); if any lands in
`vmux_core::event`, cfg-gate the Bevy `Message` to `not(wasm32)`.

## Open implementation choices (decide in the plan)

1. **Native git execution** — async `JobKind::Status` job (non-blocking, more
   plumbing) vs synchronous `runner::status` per repo (simpler, small blocking
   spawn). Lean async.
2. **Dirty-set lifetime** — recomputed per tidy pass (MVP) vs a cached
   `DirtyFiles` resource kept fresh by git events (reused by future features).
3. **Prompt placement** — banner docked in the follow-pane vs a global
   toast-with-actions. Lean in-pane, unobtrusive.

## Out of scope

- General layout tab limits / LRU eviction.
- Pinned tabs.
- Tidying non-agent panes.
- Closing dirty files.
