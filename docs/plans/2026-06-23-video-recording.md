# Video Recording MCP Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **vmux note:** Do NOT subagent-drive this plan (CEF builds are huge; long agents drop sockets). Execute inline with a warm target dir. Run native checks per crate with `cargo test -p <crate>`.

**Goal:** Add `vmux_record_start` / `vmux_record_stop` MCP tools that record the vmux window to a compressed H.264 mp4 (plus an optional GIF), so an agent can capture a feature demo and drop it into a PR.

**Architecture:** Mirror the existing screenshot path end-to-end (`vmux_mcp` → `vmux_service` socket → `vmux_agent` Bevy messages → `vmux_desktop` capture). Screenshot uses one-shot `SCScreenshotManager`; recording uses continuous `SCStream` + AVFoundation `AVAssetWriter` (hardware H.264), with a pure-Rust GIF tee. Start/stop are two MCP calls with a `max_secs` auto-stop. Results are path-only (no inline bytes).

**Tech Stack:** Rust, Bevy 0.19-rc, rkyv IPC, objc2 (ScreenCaptureKit, AVFoundation, CoreMedia, CoreVideo), `gif` + `color_quant`.

**Spec:** `docs/specs/2026-06-23-video-recording-mcp-tool-design.md`

---

## File Structure

- `crates/vmux_service/src/protocol.rs` — add `AgentQuery::RecordStart/RecordStop`, `AgentQueryResult::Recording`, `RECORD_STOP_TIMEOUT`. (Modify)
- `crates/vmux_service/src/agent_broker.rs` — per-query timeout selection. (Modify)
- `crates/vmux_service/src/server.rs` — `query_result_to_content` arm for `Recording`. (Modify)
- `crates/vmux_agent/src/events.rs` — `RecordStart/Stop` request/response messages + `RecordingInfo`. (Modify)
- `crates/vmux_agent/src/plugin.rs` — query arms, forwarders, registration. (Modify)
- `crates/vmux_mcp/src/tools.rs` — tool definitions + dispatch arms. (Modify)
- `crates/vmux_mcp/src/protocol.rs` — `query_result_to_mcp_response` arm. (Modify)
- `crates/vmux_desktop/src/recording.rs` — capture: cross-platform helpers + systems + macOS `SCStream`/`AVAssetWriter` module + non-macOS stub. (Create)
- `crates/vmux_desktop/src/lib.rs` — register bridge + systems. (Modify)
- `crates/vmux_desktop/Cargo.toml` — new deps. (Modify)
- `docs/features/README.md` — committed-demo convention. (Create)

---

## Task 1: Protocol types (vmux_service)

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (`AgentQuery` ~167, `AgentQueryResult` ~182, consts ~134)
- Test: same file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write failing rkyv round-trip tests**

Add to the `tests` module in `crates/vmux_service/src/protocol.rs`:

```rust
    #[test]
    fn agent_query_record_start_rkyv_round_trip() {
        let q = AgentQuery::RecordStart {
            gif: true,
            max_secs: 120,
            pane: Some("pane:7".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn agent_query_record_stop_rkyv_round_trip() {
        let q = AgentQuery::RecordStop {
            dir: Some("/tmp/out".into()),
            name: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn agent_query_result_recording_rkyv_round_trip() {
        let r = AgentQueryResult::Recording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_234_567,
            auto_stopped: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let back: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, r);
    }
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p vmux_service --lib protocol::tests::agent_query_record`
Expected: FAIL — `no variant named RecordStart` / `Recording`.

- [ ] **Step 3: Add the variants and const**

In `AgentQuery` (after `Screenshot { pane: Option<String> }`, ~line 169):

```rust
    RecordStart {
        gif: bool,
        max_secs: u32,
        pane: Option<String>,
    },
    RecordStop {
        dir: Option<String>,
        name: Option<String>,
    },
```

In `AgentQueryResult` (after the `Image { .. }` variant, ~line 187):

```rust
    Recording {
        mp4_path: String,
        gif_path: Option<String>,
        duration_ms: u64,
        bytes: u64,
        auto_stopped: bool,
    },
```

After `AGENT_QUERY_TIMEOUT` (~line 134):

