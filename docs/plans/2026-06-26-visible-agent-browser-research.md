# Visible, Viewport-Aware Agent Web Research — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Execution note (this repo):** CEF builds are large; implement directly with a warm target dir (do NOT subagent-drive). Run targeted unit tests during the loop. Defer the single manual/runtime test pass to the very end. A change to `patches/` (Task C2) also requires building the patched package; `cargo fmt` reformats `patches/` — after fmt, `git checkout -- patches/` for any unintended reformat and commit only intended `patches/` edits.

**Goal:** Make the Vibe agent do all web research in the user's visible, logged-in browser pane — disabling its invisible `web_search`/`web_fetch`, returning a page snapshot inline from every navigation, and making snapshots viewport-aware with a scroll tool — so the user watches the agent work.

**Architecture:** Three workstreams. (A) `vmux_agent` injects `VIBE_DISABLED_TOOLS` at Vibe launch + browser-first guidance lives in MCP tool descriptions. (B) Navigation MCP tools wait for load-settle and return the snapshot inline via the existing snapshot machinery. (C) The render-process DOM walker emits viewport geometry; `shape_snapshot` computes per-element `in_viewport`; a new `browser_scroll` tool drives the visible pane and returns a fresh snapshot.

**Tech Stack:** Rust, Bevy ECS (messages + systems), bevy_cef (patched CEF crate in `patches/`), serde/serde_json, toml.

---

## Implementation order & dependencies

1. **A** (vibe env) — independent.
2. **C1** (snapshot types + `in_viewport` math) — foundation; B and C reuse it.
3. **C2** (render-process viewport capture) — fills the data C1 consumes.
4. **C3** (`browser_scroll` tool).
5. **B** (navigation returns snapshot) — reuses C1/C2 snapshot output.
6. **Descriptions + final verification.**

---

## File structure

- `crates/vmux_agent/Cargo.toml` — add `toml` dep.
- `crates/vmux_agent/src/client/cli/vibe.rs` — `VIBE_DISABLED_TOOLS` env + config-read union (Task A).
- `crates/vmux_core/src/dom_snapshot.rs` — viewport types + `in_viewport` (Task C1).
- `patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs` — emit viewport metadata (Task C2).
- `crates/vmux_mcp/src/tools.rs` — `browser_scroll` def + dispatch; nav tool description updates (Tasks C3, B, Descriptions).
- `crates/vmux_service/src/protocol.rs` — `AgentQuery::BrowserScroll`, `AgentCommandResult::Text` (Tasks C3, B).
- `crates/vmux_agent/src/events.rs` — `BrowserScrollRequest` (Task C3); pending-nav plumbing (Task B).
- `crates/vmux_agent/src/plugin.rs` — dispatch scroll query; pending-nav tracker + settle/timeout systems (Tasks C3, B).
- `crates/vmux_desktop/src/browser_snapshot.rs` (or a sibling `browser_scroll.rs`) — scroll system: execute_js + snapshot (Task C3).
- `crates/vmux_browser/src/lib.rs` — surface load-settle transitions to the pending-nav tracker (Task B).

---

## Task A: Disable Vibe's web tools at launch (additively)

**Why additive:** `VIBE_*` env overrides replace the whole `disabled_tools` field (pydantic-settings, env outranks TOML). The user's `~/.vibe/config.toml` may already disable tools (e.g. `["bash"]`). We must union, not clobber, so we don't re-enable the user's existing disables.

**Files:**
- Modify: `crates/vmux_agent/Cargo.toml`
- Modify: `crates/vmux_agent/src/client/cli/vibe.rs:51-54` (`build_env`) + add helpers + tests

- [ ] **Step 1: Add `toml` dependency**

In `crates/vmux_agent/Cargo.toml`, under `[dependencies]` (next to `serde`/`serde_json`):

```toml
toml = { workspace = true }
```

If `toml` is not in the workspace `[workspace.dependencies]`, add `toml = "0.8"` there first (it is already in `Cargo.lock`). Run `cargo metadata` or a build to confirm resolution.

- [ ] **Step 2: Write failing tests for the union helper**

Add to the `tests` module in `crates/vmux_agent/src/client/cli/vibe.rs`:

