# Rkyv Binary IPC for cef↔bevy Bridge — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new binary IPC channel (`bin-host-emit` / `bin-js-emit`) to bevy_cef_core, parallel to the existing JSON `host-emit` / `js-emit` channels, and migrate vmux event types onto it using rkyv as the codec.

**Architecture:** **Additive, not replace.** Fork bevy_cef_core into `patches/` and add new V8 natives (`cef.binEmit` / `cef.binListen`), a new `ProcessMessageHandler`, and a parallel `BinJsEmitEventPlugin<E>` / `BinHostEmitEvent` Bevy plugin. The existing JSON channels stay bit-identical to upstream — zero risk to current consumers, BRP untouched, easy upstream PR, per-event rollback. vmux migrates events one at a time onto the new channel.

**Tech Stack:** Rust 2024, Bevy 0.18, bevy_cef 0.5.2 / bevy_cef_core 0.5.2 (vendored), CEF 145.x via `cef` crate, rkyv 0.8, dioxus wasm.

**Linear:** [VMX-106](https://linear.app/vmux/issue/VMX-106/fork-bevy-cef-core-for-rkyv-binary-ipc-channel)

**Pre-commit per AGENTS.md:** `make lint && make test` before every push. No `--no-verify`.

---

## Naming Convention

- **Channel name:** "binary" / `bin` (the channel transports raw bytes; the codec is vmux's choice)
- **Process message names:** `"bin-host-emit"`, `"bin-js-emit"` (parallel to `"host-emit"`, `"js-emit"`)
- **V8 natives:** `__cef_bin_emit`, `__cef_bin_listen` → exposed as `cef.binEmit`, `cef.binListen`
- **Bevy types:** `BinHostEmitEvent`, `BinJsEmitEventPlugin<E>`, `BinReceive<E>`, `BinIpcEventRaw`
- **vmux helpers:** `try_cef_bin_emit_rkyv`, `decode_bin_host_emit_js`

---

## File Structure

**New (vendored fork of bevy_cef_core 0.5.2):**
- `patches/bevy_cef_core-0.5.2/` (full crate copy)
- `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler/bin_emit_event_handler.rs` — new `ProcessMessageHandler` for `bin-js-emit`

**Modified — vendored bevy_cef_core (additive only):**
- `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler.rs` — `pub use bin_emit_event_handler::*;`
- `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs` — add `emit_event_bytes()` method, register `BinEmitEventHandler` on browser creation
- `patches/bevy_cef_core-0.5.2/src/browser_process/cef_thread.rs` — mirror bytes path
- `patches/bevy_cef_core-0.5.2/src/browser_process/cef_command.rs` — `CefCommand::EmitEventBytes` variant
- `patches/bevy_cef_core-0.5.2/src/render_process/cef_api_handler.rs` — add `execute_bin_emit` + `execute_bin_listen` natives
- `patches/bevy_cef_core-0.5.2/src/render_process/render_process_handler.rs` — extend `CEF_API_EXTENSION_CODE` with `cef.binEmit` / `cef.binListen`, dispatch new process messages
- `patches/bevy_cef_core-0.5.2/src/util.rs` (or wherever the constants live) — add `PROCESS_MESSAGE_BIN_HOST_EMIT`, `PROCESS_MESSAGE_BIN_JS_EMIT`

**New — bevy_cef patch (additive only):**
- `patches/bevy_cef-0.5.2/src/common/ipc/bin_host_emit.rs` — `BinHostEmitEvent` parallel to `HostEmitEvent`
- `patches/bevy_cef-0.5.2/src/common/ipc/bin_js_emit.rs` — `BinJsEmitEventPlugin<E>`, `BinReceive<E>`, `BinIpcEventRawBuffer`

**Modified — bevy_cef patch (minimal additive edits):**
- `patches/bevy_cef-0.5.2/Cargo.toml` — point `bevy_cef_core` dep to vendored path
- `patches/bevy_cef-0.5.2/src/common/ipc.rs` (or wherever the module is declared) — `pub mod bin_host_emit; pub mod bin_js_emit;`
- `patches/bevy_cef-0.5.2/src/lib.rs` (or `prelude.rs`) — export new types from prelude

**Modified — workspace:**
- `Cargo.toml` — add `rkyv` workspace dep, point `bevy_cef`/`bevy_cef_core` to vendored paths

**Modified — vmux event types (add `#[derive(Archive, RkyvSerialize, RkyvDeserialize)]` ONLY for events being migrated):**
- See migration list in Phase 6

**Modified — vmux Bevy→JS callsites (one per event migration):**
- See migration list in Phase 6

**New — vmux wasm helpers (additive next to existing):**
- `crates/vmux_ui/src/hooks/event_listener.rs` — add `try_cef_bin_emit_rkyv`, `decode_bin_host_emit_js`, `use_bin_event_listener` (don't touch the serde versions)

**Untouched:**
- BRP plumbing
- Existing `host-emit` / `js-emit` JSON channels in bevy_cef_core
- Existing `HostEmitEvent`, `JsEmitEventPlugin<E>`, `Receive<E>` in bevy_cef
- vmux events that don't get migrated yet (they keep using the JSON path)
- On-disk persistence (`settings.ron`, `sessions.ron`)

---

## Phase 1: Vendor bevy_cef_core 0.5.2

### Task 1.1: Extract bevy_cef_core 0.5.2 source into `patches/`

**Files:**
- Create: `patches/bevy_cef_core-0.5.2/` (full crate)

- [ ] **Step 1: Download and extract the .crate**

```bash
mkdir -p /tmp/bcc-extract
curl -L "https://crates.io/api/v1/crates/bevy_cef_core/0.5.2/download" -o /tmp/bevy_cef_core-0.5.2.crate
tar -xzf /tmp/bevy_cef_core-0.5.2.crate -C /tmp/bcc-extract
mkdir -p patches/bevy_cef_core-0.5.2
cp -R /tmp/bcc-extract/bevy_cef_core-0.5.2/. patches/bevy_cef_core-0.5.2/
```

- [ ] **Step 2: Restore the original Cargo.toml from `Cargo.toml.orig`**

```bash
if [ -f patches/bevy_cef_core-0.5.2/Cargo.toml.orig ]; then
  cp patches/bevy_cef_core-0.5.2/Cargo.toml.orig patches/bevy_cef_core-0.5.2/Cargo.toml
fi
```

- [ ] **Step 3: Confirm structure**

```bash
ls patches/bevy_cef_core-0.5.2/src/browser_process/client_handler/
ls patches/bevy_cef_core-0.5.2/src/render_process/
```

Expected: directory contains `js_emit_event_handler.rs`, `cef_api_handler.rs`, `render_process_handler.rs`. **If layout differs from 0.8.1 (which I used as reference during planning), open each modified file before editing to confirm actual structure.**

- [ ] **Step 4: Wire as path dependency**

Edit `Cargo.toml` (workspace root) — replace the registry deps:
```toml
[workspace.dependencies]
bevy_cef_core = { path = "patches/bevy_cef_core-0.5.2" }
bevy_cef = { path = "patches/bevy_cef-0.5.2" }
```

Update `patches/bevy_cef-0.5.2/Cargo.toml` `[dependencies.bevy_cef_core]`:
```toml
[dependencies.bevy_cef_core]
path = "../bevy_cef_core-0.5.2"
version = "0.5.2"
```

- [ ] **Step 5: Build clean**

```bash
env -u CEF_PATH cargo build -p bevy_cef_core -p bevy_cef 2>&1 | tail -20
```

Expected: clean build, identical behavior to crates.io version.

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef_core-0.5.2 Cargo.toml patches/bevy_cef-0.5.2/Cargo.toml Cargo.lock
git commit -m "chore: vendor bevy_cef_core 0.5.2 into patches/"
```

---

## Phase 2: Add rkyv as workspace dependency

### Task 2.1: Wire rkyv into the workspace and prove a round-trip on one event type

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/vmux_ui/Cargo.toml`, `crates/vmux_ui/src/theme.rs`

- [ ] **Step 1: Add rkyv to workspace deps**

In `Cargo.toml` (workspace root) `[workspace.dependencies]`:
```toml
rkyv = { version = "0.8", default-features = false, features = ["alloc", "bytecheck", "pointer_width_32"] }
```

`pointer_width_32` is required so archives encoded on the 64-bit Bevy host can be read on the 32-bit wasm target.

- [ ] **Step 2: Add rkyv derives to `ThemeEvent` and a round-trip test**

In `crates/vmux_ui/Cargo.toml`:
```toml
rkyv = { workspace = true }
```

In `crates/vmux_ui/src/theme.rs`:
```rust
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Archive, RkyvSerialize, RkyvDeserialize, /* keep existing derives */)]
pub struct ThemeEvent { /* ... */ }

#[cfg(test)]
mod tests {
    use super::*;
    use rkyv::rancor::Error;

    #[test]
    fn theme_event_rkyv_roundtrip() {
        let original = ThemeEvent { radius: 8.0 };
        let bytes = rkyv::to_bytes::<Error>(&original).expect("serialize");
        let recovered = rkyv::from_bytes::<ThemeEvent, Error>(&bytes).expect("deserialize");
        assert_eq!(original.radius, recovered.radius);
    }
}
```

- [ ] **Step 3: Run the test**

```bash
env -u CEF_PATH cargo test -p vmux_ui theme_event_rkyv_roundtrip
```

Expected: PASS. If `ThemeEvent` has fields rkyv can't archive, surface them now and decide on representation.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/vmux_ui Cargo.lock
git commit -m "feat: add rkyv workspace dep + ThemeEvent codec round-trip"
```

---

## Phase 3: Add binary IPC channel to bevy_cef_core (additive)

### Task 3.1: Add new process message constants

**Files:**
- Modify: wherever `PROCESS_MESSAGE_HOST_EMIT` is defined (likely `src/util.rs` or `src/render_process/render_process_handler.rs`)

- [ ] **Step 1: Locate the existing constants**

```bash
rg 'pub const PROCESS_MESSAGE_' patches/bevy_cef_core-0.5.2/src -n
```

- [ ] **Step 2: Add binary equivalents next to the JSON ones**

```rust
pub const PROCESS_MESSAGE_BIN_HOST_EMIT: &str = "bin-host-emit";
pub const PROCESS_MESSAGE_BIN_JS_EMIT: &str = "bin-js-emit";
```

- [ ] **Step 3: Re-export from the prelude/lib if the existing ones are exported there**

```bash
rg 'PROCESS_MESSAGE_HOST_EMIT' patches/bevy_cef_core-0.5.2/src/lib.rs patches/bevy_cef_core-0.5.2/src/prelude.rs 2>/dev/null
```

Mirror the export pattern.

- [ ] **Step 4: Compile**

```bash
env -u CEF_PATH cargo build -p bevy_cef_core 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src
git commit -m "feat(bevy_cef_core): add bin-host-emit / bin-js-emit process message names"
```

### Task 3.2: Browser-process — add `BinEmitEventHandler` and `emit_event_bytes`

**Files:**
- Create: `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler/bin_emit_event_handler.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/client_handler.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/cef_thread.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/cef_command.rs`

- [ ] **Step 1: Create `bin_emit_event_handler.rs`**

```rust
use crate::browser_process::client_handler::ProcessMessageHandler;
use crate::prelude::PROCESS_MESSAGE_BIN_JS_EMIT;
use async_channel::Sender;
use bevy::prelude::Entity;
use cef::{Browser, Frame, ImplBinaryValue, ImplListValue, ListValue};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinIpcEventRaw {
    pub webview: Entity,
    pub payload: Vec<u8>,
}

pub struct BinEmitEventHandler {
    webview: Entity,
    sender: Sender<BinIpcEventRaw>,
}

impl BinEmitEventHandler {
    pub const fn new(webview: Entity, sender: Sender<BinIpcEventRaw>) -> Self {
        Self { sender, webview }
    }
}

impl ProcessMessageHandler for BinEmitEventHandler {
    fn process_name(&self) -> &'static str {
        PROCESS_MESSAGE_BIN_JS_EMIT
    }

    fn handle_message(&self, _browser: &mut Browser, _frame: &mut Frame, args: Option<ListValue>) {
        if let Some(args) = args
            && let Some(binary) = args.binary(0)
        {
            let len = binary.size();
            // Pre-size with zeros, NOT with_capacity — cef's `data()` writes into `buf[0..buf.len()]`,
            // so a zero-length buffer would silently capture nothing.
            let mut payload = vec![0u8; len];
            binary.data(Some(&mut payload), 0);
            let _ = self.sender.send_blocking(BinIpcEventRaw {
                webview: self.webview,
                payload,
            });
        }
    }
}
```

- [ ] **Step 2: Re-export from `client_handler.rs`**

Add `mod bin_emit_event_handler;` and `pub use bin_emit_event_handler::{BinEmitEventHandler, BinIpcEventRaw};`.

- [ ] **Step 3: Add `emit_event_bytes` on `Browsers`**

In `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`, immediately after the existing `emit_event`:

```rust
pub fn emit_event_bytes(&self, webview: &Entity, id: impl Into<String>, payload: &[u8]) {
    if let Some(mut process_message) =
        process_message_create(Some(&PROCESS_MESSAGE_BIN_HOST_EMIT.into()))
        && let Some(argument_list) = process_message.argument_list()
        && let Some(mut binary) = binary_value_create(Some(payload))
        && let Some((browser, _)) = self.0.get(webview)
        && let Some(frame) = browser.main_frame()
    {
        argument_list.set_string(0, Some(&id.into().as_str().into()));
        argument_list.set_binary(1, Some(&mut binary));
        frame.send_process_message(
            ProcessId::from(cef_process_id_t::PID_RENDERER),
            Some(&mut process_message),
        );
    }
}
```

Add imports: `use cef::{binary_value_create, ImplListValue};` and `use crate::prelude::PROCESS_MESSAGE_BIN_HOST_EMIT;`. Match the exact `if let` chain shape used by the existing `emit_event` function.

- [ ] **Step 4: Register `BinEmitEventHandler` in browser creation**

Find the line `.with_message_handler(JsEmitEventHandler::new(webview, ipc_event_sender))` in `browsers.rs`. Add immediately after it (and again in the parallel block in `cef_thread.rs`):

```rust
.with_message_handler(BinEmitEventHandler::new(webview, bin_ipc_event_sender))
```

This requires plumbing a new `bin_ipc_event_sender: async_channel::Sender<BinIpcEventRaw>` to wherever browsers are created. Mirror the existing `ipc_event_sender` plumbing — likely a `NonSend` resource or constructor argument.

- [ ] **Step 5: Add `CefCommand::EmitEventBytes` and the public dispatch method**

In `cef_command.rs`, find the existing `CefCommand::EmitEvent` variant. Add:

```rust
EmitEventBytes {
    webview: Entity,
    id: String,
    payload: Vec<u8>,
},
```

Add a dispatch method (next to the existing `emit_event`):
```rust
pub fn emit_event_bytes(&self, webview: &Entity, id: impl Into<String>, payload: Vec<u8>) {
    let _ = self.tx.send_blocking(CefCommand::EmitEventBytes {
        webview: *webview,
        id: id.into(),
        payload,
    });
}
```

In `cef_thread.rs`, add the dispatch arm next to `CefCommand::EmitEvent`:
```rust
CefCommand::EmitEventBytes { webview, id, payload } => {
    self.emit_event_bytes(&webview, id, &payload);
}
```

And add the corresponding `fn emit_event_bytes(&self, ...)` private method on `CefThread`, mirroring the new public `emit_event_bytes` on `Browsers`.

- [ ] **Step 6: Compile**

```bash
env -u CEF_PATH cargo build -p bevy_cef_core 2>&1 | tail -20
```

- [ ] **Step 7: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/browser_process
git commit -m "feat(bevy_cef_core): add BinEmitEventHandler + emit_event_bytes (browser process)"
```

### Task 3.3: Render-process — add `__cef_bin_emit` and `__cef_bin_listen` natives

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/render_process/cef_api_handler.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/render_process/render_process_handler.rs`

**Design decision (single shared listener registration):** there is no separate `__cef_bin_listen` native. `cef.binListen` is wired to the same `__cef_listen` native as `cef.listen`. Listener registration is just `(id, callback)` — the dispatch layer in `handle_listen_message` decides whether to give the callback a parsed JS object (when the inbound process message is `host-emit`) or an `ArrayBuffer` (when it's `bin-host-emit`). This keeps the V8 native count at 4 and means the JS side just picks which `emit` native to call based on its payload format.

- [ ] **Step 1: Extend `CEF_API_EXTENSION_CODE` with the new emit native**

In `render_process_handler.rs`, find the `CEF_API_EXTENSION_CODE` const. The current shape is:
```js
var cef;
if (!cef) cef = {};
(function() {
  native function __cef_brp();
  native function __cef_emit();
  native function __cef_listen();
  cef.brp = __cef_brp;
  cef.emit = __cef_emit;
  cef.listen = __cef_listen;
})();
```

Replace with:
```js
var cef;
if (!cef) cef = {};
(function() {
  native function __cef_brp();
  native function __cef_emit();
  native function __cef_listen();
  native function __cef_bin_emit();
  cef.brp = __cef_brp;
  cef.emit = __cef_emit;
  cef.listen = __cef_listen;
  cef.binEmit = __cef_bin_emit;
  cef.binListen = __cef_listen;  // shares registration with cef.listen
})();
```

- [ ] **Step 2: Wire `__cef_bin_emit` in `cef_api_handler.rs`**

In the `execute()` match in `CefApiHandler`, add one case:
```rust
"__cef_bin_emit" => self.execute_bin_emit(arguments),
```

Add the new method (mirror `execute_emit` shape):

```rust
fn execute_bin_emit(&self, arguments: Option<&[Option<V8Value>]>) -> c_int {
    let Some(context) = v8_context_get_current_context() else { return 0; };
    let Some(frame) = context.frame() else { return 0; };

    if let Some(mut process) = process_message_create(Some(&PROCESS_MESSAGE_BIN_JS_EMIT.into()))
        && let Some(arguments_list) = process.argument_list()
        && let Some(arguments) = arguments
        && let Some(Some(arg)) = arguments.first()
        && arg.is_array_buffer() != 0
    {
        let len = arg.array_buffer_byte_length();
        let data_ptr = arg.array_buffer_data();
        if data_ptr.is_null() || len == 0 {
            return 1;
        }
        // SAFETY: V8 guarantees the buffer is valid for `len` bytes during this call.
        let bytes = unsafe { std::slice::from_raw_parts(data_ptr.cast::<u8>(), len).to_vec() };

        if let Some(mut binary) = binary_value_create(Some(&bytes)) {
            arguments_list.set_binary(0, Some(&mut binary));
            frame.send_process_message(
                ProcessId::from(cef_process_id_t::PID_BROWSER),
                Some(&mut process),
            );
        }
    }
    1
}
```

- [ ] **Step 3: Update `handle_listen_message` to handle the new process message name**

In `render_process_handler.rs`, find where `PROCESS_MESSAGE_HOST_EMIT` is matched. Today:
```rust
PROCESS_MESSAGE_HOST_EMIT => {
    handle_listen_message(message, browser, frame, ctx);
}
```

Add a parallel arm:
```rust
PROCESS_MESSAGE_BIN_HOST_EMIT => {
    handle_bin_listen_message(message, browser, frame, ctx);
}
```

And implement `handle_bin_listen_message` next to `handle_listen_message`:
```rust
fn handle_bin_listen_message(
    message: &ProcessMessage,
    browser: &mut Browser,
    frame: &mut Frame,
    mut ctx: V8Context,
) {
    let Some(argument_list) = message.argument_list() else { return; };
    let id = argument_list.string(0).into_string();
    let Some(binary) = argument_list.binary(1) else { return; };
    let len = binary.size();
    // Pre-size with zeros — cef's `data()` writes into `buf[0..buf.len()]`.
    let mut buffer = vec![0u8; len];
    binary.data(Some(&mut buffer), 0);

    let key = context_key(browser, frame);
    let callback = LISTEN_EVENTS
        .lock()
        .ok()
        .and_then(|events| events.get(&key)?.get(&id).cloned());
    let Some(callback) = callback else { return; };

    if ctx.enter() != 0 {
        let Some(array_buffer) =
            v8_value_create_array_buffer_with_copy(buffer.as_mut_ptr(), buffer.len())
        else {
            ctx.exit();
            return;
        };
        let mut obj = v8_value_create_object(
            Some(&mut V8DefaultAccessorBuilder::build()),
            Some(&mut V8DefaultInterceptorBuilder::build()),
        );
        callback.execute_function_with_context(
            Some(&mut ctx),
            obj.as_mut(),
            Some(&[Some(array_buffer)]),
        );
        ctx.exit();
    }
}
```

Add `v8_value_create_array_buffer_with_copy` to the imports.

- [ ] **Step 4: Compile**

```bash
env -u CEF_PATH cargo build -p bevy_cef_core 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/render_process
git commit -m "feat(bevy_cef_core): add cef.binEmit native + bin host_emit dispatch"
```

---

## Phase 4: New `BinHostEmitEvent` / `BinJsEmitEventPlugin` in bevy_cef patch

### Task 4.1: Add `bin_host_emit.rs`

**Files:**
- Create: `patches/bevy_cef-0.5.2/src/common/ipc/bin_host_emit.rs`
- Modify: `patches/bevy_cef-0.5.2/src/common/ipc.rs` (or wherever the module is declared)
- Modify: `patches/bevy_cef-0.5.2/Cargo.toml` (add rkyv dep if needed for the helper constructor)

- [ ] **Step 1: Add rkyv to bevy_cef patch deps**

In `patches/bevy_cef-0.5.2/Cargo.toml`:
```toml
[dependencies.rkyv]
version = "0.8"
default-features = false
features = ["alloc", "bytecheck", "pointer_width_32"]
```

- [ ] **Step 2: Create `bin_host_emit.rs`**

```rust
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;

#[derive(Reflect, Debug, Clone, EntityEvent)]
#[reflect(opaque)]
pub struct BinHostEmitEvent {
    #[event_target]
    pub webview: Entity,
    pub id: String,
    pub payload: Vec<u8>,
}

impl BinHostEmitEvent {
    pub fn from_bytes(webview: Entity, id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self { webview, id: id.into(), payload }
    }

    pub fn from_rkyv<T>(webview: Entity, id: impl Into<String>, value: &T) -> Self
    where
        T: for<'a> rkyv::Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>>,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .map(|b| b.into_vec())
            .unwrap_or_default();
        Self::from_bytes(webview, id, bytes)
    }
}

pub(super) struct BinHostEmitPlugin;

impl Plugin for BinHostEmitPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<BinHostEmitEvent>().add_observer(bin_host_emit);
    }
}

