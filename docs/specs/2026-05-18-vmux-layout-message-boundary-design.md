# vmux_layout Message Boundary Design

## Problem

Layout logic is split across two crates in conflicting styles.

- `vmux_layout` owns the canonical ECS components (`Pane`, `PaneSplit`, `Stack`, `Tab`), bundle factories (`leaf_pane_bundle`, `stack_bundle`, `tab_bundle`), and Commands-based spawners (`split_pane_in_two`).
- `vmux_desktop::agent_layout` owns the MCP reconciler: `apply.rs` (1174 lines) re-implements component sets in raw `World` code, and `agent_layout.rs` (`build_layout_snapshot`) reads layout state by querying the `Terminal` marker — a `vmux_desktop` type — to classify tabs.

Symptoms of the duplication: every bundle bug we shipped this branch (missing `Transform`/`GlobalTransform`/`Visibility` on splits, default `Stack` instead of `stack_bundle()`, bare `Tab` instead of `tab_bundle()`) was a hand-rolled component set drifting from its canonical sibling.

## Goal

Move all layout logic into `vmux_layout` behind a Bevy message boundary. `vmux_desktop` becomes a thin dispatcher that translates wire-protocol requests into messages and routes responses back to the service client.

Out of scope (separate PRs): equivalent reorganizations for `vmux_settings`, `vmux_agent`, `vmux_command`, `vmux_terminal`.

## Architecture

```
service ──AgentCommand──┐
                        ├─► vmux_desktop::agent.rs
                        │     │ writes
                        │     ▼
                        │   LayoutApplyRequest ──► vmux_layout::reconcile
service ──AgentQuery────┤   LayoutSnapshotRequest      │ applies / reads
                        │                              ▼
                        │   LayoutApplyResponse ◄─ LayoutSnapshot
                        │   LayoutSnapshotResponse ◄─┘
                        │     │ reads
                        │     ▼
                        └── vmux_desktop::agent.rs ──ClientMessage──► service
```

`vmux_desktop` and `vmux_layout` communicate only through four `Message` types. No direct function calls across the crate boundary.

## Components

### Messages (in `vmux_layout::reconcile`)

```rust
#[derive(Message)]
pub struct LayoutApplyRequest {
    pub request_id: u64,
    pub snapshot: LayoutSnapshot,
}

#[derive(Message)]
pub struct LayoutApplyResponse {
    pub request_id: u64,
    pub result: Result<LayoutSnapshot, String>,
}

#[derive(Message)]
pub struct LayoutSnapshotRequest {
    pub request_id: u64,
}

#[derive(Message)]
pub struct LayoutSnapshotResponse {
    pub request_id: u64,
    pub snapshot: LayoutSnapshot,
}
```

Request ids are opaque `u64`s on this side of the boundary. `vmux_desktop::agent.rs` unwraps `AgentRequestId` from `vmux_service::protocol` into `u64` when writing the request, and re-wraps it when forwarding the response to the service client. Keeps `vmux_layout` independent of the service protocol type.

### Systems (in `vmux_layout::reconcile`)

`apply_layout_requests`
- Reads `MessageReader<LayoutApplyRequest>`.
- Uses `commands.queue(|world| ...)` for exclusive `&mut World` access (matches current pattern in `vmux_desktop::agent.rs`).
- Validates → diffs → applies → builds new snapshot → emits `LayoutApplyResponse`.

`serve_snapshot_requests`
- Reads `MessageReader<LayoutSnapshotRequest>`.
- Runs the snapshot queries, emits `LayoutSnapshotResponse`.

Both systems are added by `LayoutPlugin`.

### Protocol DTOs (in `vmux_layout::protocol`)

Moved from `vmux_service::protocol::layout`. Renamed to drop the `Dto` suffix:

```
SpaceDto          → Space
LayoutNodeDto     → LayoutNode  (keeps prefix to avoid colliding with Bevy `Node`)
TabDto            → Tab
FocusDto          → Focus
SplitDirectionDto → SplitDirection
LayoutSnapshot    (unchanged)
NodeKind          (unchanged)
```

