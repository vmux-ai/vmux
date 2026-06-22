# Screenshot MCP Tool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. (Do NOT subagent-drive this plan — vmux CEF builds are huge and long-lived subagents drop sockets; implement inline with a warm target dir.) Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an MCP tool `screenshot` so an agent inside a vmux terminal pane can capture the vmux window (optionally cropped to a pane) and see it inline, to test-drive its own UI work.

**Architecture:** Reuse the existing async agent-query relay (the `read_layout` path). A new `AgentQuery::Screenshot` is relayed MCP → service → GUI. On the GUI, `vmux_agent` emits a `ScreenshotRequest` Bevy message; `vmux_desktop` captures the single main `NSWindow` via ScreenCaptureKit (windowed CEF panes are child subviews, so one window capture is faithful), saves a full-res PNG to `~/.vmux/screenshots/`, downscales a copy, and returns it via `AgentQueryResult::Image`. The off-thread SCK completion handler wakes the winit loop so the response is flushed within the 5s query timeout.

**Tech Stack:** Rust, Bevy 0.19 (messages/systems), rkyv (service protocol), ScreenCaptureKit via `objc2-screen-capture-kit` 0.3, `image` 0.25 (crop/downscale/PNG), `crossbeam-channel` (bridge), `base64` (MCP image block).

---

## File Structure

- `crates/vmux_service/src/protocol.rs` — **Modify.** Add `AgentQuery::Screenshot` + `AgentQueryResult::Image`.
- `crates/vmux_core/src/profile.rs` — **Modify.** Add `screenshots_dir()`.
- `crates/vmux_mcp/src/tools.rs` — **Modify.** `screenshot` tool definition + dispatch.
- `crates/vmux_mcp/src/protocol.rs` — **Modify.** Map `AgentQueryResult::Image` → MCP text + image content.
- `crates/vmux_mcp/Cargo.toml` — **Modify.** Add `base64`.
- `crates/vmux_agent/src/plugin.rs` — **Modify.** `ScreenshotRequest`/`ScreenshotResponse`/`ScreenshotImage` messages, query arm, forwarder, registration.
- `crates/vmux_desktop/src/screenshot.rs` — **Create.** Pure helpers (crop/downscale/filename), bridge resource, start/drain systems, native SCK capture (macOS) + non-macOS stub.
- `crates/vmux_desktop/src/lib.rs` — **Modify.** Declare module, register resource + systems.
- `crates/vmux_desktop/Cargo.toml` — **Modify.** Add `objc2-screen-capture-kit`, `crossbeam-channel`, `image`.

Constant: `MAX_INLINE_EDGE = 1568` (long-edge cap for the inline image).

---

