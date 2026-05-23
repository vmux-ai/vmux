# OpenCommand consolidation design

Date: 2026-05-22

## Goal

Replace the scattered set of "open a new page" commands (`StackCommand::New`, `TabCommand::New`, `PaneCommand::SplitV`/`SplitH`, `TerminalCommand::New`/`NewTab`, `CommandBarOpenEvent.new_tab`) with a single `OpenCommand` enum under `BrowserCommand`. Express all five placement intents in one shape, accept an optional URL, and route every trigger (menu, shortcut, command bar, MCP, agents) through the same handler.

## Motivation

Today the command tree mixes "open" verbs across four sub-enums, each with its own naming convention and partial URL handling:

- `StackCommand::New` (`super+n`) creates an empty stack in the current pane.
- `TabCommand::New` (`super+t`) creates a workspace tab — but `TabCommand` operates on `Space` entities, a leftover from the in-flight `Space → Tab` rename.
- `PaneCommand::SplitV` / `SplitH` (`<leader> %` / `"`) split a pane in only two directions despite the UI supporting four.
- `TerminalCommand::New` and `NewTab` (`ctrl+\``) spawn terminal entities directly instead of opening a `vmux://terminal/` URL.
- `CommandBarOpenEvent.new_tab: bool` plus `CommandBarActionEvent.action: "navigate" | "new_tab"` carry the placement intent across the webview ↔ host boundary as a flag + string.

The result: five overlapping concepts, four naming styles, two-direction splits, no shared URL/payload contract, and a confusing match between the user's mental model ("open this URL there") and the enum surface.

## Final shape

### Command tree

```
AppCommand
├─ Scene
├─ Layout              # Window / Space / Tab / Pane / Stack / ToggleLayout
├─ Terminal
├─ Browser             # becomes OsSubMenuGroup
│  ├─ Navigation       # PrevPage / NextPage / Reload / HardReload / Stop
│  ├─ Open             # NEW — OpenCommand, 5 variants
│  ├─ View             # Zoom* / DevTools / ViewSource / Print
│  └─ Bar              # FocusAddressBar / OpenCommandBar / OpenPathBar / OpenCommands / Find
└─ Service
```

`BrowserCommand` mirrors the transformation `LayoutCommand` already went through in [2026-05-11-layout-command-refactor-design.md](2026-05-11-layout-command-refactor-design.md): it changes from a flat unit enum to an `OsSubMenuGroup` of four leaf sub-enums. Existing browser variants regroup under Navigation / View / Bar.

### `OpenCommand`

