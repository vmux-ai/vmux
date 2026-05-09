# MCP Tool Derive Macro (extension of VMX-107) — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended)

## Goal

1. Replace the hand-coded MCP tool registration in `vmux_mcp` with a single `McpTool` derive macro that handles BOTH zero-arg auto-generated tools (from `AppCommand`) and the param-bearing tools (currently hand-written).
2. Disambiguate the `split_v` / `split_h` MCP tool descriptions for LLM agents (they currently say "Split Vertically/Horizontally" — direction is not obvious).
3. Add an optional `pane` parameter to `browser_navigate` so agents can target a specific pane regardless of focus state (avoids the "user clicks confirm prompt → focus moves → navigate hits wrong pane" race observed during dogfooding).

## Why

Two distinct pains showed up while testing the MCP surface:

- **Direction confusion**: An agent asked to "open google.com on the right" sometimes called `split_h` instead of `split_v` because "vertical" vs "horizontal" is ambiguous in the menu labels.
- **Focus race**: After `split_v` + `select_pane_right` + `browser_navigate`, the navigate often hit the original (CLI) pane because the user's confirmation click on the MCP prompt moved focus back before the navigate executed. Agents can't reliably depend on focus state across multi-step workflows.

The user also requested architectural cleanup: MCP tool concerns should live in their own derive macro rather than overloading the existing `CommandBar`/`menu` machinery. Currently `CommandBar` derive generates `agent_entries()` and `from_agent_id` for MCP — those move to a dedicated `McpTool` derive.

## Approach

### Architecture

Three enums, one macro:

- **`AppCommand`** (existing, in `vmux_command::command`): gains `#[derive(McpTool)]`. Each variant has at most a `#[mcp(description = "...")]` attribute. Macro generates `mcp_tool_entries()` and `from_mcp_id()` for the ~50 zero-arg auto-generated tools.

- **`McpParamTool`** (new, in `vmux_mcp::tools`): explicitly enumerates the 6 param-bearing command tools. Variants have fields. Macro generates `mcp_tool_entries()` (with JSON schema inferred from field types) and `from_mcp_call()` (parses JSON args into the variant).

- **`McpQueryTool`** (new, in `vmux_mcp::tools`): enumerates the 5 query tools. Currently all zero-arg, so the macro emits `from_mcp_id()` (same shape as `AppCommand`). When a future query needs params, switch the affected variants to fielded form and the macro emits `from_mcp_call()` instead.

Per-enum hand-written translation:
- `McpParamTool::to_agent_command()` → `AgentCommand` (e.g. `OpenCommandBar { mode }` → `AppCommand { id: format!("browser_open_{mode}") }`).
- `McpQueryTool::to_agent_query()` → `AgentQuery` (1:1 mapping for now: `GetState` → `AgentQuery::GetState`, etc.).

A unified dispatcher in `vmux_mcp::tools` returns a `DispatchTarget` enum (Command or Query), and `tool_call_result` in `vmux_mcp::protocol` routes to the appropriate `run_agent_*` helper.

### Entity-id encoding

Switch `vmux_desktop::agent_query` from `entity.to_string()` (e.g. `"12v0"`) to `entity.to_bits().to_string()` (e.g. `"4294967308"`). The latter round-trips cleanly via `u64::from_str` + `Entity::try_from_bits(u64)`. Existing query tests update to the new format.

## Changes

### 1. `vmux_macro::McpTool` derive

New proc-macro derive in `crates/vmux_macro/src/lib.rs`. Reads its own attribute namespace `#[mcp(...)]`.

**Variant attributes:**
- `#[mcp(description = "...")]` — description string (required for variants with fields; optional for zero-arg variants where the menu label fallback applies).
- `#[mcp(skip)]` — exclude this variant from MCP tool list. (Reserved for future use — initially no variants set this.)

**Field attributes (for variants with fields):**
- `#[mcp(enum_values = ["a", "b", "c"])]` — declares a string field's allowed values. Emitted as `"enum": [...]` in the JSON schema.

**Generated code per leaf enum:**