```rust
/// Stop-recording round-trip bound. `finishWriting` after live encoding is
/// fast, but a large clip's moov flush can take a few seconds. Comfortably
/// under vibe's 60s MCP tool timeout.
pub const RECORD_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p vmux_service --lib protocol::tests::agent_query_record protocol::tests::agent_query_result_recording`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(service): RecordStart/RecordStop query + Recording result"
```

---

## Task 2: Broker timeout + service content mapping (vmux_service)

**Files:**
- Modify: `crates/vmux_service/src/agent_broker.rs` (import ~7, `query` ~94)
- Modify: `crates/vmux_service/src/server.rs` (`query_result_to_content` ~169)
- Test: `crates/vmux_service/src/agent_broker.rs` tests

This makes `RecordStop` use `RECORD_STOP_TIMEOUT` and renders the `Recording` result for non-MCP clients.

- [ ] **Step 1: Write a failing test for timeout selection**

Factor the timeout into a pure fn so it is testable. Add to `crates/vmux_service/src/agent_broker.rs` (top-level, not in impl):

```rust
fn query_timeout(query: &AgentQuery) -> std::time::Duration {
    match query {
        AgentQuery::RecordStop { .. } => vmux_core_record_stop_timeout(),
        _ => AGENT_QUERY_TIMEOUT,
    }
}
```

Wait — `RECORD_STOP_TIMEOUT` lives in `crate::protocol`. Use it directly instead of a wrapper:

```rust
fn query_timeout(query: &AgentQuery) -> std::time::Duration {
    match query {
        AgentQuery::RecordStop { .. } => crate::protocol::RECORD_STOP_TIMEOUT,
        _ => AGENT_QUERY_TIMEOUT,
    }
}
```

Add a test at the bottom of `agent_broker.rs` (create a `#[cfg(test)] mod tests` if none exists):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_stop_gets_longer_timeout() {
        let stop = AgentQuery::RecordStop { dir: None, name: None };
        assert_eq!(query_timeout(&stop), crate::protocol::RECORD_STOP_TIMEOUT);
        let other = AgentQuery::GetSettings;
        assert_eq!(query_timeout(&other), AGENT_QUERY_TIMEOUT);
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p vmux_service --lib agent_broker::tests::record_stop_gets_longer_timeout`
Expected: FAIL — `query_timeout` not found (or AgentQuery import missing).

- [ ] **Step 3: Wire `query_timeout` into `query`**

Ensure `AgentQuery` is imported in `agent_broker.rs` (the `use crate::protocol::{...}` already imports `AgentCommand` etc.; add `AgentQuery` if absent — it is used in the `query` signature so it is already in scope).

Replace the timeout line in `query` (~94):

```rust
        match tokio::time::timeout(AGENT_QUERY_TIMEOUT, rx).await {
```

with:

```rust
        match tokio::time::timeout(query_timeout(&query), rx).await {
```

Note: `query` is moved into the `ServiceMessage::AgentQuery { request_id, query }` send above (~87). Compute the timeout **before** that send: add `let timeout = query_timeout(&query);` immediately after the `oneshot::channel` line (~82), and use `tokio::time::timeout(timeout, rx)`.

- [ ] **Step 4: Add the `server.rs` content arm**

In `query_result_to_content` (`crates/vmux_service/src/server.rs:169`), add before the `Error` arm:

```rust
        AgentQueryResult::Recording {
            mp4_path,
            gif_path,
            duration_ms,
            bytes,
            auto_stopped,
        } => {
            let secs = duration_ms as f64 / 1000.0;
            let gif = gif_path
                .map(|g| format!(" + {g}"))
                .unwrap_or_default();
            let auto = if auto_stopped { " (auto-stopped)" } else { "" };
            (
                format!("recorded {secs:.1}s -> {mp4_path} ({bytes} bytes){gif}{auto}"),
                false,
            )
        }
```

- [ ] **Step 5: Run tests + build, verify pass**

Run: `cargo test -p vmux_service --lib`
Expected: PASS (new test green; existing match arms now exhaustive — build succeeds).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_service/src/agent_broker.rs crates/vmux_service/src/server.rs
git commit -m "feat(service): longer RecordStop timeout + Recording content"
```

---

## Task 3: Agent messages (vmux_agent)

**Files:**
- Modify: `crates/vmux_agent/src/events.rs` (after `ScreenshotResponse` ~75)

These compile-only message types mirror `ScreenshotRequest/Image/Response`.

- [ ] **Step 1: Add the message types**

Append to `crates/vmux_agent/src/events.rs`:

```rust
#[derive(Message, Clone)]
pub struct RecordStartRequest {
    pub request_id: [u8; 16],
    pub gif: bool,
    pub max_secs: u32,
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct RecordStartResponse {
    pub request_id: [u8; 16],
    /// On success: the effective `max_secs` (for the ack text).
    pub result: Result<u32, String>,
}

#[derive(Message, Clone)]
pub struct RecordStopRequest {
    pub request_id: [u8; 16],
    pub dir: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone)]
pub struct RecordingInfo {
    pub mp4_path: String,
    pub gif_path: Option<String>,
    pub duration_ms: u64,
    pub bytes: u64,
    pub auto_stopped: bool,
}

#[derive(Message, Clone)]
pub struct RecordStopResponse {
    pub request_id: [u8; 16],
    pub result: Result<RecordingInfo, String>,
}
```

- [ ] **Step 2: Verify it compiles + re-export check**

Confirm `crates/vmux_agent/src/lib.rs` re-exports `events::*` (the screenshot types are imported elsewhere as `vmux_agent::ScreenshotRequest`). Grep:

Run: `grep -n 'pub use' crates/vmux_agent/src/lib.rs | grep -i event`
Expected: a `pub use events::*;` (or explicit list). If explicit, add the four new `RecordStart*`/`RecordStop*` + `RecordingInfo` names.

Run: `cargo build -p vmux_agent`
Expected: builds clean.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/events.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(agent): record start/stop message types"
```

---

## Task 4: Agent plugin wiring (vmux_agent)

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (imports ~37, registration ~112/176, `handle_agent_queries` ~964, forwarders ~1066, tests ~1713)

- [ ] **Step 1: Write failing mapping tests**

In the `#[cfg(test)] mod tests` of `plugin.rs` (near `screenshot_response_maps_ok_and_err` ~1713), add:

```rust
    #[test]
    fn record_start_response_maps_ok_and_err() {
        let ok = record_start_response_to_query_result(&Ok(120));
        assert!(matches!(ok, AgentQueryResult::Text(t) if t.contains("120")));
        let err = record_start_response_to_query_result(&Err("nope".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
    }

    #[test]
    fn record_stop_response_maps_ok_and_err() {
        let ok = record_stop_response_to_query_result(&Ok(RecordingInfo {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: None,
            duration_ms: 1000,
            bytes: 42,
            auto_stopped: false,
        }));
        assert!(matches!(ok, AgentQueryResult::Recording { mp4_path, .. } if mp4_path == "/tmp/x.mp4"));
        let err = record_stop_response_to_query_result(&Err("boom".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "boom"));
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p vmux_agent --lib plugin::tests::record_`
Expected: FAIL — functions not found; `RecordingInfo` unresolved.

- [ ] **Step 3: Extend imports**

In `plugin.rs` imports (~37-38) add the new names to the `vmux_agent` / `events` import group that currently lists `ScreenshotImage, ScreenshotRequest, ScreenshotResponse`:

```rust
    AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, CommandOrigin, RecordingInfo,
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, ScreenshotImage,
    ScreenshotRequest, ScreenshotResponse,
```

(Match the existing import path — same `use` that brings in `ScreenshotRequest`.)

- [ ] **Step 4: Add mapping fns + forwarders**

After `forward_screenshot_responses` (~1091) add:

```rust
fn record_start_response_to_query_result(result: &Result<u32, String>) -> AgentQueryResult {
    match result {
        Ok(max_secs) => AgentQueryResult::Text(format!("recording started, max {max_secs}s")),
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_record_start_responses(
    mut reader: MessageReader<RecordStartResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: record_start_response_to_query_result(&response.result),
        });
    }
}

fn record_stop_response_to_query_result(
    result: &Result<RecordingInfo, String>,
) -> AgentQueryResult {
    match result {
        Ok(info) => AgentQueryResult::Recording {
            mp4_path: info.mp4_path.clone(),
            gif_path: info.gif_path.clone(),
            duration_ms: info.duration_ms,
            bytes: info.bytes,
            auto_stopped: info.auto_stopped,
        },
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_record_stop_responses(
    mut reader: MessageReader<RecordStopResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: record_stop_response_to_query_result(&response.result),
        });
    }
}
```

- [ ] **Step 5: Add query writers + arms in `handle_agent_queries`**

Add two writer params after `screenshot_writer` (~978):

```rust
    mut record_start_writer: MessageWriter<RecordStartRequest>,
    mut record_stop_writer: MessageWriter<RecordStopRequest>,
```

Add arms after the `Screenshot` arm (~1026), before the `ReadTerminal | ...` catch-all:

```rust
            AgentQuery::RecordStart {
                gif,
                max_secs,
                ref pane,
            } => {
                record_start_writer.write(RecordStartRequest {
                    request_id: request.request_id.0,
                    gif,
                    max_secs,
                    pane: pane.clone(),
                });
            }
            AgentQuery::RecordStop { ref dir, ref name } => {
                record_stop_writer.write(RecordStopRequest {
                    request_id: request.request_id.0,
                    dir: dir.clone(),
                    name: name.clone(),
                });
            }
```

- [ ] **Step 6: Register messages + systems**

In `build` (~112-113) after the two `ScreenshotResponse`/`Request` `.add_message`:

```rust
            .add_message::<RecordStartRequest>()
            .add_message::<RecordStartResponse>()
            .add_message::<RecordStopRequest>()
            .add_message::<RecordStopResponse>()
```

In the forwarder `add_systems` group (~171-178, with `forward_screenshot_responses`):

```rust
                    forward_record_start_responses,
                    forward_record_stop_responses,
```

- [ ] **Step 7: Run tests, verify pass**

Run: `cargo test -p vmux_agent --lib plugin::tests::record_`
Expected: PASS (2 tests). Also `cargo build -p vmux_agent` clean.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): forward record start/stop queries to messages"
```

---

## Task 5: MCP tool definitions + dispatch (vmux_mcp)

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs` (defs ~400, `tool_definitions` ~420, dispatch ~570, tests ~615)

- [ ] **Step 1: Write failing tests**

In `tools.rs` `mod tests`, add a query-dispatch helper (next to `dispatch_command` ~627) and tests:

```rust
    fn dispatch_query(name: &str, args: serde_json::Value) -> Result<AgentQuery, String> {
        match dispatch_from_tool_call(name, args)? {
            DispatchTarget::Query(q) => Ok(q),
            DispatchTarget::Command(_) => Err("expected Query, got Command".to_string()),
        }
    }

    #[test]
    fn record_tools_are_listed() {
        let names = tool_names();
        assert!(names.contains(&"vmux_record_start".to_string()));
        assert!(names.contains(&"vmux_record_stop".to_string()));
    }

    #[test]
    fn record_start_dispatch_defaults() {
        let q = dispatch_query("record_start", serde_json::json!({})).unwrap();
        assert_eq!(
            q,
            AgentQuery::RecordStart {
                gif: false,
                max_secs: 120,
                pane: None
            }
        );
    }

    #[test]
    fn record_start_dispatch_args() {
        let q = dispatch_query(
            "record_start",
            serde_json::json!({"gif": true, "max_secs": 30, "pane": "pane:3"}),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::RecordStart {
                gif: true,
                max_secs: 30,
                pane: Some("pane:3".into())
            }
        );
    }

    #[test]
    fn record_stop_dispatch_args() {
        let q = dispatch_query(
            "record_stop",
            serde_json::json!({"dir": "/tmp/out", "name": "feature-x"}),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::RecordStop {
                dir: Some("/tmp/out".into()),
                name: Some("feature-x".into())
            }
        );
        let empty = dispatch_query("record_stop", serde_json::json!({})).unwrap();
        assert_eq!(empty, AgentQuery::RecordStop { dir: None, name: None });
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p vmux_mcp --lib tools::tests::record_`
Expected: FAIL — `unknown tool: record_start`; defs missing.

- [ ] **Step 3: Add tool definitions**

After `screenshot_definition` (~400) add:

```rust
fn record_start_definition() -> ToolDefinition {
    ToolDefinition {
        name: "vmux_record_start".into(),
        description: "Start recording the vmux window to an mp4 video (optionally also a GIF). \
Returns immediately so you can drive the UI with other tools to demonstrate a feature, then call \
vmux_record_stop. Auto-stops after `max_secs` (default 120) as a safety cap. Only one recording at a \
time. macOS only; the first call may prompt for Screen Recording permission - grant it in System \
Settings > Privacy & Security > Screen Recording, then call again."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "gif": {"type": "boolean", "description": "Also emit a GIF next to the mp4 (default false)."},
                "max_secs": {"type": "integer", "description": "Auto-stop cap in seconds (default 120)."},
                "pane": {"type": "string", "description": "Optional pane:<id> or stack:<id> to crop to; whole window if omitted."}
            }
        }),
    }
}

fn record_stop_definition() -> ToolDefinition {
    ToolDefinition {
        name: "vmux_record_stop".into(),
        description: "Stop the active recording and write the file(s). Returns the mp4 path, duration, \
and size (plus the GIF path if one was requested). By default saves to ~/.vmux/screenshots/; pass `dir` \
(absolute) and `name` (basename, no extension) to save elsewhere - e.g. dir=<repo>/docs/features, \
name=<feature> to drop a demo straight into the repo."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "dir": {"type": "string", "description": "Absolute output directory (default ~/.vmux/screenshots)."},
                "name": {"type": "string", "description": "Output basename without extension (default vmux-<timestamp>)."}
            }
        }),
    }
}
```

- [ ] **Step 4: Register definitions**

In `tool_definitions` (~420), after `defs.push(screenshot_definition());`:

```rust
    defs.push(record_start_definition());
    defs.push(record_stop_definition());
```

- [ ] **Step 5: Add dispatch arms**

After the `screenshot` arm (~570), before `if name == "read_layout"`:

```rust
    if name == "record_start" {
        let gif = arguments.get("gif").and_then(Value::as_bool).unwrap_or(false);
        let max_secs = arguments
            .get("max_secs")
            .and_then(Value::as_u64)
            .unwrap_or(120) as u32;
        let pane = match arguments.get("pane") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            Some(_) => return Err("record_start.pane must be a string".to_string()),
        };
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::RecordStart {
                gif,
                max_secs,
                pane,
            },
        ));
    }
    if name == "record_stop" {
        let parse_opt = |key: &str| match arguments.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => {
                let s = s.trim();
                Ok((!s.is_empty()).then(|| s.to_string()))
            }
            Some(_) => Err(format!("record_stop.{key} must be a string")),
        };
        let dir = parse_opt("dir")?;
        let out_name = parse_opt("name")?;
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::RecordStop { dir, name: out_name },
        ));
    }
```

- [ ] **Step 6: Run tests, verify pass**

Run: `cargo test -p vmux_mcp --lib tools::tests::record_`
Expected: PASS (4 tests). Also the `list_tools` `all().starts_with("vmux_")` invariant still holds.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): vmux_record_start / vmux_record_stop tools"
```

---

## Task 6: MCP response mapping (vmux_mcp)

**Files:**
- Modify: `crates/vmux_mcp/src/protocol.rs` (`query_result_to_mcp_response` ~368)

- [ ] **Step 1: Write failing test**

Add a `#[cfg(test)] mod tests` to `crates/vmux_mcp/src/protocol.rs` (or extend the existing one):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::AgentQueryResult;

    #[test]
    fn recording_maps_to_text_block() {
        let v = query_result_to_mcp_response(AgentQueryResult::Recording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_000_000,
            auto_stopped: true,
        });
        let text = v["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("/tmp/x.mp4"));
        assert!(text.contains("/tmp/x.gif"));
        assert!(text.contains("auto-stopped"));
        assert!(v.get("isError").is_none());
    }
}
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p vmux_mcp --lib protocol::tests::recording_maps_to_text_block`
Expected: FAIL — non-exhaustive match / variant unhandled.

- [ ] **Step 3: Add the arm**

In `query_result_to_mcp_response` (~368), before the `Error` arm:

```rust
        AgentQueryResult::Recording {
            mp4_path,
            gif_path,
            duration_ms,
            bytes,
            auto_stopped,
        } => {
            let secs = duration_ms as f64 / 1000.0;
            let mut text = format!("recorded {secs:.1}s → {mp4_path} ({bytes} bytes)");
            if let Some(g) = gif_path {
                text.push_str(&format!(" + {g}"));
            }
            if auto_stopped {
                text.push_str(" (auto-stopped)");
            }
            json!({
                "content": [{"type": "text", "text": text}]
            })
        }