fn bin_host_emit(trigger: On<BinHostEmitEvent>, browsers: NonSend<Browsers>) {
    webview_debug_log(format!(
        "bin_host_emit entity={:?} id={} payload_len={}",
        trigger.webview,
        trigger.id,
        trigger.payload.len()
    ));
    browsers.emit_event_bytes(&trigger.webview, trigger.id.clone(), &trigger.payload);
}
```

- [ ] **Step 3: Wire the module**

In `patches/bevy_cef-0.5.2/src/common/ipc.rs`:
```rust
pub mod bin_host_emit;
pub use bin_host_emit::{BinHostEmitEvent, BinHostEmitPlugin};
```

In `patches/bevy_cef-0.5.2/src/common/ipc.rs` `IpcPlugin::build` (or wherever the existing `HostEmitPlugin` is added), add `.add_plugins(BinHostEmitPlugin)`.

- [ ] **Step 4: Add to prelude**

`pub use crate::common::ipc::{BinHostEmitEvent};` (mirror how `HostEmitEvent` is re-exported).

- [ ] **Step 5: Compile**

```bash
env -u CEF_PATH cargo build -p bevy_cef 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef-0.5.2
git commit -m "feat(bevy_cef): add BinHostEmitEvent + plugin for binary outbound channel"
```

### Task 4.2: Add `bin_js_emit.rs`

**Files:**
- Create: `patches/bevy_cef-0.5.2/src/common/ipc/bin_js_emit.rs`
- Modify: `patches/bevy_cef-0.5.2/src/common/ipc.rs`

- [ ] **Step 1: Create `bin_js_emit.rs`**

```rust
use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

