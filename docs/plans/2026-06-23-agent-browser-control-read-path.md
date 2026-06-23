# Agent Browser Control — Plan 1: Read Path (`vmux_browser_snapshot`)

> **For agentic workers:** Implement this plan directly in this session (NOT subagent-driven). CEF builds are huge and long-lived agents drop sockets — see the Execution note at the end. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose an MCP tool `vmux_browser_snapshot` that reads the live DOM of the focused (or targeted) browser page natively via CEF and returns a compact semantic JSON snapshot (interactive elements with `ref`, `role`, `name`, `value`, `bbox`, `state`).

**Architecture:** Mirrors the existing **Screenshot** agent path across 5 layers. The render process walks the DOM natively with `Frame::visit_dom` (no JavaScript), serializes a raw node dump to JSON, and ships it to the browser process via a `VMUX_SNAPSHOT_RESULT` process message. The browser/Bevy side deserializes it, runs a pure shaping function into the agent-facing snapshot, and returns it as `AgentQueryResult::Text`. Reads only — no auto-wait, no interaction (those are Plan 2).

**Tech Stack:** Rust, Bevy 0.19 (Message API), patched `cef 148.2.0+148.0.8` bindings, `bevy_cef`/`bevy_cef_core` (patched), rkyv (service protocol), serde_json (snapshot), crossbeam/async-channel (CEF→Bevy bridge).

**Resolved feasibility:** `Frame::visit_dom`, `Domvisitor`/`ImplDomvisitor`, `Domdocument`, `Domnode` (with `element_tag_name`, `element_attribute`, `value`, `element_inner_text`, `element_bounds`, `first_child`, `next_sibling`, …), `Rect`, `DomNodeType` are all present in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cef-148.2.0+148.0.8/src/bindings/aarch64_apple_darwin.rs`. Bindings drop the C `get_` prefix. DOM string getters return `CefStringUserfree` → convert with `bevy_cef_core::util::IntoString::into_string`.

---

## File Structure

| File | Responsibility | Tested by |
|------|----------------|-----------|
| `crates/vmux_core/src/dom_snapshot.rs` (create) | Pure serde types (`RawDomNode`, `RawSnapshot`, `Snapshot`, `SnapNode`), the `SNAPSHOT_ATTRS` allowlist, and `shape_snapshot()` + role/name/state derivation. No CEF, no Bevy. | `cargo test -p vmux_core` |
| `crates/vmux_core/src/lib.rs` (modify) | `pub mod dom_snapshot;` | — |
| `crates/vmux_service/src/protocol.rs` (modify) | `AgentQuery::BrowserSnapshot { pane }` variant | `cargo test -p vmux_mcp` (via dispatch) |
| `crates/vmux_mcp/src/tools.rs` (modify) | `browser_snapshot_definition()` + dispatch arm | `cargo test -p vmux_mcp` |
| `patches/bevy_cef_core-0.5.2/Cargo.toml` (modify) | add `vmux_core` dep | — |
| `patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs` (create) | `SnapshotVisitor` (`wrap_domvisitor!`) building `RawSnapshot`; render-side handler entry helper | manual build |
| `patches/bevy_cef_core-0.5.2/src/render_process/render_process_handler.rs` (modify) | `"VMUX_SNAPSHOT"` arm calling the visitor + result send | manual |
| `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs` (modify) | `Browsers::request_snapshot()` send (no trust gate) + register `SnapshotResultHandler` | manual |
| `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler.rs` (modify) | allow `VMUX_SNAPSHOT_RESULT` past the trust gate | manual |
| `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler/snapshot_result_handler.rs` (create) | `ProcessMessageHandler` pushing results into a channel | manual |
| `patches/bevy_cef-0.5.2/src/common/ipc/dom_snapshot.rs` (create) | Bevy channel resources + `SnapshotResult` message + drain system + plugin | manual |
| `crates/vmux_agent/src/events.rs` (modify) | `BrowserSnapshotRequest` / `BrowserSnapshotResponse` messages + pure `snapshot_response_to_query_result` | `cargo test -p vmux_agent` |
| `crates/vmux_agent/src/plugin.rs` (modify) | `handle_agent_queries` arm + `forward_snapshot_responses` + registration | `cargo test -p vmux_agent` |
| `crates/vmux_desktop/src/browser_snapshot.rs` (create) | `start_snapshots` (NonSend `Browsers`, target resolution, `request_snapshot`) + `shape_snapshot_results` (RawSnapshot→shape→response) + plugin wiring | `cargo build -p vmux_desktop` + manual |

Shared types live in `vmux_core` (a leaf crate) so both the patched CEF crate (producer) and the Bevy side (consumer) import them with no dependency cycle and the shaping logic is unit-testable without a CEF runtime.

---

## Task 1: Pure snapshot types + shaping (`vmux_core`)

**Files:**
- Create: `crates/vmux_core/src/dom_snapshot.rs`
- Modify: `crates/vmux_core/src/lib.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Add the module declaration**

In `crates/vmux_core/src/lib.rs`, add (alphabetical with the other `pub mod` lines):

```rust
pub mod dom_snapshot;
```

- [ ] **Step 2: Write the types + allowlist (compile scaffold)**