```rust
#[test]
fn disabled_tools_unions_web_tools_with_existing() {
    let existing = vec!["bash".to_string()];
    let out = vibe_disabled_tools(existing);
    assert!(out.contains(&"bash".to_string()));
    assert!(out.contains(&"web_search".to_string()));
    assert!(out.contains(&"web_fetch".to_string()));
}

#[test]
fn disabled_tools_dedups_when_web_tool_already_present() {
    let existing = vec!["web_search".to_string()];
    let out = vibe_disabled_tools(existing);
    assert_eq!(out.iter().filter(|t| *t == "web_search").count(), 1);
}

#[test]
fn parse_disabled_from_toml_reads_array() {
    let toml = "disabled_tools = [\"bash\", \"foo\"]\nother = 1\n";
    let out = parse_disabled_tools_toml(toml);
    assert_eq!(out, vec!["bash".to_string(), "foo".to_string()]);
}

#[test]
fn parse_disabled_from_toml_defaults_empty_when_absent_or_bad() {
    assert!(parse_disabled_tools_toml("x = 1").is_empty());
    assert!(parse_disabled_tools_toml("not = [valid").is_empty());
}

#[test]
fn build_env_sets_disabled_tools_json_array() {
    let mcp = McpServerConfig { command: "vmux".to_string(), args: vec![], cwd: None };
    let env = VibeStrategy.build_env(&mcp);
    let val = env.iter().find(|(k, _)| k == "VIBE_DISABLED_TOOLS").map(|(_, v)| v.clone());
    let val = val.expect("VIBE_DISABLED_TOOLS present");
    let parsed: Vec<String> = serde_json::from_str(&val).unwrap();
    assert!(parsed.contains(&"web_search".to_string()));
    assert!(parsed.contains(&"web_fetch".to_string()));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vmux_agent vibe -- disabled_tools build_env_sets`
Expected: FAIL (`vibe_disabled_tools`, `parse_disabled_tools_toml` not defined; `VIBE_DISABLED_TOOLS` absent).

- [ ] **Step 4: Implement the helpers + wire `build_env`**

In `crates/vmux_agent/src/client/cli/vibe.rs`, add near `serialize_vibe_mcp_env`:

```rust
const VIBE_WEB_TOOLS: [&str; 2] = ["web_search", "web_fetch"];

fn vibe_config_path() -> PathBuf {
    std::env::var("VIBE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".vibe"))
        .join("config.toml")
}

fn parse_disabled_tools_toml(text: &str) -> Vec<String> {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("disabled_tools").cloned())
        .and_then(|v| v.try_into::<Vec<String>>().ok())
        .unwrap_or_default()
}

fn read_user_disabled_tools() -> Vec<String> {
    std::fs::read_to_string(vibe_config_path())
        .map(|t| parse_disabled_tools_toml(&t))
        .unwrap_or_default()
}

fn vibe_disabled_tools(existing: Vec<String>) -> Vec<String> {
    let mut out = existing;
    for tool in VIBE_WEB_TOOLS {
        if !out.iter().any(|t| t == tool) {
            out.push(tool.to_string());
        }
    }
    out
}
```

Replace `build_env` (lines 51-54):

```rust
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)> {
        let mcp_json = serialize_vibe_mcp_env(mcp);
        let disabled = vibe_disabled_tools(read_user_disabled_tools());
        let disabled_json = serde_json::to_string(&disabled).unwrap_or_else(|_| "[]".to_string());
        vec![
            ("VIBE_MCP_SERVERS".to_string(), mcp_json),
            ("VIBE_DISABLED_TOOLS".to_string(), disabled_json),
        ]
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vmux_agent vibe`
Expected: PASS (all, including pre-existing vibe tests).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/client/cli/vibe.rs Cargo.toml Cargo.lock
git commit -m "feat(agent): disable vibe web_search/web_fetch at launch (additive)"
```

---

## Task C1: Viewport-aware snapshot types + `in_viewport`

**Files:**
- Modify: `crates/vmux_core/src/dom_snapshot.rs`

- [ ] **Step 1: Write failing tests**

Add to the `tests` module in `crates/vmux_core/src/dom_snapshot.rs`. Update the existing `raw(...)` test helper to set a default viewport, and add new cases:

```rust
fn raw_vp(nodes: Vec<RawDomNode>, viewport: Option<RawViewport>) -> RawSnapshot {
    RawSnapshot {
        url: "https://example.com".to_string(),
        title: "Example".to_string(),
        nodes,
        viewport,
    }
}