#[derive(Resource, Default)]
pub struct BinIpcEventRawBuffer(pub Vec<BinIpcEventRaw>);

fn drain_bin_ipc_events(
    receiver: ResMut<BinIpcEventRawReceiver>,
    mut buffer: ResMut<BinIpcEventRawBuffer>,
) {
    buffer.0.clear();
    while let Ok(event) = receiver.0.try_recv() {
        buffer.0.push(event);
    }
}

#[derive(Debug, EntityEvent)]
pub struct BinReceive<M: Sync + Send + 'static> {
    #[event_target]
    pub webview: Entity,
    pub payload: M,
}

impl<M> Deref for BinReceive<M>
where M: Sync + Send + 'static {
    type Target = M;
    fn deref(&self) -> &Self::Target { &self.payload }
}

impl<M> DerefMut for BinReceive<M>
where M: Sync + Send + 'static {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.payload }
}

pub struct BinJsEmitEventPlugin<E>(PhantomData<E>);

impl<E> Plugin for BinJsEmitEventPlugin<E>
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        app.add_systems(Update, receive_bin_events::<E>.after(drain_bin_ipc_events));
    }
}

impl<E> Default for BinJsEmitEventPlugin<E> {
    fn default() -> Self { Self(PhantomData) }
}

fn receive_bin_events<E>(mut commands: Commands, buffer: Res<BinIpcEventRawBuffer>)
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
{
    for event in &buffer.0 {
        if let Ok(payload) = rkyv::from_bytes::<E, rkyv::rancor::Error>(&event.payload) {
            commands.trigger(BinReceive {
                webview: event.webview,
                payload,
            });
        }
    }
}

