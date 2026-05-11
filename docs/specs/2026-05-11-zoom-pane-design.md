# Zoom Pane (tmux-style) — Design

**Linear issue:** [VMX-112](https://linear.app/vmux/issue/VMX-112/zoom-pane-tmux-style-toggle-to-fill-main-area)
**Branch:** `vmx-112-zoom-pane`

## Goal

Implement a tmux-style "zoom pane" command that toggles the focused leaf pane to fill the entire tab's main area, hiding sibling panes. Bound to `<leader> z` (chord `Ctrl+G, z`), already wired as `PaneCommand::Zoom` with a no-op handler at `crates/vmux_layout/src/pane.rs:418`.

## Behavior (tmux parity)

- `<leader> z` toggles zoom on the focused leaf pane.
- While zoomed, the focused pane fills the tab's main area; siblings are hidden via `Display::None`.
- Pane navigation (`Focus{Up/Down/Left/Right}`) auto-unzooms first, then performs the navigation in one keypress.
- Splitting inside the zoomed pane auto-unzooms first.
- Closing the zoomed pane clears the zoom state automatically.
- Tab switch preserves zoom per-tab. Each tab carries its own zoom state independently — zooming in tab A does not affect tab B, and switching from A to B and back to A leaves A still zoomed.
- Zooming a pane with no siblings is a no-op (no state stored, no visual change).
- Visual indicator: a small "Z" pill appears in the header chrome, immediately to the right of the address bar, when the active tab is zoomed.

## Non-goals

- No persisted zoom state across sessions (zoom is ephemeral, like tmux).
- No animation or transition.
- No "zoom-out by clicking outside" — only the toggle key, navigation, split, or pane removal exits zoom.

## Approach

Hide siblings via Bevy's `Display::None` rather than restructuring the pane tree.

tmux saves the previous layout to `window->saved_layout` and rebuilds the active layout with only the zoomed pane, because text-based layouts have no notion of "hidden but present." Bevy's flex layout supports `Display::None`, which removes a node from layout calculations while keeping its entity in the ECS tree. Same observable behavior, less risk than reparenting.

## Architecture

### State

A new component on the `Tab` entity:

```rust
// crates/vmux_layout/src/pane.rs
#[derive(Component, Debug)]
pub struct Zoomed {
    /// The leaf pane that is currently zoomed.
    pub leaf: Entity,
    /// Entities whose `Display` we set to `None`. Captured so unzoom can be exact.
    hidden: SmallVec<[Entity; 8]>,
}
```

One `Zoomed` per tab → tmux-style per-window state.

### Command handler (`handle_pane_commands`, `pane.rs:245`)

Match arm at `pane.rs:418`:

```rust
PaneCommand::Zoom => toggle_zoom(...)
```

`toggle_zoom`:
1. Resolve focused tab + focused leaf pane via `FocusedStack`.
2. If the tab has `Zoomed` → remove it (the `OnRemove` hook restores visibility).
3. Else collect the set of sibling entities to hide (see algorithm below) and insert `Zoomed { leaf, hidden }`. If the set is empty, return without inserting.

Auto-unzoom is added at the start of these handlers (before their normal logic):

- `PaneCommand::FocusUp | FocusDown | FocusLeft | FocusRight` → remove `Zoomed` from the focused tab if present.
- `PaneCommand::SplitH | SplitV` → remove `Zoomed` from the focused tab if present.

### Render system (`sync_zoom_visibility`, new)

- Scheduled in `PostUpdate`, before `bevy::ui::UiSystems::Layout` (same slot as `sync_header_visibility` in `crates/vmux_layout/src/header.rs:13`).
- Runs only when a `Zoomed` component is added or changed.
- Reads the `hidden` list and sets `Display::None` on every listed entity's `Node`.

### Restore on removal

Bevy's `RemovedComponents<Zoomed>` only yields entity IDs, not previous component data, so the `hidden` list must be captured before removal. Use a component lifecycle hook:

```rust
.world_mut()
    .register_component_hooks::<Zoomed>()
    .on_remove(|mut world, ctx| {
        // Read the Zoomed about to be removed, push its hidden list into a
        // RestoreVisibility resource queue, then a system reads the queue
        // and resets Display::Flex on each entity.
    });
```

A small resource `PendingZoomRestores(Vec<SmallVec<[Entity; 8]>>)` collects pending restores; a system consumes it in `PostUpdate` before `UiSystems::Layout`.

### Auto-clear on pane removal

`OnRemove` hook on `Pane`: scan all tabs; if any `Zoomed.leaf == removed_entity`, remove that `Zoomed`. The existing visibility restore path then handles cleanup.

### Algorithm: collect siblings to hide

```
fn siblings_to_hide(world, leaf, tab) -> Vec<Entity>:
    result = []
    cur = leaf
    while cur != tab:
        parent = world.parent(cur)
        if world.has::<PaneSplit>(parent):
            for child in world.children(parent):
                if child != cur:
                    result.push(child)
        cur = parent
    return result
```

Walk from leaf to tab root. At every `PaneSplit` ancestor, every sibling of the path becomes hidden. The path itself stays visible (which means the split chain remains in layout, but only the zoomed leaf's branch shows content).

### Indicator (header chrome)

Two changes:

1. `crates/vmux_layout/src/event.rs` — extend `TabsHostEvent`:
   ```rust
   pub struct TabsHostEvent {
       pub tabs: Vec<TabRow>,
       pub can_go_back: bool,
       pub can_go_forward: bool,
       pub is_zoomed: bool,  // new
   }
   ```

2. The system that publishes `TabsHostEvent` reads the focused tab's `Zoomed` component and sets `is_zoomed`.

3. `crates/vmux_layout/src/app.rs` `HeaderView`: render a small "Z" pill placed as the next sibling after `HeaderAddressBar` (currently at `app.rs:225`) inside the same flex row. The row already uses `gap-1`, so no extra margin is needed.
   ```rsx
   if is_zoomed {
       span {
           class: "inline-flex h-5 items-center rounded px-1.5 text-ui-xs font-mono bg-glass-hover text-foreground",
           title: "Pane zoomed",
           "Z"
       }
   }
   ```

## Data flow

```
key chord (Ctrl+G, z)
  → process_key_input writes AppCommand::Pane(PaneCommand::Zoom)
  → handle_pane_commands: toggle Zoomed component on focused Tab
  → sync_zoom_visibility (or restore hook): mutate sibling Node.display
  → Bevy UI layout pass: zoomed pane fills available space
  → focus ring follows (already driven by FocusedStack/LastActivatedAt)
  → publish TabsHostEvent { is_zoomed: true } to layout chrome webview
  → Dioxus HeaderView renders "Z" pill
```

## Testing

Bevy ECS unit tests in `pane.rs`:

- `zoom_hides_all_siblings_along_ancestor_path`
- `unzoom_restores_all_hidden_panes`
- `focus_navigation_auto_unzooms`
- `split_in_zoomed_unzooms_first`
- `closing_zoomed_pane_clears_zoom`
- `single_pane_zoom_is_noop`
- `tab_switch_preserves_zoom_per_tab`

Set up a minimal Bevy `App` with the layout plugin systems registered, spawn a tab with a known split tree, dispatch `AppCommand`s through the messages channel, run `app.update()`, then assert on `Zoomed` components and `Node.display` values. Existing `pane.rs` tests demonstrate the pattern.

## Files Touched

| File | Change |
|------|--------|
| `crates/vmux_layout/src/pane.rs` | `Zoomed` component, toggle logic, `sync_zoom_visibility`, auto-unzoom in handlers, `OnRemove` hooks |
| `crates/vmux_layout/src/lib.rs` | Register `sync_zoom_visibility` system + `PendingZoomRestores` resource + hooks |
| `crates/vmux_layout/src/event.rs` | `is_zoomed: bool` on `TabsHostEvent` |
| `crates/vmux_layout/src/chrome.rs` (or wherever the publisher lives) | Set `is_zoomed` from focused tab's `Zoomed` |
| `crates/vmux_layout/src/app.rs` | "Z" pill in `HeaderView` |

## Open questions

None — tmux behavior fully specified above.

## Notes

- The user originally typed the binding as `<leader>,z` (vim-style notation). This maps to the chord `Ctrl+G, z` already wired as `PaneCommand::Zoom`. Throughout this spec, `<leader> z` and the chord `Ctrl+G, z` refer to the same key sequence.

## Acceptance criteria

- [ ] `<leader> z` toggles zoom on the focused pane.
- [ ] Sibling panes are hidden (`Display::None`) while zoomed; restored on unzoom.
- [ ] Pane navigation while zoomed auto-unzooms.
- [ ] Splitting a zoomed pane auto-unzooms first.
- [ ] Closing the zoomed pane clears zoom state.
- [ ] Tab switch preserves per-tab zoom.
- [ ] No-op when focused pane has no siblings.
- [ ] "Z" indicator appears in header chrome when active tab is zoomed.
- [ ] Unit tests cover all of the above.
- [ ] `cargo fmt -p vmux_layout -- --check`, `cargo clippy -p vmux_layout --all-targets -- -D warnings`, and `cargo test -p vmux_layout` pass.