```

- [ ] **Step 4: Run, verify pass**

Run: `cargo test -p vmux_mcp --lib protocol::tests::recording_maps_to_text_block`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_mcp/src/protocol.rs
git commit -m "feat(mcp): render Recording query result as text"
```

**Checkpoint:** The full IPC + MCP path now compiles end-to-end with the desktop side still missing. `start_recording`/`stop_recording` don't exist yet, so `RecordStartRequest`/`RecordStopRequest` are written but never consumed (harmless). Run `cargo build -p vmux_mcp -p vmux_service -p vmux_agent` — expected clean.

---

## Task 7: Desktop dependencies (vmux_desktop)

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml`

- [ ] **Step 1: Add cross-platform deps**

In `[dependencies]` (after `image = ...` ~57):

```toml
gif = "0.13"
color_quant = "1.1"
```

- [ ] **Step 2: Extend the screen-capture-kit features + add AVFoundation/CoreMedia/CoreVideo**

In `[target.'cfg(target_os = "macos")'.dependencies]`, replace the `objc2-screen-capture-kit` feature list to add the stream-config/content-filter/output types, and add the new crates:

```toml
objc2-screen-capture-kit = { version = "0.3", features = [
    "SCStream",
    "SCShareableContent",
    "SCScreenshotManager",
    "SCStreamConfiguration",
    "SCContentFilter",
    "objc2-core-graphics",
    "block2",
] }
objc2-av-foundation = { version = "0.3", features = [
    "AVAssetWriter",
    "AVAssetWriterInput",
    "AVMediaFormat",
    "AVVideoSettings",
    "objc2-core-media",
] }
objc2-core-media = { version = "0.3", features = [
    "CMSampleBuffer",
    "CMTime",
    "CMFormatDescription",
] }
objc2-core-video = { version = "0.3", features = [
    "CVPixelBuffer",
    "CVImageBuffer",
    "CVReturn",
] }
dispatch2 = "0.3"
```

Notes for the implementer:
- Exact feature names vary by `objc2-*` minor version. If a feature is rejected, run `cargo doc -p objc2-av-foundation --open` (or check docs.rs) and enable the module that exports `AVAssetWriterInputPixelBufferAdaptor`, `AVVideoCodecKey`, etc. Pin versions to match the workspace's existing `objc2 = 0.6` family.
- `dispatch2` provides the serial `DispatchQueue` for the SCStream output callback.

- [ ] **Step 3: Verify deps resolve**

Run: `cargo fetch && cargo build -p vmux_desktop`
Expected: builds (no recording.rs yet — only dep resolution matters; the crate still compiles).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml Cargo.lock
git commit -m "build(desktop): add AVFoundation + gif deps for recording"
```

