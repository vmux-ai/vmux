# Expose All App Commands to MCP — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp)

## Goal

Auto-expose every `AppCommand` variant as an MCP tool so external agents can invoke any in-app command. Replace the current four-command allowlist (`tab_new`, `browser_open_command_bar`, `browser_open_path_bar`, `browser_open_commands`) with macro-generated tool definitions covering the full enum (~50 commands).

## Why

Today MCP exposes only a hand-picked subset gated by an `agent` attribute on each enum variant. Any new command requires an explicit opt-in plus a hand-written tool entry. This means the MCP surface drifts behind the in-app surface, and adding commands costs extra work.

## Approach

Drive the MCP tool list off the existing `CommandBar` derive macro. Drop the `agent` attribute entirely. Keep the three param-bearing tools (`open_command_bar`, `new_terminal_tab`, `run_shell`) hand-written.

## Changes

### 1. `vmux_macro::CommandBar`

- Remove the `agent` field from `MenuProps` and the parsing branch in `MenuProps::from_attrs`.
- In `impl_command_bar_leaf`, generate a new associated function:

  ```rust
  pub fn agent_entries() -> Vec<(&'static str, &'static str)>
  ```

  Returns `(id, description)` for **every** variant — visible and hidden. `description` is the variant's `label`, with the `\t…` shortcut hint stripped and trimmed.

- In `impl_command_bar_leaf`, change `from_agent_id` to match every variant unconditionally (no `agent` flag check). The function becomes equivalent to the existing `from_menu_id`, but kept under a separate name to preserve the public surface used by `vmux_desktop::agent`.

- In `impl_command_bar_root`, aggregate `agent_entries()` from sub-enums; chain `from_agent_id` over all sub-enums.

### 2. `vmux_command::command::AppCommand`

Remove the four `agent` markers in `command.rs`:
- `tab_new` (TabCommand::New)
- `browser_open_command_bar` (BrowserCommand::OpenCommandBar)
- `browser_open_path_bar` (BrowserCommand::OpenPathBar)
- `browser_open_commands` (BrowserCommand::OpenCommands)

### 3. `vmux_mcp::tools`

`tool_definitions()`:
- Iterate `AppCommand::agent_entries()` to build one `ToolDefinition` per command:
  - `name`: the command id verbatim (e.g. `tab_new`, `split_v`, `terminal_clear`).
  - `description`: the cleaned label (e.g. `"Split Vertically"`).
  - `input_schema`: `{ "type": "object", "properties": {} }`.
- Append the three hand-written tools: `open_command_bar` (mode param), `new_terminal_tab` (cwd param), `run_shell` (command/cwd/mode params).
- Drop the hand-written `new_tab` (now redundant — covered by auto-generated `tab_new`).

`agent_command_from_tool_call(name, arguments)`:
- Match the three hand-written tools first (preserves their param handling).
- Fallback: if `AppCommand::from_agent_id(name).is_some()`, return `Ok(AgentCommand::AppCommand { id: name.to_string() })`.
- Otherwise return `Err("unknown tool: {name}")`.

### 4. Tests

`vmux_macro` (no test crate today; covered downstream).

`vmux_command::command::tests`:
- Update `agent_command_lookup_exposes_only_allowlisted_commands` → rename to `agent_lookup_resolves_every_command_id`, assert representative ids across multiple sub-enums resolve (e.g. `tab_new`, `tab_close`, `split_v`, `terminal_clear`, `browser_reload`).

`vmux_mcp::tools::tests`:
- Update `list_tools_exposes_mvp_tools` → rename to `list_tools_includes_auto_generated_and_handwritten`. Assert the list contains:
  - hand-written: `open_command_bar`, `new_terminal_tab`, `run_shell`
  - auto-generated samples: `tab_new`, `tab_close`, `split_v`, `terminal_clear`, `browser_reload`
  - does NOT contain `new_tab`
- Add `auto_generated_tool_dispatches_as_app_command`: assert `agent_command_from_tool_call("split_v", json!({}))` returns `AgentCommand::AppCommand { id: "split_v" }`.
- Keep `empty_run_shell_command_returns_tool_error`.

## Out of Scope

- Adding new param-bearing tools (e.g. `select_tab(index)` collapsing the eight `tab_select_N` variants). Future task.
- Changing the MCP wire protocol or `AgentCommand` enum.
- Permissions / per-tool gating.
- Localization of descriptions.

## Risks

- **Tool list size**: ~50 tools advertised on `tools/list`. Acceptable — well within MCP client expectations.
- **Hidden commands exposed**: variants like `terminal_copy_mode` and `tab_select_5` are now callable by agents. Intentional per scope decision; agents can ignore them via description.
- **Description quality**: cleaned labels are short ("Toggle", "Reopen Closed Tab"). Sufficient for discovery; can be enriched later.