`vmux_service::protocol::layout` becomes a re-export of `vmux_layout::protocol` so wire-protocol consumers (`vmux_mcp`, `vmux_service` itself) compile without churn.

**Known name clash**: `vmux_layout::Tab` (component) marks a *space*; `vmux_layout::protocol::Tab` (DTO) describes a *pane-tab* (what the user clicks at the top). Code touching both qualifies via module path:

```rust
use vmux_layout::{Pane, Stack};       // components
use vmux_layout::protocol as proto;
// proto::Tab, proto::LayoutNode::Split { ... }
```

The broader fix is renaming the `Tab` component to `Space` (per the codebase's "always use space" rule), but that is a follow-up PR.

### Kind detection (URL is the source of truth)

The `TabKind` distinction (`browser` / `terminal`) is redundant. Every tab is a webview; the only difference is the URL scheme:
- `vmux://terminal/...` → spawns `Terminal` (PTY-backed)
- `vmux://agent/...` → spawns agent (process-backed)
- `vmux://processes`, `vmux://spaces/`, etc. → built-in webview apps
- anything else → `Browser` (CEF)

`spawn_url_into_stack` already dispatches by URL prefix. The snapshot does the same:

```rust
fn classify_tab(url: &str) -> &'static str {
    if url.starts_with("vmux://terminal/") {
        "terminal"
    } else {
        "browser"
    }
}
```

Consequence: the snapshot no longer queries `With<Terminal>`. The `Terminal`-marker coupling is gone.

`spawn_tab` in the reconciler sets `Stack`'s `PageMetadata.url = tab.url` immediately at spawn time so the snapshot reads an authoritative URL even before CEF loads.

The `kind` field is removed from the MCP `update_layout` schema. The LLM specifies URL only; the reconciler infers kind from URL. The snapshot omits `kind` from output.

### Bundle factory: `split_root_bundle`

Currently the split entity's component set is inlined in two places (`split_pane_in_two` in `vmux_layout::pane`, `spawn_split` in `vmux_desktop::agent_layout::apply`). Both must stay in sync; bugs we hit prove they didn't.

Extract a factory:

```rust
// vmux_layout::pane
pub fn split_root_bundle(direction: PaneSplitDirection) -> impl Bundle {
    let flex_direction = match direction { /* ... */ };
    let gap = pane_split_gaps(direction, crate::event::PANE_GAP_PX);
    (
        Pane,
        PaneSplit { direction },
        PaneSize::default(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
        Node {
            flex_grow: 1.0,
            flex_direction,
            column_gap: gap.column_gap,
            row_gap: gap.row_gap,
            align_items: AlignItems::Stretch,
            ..default()
        },
    )
}
```

Both `split_pane_in_two` (Commands) and the reconciler (`world.spawn(...)`) use it. One bundle, two callers, no drift.

The other factories already exist (`leaf_pane_bundle`, `stack_bundle`, `tab_bundle`) and the reconciler will use them directly (we already migrated `spawn_tab` to `stack_bundle()` and new-space spawning to `tab_bundle()` in this branch).

`set_split_direction` (in-place direction mutator, used by the "reuse existing root split" path) stays in the reconciler — it mutates an existing entity rather than spawning.

## File moves and module layout

### New in `vmux_layout`

- `src/protocol.rs` — DTOs (`Space`, `LayoutNode`, `Tab`, `Focus`, `SplitDirection`, `LayoutSnapshot`, `NodeKind`, `parse_id`, `format_id`). Moved from `vmux_service::protocol::layout`.
- `src/reconcile.rs` — message types + `apply_layout_requests` + `serve_snapshot_requests` systems. Contains the `validate`, `plan_diff`, `apply` logic moved from `vmux_desktop::agent_layout::{reconcile, apply}`.
- `src/snapshot.rs` — `build_layout_snapshot` moved from `vmux_desktop::agent_layout`. URL-based kind detection.
- `src/pane.rs` — adds `split_root_bundle(direction)` factory; `split_pane_in_two` rewires to use it.

`LayoutPlugin` registers the four message types and the two systems.

### Deleted from `vmux_desktop`

- `src/agent_layout/` (apply.rs, reconcile.rs)
- `src/agent_layout.rs`
- `src/agent_query.rs` (logic moves; the `AgentQuery::GetSettings` branch — the only non-layout piece — moves to a tiny handler in `settings_view.rs`)
- `src/layout.rs` re-export shim (callers import from `vmux_layout` directly)

### Thinned in `vmux_desktop::agent.rs`

`ServiceAgentCommand::UpdateLayout`:

```rust
ServiceAgentCommand::UpdateLayout { layout } => {
    layout_request_writer.write(LayoutApplyRequest {
        request_id: request.request_id,
        snapshot: layout.clone(),
    });
    continue;
}
```

`AgentQuery::ReadLayout`:

```rust
AgentQuery::ReadLayout => {
    layout_snapshot_writer.write(LayoutSnapshotRequest {
        request_id: request.request_id,
    });
    continue;
}
```

New small system in `vmux_desktop` reads `LayoutApplyResponse` and `LayoutSnapshotResponse` and sends `ClientMessage::AgentCommandResponse` / `AgentQueryResponse` to the service client.

### Updated

- `vmux_mcp::tools` — imports DTOs from `vmux_service::protocol::layout` (now a re-export), no logic change. The `kind` field is removed from the `update_layout` schema (`enum: ["browser", "terminal"]` deleted).
- `vmux_service::protocol::layout` — becomes a re-export of `vmux_layout::protocol`.
- `vmux_service::protocol::AgentRequestId` — becomes a re-export of `vmux_core::RequestId`.

## Dep graph

One new edge:
- `vmux_service ──► vmux_layout` — to re-export DTOs from `vmux_layout::protocol` as `vmux_service::protocol::layout`.

`vmux_layout` does **not** gain a `vmux_service` dep. Request ids cross the boundary as plain `u64` so `vmux_layout` stays independent of the service protocol.

No cycles: `vmux_layout` deps (`vmux_core`, `vmux_space`, `vmux_command`, `vmux_history`, `vmux_webview_app`, `vmux_ui`) do not include `vmux_service`.

## Testing

Existing tests in `vmux_desktop::agent_layout::{apply, reconcile}::tests` move alongside the code into `vmux_layout::reconcile::tests`. They run against the same logic, so they continue to pass without semantic changes — only the module path and the import lines change.

The snapshot tests in `vmux_desktop::agent_layout::tests` move into `vmux_layout::snapshot::tests`. They lose the `terminals: Query<...>` argument and drop the test setups that inserted `Terminal` markers (replaced with `PageMetadata { url: "vmux://terminal/", ... }` on the relevant Stacks to exercise the URL-based classifier).

A new integration test verifies the message round-trip end-to-end: write a `LayoutApplyRequest`, run the apply system, assert a `LayoutApplyResponse` is emitted with the expected snapshot.

## Migration / compatibility

- `vmux_mcp` tool schema change (drop `kind`) is a backward-incompatible schema change for the LLM. Acceptable since the tool is internal and no external integrations consume it yet.
- Saved layouts on disk don't carry kind metadata that this change affects — kind has always been derived from runtime state. No persistence migration needed.

## Non-goals

- Renaming the `Tab` component to `Space`. Worthwhile but separate.
- Moving the broader spaces (`vmux_desktop::spaces.rs`) or terminal lifecycle (`vmux_desktop::terminal.rs`) into their domain crates. Each is its own PR.
- Eliminating `vmux_desktop::agent.rs` entirely. It still owns service-client dispatch for non-layout commands (terminal_send, browser_navigate, etc.).