Create `crates/vmux_core/src/dom_snapshot.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const SNAPSHOT_ATTRS: &[&str] = &[
    "role",
    "aria-label",
    "aria-expanded",
    "aria-selected",
    "alt",
    "title",
    "placeholder",
    "type",
    "name",
    "href",
    "id",
    "tabindex",
    "disabled",
    "required",
    "checked",
];

pub const SNAPSHOT_NODE_CAP: usize = 300;
pub const SNAPSHOT_NAME_CAP: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawDomNode {
    pub tag: String,
    pub text: String,
    pub value: String,
    pub attrs: Vec<(String, String)>,
    pub bounds: [i32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSnapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<RawDomNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapNode {
    #[serde(rename = "ref")]
    pub reference: u32,
    pub role: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub bbox: [i32; 4],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub url: String,
    pub title: String,
    pub nodes: Vec<SnapNode>,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}
```

- [ ] **Step 3: Write failing tests for shaping**

Append to `crates/vmux_core/src/dom_snapshot.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn node(tag: &str, text: &str, attrs: &[(&str, &str)], bounds: [i32; 4]) -> RawDomNode {
        RawDomNode {
            tag: tag.to_string(),
            text: text.to_string(),
            value: String::new(),
            attrs: attrs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            bounds,
        }
    }

    fn raw(nodes: Vec<RawDomNode>) -> RawSnapshot {
        RawSnapshot {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            nodes,
        }
    }

    #[test]
    fn skips_plain_container_without_role_or_text() {
        let snap = shape_snapshot(raw(vec![node("div", "", &[], [0, 0, 100, 40])]));
        assert!(snap.nodes.is_empty());
    }

    #[test]
    fn keeps_button_with_role_and_name_from_text() {
        let snap = shape_snapshot(raw(vec![node("button", "Sign in", &[], [1, 2, 80, 30])]));
        assert_eq!(snap.nodes.len(), 1);
        let n = &snap.nodes[0];
        assert_eq!(n.reference, 0);
        assert_eq!(n.role, "button");
        assert_eq!(n.name, "Sign in");
        assert_eq!(n.bbox, [1, 2, 80, 30]);
    }

    #[test]
    fn input_email_maps_to_textbox_with_placeholder_name() {
        let mut email = node(
            "input",
            "",
            &[("type", "email"), ("placeholder", "Email")],
            [0, 0, 200, 30],
        );
        email.value = "a@b.com".to_string();
        let snap = shape_snapshot(raw(vec![email]));
        let n = &snap.nodes[0];
        assert_eq!(n.role, "textbox");
        assert_eq!(n.name, "Email");
        assert_eq!(n.value.as_deref(), Some("a@b.com"));
    }

    #[test]
    fn aria_label_beats_inner_text() {
        let snap = shape_snapshot(raw(vec![node(
            "a",
            "click here",
            &[("aria-label", "Home")],
            [0, 0, 50, 20],
        )]));
        assert_eq!(snap.nodes[0].role, "link");
        assert_eq!(snap.nodes[0].name, "Home");
    }

    #[test]
    fn disabled_and_required_become_state_flags() {
        let snap = shape_snapshot(raw(vec![node(
            "button",
            "Go",
            &[("disabled", ""), ("required", "")],
            [0, 0, 40, 20],
        )]));
        assert!(snap.nodes[0].state.contains(&"disabled".to_string()));
        assert!(snap.nodes[0].state.contains(&"required".to_string()));
    }

    #[test]
    fn zero_area_node_is_hidden_and_skipped() {
        let snap = shape_snapshot(raw(vec![node("button", "Hidden", &[], [0, 0, 0, 0])]));
        assert!(snap.nodes.is_empty());
    }

    #[test]
    fn refs_are_sequential_and_truncation_sets_flag() {
        let mut nodes = Vec::new();
        for i in 0..(SNAPSHOT_NODE_CAP + 5) {
            nodes.push(node("button", &format!("b{i}"), &[], [0, 0, 10, 10]));
        }
        let snap = shape_snapshot(raw(nodes));
        assert_eq!(snap.nodes.len(), SNAPSHOT_NODE_CAP);
        assert!(snap.truncated);
        assert_eq!(snap.nodes[0].reference, 0);
        assert_eq!(snap.nodes[1].reference, 1);
    }

    #[test]
    fn role_attribute_overrides_tag() {
        let snap = shape_snapshot(raw(vec![node(
            "div",
            "Menu",
            &[("role", "button")],
            [0, 0, 30, 30],
        )]));
        assert_eq!(snap.nodes[0].role, "button");
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p vmux_core dom_snapshot`
Expected: FAIL — `cannot find function shape_snapshot in this scope`.

- [ ] **Step 5: Implement `shape_snapshot` + derivation helpers**

Insert above the `#[cfg(test)]` module in `crates/vmux_core/src/dom_snapshot.rs`:

```rust
pub fn shape_snapshot(raw: RawSnapshot) -> Snapshot {
    let mut nodes = Vec::new();
    let mut truncated = false;
    for raw_node in &raw.nodes {
        if !is_interesting(raw_node) {
            continue;
        }
        if nodes.len() >= SNAPSHOT_NODE_CAP {
            truncated = true;
            break;
        }
        let reference = nodes.len() as u32;
        nodes.push(SnapNode {
            reference,
            role: derive_role(raw_node),
            name: derive_name(raw_node),
            value: derive_value(raw_node),
            bbox: raw_node.bounds,
            state: derive_state(raw_node),
        });
    }
    Snapshot {
        url: raw.url,
        title: raw.title,
        nodes,
        truncated,
    }
}

fn attr<'a>(node: &'a RawDomNode, key: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn has_attr(node: &RawDomNode, key: &str) -> bool {
    node.attrs.iter().any(|(k, _)| k == key)
}

fn area(node: &RawDomNode) -> i32 {
    node.bounds[2] * node.bounds[3]
}

const INTERACTIVE_TAGS: &[&str] = &[
    "a", "button", "input", "select", "textarea", "option", "summary", "label",
];
const LANDMARK_TAGS: &[&str] = &[
    "nav", "main", "header", "footer", "aside", "h1", "h2", "h3", "h4", "h5", "h6",
];

fn is_interesting(node: &RawDomNode) -> bool {
    if area(node) <= 0 {
        return false;
    }
    let tag = node.tag.as_str();
    if INTERACTIVE_TAGS.contains(&tag) {
        return true;
    }
    if has_attr(node, "role") || has_attr(node, "tabindex") || has_attr(node, "aria-label") {
        return true;
    }
    if LANDMARK_TAGS.contains(&tag) && !node.text.trim().is_empty() {
        return true;
    }
    false
}

fn derive_role(node: &RawDomNode) -> String {
    if let Some(role) = attr(node, "role") {
        if !role.is_empty() {
            return role.to_string();
        }
    }
    match node.tag.as_str() {
        "a" => "link".to_string(),
        "button" | "summary" => "button".to_string(),
        "select" => "combobox".to_string(),
        "textarea" => "textbox".to_string(),
        "option" => "option".to_string(),
        "label" => "label".to_string(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading".to_string(),
        "nav" => "navigation".to_string(),
        "main" => "main".to_string(),
        "header" => "banner".to_string(),
        "footer" => "contentinfo".to_string(),
        "aside" => "complementary".to_string(),
        "input" => match attr(node, "type").unwrap_or("text") {
            "checkbox" => "checkbox".to_string(),
            "radio" => "radio".to_string(),
            "submit" | "button" | "reset" => "button".to_string(),
            "range" => "slider".to_string(),
            _ => "textbox".to_string(),
        },
        other => other.to_string(),
    }
}

fn derive_name(node: &RawDomNode) -> String {
    let candidate = attr(node, "aria-label")
        .filter(|v| !v.trim().is_empty())
        .or_else(|| attr(node, "alt").filter(|v| !v.trim().is_empty()))
        .or_else(|| attr(node, "title").filter(|v| !v.trim().is_empty()))
        .or_else(|| attr(node, "placeholder").filter(|v| !v.trim().is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| node.text.trim().to_string());
    let mut name: String = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.chars().count() > SNAPSHOT_NAME_CAP {
        name = name.chars().take(SNAPSHOT_NAME_CAP).collect();
    }
    name
}

fn derive_value(node: &RawDomNode) -> Option<String> {
    matches!(node.tag.as_str(), "input" | "textarea" | "select")
        .then(|| node.value.clone())
}

fn derive_state(node: &RawDomNode) -> Vec<String> {
    let mut state = Vec::new();
    for flag in ["disabled", "required", "checked"] {
        if has_attr(node, flag) {
            state.push(flag.to_string());
        }
    }
    if attr(node, "aria-expanded") == Some("true") {
        state.push("expanded".to_string());
    }
    if attr(node, "aria-selected") == Some("true") {
        state.push("selected".to_string());
    }
    state
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vmux_core dom_snapshot`
Expected: PASS (8 tests).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_core/src/dom_snapshot.rs crates/vmux_core/src/lib.rs
git commit -m "feat(core): DOM snapshot types + shaping for agent browser control"
```

---

## Task 2: Service protocol variant (`vmux_service`)

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs:153-184` (the `AgentQuery` enum)

- [ ] **Step 1: Add the query variant**

In the `enum AgentQuery` block, add a variant (keep style consistent with `Screenshot`):

```rust
    BrowserSnapshot {
        pane: Option<String>,
    },
```

(Reuse `AgentQueryResult::Text(String)` for the response — no result-enum change. The variant derives `rkyv::Archive/Serialize/Deserialize + PartialEq, Eq`; `Option<String>` satisfies all.)

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build -p vmux_service`
Expected: builds clean. (`AgentQuery` gains a non-exhaustive arm; downstream `match` sites in `vmux_agent` are updated in Task 6 — compiling `vmux_service` alone passes.)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(service): AgentQuery::BrowserSnapshot variant"
```

---

## Task 3: MCP tool definition + dispatch (`vmux_mcp`)

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs` (`tool_definitions()` ~444, `dispatch_with_anchor` ~472, new `browser_snapshot_definition()`)
- Test: `crates/vmux_mcp/src/tools.rs` (`#[cfg(test)] mod tests` ~702)

- [ ] **Step 1: Write failing tests**

In the existing test module in `crates/vmux_mcp/src/tools.rs`, add:

```rust
    #[test]
    fn browser_snapshot_dispatches_to_query_with_pane() {
        let q = dispatch_query(
            "vmux_browser_snapshot",
            serde_json::json!({ "target": "pane:42" }),
        )
        .unwrap();
        assert_eq!(
            q,
            AgentQuery::BrowserSnapshot {
                pane: Some("pane:42".to_string())
            }
        );
    }

    #[test]
    fn browser_snapshot_defaults_pane_to_none() {
        let q = dispatch_query("vmux_browser_snapshot", serde_json::json!({})).unwrap();
        assert_eq!(q, AgentQuery::BrowserSnapshot { pane: None });
    }

    #[test]
    fn browser_snapshot_is_listed() {
        assert!(
            tool_definitions()
                .iter()
                .any(|d| d.name == "vmux_browser_snapshot")
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_mcp browser_snapshot`
Expected: FAIL — dispatch returns `Err("unknown tool …")` / tool not listed.

- [ ] **Step 3: Add the definition builder**

Near `screenshot_definition()` in `crates/vmux_mcp/src/tools.rs`:

```rust
fn browser_snapshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "vmux_browser_snapshot".into(),
        description: "Read the current page's DOM as a compact semantic snapshot. Returns JSON \
with the page url/title and a list of interactive elements, each with a stable `ref`, `role`, \
`name`, `value`, `bbox` ([x,y,w,h] in CSS px), and `state` flags. Use the `ref` values to target \
later interaction tools. Pass `target` = a pane:<id> or stack:<id> from vmux_read_layout to pick a \
specific page; defaults to the focused page."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Optional pane:<id> or stack:<id>; focused page if omitted."
                }
            }
        }),
    }
}
```

- [ ] **Step 4: Register it in `tool_definitions()`**

After `defs.push(screenshot_definition());`:

```rust
    defs.push(browser_snapshot_definition());
```

- [ ] **Step 5: Add the dispatch arm**

In `dispatch_with_anchor`, alongside the `screenshot` arm (note `name` already had `vmux_` stripped):

```rust
    if name == "browser_snapshot" {
        let pane = match arguments.get("target") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            Some(_) => return Err("browser_snapshot.target must be a string".to_string()),
        };
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::BrowserSnapshot { pane },
        ));
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vmux_mcp browser_snapshot`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): vmux_browser_snapshot tool definition + dispatch"
```

---

## Task 4: Agent bridge messages + mapping (`vmux_agent`)

**Files:**
- Modify: `crates/vmux_agent/src/events.rs:56-74` (message types)
- Modify: `crates/vmux_agent/src/plugin.rs` (`handle_agent_queries` ~947, `forward_*` ~1070, registration ~106)
- Test: `crates/vmux_agent/src/events.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Add the message + response types and a pure mapper**

In `crates/vmux_agent/src/events.rs`, mirroring `ScreenshotRequest`/`ScreenshotResponse`:

```rust
#[derive(Message, Clone)]
pub struct BrowserSnapshotRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct BrowserSnapshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<String, String>,
}

pub fn snapshot_response_to_query_result(
    result: &Result<String, String>,
) -> vmux_service::protocol::AgentQueryResult {
    use vmux_service::protocol::AgentQueryResult;
    match result {
        Ok(json) => AgentQueryResult::Text(json.clone()),
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}
```

- [ ] **Step 2: Write a failing test for the mapper**

Append to `crates/vmux_agent/src/events.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::AgentQueryResult;

    #[test]
    fn ok_snapshot_maps_to_text() {
        let out = snapshot_response_to_query_result(&Ok("{\"url\":\"x\"}".to_string()));
        assert_eq!(out, AgentQueryResult::Text("{\"url\":\"x\"}".to_string()));
    }

    #[test]
    fn err_snapshot_maps_to_error() {
        let out = snapshot_response_to_query_result(&Err("no page".to_string()));
        assert_eq!(out, AgentQueryResult::Error("no page".to_string()));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vmux_agent snapshot`
Expected: FAIL — `snapshot_response_to_query_result` not found until Step 1 compiles; if Step 1 already saved, FAIL becomes the missing `match` arm in `plugin.rs` (Step 4). Run after Step 4 if needed.

- [ ] **Step 4: Wire the query arm + forwarder + registration in `plugin.rs`**

In `handle_agent_queries`, add a `MessageWriter<BrowserSnapshotRequest>` param and the arm:

```rust
            AgentQuery::BrowserSnapshot { ref pane } => {
                browser_snapshot_writer.write(BrowserSnapshotRequest {
                    request_id: request.request_id.0,
                    pane: pane.clone(),
                });
            }
```

Add the forwarder (next to `forward_screenshot_responses`):

```rust
fn forward_snapshot_responses(
    mut reader: MessageReader<BrowserSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: snapshot_response_to_query_result(&response.result),
        });
    }
}
```

In the plugin `build` (~106-123), register messages + the system:

```rust
            .add_message::<BrowserSnapshotRequest>()
            .add_message::<BrowserSnapshotResponse>()
```
```rust
            .add_systems(Update, forward_snapshot_responses)
```

(Import `BrowserSnapshotRequest`, `BrowserSnapshotResponse`, `snapshot_response_to_query_result` from `crate::events`.)

- [ ] **Step 5: Run tests + build**