---

## Task 8: Recording helpers + systems + non-macOS stub (vmux_desktop)

**Files:**
- Create: `crates/vmux_desktop/src/recording.rs`
- Test: same file `#[cfg(test)] mod tests`

This task creates the **cross-platform** skeleton: the `RecordingBridge` resource, the `start_recording`/`drain_recordings`/`auto_stop_recordings` systems wired to Bevy messages, pure helpers (unit-tested), and a non-macOS capture stub. The macOS capture body lands in Task 10 as `mod capture` (referenced here but stubbed to compile via the non-macOS path first).

To keep this task self-contained and green on Linux CI, write the capture module boundary so the file compiles on all platforms now, with macOS capture delegated to `capture::start`/`capture::stop` that Task 10 fills in. For this task, provide BOTH a non-macOS stub AND a temporary macOS stub (Task 10 replaces the macOS one).

- [ ] **Step 1: Write the helpers + failing tests**

Create `crates/vmux_desktop/src/recording.rs`:

```rust
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::sync::Arc;
use vmux_agent::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
};

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_FPS: u32 = 12;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_MAX_EDGE: u32 = 800;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PERMISSION_MSG: &str = "Screen Recording permission required - grant it in System Settings > \
Privacy & Security > Screen Recording, then call vmux_record_start again.";

/// Carries finalize outcomes from off-thread (stop/auto-stop) back to Bevy.
/// `request_id == None` means an auto-stop (no pending query to answer).
pub(crate) struct RecordOutcome {
    pub request_id: Option<[u8; 16]>,
    pub result: Result<RecordingInfo, String>,
}

#[derive(Resource)]
pub(crate) struct RecordingBridge {
    pub(crate) tx: Sender<RecordOutcome>,
    rx: Receiver<RecordOutcome>,
}

impl Default for RecordingBridge {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn start_err(request_id: [u8; 16], message: impl Into<String>) -> RecordStartResponse {
    RecordStartResponse {
        request_id,
        result: Err(message.into()),
    }
}

/// Resolve dir/name into final mp4 + optional gif paths.
pub(crate) fn resolve_output_paths(
    dir: Option<&str>,
    name: Option<&str>,
    gif: bool,
    timestamp: &str,
) -> (PathBuf, Option<PathBuf>) {
    let base_dir = dir
        .map(PathBuf::from)
        .unwrap_or_else(vmux_core::profile::screenshots_dir);
    let base_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("vmux-{timestamp}"));
    let mp4 = base_dir.join(format!("{base_name}.mp4"));
    let gif_path = gif.then(|| base_dir.join(format!("{base_name}.gif")));
    (mp4, gif_path)
}

/// Whether a frame at `elapsed_ms` should be sampled into the GIF given the
/// last sampled timestamp and target fps.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn should_sample_gif_frame(
    elapsed_ms: u64,
    last_sampled_ms: Option<u64>,
    fps: u32,
) -> bool {
    let interval = (1000 / fps.max(1)) as u64;
    match last_sampled_ms {
        None => true,
        Some(last) => elapsed_ms.saturating_sub(last) >= interval,
    }
}

/// BGRA (ScreenCaptureKit native) → RGBA (image/gif crates).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; bgra.len()];
    for (i, px) in bgra.chunks_exact(4).enumerate() {
        let o = i * 4;
        out[o] = px[2];
        out[o + 1] = px[1];
        out[o + 2] = px[0];
        out[o + 3] = px[3];
    }
    out
}

/// Cap the long edge at `max_edge`, never upscaling. Mirrors screenshot's
/// `downscale_dims`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn downscale_to(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long == 0 {
        return (1, 1);
    }
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let scale = max_edge as f64 / long as f64;
    (
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub(crate) fn crop_rect_from_node(
    center_x: f32,
    center_y: f32,
    size_x: f32,
    size_y: f32,
    img_w: u32,
    img_h: u32,
) -> CropRect {
    let left = (center_x - size_x * 0.5).round().max(0.0) as u32;
    let top = (center_y - size_y * 0.5).round().max(0.0) as u32;
    let left = left.min(img_w.saturating_sub(1));
    let top = top.min(img_h.saturating_sub(1));
    let w = (size_x.round().max(1.0) as u32).min(img_w - left);
    let h = (size_y.round().max(1.0) as u32).min(img_h - top);
    CropRect { x: left, y: top, w, h }
}

fn resolve_crop(
    id: &str,
    node_q: &Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: &Query<&ChildOf>,
    img_w: u32,
    img_h: u32,
) -> Option<CropRect> {
    use bevy::ecs::relationship::Relationship;
    let (_, bits) = vmux_layout::protocol::parse_id(id).ok()?;
    let mut entity = Entity::from_bits(bits);
    for _ in 0..8 {
        if let Ok((computed, gt)) = node_q.get(entity) {
            let size = computed.size;
            let center = gt.transform_point2(Vec2::ZERO);
            return Some(crop_rect_from_node(
                center.x, center.y, size.x, size.y, img_w, img_h,
            ));
        }
        entity = child_of_q.get(entity).ok()?.get();
    }
    None
}

pub(crate) fn start_recording(
    _non_send: NonSendMarker,
    mut start_reader: MessageReader<RecordStartRequest>,
    mut stop_reader: MessageReader<RecordStopRequest>,
    mut start_responses: MessageWriter<RecordStartResponse>,
    bridge: Res<RecordingBridge>,
    window_q: Query<(Entity, &Window), With<PrimaryWindow>>,
    node_q: Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: Query<&ChildOf>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    for req in start_reader.read() {
        let Ok((window_entity, window)) = window_q.single() else {
            start_responses.write(start_err(req.request_id, "no primary vmux window"));
            continue;
        };
        let img_w = window.resolution.physical_width();
        let img_h = window.resolution.physical_height();
        let crop = match &req.pane {
            Some(id) => match resolve_crop(id, &node_q, &child_of_q, img_w, img_h) {
                Some(rect) => Some(rect),
                None => {
                    start_responses.write(start_err(req.request_id, format!("pane not found: {id}")));
                    continue;
                }
            },
            None => None,
        };
        let wake: Option<WakeFn> = proxy.as_ref().map(|p| {
            let proxy = (***p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as WakeFn
        });
        let resp = capture::start(
            window_entity,
            img_w,
            img_h,
            crop,
            req.request_id,
            req.gif,
            req.max_secs,
            bridge.tx.clone(),
            wake,
        );
        start_responses.write(resp);
    }

    for req in stop_reader.read() {
        capture::stop(req.request_id, req.dir.clone(), req.name.clone());
    }
}

pub(crate) fn auto_stop_recordings(_non_send: NonSendMarker) {
    capture::poll_auto_stop();
}

pub(crate) fn drain_recordings(
    bridge: Res<RecordingBridge>,
    mut last_auto: Local<Option<RecordingInfo>>,
    mut stop_responses: MessageWriter<RecordStopResponse>,
) {
    while let Ok(outcome) = bridge.rx.try_recv() {
        match outcome.request_id {
            Some(request_id) => {
                let result = match (&outcome.result, last_auto.take()) {
                    (Err(_), Some(info)) => Ok(info),
                    (r, _) => r.clone(),
                };
                stop_responses.write(RecordStopResponse { request_id, result });
            }
            None => {
                if let Ok(info) = outcome.result {
                    *last_auto = Some(info);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_paths_default_dir_and_name() {
        let (mp4, gif) = resolve_output_paths(None, None, false, "20260623-101010-001");
        assert!(mp4.ends_with("vmux-20260623-101010-001.mp4"));
        assert!(mp4.starts_with(vmux_core::profile::screenshots_dir()));
        assert!(gif.is_none());
    }

    #[test]
    fn output_paths_custom_dir_name_and_gif() {
        let (mp4, gif) = resolve_output_paths(Some("/tmp/out"), Some("feature-x"), true, "ts");
        assert_eq!(mp4, PathBuf::from("/tmp/out/feature-x.mp4"));
        assert_eq!(gif, Some(PathBuf::from("/tmp/out/feature-x.gif")));
    }

    #[test]
    fn gif_sampling_respects_fps() {
        assert!(should_sample_gif_frame(0, None, 12));
        assert!(!should_sample_gif_frame(40, Some(0), 12)); // 40ms < ~83ms interval
        assert!(should_sample_gif_frame(90, Some(0), 12));
    }

    #[test]
    fn bgra_to_rgba_swaps_channels() {
        let bgra = vec![1u8, 2, 3, 4]; // B G R A
        assert_eq!(bgra_to_rgba(&bgra), vec![3, 2, 1, 4]); // R G B A
    }

    #[test]
    fn crop_rect_clamps_to_image() {
        let r = crop_rect_from_node(100.0, 100.0, 80.0, 60.0, 1000, 1000);
        assert_eq!(r, CropRect { x: 60, y: 70, w: 80, h: 60 });
    }

    #[test]
    fn downscale_caps_long_edge_without_upscaling() {
        assert_eq!(downscale_to(800, 600, 800), (800, 600));
        assert_eq!(downscale_to(1600, 800, 800), (800, 400));
        assert_eq!(downscale_to(0, 0, 800), (1, 1));
    }
}

#[cfg(not(target_os = "macos"))]
mod capture {
    use super::{CropRect, RecordOutcome, RecordingInfo, WakeFn};
    use bevy::prelude::Entity;
    use crossbeam_channel::Sender;
    use vmux_agent::RecordStartResponse;

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start(
        _window_entity: Entity,
        _img_w: u32,
        _img_h: u32,
        _crop: Option<CropRect>,
        request_id: [u8; 16],
        _gif: bool,
        _max_secs: u32,
        _tx: Sender<RecordOutcome>,
        _wake: Option<WakeFn>,
    ) -> RecordStartResponse {
        RecordStartResponse {
            request_id,
            result: Err("recording is only supported on macOS".to_string()),
        }
    }

    pub(crate) fn stop(_request_id: [u8; 16], _dir: Option<String>, _name: Option<String>) {}

    pub(crate) fn poll_auto_stop() {}

    #[allow(dead_code)]
    fn _unused(_: RecordingInfo) {}
}

#[cfg(target_os = "macos")]
#[path = "recording_capture_macos.rs"]
mod capture;
```

