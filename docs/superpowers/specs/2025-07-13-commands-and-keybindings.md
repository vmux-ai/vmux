# Commands and Keybindings Spec

## Goal

Define the complete command inventory for vmux with Chromium-style keybindings for tab/browser commands and tmux-style chord bindings for pane/space management. Add all command variants to the enum definitions with OS menu accelerators and keybindings. Unimplemented commands are no-op stubs.

## Keybinding System

Two binding mechanisms:

- `accel = "super+t"` on `#[menu(...)]` — OS-level menu accelerator via muda. Shown in native menu bar.
- `#[bind(direct = "super+t")]` or `#[bind(chord = "ctrl+b, v")]` — Custom keybinding system. `direct` for single-combo shortcuts. `chord` for tmux-style leader+key sequences.

Both can coexist on the same variant. `accel` requires `id` and `label` in `#[menu(...)]`.

macOS menu shortcut symbols: Cmd = Cmd, Shift = Shift, Opt = Opt, Ctrl = Ctrl

## Command Definitions

### TabCommand

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| New | tab_new | New Tab | super+t | Cmd+T | | exists |
| Close | tab_close | Close Tab | super+w | Cmd+W | | exists |
| Next | tab_next | Next Tab | super+shift+] | Cmd+Shift+] | | fix: cycle tabs not panes |
| Previous | tab_previous | Previous Tab | super+shift+[ | Cmd+Shift+[ | | fix: cycle tabs not panes |
| SelectIndex1 | tab_select_1 | Select Tab 1 | super+1 | Cmd+1 | | new |
| SelectIndex2 | tab_select_2 | Select Tab 2 | super+2 | Cmd+2 | | new |
| SelectIndex3 | tab_select_3 | Select Tab 3 | super+3 | Cmd+3 | | new |
| SelectIndex4 | tab_select_4 | Select Tab 4 | super+4 | Cmd+4 | | new |
| SelectIndex5 | tab_select_5 | Select Tab 5 | super+5 | Cmd+5 | | new |
| SelectIndex6 | tab_select_6 | Select Tab 6 | super+6 | Cmd+6 | | new |
| SelectIndex7 | tab_select_7 | Select Tab 7 | super+7 | Cmd+7 | | new |
| SelectIndex8 | tab_select_8 | Select Tab 8 | super+8 | Cmd+8 | | new |
| SelectLast | tab_select_last | Select Last Tab | super+9 | Cmd+9 | | new |
| Reopen | tab_reopen | Reopen Closed Tab | super+shift+t | Cmd+Shift+T | | stub |
| Duplicate | tab_duplicate | Duplicate Tab | | | | stub |
| Pin | tab_pin | Pin Tab | | | | stub |
| Mute | tab_mute | Mute Tab | | | | stub |
| MoveToPane | tab_move_to_pane | Move Tab to Pane | | | | stub |

### BrowserCommand

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| PrevPage | browser_prev_page | Back | super+[ | Cmd+[ | | exists |
| NextPage | browser_next_page | Forward | super+] | Cmd+] | | exists |
| Reload | browser_reload | Reload | super+r | Cmd+R | | exists |
| HardReload | browser_hard_reload | Hard Reload | super+shift+r | Cmd+Shift+R | | stub |
| Stop | browser_stop | Stop Loading | | | | stub |
| FocusAddressBar | browser_focus_address_bar | Open Location | super+l | Cmd+L | | stub |
| Find | browser_find | Find | super+f | Cmd+F | | stub |
| ZoomIn | browser_zoom_in | Zoom In | super+= | Cmd+= | | stub |
| ZoomOut | browser_zoom_out | Zoom Out | super+- | Cmd+- | | stub |
| ZoomReset | browser_zoom_reset | Actual Size | super+0 | Cmd+0 | | stub |
| DevTools | browser_dev_tools | Developer Tools | super+alt+i | Cmd+Opt+I | | stub |
| ViewSource | browser_view_source | View Source | super+alt+u | Cmd+Opt+U | | stub |
| Print | browser_print | Print | super+p | Cmd+P | | stub |

### PaneCommand

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| SplitV | split_v | Split Vertically | | | ctrl+b, v | exists |
| SplitH | split_h | Split Horizontally | | | ctrl+b, h | exists |
| Close | close_pane | Close Pane | | | ctrl+b, x | exists |
| Toggle | toggle_pane | Toggle Pane | | | ctrl+b, t | stub |
| Zoom | zoom_pane | Zoom Pane | | | ctrl+b, z | stub |
| SelectLeft | select_pane_left | Select Left Pane | | | ctrl+b, left | stub |
| SelectRight | select_pane_right | Select Right Pane | | | ctrl+b, right | stub |
| SelectUp | select_pane_up | Select Up Pane | | | ctrl+b, up | stub |
| SelectDown | select_pane_down | Select Down Pane | | | ctrl+b, down | stub |
| SwapPrev | swap_pane_prev | Swap Pane Previous | | | ctrl+b, { | stub |
| SwapNext | swap_pane_next | Swap Pane Next | | | ctrl+b, } | stub |
| RotateForward | rotate_forward | Rotate Forward | | | ctrl+b, ctrl+o | stub |
| RotateBackward | rotate_backward | Rotate Backward | | | ctrl+b, alt+o | stub |
| EqualizeSize | equalize_pane_size | Equalize Pane Size | | | ctrl+b, = | stub |
| ResizeLeft | resize_pane_left | Resize Pane Left | | | ctrl+b, alt+left | stub |
| ResizeRight | resize_pane_right | Resize Pane Right | | | ctrl+b, alt+right | stub |
| ResizeUp | resize_pane_up | Resize Pane Up | | | ctrl+b, alt+up | stub |
| ResizeDown | resize_pane_down | Resize Pane Down | | | ctrl+b, alt+down | stub |

### SpaceCommand

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| New | new_space | New Space | | | ctrl+b, c | stub |
| Close | close_space | Close Space | | | ctrl+b, & | stub |
| Next | next_space | Next Space | ctrl+tab | Ctrl+Tab | | stub |
| Previous | prev_space | Previous Space | ctrl+shift+tab | Ctrl+Shift+Tab | | stub |
| Rename | rename_space | Rename Space | | | | stub |

### SideSheetCommand

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| Toggle | toggle_side_sheet | Toggle Side Sheet | | | ctrl+b, s | exists |
| ToggleRight | toggle_side_sheet_right | Toggle Right Sheet | | | | stub |
| ToggleBottom | toggle_side_sheet_bottom | Toggle Bottom Sheet | | | | stub |

### WindowCommand (new)

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| NewWindow | new_window | New Window | super+n | Cmd+N | | stub |
| CloseWindow | close_window | Close Window | super+shift+w | Cmd+Shift+W | | stub |
| Minimize | minimize_window | Minimize | super+m | Cmd+M | | stub |
| ToggleFullscreen | toggle_fullscreen | Toggle Fullscreen | ctrl+super+f | Ctrl+Cmd+F | | stub |
| Settings | open_settings | Settings | super+, | Cmd+, | | stub |

### CameraCommand (unchanged)

| Variant | menu id | label | accel | Menu Shortcut | bind | Handler |
|---------|---------|-------|-------|---------------|------|---------|
| Reset | reset_camera | Reset Camera | | | | exists |
| ToggleFreeCamera | toggle_free_camera | Toggle Free Camera | | | | exists |

## Behavioral Changes

### Tab Next/Previous Fix

Currently `on_pane_cycle` in `pane.rs` intercepts `TabCommand::Next` and `TabCommand::Previous` to cycle between panes. This must change:

- `TabCommand::Next/Previous` cycles tabs within the active pane
- Pane cycling moves to `PaneCommand` variants (e.g. SelectLeft/Right or a dedicated cycle command)

The `on_pane_cycle` system should stop listening for `TabCommand::Next/Previous`. Instead, `handle_tab_commands` in `tab.rs` should handle Next/Previous by moving Active between Tab siblings within the active pane.

### Tab SelectIndex

`SelectIndex1..8` activates the Nth tab (0-indexed: index 0..7) in the active pane. If the index exceeds tab count, no-op. `SelectLast` activates the last tab regardless of count.

Implementation in `handle_tab_commands`: query children of active pane, filter to Tab entities, sort by entity bits (stable ordering), pick by index, swap Active.

## Scope

### Implemented in this change

- All command enum variants added with `#[menu(...)]` attributes
- All `accel` values on variants that have them
- All `#[bind(...)]` values on variants that have them
- `TabCommand::Next/Previous` handler: cycle tabs in active pane
- `TabCommand::SelectIndex1..8/SelectLast` handler: select tab by index
- Remove `on_pane_cycle` interception of `TabCommand::Next/Previous`
- `BrowserCommand` accel values on existing PrevPage/NextPage/Reload

### Stub only (no-op match arm)

Everything marked "stub" in the tables above.

## Files Changed

- `crates/vmux_desktop/src/command.rs` — all enum definitions, new WindowCommand
- `crates/vmux_desktop/src/layout/tab.rs` — Next/Previous/SelectIndex handlers
- `crates/vmux_desktop/src/layout/pane.rs` — remove on_pane_cycle TabCommand listener
- `crates/vmux_desktop/src/layout/side_sheet.rs` — new SideSheetCommand variants
- `crates/vmux_desktop/src/layout/space.rs` — new SpaceCommand variant (Rename)
