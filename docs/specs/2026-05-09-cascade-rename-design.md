# Cascade rename: Session/Space/Tab → Space/Tab/Stack + zen toggle

**Linear:** [VMX-108](https://linear.app/vmux/issue/VMX-108/cascade-rename-sessionspacetab-spacetabstack-zen-toggle)
**Date:** 2026-05-09
**Status:** Approved

## Motivation

The current naming is borrowed from tmux (`Session`, `Space`, `Pane`, `Tab`) and reads awkwardly in vmux's UI surface. Users see "tabs" at multiple levels and conflate the workspace and the page-inside-a-pane. Promote each layer up one slot so the words match what the user sees:

| Today | After | Concept |
|-------|-------|---------|
| `Session` | `Space` | Top-level workspace, persistence root |
| `Space` | `Tab` | Container of panes, what `cmd+t` opens |
| `Pane` | `Pane` | Split-tree container (unchanged) |
| `Tab` | `Stack` | Z-stacked browser/terminal page inside a pane |

While we're in there: rework the keybindings to surface the new structure, and add a one-shot zen toggle (`cmd+shift+s`) that hides all chrome at once. Individual chrome toggles are removed.

## Hierarchy

```
Space (was Session, persistence root)
  └─ Tab (was Space, cmd+t to open)
       └─ Pane (split-tree container, unchanged)
            └─ Stack (was Tab, cmd+n pushes new on top)
```

Multiple Stacks per Pane. Only the top Stack is visible (z-stacked). `cmd+[` / `cmd+]` cycle within the pane.

## Rename map

### Crates

| Today | After |
|-------|-------|
| `vmux_session` | `vmux_space` |

### Files / modules in `vmux_layout`

| Today | After |
|-------|-------|
| `space.rs` + `space/` | `tab.rs` + `tab/` |
| `tab.rs` + `tab/` | `stack.rs` + `stack/` |

### Types

| Today | After |
|-------|-------|
| `SessionRecord`, `SessionRegistry`, `SessionCommand` | `SpaceRecord`, `SpaceRegistry`, `SpaceCommand` |
| `Space`, `SpaceCommand` | `Tab`, `TabCommand` |
| `Tab`, `FocusedTab`, `TabCommand`, `TabCommandSet`, `PendingTabClose` | `Stack`, `FocusedStack`, `StackCommand`, `StackCommandSet`, `PendingStackClose` |
| `active_pane_in_space`, `active_tab_in_pane`, `first_tab_in_pane` | `active_pane_in_tab`, `active_stack_in_pane`, `first_stack_in_pane` |

`Pane`, `PaneSplit`, `PaneSplitDirection`, `PaneSize`, `PaneCommand` are **unchanged**.

### type_path attributes

`#[type_path = "..."]` annotations are updated to match the new module/type names. They are not pinned to old paths — see Persistence.

### vmux:// URL scheme

The internal CEF scheme paths follow the type rename:

| Today | After |
|-------|-------|
| `vmux://sessions/` | `vmux://spaces/` |

Affected files:

- `crates/vmux_session/src/event.rs:1` — `SESSIONS_WEBVIEW_URL` const → `SPACES_WEBVIEW_URL`
- `crates/vmux_command/src/results.rs:3-4` — `SESSIONS_QUERY`, `SESSIONS_PAGE_URL` consts → `SPACES_*`
- `crates/vmux_command/src/results.rs:237,276` — test strings
- `crates/vmux_desktop/src/command_bar.rs:1918` — string literal
- `crates/vmux_desktop/src/sessions.rs` (file rename → `spaces.rs`)
- CEF scheme handler registration (search for `"sessions"` in scheme setup)

Other `vmux://` paths (`terminal`, `services`, `command-bar`, `header`, `footer`) are unaffected — they refer to component types not in the rename map.

## Persistence

The product is pre-release; no users depend on saved state. We accept data loss instead of writing migration code.

On first launch after upgrade:

1. If `~/.vmux/sessions.ron` exists, rename it to `~/.vmux/sessions.ron.bak`.
2. For each `~/.vmux/profiles/*/session.ron` and `~/.vmux/sessions/*/session.ron`, rename to `*.bak`.
3. Log a one-line warning so users know where their old files went.
4. Boot fresh.

The `.bak` rename happens once at startup, gated by a marker file (`~/.vmux/.cascade_rename_done`) so we don't repeatedly try on every launch.

The `vmux_layout::lib.rs` test that asserts `Space::type_path() == "vmux_desktop::layout::space::Space"` is updated to assert the new paths.

## Keybindings

### After

| Key | Command |
|-----|---------|
| `cmd+t` | `TabCommand::New` (opens a new Tab — top-level workspace) |
| `cmd+n` | `StackCommand::New` (pushes a new Stack on top in the focused Pane) |
| `cmd+w` | `StackCommand::Close` (closes the focused Stack, with existing close-confirmation dialog) |
| `cmd+[` / `cmd+]` | prev/next Stack in the focused Pane |
| `cmd+shift+[` / `cmd+shift+]` | prev/next Tab |
| `cmd+1`..`cmd+9` | jump to Tab #N |
| `cmd+shift+s` | `ZenCommand::Toggle` — hide all chrome / show all chrome |

### Removed

| Key | Was |
|-----|-----|
| `super+s` | `SideSheetCommand::Toggle` |
| `super+shift+h` | `HeaderCommand::Toggle` |
| `super+shift+f` | `FooterCommand::Toggle` |
| `Ctrl+g` chord (was Space cmds) | freed entirely |

The commands themselves (`SideSheetCommand::Toggle`, `HeaderCommand::Toggle`, `FooterCommand::Toggle`) are deleted from the codebase. The chrome components remain — they're now toggled only via `ZenCommand::Toggle` (collectively).

`cmd+w` retains today's `Tab::Close` behavior (renamed to `StackCommand::Close`) — close the focused Stack, prompt confirmation for last-of-its-kind closures via the existing dialog. No new multi-level cascade is introduced in this PR.

## Zen toggle

Single command: `ZenCommand::Toggle`.

State: a single `bool` `ZenMode` resource. No snapshot of prior state.

- **Off → On:** hide Header, SideSheet, Footer.
- **On → Off:** show Header, SideSheet, Footer.

Default chrome state on app launch: **all visible** (Header + SideSheet + Footer). The footer-hidden default that exists today is dropped, since users no longer have a way to toggle individual chrome anyway.

## Implementation strategy

One PR, but ordered commits for bisect:

1. **Types: `Tab → Stack`** (smallest blast radius, leaf of the hierarchy)
   - Rename file `vmux_layout/src/tab.rs → stack.rs` + `tab/ → stack/`
   - Rename `Tab` struct, `FocusedTab`, `TabCommand`, etc.
   - Update `#[type_path]`, lib.rs test
2. **Types: `Space → Tab`**
   - Rename file `vmux_layout/src/space.rs → tab.rs` + `space/ → tab/`
   - Rename `Space` struct, `SpaceCommand`, etc.
   - Update `#[type_path]`, lib.rs test
3. **Crate: `vmux_session → vmux_space`**
   - Rename crate directory and `Cargo.toml` package name
   - Rename `SessionRecord`, `SessionRegistry`, `SessionCommand` types
   - Rename `vmux://sessions/ → vmux://spaces/` (and constants)
   - Rename `vmux_desktop/src/sessions.rs → spaces.rs`
   - Update workspace `Cargo.toml` and all `vmux_session = ...` deps
   - Update `#[type_path]`, lib.rs test
4. **Persistence cleanup**
   - Add startup logic to move old `*.ron` files to `*.bak`, gated by marker file
5. **Keybinding rework**
   - Reassign `cmd+t`, `cmd+n`, add `cmd+[`/`]`, `cmd+shift+[`/`]`, `cmd+1..9`
   - Remove `Ctrl+g` chord, `super+s`, `super+shift+h`, `super+shift+f` bindings
6. **Zen toggle + delete individual chrome toggles**
   - Add `ZenCommand::Toggle` and `ZenMode` resource
   - Bind to `cmd+shift+s`
   - Delete `HeaderCommand::Toggle`, `FooterCommand::Toggle`, `SideSheetCommand::Toggle` and call sites
   - Default all chrome to visible on launch

`make lint` and `make test` must pass after each commit (so the branch is bisectable).

## Testing

- Existing unit tests are renamed/updated as types change.
- The `vmux_layout::lib.rs` type_path assertion test is kept (and its expected strings updated to the new paths) — it proves type_path still serializes the names we expect.
- Manual smoke test before merge:
  - Open a fresh app (no `~/.vmux/`): verify Space, Tab, Pane, Stack all spawn correctly
  - `cmd+t` opens new Tab; `cmd+n` pushes new Stack; `cmd+[`/`]` cycles
  - `cmd+shift+s` hides everything; pressing again brings everything back
  - Old `~/.vmux/` from a prior install: verify `.bak` rename happens, no crash
- `make lint` and `make test` pass on the final commit.

## Out of scope

- Migration of old `session.ron` files (deferred indefinitely; pre-release).
- Stack tab-bar UI (visual indicator of how many stacks are in a pane). Filed as a follow-up if needed.
- Settings entries to configure default chrome visibility.
- Renaming `vmux_session` → `vmux_space` references in commit history, changelogs, or docs older than this PR.
