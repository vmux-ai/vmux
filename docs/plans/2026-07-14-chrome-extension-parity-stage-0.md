# Chrome Extension Parity Stage 0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the testable foundation for Chrome extension parity: an audited capability matrix, immutable generated runtimes, an authenticated extension bridge, a canonical Chrome window/tab model, and a Chromium differential harness.

**Architecture:** `vmux_browser` owns an extension compatibility service. Installed CRX contents remain immutable under `packages/`; vmux generates loadable runtime copies under `runtime/{profile}/`. A per-extension hidden extension page connects to a loopback WebSocket and relays versioned protocol messages to extension workers through native `chrome.runtime` messaging. A Rust `ChromeModel` projects extension-visible browser state from production ECS entities.

**Tech Stack:** Rust, Bevy ECS/messages, CEF 148, MV3 JavaScript, serde/JSON, RON, SHA-256, tungstenite, crossbeam-channel, shell-based local conformance runner.

---

## Scope

This plan implements Stage 0 from
`docs/specs/2026-07-14-chrome-extension-parity-design.md`. It does not implement public
`chrome.tabs`, `chrome.windows`, or action APIs. Stage 0 exposes a test-only model snapshot
through the bridge when `VMUX_EXTENSION_CONFORMANCE=1`; Stage 1 replaces that test hook with
real Chrome namespace handlers.

## File Map

| Path | Responsibility |
|---|---|
| `crates/vmux_browser/src/extensions/capability.rs` | Parse and query the versioned support matrix |
| `crates/vmux_browser/src/extensions/capabilities.ron` | Initial Chromium 148 capability entries |
| `crates/vmux_core/src/extension/protocol.rs` | Shared bridge request, response, event, hello, and error types |
| `crates/vmux_core/src/extension/store.rs` | Immutable package/runtime paths, hashes, migration, uninstall |
| `crates/vmux_browser/src/extensions/runtime.rs` | Generate isolated runtime copies and patched manifests |
| `crates/vmux_browser/src/extensions/runtime/worker.js` | Worker-side reserved bridge adapter |
| `crates/vmux_browser/src/extensions/runtime/bridge.html` | Hidden extension-origin bridge document |
| `crates/vmux_browser/src/extensions/runtime/bridge.js` | WebSocket/runtime-message relay |
| `crates/vmux_browser/src/extensions/bridge.rs` | Authenticated loopback WebSocket server and session routing |
| `crates/vmux_browser/src/extensions/bridge_page.rs` | Spawn and maintain hidden bridge webviews |
| `crates/vmux_browser/src/extensions/broker.rs` | Drain bridge requests and produce responses/events |
| `crates/vmux_browser/src/extensions/model.rs` | Canonical Chrome window/tab state and stable IDs |
| `crates/vmux_browser/src/extensions/model/project.rs` | Project vmux ECS hierarchy into `ChromeModel` |
| `crates/vmux_browser/src/bin/vmux-extension-conformance.rs` | Launch target, collect fixture result, normalize JSON |
| `crates/vmux_browser/tests/fixtures/extension_conformance/` | MV3 fixture extension and expected shared observations |
| `scripts/extension-conformance.sh` | Run matching Chrome and vmux captures and compare them |

### Task 1: Add the capability matrix

**Files:**
- Create: `crates/vmux_browser/src/extensions/capability.rs`
- Create: `crates/vmux_browser/src/extensions/capabilities.ron`
- Modify: `crates/vmux_browser/src/extensions.rs`

- [ ] **Step 1: Write failing parser and coverage tests**

Add this test module to `capability.rs` before implementing the parser:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_matrix_has_unique_entries_for_chromium_148() {
        let matrix = CapabilityMatrix::embedded().unwrap();
        assert_eq!(matrix.chromium_major, 148);
        assert_eq!(matrix.lookup("macos", "tabs", "query", CapabilityKind::Method).unwrap().status, CapabilityStatus::Untested);
        matrix.validate().unwrap();
    }

    #[test]
    fn advertised_entries_require_scenarios() {
        let matrix = CapabilityMatrix {
            chromium_major: 148,
            entries: vec![CapabilityEntry {
                platform: "macos".into(),
                namespace: "runtime".into(),
                member: "sendMessage".into(),
                kind: CapabilityKind::Method,
                status: CapabilityStatus::Native,
                owner: Some("cef".into()),
                scenario: None,
            }],
        };
        assert_eq!(matrix.validate().unwrap_err(), "runtime.sendMessage on macos is Native without a scenario");
    }
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```bash
cargo test -p vmux_browser capability --lib
```

Expected: compilation fails because `CapabilityMatrix`, `CapabilityEntry`,
`CapabilityKind`, and `CapabilityStatus` do not exist.

- [ ] **Step 3: Implement capability types and validation**