#[test]
fn viewport_passes_through_and_marks_in_viewport_by_bbox() {
    let vp = RawViewport { scroll_x: 0, scroll_y: 0, width: 800, height: 600, page_width: 800, page_height: 4000 };
    // on-screen button (y=10) and off-screen button (y=2000)
    let on = node("button", "On", &[], [0, 10, 100, 30]);
    let off = node("button", "Off", &[], [0, 2000, 100, 30]);
    let snap = shape_snapshot(raw_vp(vec![on, off], Some(vp)));
    let on_n = snap.nodes.iter().find(|n| n.name == "On").unwrap();
    let off_n = snap.nodes.iter().find(|n| n.name == "Off").unwrap();
    assert!(on_n.in_viewport);
    assert!(!off_n.in_viewport);
    let v = snap.viewport.unwrap();
    assert_eq!(v.height, 600);
    assert_eq!(v.page_height, 4000);
}

#[test]
fn no_viewport_means_nodes_default_in_viewport_false_and_field_absent() {
    let snap = shape_snapshot(raw_vp(vec![node("button", "X", &[], [0, 0, 10, 10])], None));
    assert!(snap.viewport.is_none());
    assert!(!snap.nodes[0].in_viewport);
}
```

Note: the existing tests call `raw(...)`. Keep the old `raw(...)` helper but have it delegate: `fn raw(nodes) { raw_vp(nodes, None) }`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_core dom_snapshot`
Expected: FAIL (`RawViewport`, `viewport` field, `in_viewport` not defined).

- [ ] **Step 3: Implement types + shaping**

In `crates/vmux_core/src/dom_snapshot.rs`:

Add the raw viewport (deserialized from the render process) and update `RawSnapshot`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawViewport {
    #[serde(rename = "scrollX")]
    pub scroll_x: i32,
    #[serde(rename = "scrollY")]
    pub scroll_y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(rename = "pageWidth")]
    pub page_width: i32,
    #[serde(rename = "pageHeight")]
    pub page_height: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSnapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<RawDomNode>,
    #[serde(default)]
    pub viewport: Option<RawViewport>,
}
```

Add the output viewport and extend `SnapNode` / `Snapshot`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Viewport {
    #[serde(rename = "scrollX")]
    pub scroll_x: i32,
    #[serde(rename = "scrollY")]
    pub scroll_y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(rename = "pageWidth")]
    pub page_width: i32,
    #[serde(rename = "pageHeight")]
    pub page_height: i32,
}
```

In `SnapNode`, add after `bbox`:

```rust
    #[serde(rename = "inViewport", skip_serializing_if = "is_false")]
    pub in_viewport: bool,
```

In `Snapshot`, add after `title`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
```

Update `shape_snapshot` to compute `in_viewport` and carry the viewport. `bbox` is `[x, y, w, h]`; viewport-membership tests intersection of the element rect with the viewport rect. **The coordinate system of `bbox` is settled in Task C2** — this code assumes the render process reports element bounds **viewport-relative** (origin = top-left of the visible viewport), matching the helper below; if C2 finds page-relative bounds, change `in_viewport_of` to subtract `scroll_x/scroll_y`.

```rust
fn in_viewport_of(bbox: [i32; 4], vp: &RawViewport) -> bool {
    let (x, y, w, h) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let intersects_x = x < vp.width && (x + w) > 0;
    let intersects_y = y < vp.height && (y + h) > 0;
    intersects_x && intersects_y
}
```

In the node-push loop, set `in_viewport`:

```rust
            in_viewport: raw
                .viewport
                .as_ref()
                .map(|vp| in_viewport_of(raw_node.bounds, vp))
                .unwrap_or(false),
```

And build the output `viewport` in the returned `Snapshot`:

```rust
        viewport: raw.viewport.map(|v| Viewport {
            scroll_x: v.scroll_x,
            scroll_y: v.scroll_y,
            width: v.width,
            height: v.height,
            page_width: v.page_width,
            page_height: v.page_height,
        }),