## Task 1: Protocol — `AgentQuery::Screenshot` + `AgentQueryResult::Image`

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (enum `AgentQuery` ~line 149, `AgentQueryResult` ~line 170, tests ~line 608)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/vmux_service/src/protocol.rs`:

```rust
    #[test]
    fn agent_query_screenshot_rkyv_round_trip() {
        let q = AgentQuery::Screenshot { pane: Some("pane:42".into()) };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);

        let none = AgentQuery::Screenshot { pane: None };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&none).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, none);
    }

    #[test]
    fn agent_query_result_image_rkyv_round_trip() {
        let r = AgentQueryResult::Image {
            path: "/tmp/x.png".into(),
            png: vec![1, 2, 3, 4],
            width: 320,
            height: 200,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let back: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, r);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_service --lib agent_query_screenshot_rkyv_round_trip agent_query_result_image_rkyv_round_trip`
Expected: FAIL to compile (`Screenshot` / `Image` variants don't exist).

- [ ] **Step 3: Add the variants**

In `enum AgentQuery` (after `ListSpaces,`):

```rust
    Screenshot {
        pane: Option<String>,
    },
```

In `enum AgentQueryResult` (after `Spaces(String),`):

```rust
    Image {
        path: String,
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_service --lib agent_query_screenshot_rkyv_round_trip agent_query_result_image_rkyv_round_trip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(service): screenshot agent query + image result"
```

---

## Task 2: `vmux_core::profile::screenshots_dir()`

**Files:**
- Modify: `crates/vmux_core/src/profile.rs` (near `config_dir()` ~line 34)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_core/src/profile.rs` (create one if absent):

```rust
#[cfg(test)]
mod screenshot_dir_tests {
    use super::*;

    #[test]
    fn screenshots_dir_is_under_config_dir() {
        assert_eq!(screenshots_dir(), config_dir().join("screenshots"));
        assert!(screenshots_dir().ends_with(".vmux/screenshots"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core --lib screenshots_dir_is_under_config_dir`
Expected: FAIL to compile (`screenshots_dir` not found).

- [ ] **Step 3: Implement**

Add after `config_dir()`:

```rust
/// Screenshot output directory: `~/.vmux/screenshots`.
pub fn screenshots_dir() -> PathBuf {
    config_dir().join("screenshots")
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_core --lib screenshots_dir_is_under_config_dir`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/profile.rs
git commit -m "feat(core): screenshots_dir helper"
```

---

## Task 3: MCP tool — definition, dispatch, response mapping

**Files:**
- Modify: `crates/vmux_mcp/Cargo.toml`
- Modify: `crates/vmux_mcp/src/tools.rs` (definitions ~line 378, dispatch ~line 535, tests)
- Modify: `crates/vmux_mcp/src/protocol.rs` (`query_result_to_mcp_response` ~line 368, tests)

- [ ] **Step 1: Add the base64 dependency**

In `crates/vmux_mcp/Cargo.toml` under `[dependencies]`:

```toml
base64 = "0.22"
```

- [ ] **Step 2: Write the failing tests**

Add to the `tests` module in `crates/vmux_mcp/src/tools.rs`:

```rust
    #[test]
    fn list_tools_includes_screenshot() {
        assert!(tool_names().contains(&"screenshot".to_string()));
    }

    #[test]
    fn screenshot_dispatches_to_query_with_and_without_pane() {
        let target = dispatch_from_tool_call("screenshot", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::Screenshot { pane: None })
        ));

        let target = dispatch_from_tool_call(
            "screenshot",
            serde_json::json!({ "pane": "stack:7" }),
        )
        .unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::Screenshot { pane: Some(p) })
                if p == "stack:7"
        ));

        // blank pane is treated as None.
        let target = dispatch_from_tool_call("screenshot", serde_json::json!({ "pane": "  " })).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::Screenshot { pane: None })
        ));
    }
```

Add to the `tests` module in `crates/vmux_mcp/src/protocol.rs`:

```rust
    #[test]
    fn image_query_result_maps_to_text_and_image_blocks() {
        use vmux_service::protocol::AgentQueryResult;
        let resp = query_result_to_mcp_response(AgentQueryResult::Image {
            path: "/tmp/shot.png".into(),
            png: vec![137, 80, 78, 71],
            width: 800,
            height: 600,
        });
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"].as_str().unwrap().contains("/tmp/shot.png"));
        assert!(content[0]["text"].as_str().unwrap().contains("800"));
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["mimeType"], "image/png");
        assert_eq!(content[1]["data"], "iVBORw==");
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vmux_mcp --lib screenshot image_query_result_maps_to_text_and_image_blocks`
Expected: FAIL (tool absent / `Image` arm missing).

- [ ] **Step 4: Add the tool definition**

In `crates/vmux_mcp/src/tools.rs`, add after `read_terminal_definition()`:

```rust
fn screenshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "screenshot".into(),
        description: "Capture the vmux window as a PNG and return it inline so you can SEE the current UI \
(use it to verify your own UI changes). Captures the whole window exactly as it appears on screen — all \
visible panes (browser, terminal, editor) and layout chrome. Optionally pass `pane` (a pane:<id> or \
stack:<id> from read_layout) to crop to just that region. The full-resolution image is saved under \
~/.vmux/screenshots/ and a downscaled copy is returned inline. macOS only; the first call may prompt for \
Screen Recording permission — grant it in System Settings ▸ Privacy & Security ▸ Screen Recording, then \
call again."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "pane": {
                    "type": "string",
                    "description": "Optional pane:<id> or stack:<id> to crop to; whole window if omitted."
                }
            }
        }),
    }
}
```

In `tool_definitions()`, after `defs.push(read_terminal_definition());`:

```rust
    defs.push(screenshot_definition());