Run: `cargo test -p vmux_agent snapshot`
Expected: PASS (2 tests).
Run: `cargo build -p vmux_agent`
Expected: builds clean (the new `AgentQuery` arm makes the `match` exhaustive).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/src/events.rs crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): browser snapshot request/response bridge"
```

---

> **Checkpoint:** Tasks 1-4 are fully covered by `cargo test`. Tasks 5-8 touch the patched CEF crates (`bevy_cef`/`bevy_cef_core`), which are **excluded** from `cargo test` (CI: `--exclude bevy_cef_core --exclude bevy_cef`). They are verified by `cargo build -p <crate>` and the manual runtime test in Task 8. Build the CEF crates with a warm target dir; expect long builds.

---

## Task 5: CEF render-side DOM visitor (`bevy_cef_core`)

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/Cargo.toml` (add `vmux_core`)
- Create: `patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/lib.rs` (module decl + prelude export)
- Modify: `patches/bevy_cef_core-0.5.2/src/render_process/render_process_handler.rs:43-47,132-158`

- [ ] **Step 1: Add the `vmux_core` dependency**

In `patches/bevy_cef_core-0.5.2/Cargo.toml` `[dependencies]`:

```toml
vmux_core = { path = "../../crates/vmux_core" }
```

- [ ] **Step 2: Write the visitor**

Create `patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs`:

```rust
use crate::util::IntoString;
use cef::rc::Rc;
use cef::{
    Domdocument, Domnode, Domvisitor, ImplDomdocument, ImplDomnode, WrapDomvisitor, wrap_domvisitor,
};
use vmux_core::dom_snapshot::{RawDomNode, RawSnapshot, SNAPSHOT_ATTRS, SNAPSHOT_NAME_CAP};

pub fn snapshot_json_for(document: Option<&mut Domdocument>) -> String {
    let snapshot = build_raw_snapshot(document);
    serde_json::to_string(&snapshot).unwrap_or_else(|_| "{\"url\":\"\",\"title\":\"\",\"nodes\":[]}".to_string())
}

fn build_raw_snapshot(document: Option<&mut Domdocument>) -> RawSnapshot {
    let Some(document) = document else {
        return RawSnapshot {
            url: String::new(),
            title: String::new(),
            nodes: Vec::new(),
        };
    };
    let url = document.base_url().into_string();
    let title = document.title().into_string();
    let mut nodes = Vec::new();
    if let Some(body) = document.body() {
        walk(&body, &mut nodes);
    }
    RawSnapshot { url, title, nodes }
}

fn walk(node: &Domnode, out: &mut Vec<RawDomNode>) {
    if node.is_element() != 0 {
        out.push(raw_from_element(node));
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        walk(&current, out);
        child = current.next_sibling();
    }
}

fn raw_from_element(node: &Domnode) -> RawDomNode {
    let tag = node.element_tag_name().into_string().to_lowercase();
    let mut text = node.element_inner_text().into_string();
    if text.chars().count() > SNAPSHOT_NAME_CAP {
        text = text.chars().take(SNAPSHOT_NAME_CAP).collect();
    }
    let value = node.value().into_string();
    let mut attrs = Vec::new();
    for key in SNAPSHOT_ATTRS {
        let cef_key = (*key).into();
        if node.has_element_attribute(Some(&cef_key)) != 0 {
            let v = node.element_attribute(Some(&cef_key)).into_string();
            attrs.push(((*key).to_string(), v));
        }
    }
    let bounds = node.element_bounds();
    RawDomNode {
        tag,
        text,
        value,
        attrs,
        bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
    }
}

wrap_domvisitor! {
    struct SnapshotVisitor {
        sink: std::rc::Rc<std::cell::RefCell<String>>,
    }
    impl Domvisitor {
        fn visit(&self, document: Option<&mut Domdocument>) {
            *self.sink.borrow_mut() = snapshot_json_for(document);
        }
    }
}
```

> Note: confirm the exact `wrap_domvisitor!` field/visit signature against `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cef-148.2.0+148.0.8/src/bindings/aarch64_apple_darwin.rs:5613-5626` while implementing; the `visit(&self, document: Option<&mut Domdocument>)` shape matches `ImplDomvisitor`. The `sink`/`RefCell` captures the JSON synchronously because `visit_dom`'s callback runs inline on the renderer thread.

- [ ] **Step 3: Export the module**

In `patches/bevy_cef_core-0.5.2/src/lib.rs` add `pub mod dom_snapshot;` and, if the crate has a `prelude`, re-export `pub use crate::dom_snapshot::snapshot_json_for;`.

- [ ] **Step 4: Add the message-name constants**

In `render_process_handler.rs` near line 43-47:

```rust
pub const PROCESS_MESSAGE_SNAPSHOT: &str = "vmux-snapshot";
pub const PROCESS_MESSAGE_SNAPSHOT_RESULT: &str = "vmux-snapshot-result";
```

- [ ] **Step 5: Add the render-side handler arm (outside the embedded-scheme/v8 guard)**

In `render_process_handler.rs` `on_process_message_received`, add a branch BEFORE the existing `if let … has_embedded_scheme … v8_context()` block (snapshot needs neither):