```

Ensure `is_false` is in scope (it already exists for `truncated`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_core dom_snapshot`
Expected: PASS (new + all existing snapshot tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/dom_snapshot.rs
git commit -m "feat(core): viewport geometry + in_viewport flags in dom snapshot"
```

---

## Task C2: Render-process emits viewport metadata

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs`

This is a patched CEF crate. The DOM visitor (`build_json`) already walks elements and reports `element_bounds()`. Add a `viewport` object to the emitted JSON so `RawSnapshot.viewport` (Task C1) is populated.

- [ ] **Step 1: Determine `element_bounds()` coordinate system (5-min runtime check)**

`Domnode::element_bounds()` may return page-relative or viewport-relative bounds depending on CEF. Add a temporary unconditional `eprintln!` in `node_json` printing the first node's `bounds` and the document scroll/size, build, open a long page, scroll down, snapshot, and observe whether on-screen elements have small `y` (viewport-relative) or `y ≈ scrollY+offset` (page-relative). Record the result, then remove the diagnostic. This decides the `in_viewport_of` math in C1 (subtract scroll if page-relative).

- [ ] **Step 2: Capture viewport geometry from the document**

In `build_json`, after computing `title`, obtain the visual viewport + scroll + scroll-size. The CEF `Domdocument` does not expose scroll/innerSize directly, so evaluate it via the frame's V8 context. Add a helper that runs a tiny script through the frame and returns the six integers. Concretely, in `request_dom_snapshot`/`SnapshotVisitor` the `frame` is available; use the frame to read:

```rust
// pseudostructure — implement against the cef V8 API available in this crate:
// returns RawViewport-shaped JSON object {scrollX,scrollY,width,height,pageWidth,pageHeight}
fn viewport_json(frame: &Frame) -> serde_json::Value {
    // Evaluate in the frame's V8 context:
    //   const d=document.documentElement, b=document.body;
    //   [window.scrollX|0, window.scrollY|0, window.innerWidth|0, window.innerHeight|0,
    //    Math.max(d.scrollWidth,b?b.scrollWidth:0)|0, Math.max(d.scrollHeight,b?b.scrollHeight:0)|0]
    // Map the result array to:
    serde_json::json!({
        "scrollX": sx, "scrollY": sy, "width": iw, "height": ih,
        "pageWidth": pw, "pageHeight": ph
    })
}
```

Implementation guidance: the visitor runs in the render process where a V8 context is current during `visit`. Use the same crate's V8 access pattern (search this crate for existing `get_v8context` / `eval` / `execute` usage; if none exists, prefer `frame.visit_dom` is DOM-only — use the frame's `ExecuteJavaScript` is fire-and-forget and cannot return a value, so instead read what the DOM API offers: `document.element_from_point`/scroll are unavailable; fall back to obtaining innerWidth/Height from `document.document_element().element_bounds()` height/width and scroll via a V8 context visitor). If no value-returning V8 path is ergonomic in this crate version, emit only what is available (`pageWidth`/`pageHeight` from the documentElement bounds; `width`/`height` from the body/clientbounds; `scrollX/Y` default 0) and let vmux fill `width/height` from the known pane size in Task C2b.

Add it to the JSON in `build_json`:

```rust
    let value = serde_json::json!({
        "url": url,
        "title": title,
        "nodes": nodes,
        "viewport": viewport_json(frame),   // frame threaded into build_json
    });
```

Thread `frame: &Frame` into `build_json` (the visitor holds `self.frame`).

- [ ] **Step 2b (fallback, only if V8 width/height unavailable):** In `crates/vmux_desktop/src/browser_snapshot.rs::shape_snapshot_results`, if `raw.viewport` is `Some` but `width`/`height` are 0, fill them from the target webview's on-screen size (the pane rect vmux already tracks) before calling `shape_snapshot`. Skip this step if Step 2 yields real `innerWidth/Height`.

- [ ] **Step 3: Keep the const JSON in sync**

`EMPTY_SNAPSHOT` (line 31) has no `viewport` key — fine, `RawSnapshot.viewport` is `#[serde(default)]`. Leave as-is.

- [ ] **Step 4: Build the patched package + a manual snapshot check**

Run: `cargo build -p bevy_cef_core` then a full app build. Manual check deferred to the final verification pass.

- [ ] **Step 5: fmt guard + commit**

```bash
cargo fmt
git checkout -- patches/   # discard any unintended reformat
git add patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs
# (also crates/vmux_desktop/src/browser_snapshot.rs if Step 2b used)
git commit -m "feat(cef): emit viewport geometry in dom snapshot"
```