pub(crate) struct BinIpcRawEventPlugin;

impl Plugin for BinIpcRawEventPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(BinIpcEventRawSender(tx))
            .insert_resource(BinIpcEventRawReceiver(rx))
            .init_resource::<BinIpcEventRawBuffer>()
            .add_systems(Update, drain_bin_ipc_events);
    }
}

#[derive(Resource)]
pub(crate) struct BinIpcEventRawSender(pub Sender<BinIpcEventRaw>);

#[derive(Resource)]
pub(crate) struct BinIpcEventRawReceiver(pub Receiver<BinIpcEventRaw>);
```

- [ ] **Step 2: Wire the module and plugin**

In `patches/bevy_cef-0.5.2/src/common/ipc.rs`:
```rust
pub mod bin_js_emit;
pub use bin_js_emit::{BinJsEmitEventPlugin, BinReceive, BinIpcEventRawBuffer};
pub(crate) use bin_js_emit::{BinIpcRawEventPlugin, BinIpcEventRawSender};
```

Add `.add_plugins(BinIpcRawEventPlugin)` to `IpcPlugin::build`.

- [ ] **Step 3: Plumb the sender to browser creation**

The existing `JsEmitEventHandler` is constructed with an `IpcEventRawSender` extracted from a Bevy resource. Mirror this for `BinEmitEventHandler` — pass `BinIpcEventRawSender` through wherever browsers are created.

```bash
rg 'IpcEventRawSender' patches/bevy_cef-0.5.2/src patches/bevy_cef_core-0.5.2/src -n
```

For each callsite that pulls `IpcEventRawSender` from the world, also pull `BinIpcEventRawSender` and pass it to `BinEmitEventHandler::new`.

- [ ] **Step 4: Compile**

```bash
env -u CEF_PATH cargo build -p bevy_cef -p bevy_cef_core 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add patches/bevy_cef-0.5.2 patches/bevy_cef_core-0.5.2
git commit -m "feat(bevy_cef): add BinJsEmitEventPlugin + BinReceive for binary inbound channel"
```

---

## Phase 5: vmux wasm bridge — add `bin` helpers (additive)

### Task 5.1: Add `try_cef_bin_emit_rkyv` and `decode_bin_host_emit_js`

**Files:**
- Modify: `crates/vmux_ui/src/hooks/event_listener.rs`
- Modify: `crates/vmux_ui/Cargo.toml` (already has rkyv from Phase 2)

- [ ] **Step 1: Add `try_cef_bin_emit_rkyv`**

In `crates/vmux_ui/src/hooks/event_listener.rs`, next to `try_cef_emit_serde`:

```rust
fn cef_bin_emit_fn(cef: &JsValue) -> Result<Function, EventListenerError> {
    let Ok(emit) = js_sys::Reflect::get(cef, &JsValue::from_str("binEmit")) else {
        return Err(EventListenerError::NoEmitMethod);
    };
    emit.dyn_into::<Function>().map_err(|_| EventListenerError::EmitNotCallable)
}

