# MCP `split_and_navigate` Composite Tool — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended)

## Goal

Add a single composite MCP tool, `split_and_navigate(direction, url)`, that splits the focused pane and opens the given URL in the new pane atomically — one MCP call, one permission prompt, no focus race.

## Why

Even with `target_pane: Option<String>` on `browser_navigate` and clearer split descriptions, agents still hit the focus-race bug in real workflows:

1. Agent receives "open google.com on the right".
2. Agent calls `split_v` → user's permission prompt steals focus → split executes.
3. Agent calls `select_pane_right` → another prompt → focus race continues.
4. Agent calls `browser_navigate(url)` → another prompt → user clicks, focus moves to wherever they clicked → navigate hits the wrong pane.

Even an agent that knows about `target_pane` would need to call `get_state` first, then call `browser_navigate(url, pane=<id>)` — three MCP calls, three permission prompts, three opportunities for focus to drift.

A composite tool reduces the entire workflow to **one MCP call**, **one permission prompt**, **zero focus dependency**.

## Approach

### New tool

```
split_and_navigate(direction: "right" | "down", url: String)
```

- `direction = "right"` → vertical split, new pane appears to the right of the focused pane (existing `split_v` semantics).
- `direction = "down"` → horizontal split, new pane appears below (existing `split_h` semantics).
- The URL is loaded into a new browser tab in the freshly-created pane.

### Architecture

1. **New `AgentCommand::SplitAndNavigate { direction: String, url: String }`** in `vmux_service::protocol`.
2. **New `McpParamTool::SplitAndNavigate` variant** with `#[mcp(enum_values = ["right", "down"])]` on `direction`. Translates 1:1 to the new `AgentCommand` variant via `to_agent_command`.
3. **Public helper `vmux_layout::pane::split_pane_in_two`** extracted from the existing `handle_pane_commands::SplitV/SplitH` arm. Returns `(pane1, pane2)` where `pane1` holds existing tabs and `pane2` is the fresh leaf pane. `handle_pane_commands` is refactored to call this helper — no behavioural change to existing splits.
4. **Desktop handler** in `vmux_desktop::agent::handle_agent_commands::SplitAndNavigate`:
   - Resolve focused pane (or `Error("split_and_navigate: no focused pane")`).
   - Parse `direction` to `PaneSplitDirection` (`Error` on unknown).
   - Call `split_pane_in_two(...)` to get the new pane entity.
   - Call `spawn_browser_tab(new_pane, url, ...)` (existing helper).
   - Return `Ok`.

All in one Bevy tick — no event hop, no cross-frame coordination, no focus dependency.

## Changes

### 1. `vmux_layout::pane`

Extract a public helper from the existing `PaneCommand::SplitV | PaneCommand::SplitH` arm in `handle_pane_commands` (around lines 256-304):

```rust
pub fn split_pane_in_two(
    commands: &mut Commands,
    active: Entity,
    direction: PaneSplitDirection,
    pane_settings: &crate::settings::PaneSettings,
    existing_tabs: &[Entity],
) -> (Entity, Entity) {
    let pane1 = spawn_leaf_pane(commands, active);
    let pane2 = spawn_leaf_pane(commands, active);

    for tab in existing_tabs {
        commands.entity(*tab).insert(ChildOf(pane1));
    }

    let flex_direction = match direction {
        PaneSplitDirection::Row => FlexDirection::Row,
        PaneSplitDirection::Column => FlexDirection::Column,
    };
    let gap = pane_split_gaps(direction, pane_settings.gap);
    commands.entity(active).insert(PaneSplit { direction });
    commands.entity(active).insert(Node {
        flex_grow: 1.0,
        flex_direction,
        column_gap: gap.column_gap,
        row_gap: gap.row_gap,
        align_items: AlignItems::Stretch,
        ..default()
    });
    commands.entity(pane2).insert(LastActivatedAt::now());

    (pane1, pane2)
}
```

`spawn_leaf_pane` and `pane_split_gaps` stay private; the new helper wraps both.

`handle_pane_commands::SplitV/SplitH` arm refactored to call the helper:

```rust
PaneCommand::SplitV | PaneCommand::SplitH => {
    let split_dir = if pane_cmd == PaneCommand::SplitV {
        PaneSplitDirection::Row
    } else {
        PaneSplitDirection::Column
    };
    let existing_tabs: Vec<Entity> = pane_children
        .get(active)
        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
        .unwrap_or_default();

    let (_pane1, pane2) =
        split_pane_in_two(&mut commands, active, split_dir, &settings.pane, &existing_tabs);

    let new_tab = commands
        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane2)))
        .id();
    new_tab_ctx.tab = Some(new_tab);
    new_tab_ctx.previous_tab = active_tab_opt;
    new_tab_ctx.needs_open = true;

    hover_intent.target = None;
    hover_intent.last_activation = Some(Instant::now());
    resize_q.p3().target = Some(pane2);
}
```

Existing test `pane.rs` (if any) verifies behaviour unchanged.

### 2. `vmux_service::protocol`

Add to `AgentCommand`:

```rust
SplitAndNavigate {
    direction: String,
    url: String,
},
```

Add to `validate_agent_command`:

```rust
AgentCommand::SplitAndNavigate { direction, url } => {
    if direction.is_empty() {
        return Err("split_and_navigate.direction is empty");
    }
    if url.trim().is_empty() {
        return Err("split_and_navigate.url is empty");
    }
    Ok(())
}
```

