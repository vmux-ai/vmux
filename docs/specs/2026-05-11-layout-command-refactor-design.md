# LayoutCommand refactor design

Date: 2026-05-11

## Goal

Group the six layout-related sub-enums (Window, Zen, Tab, Pane, Stack, Space) under a new `LayoutCommand` wrapper inside `vmux_command`. Reflect the grouping in the native menubar (single "Layout" menu containing six nested submenus) without changing where the enum definitions live.

## Motivation

`AppCommand` currently has ten top-level variants. Six of them (`Window`, `Zen`, `Tab`, `Pane`, `Stack`, `Space`) are layout concerns — they manipulate the visible structure of the app. The remaining four (`Scene`, `Terminal`, `Browser`, `Service`) are not. Mixing them at the top level makes the menubar wide, the enum hard to scan, and obscures the natural grouping. Collapsing the six under one `LayoutCommand` umbrella makes the structure read at a glance and enables a tidier nested menu.

## Final shape

### `AppCommand`

```rust
pub enum AppCommand {
    #[menu(label = "Scene")]    Scene(SceneCommand),
    #[menu(label = "Layout")]   Layout(LayoutCommand),
    #[menu(label = "Terminal")] Terminal(TerminalCommand),
    #[menu(label = "Browser")]  Browser(BrowserCommand),
    #[menu(label = "Service")]  Service(ServiceCommand),
}
```

Order: `Scene → Layout → Terminal → Browser → Service`. Mirrors the native menubar from left to right.

### `LayoutCommand`

```rust
#[derive(OsSubMenuGroup, DefaultShortcuts, CommandBar, McpTool, ...)]
pub enum LayoutCommand {
    #[menu(label = "Window")] Window(WindowCommand),
    #[menu(label = "Zen")]    Zen(ZenCommand),
    #[menu(label = "Tab")]    Tab(TabCommand),
    #[menu(label = "Pane")]   Pane(PaneCommand),
    #[menu(label = "Stack")]  Stack(StackCommand),
    #[menu(label = "Space")]  Space(SpaceCommand),
}
```

The six leaf sub-enums (`WindowCommand`, `ZenCommand`, `TabCommand`, `PaneCommand`, `StackCommand`, `SpaceCommand`) keep their existing definitions, attributes, and derives — only their position in the type hierarchy changes.

### Native menubar

```
Vmux | Scene | Layout > [Window > …, Zen > …, Tab > …, Pane > …, Stack > …, Space > …] | Terminal | Browser | Service
```

The `Layout` top-level menu opens a single submenu whose entries are themselves submenus.

## Macro design

### New derive: `OsSubMenuGroup`

`LayoutCommand` is neither a leaf submenu (it doesn't hold `MenuItem`s) nor the root menu (it doesn't seed the menubar with the about/quit block). It needs its own derive that produces the same surface as `OsSubMenu` so the existing `OsMenu` derive can plug it in unchanged.

Generated impl:

```rust
impl LayoutCommand {
    pub(crate) const HAS_VISIBLE_ITEMS: bool =
        WindowCommand::HAS_VISIBLE_ITEMS
        || ZenCommand::HAS_VISIBLE_ITEMS
        || TabCommand::HAS_VISIBLE_ITEMS
        || PaneCommand::HAS_VISIBLE_ITEMS
        || StackCommand::HAS_VISIBLE_ITEMS
        || SpaceCommand::HAS_VISIBLE_ITEMS;

    pub(crate) fn append_native_menu_leaf(
        submenu: &mut ::muda::Submenu,
    ) -> Result<(), ::muda::Error> {
        if WindowCommand::HAS_VISIBLE_ITEMS {
            let mut nested = ::muda::Submenu::new("Window", true);
            WindowCommand::append_native_menu_leaf(&mut nested)?;
            submenu.append(&nested)?;
        }
        // ...repeat for Zen/Tab/Pane/Stack/Space using their #[menu(label = ...)]...
        Ok(())
    }

    pub fn from_menu_id(id: &str) -> Option<Self> {
        WindowCommand::from_menu_id(id).map(LayoutCommand::Window)
            .or_else(|| ZenCommand::from_menu_id(id).map(LayoutCommand::Zen))
            .or_else(|| TabCommand::from_menu_id(id).map(LayoutCommand::Tab))
            .or_else(|| PaneCommand::from_menu_id(id).map(LayoutCommand::Pane))
            .or_else(|| StackCommand::from_menu_id(id).map(LayoutCommand::Stack))
            .or_else(|| SpaceCommand::from_menu_id(id).map(LayoutCommand::Space))
    }
}
```