```rust
        if let Some(message) = message.as_ref()
            && message.name().into_string() == PROCESS_MESSAGE_SNAPSHOT
            && let Some(frame) = frame.as_ref()
        {
            let request_id = message
                .argument_list()
                .map(|a| a.string(0).into_string())
                .unwrap_or_default();
            let sink = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
            let mut visitor = crate::dom_snapshot::SnapshotVisitor::new(sink.clone());
            frame.visit_dom(Some(&mut visitor));
            let json = sink.borrow().clone();
            if let Some(mut out) = process_message_create(Some(&PROCESS_MESSAGE_SNAPSHOT_RESULT.into()))
                && let Some(args) = out.argument_list()
            {
                args.set_string(0, Some(&request_id.as_str().into()));
                args.set_string(1, Some(&json.as_str().into()));
                frame.send_process_message(
                    ProcessId::from(cef_dll_sys::cef_process_id_t::PID_BROWSER),
                    Some(&mut out),
                );
            }
            return 1;
        }
```

> The exact borrow pattern around `message`/`frame` (they are `Option<&mut …>`) must be adapted so the existing block below still compiles — destructure once at the top or use `.as_ref()`/`.as_deref_mut()` as the borrow checker requires. The `process_message_create`, `ProcessId::from(cef_dll_sys::cef_process_id_t::PID_BROWSER)`, and `args.set_string` idioms are verbatim from `cef_api_handler.rs::execute_emit`.

- [ ] **Step 6: Build**

Run: `cargo build -p bevy_cef_core`
Expected: builds (long, CEF). Fix borrow/signature mismatches against the binding source.

- [ ] **Step 7: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/Cargo.toml patches/bevy_cef_core-0.5.2/src/dom_snapshot.rs patches/bevy_cef_core-0.5.2/src/lib.rs patches/bevy_cef_core-0.5.2/src/render_process/render_process_handler.rs
git commit -m "feat(cef): native DOM visit_dom snapshot in render process"
```

---

## Task 6: CEF browser-side send + result handler + trust allowance (`bevy_cef_core`)

**Files:**
- Create: `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler/snapshot_result_handler.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler.rs:218-259` (trust allowance) + handler module decl
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs` (`request_snapshot` method ~587 area; handler registration ~1531)

- [ ] **Step 1: Write the result handler**

Create `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler/snapshot_result_handler.rs` (mirror `js_emit_event_handler.rs`):

```rust
use crate::prelude::*;
use crate::util::IntoString;
use async_channel::Sender;
use bevy::prelude::Entity;
use cef::{Browser, Frame, ImplListValue, ListValue};

#[derive(Debug, Clone)]
pub struct SnapshotResultRaw {
    pub webview: Entity,
    pub request_id: String,
    pub json: String,
}

pub struct SnapshotResultHandler {
    webview: Entity,
    sender: Sender<SnapshotResultRaw>,
}

impl SnapshotResultHandler {
    pub const fn new(webview: Entity, sender: Sender<SnapshotResultRaw>) -> Self {
        Self { webview, sender }
    }
}

impl ProcessMessageHandler for SnapshotResultHandler {
    fn process_name(&self) -> &'static str {
        crate::render_process::render_process_handler::PROCESS_MESSAGE_SNAPSHOT_RESULT
    }
    fn handle_message(&self, _browser: &mut Browser, _frame: &mut Frame, args: Option<ListValue>) {
        if let Some(args) = args {
            let _ = self.sender.send_blocking(SnapshotResultRaw {
                webview: self.webview,
                request_id: args.string(0).into_string(),
                json: args.string(1).into_string(),
            });
        }
    }
}
```

(Declare the new file as a submodule wherever `js_emit_event_handler` is declared, and re-export `SnapshotResultHandler` + `SnapshotResultRaw` through the prelude.)

- [ ] **Step 2: Allow the result message past the browser-side trust gate**

In `client_handler.rs` `on_process_message_received`, the gate currently drops any message whose frame url is not a trusted embedded page. Add an allowance BEFORE the drop, so the snapshot result (which arrives on an arbitrary `https://` frame) reaches its handler:

```rust
            let name = message.name().into_string();
            let url = frame.url().into_string();
            let snapshot_result =
                name == crate::render_process::render_process_handler::PROCESS_MESSAGE_SNAPSHOT_RESULT;
            if !snapshot_result && !crate::util::is_trusted_embedded_page(&url) {
                crate::util::webview_debug_log(format!(
                    "ipc: dropped inbound '{name}' from untrusted url={url}"
                ));
                return 1;
            }
```

(Leave the existing BRP/non-debug check unchanged; it only fires for `PROCESS_MESSAGE_BRP`.)

- [ ] **Step 3: Add `Browsers::request_snapshot` (no outbound trust gate)**

In `browsers.rs`, modeled on `emit_event_raw_json` but WITHOUT the `is_trusted_embedded_page` check and targeting `PID_RENDERER`:

```rust
    pub fn request_snapshot(&self, webview: &Entity, request_id: &str) {
        if let Some(mut process_message) = process_message_create(Some(
            &crate::render_process::render_process_handler::PROCESS_MESSAGE_SNAPSHOT.into(),
        )) && let Some(argument_list) = process_message.argument_list()
            && let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
        {
            argument_list.set_string(0, Some(&request_id.into()));
            frame.send_process_message(
                ProcessId::from(cef_dll_sys::cef_process_id_t::PID_RENDERER),
                Some(&mut process_message),
            );
        }
    }
```

- [ ] **Step 4: Register the handler at browser-build (carry the sender in)**

The build site (`browsers.rs:1531`) chains `.with_message_handler(...)`. The browser-build context already threads `ipc_event_sender`/`bin_ipc_event_sender`; thread a new `snapshot_result_sender: Sender<SnapshotResultRaw>` the same way and add:

```rust
            .with_message_handler(SnapshotResultHandler::new(webview, snapshot_result_sender))
```

Trace the `ipc_event_sender` parameter back to where `Browsers` is constructed and add a parallel `snapshot_result_sender` field/argument (created in Task 7 Step 1 on the Bevy side and inserted alongside `IpcEventRawSender`).

- [ ] **Step 5: Build**

Run: `cargo build -p bevy_cef_core`
Expected: builds (long). Resolve the sender-threading and prelude exports.

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/browser_process/
git commit -m "feat(cef): snapshot request send + result handler + trust allowance"
```

---

## Task 7: Bevy IPC plumbing + desktop worker (`bevy_cef`, `vmux_desktop`)

**Files:**
- Create: `patches/bevy_cef-0.5.2/src/common/ipc/dom_snapshot.rs`
- Modify: `patches/bevy_cef-0.5.2/src/common/ipc.rs` (module decl) + the IPC plugin set + `Browsers` construction wiring
- Create: `crates/vmux_desktop/src/browser_snapshot.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (module decl + plugin add)

- [ ] **Step 1: Bevy snapshot channel + message (mirror `js_emit.rs`)**

Create `patches/bevy_cef-0.5.2/src/common/ipc/dom_snapshot.rs`:

```rust
use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::SnapshotResultRaw;

#[derive(Resource)]
pub struct SnapshotResultSender(pub Sender<SnapshotResultRaw>);
#[derive(Resource)]
pub struct SnapshotResultReceiver(pub Receiver<SnapshotResultRaw>);

#[derive(Message, Clone)]
pub struct SnapshotResult {
    pub webview: Entity,
    pub request_id: String,
    pub json: String,
}

fn drain_snapshot_results(
    receiver: Res<SnapshotResultReceiver>,
    mut writer: MessageWriter<SnapshotResult>,
) {
    while let Ok(raw) = receiver.0.try_recv() {
        writer.write(SnapshotResult {
            webview: raw.webview,
            request_id: raw.request_id,
            json: raw.json,
        });
    }
}

pub struct DomSnapshotPlugin;
impl Plugin for DomSnapshotPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(SnapshotResultSender(tx))
            .insert_resource(SnapshotResultReceiver(rx))
            .add_message::<SnapshotResult>()
            .add_systems(Update, drain_snapshot_results);
    }
}
```

Declare it in `patches/bevy_cef-0.5.2/src/common/ipc.rs` (`pub mod dom_snapshot;`), add `DomSnapshotPlugin` to the cef plugin group, and at `Browsers` construction pass `SnapshotResultSender.0.clone()` through to the `snapshot_result_sender` added in Task 6 Step 4. Re-export `SnapshotResult`, `DomSnapshotPlugin` via the bevy_cef prelude.

- [ ] **Step 2: Build the CEF Bevy crate**

Run: `cargo build -p bevy_cef`
Expected: builds (long). Confirm the sender reaches `Browsers`.

- [ ] **Step 3: Desktop worker — request + shape (NonSend `Browsers`)**

Create `crates/vmux_desktop/src/browser_snapshot.rs`:

```rust
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_agent::events::{BrowserSnapshotRequest, BrowserSnapshotResponse};
use vmux_core::dom_snapshot::{shape_snapshot, RawSnapshot};

fn hex(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_hex(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn start_snapshots(
    _non_send: NonSendMarker,
    mut reader: MessageReader<BrowserSnapshotRequest>,
    browsers: NonSend<Browsers>,
    focused: Res<FocusedStack>,
    panes: Query<&vmux_layout::Pane>,
    stacks: Query<(&vmux_layout::Stack, Option<&Children>)>,
    web_pages: Query<(), (With<Browser>, Without<vmux_terminal::Terminal>)>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
) {
    for req in reader.read() {
        match resolve_target(&req.pane, &focused, &panes, &stacks, &web_pages) {
            Some(webview) => browsers.request_snapshot(&webview, &hex(&req.request_id)),
            None => writer.write(BrowserSnapshotResponse {
                request_id: req.request_id,
                result: Err("no browser page to snapshot".to_string()),
            }),
        }
    }
}

fn shape_snapshot_results(
    mut reader: MessageReader<SnapshotResult>,
    mut writer: MessageWriter<BrowserSnapshotResponse>,
) {
    for result in reader.read() {
        let Some(request_id) = parse_hex(&result.request_id) else {
            continue;
        };
        let mapped = serde_json::from_str::<RawSnapshot>(&result.json)
            .map(|raw| serde_json::to_string(&shape_snapshot(raw)).unwrap_or_default())
            .map_err(|e| format!("snapshot parse error: {e}"));
        writer.write(BrowserSnapshotResponse {
            request_id,
            result: mapped,
        });
    }
}

pub struct BrowserSnapshotPlugin;
impl Plugin for BrowserSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start_snapshots, shape_snapshot_results));
    }
}
```