---

## Task C3: `browser_scroll` tool

Models scroll as a Query that returns the post-scroll snapshot (consistent with `browser_snapshot`). v1 supports `to: top|bottom` and `delta: <px>` via `window.scrollTo/scrollBy`. (Ref-based scroll is a follow-up; the agent can derive a `delta` from the `bbox`/`scrollY` it already has.)

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (`AgentQuery`)
- Modify: `crates/vmux_agent/src/events.rs` (request message)
- Modify: `crates/vmux_mcp/src/tools.rs` (definition + dispatch)
- Modify: `crates/vmux_agent/src/plugin.rs` (dispatch query → request)
- Create: `crates/vmux_desktop/src/browser_scroll.rs` (system) + register in the desktop plugin

- [ ] **Step 1: Add the query variant (protocol)**

In `crates/vmux_service/src/protocol.rs`, in `enum AgentQuery` (near `BrowserSnapshot`, line 188):

```rust
    BrowserScroll {
        pane: Option<String>,
        to: Option<String>,
        delta: Option<i32>,
    },
```

- [ ] **Step 2: Write failing MCP dispatch tests**

In `crates/vmux_mcp/src/tools.rs` tests module (near `browser_snapshot_dispatches_to_query_with_pane`):

```rust
#[test]
fn browser_scroll_dispatches_with_delta() {
    let t = dispatch_query("browser_scroll", serde_json::json!({ "delta": 600 })).unwrap();
    assert_eq!(t, AgentQuery::BrowserScroll { pane: None, to: None, delta: Some(600) });
}

#[test]
fn browser_scroll_dispatches_to_bottom_with_pane() {
    let t = dispatch_query("browser_scroll", serde_json::json!({ "to": "bottom", "target": "pane:3" })).unwrap();
    assert_eq!(t, AgentQuery::BrowserScroll { pane: Some("pane:3".into()), to: Some("bottom".into()), delta: None });
}

#[test]
fn browser_scroll_is_listed() {
    assert!(tool_names().contains(&"browser_scroll".to_string()));
}
```

(Use the same `dispatch_query` test helper the snapshot tests use.)

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vmux_mcp browser_scroll`
Expected: FAIL (tool not defined).

- [ ] **Step 4: Implement definition + dispatch**

In `crates/vmux_mcp/src/tools.rs`, add a definition function and push it in `tool_definitions()` (after `browser_snapshot_definition()`):

```rust
fn browser_scroll_definition() -> ToolDefinition {
    ToolDefinition {
        name: "browser_scroll".into(),
        description:
            "Scroll the visible browser page so the user can watch, then return the post-scroll \
snapshot (same shape as browser_snapshot, including viewport + inViewport flags). Pass exactly one \
of `to` (\"top\" or \"bottom\") or `delta` (pixels; positive = down, e.g. one screen ≈ the \
snapshot's viewport.height). Pass `target` = pane:<id>/stack:<id> to pick a page; defaults to the \
focused page. Prefer scrolling to read long pages instead of assuming off-screen content."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "to": {"enum": ["top", "bottom"], "description": "Scroll to page top or bottom."},
                "delta": {"type": "integer", "description": "Scroll by pixels; positive = down."},
                "target": {"type": "string", "description": "Optional pane:<id> or stack:<id>; focused page if omitted."}
            }
        }),
    }
}
```

Add `defs.push(browser_scroll_definition());` in `tool_definitions()`.

In `dispatch_with_anchor`, add a branch (near the `browser_snapshot` branch, line 689):

```rust
    if name == "browser_scroll" {
        let pane = match arguments.get("target") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => { let s = s.trim(); (!s.is_empty()).then(|| s.to_string()) }
            Some(_) => return Err("browser_scroll.target must be a string".to_string()),
        };
        let to = match arguments.get("to").and_then(Value::as_str) {
            None => None,
            Some("top") | Some("bottom") => arguments.get("to").and_then(Value::as_str).map(str::to_string),
            Some(other) => return Err(format!("browser_scroll.to must be 'top' or 'bottom', got {other}")),
        };
        let delta = arguments.get("delta").and_then(Value::as_i64).map(|d| d as i32);
        if to.is_some() == delta.is_some() {
            return Err("browser_scroll requires exactly one of `to` or `delta`".to_string());
        }
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::BrowserScroll { pane, to, delta },
        ));
    }