```

- [ ] **Step 5: Add the dispatch arm**

In `dispatch_with_anchor`, alongside the other named queries (after the `read_terminal` block, before the `read_layout` block):

```rust
    if name == "screenshot" {
        let pane = arguments
            .get("pane")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::Screenshot { pane },
        ));
    }
```

- [ ] **Step 6: Add the response mapping**

In `crates/vmux_mcp/src/protocol.rs`, in `query_result_to_mcp_response`, add an arm before `AgentQueryResult::Error`:

```rust
        AgentQueryResult::Image {
            path,
            png,
            width,
            height,
        } => {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(&png);
            json!({
                "content": [
                    {"type": "text", "text": format!("saved {path} ({width}×{height})")},
                    {"type": "image", "data": data, "mimeType": "image/png"}
                ]
            })
        }
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p vmux_mcp`
Expected: PASS (new tests + existing ones).

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_mcp/Cargo.toml crates/vmux_mcp/src/tools.rs crates/vmux_mcp/src/protocol.rs
git commit -m "feat(mcp): screenshot tool + image result mapping"
```

---

## Task 4: GUI query wiring — `vmux_agent` messages, query arm, forwarder

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (messages near other `#[derive(Message)]` structs; `handle_agent_queries` ~line 958; forwarders ~line 1040; registration ~line 117 and ~line 166)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_agent/src/plugin.rs`:

```rust
    #[test]
    fn screenshot_response_maps_ok_and_err() {
        use vmux_service::protocol::AgentQueryResult;

        let ok = screenshot_response_to_query_result(&Ok(ScreenshotImage {
            path: "/tmp/a.png".into(),
            png: vec![9, 8, 7],
            width: 10,
            height: 20,
        }));
        assert!(matches!(
            ok,
            AgentQueryResult::Image { path, png, width, height }
                if path == "/tmp/a.png" && png == vec![9, 8, 7] && width == 10 && height == 20
        ));

        let err = screenshot_response_to_query_result(&Err("nope".to_string()));
        assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent --lib screenshot_response_maps_ok_and_err`
Expected: FAIL to compile (types/fn missing).

- [ ] **Step 3: Define the messages**

In `crates/vmux_agent/src/plugin.rs`, near the other `#[derive(Message)]` request structs (e.g. just below `AgentQueryRequest`):

```rust
#[derive(Message, Clone)]
pub struct ScreenshotRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
}

#[derive(Clone)]
pub struct ScreenshotImage {
    pub path: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Message, Clone)]
pub struct ScreenshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<ScreenshotImage, String>,
}
```

- [ ] **Step 4: Add the query arm**

In `handle_agent_queries`, add a `MessageWriter` param:

```rust
    mut screenshot_writer: MessageWriter<ScreenshotRequest>,
```

and add the arm (alongside `AgentQuery::ListSpaces`):

```rust
            AgentQuery::Screenshot { pane } => {
                screenshot_writer.write(ScreenshotRequest {
                    request_id: request.request_id.0,
                    pane,
                });
            }
```

- [ ] **Step 5: Add the forwarder + pure helper**

Add near `forward_layout_snapshot_responses`:

```rust
fn screenshot_response_to_query_result(
    result: &Result<ScreenshotImage, String>,
) -> AgentQueryResult {
    match result {
        Ok(img) => AgentQueryResult::Image {
            path: img.path.clone(),
            png: img.png.clone(),
            width: img.width,
            height: img.height,
        },
        Err(message) => AgentQueryResult::Error(message.clone()),
    }
}

fn forward_screenshot_responses(
    mut reader: MessageReader<ScreenshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: screenshot_response_to_query_result(&response.result),
        });
    }
}
```

- [ ] **Step 6: Register messages + system**

In the plugin `build`, with the other `add_message` calls (~line 117):

```rust
            .add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
```

and add `forward_screenshot_responses` to the forwarder system tuple (~line 168, next to `forward_layout_snapshot_responses`):

```rust
                    forward_layout_apply_responses,
                    forward_layout_snapshot_responses,
                    forward_screenshot_responses,
```

- [ ] **Step 7: Run test + build**

Run: `cargo test -p vmux_agent --lib screenshot_response_maps_ok_and_err`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): relay screenshot query to GUI capture"
```

---

## Task 5: `vmux_desktop` pure helpers + module skeleton

**Files:**
- Create: `crates/vmux_desktop/src/screenshot.rs`
- Modify: `crates/vmux_desktop/Cargo.toml`
- Modify: `crates/vmux_desktop/src/lib.rs` (add `mod screenshot;`)

- [ ] **Step 1: Add dependencies**

In `crates/vmux_desktop/Cargo.toml` under `[dependencies]`:

```toml
crossbeam-channel = "0.5"
image = { version = "0.25", default-features = false, features = ["png"] }
```

and under `[target.'cfg(target_os = "macos")'.dependencies]`:

```toml
objc2-screen-capture-kit = { version = "0.3", features = [
    "SCStream",
    "SCShareableContent",
    "SCScreenshotManager",
    "objc2-core-graphics",
    "block2",
] }
```

(`chrono`, `objc2`, `objc2-app-kit`, `objc2-core-graphics`, `objc2-foundation`, `block2`, `raw_window_handle` are already dependencies.)

- [ ] **Step 2: Create the module with pure helpers + failing tests**

Create `crates/vmux_desktop/src/screenshot.rs`:

```rust
pub(crate) const MAX_INLINE_EDGE: u32 = 1568;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Long-edge downscale. Never upscales. Returns at least 1x1.
pub(crate) fn downscale_dims(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
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

/// Build a crop rect from a UI node's center + size (physical px), clamped to the image.
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

/// Encode a downscaled PNG copy. Returns (png_bytes, width, height).
pub(crate) fn encode_downscaled_png(
    img: &image::RgbaImage,
    max_edge: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let (dw, dh) = downscale_dims(img.width(), img.height(), max_edge);
    let dynimg = image::DynamicImage::ImageRgba8(img.clone());
    let scaled = if (dw, dh) == (img.width(), img.height()) {
        dynimg
    } else {
        dynimg.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3)
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    scaled
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("png encode failed: {e}"))?;
    Ok((buf.into_inner(), dw, dh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_never_upscales() {
        assert_eq!(downscale_dims(800, 600, 1568), (800, 600));
        assert_eq!(downscale_dims(0, 0, 1568), (1, 1));
    }

    #[test]
    fn downscale_caps_long_edge() {
        assert_eq!(downscale_dims(3136, 1568, 1568), (1568, 784));
        assert_eq!(downscale_dims(1568, 3136, 1568), (784, 1568));
    }

    #[test]
    fn crop_rect_clamps_to_image() {
        let r = crop_rect_from_node(100.0, 100.0, 80.0, 60.0, 1000, 1000);
        assert_eq!(r, CropRect { x: 60, y: 70, w: 80, h: 60 });

        // overflow clamps width/height.
        let r = crop_rect_from_node(990.0, 990.0, 40.0, 40.0, 1000, 1000);
        assert_eq!(r, CropRect { x: 970, y: 970, w: 30, h: 30 });
    }

    #[test]
    fn encode_downscaled_png_emits_png_header() {
        let img = image::RgbaImage::new(10, 10);
        let (png, w, h) = encode_downscaled_png(&img, 1568).unwrap();
        assert_eq!((w, h), (10, 10));
        assert_eq!(&png[..4], &[137, 80, 78, 71]);
    }
}
```

In `crates/vmux_desktop/src/lib.rs`, add the module declaration alongside the others (~line 8-30):

```rust
mod screenshot;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p vmux_desktop --lib screenshot::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml crates/vmux_desktop/src/screenshot.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): screenshot pure helpers (crop, downscale, encode)"
```

---

## Task 6: `vmux_desktop` bridge resource + start/drain systems + native capture

**Files:**
- Modify: `crates/vmux_desktop/src/screenshot.rs`

This task adds the ECS systems and the macOS ScreenCaptureKit capture. The native capture is not unit-testable headlessly; the pure logic was covered in Task 5. After writing it, the verification step is a real build (and a manual capture in Task 8).

- [ ] **Step 1: Add the bridge resource, ECS imports, and system shells**

At the top of `crates/vmux_desktop/src/screenshot.rs`:

```rust
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use vmux_agent::{ScreenshotImage, ScreenshotRequest, ScreenshotResponse};

/// Type-erased "wake the winit loop" callback, so the capture module never names
/// winit proxy types (and works identically on non-macOS).
pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

#[derive(Resource)]
pub(crate) struct ScreenshotBridge {
    tx: Sender<ScreenshotResponse>,
    rx: Receiver<ScreenshotResponse>,
}

impl Default for ScreenshotBridge {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

fn err_response(request_id: [u8; 16], message: impl Into<String>) -> ScreenshotResponse {
    ScreenshotResponse {
        request_id,
        result: Err(message.into()),
    }
}

const PERMISSION_MSG: &str = "Screen Recording permission required — grant it in System Settings ▸ \
Privacy & Security ▸ Screen Recording, then call screenshot again.";
```

- [ ] **Step 2: Add `resolve_crop` + `start_screenshots`**

```rust
fn resolve_crop(
    id: &str,
    node_q: &Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: &Query<&ChildOf>,
    img_w: u32,
    img_h: u32,
) -> Option<CropRect> {
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

pub(crate) fn start_screenshots(
    mut reader: MessageReader<ScreenshotRequest>,
    bridge: Res<ScreenshotBridge>,
    window_q: Query<(Entity, &Window), With<PrimaryWindow>>,
    node_q: Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: Query<&ChildOf>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    for req in reader.read() {
        let Ok((window_entity, window)) = window_q.single() else {
            let _ = bridge.tx.send(err_response(req.request_id, "no primary vmux window"));
            continue;
        };
        let img_w = window.resolution.physical_width();
        let img_h = window.resolution.physical_height();

        let crop = match &req.pane {
            Some(id) => match resolve_crop(id, &node_q, &child_of_q, img_w, img_h) {
                Some(rect) => Some(rect),
                None => {
                    let _ = bridge
                        .tx
                        .send(err_response(req.request_id, format!("pane not found: {id}")));
                    continue;
                }
            },
            None => None,
        };

        let tx = bridge.tx.clone();
        let wake: Option<WakeFn> = proxy.as_ref().map(|p| {
            let proxy = (***p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as WakeFn
        });
        capture::capture(window_entity, img_w, img_h, crop, req.request_id, tx, wake);
    }
}

pub(crate) fn drain_screenshots(
    bridge: Res<ScreenshotBridge>,
    mut writer: MessageWriter<ScreenshotResponse>,
) {
    while let Ok(response) = bridge.rx.try_recv() {
        writer.write(response);
    }
}
```

Note: `(***p).clone()` unwraps `Res<EventLoopProxyWrapper>` → `EventLoopProxyWrapper` → inner `EventLoopProxy<WinitUserEvent>`. Adjust deref depth to match the build error if needed; the goal is a cloned `EventLoopProxy<WinitUserEvent>`.

- [ ] **Step 3: Add the macOS capture module**

Append to `crates/vmux_desktop/src/screenshot.rs`:

```rust
#[cfg(target_os = "macos")]
mod capture {
    use super::{
        encode_downscaled_png, err_response, CropRect, WakeFn, MAX_INLINE_EDGE, PERMISSION_MSG,
    };
    use bevy::prelude::Entity;
    use crossbeam_channel::Sender;
    use objc2_screen_capture_kit::{
        SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    };
    use vmux_agent::{ScreenshotImage, ScreenshotResponse};

    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    fn finish(tx: &Sender<ScreenshotResponse>, wake: &Option<WakeFn>, response: ScreenshotResponse) {
        let _ = tx.send(response);
        if let Some(w) = wake {
            w();
        }
    }

    /// Resolve the primary window's CGWindowID from its Bevy entity.
    fn window_number(window_entity: Entity) -> Option<u32> {
        use bevy::winit::WINIT_WINDOWS;
        use objc2_app_kit::NSView;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        WINIT_WINDOWS.with_borrow(|winit_windows| {
            let win = winit_windows.get_window(window_entity)?;
            let handle = win.window_handle().ok()?;
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return None;
            };
            let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
            let window = view.window()?;
            Some(window.windowNumber() as u32)
        })
    }

    pub(crate) fn capture(
        window_entity: Entity,
        img_w: u32,
        img_h: u32,
        crop: Option<CropRect>,
        request_id: [u8; 16],
        tx: Sender<ScreenshotResponse>,
        wake: Option<WakeFn>,
    ) {
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            unsafe {
                CGRequestScreenCaptureAccess();
            }
            finish(&tx, &wake, err_response(request_id, PERMISSION_MSG));
            return;
        }
        let Some(window_id) = window_number(window_entity) else {
            finish(&tx, &wake, err_response(request_id, "cannot resolve native window"));
            return;
        };

        let shareable_handler = block2::RcBlock::new(
            move |content: *mut SCShareableContent, _err: *mut objc2_foundation::NSError| {
                if content.is_null() {
                    finish(&tx, &wake, err_response(request_id, "SCShareableContent unavailable"));
                    return;
                }
                let content = unsafe { &*content };
                let windows = unsafe { content.windows() };
                let target = windows
                    .iter()
                    .find(|w| unsafe { w.windowID() } == window_id);
                let Some(window) = target else {
                    finish(&tx, &wake, err_response(request_id, "vmux window not shareable"));
                    return;
                };

                let filter = unsafe {
                    SCContentFilter::initWithDesktopIndependentWindow(
                        SCContentFilter::alloc(),
                        &window,
                    )
                };
                let config = unsafe { SCStreamConfiguration::new() };
                unsafe {
                    config.setWidth(img_w as usize);
                    config.setHeight(img_h as usize);
                }

                let tx2 = tx.clone();
                let wake2 = wake.clone();
                let capture_handler = block2::RcBlock::new(
                    move |image: *mut objc2_core_graphics::CGImage,
                          _err: *mut objc2_foundation::NSError| {
                        if image.is_null() {
                            finish(&tx2, &wake2, err_response(request_id, "capture returned no image"));
                            return;
                        }
                        let response = match cgimage_to_rgba(image) {
                            Ok(rgba) => encode_and_save(rgba, crop, request_id),
                            Err(e) => err_response(request_id, e),
                        };
                        finish(&tx2, &wake2, response);
                    },
                );

                unsafe {
                    SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                        &filter,
                        &config,
                        Some(&capture_handler),
                    );
                }
            },
        );

        unsafe {
            SCShareableContent::getShareableContentWithCompletionHandler(&shareable_handler);
        }
    }

    /// Draw the CGImage into a tightly-packed RGBA8 buffer.
    fn cgimage_to_rgba(image: *mut objc2_core_graphics::CGImage) -> Result<image::RgbaImage, String> {
        use objc2_core_graphics::{
            CGBitmapInfo, CGColorSpace, CGContext, CGImageAlphaInfo,
        };
        let image_ref = unsafe { &*image };
        let width = unsafe { objc2_core_graphics::CGImageGetWidth(Some(image_ref)) } as u32;
        let height = unsafe { objc2_core_graphics::CGImageGetHeight(Some(image_ref)) } as u32;
        if width == 0 || height == 0 {
            return Err("captured image has zero dimension".into());
        }
        let bytes_per_row = (width as usize) * 4;
        let mut buf = vec![0u8; bytes_per_row * height as usize];
        let color_space = unsafe { CGColorSpace::new_device_rgb() }
            .ok_or("failed to create color space")?;
        let bitmap_info = CGImageAlphaInfo::PremultipliedLast.0 | CGBitmapInfo::ByteOrder32Big.0;
        let ctx = unsafe {
            CGContext::new(
                buf.as_mut_ptr() as *mut _,
                width as usize,
                height as usize,
                8,
                bytes_per_row,
                Some(&color_space),
                bitmap_info,
            )
        }
        .ok_or("failed to create bitmap context")?;
        let rect = objc2_core_foundation::CGRect::new(
            objc2_core_foundation::CGPoint::new(0.0, 0.0),
            objc2_core_foundation::CGSize::new(width as f64, height as f64),
        );
        unsafe { ctx.draw_image(rect, Some(image_ref)) };
        image::RgbaImage::from_raw(width, height, buf)
            .ok_or_else(|| "failed to wrap pixel buffer".into())
    }

    fn encode_and_save(
        mut rgba: image::RgbaImage,
        crop: Option<CropRect>,
        request_id: [u8; 16],
    ) -> ScreenshotResponse {
        if let Some(c) = crop {
            rgba = image::imageops::crop_imm(&rgba, c.x, c.y, c.w, c.h).to_image();
        }
        let dir = vmux_core::profile::screenshots_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return err_response(request_id, format!("cannot create {}: {e}", dir.display()));
        }
        let path = dir.join(format!(
            "vmux-{}.png",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        if let Err(e) = rgba.save(&path) {
            return err_response(request_id, format!("cannot save screenshot: {e}"));
        }
        match encode_downscaled_png(&rgba, MAX_INLINE_EDGE) {
            Ok((png, width, height)) => ScreenshotResponse {
                request_id,
                result: Ok(ScreenshotImage {
                    path: path.to_string_lossy().into_owned(),
                    png,
                    width,
                    height,
                }),
            },
            Err(e) => err_response(request_id, e),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod capture {
    use super::{err_response, CropRect, WakeFn};
    use bevy::prelude::Entity;
    use crossbeam_channel::Sender;
    use vmux_agent::ScreenshotResponse;

    pub(crate) fn capture(
        _window_entity: Entity,
        _img_w: u32,
        _img_h: u32,
        _crop: Option<CropRect>,
        request_id: [u8; 16],
        tx: Sender<ScreenshotResponse>,
        _wake: Option<WakeFn>,
    ) {
        let _ = tx.send(err_response(request_id, "screenshots are only supported on macOS"));
    }
}
```

Both `capture` arms share one signature (`Option<WakeFn>`), so the `start_screenshots` call site is platform-agnostic.

- [ ] **Step 4: Build for macOS and fix signature mismatches**

Run: `cargo build -p vmux_desktop 2>&1 | tail -40`
Expected: This is the iteration point. The `objc2-screen-capture-kit` / `objc2-core-graphics` 0.3.2 method names used here (`getShareableContentWithCompletionHandler`, `initWithDesktopIndependentWindow`, `captureImageWithFilter_configuration_completionHandler`, `windowID`, `SCStreamConfiguration::new/setWidth/setHeight`, `CGColorSpace::new_device_rgb`, `CGContext::new`, `CGContext::draw_image`, `CGImageGetWidth/Height`) match the docs but exact `unsafe`/argument/return shapes may need small adjustments. Fix each compile error against `https://docs.rs/objc2-screen-capture-kit/0.3.2` and `https://docs.rs/objc2-core-graphics/0.3.2` until it builds. Do not change the control flow — only the FFI call shapes.

- [ ] **Step 5: Run the pure tests again (regression)**

Run: `cargo test -p vmux_desktop --lib screenshot::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/screenshot.rs
git commit -m "feat(desktop): ScreenCaptureKit window capture + bridge systems"
```

---

## Task 7: Register the bridge resource + systems in the desktop plugin

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs` (`VmuxPlugin::build`, ~line 135)

- [ ] **Step 1: Register**

In `VmuxPlugin::build`, in the cross-platform section (e.g. extend the `app.init_resource::<...>()` chain near line 135):

```rust
        app.init_resource::<screenshot::ScreenshotBridge>()
            .add_systems(
                Update,
                (screenshot::start_screenshots, screenshot::drain_screenshots),
            );
```

(`ScreenshotBridge`, `start_screenshots`, `drain_screenshots` must be `pub(crate)` — they are.)

- [ ] **Step 2: Build the binary**

Run: `cargo build -p vmux_desktop 2>&1 | tail -20`
Expected: builds cleanly.

- [ ] **Step 3: Lint + fmt**

Run: `cargo fmt -p vmux_desktop -p vmux_agent -p vmux_mcp -p vmux_service -p vmux_core && cargo clippy -p vmux_desktop -p vmux_agent -p vmux_mcp -p vmux_service -p vmux_core --all-targets 2>&1 | tail -20`
Expected: no warnings/errors. Fix any.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): register screenshot systems"
```

---

## Task 8: End-to-end manual verification

The capture path needs a real window + Screen Recording permission, so verify manually.

- [ ] **Step 1: Run vmux** (let the user launch, or a bounded self-killing run — do not spawn unbounded `make dev`). Open a browser pane and a terminal pane so the capture has windowed-CEF content to prove fidelity.

- [ ] **Step 2: From a terminal/agent inside vmux, exercise the MCP tool.** Via the vmux MCP server (e.g. the `mcp` smoke client or an agent CLI), call `tools/call` `screenshot` with no args.

- [ ] **Step 3: First-call permission.** If not yet granted, expect an `isError` text response with the permission message and a macOS prompt. Grant Screen Recording, then call `screenshot` again.

- [ ] **Step 4: Verify the result.**
  - The response has a `text` block `saved ~/.vmux/screenshots/vmux-<ts>.png (W×H)` and an `image` block (`mimeType: image/png`).
  - `ls ~/.vmux/screenshots/` shows the full-res PNG; open it and confirm the **browser pane content is present** (not a blank hole) — this proves OS-level capture fidelity.
  - Inline image dimensions have long edge ≤ 1568.

- [ ] **Step 5: Verify pane crop.** `read_layout` to get a `stack:<id>` or `pane:<id>`, then `screenshot` with `{ "pane": "<id>" }`. Confirm the saved image is cropped to that pane's region.

- [ ] **Step 6: Verify focus is retained.** After capturing, confirm the focused terminal still accepts keystrokes (programmatic SCK capture must not steal input, unlike the interactive screenshot tools).

- [ ] **Step 7: Commit any fixes**, then delete this plan file (per AGENTS.md) once fully implemented:

```bash
git rm docs/plans/2026-06-22-screenshot-mcp-tool.md
git commit -m "chore: remove implemented screenshot plan"
```

---

## Notes / Risks

- **Native FFI shapes (Task 6 Step 4)** are the only uncertain part; the control flow and pure logic are settled. Iterate against docs.rs, don't restructure.
- **5s query timeout:** SCK on a granted system completes in well under 5s; the off-thread `WakeUp` ensures `drain_screenshots` flushes the response promptly. Permission-not-granted returns immediately (no blocking on the prompt).
- **rkyv frame size:** only the downscaled PNG (≤1568px, ~<1 MB) crosses the socket; full-res stays on disk.
- **macOS only:** non-macOS returns a clear error rather than hanging.
- **`WakeFn` bound:** `Arc<dyn Fn() + Send + Sync>` requires the cloned `EventLoopProxy<WinitUserEvent>` closure to be `Send + Sync` (winit's proxy is). If `Sync` is missing on the target winit, drop to cloning the proxy into each completion block directly (concrete type) instead of sharing one `Arc`.
- **SCK completion threads:** completion handlers run on a background queue, so everything captured by the blocks (`tx`, `wake`, `request_id`, dims, `crop`) must be `Send` — they are. Encoding/saving happens on that queue, off the main thread.
