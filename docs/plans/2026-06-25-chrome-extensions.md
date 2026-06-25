# Chrome Extension Support Implementation Plan

> **For agentic workers:** Implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. CEF builds are huge — implement directly with a warm target dir; do NOT subagent-drive. The user runtime-tests the GUI-dependent phases.

**Goal:** Side-load Chrome Web Store extensions into vmux's windowed browse panes, installable via an MCP tool and a `vmux://extensions` manager page, with per-extension action icons + a manage button in the header right of the profile avatars.

**Architecture:** Pure logic (Web Store URL/ID resolve, CRX3 unpack, manifest parse, managed store) lives in `vmux_core` (CEF-free, fast tests). IO + CEF wiring (download, `--load-extension` injection, manager backend, agent path) lives in `vmux_browser`. The manager page + header `ExtensionBar` (wasm Dioxus) live in `vmux_layout`. Transport is the existing rkyv bin-event channel; install runs on a worker thread → outbox → drain → emit, mirroring `vmux://lsp`.

**Tech Stack:** Rust, Bevy ECS, bevy_cef (CEF 148, Chrome bootstrap), Dioxus/WASM, rkyv + serde, `reqwest` (blocking), `zip`.

**Spec:** `docs/specs/2026-06-25-chrome-extensions-design.md`

---

## Crate placement (refines the spec's open item)

- **`vmux_core/src/event/extension.rs`** — rkyv+serde event contract (page↔host) + the inbound request types (also derive Bevy `Message` so the agent path can write them).
- **`vmux_core/src/extension/`** (native, `cfg(not(wasm32))`) — `webstore.rs`, `crx.rs`, `manifest.rs`, `store.rs`. No CEF, no reqwest → `cargo test -p vmux_core` is fast.
- **`vmux_browser/src/extensions.rs` (+ `extensions/`)** — `download.rs`, `install.rs`, `load.rs`, `manager_page.rs`. Reqwest + CEF + ECS observers/systems.
- **`patches/bevy_cef_core-0.5.2/src/browser_process/app.rs`** — append `--load-extension` (vendored patch crate).
- **`vmux_layout/src/extensions_page.rs`** (wasm) + header edits in `vmux_layout/src/page.rs`.
- **`vmux_mcp/src/tools.rs`**, **`vmux_service/src/protocol.rs`**, **`vmux_agent/src/plugin.rs`** — MCP tool + agent fan-out.
- **`vmux_server/src/lib.rs`** — `web_pages!` registration.

Dependencies to add: `vmux_core` (native-gated) `zip = { version = "2", default-features = false, features = ["deflate"] }`, `serde_json = { workspace = true }`; `vmux_browser` `reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }`.

---

## Phase 0 — Feasibility spike (GATE; user runtime-tests)

Everything below assumes this passes. Do this first and stop if it fails.

### Task 0: Prove `--load-extension` runs an extension in a windowed browse pane

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/app.rs:74-106` (`on_before_command_line_processing`)

- [ ] **Step 1: Append the switch from an env var** (mirror the `VMUX_REMOTE_DEBUG_PORT` block at app.rs:97-105). Inside `on_before_command_line_processing`, after the existing `allow-file-access-from-files` line:

```rust
let is_browser_process = process_type
    .map(|p| p.to_string())
    .unwrap_or_default()
    .is_empty();
if is_browser_process {
    if let Ok(dirs) = std::env::var("VMUX_LOAD_EXTENSIONS") {
        if !dirs.is_empty() {
            command_line
                .append_switch_with_value(Some(&"load-extension".into()), Some(&dirs.as_str().into()));
            command_line.append_switch_with_value(
                Some(&"disable-extensions-except".into()),
                Some(&dirs.as_str().into()),
            );
        }
    }
}
```

- [ ] **Step 2: Manually stage one unpacked extension.** Download any small MV3 extension's unpacked dir (e.g. a content-script that recolors pages) to `~/.vmux/extensions/_spike/`. Confirm it has a `manifest.json`.

- [ ] **Step 3: Launch with the env var set** (user runs; do not spawn unbounded `make dev` yourself):

Run: `VMUX_LOAD_EXTENSIONS=$HOME/.vmux/extensions/_spike make dev`
Expected (user confirms): in a **windowed browse pane** (macOS, User/2D mode) the extension's content script visibly runs; navigating a tab to `chrome-extension://<id>/<popup-or-page>.html` renders the extension page.

- [ ] **Step 4: Record the result.** If it works windowed/Chrome-style: proceed to Phase 1. If it does NOT (e.g. extension only loads with Views-based Chrome-style browser views, or not at all): STOP and replan — likely options are (a) create the browse browser as an explicit Chrome-style Views browser, or (b) reduce scope to "install-only, open extension pages manually". Capture findings in the spec's "Open items".

- [ ] **Step 5: Revert the hardcoded env read? No — keep it.** This env hook is the real Phase 2 load mechanism. Leave it; Phase 2 sets `VMUX_LOAD_EXTENSIONS` programmatically.

**Patch-crate hygiene:** `cargo fmt` reformats `patches/` too — after any fmt, `git checkout -- patches/` for unrelated churn and stage only the intended app.rs hunk. Run the patched-crate check: `cargo check -p bevy_cef_core`.

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/browser_process/app.rs
git commit -m "feat(cef): load-extension switch from VMUX_LOAD_EXTENSIONS env"
```

---

## Phase 1 — Core logic in `vmux_core` (CEF-free, full TDD)

### Task 1: Event contract

**Files:**
- Create: `crates/vmux_core/src/event/extension.rs`
- Modify: `crates/vmux_core/src/event.rs` (add `pub mod extension;` / `mod extension;` per the crate's module style — match how `team` is declared)

- [ ] **Step 1: Write the types.** Mirror `event/team.rs` derive set. Inbound request types additionally derive Bevy `Message`.

```rust
use bevy::prelude::Message;
use serde::{Deserialize, Serialize};