```

- [ ] **Step 5: Run MCP tests to verify they pass**

Run: `cargo test -p vmux_mcp browser_scroll`
Expected: PASS.

- [ ] **Step 6: Wire the query through the agent plugin + desktop system**

In `crates/vmux_agent/src/events.rs`, add:

```rust
#[derive(Message, Clone)]
pub struct BrowserScrollRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
    pub to: Option<String>,
    pub delta: Option<i32>,
}
```

(Reuse `BrowserSnapshotResponse` for the reply — scroll returns a snapshot.)

In `crates/vmux_agent/src/plugin.rs`, where `AgentQuery::BrowserSnapshot` is handled (line 1237) add a sibling arm that writes a `BrowserScrollRequest` with the query's fields and the request id; register `BrowserScrollRequest` in the plugin (`.add_message::<BrowserScrollRequest>()` near line 113) and ensure its responses route via the existing `forward_snapshot_responses` (it already maps `BrowserSnapshotResponse`).

Create `crates/vmux_desktop/src/browser_scroll.rs`:

```rust
use bevy::prelude::*;
use bevy_cef::prelude::Browsers;
use vmux_agent::{BrowserScrollRequest, BrowserSnapshotRequest};
use vmux_core::LastActivatedAt;
use vmux_core::terminal::{ProcessExited, Terminal};
use vmux_layout::Browser;
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::{FocusedStack, Stack, active_stack_in_pane};
use vmux_layout::target::{active_webview_for_tab, parse_pane_target};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_scrolls(
    mut reader: MessageReader<BrowserScrollRequest>,
    cef_browsers: NonSend<Browsers>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut snap_writer: MessageWriter<BrowserSnapshotRequest>,
) {
    for req in reader.read() {
        let target = match req.pane.as_deref() {
            Some(s) => parse_pane_target(s, &panes),
            None => focus.pane.filter(|p| panes.contains(*p)),
        };
        let webview = target.and_then(|pane| {
            active_webview_for_tab(
                active_stack_in_pane(pane, &pane_children, &stack_ts),
                &browsers,
                &terminals,
            )
        });
        if let Some(webview) = webview {
            let js = match (req.to.as_deref(), req.delta) {
                (Some("top"), _) => "window.scrollTo(0,0)".to_string(),
                (Some("bottom"), _) => "window.scrollTo(0,document.documentElement.scrollHeight)".to_string(),
                (_, Some(d)) => format!("window.scrollBy(0,{d})"),
                _ => "void 0".to_string(),
            };
            cef_browsers.execute_js(&webview, &js);
        }
        // Re-snapshot the same pane and reuse the same request id so the
        // MCP query resolves with the post-scroll snapshot.
        snap_writer.write(BrowserSnapshotRequest { request_id: req.request_id, pane: req.pane.clone() });
    }
}
```

Register the module (`mod browser_scroll;` — filename pattern, no mod.rs) and add `run_scrolls` to the desktop plugin's systems alongside `start_snapshots`. Order it before `start_snapshots` in the same schedule so the emitted `BrowserSnapshotRequest` is consumed the same frame (or rely on next-frame draining — acceptable). The synchronous `execute_js` + immediate re-snapshot is best-effort; for JS layout settling, the snapshot reads the DOM after the scroll IPC. If empirically the snapshot races the scroll, add a one-frame delay component keyed by request id (mirror the pending-nav tracker in Task B).

- [ ] **Step 7: Build + commit**

Run: `cargo build -p vmux_mcp -p vmux_agent -p vmux_service` (desktop builds in the final pass).

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_agent/src/events.rs crates/vmux_agent/src/plugin.rs crates/vmux_mcp/src/tools.rs crates/vmux_desktop/src/browser_scroll.rs crates/vmux_desktop/src/<plugin-file>.rs
git commit -m "feat(mcp): browser_scroll tool returning post-scroll snapshot"
```

---

## Task B: Navigation tools return the snapshot inline

