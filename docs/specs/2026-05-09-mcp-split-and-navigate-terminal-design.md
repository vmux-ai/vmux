# MCP `split_and_navigate` Terminal URL Support — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended)

## Goal

Extend the `split_and_navigate` MCP tool so that passing `url = "vmux://terminal/"` spawns a terminal in the new pane instead of a browser. No new MCP tool — same composite, just smarter routing.

## Why

Agents need to "open a terminal on the right" with the same atomic / no-focus-race guarantee that `split_and_navigate` provides for URLs. The vmux protocol already uses `vmux://terminal/` as a marker for terminal tabs (existing `TERMINAL_WEBVIEW_URL` in `vmux_layout::event`). Reusing that convention avoids adding another MCP tool.

## Approach

In `vmux_desktop::agent::handle_agent_commands::SplitAndNavigate` arm, after `split_pane_in_two` returns the new `pane2`:
- If `url.starts_with(TERMINAL_WEBVIEW_URL)` (i.e. `"vmux://terminal/"`) → call `spawn_terminal_tab(pane2, None, None, ...)` (default cwd, no pending input).
- Else → existing `spawn_browser_tab(pane2, url, ...)` path.

Update the `McpParamTool::SplitAndNavigate` description so agents know about the special URL form.

## Changes

### `vmux_desktop::agent::handle_agent_commands` — `SplitAndNavigate` arm

After the `let (_pane1, pane2) = vmux_layout::pane::split_pane_in_two(...)` line, replace the unconditional `spawn_browser_tab` call with:

```rust
if url.starts_with(TERMINAL_WEBVIEW_URL) {
    spawn_terminal_tab(
        pane2,
        None,
        None,
        &mut commands,
        &mut meshes,
        &mut webview_mt,
        &settings,
    );
} else {
    spawn_browser_tab(
        pane2,
        url,
        &mut commands,
        &mut meshes,
        &mut webview_mt,
    );
}
```

`TERMINAL_WEBVIEW_URL` is already imported in `agent.rs` (line 21).

### `vmux_mcp::tools::McpParamTool::SplitAndNavigate` description update

Current: `"Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom."`

New: `"Split current pane and open a URL in the new pane. Direction 'right' = side-by-side (vertical separator), 'down' = top/bottom. Use url 'vmux://terminal/' to open a terminal instead of a browser."`

### Tests

`vmux_desktop::agent::tests`:
- New `split_and_navigate_with_terminal_url_spawns_terminal`: spawn focused pane, send `SplitAndNavigate { direction: "right", url: "vmux://terminal/" }`, assert (a) PaneSplit on original pane, (b) the new pane has a `Terminal` entity (not a `Browser`).

`vmux_mcp::tools::tests`:
- (Optional) Spot-check the description includes the new hint. Skip — descriptions can change freely without breaking dispatch.

## Out of Scope

- Passing a custom cwd via `vmux://terminal/?cwd=/path` query string. YAGNI — defaults work for most cases. Future ticket if needed.
- A separate `split_and_terminal` named tool. Decided against — the vmux:// URL convention covers it.
- Other vmux:// URL types (e.g. `vmux://processes/`). Add as needed when there's a use case.

## Risks

- **Description string size**: tool descriptions are visible to LLMs. The expanded description is one more sentence — fine.
- **URL normalization**: `vmux://terminal/` vs `vmux://terminal` (trailing slash). The existing code uses `TERMINAL_WEBVIEW_URL = "vmux://terminal/"` with the slash. We use `starts_with(TERMINAL_WEBVIEW_URL)` so trailing-slash variants are accepted as long as they have the prefix. Strict — agents calling `vmux://terminal` (no slash) get a browser, not a terminal. Document in description, or relax to `starts_with("vmux://terminal")`. Pick the relaxed version for ergonomics.

## File Map

- **Modify** `crates/vmux_desktop/src/agent.rs` — add the if/else branch in the SplitAndNavigate arm; add a new test.
- **Modify** `crates/vmux_mcp/src/tools.rs` — update the `SplitAndNavigate` `#[mcp(description = ...)]` string.