pub const EXTENSIONS_LIST_EVENT: &str = "extensions_list";
pub const EXT_INSTALL_PROGRESS_EVENT: &str = "ext_install_progress";
pub const EXT_STATUS_EVENT: &str = "ext_status";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ExtStatus {
    #[default]
    Installing,
    Installed,
    Disabled,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum InstallPhase {
    #[default]
    Resolving,
    Downloading,
    Unpacking,
    Done,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon: Option<String>,
    pub popup: Option<String>,
    pub enabled: bool,
    pub status: ExtStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtensionsEvent {
    pub extensions: Vec<ExtRow>,
    pub pending: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtInstallProgress {
    pub key: String,
    pub phase: InstallPhase,
    pub pct: Option<u8>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtStatusEvent {
    pub id: String,
    pub status: ExtStatus,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Message, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtInstallRequest {
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq, Message, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtToggleRequest {
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Message, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtUninstallRequest {
    pub id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Message, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtActionRequest {
    pub id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Message, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtOpenManagerRequest;

#[derive(Clone, Debug, Default, PartialEq, Message, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExtRelaunchRequest;
```

- [ ] **Step 2: Build.** Run: `cargo check -p vmux_core` → Expected: PASS (if `Message` derive path differs, match the import used by other vmux_core Message types; grep `derive(Message` under crates/vmux_core).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_core/src/event/extension.rs crates/vmux_core/src/event.rs
git commit -m "feat(core): extension event contract"
```

### Task 2: Web Store URL/ID resolver

**Files:**
- Create: `crates/vmux_core/src/extension.rs` (module root: `pub mod webstore; pub mod crx; pub mod manifest; pub mod store;`), gated `#![cfg(not(target_arch = "wasm32"))]` at the include site
- Create: `crates/vmux_core/src/extension/webstore.rs`
- Modify: `crates/vmux_core/src/lib.rs` — add `#[cfg(not(target_arch = "wasm32"))] pub mod extension;`

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_id_from_new_store_url() {
        let id = extension_id("https://chromewebstore.google.com/detail/ublock-origin/cjpalhdlnbpafiamejdnhcphjbkeiagm").unwrap();
        assert_eq!(id, "cjpalhdlnbpafiamejdnhcphjbkeiagm");
    }

    #[test]
    fn extracts_id_from_legacy_url() {
        let id = extension_id("https://chrome.google.com/webstore/detail/foo/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        assert_eq!(id, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn accepts_bare_id() {
        let id = extension_id("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        assert_eq!(id, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    }

    #[test]
    fn rejects_junk() {
        assert!(extension_id("not an extension").is_none());
        assert!(extension_id("https://example.com").is_none());
    }

    #[test]
    fn builds_crx_url() {
        let url = crx_url("cjpalhdlnbpafiamejdnhcphjbkeiagm", "120.0.0.0");
        assert!(url.contains("id%3Dcjpalhdlnbpafiamejdnhcphjbkeiagm"));
        assert!(url.contains("prodversion=120.0.0.0"));
        assert!(url.contains("acceptformat=crx2,crx3"));
    }
}
```

- [ ] **Step 2: Run → FAIL.** Run: `cargo test -p vmux_core extension::webstore -- --nocapture` → Expected: FAIL (unresolved `extension_id`).

- [ ] **Step 3: Implement**

```rust
pub fn extension_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if is_ext_id(trimmed) {
        return Some(trimmed.to_string());
    }
    trimmed
        .split(['/', '?', '#'])
        .find(|seg| is_ext_id(seg))
        .map(|s| s.to_string())
}

fn is_ext_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

pub fn crx_url(id: &str, prodversion: &str) -> String {
    format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&acceptformat=crx2,crx3&prodversion={prodversion}&x=id%3D{id}%26installsource%3Dondemand%26uc"
    )
}
```

- [ ] **Step 4: Run → PASS.** Run: `cargo test -p vmux_core extension::webstore` → Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/extension.rs crates/vmux_core/src/extension/webstore.rs crates/vmux_core/src/lib.rs
git commit -m "feat(core): chrome web store id/url resolver"
```

### Task 3: CRX3 unpacker

**Files:**
- Create: `crates/vmux_core/src/extension/crx.rs`
- Modify: `crates/vmux_core/Cargo.toml` — add native-gated `zip`

- [ ] **Step 1: Add dep.** Under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` in `crates/vmux_core/Cargo.toml`:

```toml
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 2: Failing test** (build a CRX3 in-memory: `Cr24` + version 3 + header_len + dummy header + a real zip containing `manifest.json`).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file("manifest.json", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(br#"{"name":"x","version":"1.0"}"#).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    fn make_crx3(zip: &[u8]) -> Vec<u8> {
        let header = b"fakeheaderbytes";
        let mut out = Vec::new();
        out.extend_from_slice(b"Cr24");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(header);
        out.extend_from_slice(zip);
        out
    }

    #[test]
    fn unpacks_crx3_to_dir() {
        let dir = tempfile::tempdir().unwrap();
        let crx = make_crx3(&make_zip());
        unpack_crx(&crx, dir.path()).unwrap();
        let manifest = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        assert!(manifest.contains("\"version\":\"1.0\""));
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(unpack_crx(b"NOPExxxx", dir.path()).is_err());
    }
}
```

(If `tempfile` isn't already a dev-dep of vmux_core, add it under `[dev-dependencies]`; grep first — it is widely used in the workspace.)

- [ ] **Step 3: Run → FAIL.** Run: `cargo test -p vmux_core extension::crx` → Expected: FAIL.

- [ ] **Step 4: Implement**

```rust
use std::io::Read;
use std::path::Path;

pub fn zip_offset(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"Cr24" {
        return Err("not a crx (bad magic)".into());
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    match version {
        3 => {
            let header_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let off = 12 + header_len;
            if off > bytes.len() {
                return Err("crx3 header length out of range".into());
            }
            Ok(off)
        }
        2 => {
            let pubkey_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let sig_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            Ok(16 + pubkey_len + sig_len)
        }
        v => Err(format!("unsupported crx version {v}")),
    }
}

pub fn unpack_crx(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let off = zip_offset(bytes)?;
    let cursor = std::io::Cursor::new(&bytes[off..]);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = file.enclosed_name() else { continue };
        let out_path = dest.join(name);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(&out_path, buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

- [ ] **Step 5: Run → PASS.** Run: `cargo test -p vmux_core extension::crx` → Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_core/src/extension/crx.rs crates/vmux_core/Cargo.toml
git commit -m "feat(core): crx3 unpacker"
```

### Task 4: Manifest parser

**Files:**
- Create: `crates/vmux_core/src/extension/manifest.rs`

- [ ] **Step 1: Failing tests** (MV3 `action`, MV2 `browser_action`, icon pick, missing action → no icon/popup).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mv3_action() {
        let m = parse(r#"{
            "name": "uBlock", "version": "1.6",
            "action": { "default_popup": "popup.html", "default_icon": { "16": "i16.png", "32": "i32.png" } }
        }"#).unwrap();
        assert_eq!(m.name, "uBlock");
        assert_eq!(m.version, "1.6");
        assert_eq!(m.popup.as_deref(), Some("popup.html"));
        assert_eq!(m.icon.as_deref(), Some("i32.png"));
    }

    #[test]
    fn parses_mv2_browser_action_and_string_icon() {
        let m = parse(r#"{
            "name": "x", "version": "2",
            "browser_action": { "default_popup": "p.html", "default_icon": "icon.png" }
        }"#).unwrap();
        assert_eq!(m.popup.as_deref(), Some("p.html"));
        assert_eq!(m.icon.as_deref(), Some("icon.png"));
    }

    #[test]
    fn no_action_means_no_icon() {
        let m = parse(r#"{ "name": "bg", "version": "1", "icons": { "48": "x.png" } }"#).unwrap();
        assert!(m.popup.is_none());
        assert!(m.icon.is_none());
    }
}
```

- [ ] **Step 2: Run → FAIL.** Run: `cargo test -p vmux_core extension::manifest` → Expected: FAIL.

- [ ] **Step 3: Implement** (an extension gets a header icon only if it declares an action; pick the largest icon ≤ 48, else the largest available).

```rust
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtManifest {
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
}

pub fn parse(json: &str) -> Result<ExtManifest, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let name = v.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let version = v.get("version").and_then(Value::as_str).unwrap_or_default().to_string();
    let action = v.get("action").or_else(|| v.get("browser_action"));
    let popup = action
        .and_then(|a| a.get("default_popup"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let icon = action.and_then(|a| a.get("default_icon")).and_then(pick_icon);
    Ok(ExtManifest { name, version, popup, icon })
}

fn pick_icon(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    let map = v.as_object()?;
    let mut best: Option<(u32, String)> = None;
    for (k, val) in map {
        let (Ok(size), Some(path)) = (k.parse::<u32>(), val.as_str()) else { continue };
        let prefer = size <= 48;
        let better = match &best {
            None => true,
            Some((bsize, _)) => {
                let bprefer = *bsize <= 48;
                match (prefer, bprefer) {
                    (true, false) => true,
                    (false, true) => false,
                    (true, true) => size > *bsize,
                    (false, false) => size < *bsize,
                }
            }
        };
        if better {
            best = Some((size, path.to_string()));
        }
    }
    best.map(|(_, p)| p)
}
```

- [ ] **Step 4: Run → PASS.** Run: `cargo test -p vmux_core extension::manifest` → Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/extension/manifest.rs
git commit -m "feat(core): extension manifest parser"
```

### Task 5: Managed store + index + dirty logic

**Files:**
- Create: `crates/vmux_core/src/extension/store.rs`

- [ ] **Step 1: Failing tests** (index round-trip; enable/disable flips `enabled_dirs`; uninstall removes; dirty when enabled set ≠ loaded set).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, enabled: bool) -> ExtEntry {
        ExtEntry { id: id.into(), name: id.into(), version: "1".into(), popup: None, icon: None, enabled }
    }

    #[test]
    fn index_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.save(dir.path()).unwrap();
        let loaded = Index::load(dir.path()).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries[0].enabled);
    }

    #[test]
    fn enabled_dirs_reflects_toggle() {
        let root = tempfile::tempdir().unwrap();
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        idx.upsert(entry("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false));
        let dirs = idx.enabled_dirs(root.path());
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn dirty_when_enabled_set_differs_from_loaded() {
        let mut idx = Index::default();
        idx.upsert(entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true));
        assert!(idx.is_dirty(&[]));
        assert!(!idx.is_dirty(&["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()]));
    }
}
```

- [ ] **Step 2: Run → FAIL.** Run: `cargo test -p vmux_core extension::store` → Expected: FAIL.

- [ ] **Step 3: Implement** (`~/.vmux/extensions/{<id>/, staging/, index.json}`; `loaded` = the ids that were enabled at the last CEF init, recorded in a sibling `loaded.json` written by Phase 2 at startup).

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub entries: Vec<ExtEntry>,
}

pub fn root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".vmux").join("extensions")
}

impl Index {
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("index.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(root.join("index.json"), s).map_err(|e| e.to_string())
    }

    pub fn upsert(&mut self, e: ExtEntry) {
        if let Some(slot) = self.entries.iter_mut().find(|x| x.id == e.id) {
            *slot = e;
        } else {
            self.entries.push(e);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|x| x.id != id);
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(slot) = self.entries.iter_mut().find(|x| x.id == id) {
            slot.enabled = enabled;
        }
    }

    pub fn enabled_ids(&self) -> Vec<String> {
        self.entries.iter().filter(|e| e.enabled).map(|e| e.id.clone()).collect()
    }

    pub fn enabled_dirs(&self, root: &Path) -> Vec<PathBuf> {
        self.enabled_ids().into_iter().map(|id| root.join(id)).collect()
    }

    pub fn is_dirty(&self, loaded: &[String]) -> bool {
        let mut a = self.enabled_ids();
        let mut b = loaded.to_vec();
        a.sort();
        b.sort();
        a != b
    }
}

pub fn uninstall(root: &Path, id: &str) -> Result<(), String> {
    let dir = root.join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let mut idx = Index::load(root)?;
    idx.remove(id);
    idx.save(root)
}
```

- [ ] **Step 4: Run → PASS.** Run: `cargo test -p vmux_core extension::store` → Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/extension/store.rs
git commit -m "feat(core): extension managed store + index"
```

---

## Phase 2 — `vmux_browser` engine + CEF load wiring

### Task 6: Download + install function

**Files:**
- Create: `crates/vmux_browser/src/extensions.rs` (module root: `mod download; mod install; mod load; mod manager_page;` + `pub use`), declare `mod extensions;` in `crates/vmux_browser/src/lib.rs`
- Create: `crates/vmux_browser/src/extensions/download.rs`
- Create: `crates/vmux_browser/src/extensions/install.rs`
- Modify: `crates/vmux_browser/Cargo.toml` — add `reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }`

- [ ] **Step 1: Download helper** (`download.rs`): blocking GET to a temp file (follow redirects — the CRX endpoint 302s to a CDN; reqwest blocking follows by default).

```rust
use std::io::Write;
use std::path::Path;

pub fn fetch(url: &str, dest: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let mut resp = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 8192];
    let mut got = 0u64;
    loop {
        let n = std::io::Read::read(&mut resp, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        progress(got, total);
    }
    Ok(())
}
```

- [ ] **Step 2: Install function** (`install.rs`): resolve → download → unpack into `staging/<id>` → read manifest → base64 the icon → atomic rename into `<id>/` → update index. Emits phase callbacks. `prodversion` comes from the embedded Chromium version (Task 9 wires the real value; default below is a safe current value).

```rust
use std::path::Path;
use vmux_core::event::extension::{ExtStatus, InstallPhase};
use vmux_core::extension::{crx, manifest, store, webstore};

pub const DEFAULT_PRODVERSION: &str = "120.0.0.0";

pub fn install(source: &str, prodversion: &str, mut progress: impl FnMut(InstallPhase, Option<u8>, &str)) -> Result<store::ExtEntry, String> {
    progress(InstallPhase::Resolving, None, "resolving");
    let id = webstore::extension_id(source).ok_or("not a Chrome Web Store URL or extension id")?;
    let root = store::root();
    let staging = root.join("staging").join(&id);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let crx_path = staging.join("download.crx");
    progress(InstallPhase::Downloading, None, "downloading");
    super::download::fetch(&webstore::crx_url(&id, prodversion), &crx_path, |_, _| {})?;

    progress(InstallPhase::Unpacking, None, "unpacking");
    let bytes = std::fs::read(&crx_path).map_err(|e| e.to_string())?;
    let unpack_dir = staging.join("unpacked");
    crx::unpack_crx(&bytes, &unpack_dir)?;

    let manifest_json = std::fs::read_to_string(unpack_dir.join("manifest.json")).map_err(|e| e.to_string())?;
    let m = manifest::parse(&manifest_json)?;
    let icon = m.icon.as_ref().and_then(|rel| icon_data_url(&unpack_dir, rel));

    let final_dir = root.join(&id);
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&unpack_dir, &final_dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging);

    let entry = store::ExtEntry { id: id.clone(), name: m.name, version: m.version, popup: m.popup, icon, enabled: true };
    let mut idx = store::Index::load(&root)?;
    idx.upsert(entry.clone());
    idx.save(&root)?;
    progress(InstallPhase::Done, Some(100), "done");
    Ok(entry)
}

fn icon_data_url(dir: &Path, rel: &str) -> Option<String> {
    let bytes = std::fs::read(dir.join(rel)).ok()?;
    let mime = if rel.ends_with(".svg") { "image/svg+xml" } else { "image/png" };
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}
```

For `base64_encode`, reuse the workspace base64 crate if present (grep `base64` in Cargo.lock; `base64 = "0.22"` is common). If absent, add `base64 = "0.22"` to vmux_browser and use `base64::engine::general_purpose::STANDARD.encode(bytes)`.

- [ ] **Step 3: Build.** Run: `cargo check -p vmux_browser` → Expected: PASS (warm target dir).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/extensions/download.rs crates/vmux_browser/src/extensions/install.rs crates/vmux_browser/Cargo.toml crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): extension download + install engine"
```

### Task 7: CEF load wiring (set `VMUX_LOAD_EXTENSIONS` at startup) + record loaded set

**Files:**
- Create: `crates/vmux_browser/src/extensions/load.rs`
- Modify: `crates/vmux_browser/src/lib.rs` — call `extensions::load::apply_env()` BEFORE `CefPlugin` is added (CEF reads the env at init; see lib.rs:87 `CefPlugin { .. }`)

- [ ] **Step 1: Implement** (`load.rs`): compute the enabled dirs, set the env var the patch reads (Phase 0), and persist the loaded id set so the manager can compute "dirty/pending".

```rust
use vmux_core::extension::store;

pub fn apply_env() {
    let root = store::root();
    let Ok(idx) = store::Index::load(&root) else { return };
    let dirs: Vec<String> = idx
        .enabled_dirs(&root)
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    if !dirs.is_empty() {
        std::env::set_var("VMUX_LOAD_EXTENSIONS", dirs.join(","));
    }
    let _ = std::fs::write(
        root.join("loaded.json"),
        serde_json::to_string(&idx.enabled_ids()).unwrap_or_else(|_| "[]".into()),
    );
}

pub fn loaded_ids() -> Vec<String> {
    let path = store::root().join("loaded.json");
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
```

- [ ] **Step 2: Wire the call.** In `crates/vmux_browser/src/lib.rs`, immediately before the `CefPlugin { root_cache_path: ..., embedded_hosts, ..default() }` construction (lib.rs:87), add:

```rust
crate::extensions::load::apply_env();
```

(`set_var` must run in the main/browser process before `CefInitialize`; this site precedes the message-loop plugin build.)

- [ ] **Step 3: Build.** Run: `cargo check -p vmux_browser` → Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_browser/src/extensions/load.rs crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): load enabled extensions via env at cef init"
```

---

## Phase 3 — MCP tool + agent path

### Task 8: MCP `browser_install_extension` + `browser_list_extensions`

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs` (enum variant ~`:25`, `to_agent_command` ~`:96`)
- Modify: `crates/vmux_service/src/protocol.rs` (`AgentCommand` enum ~`:54`; add `AgentQuery::ListExtensions` if a query enum exists — grep `enum AgentQuery`)
- Modify: `crates/vmux_agent/src/plugin.rs` (`handle_agent_commands` ~`:545`, arm ~`:583`; writer field ~`:548`)

- [ ] **Step 1: MCP variant** — in `McpParamTool` add:

```rust
#[mcp(
    description = "Install a Chrome extension from the Chrome Web Store (accepts a store URL or a 32-char extension id). Activates after the next vmux relaunch; runs only in windowed browse panes."
)]
BrowserInstallExtension { source: String },
```

- [ ] **Step 2: `to_agent_command` arm**:

```rust
McpParamTool::BrowserInstallExtension { source } => {
    if source.trim().is_empty() {
        return Err("browser_install_extension.source is empty".to_string());
    }
    Ok(AgentCommand::BrowserInstallExtension { source })
}
```

- [ ] **Step 3: `AgentCommand` variant** (protocol.rs):

```rust
BrowserInstallExtension { source: String },
```

- [ ] **Step 4: Agent fan-out** — add a writer field to `handle_agent_commands` and an arm. Field (next to `browser_nav_writer`):

```rust
mut ext_install_writer: MessageWriter<vmux_core::event::extension::ExtInstallRequest>,
```

Arm (in the `match &request.command` block):

```rust
ServiceAgentCommand::BrowserInstallExtension { source } => {
    ext_install_writer.write(vmux_core::event::extension::ExtInstallRequest { source: source.clone() });
    continue;
}
```

- [ ] **Step 5: Register the message** where vmux_agent registers `BrowserNavigateRequest` (plugin.rs ~`:2330` / the plugin `build`): `.add_message::<vmux_core::event::extension::ExtInstallRequest>()`. (If vmux_browser also registers it, registering twice is harmless only if a single owner does it — keep registration in vmux_browser's plugin, Task 10, and have vmux_agent depend on that being present. Simplest: register in vmux_browser plugin only; the message type lives in vmux_core so both crates can name it.)

- [ ] **Step 6: Build.** Run: `cargo check -p vmux_mcp && cargo check -p vmux_agent` → Expected: PASS. (`cargo check -p vmux_service` too.)

- [ ] **Step 7: Schema test** (vmux_mcp has macro schema tests ~tools.rs:1180): assert the tool name + required field.

```rust
#[test]
fn install_extension_tool_schema() {
    let defs = tool_definitions();
    let t = defs.iter().find(|d| d.name == "browser_install_extension").unwrap();
    assert_eq!(t.input_schema["required"][0], "source");
}
```

Run: `cargo test -p vmux_mcp install_extension_tool_schema` → Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs crates/vmux_service/src/protocol.rs crates/vmux_agent/src/plugin.rs
git commit -m "feat(mcp): browser_install_extension tool + agent fan-out"
```

> **Execution note (per "finish then test"):** From here on, the heavy `cargo check -p vmux_browser` / `cargo check -p vmux_layout` (CEF + wasm) builds and ALL runtime testing are **deferred to the single Final Verification pass** (Phase 6). Implement Tasks 9–13 back-to-back; only the fast CEF-free `cargo test -p vmux_core` / `-p vmux_mcp` run inline. Commit per task regardless.

---

## Phase 4 — Manager page `vmux://extensions`

### Task 9: Manager backend (claim + observers + outbox + install system)

**Files:**
- Create: `crates/vmux_browser/src/extensions/manager_page.rs`
- Modify: `crates/vmux_browser/src/extensions.rs` — `pub use manager_page::ExtensionsPlugin;`
- Modify: the vmux_browser plugin `build` (grep `impl Plugin for` in `crates/vmux_browser/src/lib.rs`) — `app.add_plugins(crate::extensions::ExtensionsPlugin);`

- [ ] **Step 1: Implement the plugin** (mirror `vmux_editor/src/lsp/manager_page.rs` structure: `Outbox` resource, `BinEventEmitterPlugin`, observers, `drain_outbox`, plus the install `MessageReader` shared with the agent path). The claim writes `CefPageAttachRequest` exactly like LSP.

```rust
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use vmux_core::event::extension::{
    ExtInstallProgress, ExtInstallRequest, ExtRelaunchRequest, ExtRow, ExtStatus, ExtStatusEvent,
    ExtToggleRequest, ExtUninstallRequest, ExtensionsEvent, EXTENSIONS_LIST_EVENT,
    EXT_INSTALL_PROGRESS_EVENT, EXT_STATUS_EVENT, InstallPhase,
};
use vmux_core::extension::store;
use vmux_core::page::PageManifest;

const PAGE_MANIFEST: PageManifest = PageManifest {
    host: "extensions",
    title: "Extensions",
    keywords: &["extension", "extensions", "chrome", "addon", "install"],
    icon: "puzzle",
    command_bar: true,
};

enum Msg {
    Progress(ExtInstallProgress),
    Status(ExtStatusEvent),
    List(ExtensionsEvent),
}

#[derive(Resource, Default, Clone)]
struct Outbox(Arc<Mutex<Vec<Msg>>>);

pub struct ExtensionsPlugin;

impl Plugin for ExtensionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Outbox>()
            .add_message::<ExtInstallRequest>()
            .add_message::<ExtToggleRequest>()
            .add_message::<ExtUninstallRequest>()
            .add_message::<ExtRelaunchRequest>()
            .add_plugins(bevy_cef::BinEventEmitterPlugin::<(
                ExtInstallRequest,
                ExtToggleRequest,
                ExtUninstallRequest,
                ExtRelaunchRequest,
            )>::default())
            .add_observer(on_bin_install)
            .add_observer(on_bin_toggle)
            .add_observer(on_bin_uninstall)
            .add_systems(
                Update,
                (handle_extensions_page_open.in_set(vmux_core::PageOpenSet::HandleKnownPages),
                 run_installs, run_toggles, run_uninstalls, drain_outbox),
            );
        app.world_mut().spawn(PAGE_MANIFEST);
    }
}
```

(Confirm the exact `BinEventEmitterPlugin` path + `BinReceive` observer signature against `vmux_editor/src/lsp/manager_page.rs:41-50`; mirror precisely. `PageOpenSet` import path matches LSP's `vmux_core` import.)

- [ ] **Step 2: Claim + bin→message bridge + install worker** (real code; mirror LSP's thread+outbox):

```rust
fn handle_extensions_page_open(
    tasks: Query<(Entity, &vmux_core::PageOpenTask), (Without<vmux_core::PageOpenHandled>, Without<vmux_core::PageOpenError>)>,
    mut attach: MessageWriter<vmux_core::CefPageAttachRequest>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url != "vmux://extensions/" {
            continue;
        }
        attach.write(vmux_core::CefPageAttachRequest {
            stack: task.stack,
            url: task.url.clone(),
            title: "Extensions".to_string(),
            bg_color: None,
        });
        commands.entity(entity).insert(vmux_core::PageOpenHandled);
    }
}

fn on_bin_install(ev: On<bevy_cef::BinReceive<ExtInstallRequest>>, mut w: MessageWriter<ExtInstallRequest>) {
    w.write(ev.event().payload.clone());
}
fn on_bin_toggle(ev: On<bevy_cef::BinReceive<ExtToggleRequest>>, mut w: MessageWriter<ExtToggleRequest>) {
    w.write(ev.event().payload.clone());
}
fn on_bin_uninstall(ev: On<bevy_cef::BinReceive<ExtUninstallRequest>>, mut w: MessageWriter<ExtUninstallRequest>) {
    w.write(ev.event().payload.clone());
}

fn run_installs(mut reader: MessageReader<ExtInstallRequest>, outbox: Res<Outbox>) {
    for req in reader.read() {
        let source = req.source.clone();
        let out = outbox.0.clone();
        std::thread::spawn(move || {
            let key = source.clone();
            let push = |m: Msg| out.lock().map(|mut v| v.push(m)).ok();
            let res = super::install::install(&source, super::install::DEFAULT_PRODVERSION, |phase, pct, message| {
                push(Msg::Progress(ExtInstallProgress { key: key.clone(), phase, pct, message: message.to_string() }));
            });
            match res {
                Ok(entry) => {
                    push(Msg::Status(ExtStatusEvent { id: entry.id, status: ExtStatus::Installed, version: Some(entry.version) }));
                    push(Msg::List(snapshot()));
                }
                Err(e) => push(Msg::Progress(ExtInstallProgress { key, phase: InstallPhase::Failed, pct: None, message: e })),
            };
        });
    }
}

fn run_toggles(mut reader: MessageReader<ExtToggleRequest>, outbox: Res<Outbox>) {
    for req in reader.read() {
        let root = store::root();
        if let Ok(mut idx) = store::Index::load(&root) {
            idx.set_enabled(&req.id, req.enabled);
            let _ = idx.save(&root);
        }
        let _ = outbox.0.lock().map(|mut v| v.push(Msg::List(snapshot())));
    }
}

fn run_uninstalls(mut reader: MessageReader<ExtUninstallRequest>, outbox: Res<Outbox>) {
    for req in reader.read() {
        let _ = store::uninstall(&store::root(), &req.id);
        let _ = outbox.0.lock().map(|mut v| v.push(Msg::List(snapshot())));
    }
}

fn snapshot() -> ExtensionsEvent {
    let root = store::root();
    let idx = store::Index::load(&root).unwrap_or_default();
    let loaded = super::load::loaded_ids();
    let extensions = idx.entries.iter().map(|e| ExtRow {
        id: e.id.clone(), name: e.name.clone(), version: e.version.clone(),
        icon: e.icon.clone(), popup: e.popup.clone(), enabled: e.enabled,
        status: if e.enabled { ExtStatus::Installed } else { ExtStatus::Disabled },
    }).collect();
    ExtensionsEvent { extensions, pending: idx.is_dirty(&loaded) }
}
```

- [ ] **Step 3: Drain to the page** (mirror LSP `drain_manager_outbox` at manager_page.rs:275-304; gate on `host_emit_ready`). Emit `ExtInstallProgress`→`EXT_INSTALL_PROGRESS_EVENT`, `ExtStatusEvent`→`EXT_STATUS_EVENT`, `ExtensionsEvent`→`EXTENSIONS_LIST_EVENT` to the `extensions`-host webview entity. (Match the exact `Browsers`/`host_emit_ready`/`BinHostEmitEvent::from_rkyv` calls from LSP.)

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_browser/src/extensions/manager_page.rs crates/vmux_browser/src/extensions.rs crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): vmux://extensions manager backend"
```

### Task 10: Manager page UI (wasm Dioxus) + route registration

**Files:**
- Create: `crates/vmux_layout/src/extensions_page.rs`
- Modify: `crates/vmux_layout/src/lib.rs` — `pub mod extensions_page;` (gate to wasm like the other page modules; match how `command_bar`/`debug_page` are declared)
- Modify: `crates/vmux_server/src/lib.rs:54` — add to `web_pages!`: `render_extensions: "extensions" => vmux_layout::extensions_page::Page,`
- Check: `crates/vmux_server/build.rs` `track_manifest_rel_paths` includes `vmux_layout/src` (it already powers the header page; a new module in vmux_layout needs no new tracking entry — verify).

- [ ] **Step 1: Implement the page** (soft-glass styling per project convention; `try_cef_bin_emit_rkyv` + `use_bin_event_listener` from `vmux_ui::hooks`, mirroring `vmux_editor/src/lsp_page.rs`).

```rust
use dioxus::prelude::*;
use vmux_core::event::extension::{
    ExtInstallRequest, ExtRelaunchRequest, ExtToggleRequest, ExtUninstallRequest, ExtensionsEvent,
    EXTENSIONS_LIST_EVENT,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener};

#[component]
pub fn Page() -> Element {
    let mut state = use_signal(ExtensionsEvent::default);
    let _l = use_bin_event_listener::<ExtensionsEvent, _>(EXTENSIONS_LIST_EVENT, move |d| state.set(d));
    let mut source = use_signal(String::new);

    rsx! {
        div { class: "flex h-full w-full flex-col gap-4 bg-glass p-6",
            div { class: "flex items-center justify-between",
                h1 { class: "text-lg font-semibold", "Extensions" }
                if state().pending {
                    button {
                        class: "rounded-full bg-accent/20 px-3 py-1 text-sm",
                        onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ExtRelaunchRequest); },
                        "Relaunch to apply"
                    }
                }
            }
            div { class: "flex gap-2",
                input {
                    class: "min-w-0 flex-1 rounded-lg bg-glass px-3 py-2 text-sm",
                    placeholder: "Paste Chrome Web Store URL or extension ID",
                    value: "{source}",
                    oninput: move |e| source.set(e.value()),
                }
                button {
                    class: "rounded-lg bg-accent/20 px-4 py-2 text-sm",
                    onclick: move |_| {
                        let s = source();
                        if !s.trim().is_empty() {
                            let _ = try_cef_bin_emit_rkyv(&ExtInstallRequest { source: s });
                            source.set(String::new());
                        }
                    },
                    "Add"
                }
            }
            div { class: "flex flex-col gap-2 overflow-y-auto",
                for ext in state().extensions {
                    div { class: "flex items-center gap-3 rounded-xl bg-glass p-3",
                        if let Some(icon) = ext.icon.clone() {
                            img { class: "h-6 w-6", src: "{icon}" }
                        }
                        div { class: "min-w-0 flex-1",
                            div { class: "truncate text-sm font-medium", "{ext.name}" }
                            div { class: "text-xs opacity-60", "v{ext.version}" }
                        }
                        button {
                            class: "rounded-full bg-glass px-3 py-1 text-xs",
                            onclick: {
                                let id = ext.id.clone();
                                let enabled = ext.enabled;
                                move |_| { let _ = try_cef_bin_emit_rkyv(&ExtToggleRequest { id: id.clone(), enabled: !enabled }); }
                            },
                            if ext.enabled { "On" } else { "Off" }
                        }
                        button {
                            class: "rounded-full bg-glass px-3 py-1 text-xs opacity-70",
                            onclick: {
                                let id = ext.id.clone();
                                move |_| { let _ = try_cef_bin_emit_rkyv(&ExtUninstallRequest { id: id.clone() }); }
                            },
                            "Remove"
                        }
                    }
                }
            }
        }
    }
}
```

(Verify class tokens against existing pages — `bg-glass`/`bg-accent` usage in `vmux_editor/src/lsp_page.rs` / `vmux_team/src/page.rs`. Adjust to whatever the codebase uses.)

- [ ] **Step 2: Stack icon arm.** In `crates/vmux_layout/src/page.rs` `StackIcon` (~`:336`) add an arm for `url.starts_with("vmux://extensions")` (reuse a puzzle SVG) and one for `url.starts_with("chrome-extension://")`.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/extensions_page.rs crates/vmux_layout/src/lib.rs crates/vmux_server/src/lib.rs crates/vmux_layout/src/page.rs
git commit -m "feat(layout): vmux://extensions manager page"
```

---

## Phase 5 — Header surface (icons + puzzle, right of avatars)

### Task 11: Host push of the enabled-extension list + action/manager handlers

**Files:**
- Modify: `crates/vmux_browser/src/extensions/manager_page.rs`

- [ ] **Step 1: Extend the bin tuple + observers** to also receive the header's two events. Change the `BinEventEmitterPlugin::<(...)>` in `ExtensionsPlugin::build` to include `ExtActionRequest` and `ExtOpenManagerRequest`, and add observers:

```rust
.add_observer(on_bin_action)
.add_observer(on_bin_open_manager)
.add_observer(on_bin_relaunch)
```

```rust
use vmux_core::event::extension::{ExtActionRequest, ExtOpenManagerRequest};
use vmux_command::{AppCommand, BrowserCommand, OpenCommand};

fn on_bin_action(ev: On<bevy_cef::BinReceive<ExtActionRequest>>, mut cmd: MessageWriter<AppCommand>) {
    let id = ev.event().payload.id.clone();
    let idx = store::Index::load(&store::root()).unwrap_or_default();
    let Some(entry) = idx.entries.into_iter().find(|e| e.id == id) else { return };
    let Some(popup) = entry.popup else { return };
    cmd.write(AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
        url: Some(format!("chrome-extension://{id}/{popup}")),
    })));
}

fn on_bin_open_manager(_ev: On<bevy_cef::BinReceive<ExtOpenManagerRequest>>, mut cmd: MessageWriter<AppCommand>) {
    cmd.write(AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
        url: Some("vmux://extensions/".to_string()),
    })));
}

fn on_bin_relaunch(_ev: On<bevy_cef::BinReceive<ExtRelaunchRequest>>) {
    // Relaunch: if vmux_desktop exposes a relaunch helper, call it here.
    // v1 fallback: the page banner already instructs the user to restart vmux.
}
```

(Verify `AppCommand`/`BrowserCommand`/`OpenCommand` import path — the first explorer found the team plugin emitting exactly `AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url }))` in `vmux_team/src/plugin.rs`; copy that import. Confirm whether a relaunch helper exists in `crates/vmux_desktop/src/updater.rs`; if yes, call it in `on_bin_relaunch`, else leave the instruction-only fallback and note it in the spec open items.)

- [ ] **Step 2: emit_extensions push system** (mirror `vmux_team/src/plugin.rs:192-251` `emit_team`: push `ExtensionsEvent` to `LayoutCef` webviews on first ready and when changed; gate on `has_browser` + `host_emit_ready`; use a `ExtListSent` marker component). Add the system + marker to `ExtensionsPlugin` and the `Update` set. Reuse `snapshot()` for the payload, emitting to `EXTENSIONS_LIST_EVENT`.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_browser/src/extensions/manager_page.rs
git commit -m "feat(browser): push extension list to header + action/manager handlers"
```

### Task 12: `ExtensionBar` in the header

**Files:**
- Modify: `crates/vmux_layout/src/page.rs` (listener ~`:64`, `HeaderView` props ~`:200`, insert after `TeamFacepile` at ~`:274`)

- [ ] **Step 1: Listen + thread the prop.** Near the team listener (page.rs:64-67):

```rust
let mut ext_state = use_signal(vmux_core::event::extension::ExtensionsEvent::default);
let _ext_listener = use_bin_event_listener::<vmux_core::event::extension::ExtensionsEvent, _>(
    vmux_core::event::extension::EXTENSIONS_LIST_EVENT,
    move |d| ext_state.set(d),
);
```

Pass to `HeaderView` (page.rs:150-ish): `extensions: ext_state().extensions,`. Add the prop to `HeaderView` signature (page.rs:200-208): `extensions: Vec<vmux_core::event::extension::ExtRow>,`.

- [ ] **Step 2: Render the bar** right after `TeamFacepile { members: team }` (page.rs:274):

```rust
ExtensionBar { extensions }
```

```rust
#[component]
fn ExtensionBar(extensions: Vec<vmux_core::event::extension::ExtRow>) -> Element {
    use vmux_core::event::extension::{ExtActionRequest, ExtOpenManagerRequest};
    rsx! {
        div { class: "flex shrink-0 items-center gap-1 pl-2",
            for ext in extensions.iter().filter(|e| e.enabled && e.icon.is_some()) {
                button {
                    class: "flex h-7 w-7 items-center justify-center rounded-lg hover:bg-glass",
                    title: "{ext.name}",
                    onclick: {
                        let id = ext.id.clone();
                        move |_| { let _ = try_cef_bin_emit_rkyv(&ExtActionRequest { id: id.clone() }); }
                    },
                    img { class: "h-4 w-4", src: "{ext.icon.clone().unwrap_or_default()}" }
                }
            }
            button {
                class: "flex h-7 w-7 items-center justify-center rounded-lg hover:bg-glass",
                title: "Manage extensions",
                onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ExtOpenManagerRequest); },
                Icon { class: "h-4 w-4",
                    path { d: "M10 3a2 2 0 1 1 4 0v1h3a1 1 0 0 1 1 1v3h-1a2 2 0 1 0 0 4h1v3a1 1 0 0 1-1 1h-3v-1a2 2 0 1 0-4 0v1H6a1 1 0 0 1-1-1v-3H4a2 2 0 1 0 0-4h1V5a1 1 0 0 1 1-1h4V3z" }
                }
            }
        }
    }
}
```

(`Icon` is already imported at page.rs:10; `try_cef_bin_emit_rkyv` at page.rs:12. Tune classes to match `NavButton`/`TeamFacepile` styling.)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/page.rs
git commit -m "feat(layout): extension action icons + manage button in header"
```

---

## Phase 6 — Final verification (SINGLE pass; per "finish then test")

No per-task checkpoints earlier — do all of this once, here.

### Task 13: Build, lint, automated tests

- [ ] **Step 1: Format, protecting the patch crate.**

```bash
cargo fmt
git checkout -- patches/
git diff --stat
```

(Phase 0's `app.rs` hunk is already committed; `git checkout -- patches/` discards only fmt churn in vendored crates per repo convention. Commit any `crates/` fmt changes.)

- [ ] **Step 2: Fast automated tests** (CEF-free):

```bash
cargo test -p vmux_core extension
cargo test -p vmux_mcp install_extension_tool_schema
```

Expected: all PASS.

- [ ] **Step 3: Full build + clippy** (CEF + wasm; warm target dir):

```bash
cargo build
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: clean. Fix any cfg-gate/import issues (macOS vs Linux; the extension code is cross-platform but CEF windowed-only matters at runtime, not compile).

- [ ] **Step 4: Commit any fixups**

```bash
git add -A
git commit -m "chore: fmt + clippy fixups for chrome extensions"
```

### Task 14: User runtime test (the real validation)

User runs (do not self-spawn `make dev`):

- [ ] Open `vmux://extensions` via the header puzzle button → manager page renders.
- [ ] Paste a Web Store URL (e.g. uBlock Origin) → Add → progress → appears in the list as enabled.
- [ ] MCP: call `browser_install_extension { source: "<url-or-id>" }` from an agent → appears in the list. (Tests the agent path.)
- [ ] "Relaunch to apply" shows; restart vmux.
- [ ] After restart: the extension's action icon appears in the header, right of the avatars.
- [ ] Click the icon → a new tab opens `chrome-extension://<id>/<popup>` and the popup UI renders.
- [ ] Functionality works in a **windowed browse pane** (e.g. uBlock blocks ads); a **3D/OSR pane** shows no extension (expected).
- [ ] Toggle off + relaunch → icon gone, extension inactive. Remove → dir gone from `~/.vmux/extensions/`.

---

## Self-review

**Spec coverage:**
- MCP installer → Task 8. ✓
- Web Store source (URL/ID → CRX → CRX3 unpack → managed dir) → Tasks 2,3,6. ✓
- Managed store + index → Task 5. ✓
- CEF `--load-extension` at init → Tasks 0,7. ✓
- Top-right icons + puzzle, popup-in-page, manager page → Tasks 9,10,11,12. ✓
- Relaunch (install≠activate) → Tasks 7,10,11 (instruction-only fallback flagged). ✓
- Constraints (windowed-only, no auto-update, install-by-URL/ID) → enforced by design; verified in Task 14. ✓

**Placeholder scan:** no TBD/TODO. Conditionals (relaunch helper existence; exact `bevy_cef` `BinReceive`/`BinHostEmitEvent` signatures; `bg-glass`/`bg-accent` class tokens) are marked "verify against <exact file>" with the reference path — resolved by reading the cited file during impl, not deferred decisions.

**Type consistency:** `ExtInstallRequest`/`ExtToggleRequest`/`ExtUninstallRequest`/`ExtActionRequest`/`ExtOpenManagerRequest`/`ExtRelaunchRequest` (vmux_core, derive `Message`) used identically in Tasks 1,8,9,11,12. `ExtensionsEvent { extensions, pending }`, `ExtRow`, `ExtStatus`, `InstallPhase`, `ExtInstallProgress`, `ExtStatusEvent`, event-name consts consistent across Tasks 1,9,10,11,12. `store::Index` API (`upsert`/`set_enabled`/`remove`/`enabled_ids`/`enabled_dirs`/`is_dirty`/`load`/`save`, `store::root`, `store::uninstall`) consistent across Tasks 5,7,9. `webstore::extension_id`/`crx_url`, `crx::unpack_crx`, `manifest::parse` consistent across Tasks 2,3,4,6.

## Open items to resolve during impl (cited inline)

- Phase 0 spike result (windowed Chrome-style loading) — gates everything.
- Exact `bevy_cef` `BinEventEmitterPlugin` / `BinReceive` / `BinHostEmitEvent` signatures — mirror `vmux_editor/src/lsp/manager_page.rs`.
- Relaunch helper in `vmux_desktop/src/updater.rs` (else instruction-only).
- `base64` crate availability (else add to vmux_browser).
- Real `prodversion` from the embedded Chromium version (replace `DEFAULT_PRODVERSION`).
- soft-glass class tokens vs existing pages.