**Behavior:** `browser_navigate`, `browser_go_back`, `browser_go_forward`, `browser_reload`, `browser_hard_reload`, and the `open_*` tools (when the URL is a web page) wait for the target webview to settle (`is_loading → false`, ~10s cap) and return the page snapshot as the command result. Terminal/`vmux://` opens keep the plain ack. On timeout, return the partial snapshot with `timedOut: true`.

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (`AgentCommandResult::Text`)
- Modify: `crates/vmux_agent/src/plugin.rs` (pending-nav tracker + settle/timeout systems)
- Modify: `crates/vmux_browser/src/lib.rs` (surface settle transitions for tracked navs)
- Modify: `crates/vmux_mcp/src/protocol.rs` (serialize the new command result)

- [ ] **Step 1: Add a text-carrying command result**

In `crates/vmux_service/src/protocol.rs`, `enum AgentCommandResult` (line 159, currently `Ok`, `Layout(...)`):

```rust
    Text(String),
```

In `crates/vmux_mcp/src/protocol.rs` (around line 333, where `AgentCommandResult::Layout` is serialized to text), add:

```rust
        AgentCommandResult::Text(text) => { /* return text verbatim as the tool result */ }
```

(Mirror the existing `Layout` arm's text-return shape.)

- [ ] **Step 2: Add the pending-nav tracker + correlation (write the design as a unit test first)**

Add a Bevy integration test in `crates/vmux_agent` that exercises the message flow (per repo convention: register messages + systems, send a typed request, run schedule, assert resulting messages). Test outline:

```rust
#[test]
fn agent_nav_waits_for_settle_then_emits_snapshot_request() {
    // 1. App with the agent plugin's nav-tracking systems + messages registered.
    // 2. Send the agent BrowserNavigate command (origin = agent, with a request_id).
    // 3. Assert NO AgentCommandResult yet and a PendingNav recorded.
    // 4. Simulate the browser settle signal (is_loading -> false) for the pane.
    // 5. Run update; assert a BrowserSnapshotRequest was emitted with the SAME request_id.
    // 6. Feed a BrowserSnapshotResponse(Ok(json)) for that id.
    // 7. Run update; assert an AgentCommandResult::Text(json) is produced for the command.
}

#[test]
fn agent_nav_times_out_and_still_snapshots() {
    // Same setup; advance a virtual timer past the cap WITHOUT a settle signal;
    // assert a BrowserSnapshotRequest is emitted (timeout path).
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p vmux_agent agent_nav`
Expected: FAIL (tracker/systems not present).

- [ ] **Step 4: Implement the tracker + systems**

In `crates/vmux_agent/src/plugin.rs`:

```rust
#[derive(Resource, Default)]
pub struct PendingNavSnapshots {
    pub by_request: std::collections::HashMap<[u8; 16], PendingNav>,
}

pub struct PendingNav {
    pub pane: Option<String>,
    pub deadline: std::time::Duration, // elapsed-time deadline
}
```

- Where the agent `BrowserNavigate` command is currently handled (line 671) — and the analogous `BrowserGoBack`/`BrowserGoForward`/web `OpenBeside` arms — when the command origin is an agent (a `request_id` exists) and the target is a web page: record a `PendingNav` (deadline = now + 10s) instead of replying `AgentCommandResult::Ok`. For terminal/`vmux://` targets, keep the immediate `Ok`.
- Add a `nav_settle_watch` system: read the browser load-settle signal (Step 5) and, for any `PendingNav` whose pane matches a webview that transitioned to `is_loading == false`, emit `BrowserSnapshotRequest { request_id, pane }`, then move the entry to an "awaiting snapshot" set.
- Add a `nav_timeout_watch` system: for `PendingNav`s past their deadline with no settle, emit the snapshot request anyway; mark the resulting snapshot as timed out (set `timedOut` — simplest: have the awaiting-set carry a `timed_out: bool` and, when the `BrowserSnapshotResponse` arrives, inject `"timedOut":true` into the JSON object before returning, or extend `shape`/response to carry it; choose the JSON-injection approach to avoid threading through the snapshot crate).
- Extend `forward_snapshot_responses` (line 1328): if a `BrowserSnapshotResponse`'s `request_id` belongs to an awaiting-nav entry, resolve it as the **command** result `AgentCommandResult::Text(json)` (not a query result). Otherwise keep the existing query-result behavior. Use the `request_id` to disambiguate command-vs-query origin (the nav command must reuse a fresh `request_id` recorded in `PendingNavSnapshots`).

Register `PendingNavSnapshots` and the two systems in the plugin (chain App-builder calls in one expression per repo convention).

- [ ] **Step 5: Surface load-settle transitions**

In `crates/vmux_browser/src/lib.rs`, `drain_loading_state` (line 2013) already observes per-webview `is_loading`. Emit a typed message on the **false** transition (e.g. `WebviewSettled { pane/webview }`) that the `nav_settle_watch` system reads. Map the webview back to a pane id consistent with how snapshot targeting resolves panes (`parse_pane_target`/`active_webview_for_tab`). Register the message in both the browser plugin and the agent plugin.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vmux_agent agent_nav && cargo test -p vmux_mcp && cargo test -p vmux_service`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_mcp/src/protocol.rs crates/vmux_agent/src/plugin.rs crates/vmux_browser/src/lib.rs
git commit -m "feat(agent): navigation tools return page snapshot after load-settle"
```

---

## Task Descriptions: steer via MCP tool text

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs` (descriptions of `browser_navigate`, `browser_go_back`, `browser_go_forward`, `open_page`, and the `browser_snapshot` note)

- [ ] **Step 1: Update `browser_navigate` description (the primary steer)**

Replace the `#[mcp(description = ...)]` on `BrowserNavigate` (line 22-24) to add, after the existing URL-rules text:

> " This is your primary web tool: do ALL web research here so the user can watch in their visible, logged-in browser. To search, navigate to a search engine results URL (e.g. https://duckduckgo.com/?q=...), then read the returned snapshot and open results. Returns the page snapshot after load — no separate browser_snapshot call needed; use browser_scroll to bring more content into view."

- [ ] **Step 2: Note the inline-snapshot return on back/forward/open_page**

Append to the `BrowserGoBack`, `BrowserGoForward` (lines 52-56) and `open_page_definition()` descriptions: " Returns the page snapshot after load."

- [ ] **Step 3: Update the `page_source.rs` / style assertions if present**

Run: `cargo test -p vmux_mcp -p vmux_layout` to catch any `include_str!`/source-scrape text assertions tied to tool descriptions; update expected strings if a test pins them.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): steer agents to the visible browser in tool descriptions"
```

---

## Final verification (single pass)

- [ ] **Workspace checks**

Run: `cargo fmt && git checkout -- patches/ && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
(Re-apply intended `patches/` edits if fmt touched them; commit only intended changes.)

