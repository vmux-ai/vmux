# MCP Query Tools (extension of VMX-107) — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended again)

## Goal

Add five read-only MCP tools so agents can inspect vmux state:

- `get_state()` — full layout snapshot (spaces → panes → tabs + focused pointers).
- `list_tabs()` — flat list of tabs across all spaces.
- `list_spaces()` — list of spaces with their panes and tabs.
- `list_terminals()` — list of terminal processes with cwd and pid.
- `get_focused()` — currently focused space / pane / tab.

## Why

Today the MCP surface is action-only. Agents can open/close/select but can't see what exists. Even a basic question like "how many tabs are open?" is unanswerable. Read access unlocks state-aware agent workflows (e.g. "switch to the tab showing google.com" only works if the agent can list tabs first).

## Architectural change

This is a meaningful new pattern for vmux's IPC. Today `AgentCommand` is fire-and-forget: mcp → service → broadcast → desktop, service immediately acks. Queries need round-trip data.

### New wire protocol (rkyv-serialized)

```rust
pub enum AgentQuery {
    GetState,
    ListTabs,
    ListSpaces,
    ListTerminals,
    GetFocused,
}

pub struct TabInfo {
    pub id: String,         // entity.to_string()
    pub title: String,
    pub url: String,
    pub kind: String,       // "browser" | "terminal"
}

pub struct TerminalInfo {
    pub id: String,         // entity.to_string()
    pub cwd: String,
    pub pid: u32,
}

pub struct PaneInfo {
    pub id: String,
    pub tabs: Vec<TabInfo>,
}

pub struct SpaceInfo {
    pub id: String,
    pub name: String,
    pub panes: Vec<PaneInfo>,
    pub active: bool,
}

pub struct FocusedInfo {
    pub space: Option<String>,
    pub pane: Option<String>,
    pub tab: Option<String>,
}

pub struct StateSnapshot {
    pub spaces: Vec<SpaceInfo>,
    pub focused: FocusedInfo,
}

pub enum AgentQueryResult {
    State(StateSnapshot),
    Tabs(Vec<TabInfo>),
    Spaces(Vec<SpaceInfo>),
    Terminals(Vec<TerminalInfo>),
    Focused(FocusedInfo),
    Error(String),
}
```

Four new `ClientMessage`/`ServiceMessage` variants:

```rust
ClientMessage::AgentQuery { request_id: AgentRequestId, query: AgentQuery }
ClientMessage::AgentQueryResponse { request_id: AgentRequestId, result: AgentQueryResult }

ServiceMessage::AgentQuery { request_id: AgentRequestId, query: AgentQuery }
ServiceMessage::AgentQueryResult { request_id: AgentRequestId, result: AgentQueryResult }
```

### vmux_service routing

Add shared state at server startup:

```rust
type PendingQueries = Arc<Mutex<HashMap<AgentRequestId, oneshot::Sender<AgentQueryResult>>>>;
```

Handler additions:

- `ClientMessage::AgentQuery { request_id, query }`:
  1. Validate that at least one desktop is subscribed (`agent_tx.receiver_count() > 0`); else return `ServiceMessage::Error`.
  2. Create `oneshot::channel`; insert sender into `pending_queries` keyed by `request_id`.
  3. Broadcast `ServiceMessage::AgentQuery { request_id, query }` via `agent_tx`.
  4. `tokio::time::timeout(Duration::from_secs(5), receiver)`:
     - Ok(Ok(result)) → write `ServiceMessage::AgentQueryResult { request_id, result }` to this client's writer.
     - Timeout or sender dropped → remove the entry, write `ServiceMessage::Error { message: "agent query timed out" }`.

- `ClientMessage::AgentQueryResponse { request_id, result }`:
  - Look up `request_id` in `pending_queries`, remove, send the result on the oneshot. Drop silently if no entry (late response, already timed out).

### vmux_desktop query handler

New file `crates/vmux_desktop/src/agent_query.rs` (split from `agent.rs` since the query logic is sizable and read-only). Plugin wiring lives in `agent.rs` (`AgentPlugin` adds the new system).

- New `AgentQueryRequest` Bevy message (mirrors `AgentCommandRequest`).
- A bridging system in the IPC subscriber translates `ServiceMessage::AgentQuery` into `AgentQueryRequest` events.
- `handle_agent_queries` system reads requests; for each, builds the payload by querying the world; sends `ClientMessage::AgentQueryResponse` via the existing `ServiceConnection`.

Payload construction queries:
- `Query<(Entity, &Space, &Children, Option<&LastActivatedAt>)>` for spaces.
- `Query<(Entity, &Pane, &Children), Without<PaneSplit>>` for leaf panes.
- `Query<(Entity, &Tab, Option<&PageMetadata>), With<Tab>>` for tabs.
- `Query<(Entity, &Terminal, &ProcessId, Option<&ChildOf>), Without<ProcessExited>>` for terminals.
- `Res<FocusedTab>` for focused pointers.