```rust
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, PartialEq, Eq)]
pub enum OpenCommand {
    #[menu(id = "open_in_place",     label = "Open Here",         accel = "super+l")]
    #[mcp(description = "\
        Navigate the currently focused stack to the given URL. \
        Equivalent to the user typing a URL in the address bar and pressing enter. \
        Use this when the user asks to 'go to', 'navigate to', or 'open' a URL \
        without specifying placement — the current page is replaced. \
        If `url` is omitted, falls back to the configured startup URL.")]
    InPlace      {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_stack", label = "Open in New Stack", accel = "super+n")]
    #[mcp(description = "\
        Open the URL as a new stack inside the currently focused pane. \
        Stacks are the in-pane tab strip: the current stack stays alive and a new \
        one is added next to it, becoming active. Use when the user wants to \
        preserve the current page and view a new one alongside, in the same pane.")]
    InNewStack   {
        #[mcp(description = "Absolute URL to open in the new stack. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(
        expand          = "direction",
        id_template     = "open_in_pane_{dir}",
        label_template  = "Open in Pane {Dir}",
    )]
    #[shortcut(
        expand = "direction",
        top    = "Super+Shift+K",
        right  = "Super+Shift+L",
        bottom = "Super+Shift+J",
        left   = "Super+Shift+H",
    )]
    // Legacy tmux-style chords (preserved from PaneCommand::SplitV / SplitH):
    #[shortcut(chord = "Ctrl+g, %",  variant = "InPane { direction: Right,  target: NewSplit, mode: NewStack, url: None }")]
    #[shortcut(chord = "Ctrl+g, \"", variant = "InPane { direction: Bottom, target: NewSplit, mode: NewStack, url: None }")]
    #[mcp(description = "\
        Open the URL in a sibling pane in the given direction. Two axes control \
        behaviour: `target` chooses whether to reuse an adjacent pane or create a \
        new split; `mode` chooses whether to navigate the chosen pane in-place or \
        add a new stack to it. \
        \n\n\
        Examples for an agent:\n\
        - 'split right and open foo.com' → \
          `direction: Right, target: NewSplit, mode: NewStack, url: 'https://foo.com'`\n\
        - 'show docs in the pane to the right' (pane exists) → \
          `direction: Right, target: Existing, mode: InPlace, url: '...'`\n\
        - 'open new stack in the bottom pane' → \
          `direction: Bottom, target: Existing, mode: NewStack, url: '...'`\n\
        \n\
        If `target == Existing` but no pane exists in `direction`, the handler \
        silently falls back to `NewSplit`. If `url` is omitted, opens the startup URL.")]
    InPane {
        #[mcp(description = "Direction of the target pane relative to the currently focused pane.", enum_values = ["top", "right", "bottom", "left"])]
        direction: PaneDirection,
        #[mcp(description = "Whether to reuse the existing sibling pane (Existing) or always split to create one (NewSplit). Existing falls back to NewSplit if no sibling exists.", enum_values = ["existing", "new_split"])]
        target: PaneTarget,
        #[mcp(description = "Where to put the URL within the chosen pane: navigate its active stack (InPlace) or append a new stack (NewStack).", enum_values = ["in_place", "new_stack"])]
        mode: PaneOpenMode,
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_tab",   label = "Open in New Tab",   accel = "super+t")]
    #[mcp(description = "\
        Open the URL in a brand-new Tab within the current Space. \
        Tabs are the workspace-tab strip (one level above panes); creating one \
        gives the user a fresh layout container. Use when the user asks for a new \
        workspace tab, a fresh layout, or wants to isolate the new page from the \
        current pane structure.")]
    InNewTab     {
        #[mcp(description = "Absolute URL to open in the new Tab. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_space", label = "Open in New Space", accel = "super+shift+n")]
    #[mcp(description = "\
        Open the URL in a brand-new Space (top-level profile). \
        Spaces are the highest-level container and each carries its own profile \
        (cookies, identity, theme). Use only when the user explicitly asks for a \
        new profile, a separate identity, or a top-level workspace switch.")]
    InNewSpace   {
        #[mcp(description = "Absolute URL to open in the new Space. If omitted, opens the startup URL.")]
        url: Option<String>,
    },
}

pub enum PaneDirection { Top, Right, Bottom, Left }

/// What pane to act on.
pub enum PaneTarget {
    /// Reuse the pane that already exists in `direction`.
    /// Falls back to `NewSplit` if no such pane exists.
    Existing,
    /// Always split the current pane; create a new pane in `direction`.
    NewSplit,
}

/// How to place the URL within the chosen pane.
/// For `PaneTarget::NewSplit` both modes degenerate to the same behaviour
/// (a fresh pane starts with exactly one stack).
pub enum PaneOpenMode {
    /// Navigate the pane's active stack.
    InPlace,
    /// Add a new stack to the pane and open the URL there.
    NewStack,
}
```

Five variants. `InPane` carries three parameters: `direction`, `target` (existing pane vs. new split), and `mode` (in-place vs. new stack within that pane). The `expand = "direction"` macro attribute fans `InPane` into four direction-specific menu IDs / shortcuts at derive time. `target` + `mode` are **not** expanded — shortcuts default to `target: NewSplit, mode: NewStack` (matches current SplitV/SplitH behaviour: split and create a fresh stack). Other combinations are reachable via the command bar (modifier-key chord) and MCP.

When `target == Existing` but no pane exists in the requested direction, the handler degrades silently to `NewSplit`. Runtime fallback, not a separate variant.

`(NewSplit, InPlace)` and `(NewSplit, NewStack)` produce identical results — a brand-new pane always starts with one stack containing the URL. Kept as separate states because the alternative (nesting `mode` inside `Existing { mode }`) breaks the macro's flat-field expectation; the redundancy is harmless and easier to derive against.