The signature matches `OsSubMenu`'s output exactly, so `OsMenu`'s existing per-variant submenu block (`if <inner>::HAS_VISIBLE_ITEMS { … inner::append_native_menu_leaf(&mut sub) … }`) works without modification.

### No change to other derives

`DefaultShortcuts`, `CommandBar`, and `McpTool` already split into a `_leaf` (unit variants) and `_root` (tuple variants) path. `LayoutCommand` has tuple variants, so all three route through the existing `_root` codegen which simply iterates inner types and concatenates. The result is naturally recursive — `AppCommand → LayoutCommand → WindowCommand` works because each layer just calls the next layer's identical method.

### Macro layout

`OsSubMenuGroup` is a sibling of `OsSubMenu` in `crates/vmux_macro/src/lib.rs`. Implementation reuses `MenuProps::from_attrs` and `heck_variant_snake_case` already in the file. New `proc_macro_derive` registration mirrors the existing four.

## Call-site updates

Every existing match/construction of the form `AppCommand::<Inner>(...)` for `Inner ∈ {Window, Zen, Tab, Pane, Stack, Space}` becomes `AppCommand::Layout(LayoutCommand::<Inner>(...))`. Concrete sites:

- `crates/vmux_layout/src/window.rs` — line 25 import, line 101 match arm
- `crates/vmux_layout/src/zen.rs` — line 5 import, line 29 `matches!`
- `crates/vmux_layout/src/tab.rs` — line 166 match, line 452 `messages.write`
- `crates/vmux_layout/src/pane.rs` — line 273 match, lines 588–591 direction map, lines 1093/1153 writes
- `crates/vmux_layout/src/stack.rs` — line 195 match, lines 643/705 writes
- `crates/vmux_desktop/src/shortcut.rs` — lines 326/344/364/390 test assertions
- `crates/vmux_desktop/src/command_bar.rs` — lines 309/313/316–317 (Space/Stack/Pane match arms) and 1930/1938/2039 (Space/Stack writes). The `BrowserCommand::*` and `TerminalCommand::*`/`ServiceCommand::*` arms in the same file stay unchanged.
- `crates/vmux_desktop/src/terminal.rs` — line 731 (`AppCommand::Stack(StackCommand::Close)` write). Line 1787's `TerminalCommand::CopyMode` reader stays — Terminal is not in Layout.
- `crates/vmux_command/src/command.rs` tests — lines 444/448 expected variants

`Scene`, `Terminal`, `Browser`, `Service` references stay unchanged.

## Testing

- Existing `command.rs` unit tests:
  - `hidden_commands_can_have_default_shortcuts` — already covers `TerminalCommand::CopyMode`; unaffected.
  - `mcp_lookup_resolves_every_command_id` — update expected variants for `close_tab` (now `Layout(LayoutCommand::Tab(TabCommand::Close))`) and `split_v` (now `Layout(LayoutCommand::Pane(PaneCommand::SplitV))`).
- Add a new test asserting `AppCommand::from_menu_id("split_v")` resolves through the nested chain to `AppCommand::Layout(LayoutCommand::Pane(PaneCommand::SplitV))`, locking in the recursive `from_menu_id` dispatch.
- Existing shortcut-routing tests in `vmux_desktop/src/shortcut.rs` already exercise `SpaceCommand::Open`; updating their expected `AppCommand` variant validates the new wrapper at the dispatch boundary.

## Verification

Per project rules, run the changed-crate fmt/clippy/test loop on `vmux_macro`, `vmux_command`, `vmux_layout`, `vmux_desktop` (and any downstream crate the macro change ripples into).

## Out of scope

- Moving any enum definition to a different crate.
- Changes to `OsMenu`, `DefaultShortcuts`, `CommandBar`, or `McpTool` derives beyond adding the new sibling `OsSubMenuGroup`.
- Touching the keyboard shortcut bindings, MCP tool descriptions, or visible labels of any leaf command.
- Visual changes to the command bar UI (the entries it surfaces are still the same set).
