# Loading Indicator Design

## Overview

Show a visual loading state when the active pane's browser is loading a page. Two effects:

1. **Progress bar** — thin indeterminate bar at the top of the active pane
2. **Focus ring speed-up** — the existing gradient animation runs faster during load

Only the active pane shows loading indicators. Inactive/background pane loading has no visual effect.

## Data Flow

```
CEF on_loading_state_change(is_loading: bool)
  → WebviewLoadingStateEvent { webview: Entity, is_loading }
  → async_channel → WebviewLoadingStateReceiver (already exists, currently unread)
  → drain_loading_state system (Update)
    → insert Loading on Browser entity when is_loading=true
    → remove Loading from Browser entity when is_loading=false
```

## Components

| Name | Type | Entity | Purpose |
|------|------|--------|---------|
| `Loading` | Marker component | Browser | Tracks which browsers are currently loading |

No `LoadingBar` component — the bar is a child `Node` of the `Tab` entity, shown/hidden based on `Loading` presence.

## Visual: Progress Bar

- **Position**: top edge of the active pane, absolutely positioned inside the Tab
- **Height**: 2px
- **Color**: accent blue from settings (`layout.pane.outline.gradient.accent`)
- **Z-index**: above webview content
- **Animation**: indeterminate left-to-right sweep — a narrow highlight slides across using `Node.left` animated by elapsed time
- **Lifecycle**: spawned as child of Tab alongside the Browser entity; visibility toggled by Loading presence on active pane only

## Visual: Focus Ring Speed-Up

The existing focus ring shader has `gradient_params.y` which controls gradient animation speed (default: 1.2 from settings). When the active pane's browser has `Loading`:

- Multiply speed by 3x (e.g. 1.2 -> 3.6)
- When loading finishes, restore to normal speed

This is done in `build_focus_ring_material` or `sync_focus_ring_to_active_pane` by checking if the active pane's browser has `Loading`.

## Systems

### `drain_loading_state` (Update)

```rust
fn drain_loading_state(
    receiver: Res<WebviewLoadingStateReceiver>,
    mut commands: Commands,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if ev.is_loading {
            commands.entity(ev.webview).insert(Loading);
        } else {
            commands.entity(ev.webview).remove::<Loading>();
        }
    }
}
```

### `sync_loading_bar` (PostUpdate, after UI layout)

- Find the active pane's active tab's browser entity
- If it has `Loading`, show the loading bar node (set `Visibility::Visible`, animate position)
- If not, hide it (`Visibility::Hidden`)

### Focus ring integration

In `sync_focus_ring_to_active_pane` (already exists in `focus_ring.rs`):

- Query for `Loading` on the active pane's browser
- If loading, set `gradient_params.y = settings.speed * 3.0`
- If not, use `settings.speed` as-is

## Files to Change

| File | Change |
|------|--------|
| `crates/vmux_desktop/src/browser.rs` | Add `Loading` component, `drain_loading_state` system, register in `BrowserPlugin` |
| `crates/vmux_desktop/src/layout/focus_ring.rs` | Query `Loading` in `sync_focus_ring_to_active_pane`, multiply speed when active browser is loading |
| `crates/vmux_desktop/src/layout/tab.rs` or `browser.rs` | Spawn loading bar Node as child of Tab, add `sync_loading_bar` system |

## Out of Scope

- Real progress percentage (requires implementing `DisplayHandler::on_loading_progress_change` in bevy_cef_core)
- Loading indicators on inactive/background panes
- Loading state in header or sidebar UI