#[allow(dead_code)]
pub fn try_cef_bin_emit_rkyv<T>(payload: &T) -> Result<(), EventListenerError>
where
    T: for<'a> rkyv::Serialize<rkyv::api::high::HighSerializer<rkyv::util::AlignedVec, rkyv::ser::allocator::ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    use js_sys::{ArrayBuffer, Uint8Array};

    let cef = window_cef()?;
    let emit_fn = cef_bin_emit_fn(&cef)?;

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload)
        .map_err(|_| EventListenerError::SerializePayload)?;

    let buffer = ArrayBuffer::new(bytes.len() as u32);
    let view = Uint8Array::new(&buffer);
    view.copy_from(&bytes);

    let _ = emit_fn.call1(&cef, &buffer.into());
    Ok(())
}
```

- [ ] **Step 2: Add `decode_bin_host_emit_js`**

```rust
pub fn decode_bin_host_emit_js<T>(e: &JsValue) -> Option<T>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
{
    use js_sys::{ArrayBuffer, Uint8Array};
    use wasm_bindgen::JsCast;

    let buffer: ArrayBuffer = if let Some(buf) = e.dyn_ref::<ArrayBuffer>() {
        buf.clone()
    } else if let Some(arr) = e.dyn_ref::<Uint8Array>() {
        arr.buffer()
    } else {
        return None;
    };

    let view = Uint8Array::new(&buffer);
    let mut bytes = vec![0u8; view.length() as usize];
    view.copy_to(&mut bytes);

    rkyv::from_bytes::<T, rkyv::rancor::Error>(&bytes).ok()
}
```

- [ ] **Step 3: Add `try_cef_bin_listen` and `use_bin_event_listener`**

Mirror the existing `try_cef_listen` and `use_event_listener` shape, but call `decode_bin_host_emit_js` for payload decoding. Use `cef.binListen` (which is wired to the same V8 native as `cef.listen` per the decision in Task 3.3 — registration is shared).

- [ ] **Step 4: Build a wasm crate to verify**

```bash
env -u CEF_PATH cargo build -p vmux_command --target wasm32-unknown-unknown 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_ui
git commit -m "feat(vmux_ui): add bin emit/listen wasm helpers (rkyv ArrayBuffer)"
```

---

## Phase 6: Migrate event types onto the binary channel (one per task)

For each event group, the migration shape is identical:

1. Add `#[derive(Archive, RkyvSerialize, RkyvDeserialize)]` to the event type
2. Add `rkyv = { workspace = true }` to the owning crate's Cargo.toml if not already there
3. Add a unit round-trip test
4. **Bevy→JS event:** swap `HostEmitEvent` → `BinHostEmitEvent`, `ron::ser::to_string` + `HostEmitEvent::new(.., &body)` → `BinHostEmitEvent::from_rkyv(.., &payload)`
5. **JS→Bevy event:** swap `JsEmitEventPlugin<E>` → `BinJsEmitEventPlugin<E>`, `On<Receive<E>>` → `On<BinReceive<E>>`, `try_cef_emit_serde` → `try_cef_bin_emit_rkyv`, `use_event_listener` callsites that consume this event use `use_bin_event_listener`
6. Compile, test, commit