```rust
impl Self {
    pub fn mcp_tool_entries() -> Vec<McpToolEntry> { ... }
    pub fn from_mcp_id(id: &str) -> Option<Self> { ... }              // only when all variants are unit
    pub fn from_mcp_call(name: &str, args: Value)                     // only when at least one variant has fields
        -> Option<Result<Self, String>> { ... }
}
```

`McpToolEntry` is a small struct (added to `vmux_macro` or shared): `{ name: &'static str, description: &'static str, schema: serde_json::Value }`.

The macro emits `from_mcp_id` for unit-only enums, `from_mcp_call` for enums with at least one fielded variant. (The `AppCommand` enum is unit-variant via its sub-enums — all sub-enums are unit, root is tuple-of-unit. Macro handles both root and leaf cases as today's `CommandBar` derive does.)

**Schema generation:**
- `String` field → `{"type": "string"}`, required (added to required list).
- `Option<String>` field → `{"type": "string"}`, optional (not in required list).
- `u8` / `u16` / `u32` / `u64` / `i32` field → `{"type": "integer"}`, required.
- `Option<u8>` etc. → `{"type": "integer"}`, optional.
- `bool` / `Option<bool>` → `{"type": "boolean"}`.
- Field with `#[mcp(enum_values = [...])]` → `{"type": "string", "enum": [...]}`.
- Unsupported field types → compile error with span pointing at the field.

For the AppCommand zero-arg case, the inputSchema is always `{"type": "object", "properties": {}}`.

**Description fallback for AppCommand zero-arg variants:**
- If `#[mcp(description = "...")]` set on the variant → use it.
- Else → use the cleaned `#[menu(label = "...")]` (split on `\t`, take prefix, trim) — same logic as the current `agent_entries`.
- Else (no menu attribute either) → empty string.

**Description requirement for fielded variants:**
- `#[mcp(description = "...")]` is required. Compile error if missing — these variants don't have a menu label fallback.

### 2. Remove `agent_entries` / `from_agent_id` from `CommandBar` derive

Delete from `vmux_macro::impl_command_bar_leaf` and `impl_command_bar_root`:
- `agent_entries()` generation
- `from_agent_id()` generation
- `agent_arms` collection

`CommandBar` derive becomes purely about the command-bar UI list (`command_bar_entries()`).

### 3. `AppCommand` annotations in `vmux_command::command`

Add `#[derive(McpTool)]` to `AppCommand` and every sub-enum (TabCommand, BrowserCommand, etc.).

Add `#[mcp(description = "...")]` only where the menu label is ambiguous for an LLM:

```rust
#[menu(id = "split_v", label = "Split Vertically\t<leader> %")]
#[mcp(description = "Split current pane into LEFT and RIGHT halves.")]
#[shortcut(chord = "Ctrl+g, %")]
SplitV,

#[menu(id = "split_h", label = "Split Horizontally\t<leader> \"")]
#[mcp(description = "Split current pane into TOP and BOTTOM halves.")]
#[shortcut(chord = "Ctrl+g, \"")]
SplitH,
```

Other variants use the menu-label fallback. (Future cleanup may add more `#[mcp(description = ...)]` overrides as agents discover ambiguous descriptions.)

Update the existing test `agent_lookup_resolves_every_command_id` → rename to `mcp_lookup_resolves_every_command_id`. Replace `agent_entries()` with `mcp_tool_entries()` and `from_agent_id` with `from_mcp_id`.

### 4. `McpParamTool` in `vmux_mcp::tools`

```rust
#[derive(Debug, McpTool)]
pub enum McpParamTool {
    #[mcp(description = "Open the Vmux command bar.")]
    OpenCommandBar {
        #[mcp(enum_values = ["default", "commands", "path"])]
        mode: Option<String>,
    },
    #[mcp(description = "Create a visible Vmux terminal tab.")]
    NewTerminalTab {
        cwd: Option<String>,
    },
    #[mcp(description = "Run a shell command in a visible Vmux terminal.")]
    RunShell {
        command: String,
        cwd: Option<String>,
        #[mcp(enum_values = ["new_tab", "active"])]
        mode: Option<String>,
    },
    #[mcp(description = "Navigate the active webview to a URL.")]
    BrowserNavigate {
        url: String,
        pane: Option<String>,
    },
    #[mcp(description = "Send raw text to the active terminal (no carriage return appended).")]
    TerminalSend {
        text: String,
        terminal: Option<String>,
    },
    #[mcp(description = "Select a tab by index (1-8).")]
    SelectTab {
        index: u8,
    },
}

impl McpParamTool {
    pub fn to_agent_command(self) -> Result<AgentCommand, String> { /* … */ }
}
```

`to_agent_command` is the only hand-written piece — handles per-variant translation:
- `OpenCommandBar { mode }`: maps mode → `browser_open_*` id, returns `AgentCommand::AppCommand { id }`.
- `NewTerminalTab { cwd }`: returns `AgentCommand::NewTerminalTab { cwd: cwd.unwrap_or_default() }`.
- `RunShell { command, cwd, mode }`: validates command non-empty, parses mode to `AgentShellMode`, returns `AgentCommand::RunShell { ... }`.
- `BrowserNavigate { url, pane }`: validates url non-empty, returns `AgentCommand::BrowserNavigate { url, pane }`.
- `TerminalSend { text, terminal }`: validates text non-empty, returns `AgentCommand::TerminalSend { text, terminal }`.
- `SelectTab { index }`: range-checks 1..=8, returns `AgentCommand::AppCommand { id: format!("tab_select_{index}") }`.

### 5. Wire-protocol additions in `vmux_service::protocol`

Add `pane: Option<String>` to `AgentCommand::BrowserNavigate`:

```rust
BrowserNavigate {
    url: String,
    pane: Option<String>,
},
```

Add `terminal: Option<String>` to `AgentCommand::TerminalSend`:

```rust
TerminalSend {
    text: String,
    terminal: Option<String>,
},
```

Both fields default to `None` (existing callers stay backward-compatible — rkyv auto-handles new optional field via the enum-variant approach).

`validate_agent_command` unchanged; the new optional fields don't affect existing validation rules.

Tests: rkyv roundtrip for the new fields populated and unpopulated.

### 6. `vmux_desktop::agent` honors target pane

`handle_agent_commands::BrowserNavigate { url, pane }` arm:
1. If `pane` is `Some(s)`:
   - Parse via `s.parse::<u64>().ok().and_then(Entity::try_from_bits)`.
   - If parsed entity is in the `panes` query (leaf pane), use it as the target pane.
   - If parse fails or entity is not a leaf pane, return `AgentCommandResult::Error("browser_navigate: invalid pane id")`.
2. If `pane` is `None`: use `focus.pane` (existing behavior).
3. Within the resolved target pane:
   - Find the active tab in that pane that has a non-terminal `Browser` webview. If found, trigger `RequestNavigate { webview, url }`.
   - Else, call `spawn_browser_tab(target_pane, &url, ...)` (existing helper from prior wave).
4. Return `AgentCommandResult::Ok` on either path; `AgentCommandResult::Error("browser_navigate: target pane has no tabs and could not spawn")` if neither succeeds (defensive — should not happen).

Helper extraction (new):
```rust
fn parse_pane_target(s: &str, panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>) -> Option<Entity> {
    let bits = s.parse::<u64>().ok()?;
    let entity = Entity::try_from_bits(bits).ok()?;
    panes.contains(entity).then_some(entity)
}
```

For `TerminalSend { text, terminal }`: similar — if `terminal` is `Some`, parse and validate against the `terminals` query; else fall back to `active_terminal_for_tab(focus.tab, ...)`.

### 7. `vmux_desktop::agent_query` entity-id encoding

In `crates/vmux_desktop/src/agent_query.rs`, replace every `entity.to_string()` with `entity.to_bits().to_string()` for ids in `TabInfo`, `TerminalInfo`, `PaneInfo`, `SpaceInfo`, and `FocusedInfo`.

Update existing test `focused_info_propagates_entity_ids` to compare against `entity.to_bits().to_string()`.

### 8. `vmux_mcp::tools` unification

Define `McpQueryTool` (new):

```rust
#[derive(Debug, McpTool)]
pub enum McpQueryTool {
    #[mcp(description = "Return the full vmux layout snapshot (spaces, panes, tabs, focused).")]
    GetState,
    #[mcp(description = "List all tabs across all spaces with title, url, and kind.")]
    ListTabs,
    #[mcp(description = "List all spaces with their panes and tabs.")]
    ListSpaces,
    #[mcp(description = "List all terminal processes with cwd and pid.")]
    ListTerminals,
    #[mcp(description = "Return the currently focused space, pane, and tab ids.")]
    GetFocused,
}

impl McpQueryTool {
    pub fn to_agent_query(self) -> AgentQuery {
        match self {
            Self::GetState => AgentQuery::GetState,
            Self::ListTabs => AgentQuery::ListTabs,
            Self::ListSpaces => AgentQuery::ListSpaces,
            Self::ListTerminals => AgentQuery::ListTerminals,
            Self::GetFocused => AgentQuery::GetFocused,
        }
    }
}
```

Define a unified dispatcher in `vmux_mcp::tools`:

```rust
pub enum DispatchTarget {
    Command(AgentCommand),
    Query(AgentQuery),
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    AppCommand::mcp_tool_entries()
        .into_iter()
        .chain(McpParamTool::mcp_tool_entries())
        .chain(McpQueryTool::mcp_tool_entries())
        .map(|entry| ToolDefinition {
            name: entry.name.to_string(),
            description: entry.description.to_string(),
            input_schema: entry.schema,
        })
        .collect()
}

pub fn dispatch_from_tool_call(
    name: &str,
    arguments: Value,
) -> Result<DispatchTarget, String> {
    if let Some(parsed) = McpQueryTool::from_mcp_id(name) {
        return Ok(DispatchTarget::Query(parsed.to_agent_query()));
    }
    if let Some(parsed) = McpParamTool::from_mcp_call(name, arguments) {
        return Ok(DispatchTarget::Command(parsed?.to_agent_command()?));
    }
    if AppCommand::from_mcp_id(name).is_some() {
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand {
            id: name.to_string(),
        }));
    }
    Err(format!("unknown tool: {name}"))
}
```

Replace `vmux_mcp::protocol::tool_call_result` body to route via `DispatchTarget`:

```rust
async fn tool_call_result(params: &Value) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    match crate::tools::dispatch_from_tool_call(name, arguments)? {
        DispatchTarget::Command(cmd) => run_agent_command(cmd).await,
        DispatchTarget::Query(query) => run_agent_query(query).await,
    }
}
```

The old `agent_command_from_tool_call` and `agent_query_from_tool_call` functions are removed (their behavior is now inside `dispatch_from_tool_call`). The hand-written `run_agent_command` and `run_agent_query` IPC helpers stay unchanged — they handle the response loop.

### 9. Remove deprecated `ServiceMessage::AgentCommandAccepted` variant

The variant has zero remaining references in the codebase (verified: only the definition remains; the prior wave's `run_agent_command` rewrite removed the matcher, and the service no longer sends it). Pure cleanup.

In `crates/vmux_service/src/protocol.rs`, delete this variant from `pub enum ServiceMessage`:

```rust
    AgentCommandAccepted {
        request_id: AgentRequestId,
    },
```

This is a wire-format break for any older binary that expects to deserialize the variant — but no such consumer exists. `vmux_mcp` and `vmux_desktop` ship together.

### 10. Tests

**`vmux_macro`** (no test crate today; covered downstream):
- (Optional, time permitting) Add a small `tests/` directory with a sample enum to verify the derive expands correctly.

**`vmux_command::command::tests`**:
- Rename `agent_lookup_resolves_every_command_id` → `mcp_lookup_resolves_every_command_id`. Replace API calls. Assert split_v's description is the new override string.

**`vmux_service::protocol::tests`**:
- New rkyv roundtrip: `BrowserNavigate { url, pane: Some("12345") }`.
- New rkyv roundtrip: `TerminalSend { text, terminal: Some("67890") }`.

**`vmux_desktop::agent::tests`**:
- Update `browser_navigate_triggers_request_navigate_with_url` to set `pane: None`. Still works against existing webview.
- Update `browser_navigate_auto_spawns_tab_when_pane_is_empty` similarly.
- New test `browser_navigate_targets_specific_pane`: spawn pane A (focused) and pane B (not focused). Send `BrowserNavigate { url, pane: Some(B.to_bits().to_string()) }`. Assert tab spawned in B, NOT in A.
- New test `terminal_send_targets_specific_terminal`: spawn two terminals in different panes. Send `TerminalSend { text, terminal: Some(target.to_bits().to_string()) }`. Assert input went to target, not active.

**`vmux_desktop::agent_query::tests`**:
- Update `focused_info_propagates_entity_ids` to compare bits format.

**`vmux_mcp::tools::tests`**:
- Update tool list inclusion tests — all 6 param tools, 5 query tools, and 50+ auto-gen still present.
- Update dispatch tests to flow through `dispatch_from_tool_call`. Assert command tools return `DispatchTarget::Command(...)` and query tools return `DispatchTarget::Query(...)`.
- New test: `browser_navigate` JSON with `pane` arg flows through to `DispatchTarget::Command(AgentCommand::BrowserNavigate { pane: Some(...) })`.
- New test: `get_state` flows through to `DispatchTarget::Query(AgentQuery::GetState)`.

## Out of Scope

- Per-tool timeouts (still single 5s `AGENT_COMMAND_TIMEOUT`).
- Adding `target_pane` to `terminal_send` was discussed but **is included** in this design (Sections 5, 6, 10) — minor addition, scoped within the same wave.
- Removing the deprecated `ServiceMessage::AgentCommandAccepted` variant **is included** in this wave (Section 9) — verified to have no remaining references in the codebase.

## Risks

- **Macro complexity**: `McpTool` derive needs to introspect field types and generate JSON schema. Limited type coverage (String / integer / bool / Option / `enum_values`-attributed string) keeps it tractable. Unsupported types produce a compile error with a clear message.
- **AppCommand variant variants**: `AppCommand` is a tuple-of-unit-enums (root: tuple variants like `Tab(TabCommand)`; leaves: unit variants). The derive must handle both — same shape as the existing `CommandBar` derive. No new enum-shape complications.
- **Entity-id format change**: any external consumer parsing the old `"12v0"` format breaks. Currently no known external consumer — agents have only had `get_state` for a few hours. Acceptable.
- **`Option<String>` field for `pane`/`terminal`**: rkyv serializes `Option<String>` natively, no schema bump needed. Adding a field to an existing enum variant in rkyv is a wire break for older binaries — `vmux_mcp` and `vmux_desktop` ship together so this is fine.
- **Plan size**: this is wave 5 of VMX-107. The PR will be very large. The user has approved staying on the same branch.

## File Map

- **Modify** `crates/vmux_macro/src/lib.rs` — add `McpTool` derive (new proc-macro), the `McpToolEntry` struct, attribute parsing, schema generation logic. Remove `agent_entries` / `from_agent_id` from `CommandBar` derive.
- **Modify** `crates/vmux_command/src/command.rs` — add `#[derive(McpTool)]` to `AppCommand` and every sub-enum. Add `#[mcp(description = "...")]` on `split_v` and `split_h`. Update test naming and APIs.
- **Modify** `crates/vmux_service/src/protocol.rs` — add `pane: Option<String>` to `AgentCommand::BrowserNavigate`; add `terminal: Option<String>` to `AgentCommand::TerminalSend`; remove deprecated `ServiceMessage::AgentCommandAccepted` variant; rkyv roundtrip tests.
- **Modify** `crates/vmux_desktop/src/agent.rs` — update `BrowserNavigate` and `TerminalSend` arms to honor target pane/terminal id (with parse + validate). Add `parse_pane_target` / `parse_terminal_target` helpers. Update tests.
- **Modify** `crates/vmux_desktop/src/agent_query.rs` — switch entity-id encoding from `to_string()` to `to_bits().to_string()`. Update test.
- **Modify** `crates/vmux_mcp/src/tools.rs` — remove hand-written tool list, dispatch, and `agent_query_from_tool_call`. Add `McpParamTool` and `McpQueryTool` enums (both with `McpTool` derive). Add `to_agent_command` and `to_agent_query` impls. Add `DispatchTarget` enum. New `dispatch_from_tool_call` replaces the old per-kind dispatchers. Update tests.
- **Modify** `crates/vmux_mcp/src/protocol.rs` — `tool_call_result` routes via `DispatchTarget`. The `run_agent_command` and `run_agent_query` IPC helpers stay unchanged.

No new files. No deletions of existing files.