Note: the macOS `mod capture` is declared via `#[path = "recording_capture_macos.rs"]` and created in Task 10. To keep THIS task compiling on macOS too, create a minimal placeholder `crates/vmux_desktop/src/recording_capture_macos.rs` now (Task 10 replaces its body):

```rust
use super::{CropRect, RecordOutcome, WakeFn};
use bevy::prelude::Entity;
use crossbeam_channel::Sender;
use vmux_agent::RecordStartResponse;

#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    _window_entity: Entity,
    _img_w: u32,
    _img_h: u32,
    _crop: Option<CropRect>,
    request_id: [u8; 16],
    _gif: bool,
    _max_secs: u32,
    _tx: Sender<RecordOutcome>,
    _wake: Option<WakeFn>,
) -> RecordStartResponse {
    RecordStartResponse {
        request_id,
        result: Err("recording not yet implemented".to_string()),
    }
}

pub(crate) fn stop(_request_id: [u8; 16], _dir: Option<String>, _name: Option<String>) {}

pub(crate) fn poll_auto_stop() {}
```

- [ ] **Step 2: Declare the module**

In `crates/vmux_desktop/src/lib.rs`, near `mod screenshot;` (~23) add:

```rust
mod recording;
```

- [ ] **Step 3: Run helper tests, verify pass**

Run: `cargo test -p vmux_desktop --lib recording::tests`
Expected: PASS (6 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/recording.rs crates/vmux_desktop/src/recording_capture_macos.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): recording helpers, systems, capture stubs"
```

---

## Task 9: Desktop plugin registration (vmux_desktop)

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs` (~138-148)

- [ ] **Step 1: Register bridge + systems**

After `.init_resource::<screenshot::ScreenshotBridge>()` (~138):

```rust
            .init_resource::<recording::RecordingBridge>()
```

Extend the screenshot `add_systems` group (~143-148) — chain the recording systems alongside:

```rust
            .add_systems(
                Update,
                (
                    screenshot::start_screenshots,
                    screenshot::drain_screenshots,
                    recording::start_recording,
                    recording::auto_stop_recordings,
                    recording::drain_recordings,
                )
                    .chain()
                    .after(WriteAppCommands),
            );
```

- [ ] **Step 2: Build, verify clean**

Run: `cargo build -p vmux_desktop`
Expected: builds. (macOS path uses the placeholder capture; recording returns "not yet implemented" at runtime.)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): register recording bridge + systems"
```

---

## Task 10: macOS native capture (vmux_desktop)

**Files:**
- Modify (replace placeholder body): `crates/vmux_desktop/src/recording_capture_macos.rs`

**Verification model:** This is native objc2 glue against ScreenCaptureKit + AVFoundation. Like `screenshot.rs::capture`, it is **not unit-tested** — it is iterated against the compiler and **verified manually** by recording a real window (Step 6). The exact `objc2-*` selector/type names depend on the installed crate versions; expect to adjust signatures using `cargo doc` / docs.rs while compiling. Do not claim done until Step 6 produces a playable mp4.

**Architecture (read before coding):**
- One serial `dispatch2::DispatchQueue` receives `SCStream` sample buffers via a `define_class!` delegate implementing `SCStreamOutput`.
- A single `RecordingState` (`Arc`) holds all encode state; objc `Retained` objects are wrapped in a `SendRetained` newtype (`unsafe impl Send`/`Sync`) because they cross from the main thread (create/stop) to the dispatch queue + `finishWriting` completion thread. All access is serialized through a `Mutex`, and stream frames arrive on the single serial queue.
- A process-global `static ACTIVE: Mutex<Option<Arc<RecordingState>>>` lets `stop()` / `poll_auto_stop()` (called from the Bevy system, no session arg) reach the live recording.
- mp4: `AVAssetWriter` + `AVAssetWriterInput` (codec H.264) + `AVAssetWriterInputPixelBufferAdaptor`. First sample → `startSession(atSourceTime:)`. Each sample → `adaptor.appendPixelBuffer:withPresentationTime:`.
- gif (optional): a worker thread owns a `gif::Encoder<BufWriter<File>>`; the delegate samples frames at `GIF_FPS`, converts BGRA→RGBA (`super::bgra_to_rgba`), downscales to `GIF_MAX_EDGE`, and `try_send`s to the worker (drop on full). Joined during finalize.

- [ ] **Step 1: Imports, Send wrapper, globals**

Replace the entire `recording_capture_macos.rs` with the implementation below (adjust names to the crate APIs as needed):

```rust
use super::{
    CropRect, GIF_FPS, GIF_MAX_EDGE, PERMISSION_MSG, RecordOutcome, RecordingInfo, WakeFn,
    bgra_to_rgba, downscale_to, resolve_output_paths, should_sample_gif_frame,
};
use bevy::prelude::Entity;
use block2::RcBlock;
use crossbeam_channel::Sender;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, define_class, msg_send};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;
use vmux_agent::RecordStartResponse;

unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// objc `Retained<T>` is not auto-`Send`. We hand these between the main thread
/// and SCStream's dispatch queue / `finishWriting` completion thread, with all
/// access guarded by `RecordingState`'s `Mutex` and SCStream's serial queue.
struct SendRetained<T>(Retained<T>);
unsafe impl<T> Send for SendRetained<T> {}
unsafe impl<T> Sync for SendRetained<T> {}

type GifMsg = (Vec<u8>, u32, u32); // rgba, w, h

struct EncodeState {
    writer: SendRetained<objc2_av_foundation::AVAssetWriter>,
    input: SendRetained<objc2_av_foundation::AVAssetWriterInput>,
    adaptor: SendRetained<objc2_av_foundation::AVAssetWriterInputPixelBufferAdaptor>,
    started_session: bool,
    crop: Option<CropRect>,
    start: Instant,
    last_gif_ms: Option<u64>,
    gif_tx: Option<Sender<GifMsg>>,
}

struct RecordingState {
    stream: Mutex<Option<SendRetained<objc2_screen_capture_kit::SCStream>>>,
    encode: Mutex<EncodeState>,
    gif_join: Mutex<Option<JoinHandle<()>>>,
    temp_mp4: PathBuf,
    temp_gif: Option<PathBuf>,
    gif: bool,
    max_secs: u32,
    deadline: Instant,
    out: Mutex<FinalizeTarget>,
    tx: Sender<RecordOutcome>,
    wake: Option<WakeFn>,
}

#[derive(Default, Clone)]
struct FinalizeTarget {
    dir: Option<String>,
    name: Option<String>,
    request_id: Option<[u8; 16]>,
    finalizing: bool,
}

fn active() -> &'static Mutex<Option<Arc<RecordingState>>> {
    static ACTIVE: OnceLock<Mutex<Option<Arc<RecordingState>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}
```

- [ ] **Step 2: The SCStreamOutput delegate**

```rust
define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[name = "VmuxStreamOutput"]
    #[ivars = Arc<RecordingState>]
    struct StreamOutput;

    unsafe impl objc2_screen_capture_kit::SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(
            &self,
            _stream: &objc2_screen_capture_kit::SCStream,
            sample: &objc2_core_media::CMSampleBuffer,
            kind: objc2_screen_capture_kit::SCStreamOutputType,
        ) {
            if kind != objc2_screen_capture_kit::SCStreamOutputType::Screen {
                return;
            }
            let state = self.ivars().clone();
            handle_sample(&state, sample);
        }
    }
);