**Strict rule:** if an event has both directions (e.g., terminal sends and receives), migrate both in the same task. Mixing channels for one logical event is a footgun.

### Task 6.1: Migrate `ThemeEvent` (Bevy→JS only)

- [ ] Swap callsite in `crates/vmux_desktop/src/browser.rs:120`:

Before:
```rust
let body = ron::ser::to_string(&payload).unwrap_or_default();
commands.trigger(HostEmitEvent::new(entity, THEME_EVENT, &body));
```

After:
```rust
commands.trigger(BinHostEmitEvent::from_rkyv(entity, THEME_EVENT, &payload));
```

- [ ] Update wasm-side listener (find via `rg 'THEME_EVENT|theme_event' crates/vmux_ui/src crates/vmux_command crates/vmux_layout`) — switch from `use_event_listener` to `use_bin_event_listener`.
- [ ] Compile, run app, verify theme propagates.
- [ ] Commit: `feat(vmux): migrate ThemeEvent to binary IPC channel`

### Task 6.2: Migrate command bar events

Events: `CommandBarReadyEvent`, `CommandBarRenderedEvent`, `CommandBarActionEvent`, `PathCompleteRequest`.

Files affected:
- `crates/vmux_command_bar/src/event.rs` — derives + tests
- `crates/vmux_command/src/app.rs` — wasm emit callsites
- `crates/vmux_desktop/src/command_bar.rs:71-74` — plugin registration; `:651, :1487` — outbound emits; `:265, :271, :770, :1472` — inbound receivers
- Commit: `feat(vmux): migrate command bar events to binary IPC channel`