(Wrap as match arm with appropriate guard syntax — existing arms use `if guard => Err(...)` pattern; pick whichever is clearer.)

rkyv roundtrip test for the new variant.

### 3. `vmux_mcp::tools` — `McpParamTool` variant

Add to the existing `McpParamTool` enum:

```rust
#[mcp(description = "Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom.")]
SplitAndNavigate {
    #[mcp(enum_values = ["right", "down"])]
    direction: String,
    url: String,
},
```

Add to `to_agent_command`:

```rust
McpParamTool::SplitAndNavigate { direction, url } => {
    if !["right", "down"].contains(&direction.as_str()) {
        return Err(format!(
            "split_and_navigate: direction must be 'right' or 'down', got '{direction}'"
        ));
    }
    if url.trim().is_empty() {
        return Err("split_and_navigate.url is empty".to_string());
    }
    Ok(AgentCommand::SplitAndNavigate { direction, url })
}
```

### 4. `vmux_desktop::agent`

New handler arm (after `TerminalSend`). Compute direction first; then spawn if focused pane exists:

```rust
ServiceAgentCommand::SplitAndNavigate { direction, url } => {
    let split_dir = match direction.as_str() {
        "right" => vmux_layout::pane::PaneSplitDirection::Row,
        "down" => vmux_layout::pane::PaneSplitDirection::Column,
        other => {
            return_error(format!("split_and_navigate: invalid direction '{other}'"))
        }
    };

    if let Some(active_pane) = focus.pane.filter(|p| panes.contains(*p)) {
        let existing_tabs: Vec<Entity> = pane_children
            .get(active_pane)
            .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
            .unwrap_or_default();

        let (_pane1, pane2) = vmux_layout::pane::split_pane_in_two(
            &mut commands,
            active_pane,
            split_dir,
            &settings.layout.pane,
            &existing_tabs,
        );
        spawn_browser_tab(pane2, url, &mut commands, &mut meshes, &mut webview_mt);
        AgentCommandResult::Ok
    } else {
        AgentCommandResult::Error("split_and_navigate: no focused pane".to_string())
    }
}
```

The `return_error` placeholder above means: produce `AgentCommandResult::Error(message)` directly (the surrounding `let result = match { ... }` makes early-return-as-expression awkward). Idiomatic rewrite: compute `split_dir` as `Result<PaneSplitDirection, String>`, then nest the focus check inside an `Ok(_)` arm — see the implementation plan for the concrete code.

The handler needs new query parameters:
- `pane_children: Query<&Children, With<vmux_layout::pane::Pane>>` (or filtered to leaf panes — match what the existing pane handler uses).
- `tab_filter: Query<(), With<vmux_layout::tab::Tab>>` — used to identify which children of the active pane are tabs.

These additions to `handle_agent_commands` are scoped to this arm; other arms ignore them.

### 5. Tests

- **`vmux_service::protocol`**: rkyv roundtrip for `SplitAndNavigate { direction, url }`. Validation rejects empty direction and empty url.
- **`vmux_mcp::tools`**: tool list includes `split_and_navigate`. `from_mcp_call` parses correctly. `to_agent_command` rejects invalid direction.
- **`vmux_desktop::agent`**: `split_and_navigate_creates_split_and_navigates` — set up focused pane with one existing tab; send `SplitAndNavigate { direction: "right", url: "https://example.com" }`; assert (a) two new leaf panes exist, (b) original pane has `PaneSplit` component, (c) the new pane (pane2) contains a tab with a `Browser` child carrying the URL.

## Out of Scope

- A `select_and_navigate(pane_id, url)` for arbitrary existing panes (current `browser_navigate(url, pane=...)` covers this).
- A composite for terminal workflows (e.g. `split_and_run`). YAGNI for now.
- Auto-approving the MCP server in Claude Code config — settings concern, not code.

## Risks

- **Helper extraction**: refactoring `handle_pane_commands` to use `split_pane_in_two` is straightforward but touches existing layout code. Test coverage in `pane.rs` (if any) catches regressions; otherwise relies on manual verification that splitting still works in the running app.
- **Side effects of split**: the existing split sets `new_tab_ctx`, `hover_intent`, `resize_q.p3().target` — these are kept INSIDE `handle_pane_commands` (not moved into the helper) since they're relevant to the user-driven split flow but not the agent flow. Agent split skips these — the agent doesn't need a pending-empty-tab workflow.
- **Direction naming**: `"right"` / `"down"` are intuitive for LLMs (matches "open google.com on the right"). The internal `Row` / `Column` Flex naming is preserved in the helper.

## File Map

- **Modify** `crates/vmux_layout/src/pane.rs` — extract public `split_pane_in_two` helper. Refactor `handle_pane_commands::SplitV/SplitH` arm to call it.
- **Modify** `crates/vmux_service/src/protocol.rs` — add `SplitAndNavigate` variant; extend `validate_agent_command`; rkyv roundtrip test.
- **Modify** `crates/vmux_mcp/src/tools.rs` — add `SplitAndNavigate` variant to `McpParamTool`; extend `to_agent_command`; tests.
- **Modify** `crates/vmux_desktop/src/agent.rs` — new handler arm; new query params; test.