> `resolve_target(...)` is the target resolver. Reuse the exact resolution `handle_browser_navigate_requests` uses (`crates/vmux_browser/src/lib.rs:3265`): explicit `pane:`/`stack:` string via `vmux_layout::target::parse_pane_target` → that pane's active webview; else `FocusedStack` active webview; else focused pane. Lift the shared resolution into a `pub fn` in `vmux_layout::target` (e.g. `resolve_browser_target(...) -> Option<Entity>`) so both call sites use it, rather than duplicating. Match its real signature when implementing; the params above are indicative and must be reconciled with that function.

- [ ] **Step 4: Register the plugin**

In `crates/vmux_desktop/src/lib.rs` add `mod browser_snapshot;` and add `BrowserSnapshotPlugin` to the app (after the agent plugin and the bevy_cef `DomSnapshotPlugin` so `SnapshotResult` is registered first).

- [ ] **Step 5: Build the desktop binary**

Run: `cargo build -p vmux_desktop`
Expected: builds (long). Reconcile `resolve_target` with the real `vmux_layout::target` API and component imports.

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef-0.5.2/src/common/ patches/bevy_cef-0.5.2/src/ crates/vmux_desktop/src/browser_snapshot.rs crates/vmux_desktop/src/lib.rs crates/vmux_layout/src/target.rs
git commit -m "feat(desktop): browser snapshot worker + bevy_cef IPC plumbing"
```

---

## Task 8: End-to-end runtime verification (manual)

**Files:** none (verification only)

- [ ] **Step 1: Full build**

Run: `cargo build -p vmux_desktop`
Expected: clean build.

- [ ] **Step 2: Launch and drive via MCP**

Start vmux, open a normal `https://` page (e.g. a login form) in the focused pane. From an MCP client (vibe, or `vmux mcp --anchor <id>` driven by a manual JSON-RPC `tools/call`), call `vmux_browser_snapshot` with no args.

Expected: a JSON snapshot with the real `url`/`title` and a non-empty `nodes` list; each interactive element has `ref`, `role`, `name`, `bbox` with non-zero width/height; inputs carry `value`; disabled/required elements show `state`.

- [ ] **Step 3: Targeted snapshot**

Call `vmux_browser_snapshot` with `{"target":"pane:<id>"}` from `vmux_read_layout`. Expected: snapshot of that specific page.

- [ ] **Step 4: Security spot-check**

Confirm the trust-gate relaxation is scoped: only `vmux-snapshot-result` bypasses `is_trusted_embedded_page`. Grep the diff for the new allowance and confirm no other inbound message name is let through. Confirm `request_snapshot` is the only outbound send lacking the trust check, and it only ever sends the benign `vmux-snapshot` (no payload).

- [ ] **Step 5: Bounds sanity**

Pick one element from the snapshot, eyeball its `bbox` against where it sits on screen (top-left origin, CSS px). If clearly offset/scaled, note the `device_scale_factor` relationship for Plan 2 (interaction maps bbox→click coords). Reading alone does not require this to be exact.

- [ ] **Step 6: Commit any fixes, then run the CI-equivalent test subset**

```bash
env -u CEF_PATH cargo test -p vmux_core -p vmux_mcp -p vmux_agent
```
Expected: PASS. (CEF crates are excluded from tests by design.)

---

## Self-Review

- **Spec coverage:** read DOM ✓ (Tasks 1,5–8), find ✗ (deferred to Plan 2), interact ✗ (Plan 2), targeting ✓ (Task 3 param, Task 7 resolver), one-shot MCP ✓ (reuses `run_agent_query`; snapshot is fast, no blocking loop needed), native/no-eval ✓ (`visit_dom`, no V8), live high-trust ✓ (no gate; scoped trust allowance). Auto-wait ✗ — correctly out of scope for a pure read (belongs to action tools in Plan 2).
- **Placeholder scan:** CEF tasks carry explicit "reconcile against binding/borrow-checker" notes, not TODOs — the code is concrete and modeled on verbatim excerpts. `resolve_target` is the one function whose signature must be matched to the real `vmux_layout::target` API at implementation time (flagged in-task).
- **Type consistency:** `RawSnapshot`/`RawDomNode` (producer in `bevy_cef_core::dom_snapshot`, consumer via `vmux_core`) ✓ identical type; `SnapshotResultRaw` (cef_core) → `SnapshotResult` (bevy_cef) → `BrowserSnapshotResponse` (vmux_agent) field names aligned; `request_id` is `[u8;16]` on the Bevy/service side, hex `String` across the CEF process-message boundary, converted by `hex`/`parse_hex` ✓; message names `vmux-snapshot` / `vmux-snapshot-result` referenced by one shared const each ✓.

---

## Execution note

Implement **inline, in this session**, not via subagent-driven development. The CEF crates produce very large builds and long-running subagents drop their sockets mid-build (project history). Build with a warm, worktree-local target dir (do not share `CARGO_TARGET_DIR` across worktrees — CEF cmake pins absolute paths). Tasks 1–4 are TDD and fast; Tasks 5–7 are build-and-manual against the patched CEF crates; Task 8 is the runtime gate.

**Follow-on plans (not this plan):**
- **Plan 2 — Interaction:** `click`/`type`/`press_key`/`scroll` via cached `ref→bbox` + native input, the auto-wait state machine (load-state + debounce), and upgrading `navigate`/`go_back`/`go_forward`/`reload` to return a snapshot. Reuses this plan's `bbox` and target resolver.
- **Plan 3 — `find` + `text` mode:** snapshot filtering by name/role and an innerText reading mode.