### Task 6.3: Migrate session events

Events: `SessionCommandEvent`.

Files affected:
- `crates/vmux_sessions/src/event.rs`
- `crates/vmux_session/src/app.rs`
- `crates/vmux_desktop/src/sessions.rs:118, :256, :365`
- `crates/vmux_desktop/src/command_bar.rs:2161` (consumes SessionCommandEvent)
- Commit: `feat(vmux): migrate session events to binary IPC channel`

### Task 6.4: Migrate layout events

Events: `HeaderCommandEvent`, `FooterCommandEvent`, `SideSheetCommandEvent`.

Files affected:
- `crates/vmux_layout/src/event.rs`
- `crates/vmux_layout/src/app.rs:197, :288, :344, :359, :382, :478, :497`
- `crates/vmux_desktop/src/browser.rs:59-60, :758, :867, :965, :1005`
- `crates/vmux_layout/src/space.rs:32, :387, :396`
- Commit: `feat(vmux): migrate layout events to binary IPC channel`

### Task 6.5: Migrate terminal events

Events: `TermResizeEvent`, `TermMouseEvent`, plus the `try_cef_emit_keyed` keyboard event.

Files affected:
- `crates/vmux_terminal/src/event.rs` — derives, plus define a typed `TermKeyboardEvent` to replace the keyed-object hack
- `crates/vmux_terminal/src/app.rs:428, :509` — replace `try_cef_emit_keyed` with typed `try_cef_bin_emit_rkyv(&TermKeyboardEvent { ... })`
- `crates/vmux_desktop/src/terminal.rs:197-198, :1550, :1593, :1685`
- Commit: `feat(vmux): migrate terminal events to binary IPC channel + typed keyboard event`