fn handle_sample(state: &Arc<RecordingState>, sample: &objc2_core_media::CMSampleBuffer) {
    use objc2_core_media::{CMSampleBufferGetImageBuffer, CMSampleBufferGetPresentationTimeStamp};
    let mut enc = state.encode.lock().unwrap();
    let Some(pixel_buffer) = (unsafe { CMSampleBufferGetImageBuffer(sample) }) else {
        return;
    };
    let pts = unsafe { CMSampleBufferGetPresentationTimeStamp(sample) };

    // mp4: start session on first frame, then append while ready.
    if !enc.started_session {
        unsafe { enc.writer.0.startSessionAtSourceTime(pts) };
        enc.started_session = true;
    }
    if unsafe { enc.input.0.isReadyForMoreMediaData() } {
        unsafe {
            enc.adaptor
                .0
                .appendPixelBuffer_withPresentationTime(&pixel_buffer, pts);
        }
    }

    // gif: sample at GIF_FPS, convert + downscale, push to worker.
    if let Some(gif_tx) = enc.gif_tx.clone() {
        let elapsed_ms = enc.start.elapsed().as_millis() as u64;
        if should_sample_gif_frame(elapsed_ms, enc.last_gif_ms, GIF_FPS) {
            enc.last_gif_ms = Some(elapsed_ms);
            if let Some((rgba, w, h)) = pixel_buffer_to_downscaled_rgba(&pixel_buffer, enc.crop) {
                let _ = gif_tx.try_send((rgba, w, h));
            }
        }
    }
}
```

`pixel_buffer_to_downscaled_rgba`: lock the `CVPixelBuffer` base address, read BGRA rows (respect `bytesPerRow` padding), apply `crop`, `bgra_to_rgba`, then downscale to `GIF_MAX_EDGE` via `image::imageops`:

```rust
fn pixel_buffer_to_downscaled_rgba(
    pb: &objc2_core_video::CVPixelBuffer,
    crop: Option<CropRect>,
) -> Option<(Vec<u8>, u32, u32)> {
    use objc2_core_video::{
        CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
        CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferUnlockBaseAddress,
    };
    unsafe {
        CVPixelBufferLockBaseAddress(pb, objc2_core_video::CVPixelBufferLockFlags::ReadOnly);
        let w = CVPixelBufferGetWidth(pb) as u32;
        let h = CVPixelBufferGetHeight(pb) as u32;
        let stride = CVPixelBufferGetBytesPerRow(pb);
        let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
        if base.is_null() || w == 0 || h == 0 {
            CVPixelBufferUnlockBaseAddress(pb, objc2_core_video::CVPixelBufferLockFlags::ReadOnly);
            return None;
        }
        // Tight-pack BGRA from a possibly-padded buffer.
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        for row in 0..h as usize {
            let src = base.add(row * stride);
            let dst = bgra.as_mut_ptr().add(row * w as usize * 4);
            std::ptr::copy_nonoverlapping(src, dst, w as usize * 4);
        }
        CVPixelBufferUnlockBaseAddress(pb, objc2_core_video::CVPixelBufferLockFlags::ReadOnly);

        let rgba = bgra_to_rgba(&bgra);
        let mut img = image::RgbaImage::from_raw(w, h, rgba)?;
        if let Some(c) = crop {
            img = image::imageops::crop_imm(&img, c.x, c.y, c.w, c.h).to_image();
        }
        let (dw, dh) = downscale_to(img.width(), img.height(), GIF_MAX_EDGE);
        let scaled = if (dw, dh) == (img.dimensions()) {
            img
        } else {
            image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle)
        };
        let (fw, fh) = scaled.dimensions();
        Some((scaled.into_raw(), fw, fh))
    }
}
```

`downscale_to` is already defined and tested in `recording.rs` (Task 8). It is imported via the `use super::{…}` block at the top of this module, so call it bare as `downscale_to(...)`.

- [ ] **Step 3: `start()`**

```rust
#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    window_entity: Entity,
    img_w: u32,
    img_h: u32,
    crop: Option<CropRect>,
    request_id: [u8; 16],
    gif: bool,
    max_secs: u32,
    tx: Sender<RecordOutcome>,
    wake: Option<WakeFn>,
) -> RecordStartResponse {
    let err = |m: String| RecordStartResponse { request_id, result: Err(m) };

    if active().lock().unwrap().is_some() {
        return err("a recording is already in progress; stop it first".into());
    }
    if !os_at_least_14() {
        return err("recording requires macOS 14 or later".into());
    }
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        unsafe { CGRequestScreenCaptureAccess() };
        return err(PERMISSION_MSG.into());
    }
    let Some(window_id) = window_number(window_entity) else {
        return err("cannot resolve native window".into());
    };

    // Output size (cropped region if set, else full window).
    let (out_w, out_h) = crop.map_or((img_w, img_h), |c| (c.w, c.h));

    // Temp paths under the screenshots dir.
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let rid: String = request_id[..4].iter().map(|b| format!("{b:02x}")).collect();
    let tmp_dir = vmux_core::profile::screenshots_dir();
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        return err(format!("cannot create {}: {e}", tmp_dir.display()));
    }
    let temp_mp4 = tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.mp4"));
    let temp_gif = gif.then(|| tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.gif")));

    // Build AVAssetWriter (H.264) + input + pixel-buffer adaptor.
    let (writer, input, adaptor) = match build_writer(&temp_mp4, out_w, out_h) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    unsafe {
        input.setExpectsMediaDataInRealTime(true);
        if !writer.startWriting() {
            return err("AVAssetWriter.startWriting failed".into());
        }
    }

    // gif worker thread.
    let (gif_tx, gif_join) = if let Some(path) = temp_gif.clone() {
        let (s, r) = crossbeam_channel::bounded::<GifMsg>(8);
        let join = std::thread::spawn(move || gif_worker(path, r));
        (Some(s), Some(join))
    } else {
        (None, None)
    };

    let start = Instant::now();
    let state = Arc::new(RecordingState {
        stream: Mutex::new(None),
        encode: Mutex::new(EncodeState {
            writer: SendRetained(writer),
            input: SendRetained(input),
            adaptor: SendRetained(adaptor),
            started_session: false,
            crop,
            start,
            last_gif_ms: None,
            gif_tx,
        }),
        gif_join: Mutex::new(gif_join),
        temp_mp4,
        temp_gif,
        gif,
        max_secs,
        deadline: start + std::time::Duration::from_secs(max_secs as u64),
        out: Mutex::new(FinalizeTarget::default()),
        tx,
        wake,
    });

    // Build SCStream against the vmux window, add our output on a serial queue.
    if let Err(e) = start_stream(&state, window_id, img_w, img_h) {
        return err(e);
    }

    *active().lock().unwrap() = Some(state);
    RecordStartResponse { request_id, result: Ok(max_secs) }
}
```

`build_writer`: create `AVAssetWriter::initWithURL_fileType_error` (NSURL fileURL from `temp_mp4`, `AVFileTypeMPEG4`), an `AVAssetWriterInput::initWithMediaType_outputSettings` with media type `AVMediaTypeVideo` and an `NSDictionary` of `{ AVVideoCodecKey: AVVideoCodecTypeH264, AVVideoWidthKey: out_w, AVVideoHeightKey: out_h }` (optionally `AVVideoCompressionPropertiesKey: { AVVideoAverageBitRateKey: ~6_000_000 }`), then `AVAssetWriterInputPixelBufferAdaptor::initWithAssetWriterInput_sourcePixelBufferAttributes` with `{ kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA }`, and `writer.addInput(&input)`. Return `(writer, input, adaptor)` as `Retained`.

`start_stream`: fetch `SCShareableContent` (use the same async `getShareableContentWithCompletionHandler` block pattern as `screenshot.rs`, but block on it with a `std::sync::mpsc` rendezvous so `start()` stays synchronous), find the `SCWindow` with matching `windowID`, build `SCContentFilter::initWithDesktopIndependentWindow:`, build `SCStreamConfiguration` and set `width=img_w`, `height=img_h`, `pixelFormat = kCVPixelFormatType_32BGRA` (via `setPixelFormat`), `minimumFrameInterval = CMTime{value:1,timescale:60}`, `showsCursor = true`, `queueDepth = 6`. Create `SCStream::initWithFilter_configuration_delegate(filter, config, None)`, create the `StreamOutput` delegate with `Retained` set to `state.clone()`, add it via `addStreamOutput_type_sampleHandlerQueue_error(ProtocolObject::from_ref(&*delegate), Screen, &queue)` (a `dispatch2::DispatchQueue::new("ai.vmux.recording", serial)`), then `startCaptureWithCompletionHandler:` (log any error in the block). Store the stream in `state.stream`.

- [ ] **Step 4: gif worker**

```rust
fn gif_worker(path: PathBuf, rx: crossbeam_channel::Receiver<GifMsg>) {
    let Ok(file) = std::fs::File::create(&path) else { return };
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder: Option<gif::Encoder<&mut std::io::BufWriter<std::fs::File>>> = None;
    let delay = (100 / GIF_FPS.max(1)) as u16; // centiseconds
    while let Ok((rgba, w, h)) = rx.recv() {
        let enc = encoder.get_or_insert_with(|| {
            let mut e = gif::Encoder::new(&mut writer, w as u16, h as u16, &[]).unwrap();
            let _ = e.set_repeat(gif::Repeat::Infinite);
            e
        });
        let nq = color_quant::NeuQuant::new(10, 256, &rgba);
        let indices: Vec<u8> = rgba
            .chunks_exact(4)
            .map(|p| nq.index_of(p) as u8)
            .collect();
        let mut frame = gif::Frame {
            width: w as u16,
            height: h as u16,
            delay,
            ..Default::default()
        };
        frame.buffer = std::borrow::Cow::Owned(indices);
        frame.palette = Some(nq.color_map_rgb());
        let _ = enc.write_frame(&frame);
    }
    // channel closed → drop encoder writes the trailer.
}
```

(If the `gif::Encoder` lifetime against `&mut BufWriter` is awkward, own the `File` directly: `gif::Encoder<std::fs::File>` created lazily with `File` moved in.)

- [ ] **Step 5: `stop()`, `poll_auto_stop()`, finalize**

```rust
pub(crate) fn stop(request_id: [u8; 16], dir: Option<String>, name: Option<String>) {
    let Some(state) = active().lock().unwrap().clone() else {
        return; // nothing active; drain_recordings has no last_auto → handled there
    };
    {
        let mut out = state.out.lock().unwrap();
        if out.finalizing {
            return;
        }
        out.dir = dir;
        out.name = name;
        out.request_id = Some(request_id);
        out.finalizing = true;
    }
    finalize(state);
}

pub(crate) fn poll_auto_stop() {
    let Some(state) = active().lock().unwrap().clone() else { return };
    if Instant::now() < state.deadline {
        return;
    }
    {
        let mut out = state.out.lock().unwrap();
        if out.finalizing {
            return;
        }
        out.request_id = None; // auto-stop → default dir/name
        out.finalizing = true;
    }
    finalize(state);
}

fn finalize(state: Arc<RecordingState>) {
    // Stop the stream first so no more samples arrive.
    if let Some(stream) = state.stream.lock().unwrap().take() {
        let s = state.clone();
        let completion = RcBlock::new(move |_err: *mut objc2_foundation::NSError| {
            finish_writer(s.clone());
        });
        unsafe { stream.0.stopCaptureWithCompletionHandler(Some(&*completion)) };
    } else {
        finish_writer(state);
    }
}

fn finish_writer(state: Arc<RecordingState>) {
    // Close gif input and mark mp4 input finished.
    {
        let mut enc = state.encode.lock().unwrap();
        enc.gif_tx = None; // closes channel → gif worker flushes
        unsafe { enc.input.0.markAsFinished() };
    }
    if let Some(join) = state.gif_join.lock().unwrap().take() {
        let _ = join.join();
    }
    let s = state.clone();
    let writer = state.encode.lock().unwrap().writer.0.clone();
    let completion = RcBlock::new(move || {
        deliver(s.clone());
    });
    unsafe { writer.finishWritingWithCompletionHandler(Some(&*completion)) };
}