`Copy` is dropped (the `Option<String>` payload isn't `Copy`); handlers take `&OpenCommand` or clone, matching message patterns already used elsewhere in `vmux_command`.

The user-facing menu/label text keeps the verb "Open …" so the menubar reads naturally. The short variant names (`InPlace`, `InNewStack`, etc.) work because the enum name `OpenCommand` already supplies the verb at call sites: `OpenCommand::InNewStack { .. }` reads as "open in new stack".

### URL resolution

Every handler funnels through one helper:

```rust
fn resolve_url(
    cmd_url: Option<&str>,
    startup: &EffectiveStartupUrl,
) -> String {
    cmd_url
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| startup.get())
        .unwrap_or_else(|| DEFAULT_NEW_PAGE_URL.to_string())
}
```

Rule: explicit URL wins; otherwise fall back to the `EffectiveStartupUrl` setting; otherwise a hard-coded default (`vmux://new-page/` or similar — final URL TBD when implementing).

### Trigger paths

| Source | URL value |
|---|---|
| Menu click | `None` |
| Keyboard shortcut | `None` |
| Command bar — user submits text | `Some(typed)` |
| Command bar — empty submit on intent shortcut | `None` |
| MCP tool / agent | caller-supplied `Option<String>` |
| In-app links / history rows | `Some(href)` |

Menu + shortcut already fire `AppCommand` via `MessageWriter`; the macro generates instantiations like `BrowserCommand::Open(OpenCommand::InNewStack { url: None })` when their menu IDs trigger.

Command bar replaces today's stringly-typed `CommandBarActionEvent.action` with a direct `BrowserCommand::Open(...)` emission. The webview-side action handler picks the right variant from the modal's current placement intent.

## Migration table

| Current command | Disposition |
|---|---|
| `StackCommand::New` (`super+n`) | **removed**; shortcut → `Open::InNewStack { url: None }` |
| `TabCommand::New` (`super+t`) | **removed**; shortcut → `Open::InNewTab` |
| `PaneCommand::SplitV` (`<leader> %`) | **command removed**; `Ctrl+g, %` chord rebinds to `Open::InPane { direction: Right, target: NewSplit, mode: NewStack, url: None }` (matches current SplitV → `PaneSplitDirection::Row` behaviour) |
| `PaneCommand::SplitH` (`<leader> "`) | **command removed**; `Ctrl+g, "` chord rebinds to `Open::InPane { direction: Bottom, target: NewSplit, mode: NewStack, url: None }` |
| `TerminalCommand::New` | **removed**; synthesises `Open::InNewStack { url: Some("vmux://terminal/") }` |
| `TerminalCommand::NewTab` (`ctrl+\``) | **removed**; synthesises `Open::InNewTab { url: Some("vmux://terminal/") }` |
| `BrowserCommand::FocusAddressBar` (`super+l`) | **removed**; shortcut → `Open::InPlace { url: None }` (same end UX) |
| `StackCommand::Duplicate`, `StackCommand::MoveToPane`, `StackCommand::SwapPrev/Next`, `StackCommand::Reopen` | unchanged — operate on existing entities |
| `TabCommand::Close / Next / Previous / Rename / SelectIndex* / Swap*` | unchanged |
| `PaneCommand::Select* / Resize* / Swap* / Rotate* / Zoom / Equalize / Close` | unchanged |
| `CommandBarOpenEvent.new_tab: bool` | replaced by `target: Option<OpenTarget>` enum field |
| `CommandBarActionEvent.action: "navigate" \| "new_tab"` | removed; command bar emits `BrowserCommand::Open(...)` directly |
| `BrowserCommand` (flat enum) | regrouped under `Navigation` / `Open` / `View` / `Bar` sub-enums |

Call-site count from `rg`: ~6 files in `vmux_layout/`, ~3 in `vmux_desktop/`, ~2 in `vmux_command/` tests, plus the webview-side action sender in `vmux_layout/src/command_bar/`.

## Macro changes

Three derives need to accept `Fields::Named` variants. Today they hard-error at `Fields::Unit` (e.g. `crates/vmux_macro/src/lib.rs:73`).

| Derive | Extension |
|---|---|
| `OsSubMenu` | Accept named-field variants. Menu ID resolution returns the variant with all fields set to `Default::default()` — for `Option<String>` that's `None`. |
| `DefaultShortcuts` | Same. Shortcut → variant with default fields. |
| `CommandBar` | Same. Command-bar entries emit the variant with default fields; the action handler upgrades `url` from the user's input. |
| `McpTool` | Already supports named fields (`Fields::Named` branch at `lib.rs:873`). No change for the payload itself. |

Two new attribute forms:

1. `#[menu(expand = "<field>")]` + `#[shortcut(expand = "<field>", <dir> = "<key>")]` — auto-fan-out over a bounded enum field (here, `PaneDirection`). One source variant produces N menu IDs / shortcuts.
   - ID template: `open_in_pane_{dir}` → `open_in_pane_top` etc.
   - Label template: `Open in Pane {Dir}` → `Open in Pane Top` etc.
   - Instantiation: `OpenCommand::InPane { direction: PaneDirection::Top, target: PaneTarget::NewSplit, mode: PaneOpenMode::NewStack, url: None }`. Non-expanded fields use `Default` impls (`PaneTarget::default() = NewSplit`, `PaneOpenMode::default() = NewStack`).
2. `#[shortcut(chord = "<keys>", variant = "<literal>")]` — bind an additional chord to a specific variant *instantiation* with explicit field values. Used to preserve legacy tmux chords (`Ctrl+g, %`, `Ctrl+g, "`) without expansion. The `variant` value is a Rust expression string the macro splices into the dispatch table.

Staging option (recommended): land Named-field support first with a single menu entry per variant (works for the four non-direction variants); add `expand` support and the explicit-variant `chord` form in a follow-up pass that introduces `InPane`'s directional bindings + tmux chord aliases.

## MCP

OpenCommand is intentionally exposed to MCP with thorough per-variant descriptions so that agents prefer these specific tools over the generic `layout_read` / `layout_apply` API. Generic layout APIs require the agent to read the tree, compute a delta, and submit it — slow, error-prone, and easy to get wrong. The five Open tools (or eight, after `InPane` direction expansion) let an agent express intent declaratively in a single call.

Tools exposed (final names depend on macro output — confirm during implementation):

| Tool | Required args | Description summary |
|---|---|---|
| `open_in_place` | `url?` | Replace current page (like typing in address bar) |
| `open_in_new_stack` | `url?` | New stack in current pane |
| `open_in_pane_top` / `_right` / `_bottom` / `_left` | `target` (existing\|new_split), `mode` (in_place\|new_stack), `url?` | Open in sibling pane in that direction |
| `open_in_new_tab` | `url?` | New workspace Tab |
| `open_in_new_space` | `url?` | New top-level Space (profile) |

Each tool description (see `OpenCommand` variant `#[mcp(description = ...)]` attributes) includes worked examples so the agent can disambiguate "open foo.com in a new pane on the right" → `open_in_pane_right { target: NewSplit, mode: NewStack, url: 'foo.com' }` from "show docs in the right pane" → `open_in_pane_right { target: Existing, mode: InPlace, url: '...' }`.

Field-level `enum_values` constraints (via existing `McpTool` support) force the agent to pick valid strings for `target` / `mode` / `direction` rather than free text.

## Open issues

1. **`super+l` semantics** — currently `FocusAddressBar` puts focus in the URL bar. Rebinding to `Open::InPlace { url: None }` should produce the same UX (open command bar pre-filled with the current URL, in-place navigation mode). Verify the command-bar handler treats `target = InPlace` identically to today's focus-bar entry point.
2. **IPC breaking changes** — `CommandBarOpenEvent.new_tab: bool` becoming `target: Option<OpenTarget>` and `CommandBarActionEvent.action` removal break the webview ↔ host bin-event contract. Persisted session snapshots that reference the old fields need a migration pass. Coordinate the rkyv schema version bump.
3. **Entity rename dependency** — the Space → Tab / Profile → Space / leaf-Tab → Stack rename is partially landed. OpenCommand naming uses the *target* model (`InNewTab` means "open in new workspace tab", `InNewStack` means "open in new in-pane page"). If the rename ships in chunks, the OpenCommand handlers must call into whichever entity-spawn helper currently exists; rename in lockstep where possible.
4. **Other legacy tmux chords** — `Ctrl+g, %` and `Ctrl+g, "` are explicitly preserved. Audit other tmux-style chords (`Ctrl+g, c` for new window if any, `Ctrl+g, d` for duplicate, `Ctrl+g, !` for MoveToPane) and confirm they continue to map to surviving commands (`StackCommand::Duplicate`, `StackCommand::MoveToPane` already keep them).

## Testing

Each `OpenCommand` variant gets a handler test that exercises both URL paths (`Some(url)` and `None` with startup URL set / unset). One MCP round-trip test per variant verifies the tool name + argument schema, including the per-field `enum_values` constraint for `direction`, `target`, and `mode`. Macro-level tests in `vmux_macro` cover Named-field instantiation, the `expand = "direction"` fan-out (asserting four menu IDs + four shortcut entries from one source variant), and the explicit-variant `chord` form (asserting `Ctrl+g, %` and `Ctrl+g, "` map to the expected `InPane { direction, target: NewSplit, mode: NewStack }` instantiations). Existing call-site coverage (e.g. `from_menu_id` round-trip in `command.rs`) extends to the new IDs.

The `InPane` handler additionally needs tests for all four `(target, mode)` combinations and for the `Existing → NewSplit` fallback when no sibling pane exists.

The IPC change is exercised by the existing `command_bar_open_event_*` rkyv round-trip tests, extended to assert the new `target: Option<OpenTarget>` field round-trips correctly.

### Per-variant test coverage

**InPlace** — `crates/vmux_desktop/src/browser.rs` `tests::open_in_place_flow`:
- `in_place_with_explicit_url_triggers_request_navigate`
- `in_place_with_none_url_uses_startup_setting`
- `in_place_with_none_url_and_no_startup_uses_default`

**InNewStack** — `crates/vmux_layout/src/stack.rs` `tests`:
- `open_in_new_stack_with_explicit_url`
- `open_in_new_stack_none_url_with_startup`
- `open_in_new_stack_none_url_no_startup_falls_back`
- `in_new_stack_with_no_url_uses_startup_url`

**InPane** — `crates/vmux_layout/src/pane.rs` `tests`:
- `in_pane_new_split_right_creates_pane_to_the_right`
- `in_pane_existing_in_place_navigates_neighbor_active_stack`
- `in_pane_existing_new_stack_adds_stack_to_neighbor`
- `in_pane_existing_falls_back_to_new_split_when_no_sibling`

**InNewTab** — `crates/vmux_layout/src/space.rs` `tests`:
- `open_in_new_tab_explicit_url_spawns_new_space_with_url`
- `open_in_new_tab_none_url_falls_back_to_startup`
- `open_in_new_tab_none_url_no_startup_falls_back_to_default`

**InNewSpace** — `crates/vmux_layout/src/profile.rs` `tests`:
- `open_in_new_space_explicit_url_spawns_new_profile_with_url`
- `open_in_new_space_none_url_falls_back_to_startup`
- `open_in_new_space_none_url_no_startup_falls_back_to_default`

**Derive / macro coverage** — `crates/vmux_command/tests/open_command_derives.rs`:
- `default_pane_target_is_new_split` / `default_pane_open_mode_is_new_stack`
- `open_command_in_place_has_none_url_default` / `open_command_in_pane_carries_all_four_fields`
- `from_menu_id_resolves_all_expanded_pane_directions` / `from_menu_id_resolves_non_expanded_variants`
- `default_shortcuts_contains_expected_ids` / `extra_chord_bindings_has_two_tmux_chords`
- `command_bar_entries_has_eight_entries` / `mcp_tool_entries_has_all_variants`
- `open_target_default_is_in_place` / `open_target_in_pane_variant`
