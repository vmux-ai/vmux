# Commands and Keybindings Spec

## Goal

Define the complete command inventory for vmux with Chromium-style keybindings for tab/browser commands and tmux-style chord bindings for pane/space management. Add all command variants to the enum definitions with OS menu accelerators and keybindings. Unimplemented commands are no-op stubs.

## Keybinding System

Two binding mechanisms:

- `accel = "super+t"` on `#[menu(...)]` — OS-level menu accelerator via muda. Shown in native menu bar.
- `#[bind(direct = "super+t")]` or `#[bind(chord = "<leader>, v")]` — Custom keybinding system. `direct` for single-combo shortcuts. `chord` for tmux-style leader+key sequences.

Both can coexist on the same variant. `accel` requires `id` and `label` in `#[menu(...)]`.

For commands with chord bindings but no `accel`, the chord is embedded in the menu label text itself, e.g. `"Split Vertically\t<leader>, V"`. This displays the chord right-aligned in the menu item since native accelerators don't support two-step sequences.

## Handler Status Legend

- ✅ Implemented
- 🔲 Stub (no-op `{}` match arm)

## Command Definitions

### TabCommand

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| New | tab_new | New Tab | super+t | New Tab  Cmd+T | | ✅ |
| Close | tab_close | Close Tab | super+w | Close Tab  Cmd+W | | ✅ |
| Next | tab_next | Next Tab | super+shift+] | Next Tab  Cmd+Shift+] | | ✅ |
| Previous | tab_previous | Previous Tab | super+shift+[ | Previous Tab  Cmd+Shift+[ | | ✅ |
| SelectIndex1 | tab_select_1 | Select Tab 1 | super+1 | Select Tab 1  Cmd+1 | | ✅ |
| SelectIndex2 | tab_select_2 | Select Tab 2 | super+2 | Select Tab 2  Cmd+2 | | ✅ |
| SelectIndex3 | tab_select_3 | Select Tab 3 | super+3 | Select Tab 3  Cmd+3 | | ✅ |
| SelectIndex4 | tab_select_4 | Select Tab 4 | super+4 | Select Tab 4  Cmd+4 | | ✅ |
| SelectIndex5 | tab_select_5 | Select Tab 5 | super+5 | Select Tab 5  Cmd+5 | | ✅ |
| SelectIndex6 | tab_select_6 | Select Tab 6 | super+6 | Select Tab 6  Cmd+6 | | ✅ |
| SelectIndex7 | tab_select_7 | Select Tab 7 | super+7 | Select Tab 7  Cmd+7 | | ✅ |
| SelectIndex8 | tab_select_8 | Select Tab 8 | super+8 | Select Tab 8  Cmd+8 | | ✅ |
| SelectLast | tab_select_last | Select Last Tab | super+9 | Select Last Tab  Cmd+9 | | ✅ |
| Reopen | tab_reopen | Reopen Closed Tab | super+shift+t | Reopen Closed Tab  Cmd+Shift+T | | 🔲 |
| Duplicate | tab_duplicate | Duplicate Tab | | Duplicate Tab | | 🔲 |
| Pin | tab_pin | Pin Tab | | Pin Tab | | 🔲 |
| Mute | tab_mute | Mute Tab | | Mute Tab | | 🔲 |
| MoveToPane | tab_move_to_pane | Move Tab to Pane | | Move Tab to Pane | | 🔲 |

### BrowserCommand

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| PrevPage | browser_prev_page | Back | super+[ | Back  Cmd+[ | | ✅ |
| NextPage | browser_next_page | Forward | super+] | Forward  Cmd+] | | ✅ |
| Reload | browser_reload | Reload | super+r | Reload  Cmd+R | | ✅ |
| HardReload | browser_hard_reload | Hard Reload | super+shift+r | Hard Reload  Cmd+Shift+R | | ✅ |
| Stop | browser_stop | Stop Loading | | Stop Loading | | 🔲 |
| FocusAddressBar | browser_focus_address_bar | Open Location | super+l | Open Location  Cmd+L | | 🔲 |
| Find | browser_find | Find | super+f | Find  Cmd+F | | 🔲 |
| ZoomIn | browser_zoom_in | Zoom In | super+= | Zoom In  Cmd+= | | ✅ |
| ZoomOut | browser_zoom_out | Zoom Out | super+- | Zoom Out  Cmd+- | | ✅ |
| ZoomReset | browser_zoom_reset | Actual Size | super+0 | Actual Size  Cmd+0 | | ✅ |
| DevTools | browser_dev_tools | Developer Tools | super+alt+i | Developer Tools  Cmd+Opt+I | | ✅ |
| ViewSource | browser_view_source | View Source | super+alt+u | View Source  Cmd+Opt+U | | 🔲 |
| Print | browser_print | Print | super+p | Print  Cmd+P | | 🔲 |

### PaneCommand

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| SplitV | split_v | Split Vertically\t<leader>, % | | Split Vertically  <leader>, % | <leader>, % | ✅ |
| SplitH | split_h | Split Horizontally\t<leader>, " | | Split Horizontally  <leader>, " | <leader>, " | ✅ |
| Close | close_pane | Close Pane\t<leader>, X | | Close Pane  <leader>, X | <leader>, x | ✅ |
| Toggle | toggle_pane | Toggle Pane\t<leader>, T | | Toggle Pane  <leader>, T | <leader>, t | 🔲 |
| Zoom | zoom_pane | Zoom Pane\t<leader>, Z | | Zoom Pane  <leader>, Z | <leader>, z | 🔲 |
| SelectLeft | select_pane_left | Select Left Pane\t<leader>, Left | | Select Left Pane  <leader>, Left | <leader>, left | ✅ |
| SelectRight | select_pane_right | Select Right Pane\t<leader>, Right | | Select Right Pane  <leader>, Right | <leader>, right | ✅ |
| SelectUp | select_pane_up | Select Up Pane\t<leader>, Up | | Select Up Pane  <leader>, Up | <leader>, up | ✅ |
| SelectDown | select_pane_down | Select Down Pane\t<leader>, Down | | Select Down Pane  <leader>, Down | <leader>, down | ✅ |
| SwapPrev | swap_pane_prev | Swap Pane Previous\t<leader>, { | | Swap Pane Previous  <leader>, { | <leader>, { | 🔲 |
| SwapNext | swap_pane_next | Swap Pane Next\t<leader>, } | | Swap Pane Next  <leader>, } | <leader>, } | 🔲 |
| RotateForward | rotate_forward | Rotate Forward\t<leader>, Ctrl+O | | Rotate Forward  <leader>, Ctrl+O | <leader>, ctrl+o | 🔲 |
| RotateBackward | rotate_backward | Rotate Backward\t<leader>, Opt+O | | Rotate Backward  <leader>, Opt+O | <leader>, alt+o | 🔲 |
| EqualizeSize | equalize_pane_size | Equalize Pane Size\t<leader>, = | | Equalize Pane Size  <leader>, = | <leader>, = | 🔲 |
| ResizeLeft | resize_pane_left | Resize Pane Left\t<leader>, Opt+Left | | Resize Pane Left  <leader>, Opt+Left | <leader>, alt+left | 🔲 |
| ResizeRight | resize_pane_right | Resize Pane Right\t<leader>, Opt+Right | | Resize Pane Right  <leader>, Opt+Right | <leader>, alt+right | 🔲 |
| ResizeUp | resize_pane_up | Resize Pane Up\t<leader>, Opt+Up | | Resize Pane Up  <leader>, Opt+Up | <leader>, alt+up | 🔲 |
| ResizeDown | resize_pane_down | Resize Pane Down\t<leader>, Opt+Down | | Resize Pane Down  <leader>, Opt+Down | <leader>, alt+down | 🔲 |

### SpaceCommand

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| New | new_space | New Space\t<leader>, C | | New Space  <leader>, C | <leader>, c | 🔲 |
| Close | close_space | Close Space\t<leader>, & | | Close Space  <leader>, & | <leader>, & | 🔲 |
| Next | next_space | Next Space | ctrl+tab | Next Space  Ctrl+Tab | | 🔲 |
| Previous | prev_space | Previous Space | ctrl+shift+tab | Previous Space  Ctrl+Shift+Tab | | 🔲 |
| Rename | rename_space | Rename Space | | Rename Space | | 🔲 |

### SideSheetCommand

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| Toggle | toggle_side_sheet | Toggle Side Sheet\t<leader>, S | | Toggle Side Sheet  <leader>, S | <leader>, s | ✅ |
| ToggleRight | toggle_side_sheet_right | Toggle Right Sheet | | Toggle Right Sheet | | 🔲 |
| ToggleBottom | toggle_side_sheet_bottom | Toggle Bottom Sheet | | Toggle Bottom Sheet | | 🔲 |

### WindowCommand (new)

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| NewWindow | new_window | New Window | super+n | New Window  Cmd+N | | 🔲 |
| CloseWindow | close_window | Close Window | super+shift+w | Close Window  Cmd+Shift+W | | 🔲 |
| Minimize | minimize_window | Minimize | super+m | Minimize  Cmd+M | | 🔲 |
| ToggleFullscreen | toggle_fullscreen | Toggle Fullscreen | ctrl+super+f | Toggle Fullscreen  Ctrl+Cmd+F | | 🔲 |
| Settings | open_settings | Settings | super+, | Settings  Cmd+, | | 🔲 |

### CameraCommand (unchanged)

| Variant | menu id | label | accel | Menu Display | bind | Handler |
|---------|---------|-------|-------|--------------|------|---------|
| Reset | reset_camera | Reset Camera | | Reset Camera | | ✅ |
| ToggleFreeCamera | toggle_free_camera | Toggle Free Camera | | Toggle Free Camera | | ✅ |

## Summary

| Category | ✅ | 🔲 | Total |
|----------|:--:|:--:|:-----:|
| Tab | 13 | 5 | 18 |
| Browser | 8 | 5 | 13 |
| Pane | 7 | 10 | 17 |
| Space | 0 | 5 | 5 |
| SideSheet | 1 | 2 | 3 |
| Window | 0 | 5 | 5 |
| Camera | 2 | 0 | 2 |
| **Total** | **31** | **32** | **63** |

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

Everything marked 🔲 in the tables above.

## Files Changed

- `crates/vmux_desktop/src/command.rs` — all enum definitions, new WindowCommand
- `crates/vmux_desktop/src/layout/tab.rs` — Next/Previous/SelectIndex handlers
- `crates/vmux_desktop/src/layout/pane.rs` — remove on_pane_cycle TabCommand listener
- `crates/vmux_desktop/src/layout/side_sheet.rs` — new SideSheetCommand variants
- `crates/vmux_desktop/src/layout/space.rs` — new SpaceCommand variant (Rename)