fn deliver(state: Arc<RecordingState>) {
    let target = state.out.lock().unwrap().clone();
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let (final_mp4, final_gif) = resolve_output_paths(
        target.dir.as_deref(),
        target.name.as_deref(),
        state.gif,
        &ts,
    );

    let result = (|| -> Result<RecordingInfo, String> {
        if let Some(parent) = final_mp4.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::rename(&state.temp_mp4, &final_mp4)
            .map_err(|e| format!("cannot move mp4: {e}"))?;
        let gif_path = match (&state.temp_gif, &final_gif) {
            (Some(tmp), Some(dst)) => {
                std::fs::rename(tmp, dst).map_err(|e| format!("cannot move gif: {e}"))?;
                Some(dst.to_string_lossy().into_owned())
            }
            _ => None,
        };
        let bytes = std::fs::metadata(&final_mp4).map(|m| m.len()).unwrap_or(0);
        let duration_ms = state.encode.lock().unwrap().start.elapsed().as_millis() as u64;
        Ok(RecordingInfo {
            mp4_path: final_mp4.to_string_lossy().into_owned(),
            gif_path,
            duration_ms,
            bytes,
            auto_stopped: target.request_id.is_none(),
        })
    })();

    *active().lock().unwrap() = None;
    let _ = state.tx.send(RecordOutcome {
        request_id: target.request_id,
        result,
    });
    if let Some(w) = &state.wake {
        w();
    }
}
```

Add the `os_at_least_14` and `window_number` helpers — copy them verbatim from `screenshot.rs` (lines 264-279 and 349-357); they are identical here. (Optionally refactor both into a shared `mod macos_capture_util` later; for now duplicate to keep tasks independent.)

- [ ] **Step 6: Build, fix, and manually verify**

Run: `cargo build -p vmux_desktop`
Iterate on selector/type mismatches until it compiles (expect several rounds — check docs.rs for `objc2-av-foundation` / `objc2-screen-capture-kit` exact names; `startSessionAtSourceTime`, `appendPixelBuffer_withPresentationTime`, `finishWritingWithCompletionHandler`, `addStreamOutput_type_sampleHandlerQueue_error` are the likely real selector spellings).

Manual verification (the real test):
1. Run vmux: `bash -c "cargo run -p vmux_desktop"` (or the project's run skill). Grant Screen Recording permission if prompted, then retry.
2. From an agent terminal (or `vmux mcp` stdio), call `vmux_record_start {"gif": true, "max_secs": 20}`.
3. Interact with the window (open a page, type in a terminal).
4. Call `vmux_record_stop {"name": "smoke"}`.
5. Confirm `~/.vmux/screenshots/smoke.mp4` plays in QuickTime and `smoke.gif` opens. Check duration/size in the returned text.
6. Verify auto-stop: `vmux_record_start {"max_secs": 5}`, wait 6s, then `vmux_record_stop` → returns the auto-stopped path with `(auto-stopped)`.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/src/recording_capture_macos.rs crates/vmux_desktop/src/recording.rs
git commit -m "feat(desktop): native SCStream + AVAssetWriter recording"
```

---

## Task 11: docs/features convention seed

**Files:**
- Create: `docs/features/README.md`

- [ ] **Step 1: Write the convention doc**

Create `docs/features/README.md`:

```markdown
# Feature demos

Short screen recordings showcasing vmux features. Captured with the
`vmux_record_start` / `vmux_record_stop` MCP tools (see
`docs/specs/2026-06-23-video-recording-mcp-tool-design.md`).

## Recording a demo

From an agent with vmux MCP tools:

1. `vmux_record_start { "gif": true, "max_secs": 60 }`
2. Drive the feature (open pages, run commands, switch tabs).
3. `vmux_record_stop { "dir": "<repo>/docs/features", "name": "<feature>" }`

This writes `<feature>.mp4` (+ `<feature>.gif`) here. Drag the `.mp4` into a PR
description to embed an inline player, or reference the `.gif` from markdown.

## Keep clips small

Committed video bloats git history. Keep demos short (a few seconds), prefer the
mp4, and only commit a GIF when inline autoplay is needed. Large/long clips
should live in the PR upload (GitHub CDN), not the repo.
```

- [ ] **Step 2: Commit**

```bash
git add docs/features/README.md
git commit -m "docs(features): seed demo recording convention"
```

---

## Task 12: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Format**

Run: `cargo fmt --all`
Then: `git diff --stat` — if rustfmt reordered cfg-gated imports or anything else, review and commit:

```bash
git add -A && git commit -m "style: cargo fmt"
```

- [ ] **Step 2: Clippy (touched crates)**

Run: `cargo clippy -p vmux_service -p vmux_agent -p vmux_mcp -p vmux_desktop --all-targets -- -D warnings`
Expected: no warnings. Fix any (common: `clippy::too_many_arguments` on `capture::start` — already `#[allow]`’d; unused imports on non-macOS).

- [ ] **Step 3: Tests (touched crates)**

Run: `cargo test -p vmux_service -p vmux_agent -p vmux_mcp -p vmux_desktop`
Expected: all green, including the existing `no_continuous_update_mode` test in `vmux_desktop` (recording must not introduce `UpdateMode::Continuous`).

- [ ] **Step 4: Manual smoke (macOS)**

Re-run the Task 10 Step 6 manual checklist if not already done in this session: start → interact → stop produces a playable mp4 (+ gif), auto-stop works, and a second concurrent `record_start` is rejected with "already in progress".

- [ ] **Step 5: Delete the plan**

Per AGENTS.md, remove the plan once fully implemented:

```bash
git rm docs/plans/2026-06-23-video-recording.md
git commit -m "chore: remove implemented video-recording plan"
```

---

## Self-Review

**Spec coverage:**
- `vmux_record_start` / `vmux_record_stop` tools, params, defaults → Tasks 5, 1.
- Start/stop control + async start response → Tasks 1, 4, 8, 10.
- Auto-stop at `max_secs` + `last_auto_stopped` fallback → Tasks 8 (`drain_recordings`), 10 (`poll_auto_stop`, `finish`/`deliver`).
- One recording at a time → Task 10 (`active()` guard in `start`).
- H.264 mp4 via AVAssetWriter → Task 10 (`build_writer`).
- Optional pure-Rust GIF tee (`gif`+`color_quant`, ~12fps, downscaled) → Tasks 7, 8 (`should_sample_gif_frame`, `downscale_to`, `bgra_to_rgba`), 10 (`gif_worker`, `handle_sample`).
- Default dir `~/.vmux/screenshots`, `dir`/`name` override → Task 8 (`resolve_output_paths`), 10 (`deliver`).
- Path-only result, no inline bytes → Tasks 1, 6 (text block), 2 (service text).
- Longer stop timeout (30s) → Tasks 1, 2.
- Reuse permission/window/crop/wake plumbing → Tasks 8, 10 (copied `os_at_least_14`/`window_number`, shared crop math).
- No `UpdateMode::Continuous` (off-thread encode, WakeUp on finalize) → Task 10 (`deliver` calls `wake`), enforced by Task 12 Step 3.
- macOS-only, non-macOS stub error → Task 8 (`#[cfg(not(target_os = "macos"))] mod capture`).
- `docs/features/` convention seed → Task 11.

**Placeholder scan:** Task 10 is intentionally a manual-verify task (native objc2 glue) with a code skeleton + explicit "iterate against the compiler / docs.rs" instruction and a concrete manual checklist — consistent with how `screenshot.rs::capture` is treated. All other tasks contain complete, runnable code + TDD steps.

**Type consistency:** `request_id: [u8; 16]` everywhere (matches `ScreenshotRequest`); `RecordingInfo` fields match `AgentQueryResult::Recording` (`mp4_path`, `gif_path`, `duration_ms`, `bytes`, `auto_stopped`) across Tasks 1/3/4/6/8/10; `RecordOutcome.request_id: Option<[u8;16]>` (None = auto-stop) consumed in `drain_recordings`; `capture::start` signature identical in non-macOS stub (Task 8), macOS placeholder (Task 8), and macOS impl (Task 10).

**Known iteration points (flagged, not placeholders):** exact `objc2-*` feature flags (Task 7) and selector spellings (Task 10) depend on installed crate versions; the `gif::Encoder` writer-ownership lifetime may need `File`-owned instead of `&mut BufWriter` (noted in Task 10 Step 4).
