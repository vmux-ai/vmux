# OSC-driven dynamic tab titles

## Problem

Terminal programs (claude, codex, vibe, plus plain CLIs) emit OSC title escape
sequences (`ESC ] 0 ; … BEL`, `ESC ] 2 ; … BEL`) to dynamically rename their
host window — the same mechanism tmux uses to update window titles. vmux already
parses these sequences but does not surface them on the tab.

Today the title escape lands as `ServiceMessage::ProcessTitle` and is forwarded
only to the terminal webview's inner document `<title>` (invisible). The visible
tab label comes from `PageMetadata.title`, which is set once to a static default
(`Terminal (<cwd>)`, `Terminal (<pid>)`, or the agent identity `Claude (<sid>)`)
and never reflects the running program's live title.

## Goal

Let any PTY program drive its tab label via OSC titles, reverting to the default
label when the program clears the title or exits.

## Scope

- **In:** all PTY terminals — plain shells and agent tabs (claude/codex/vibe).
- **Out:** macOS native window title (`set-titles` equivalent), OSC-driven
  favicon/color, replaying the last title to the GUI after an app restart that
  re-attaches to a still-live process (self-heals on the program's next emit).

## Behavior

- A non-empty OSC title overrides the default tab label while the program runs.
- An empty OSC title (`ESC ] 2 ; BEL`) clears the override → revert to default.
- Process exit clears the override → revert to default.
- The default label keeps being computed/written exactly as today; the OSC title
  is an overlay on top of it, not a replacement of the default writers.

This matches tmux `allow-rename` semantics (program rename wins) plus an explicit
revert-to-default on clear/exit.

## Existing data flow (unchanged)

```
PTY bytes
  → alacritty_terminal Processor parses OSC 0/2
  → TermEvent::Title(String)                          process.rs:40
  → ServiceMessage::ProcessTitle { process_id, title} process.rs:41
  → client service-message router                     vmux_terminal plugin.rs:1079
```

## Design (Approach A — non-persisted override component)

### Why a separate component, not `PageMetadata`

`PageMetadata` is reflection-serialized into `space.ron`
(`persistence.rs` allowlist includes `PageMetadata`) and auto-save is triggered by
`Changed<PageMetadata>` (`persistence.rs` `mark_dirty_on_change`). Writing live
OSC titles into `PageMetadata.title` would therefore (a) persist ephemeral titles
(e.g. a spinner frame) to disk, reappearing stale on next launch, and (b) thrash
auto-save on every title tick. A field inside `PageMetadata` has the same
`Changed<PageMetadata>` thrash problem. A dedicated component sidesteps both.

### Component

```rust
/// Live OSC (0/2) title for a terminal, overriding the default tab label.
/// Absent when no program-set title is active. Never persisted.
#[derive(Component)]
pub struct OscTitle(pub String);
```

- Defined in `vmux_core` (next to `PageMetadata`) so both `vmux_terminal` (writer)
  and `vmux_browser` (reader) can use it without a new cross-crate dependency.
- Lives on the terminal **content** entity. That single entity already carries
  `Terminal + Browser + ProcessId + PageMetadata` together
  (`vmux_terminal` plugin.rs:592-605), so it is both addressable by `ProcessId`
  (writer) and visible to the `With<Browser>` emit queries (readers).
- **Not** added to the `save.components` allowlist → never written to `space.ron`.

### Systems / changes

1. **Set/clear override** — extend the `ServiceMessage::ProcessTitle` arm
   (`vmux_terminal` plugin.rs:1079). The arm already iterates the
   `terminals: Query<(Entity, &ProcessId, &ChildOf), With<Terminal>>` to match
   `process_id`. On match: if `title` is non-empty, insert/update
   `OscTitle(title)` on that entity; if empty, remove `OscTitle`. Keep the
   existing inner `doc.set_title` emit.
2. **Revert on exit** — in the `ServiceMessage::ProcessExited` arm
   (plugin.rs:1129) remove `OscTitle`. Also remove it in
   `reset_terminal_title_on_agent_removed` (plugin.rs:2502) so an agent→plain
   transition starts clean.
3. **Render overlay** — the visible tab strip is emitted from `vmux_browser`,
   which reads `PageMetadata` on the `With<Browser>` content entity in three
   functions. Each gains `Option<&OscTitle>` in its `browser_meta` query and uses
   the OSC string when present, else `meta.title`:
   - `push_stacks_host_emit` (lib.rs:2007, `STACKS_EVENT`)
   - `push_pane_tree_emit` (lib.rs:2099, `PANE_TREE_EVENT`)
   - `push_tabs_host_emit` (lib.rs:2209, `TABS_EVENT`)
   Each already diffs its serialized payload before emitting, so feeding the
   effective title into the payload is enough to push live updates.
   `vmux_layout/src/snapshot.rs` (`StackDto`/`LayoutSnapshot`) is **not** the
   visible strip — it feeds MCP (`vmux_mcp/src/tools.rs`) and layout
   reconcile — and is left on the default title (out of scope).

### Precedence

Pure overlay. Default writers (terminal spawn cwd/pid label, agent
`format_agent_url` identity label, `reset_terminal_title_on_agent_removed`) keep
writing `PageMetadata.title` untouched. `OscTitle` wins at render only while
present. No guards needed on the default writers.

### Persistence

`OscTitle` excluded from the save allowlist → `space.ron` always stores the
stable default. After restart the tab shows the default until the program emits a
new OSC title. OSC updates never mark `PageMetadata` changed → no auto-save churn.

## Testing

- Non-empty `ProcessTitle` inserts `OscTitle`; effective tab label = OSC string.
- Empty `ProcessTitle` removes `OscTitle`; effective label = default.
- `ProcessExited` removes `OscTitle`; effective label = default.
- An OSC update does not set `Changed<PageMetadata>` (auto-save stays clean).
- Renderer overlay: with `OscTitle` present the emitted `StackNode`/`StackDto`
  title is the OSC string; without it, the `PageMetadata.title` default.

## Files touched

- `crates/vmux_core/src/lib.rs` — new `OscTitle` component.
- `crates/vmux_terminal/src/plugin.rs` — set/clear `OscTitle` in the
  `ProcessTitle` arm; remove it in the `ProcessExited` arm and in
  `reset_terminal_title_on_agent_removed`.
- `crates/vmux_browser/src/lib.rs` — overlay `OscTitle` over `meta.title` in
  `push_stacks_host_emit`, `push_pane_tree_emit`, `push_tabs_host_emit`.
- No change to `crates/vmux_service` (source path already emits `ProcessTitle`).
- No change to `crates/vmux_layout/src/snapshot.rs` (MCP/reconcile keep default).
- No change to `persistence.rs` (omission from allowlist is the intended behavior).