Create `capability.rs` with these public types and behavior:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const EMBEDDED: &str = include_str!("capabilities.ron");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityKind {
    Method,
    Event,
    Property,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityStatus {
    Native,
    Bridged,
    Unsupported { reason: String },
    Untested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub platform: String,
    pub namespace: String,
    pub member: String,
    pub kind: CapabilityKind,
    pub status: CapabilityStatus,
    pub owner: Option<String>,
    pub scenario: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub chromium_major: u32,
    pub entries: Vec<CapabilityEntry>,
}

impl CapabilityMatrix {
    pub fn embedded() -> Result<Self, String> {
        ron::from_str(EMBEDDED).map_err(|error| error.to_string())
    }

    pub fn lookup(
        &self,
        platform: &str,
        namespace: &str,
        member: &str,
        kind: CapabilityKind,
    ) -> Option<&CapabilityEntry> {
        self.entries.iter().find(|entry| {
            entry.platform == platform
                && entry.namespace == namespace
                && entry.member == member
                && entry.kind == kind
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut keys = HashSet::new();
        for entry in &self.entries {
            let key = (
                entry.platform.as_str(),
                entry.namespace.as_str(),
                entry.member.as_str(),
                entry.kind,
            );
            if !keys.insert(key) {
                return Err(format!(
                    "duplicate capability {}.{} on {}",
                    entry.namespace, entry.member, entry.platform
                ));
            }
            if matches!(entry.status, CapabilityStatus::Native | CapabilityStatus::Bridged)
                && entry.scenario.as_deref().is_none_or(str::is_empty)
            {
                return Err(format!(
                    "{}.{} on {} is {:?} without a scenario",
                    entry.namespace, entry.member, entry.platform, entry.status
                ));
            }
        }
        Ok(())
    }
}
```

Create `capabilities.ron`:

```ron
(
  chromium_major: 148,
  entries: [
    (platform: "macos", namespace: "runtime", member: "sendMessage", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "macos", namespace: "storage.local", member: "get", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "macos", namespace: "tabs", member: "query", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "macos", namespace: "windows", member: "getCurrent", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "linux", namespace: "runtime", member: "sendMessage", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "linux", namespace: "storage.local", member: "get", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "linux", namespace: "tabs", member: "query", kind: Method, status: Untested, owner: None, scenario: None),
    (platform: "linux", namespace: "windows", member: "getCurrent", kind: Method, status: Untested, owner: None, scenario: None),
  ],
)
```

Register the module in `extensions.rs` with `mod capability;`.

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p vmux_browser capability --lib
```

Expected: 2 capability tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/extensions/capability.rs crates/vmux_browser/src/extensions/capabilities.ron
git commit -m "feat(browser): add extension capability matrix"
```

### Task 2: Define the versioned bridge protocol

**Files:**
- Create: `crates/vmux_core/src/extension/protocol.rs`
- Modify: `crates/vmux_core/src/extension.rs`

- [ ] **Step 1: Write failing JSON round-trip tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trips_as_tagged_json() {
        let message = BridgeClientMessage::Hello(BridgeHello {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            profile_id: "personal".into(),
            token: "secret".into(),
            context_id: "bridge-page".into(),
            context_kind: ExtensionContextKind::BridgePage,
        });
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("hello"));
        assert_eq!(serde_json::from_str::<BridgeClientMessage>(&json).unwrap(), message);
    }

    #[test]
    fn api_response_has_exactly_one_result_channel() {
        let response = ApiResponse::success("r1", serde_json::json!({ "ok": true }));
        response.validate().unwrap();
        assert!(ApiResponse {
            request_id: "r2".into(),
            result: Some(serde_json::Value::Null),
            error: Some(ChromeError::new("invalid", "bad")),
        }
        .validate()
        .is_err());
    }
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```bash
cargo test -p vmux_core extension::protocol --lib
```

Expected: compilation fails because `extension::protocol` is absent.

- [ ] **Step 3: Implement the protocol**

Create `protocol.rs` with serde-tagged messages:

```rust
use serde::{Deserialize, Serialize};

pub const BRIDGE_PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionContextKind {
    BridgePage,
    ServiceWorker,
    ExtensionPage,
    ContentScript,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeHello {
    pub protocol_version: u16,
    pub extension_id: String,
    pub profile_id: String,
    pub token: String,
    pub context_id: String,
    pub context_kind: ExtensionContextKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiRequest {
    pub request_id: String,
    pub namespace: String,
    pub method: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubscribe {
    pub subscription_id: String,
    pub namespace: String,
    pub event: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BridgeClientMessage {
    Hello(BridgeHello),
    ApiRequest(ApiRequest),
    Subscribe(EventSubscribe),
    Ack { sequence: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeError {
    pub code: String,
    pub message: String,
}

impl ChromeError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiResponse {
    pub request_id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<ChromeError>,
}

impl ApiResponse {
    pub fn success(request_id: impl Into<String>, result: serde_json::Value) -> Self {
        Self { request_id: request_id.into(), result: Some(result), error: None }
    }

    pub fn failure(request_id: impl Into<String>, error: ChromeError) -> Self {
        Self { request_id: request_id.into(), result: None, error: Some(error) }
    }

    pub fn validate(&self) -> Result<(), String> {
        match (self.result.is_some(), self.error.is_some()) {
            (true, false) | (false, true) => Ok(()),
            _ => Err(format!("response {} must contain exactly one result channel", self.request_id)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiEvent {
    pub sequence: u64,
    pub namespace: String,
    pub event: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BridgeServerMessage {
    Ready { protocol_version: u16 },
    Response(ApiResponse),
    Event(ApiEvent),
    Fatal(ChromeError),
}
```

Export it with `pub mod protocol;` in `extension.rs`.

- [ ] **Step 4: Run the focused tests**

```bash
cargo test -p vmux_core extension::protocol --lib
```

Expected: 2 protocol tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/extension.rs crates/vmux_core/src/extension/protocol.rs
git commit -m "feat(core): add extension bridge protocol"
```

### Task 3: Split immutable packages from generated runtimes

**Files:**
- Modify: `crates/vmux_core/src/extension/store.rs`
- Modify: `crates/vmux_browser/src/extensions/install.rs`
- Test: existing inline store/install tests

- [ ] **Step 1: Add failing path, hash, and legacy-migration tests**

Add tests that create `root/{id}/manifest.json`, `vmux_shim.json`, and a generated worker.
The assertions must be:

```rust
let migrated = migrate_legacy_package(root.path(), &entry).unwrap();
assert_eq!(migrated, source_dir(root.path(), &entry.id, &entry.version));
let manifest: serde_json::Value = serde_json::from_str(
    &std::fs::read_to_string(migrated.join("manifest.json")).unwrap(),
).unwrap();
assert_eq!(manifest["background"]["service_worker"], "background.js");
assert!(!migrated.join("vmux_patch.js").exists());
assert!(!migrated.join("vmux_shim.json").exists());
assert_eq!(tree_sha256(&migrated).unwrap().len(), 64);
```

Add an install test using a fixture CRX and assert that the final manifest is under
`packages/{id}/{version}/source/`, while `root/{id}/` is absent.

- [ ] **Step 2: Run the tests and confirm failure**

```bash
cargo test -p vmux_core extension::store --lib
cargo test -p vmux_browser extensions::install --lib
```

Expected: compilation fails because package/runtime path and migration functions are
missing.

- [ ] **Step 3: Add immutable layout helpers and source hashing**

Add these functions to `store.rs`:

```rust
pub fn packages_root(root: &Path) -> PathBuf { root.join("packages") }
pub fn runtimes_root(root: &Path) -> PathBuf { root.join("runtime") }

pub fn source_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    packages_root(root).join(id).join(version).join("source")
}

pub fn runtime_profile_dir(root: &Path, profile: &str, id: &str) -> PathBuf {
    runtimes_root(root).join(profile).join(id)
}

pub fn tree_sha256(root: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (relative, absolute) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(absolute).map_err(|error| error.to_string())?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    for entry in std::fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            out.push((relative.to_string_lossy().replace('\\', "/"), path));
        }
    }
    Ok(())
}
```

Add these fields to `ExtEntry` and update all constructors and tests:

```rust
#[serde(default)]
pub source_hash: String,
#[serde(default)]
pub public_key_b64: Option<String>,
```

- [ ] **Step 4: Implement legacy migration without mutating the legacy directory**

Implement `copy_tree`, `restore_original_worker`, and `migrate_legacy_package` in
`store.rs`. `restore_original_worker` reads `vmux_shim.json`, restores
`background.service_worker`, and removes only generated names:

```rust
fn is_vmux_generated(name: &str) -> bool {
    name == "vmux_patch.js"
        || name == "vmux_shim.js"
        || name == "vmux_shim.json"
        || name.starts_with("vmux_sw_") && name.ends_with(".js")
}
```

Write into `source.tmp`, validate `manifest.json`, then atomically rename to `source`.
Leave `root/{id}/` untouched until the new source exists and hashes successfully.

- [ ] **Step 5: Install directly into the immutable package directory**

In `install.rs`, extract the CRX public key without modifying the unpacked manifest:

```rust
let public_key = crx::crx_public_key_for(&bytes, &id)
    .ok_or_else(|| format!("CRX does not contain the developer key for extension {id}"))?;
let public_key_b64 = Some(base64::engine::general_purpose::STANDARD.encode(public_key));
```

Remove the `manifest::prepare_unpacked` call from installation. The generated runtime
owns manifest modifications. Replace `root.join(&id)` with:

```rust
let final_dir = store::source_dir(&root, &id, &m.version);
let final_parent = final_dir.parent().ok_or("source directory has no parent")?;
std::fs::create_dir_all(final_parent).map_err(|error| error.to_string())?;
let _ = std::fs::remove_dir_all(&final_dir);
std::fs::rename(&unpack_dir, &final_dir).map_err(|error| error.to_string())?;
let source_hash = store::tree_sha256(&final_dir)?;
```

Store `source_hash` and `public_key_b64` in `ExtEntry`. Update uninstall to remove both
`packages/{id}/` and every profile runtime under `runtime/*/{id}/`.

- [ ] **Step 6: Run focused tests**

```bash
cargo test -p vmux_core extension::store --lib
cargo test -p vmux_browser extensions::install --lib
```

Expected: store migration/path/hash tests and install tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_core/src/extension/store.rs crates/vmux_browser/src/extensions/install.rs
git commit -m "refactor(extensions): separate source and runtime packages"
```

### Task 4: Generate isolated extension runtimes

**Files:**
- Create: `crates/vmux_browser/src/extensions/runtime.rs`
- Create: `crates/vmux_browser/src/extensions/runtime/worker.js`
- Create: `crates/vmux_browser/src/extensions/runtime/bridge.html`
- Create: `crates/vmux_browser/src/extensions/runtime/bridge.js`
- Modify: `crates/vmux_browser/src/extensions.rs`
- Modify: `crates/vmux_browser/src/extensions/load.rs`
- Modify: `crates/vmux_browser/src/extensions/shim.rs`

- [ ] **Step 1: Write failing runtime generation tests**

Test both classic and module workers. The classic assertions:

```rust
let prepared = prepare_runtime(root.path(), "personal", &entry).unwrap();
assert!(prepared.dir.starts_with(store::runtime_profile_dir(root.path(), "personal", &entry.id)));
assert_eq!(std::fs::read_to_string(source.join("manifest.json")).unwrap(), original_manifest);
let generated: serde_json::Value = serde_json::from_str(
    &std::fs::read_to_string(prepared.dir.join("manifest.json")).unwrap(),
).unwrap();
let worker = generated["background"]["service_worker"].as_str().unwrap();
let loader = std::fs::read_to_string(prepared.dir.join(worker)).unwrap();
assert!(loader.contains("importScripts(\"vmux_runtime.js\")"));
assert!(loader.contains("importScripts(\"vmux_patch.js\")"));
assert!(loader.contains("importScripts(\"background.js\")"));
assert!(prepared.dir.join("vmux_bridge.html").exists());
assert!(prepared.dir.join("vmux_bridge.js").exists());
```

The module test requires three static imports in the same order.

- [ ] **Step 2: Run the tests and confirm failure**

```bash
cargo test -p vmux_browser extensions::runtime --lib
```

Expected: compilation fails because `runtime` and `prepare_runtime` are absent.

- [ ] **Step 3: Add bridge assets**

Create `runtime/bridge.html`:

```html
<!doctype html>
<meta charset="utf-8">
<script src="vmux_bridge.js"></script>
```

Create `runtime/worker.js`:

```javascript
(() => {
  const CHANNEL = "__vmux_extension_bridge_v1";
  const listeners = new Map();
  chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (!message || message.channel !== CHANNEL) return undefined;
    if (message.type === "event") {
      const handlers = listeners.get(`${message.namespace}.${message.event}`) || [];
      for (const handler of handlers) handler(...message.arguments);
      sendResponse({ ok: true, sequence: message.sequence });
      return true;
    }
    return undefined;
  });
  globalThis.__vmuxExtensionRuntime = {
    channel: CHANNEL,
    register(namespace, event, handler) {
      const key = `${namespace}.${event}`;
      const handlers = listeners.get(key) || [];
      handlers.push(handler);
      listeners.set(key, handlers);
      chrome.runtime.sendMessage({
        channel: CHANNEL,
        type: "subscribe",
        subscriptionId: key,
        namespace,
        event,
      });
    },
    request(namespace, method, argumentsValue) {
      return chrome.runtime.sendMessage({
        channel: CHANNEL,
        type: "api_request",
        requestId: crypto.randomUUID(),
        namespace,
        method,
        arguments: argumentsValue,
      }).then((response) => {
        if (response && response.error) {
          throw new Error(response.error.message);
        }
        return response ? response.result : undefined;
      });
    },
  };
})();
```

Create `runtime/bridge.js`:

```javascript
(() => {
  const CHANNEL = "__vmux_extension_bridge_v1";
  const params = new URLSearchParams(location.search);
  const socket = new WebSocket(params.get("endpoint"));
  const pendingFrames = [];
  const pendingCallbacks = new Map();

  function send(frame) {
    const encoded = JSON.stringify(frame);
    if (socket.readyState === WebSocket.OPEN) socket.send(encoded);
    else pendingFrames.push(encoded);
  }

  socket.addEventListener("open", () => {
    send({
      type: "hello",
      payload: {
        protocol_version: 1,
        extension_id: params.get("extension"),
        profile_id: params.get("profile"),
        token: params.get("token"),
        context_id: "bridge-page",
        context_kind: "bridge_page",
      },
    });
    while (pendingFrames.length) socket.send(pendingFrames.shift());
  });

  socket.addEventListener("message", async (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "response") {
      const callback = pendingCallbacks.get(message.payload.request_id);
      if (callback) {
        pendingCallbacks.delete(message.payload.request_id);
        callback({ result: message.payload.result, error: message.payload.error });
      }
      return;
    }
    if (message.type === "event") {
      const delivery = await chrome.runtime.sendMessage({
        channel: CHANNEL,
        type: "event",
        namespace: message.payload.namespace,
        event: message.payload.event,
        arguments: message.payload.arguments,
        sequence: message.payload.sequence,
      });
      if (delivery && delivery.ok === true) {
        send({ type: "ack", payload: { sequence: message.payload.sequence } });
      }
    }
  });

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || message.channel !== CHANNEL) {
      return undefined;
    }
    if (message.type === "subscribe") {
      send({
        type: "subscribe",
        payload: {
          subscription_id: message.subscriptionId,
          namespace: message.namespace,
          event: message.event,
        },
      });
      sendResponse({ accepted: true });
      return false;
    }
    if (message.type !== "api_request") return undefined;
    pendingCallbacks.set(message.requestId, sendResponse);
    send({
      type: "api_request",
      payload: {
        request_id: message.requestId,
        namespace: message.namespace,
        method: message.method,
        arguments: message.arguments,
      },
    });
    return true;
  });
})();
```

- [ ] **Step 4: Implement deterministic runtime generation**

Define:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRuntime {
    pub extension_id: String,
    pub dir: PathBuf,
    pub runtime_hash: String,
    pub source_hash: String,
}

pub fn prepare_runtime(root: &Path, profile: &str, entry: &store::ExtEntry) -> Result<PreparedRuntime, String>
```

If `store::source_dir` does not exist, call `store::migrate_legacy_package` before hashing
or copying. Return an error when neither an immutable source nor a valid legacy directory
exists; do not silently omit an enabled extension.

Compute the source hash from disk on every preparation. If `entry.source_hash` is
non-empty and differs, return an integrity error. If it is empty because the entry was
migrated from the legacy layout, return the computed value in `PreparedRuntime` and have
`load::apply_env` persist it to the index after every enabled runtime prepares successfully.

The runtime hash is SHA-256 over:

```text
source_hash + BRIDGE_PROTOCOL_VERSION + worker.js + bridge.html + bridge.js + shim.js
```

Generate into `{hash}.tmp`, copy the immutable source, write generated files, modify the
copied manifest, validate it, rename to `{hash}`, then remove sibling runtime hashes.

After copying the source, call:

```rust
if let Some(key) = entry.public_key_b64.as_deref() {
    manifest::prepare_unpacked(&temp_dir, key, entry.popup.as_deref())?;
}
```

This preserves Web Store extension identity and popup accessibility without changing the
immutable package.

Refactor `shim.rs` to expose:

```rust
pub(crate) fn install_worker_loader(dir: &Path, runtime_file: &str) -> Result<String, String>
```

It must operate only on the generated runtime directory and return the generated loader
filename. Keep the legacy `shim.js` in the loader until Stage 1 removes it.

- [ ] **Step 5: Make `load.rs` load generated runtime directories**

Change `apply_env` to return prepared runtimes:

```rust
pub fn apply_env() -> Result<Vec<PreparedRuntime>, String> {
    let root = store::root();
    let profile = vmux_core::profile::active_profile_name();
    let idx = store::Index::load(&root)?;
    let mut prepared = Vec::new();
    for entry in idx.entries.iter().filter(|entry| entry.enabled) {
        prepared.push(runtime::prepare_runtime(&root, &profile, entry)?);
    }
    let dirs = prepared.iter().map(|item| item.dir.to_string_lossy()).collect::<Vec<_>>();
    if dirs.is_empty() {
        unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
    } else {
        unsafe { std::env::set_var("VMUX_LOAD_EXTENSIONS", dirs.join(",")) };
    }
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    std::fs::write(root.join("loaded.txt"), idx.enabled_ids().join("\n")).map_err(|error| error.to_string())?;
    Ok(prepared)
}
```

- [ ] **Step 6: Run focused tests**

```bash
cargo test -p vmux_browser extensions::runtime --lib
cargo test -p vmux_browser extensions::shim --lib
```

Expected: runtime and updated shim tests pass; source manifests remain byte-identical.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/extensions/load.rs crates/vmux_browser/src/extensions/runtime.rs crates/vmux_browser/src/extensions/runtime crates/vmux_browser/src/extensions/shim.rs crates/vmux_browser/src/extensions/shim.js
git commit -m "feat(extensions): generate isolated extension runtimes"
```

### Task 5: Add authenticated loopback bridge transport

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/vmux_browser/Cargo.toml`
- Create: `crates/vmux_browser/src/extensions/bridge.rs`
- Modify: `crates/vmux_browser/src/extensions.rs`

- [ ] **Step 1: Write failing authentication and routing tests**

Add tests that start a server with one `BridgeIdentity`, connect with
`tungstenite::connect`, and assert:

```rust
let ready: BridgeServerMessage = read_json(&mut socket);
assert_eq!(ready, BridgeServerMessage::Ready { protocol_version: BRIDGE_PROTOCOL_VERSION });
let inbound = server.try_recv().unwrap();
assert_eq!(inbound.extension_id, EXTENSION_ID);
assert_eq!(inbound.message, BridgeClientMessage::ApiRequest(request));
server.send(EXTENSION_ID, BridgeServerMessage::Response(ApiResponse::success("r1", serde_json::json!({ "ok": true })))).unwrap();
```

A second test sends the wrong token and expects a `Fatal` message with code
`authentication_failed`, followed by socket closure.

- [ ] **Step 2: Run the test and confirm failure**

```bash
cargo test -p vmux_browser extensions::bridge --lib
```

Expected: compilation fails because bridge transport types are absent.

- [ ] **Step 3: Add direct dependencies**

Add workspace dependencies:

```toml
tungstenite = "0.28"
crossbeam-channel = "0.5"
```

Add to `vmux_browser`:

```toml
crossbeam-channel = { workspace = true }
tungstenite = { workspace = true }
uuid = { workspace = true }
```

- [ ] **Step 4: Implement bridge identity and server resource**

Use these public types:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeIdentity {
    pub extension_id: String,
    pub profile_id: String,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct BridgeInbound {
    pub extension_id: String,
    pub context_id: String,
    pub message: BridgeClientMessage,
}

#[derive(Resource)]
pub struct ExtensionBridgeServer {
    endpoint: String,
    identities: HashMap<String, BridgeIdentity>,
    inbound_rx: crossbeam_channel::Receiver<BridgeInbound>,
    sessions: Arc<Mutex<HashMap<String, crossbeam_channel::Sender<BridgeServerMessage>>>>,
    shutdown: Arc<AtomicBool>,
}
```

`ExtensionBridgeServer::start(profile, extension_ids)` must:

1. Bind `TcpListener` to `127.0.0.1:0`.
2. Generate one UUID token per extension.
3. Set the listener nonblocking.
4. Spawn a named `extension-bridge-accept` thread.
5. Spawn one named connection thread per accepted socket.
6. Require `Hello` as the first frame.
7. Compare protocol, extension, profile, and token.
8. Register a per-extension outbound sender only after authentication.
9. Remove the sender when the connection closes.

Set each connection stream read timeout to 25 ms so the thread can drain outbound
messages and observe shutdown without blocking indefinitely.

- [ ] **Step 5: Implement public routing methods**

```rust
impl ExtensionBridgeServer {
    pub fn endpoint(&self) -> &str { &self.endpoint }

    pub fn identity(&self, extension_id: &str) -> Option<&BridgeIdentity> {
        self.identities.get(extension_id)
    }

    pub fn try_recv(&self) -> Result<BridgeInbound, crossbeam_channel::TryRecvError> {
        self.inbound_rx.try_recv()
    }

    pub fn send(&self, extension_id: &str, message: BridgeServerMessage) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        let sender = sessions.get(extension_id).ok_or_else(|| format!("extension {extension_id} is not connected"))?;
        sender.send(message).map_err(|error| error.to_string())
    }
}

impl Drop for ExtensionBridgeServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}
```

Use `bevy::log::warn!` for malformed frames and connection failures. Never log tokens.

- [ ] **Step 6: Run focused tests**

```bash
cargo test -p vmux_browser extensions::bridge --lib
```

Expected: authenticated routing and rejected-token tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/vmux_browser/Cargo.toml crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/extensions/bridge.rs
git commit -m "feat(extensions): add authenticated bridge transport"
```

### Task 6: Host bridge pages and connect the broker

**Files:**
- Create: `crates/vmux_browser/src/extensions/bridge_page.rs`
- Create: `crates/vmux_browser/src/extensions/broker.rs`
- Modify: `crates/vmux_browser/src/extensions.rs`
- Modify: `crates/vmux_browser/src/lib.rs`

- [ ] **Step 1: Write failing bridge-page spawning tests**

Create an app test with `PreparedExtensions`, `ExtensionBridgeServer`, and one primary
window. Run the spawn system and assert:

```rust
let (entity, bridge, source, size, visibility) = query.single(app.world()).unwrap();
assert_eq!(bridge.extension_id, EXTENSION_ID);
assert!(matches!(source, WebviewSource::Url(url) if url.starts_with(&format!("chrome-extension://{EXTENSION_ID}/vmux_bridge.html?"))));
assert_eq!(size.0, Vec2::ONE);
assert_eq!(*visibility, Visibility::Hidden);
assert!(app.world().get::<vmux_layout::Browser>(entity).is_none());
```

Add a broker test that sends an unsupported request and expects:

```rust
BridgeServerMessage::Response(ApiResponse::failure(
    "r1",
    ChromeError::new("unsupported_api", "tabs.query is Untested for Chromium 148 on macos"),
))
```

- [ ] **Step 2: Run the tests and confirm failure**

```bash
cargo test -p vmux_browser extensions::bridge_page --lib
cargo test -p vmux_browser extensions::broker --lib
```

Expected: compilation fails because page and broker modules are absent.

- [ ] **Step 3: Add prepared extension startup state**

Define in `load.rs`:

```rust
#[derive(Resource, Clone, Debug, Default)]
pub struct PreparedExtensions(pub Vec<PreparedRuntime>);
```

In `BrowserPlugin::build`, start the bridge before CEF initialization, prepare runtimes,
then insert both resources:

```rust
let root = vmux_core::extension::store::root();
let profile = vmux_core::profile::active_profile_name();
let index = vmux_core::extension::store::Index::load(&root).unwrap_or_default();
let enabled_ids = index.entries.iter().filter(|entry| entry.enabled).map(|entry| entry.id.clone()).collect::<Vec<_>>();
let bridge = crate::extensions::bridge::ExtensionBridgeServer::start(&profile, enabled_ids).expect("start extension bridge");
let prepared = crate::extensions::load::apply_env().expect("prepare extension runtimes");
app.insert_resource(crate::extensions::load::PreparedExtensions(prepared))
    .insert_resource(bridge);
```

Keep these builder calls in the existing chained `App` expression.

- [ ] **Step 4: Spawn one hidden bridge page per prepared runtime**

Create:

```rust
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBridgeWebview {
    pub extension_id: String,
}
```

The spawn system builds the URL with `url::Url`, adding `endpoint`, `token`, `extension`,
and `profile` query parameters, then spawns:

```rust
commands.spawn((
    ExtensionBridgeWebview { extension_id: runtime.extension_id.clone() },
    WebviewSource::new(url.to_string()),
    WebviewSize(Vec2::ONE),
    WebviewMaxFrameRate(1),
    Visibility::Hidden,
));
```

Add `url = { workspace = true }` to `vmux_browser`. Do not add `vmux_layout::Browser`,
`PageMetadata`, `Stack`, persistence, keyboard-target, or pointer-target components.

- [ ] **Step 5: Implement broker draining and capability rejection**

`drain_bridge_requests` reads every available `BridgeInbound`. It accepts only
`ApiRequest`; `Subscribe` is stored for Task 7; duplicate `Hello` and unknown protocol
flow produce `Fatal` errors.

For normal API requests, look up the current platform and capability. `Untested` and
`Unsupported` return `unsupported_api`; `Native` returns `native_api_not_bridged`; Stage 0
has no public `Bridged` methods.

Reject reserved conformance requests in this task because the model does not exist yet.
Task 7 adds the snapshot handler and its environment gate after defining `ChromeModel`.

- [ ] **Step 6: Register systems**

Register bridge-page spawning after startup and broker draining in `Update`. Ensure the
browser plugin chain includes:

```rust
.add_systems(Startup, crate::extensions::bridge_page::spawn_extension_bridge_pages)
.add_systems(Update, crate::extensions::broker::drain_bridge_requests)
```

- [ ] **Step 7: Run focused tests**

```bash
cargo test -p vmux_browser extensions::bridge_page --lib
cargo test -p vmux_browser extensions::broker --lib
```

Expected: bridge-page and unsupported-dispatch tests pass.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates/vmux_browser/Cargo.toml crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/extensions/load.rs crates/vmux_browser/src/extensions/bridge_page.rs crates/vmux_browser/src/extensions/broker.rs crates/vmux_browser/src/lib.rs
git commit -m "feat(extensions): host extension bridge contexts"
```

### Task 7: Project vmux ECS state into a canonical Chrome model

**Files:**
- Create: `crates/vmux_browser/src/extensions/model.rs`
- Create: `crates/vmux_browser/src/extensions/model/project.rs`
- Modify: `crates/vmux_browser/src/extensions.rs`
- Modify: `crates/vmux_browser/src/lib.rs`
- Modify: `crates/vmux_browser/src/extensions/broker.rs`

- [ ] **Step 1: Write failing projection tests**

Build a Bevy hierarchy containing two spaces, ordered vmux tabs, leaf panes, stacks, HTTP
pages, a terminal URL, and an internal URL. Run the production projection system and
assert:

```rust
let model = app.world().resource::<ChromeModel>();
assert_eq!(model.windows.len(), 1);
assert_eq!(model.tabs.len(), 3);
assert_eq!(model.tabs.iter().map(|tab| tab.url.as_str()).collect::<Vec<_>>(), vec![
    "https://one.example/",
    "https://two.example/",
    "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html",
]);
assert_eq!(model.tabs.iter().filter(|tab| tab.active).count(), 1);
assert!(!model.tabs.iter().any(|tab| tab.url.starts_with("vmux://")));
assert!(!model.tabs.iter().any(|tab| tab.url.starts_with("cef://")));
```

Record the first page's ID, change its title, update again, and assert the ID is unchanged.
Despawn it, update, and assert one `ChromeModelEvent::TabRemoved` contains that ID.

- [ ] **Step 2: Run the test and confirm failure**

```bash
cargo test -p vmux_browser extensions::model --lib
```

Expected: compilation fails because model types and projection are absent.

- [ ] **Step 3: Define model types and stable ID allocation**

Use:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ChromeWindow {
    pub id: i32,
    pub focused: bool,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ChromeTab {
    pub id: i32,
    pub window_id: i32,
    pub index: u32,
    pub active: bool,
    pub highlighted: bool,
    pub pinned: bool,
    pub url: String,
    pub title: String,
    pub status: String,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ChromeModel {
    pub windows: Vec<ChromeWindow>,
    pub tabs: Vec<ChromeTab>,
}

#[derive(Resource, Default)]
pub struct ChromeStableIds {
    next_window: i32,
    next_tab: i32,
    windows: HashMap<Entity, i32>,
    tabs: HashMap<Entity, i32>,
}
```

Initialize counters at `1`. IDs are process-lifetime stable and are not persisted.

- [ ] **Step 4: Implement the extension-visible URL filter**

```rust
pub fn extension_visible_url(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("chrome-extension://")
}
```

Bridge webviews are excluded by `Without<ExtensionBridgeWebview>` even though their URLs
use `chrome-extension://`.

- [ ] **Step 5: Implement production hierarchy projection**

Use an exclusive `rebuild_chrome_model(world: &mut World)` system so it can traverse
`Children` from spaces through vmux tabs, panes, and stacks without an oversized system
parameter. Sort spaces and vmux tabs by `Order`, visit pane children in stored child order,
then visit stack children in stored child order.

Create one `ChromeWindow` for every Bevy `Window`. Convert `WindowPosition::At` physical
coordinates to logical coordinates with the window scale factor and use
`Window::resolution.logical_width/height` for bounds. A page carrying `HostWindow` belongs
to that native window; otherwise it belongs to the `PrimaryWindow`.

For each eligible stack, read `PageMetadata`, `Loading`, and focus ancestry. The focused
web page is active. If focus ends on a terminal/editor/agent, retain the prior model's
active tab when its entity still exists. If no prior web page exists, select the eligible
stack with the greatest `LastActivatedAt`. Leave all tabs inactive only when the window has
no eligible web pages.

Build the new model first, diff against the previous model, then replace the resource and
write messages:

```rust
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum ChromeModelEvent {
    TabCreated(ChromeTab),
    TabUpdated { old: ChromeTab, new: ChromeTab },
    TabRemoved { tab_id: i32, window_id: i32 },
    TabActivated { tab_id: i32, window_id: i32 },
}
```

- [ ] **Step 6: Register model resources and ordering**

Register `ChromeModel`, `ChromeStableIds`, and `ChromeModelEvent`. Run projection after
`vmux_layout::stack::ComputeFocusSet` and after
`vmux_layout::apply_cef_state_from_webview`.

- [ ] **Step 7: Route model events to bridge subscribers**

Store `Subscribe` requests in `broker.rs`. For Stage 0, accept only
`__vmux_conformance.modelChanged`. Serialize each `ChromeModelEvent`, assign a monotonically
increasing sequence, and send it through `ExtensionBridgeServer`.

Add `PendingBridgeEvents`, keyed by extension ID and sequence. Insert an event before
sending it, remove it only after the matching `Ack`, and resend matching pending events
when a restarted worker sends `Subscribe` again. Bound each extension queue at 256 events;
the conformance event is non-coalescible, so queue overflow returns a logged bridge error
instead of dropping it.

When `VMUX_EXTENSION_CONFORMANCE=1`, also accept
`__vmux_conformance.snapshot` and return `serde_json::to_value(&ChromeModel)`. Return
`unsupported_api` for the reserved namespace when the environment gate is absent.

Add a `ConformanceWakeTimer` resource only when that environment gate is active. Start a
35-second timer after the bridge page subscribes. On expiry, send one
`__vmux_conformance.modelChanged` event containing the current snapshot. The delay exceeds
Chromium's normal 30-second service-worker idle threshold, so successful delivery verifies
that the persistent bridge page can wake the worker rather than merely talking to a worker
kept alive by the test.

- [ ] **Step 8: Run focused tests**

```bash
cargo test -p vmux_browser extensions::model --lib
cargo test -p vmux_browser extensions::broker --lib
```

Expected: ordering, exclusion, stable-ID, removal-event, and conformance subscription
tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/extensions/model.rs crates/vmux_browser/src/extensions/model crates/vmux_browser/src/extensions/broker.rs crates/vmux_browser/src/lib.rs
git commit -m "feat(extensions): project vmux pages into Chrome model"
```

### Task 8: Add the Chromium differential harness

**Files:**
- Create: `crates/vmux_browser/src/bin/vmux-extension-conformance.rs`
- Create: `crates/vmux_browser/tests/fixtures/extension_conformance/manifest.json`
- Create: `crates/vmux_browser/tests/fixtures/extension_conformance/background.js`
- Create: `crates/vmux_browser/tests/fixtures/extension_conformance/config.js`
- Create: `crates/vmux_browser/tests/fixtures/extension_conformance/test_public_key.der`
- Create: `crates/vmux_browser/tests/fixtures/extension_conformance/chromium-148-runtime.json`
- Create: `scripts/extension-conformance.sh`
- Modify: `crates/vmux_browser/Cargo.toml`

- [ ] **Step 1: Write failing normalization tests in the harness binary**

Define `Observation` and `Capture` with serde. The test compares captures while ignoring
only `target`, extension ID value, generated tab IDs, and timestamps:

```rust
#[test]
fn shared_observations_match_after_normalization() {
    let chrome = Capture::from_json(include_str!("../../tests/fixtures/extension_conformance/chromium-148-runtime.json")).unwrap();
    let vmux = Capture {
        target: "vmux".into(),
        chromium_major: 148,
        observations: vec![
            Observation::new("runtime.id.length", serde_json::json!(32)),
            Observation::new("storage.local.roundTrip", serde_json::json!("value")),
            Observation::new("runtime.message.roundTrip", serde_json::json!("pong")),
        ],
        internal_observations: vec![Observation::new("bridge.connected", serde_json::json!(true))],
    };
    compare_shared(&chrome, &vmux).unwrap();
}
```

- [ ] **Step 2: Run the test and confirm failure**

```bash
cargo test -p vmux_browser --bin vmux-extension-conformance
```

Expected: Cargo reports that the binary and fixture do not exist.

- [ ] **Step 3: Add the MV3 fixture**

Generate and commit a test-only RSA public key:

```bash
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out /tmp/vmux-extension-conformance-key.pem
openssl pkey -in /tmp/vmux-extension-conformance-key.pem -pubout -outform DER -out crates/vmux_browser/tests/fixtures/extension_conformance/test_public_key.der
```

Do not commit the private key. `manifest.json` is a template whose key is replaced by the
runner:

```json
{
  "manifest_version": 3,
  "name": "vmux extension conformance",
  "version": "1.0.0",
  "key": "__VMUX_TEST_PUBLIC_KEY__",
  "permissions": ["storage"],
  "host_permissions": ["http://127.0.0.1/*"],
  "background": { "service_worker": "background.js" }
}
```

`config.js` defines `globalThis.VMUX_CONFORMANCE = { target: "unset", collector: "" };`.
The runner copies the fixture, base64-encodes `test_public_key.der`, replaces
`__VMUX_TEST_PUBLIC_KEY__`, computes the extension ID with
`vmux_core::extension::crx::extension_id_from_key`, and overwrites `config.js` in its
temporary directory.

Create the initial checked-in baseline:

```json
{
  "target": "chrome",
  "chromium_major": 148,
  "observations": [
    { "key": "runtime.id.length", "value": 32 },
    { "key": "storage.local.roundTrip", "value": "value" },
    { "key": "runtime.message.roundTrip", "value": "pong" }
  ],
  "internal_observations": []
}
```

`background.js` must:

1. Store `{ value: "value" }` in `chrome.storage.local` and read it back.
2. Register an echo `runtime.onMessage` listener and round-trip `"ping"` to `"pong"`.
3. Record `chrome.runtime.id.length`.
4. When `globalThis.__vmuxExtensionRuntime` exists, request
   `__vmux_conformance.snapshot` and record `bridge.connected` and tab count in
   `internal_observations`.
5. Register `__vmux_conformance.modelChanged`; when received, POST a second capture with
   `worker.wakeEvent: true` in `internal_observations`.
6. POST the initial JSON `Capture` to the configured collector.

- [ ] **Step 4: Implement the collector and normalizer binary**

The binary accepts these concrete command forms:

```text
capture --target chrome --browser "/opt/chrome-for-testing-148/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" --output target/extension-conformance/chrome.json
capture --target vmux --browser target/debug/vmux_desktop --output target/extension-conformance/vmux.json
compare --baseline target/extension-conformance/chrome.json --candidate target/extension-conformance/vmux.json
```

Use `TcpListener::bind("127.0.0.1:0")` for an HTTP collector. Chrome capture expects one
request. vmux capture expects the initial request and the delayed worker-wake request. Copy the fixture
to a temporary directory, write `config.js` with the selected target and collector URL,
then launch:

```rust
Command::new(browser)
    .args([
        format!("--user-data-dir={}", profile.display()),
        format!("--load-extension={}", extension.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "about:blank".into(),
    ])
```

for Chrome. Before launching vmux, create a temporary HOME and write the fixture into the
normal immutable store layout:

```text
${TEMP_HOME}/.vmux/extensions/packages/${EXTENSION_ID}/1.0.0/source/
${TEMP_HOME}/.vmux/extensions/index.json
```

The index contains one enabled `ExtEntry` with the computed ID, source hash, popup fields,
and public key. Launch vmux with:

```rust
Command::new(browser)
    .env("VMUX_EXTENSION_CONFORMANCE", "1")
    .env("HOME", &temp_home)
    .env("VMUX_PROFILE", "extension-conformance")
```

for vmux. After the collector receives all expected JSON documents, merge observations,
write the output, terminate the child, and wait for it. Chrome times out after 30 seconds;
vmux times out after 50 seconds to include the 35-second worker-idle wake test. Timeout
errors include the child's exit status and collected stderr.

Before Chrome capture, run `browser --version`, parse the first numeric major, and reject
anything other than `148`. Never record a baseline from a newer system Chrome.

- [ ] **Step 5: Add the runner script**

Create `scripts/extension-conformance.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${CHROME_BIN:?set CHROME_BIN to a Chrome for Testing or Chromium 148 executable}"
VMUX_BIN="${VMUX_BIN:-$ROOT/target/debug/vmux_desktop}"
OUT="$ROOT/target/extension-conformance"
mkdir -p "$OUT"

cargo run -p vmux_browser --bin vmux-extension-conformance -- capture --target chrome --browser "$CHROME_BIN" --output "$OUT/chrome.json"
cargo run -p vmux_browser --bin vmux-extension-conformance -- capture --target vmux --browser "$VMUX_BIN" --output "$OUT/vmux.json"
cargo run -p vmux_browser --bin vmux-extension-conformance -- compare --baseline "$OUT/chrome.json" --candidate "$OUT/vmux.json"
```

Make it executable.

- [ ] **Step 6: Run normalization tests**

```bash
cargo test -p vmux_browser --bin vmux-extension-conformance
```

Expected: normalization and mismatch-reporting tests pass.

- [ ] **Step 7: Capture the Chromium 148 baseline locally**

Build vmux and run:

```bash
cargo build -p vmux_desktop
CHROME_BIN="/opt/chrome-for-testing-148/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" VMUX_BIN="target/debug/vmux_desktop" ./scripts/extension-conformance.sh
```

Expected: `target/extension-conformance/chrome.json` and `vmux.json` exist; shared runtime
and storage observations compare equal; vmux additionally reports bridge connectivity and
a model snapshot.

Copy the normalized Chrome shared observations to
`crates/vmux_browser/tests/fixtures/extension_conformance/chromium-148-runtime.json` and
rerun the binary tests.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_browser/Cargo.toml crates/vmux_browser/src/bin/vmux-extension-conformance.rs crates/vmux_browser/tests/fixtures/extension_conformance scripts/extension-conformance.sh
git commit -m "test(extensions): add Chromium conformance harness"
```

### Task 9: Stage 0 integration verification

**Files:**
- Modify only files required by failures discovered in these commands

- [ ] **Step 1: Run extension core tests**

```bash
cargo test -p vmux_core extension --lib
```

Expected: all extension store, CRX, manifest, webstore, and protocol tests pass.

- [ ] **Step 2: Run browser extension tests**

```bash
cargo test -p vmux_browser extensions --lib
```

Expected: capability, runtime, bridge, broker, bridge-page, model, install, and retained
legacy-shim tests pass.

- [ ] **Step 3: Run harness tests**

```bash
cargo test -p vmux_browser --bin vmux-extension-conformance
```

Expected: capture normalization and comparison tests pass.

- [ ] **Step 4: Check desktop integration**

```bash
cargo check -p vmux_desktop
```

Expected: desktop and patched CEF integration compile without errors.

- [ ] **Step 5: Verify formatting and diff integrity**

```bash
cargo fmt --check -p vmux_core -p vmux_browser
git diff --check
```

Expected: both commands exit 0.

- [ ] **Step 6: Run local Chromium/vmux differential capture**

```bash
CHROME_BIN="/opt/chrome-for-testing-148/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" VMUX_BIN="target/debug/vmux_desktop" ./scripts/extension-conformance.sh
```

Expected: shared observations match, bridge authentication succeeds, the worker reports a
model snapshot, and the command exits 0.

- [ ] **Step 7: Inspect application logs**

Read:

```text
the newest `~/Library/Application Support/Vmux/dev/logs/vmux-dev*.log`
~/Library/Application Support/Vmux/dev/profiles/extension-conformance/chrome_debug.log
```

Expected: one authenticated bridge connection, no token output, no bridge panic, no
`No window with id: -1`, and no unhandled service-worker exception.

- [ ] **Step 8: Commit verification fixes**

If verification changed files:

```bash
git add Cargo.toml Cargo.lock crates/vmux_core crates/vmux_browser scripts/extension-conformance.sh
git commit -m "fix(extensions): resolve stage zero integration issues"
```

If no files changed, do not create an empty commit.

## Stage 0 Completion Gate

Stage 0 is complete only when:

- Immutable source packages remain byte-identical after startup.
- Runtime generation is deterministic for a source/adaptor version.
- Wrong bridge tokens are rejected without leaking valid credentials.
- One hidden bridge page is created per enabled extension and excluded from user-facing
  layout/model state.
- `ChromeModel` exposes deterministic ordering, one active web page, stable process-lifetime
  IDs, and removal/update events.
- A restarted MV3 worker reconnects and receives a conformance model event.
- Chromium 148 and vmux shared observations compare equal.
- All Task 9 commands exit 0.