- [ ] **Manual runtime test (the one pass)**

Launch vmux, open a Vibe agent, give it: *"find me a hotel with AC near Paris this weekend."* Confirm:
1. Vibe does NOT call `web_search`/`web_fetch` (no "Searched …"/"Fetched …" lines).
2. It uses `browser_navigate` (a search-engine URL appears in the visible pane).
3. Navigation results arrive with the snapshot inline (one tool call, not navigate+snapshot).
4. It uses `browser_scroll`; the visible pane scrolls; snapshots show `viewport` + `inViewport`.
5. The pane is the user's real, logged-in browser and is watchable/takeover-able.

- [ ] **Delete this plan file** (per AGENTS.md) and open the PR.

```bash
git rm docs/plans/2026-06-26-visible-agent-browser-research.md
git commit -m "chore: remove completed implementation plan"
```

---

## Self-review notes

- **Spec coverage:** A (disable + steer) → Task A + Descriptions. B (nav returns snapshot) → Task B. C (viewport + scroll) → Tasks C1/C2/C3. Error handling (timeout/no-pane) → Task B Step 4 + existing snapshot error path. Testing → per-task unit tests + final manual pass.
- **Known implementation-discovery points (flagged, not placeholders):** (1) `element_bounds()` coordinate system — Task C2 Step 1 resolves it with a 5-min runtime probe and adjusts C1's `in_viewport_of`. (2) Value-returning V8 access in the render-process crate — Task C2 Step 2 with a pane-size fallback (Step 2b). (3) Command-vs-query result correlation by `request_id` — Task B Step 4, following the existing snapshot request/response pattern.
- **Type consistency:** `RawViewport`/`Viewport` field names match across C1; `BrowserScrollRequest` reuses `BrowserSnapshotResponse`; `AgentCommandResult::Text` and `AgentQuery::BrowserScroll` are referenced consistently in B/C3.