### Task 6.6: Migrate process events

Events: `ProcessNavigateEvent`, `ProcessKillEvent`, `ProcessKillAllEvent`.

Files affected:
- `crates/vmux_processes/src/event.rs`
- `crates/vmux_process/src/app.rs:66, :172, :180`
- `crates/vmux_desktop/src/processes_monitor.rs:96-98, :178, :233, :264`
- Commit: `feat(vmux): migrate process events to binary IPC channel`

### Task 6.7: Migrate `UiReady` and any history events

Files:
- `crates/vmux_webview_app/src/lib.rs:6, :103, :108`
- `crates/vmux_history/src/event.rs`, `crates/vmux_history/src/plugin.rs:45`
- Commit: `feat(vmux): migrate UiReady + history events to binary IPC channel`

### Task 6.8: Audit for any remaining `try_cef_emit_serde` / `ron::ser::to_string` callsites tied to the bridge

```bash
rg 'try_cef_emit_serde|HostEmitEvent::new|ron::ser::to_string' crates/ --type rust
```

For each remaining call: confirm whether it's bridge traffic (migrate) or unrelated (e.g., on-disk persistence — leave it). Any bridge traffic that wasn't moved is a bug.

- [ ] Commit any final migrations.

---

## Phase 7: Optional cleanup

### Task 7.1: Remove `try_cef_emit_serde` and `decode_host_emit_js` if no callers remain

```bash
rg 'try_cef_emit_serde|decode_host_emit_js' crates/ --type rust
```

If empty, delete the functions and their `EventListenerError::SerializePayload` / `EventListenerError::InvalidJson` variants.

If callers remain (e.g., a webview-content script outside vmux's control still posts JSON), keep them.

- [ ] Commit: `chore(vmux_ui): remove unused JSON emit/listen helpers`

---

## Phase 8: End-to-end verification

### Task 8.1: Lint and test

- [ ] `make lint` — fix any errors with `make lint-fix` then re-verify
- [ ] `make test` — all workspace tests pass, including all new rkyv round-trip tests

### Task 8.2: Manual smoke

- [ ] `cargo run -p vmux_desktop --features debug`
- [ ] Walk every event in the migration list:
  - Theme propagates on app start
  - Command bar opens, picks command, returns completions
  - Session picker activates sessions
  - Header / footer / side sheet buttons fire commands
  - Terminal resizes, mouse clicks land, keyboard input flows
  - Process row click navigates, kill works
  - UiReady fires and the webview is marked ready in Bevy
- [ ] Devtools console shows no decode errors on any webview
- [ ] No regressions on JSON-channel events (BRP, anything not migrated)

### Task 8.3: Push and open PR

- [ ] `make lint && make test` — both pass
- [ ] `git push -u origin jun/vmx-106-rkyv-ipc`
- [ ] Use the `open-new-pr` skill. PR title should reference VMX-106. Body should:
  - Summarize the additive design (new bin channel parallel to JSON channel)
  - List which events are migrated and which remain on JSON
  - Note that the bevy_cef_core fork is now part of the patch maintenance burden
  - Link to VMX-106
- [ ] `linear issue update VMX-106 --state "In Review"`

### Task 8.4: Delete this plan file

Per AGENTS.md: "Delete the plan file once the plan is fully implemented."

```bash
git rm docs/plans/2026-05-08-rkyv-binary-ipc.md
git commit -m "chore: remove implemented rkyv-ipc plan"
```

---

## Risk Notes for the Executor

- **0.5.2 vs 0.8.1 source drift:** I drafted bevy_cef_core changes against the 0.8.1 source on disk. 0.5.2 should have the same overall structure but exact line numbers, helper imports, and `if let` chain shapes will differ. **Read each function before editing.**

- **rkyv `pointer_width_32` feature is mandatory.** Native and wasm targets must agree on archive layout.

- **Shared `LISTEN_EVENTS` registration is the recommended path** (Task 3.3 Step 2). It keeps the V8 native count at 4 instead of 5 and makes `cef.listen` / `cef.binListen` symmetric. Document this clearly so future readers know the dispatch direction (JSON vs binary) is determined by the incoming process message name, not by which native registered the callback.

- **Mixing channels per event is a footgun.** Don't migrate one direction of an event without the other. The plan groups by event family for this reason.

- **`try_cef_emit_keyed` (terminal keyboard):** introduce a typed `TermKeyboardEvent` in Task 6.5 rather than carrying the keyed-object pattern forward. The whole point of this work is type safety end-to-end.

- **`Reflect` on `BinHostEmitEvent` with `Vec<u8>`:** use `#[reflect(opaque)]` if Bevy reflection rejects the type, or `#[reflect(ignore)]` on the payload field. BRP doesn't introspect this event so the choice is internal.

- **V8 ArrayBuffer lifetime:** `array_buffer_data()` returns a raw pointer — copy bytes into a `Vec<u8>` while the V8 value is still alive in the calling context. Don't pass the raw pointer downstream.

- **Rollback:** because everything is additive, the cleanest rollback is per-event: revert the migration commit. The fork itself stays usable — the binary channel just sits unused.