Entity ids serialized via `entity.to_string()` (e.g. `"12v0"`). Informational only.

`kind` discrimination on `TabInfo`: tab is "terminal" if any of its children has the `Terminal` component, else "browser".

### vmux_mcp dispatch

- Five new zero-arg `ToolDefinition`s in `tool_definitions()`.
- New helper `agent_query_from_tool_call(name) -> Result<AgentQuery, String>` parallel to `agent_command_from_tool_call`.
- New helper `run_agent_query(query) -> Result<Value, String>` that connects, sends `ClientMessage::AgentQuery`, loops on `connection.recv()` until `ServiceMessage::AgentQueryResult { request_id == ours }`, then serializes the result as JSON.
- `tool_call_result` in `protocol.rs` first tries the command path; if the name matches a query tool, dispatches via the query path instead. Cleanest: a single match before the existing command match.

## Testing

**vmux_service::protocol::tests:**
- `agent_query_roundtrips` — rkyv serialize/deserialize of each `AgentQuery` variant.
- `agent_query_result_roundtrips` — same for each `AgentQueryResult` variant.

**vmux_service::server::tests** (or integration test if no unit harness exists):
- `agent_query_routes_response_back_to_originator` — start service in-process, connect two clients, one subscribes (mock desktop), one sends AgentQuery + AgentQueryResponse, originator receives result.
- `agent_query_times_out_when_no_response` — service returns `Error("agent query timed out")` after 5s. Use a much shorter test-only timeout (configurable constant).

**vmux_desktop::agent_query::tests:**
- `list_tabs_returns_all_tabs_with_metadata` — fixture world with 2 spaces, 3 tabs, assert payload shape.
- `get_focused_reflects_focused_tab_resource` — set FocusedTab, assert FocusedInfo matches.
- `get_state_includes_active_space_flag` — assert one and only one space has `active: true`.

**vmux_mcp::tools::tests:**
- `tool_list_includes_query_tools` — all five names present.
- `query_tools_dispatch_via_agent_query` — call each `agent_query_from_tool_call(name)` and assert the right `AgentQuery` variant is constructed.

## Constraints

- Follow existing patterns: rkyv derives on every new type, no comments in code, no `mod.rs` files (use `agent_query.rs` not `agent_query/mod.rs`).
- 5-second timeout is hard-coded for now; expose as a constant `AGENT_QUERY_TIMEOUT` in protocol.rs.
- Cleanup on disconnect: when a client connection drops, remove any of its outstanding entries from `pending_queries`. Implementation note: the easiest approach is per-connection cleanup in the existing connection-end path (after `// Client disconnected — abort all patch forwarders`).

## Out of Scope

- Filter parameters (`list_tabs(space_id="…")`). Future ticket.
- Entity-id-keyed action tools (`select_tab_by_id(id)`). Future ticket.
- Streaming / subscription updates (live state push). Much larger effort.
- Per-tool timeouts.
- Authentication / authorization.

## Risks

- **Service routing complexity**: this is the first piece of cross-client request/response routing. The shared HashMap + oneshot pattern is standard but new in this codebase. Acknowledged.
- **Payload size**: `get_state()` on a heavy session (10+ spaces, 100+ tabs) could produce a few KB. Fine for MCP, but worth noting.
- **Latency**: round-trip is bounded by Bevy's frame rate (~16 ms typical) plus IPC. Total ~20 ms typical, well under the 5s timeout.
- **Eventual consistency**: query results reflect the world *as of the next Update tick after the request arrives*. Two queries milliseconds apart may see different states. Acceptable for the use cases we care about.
- **No backpressure**: if hundreds of queries arrive at once, the broadcast channel buffers them. Existing buffer sizes are large enough; no specific tuning needed for the expected workload.

## File Map

- **Modify** `crates/vmux_service/src/protocol.rs` — add 1 enum (AgentQuery), 6 structs, 1 result enum, 4 ClientMessage/ServiceMessage variants, AGENT_QUERY_TIMEOUT constant, rkyv tests.
- **Modify** `crates/vmux_service/src/server.rs` — add PendingQueries shared state, two new message arms, cleanup on disconnect.
- **Create** `crates/vmux_desktop/src/agent_query.rs` — new file with query handler system + payload builders + tests.
- **Modify** `crates/vmux_desktop/src/agent.rs` — `AgentPlugin::build` adds the new system; AgentQueryRequest message; bridging system that turns ServiceMessage::AgentQuery into AgentQueryRequest.
- **Modify** `crates/vmux_desktop/src/lib.rs` — declare the new `agent_query` module.
- **Modify** `crates/vmux_mcp/src/tools.rs` — five new tool definitions; `agent_query_from_tool_call` helper.
- **Modify** `crates/vmux_mcp/src/protocol.rs` — `tool_call_result` dispatches to query path or command path based on name; new `run_agent_query` helper.
