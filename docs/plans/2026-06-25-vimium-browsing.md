# Vimium-like Browsing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add keyboard-driven web navigation (link hints, scroll, history, reload, find, open) to vmux browser pages via an in-page content script authored in Rust→WASM.

**Architecture:** A new `crates/vmux_vimium` cdylib compiles to WASM (`web-sys`/`wasm-bindgen`) and implements the whole content script (modes, hints, scroll, find, open). Its own `build.rs` builds itself for `wasm32`, runs wasm-bindgen, base64-embeds the result into a self-contained `vimium_preload.js` bootstrap, and exposes it to the host via `vmux_vimium::preload_script()`. A host system in `vmux_browser` sets the `PreloadScripts` component on http/https browser tabs (before `CefSystems::CreateAndResize`), so CEF evals the bootstrap at `on_context_created`. Zero page→host IPC.

**Tech Stack:** Rust (edition 2024), `wasm-bindgen` 0.2, `web-sys`, `js-sys`, `wasm-bindgen-cli-support` (build-dep), Bevy 0.19-rc, `bevy_cef` (patched).

---

## Prerequisites

- `rustup target add wasm32-unknown-unknown` must be available in dev + CI. Add a step to CI if not already present.
- All edits happen in the `.worktrees/vimium` worktree (branch `feat/vimium-browsing`). Paths below are repo-relative to that worktree root.

## File Structure

New crate `crates/vmux_vimium/`:
- `Cargo.toml` — `crate-type = ["cdylib", "rlib"]`; wasm-only deps gated by `cfg(target_arch="wasm32")`; build-dep `wasm-bindgen-cli-support`.
- `build.rs` — host-target only: build self→wasm into an isolated target dir, run wasm-bindgen (no-modules), base64 the wasm, template `vimium_preload.js` into `OUT_DIR`.
- `src/lib.rs` — host: `preload_script()`. wasm: `#[wasm_bindgen(start)] start()` + wiring. Pure-logic re-exports.
- `src/keymap.rs` — pure: `Key`, `Action`, `KeySeq` matcher (handles `gg`). Host-testable.
- `src/mode.rs` — pure: `Mode` (Normal/Insert) + transition logic. Host-testable.
- `src/hints.rs` — wasm: clickable enumeration, label gen (pure part testable), overlay, filter, activate.
- `src/scroll.rs` — wasm: scroll actions.
- `src/find.rs` — wasm: find overlay + match nav.
- `src/openbar.rs` — wasm: URL/search overlay → current-tab navigation.
- `src/overlay.rs` — wasm: shared shadow-root container helpers.

Modified:
- `crates/vmux_browser/Cargo.toml` — add `vmux_vimium` dependency.
- `crates/vmux_browser/src/lib.rs` — register `vimium::set_vimium_preload` system before `CefSystems::CreateAndResize`.
- `crates/vmux_browser/src/vimium.rs` (new) — the host system + tests.
- `crates/vmux_setting/src/plugin/runtime.rs` — add `vimium_enabled` to `BrowserSettings`.
- `crates/vmux_setting/src/settings.ron` — add the field under `browser`.

No root `Cargo.toml` change needed: `members = ["crates/*"]` already globs the new crate.

---

## Task 1: Scaffold crate + build pipeline + CSP spike (GATING)

This task de-risks the entire approach: it proves a Rust→WASM content script can be built, embedded, injected, and **instantiated under a strict CSP**. Everything else depends on it. Do not proceed to Task 2 until the marker is confirmed on github.com.

**Files:**
- Create: `crates/vmux_vimium/Cargo.toml`, `crates/vmux_vimium/build.rs`, `crates/vmux_vimium/src/lib.rs`
- Modify: `crates/vmux_browser/Cargo.toml`, `crates/vmux_browser/src/lib.rs`
- Create: `crates/vmux_browser/src/vimium.rs`

- [ ] **Step 1: Create the crate manifest**

`crates/vmux_vimium/Cargo.toml`:

```toml
[package]
name = "vmux_vimium"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = { workspace = true }
js-sys = "0.3"
web-sys = { version = "0.3", features = [
    "Window", "Document", "Element", "HtmlElement", "HtmlInputElement",
    "Node", "NodeList", "DomRect", "CssStyleDeclaration", "ShadowRoot",
    "ShadowRootInit", "ShadowRootMode", "KeyboardEvent", "FocusEvent",
    "Event", "EventTarget", "Location", "History", "DomTokenList",
    "HtmlCollection", "Text",
] }

[build-dependencies]
wasm-bindgen-cli-support = "0.2"
base64 = "0.22"
```

- [ ] **Step 2: Write the build script**

`crates/vmux_vimium/build.rs`:

```rust
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    // When cargo is building this crate FOR wasm (either top-level or our inner
    // invocation), do nothing — only the host build orchestrates embedding.
    if target.contains("wasm32") {
        return;
    }
    // Recursion guard for the inner wasm build we spawn below.
    if env::var("VMUX_VIMIUM_INNER_WASM").is_ok() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");

    // 1. Build this crate for wasm into an isolated target dir (no lock contention
    //    with the outer host build, and wasm-only so no CEF absolute-path issues).
    let wasm_target_dir = out_dir.join("wasm-build");
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "build",
            "-p",
            "vmux_vimium",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--target-dir",
        ])
        .arg(&wasm_target_dir)
        .env("VMUX_VIMIUM_INNER_WASM", "1")
        .status()
        .expect("spawn cargo wasm build");
    assert!(status.success(), "vmux_vimium wasm build failed");

    let wasm_in = wasm_target_dir
        .join("wasm32-unknown-unknown/release/vmux_vimium.wasm");

    // 2. Run wasm-bindgen (no-modules) into OUT_DIR.
    let bindgen_out = out_dir.join("bindgen");
    fs::create_dir_all(&bindgen_out).unwrap();
    let mut b = wasm_bindgen_cli_support::Bindgen::new();
    b.input_path(&wasm_in)
        .no_modules(true)
        .unwrap()
        .typescript(false);
    b.generate(&bindgen_out).expect("wasm-bindgen generate");

    let glue = fs::read_to_string(bindgen_out.join("vmux_vimium.js")).unwrap();
    let wasm_bytes = fs::read(bindgen_out.join("vmux_vimium_bg.wasm")).unwrap();
    let wasm_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &wasm_bytes,
    );

    // 3. Template the self-contained bootstrap.
    let template = fs::read_to_string(manifest_dir.join("src/preload.js.tmpl")).unwrap();
    let preload = template
        .replace("%%GLUE_JS%%", &glue)
        .replace("%%WASM_B64%%", &wasm_b64);
    fs::write(out_dir.join("vimium_preload.js"), preload).unwrap();
}
```

- [ ] **Step 3: Write the bootstrap template**

`crates/vmux_vimium/src/preload.js.tmpl`:

```js
(function () {
  try {
    if (window.top !== window) return;
    var p = location.protocol;
    if (p !== "http:" && p !== "https:") return;
    if (window.__vmuxVimium) return;
    window.__vmuxVimium = true;
    %%GLUE_JS%%
    var b64 = "%%WASM_B64%%";
    var bin = atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    wasm_bindgen(bytes).then(function () { wasm_bindgen.start(); });
  } catch (e) {
    if (window.console) console.warn("[vmux-vimium] init failed", e);
  }
})();
```

- [ ] **Step 4: Write the minimal spike lib (host API + wasm marker)**

`crates/vmux_vimium/src/lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub fn preload_script() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/vimium_preload.js"))
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    let document = web_sys::window().unwrap().document().unwrap();
    let marker = document.create_element("div").unwrap();
    marker.set_id("__vmux_vimium_marker");
    let html: web_sys::HtmlElement = marker.dyn_into().unwrap();
    html.set_inner_text("vmux vimium ok");
    let style = html.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("bottom", "8px");
    let _ = style.set_property("right", "8px");
    let _ = style.set_property("z-index", "2147483647");
    let _ = style.set_property("background", "rgba(0,0,0,0.8)");
    let _ = style.set_property("color", "#0f0");
    let _ = style.set_property("font", "12px monospace");
    let _ = style.set_property("padding", "2px 6px");
    let _ = style.set_property("border-radius", "4px");
    if let Some(body) = document.body() {
        let _ = body.append_child(&html);
    }
}
```

- [ ] **Step 5: Add the host integration system**

`crates/vmux_browser/src/vimium.rs`:

```rust
use bevy::prelude::*;
use bevy_cef::prelude::CefSystems;
use bevy_cef::prelude::PreloadScripts;
use bevy_cef_core::prelude::ResolvedWebviewUri;
use vmux_layout::{Browser, LayoutCef};
use vmux_setting::AppSettings;

pub struct VimiumPlugin;

impl Plugin for VimiumPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            set_vimium_preload.before(CefSystems::CreateAndResize),
        );
    }
}

fn is_web_scheme(uri: &str) -> bool {
    uri.starts_with("http://") || uri.starts_with("https://")
}

fn set_vimium_preload(
    settings: Option<Res<AppSettings>>,
    mut commands: Commands,
    new_pages: Query<
        (Entity, &ResolvedWebviewUri),
        (Added<ResolvedWebviewUri>, With<Browser>, Without<LayoutCef>),
    >,
) {
    let enabled = settings.map(|s| s.browser.vimium_enabled).unwrap_or(true);
    if !enabled {
        return;
    }
    for (entity, uri) in new_pages.iter() {
        if !is_web_scheme(&uri.0) {
            continue;
        }
        commands
            .entity(entity)
            .insert(PreloadScripts(vec![vmux_vimium::preload_script().to_string()]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_scheme_detection() {
        assert!(is_web_scheme("https://example.com"));
        assert!(is_web_scheme("http://example.com"));
        assert!(!is_web_scheme("vmux://history/"));
        assert!(!is_web_scheme("file:///tmp/x"));
        assert!(!is_web_scheme("data:text/html,x"));
    }
}
```

- [ ] **Step 6: Wire the dependency + plugin**

In `crates/vmux_browser/Cargo.toml` add under `[dependencies]`:

```toml
vmux_vimium = { path = "../vmux_vimium" }
```

In `crates/vmux_browser/src/lib.rs`: add `mod vimium;` near the other module declarations, and add `.add_plugins(vimium::VimiumPlugin)` to the `BrowserPlugin` build (alongside the other `add_plugins`/`add_systems` calls). Confirm `vmux_browser` already depends on `vmux_setting` (it uses `resolve_startup_url`); if `AppSettings` is not re-exported, import it from its actual path (`vmux_setting::plugin::runtime::AppSettings` or the crate re-export).

- [ ] **Step 7: Add the `vimium_enabled` setting**

In `crates/vmux_setting/src/plugin/runtime.rs`, extend `BrowserSettings` (currently at the `struct BrowserSettings` near line 276):

```rust
pub struct BrowserSettings {
    #[serde(default = "default_browser_startup_url")]
    pub startup_url: String,
    #[serde(default = "default_vimium_enabled")]
    pub vimium_enabled: bool,
}

fn default_vimium_enabled() -> bool {
    true
}
```

Update `default_browser_settings()` to set `vimium_enabled: default_vimium_enabled()`. In `crates/vmux_setting/src/settings.ron`, add `vimium_enabled: true` inside the `browser(...)` block (do not seed other keys — per the no-config-auto-seed rule, only add this explicit field).

- [ ] **Step 8: Run host tests**

Run: `cargo test -p vmux_browser vimium`
Expected: `web_scheme_detection` passes.

Run: `cargo build -p vmux_vimium` (host build; exercises build.rs → inner wasm build → bindgen → template).
Expected: builds; `target/.../build/vmux_vimium-*/out/vimium_preload.js` exists and contains `wasm_bindgen` + a long base64 string.

- [ ] **Step 9: CSP spike — manual verification**

Build and run the app (windowed/User mode). Navigate a browser tab to:
1. `https://example.com` → green `vmux vimium ok` marker appears bottom-right.
2. `https://github.com` (strict CSP) → marker appears.

If (2) fails (CSP blocks `WebAssembly.instantiate`): STOP and report. Fallback options in priority: (a) try instantiating via a Blob/streaming path; (b) fall back to a JS implementation. The rest of the plan assumes (2) passes.

- [ ] **Step 10: Commit**

```bash
git add crates/vmux_vimium crates/vmux_browser/Cargo.toml crates/vmux_browser/src/vimium.rs crates/vmux_browser/src/lib.rs crates/vmux_setting/src/plugin/runtime.rs crates/vmux_setting/src/settings.ron
git commit -m "feat(vimium): scaffold wasm content-script crate + injection + CSP spike"
```

---

## Task 2: Mode state machine + keymap matcher (pure logic, TDD)

Pure Rust, no DOM — compiles and tests on the host target. This is the brain that the wasm keyboard listener (Task 3) drives.

**Files:**
- Create: `crates/vmux_vimium/src/mode.rs`, `crates/vmux_vimium/src/keymap.rs`
- Modify: `crates/vmux_vimium/src/lib.rs` (declare modules)

- [ ] **Step 1: Write failing tests for mode transitions**

`crates/vmux_vimium/src/mode.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

/// Whether the currently focused element should force Insert mode.
pub fn editable_tag_forces_insert(tag: &str, content_editable: bool) -> bool {
    if content_editable {
        return true;
    }
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "input" | "textarea" | "select"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_force_insert() {
        assert!(editable_tag_forces_insert("INPUT", false));
        assert!(editable_tag_forces_insert("textarea", false));
        assert!(editable_tag_forces_insert("select", false));
    }

    #[test]
    fn contenteditable_forces_insert() {
        assert!(editable_tag_forces_insert("div", true));
    }

    #[test]
    fn plain_elements_do_not_force_insert() {
        assert!(!editable_tag_forces_insert("div", false));
        assert!(!editable_tag_forces_insert("a", false));
        assert!(!editable_tag_forces_insert("body", false));
    }
}
```

- [ ] **Step 2: Run the tests, expect FAIL (module not declared)**

Run: `cargo test -p vmux_vimium mode::`
Expected: compile error until Step 5 declares the module.

- [ ] **Step 3: Write failing tests for the keymap matcher**

`crates/vmux_vimium/src/keymap.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Hints,
    ScrollDownLine,
    ScrollUpLine,
    ScrollDownHalf,
    ScrollUpHalf,
    ScrollTop,
    ScrollBottom,
    HistoryBack,
    HistoryForward,
    Reload,
    OpenFind,
    FindNext,
    FindPrev,
    OpenBar,
    EnterInsert,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    /// A complete action fired.
    Action(Action),
    /// The key is a prefix of a multi-key sequence; wait for more.
    Pending,
    /// No binding; key is not handled by vimium.
    None,
}

/// Stateful matcher for normal-mode keys. Tracks one pending prefix (`g`).
#[derive(Default)]
pub struct Matcher {
    pending_g: bool,
}

impl Matcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a single key (the `KeyboardEvent.key` value). `shift` indicates the
    /// shift modifier (used to distinguish `G` from `g`, `H`/`L`, `N`).
    pub fn feed(&mut self, key: &str) -> MatchResult {
        if self.pending_g {
            self.pending_g = false;
            return match key {
                "g" => MatchResult::Action(Action::ScrollTop),
                _ => MatchResult::None,
            };
        }
        match key {
            "f" => MatchResult::Action(Action::Hints),
            "j" => MatchResult::Action(Action::ScrollDownLine),
            "k" => MatchResult::Action(Action::ScrollUpLine),
            "d" => MatchResult::Action(Action::ScrollDownHalf),
            "u" => MatchResult::Action(Action::ScrollUpHalf),
            "g" => {
                self.pending_g = true;
                MatchResult::Pending
            }
            "G" => MatchResult::Action(Action::ScrollBottom),
            "H" => MatchResult::Action(Action::HistoryBack),
            "L" => MatchResult::Action(Action::HistoryForward),
            "r" => MatchResult::Action(Action::Reload),
            "/" => MatchResult::Action(Action::OpenFind),
            "n" => MatchResult::Action(Action::FindNext),
            "N" => MatchResult::Action(Action::FindPrev),
            "o" => MatchResult::Action(Action::OpenBar),
            "i" => MatchResult::Action(Action::EnterInsert),
            "Escape" => MatchResult::Action(Action::Escape),
            _ => MatchResult::None,
        }
    }

    /// Called when the pending-prefix timeout elapses; clears `g`.
    pub fn clear_pending(&mut self) {
        self.pending_g = false;
    }

    pub fn has_pending(&self) -> bool {
        self.pending_g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_keys_map_to_actions() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("f"), MatchResult::Action(Action::Hints));
        assert_eq!(m.feed("j"), MatchResult::Action(Action::ScrollDownLine));
        assert_eq!(m.feed("G"), MatchResult::Action(Action::ScrollBottom));
        assert_eq!(m.feed("H"), MatchResult::Action(Action::HistoryBack));
        assert_eq!(m.feed("/"), MatchResult::Action(Action::OpenFind));
        assert_eq!(m.feed("o"), MatchResult::Action(Action::OpenBar));
    }

    #[test]
    fn gg_scrolls_to_top() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("g"), MatchResult::Pending);
        assert!(m.has_pending());
        assert_eq!(m.feed("g"), MatchResult::Action(Action::ScrollTop));
        assert!(!m.has_pending());
    }

    #[test]
    fn g_then_other_key_cancels() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("g"), MatchResult::Pending);
        assert_eq!(m.feed("x"), MatchResult::None);
        assert!(!m.has_pending());
    }

    #[test]
    fn timeout_clears_pending_g() {
        let mut m = Matcher::new();
        m.feed("g");
        m.clear_pending();
        assert!(!m.has_pending());
        assert_eq!(m.feed("g"), MatchResult::Pending);
    }

    #[test]
    fn unbound_keys_are_none() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("q"), MatchResult::None);
        assert_eq!(m.feed("1"), MatchResult::None);
    }
}
```

- [ ] **Step 4: Run tests, expect FAIL (module not declared)**

Run: `cargo test -p vmux_vimium keymap::`
Expected: compile error until Step 5.

- [ ] **Step 5: Declare the modules**

In `crates/vmux_vimium/src/lib.rs`, add at the top (these compile on every target — they are DOM-free):

```rust
pub mod keymap;
pub mod mode;
```

- [ ] **Step 6: Run tests, expect PASS**

Run: `cargo test -p vmux_vimium keymap:: mode::`
Expected: all pass (8 tests).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_vimium/src/keymap.rs crates/vmux_vimium/src/mode.rs crates/vmux_vimium/src/lib.rs
git commit -m "feat(vimium): pure mode state machine + keymap matcher with tests"
```

---

## Task 3: Keyboard listener + insert-mode detection (wasm)

Wire the matcher into a real capturing `keydown` listener; auto-detect insert mode from the focused element. Actions are stubbed (log to console) until Tasks 4–7 fill them in. This task is verified manually (DOM behavior) plus a host-testable helper.

**Files:**
- Create: `crates/vmux_vimium/src/runtime.rs` (wasm glue: listener, dispatch, shared state)
- Modify: `crates/vmux_vimium/src/lib.rs` (replace spike `start()` with real wiring)

- [ ] **Step 1: Add a host-testable focus helper to `mode.rs`**

Append to `crates/vmux_vimium/src/mode.rs` (above `#[cfg(test)]`):

```rust
/// Decide the mode given the focused element's tag + contenteditable, and
/// whether the user explicitly pressed `i` (force_insert) or `Esc`
/// (force_normal). Explicit signals win over focus.
pub fn resolve_mode(
    focused_tag: Option<&str>,
    content_editable: bool,
    force_insert: bool,
    force_normal: bool,
) -> Mode {
    if force_normal {
        return Mode::Normal;
    }
    if force_insert {
        return Mode::Insert;
    }
    match focused_tag {
        Some(tag) if editable_tag_forces_insert(tag, content_editable) => Mode::Insert,
        _ => Mode::Normal,
    }
}
```

Add tests in the same `mod tests`:

```rust
    #[test]
    fn focus_on_input_is_insert() {
        assert_eq!(resolve_mode(Some("input"), false, false, false), Mode::Insert);
    }

    #[test]
    fn escape_forces_normal_even_in_input() {
        assert_eq!(resolve_mode(Some("input"), false, false, true), Mode::Normal);
    }

    #[test]
    fn i_forces_insert_on_plain_element() {
        assert_eq!(resolve_mode(Some("div"), false, true, false), Mode::Insert);
    }
```

Run: `cargo test -p vmux_vimium mode::` → expect PASS.

- [ ] **Step 2: Write the wasm runtime glue**

`crates/vmux_vimium/src/runtime.rs` (entire file gated wasm-only by lib.rs cfg):

```rust
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, KeyboardEvent};

use crate::keymap::{Action, MatchResult, Matcher};
use crate::mode::{resolve_mode, Mode};

thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::new());
    static FORCE_INSERT: RefCell<bool> = RefCell::new(false);
}

fn document() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

fn focused_tag_and_editable(doc: &Document) -> (Option<String>, bool) {
    match doc.active_element() {
        Some(el) => {
            let tag = el.tag_name();
            let editable = el
                .dyn_ref::<web_sys::HtmlElement>()
                .map(|h| h.is_content_editable())
                .unwrap_or(false);
            (Some(tag), editable)
        }
        None => (None, false),
    }
}

fn current_mode(doc: &Document) -> Mode {
    let (tag, editable) = focused_tag_and_editable(doc);
    let force_insert = FORCE_INSERT.with(|f| *f.borrow());
    resolve_mode(tag.as_deref(), editable, force_insert, false)
}

pub fn install() {
    let doc = document();
    let handler = Closure::<dyn FnMut(KeyboardEvent)>::new(move |ev: KeyboardEvent| {
        on_keydown(ev);
    });
    // Capture phase so we see keys before the page does.
    doc.add_event_listener_with_callback_and_bool(
        "keydown",
        handler.as_ref().unchecked_ref(),
        true,
    )
    .unwrap();
    handler.forget();
}

fn on_keydown(ev: KeyboardEvent) {
    if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
        return; // never shadow browser/OS/vmux shortcuts
    }
    let doc = document();
    let key = ev.key();

    if current_mode(&doc) == Mode::Insert {
        // Only Escape is meaningful in insert mode.
        if key == "Escape" {
            FORCE_INSERT.with(|f| *f.borrow_mut() = false);
            if let Some(el) = doc.active_element() {
                if let Some(h) = el.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = h.blur();
                }
            }
        }
        return;
    }

    let result = MATCHER.with(|m| m.borrow_mut().feed(&key));
    match result {
        MatchResult::Action(action) => {
            ev.prevent_default();
            ev.stop_propagation();
            dispatch(action, &doc);
        }
        MatchResult::Pending => {
            ev.prevent_default();
            ev.stop_propagation();
            // (timeout-based clear added with no behavioral dependency; the next
            // key resolves or cancels the prefix anyway.)
        }
        MatchResult::None => {}
    }
}

fn dispatch(action: Action, _doc: &Document) {
    match action {
        Action::EnterInsert => FORCE_INSERT.with(|f| *f.borrow_mut() = true),
        // Tasks 4–7 replace these stubs.
        other => web_sys::console::log_1(&format!("[vmux-vimium] {:?}", other).into()),
    }
}
```

- [ ] **Step 3: Rewrite `start()` to use the runtime**

Replace the spike `start()` in `crates/vmux_vimium/src/lib.rs` with:

```rust
#[cfg(target_arch = "wasm32")]
mod runtime;
#[cfg(target_arch = "wasm32")]
mod overlay;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    runtime::install();
}
```

Create a placeholder `crates/vmux_vimium/src/overlay.rs` (filled in Task 5):

```rust
#![allow(dead_code)]
```

- [ ] **Step 4: Build wasm to verify it compiles**

Run: `cargo build -p vmux_vimium --target wasm32-unknown-unknown`
Expected: compiles (warnings about unused `dispatch` arms are fine).

- [ ] **Step 5: Manual verify**

Rebuild + run the app. On a normal site:
- Press `j`/`k`/`f`/`G`/`o` outside any text field → console logs `[vmux-vimium] ScrollDownLine` etc.
- Click into a search box / focus an `<input>` → typing letters works normally (no logs); `Esc` blurs.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_vimium/src/runtime.rs crates/vmux_vimium/src/overlay.rs crates/vmux_vimium/src/mode.rs crates/vmux_vimium/src/lib.rs
git commit -m "feat(vimium): capturing keydown listener + auto insert-mode detection"
```

---

## Task 4: Scroll + reload + history actions (wasm)

Fill in the simplest actions. All use BOM/DOM APIs — no overlay needed.

**Files:**
- Create: `crates/vmux_vimium/src/scroll.rs`
- Modify: `crates/vmux_vimium/src/runtime.rs` (dispatch), `crates/vmux_vimium/src/lib.rs` (declare module)

- [ ] **Step 1: Add a host-testable scroll-amount helper**

`crates/vmux_vimium/src/scroll.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollKind {
    Line,
    Half,
}

pub const LINE_PX: f64 = 60.0;

/// Pixel delta for a vertical scroll. `down` negates sign. `viewport_h` used for
/// half-page.
pub fn scroll_delta(kind: ScrollKind, down: bool, viewport_h: f64) -> f64 {
    let mag = match kind {
        ScrollKind::Line => LINE_PX,
        ScrollKind::Half => viewport_h / 2.0,
    };
    if down {
        mag
    } else {
        -mag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_scroll_is_fixed() {
        assert_eq!(scroll_delta(ScrollKind::Line, true, 800.0), 60.0);
        assert_eq!(scroll_delta(ScrollKind::Line, false, 800.0), -60.0);
    }

    #[test]
    fn half_scroll_uses_viewport() {
        assert_eq!(scroll_delta(ScrollKind::Half, true, 800.0), 400.0);
        assert_eq!(scroll_delta(ScrollKind::Half, false, 800.0), -400.0);
    }
}
```

Run: `cargo test -p vmux_vimium scroll::` → expect PASS.

- [ ] **Step 2: Declare module + wire dispatch (wasm)**

In `crates/vmux_vimium/src/lib.rs` add `pub mod scroll;` (DOM-free, all targets).

Replace the `dispatch` fn in `crates/vmux_vimium/src/runtime.rs`:

```rust
use crate::scroll::{scroll_delta, ScrollKind};

fn dispatch(action: Action, doc: &Document) {
    let win = web_sys::window().unwrap();
    let vh = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
    match action {
        Action::EnterInsert => FORCE_INSERT.with(|f| *f.borrow_mut() = true),
        Action::ScrollDownLine => scroll_by(&win, scroll_delta(ScrollKind::Line, true, vh)),
        Action::ScrollUpLine => scroll_by(&win, scroll_delta(ScrollKind::Line, false, vh)),
        Action::ScrollDownHalf => scroll_by(&win, scroll_delta(ScrollKind::Half, true, vh)),
        Action::ScrollUpHalf => scroll_by(&win, scroll_delta(ScrollKind::Half, false, vh)),
        Action::ScrollTop => win.scroll_to_with_x_and_y(0.0, 0.0),
        Action::ScrollBottom => {
            let h = doc
                .document_element()
                .map(|e| e.scroll_height() as f64)
                .unwrap_or(0.0);
            win.scroll_to_with_x_and_y(0.0, h);
        }
        Action::HistoryBack => {
            let _ = win.history().and_then(|h| h.back());
        }
        Action::HistoryForward => {
            let _ = win.history().and_then(|h| h.forward());
        }
        Action::Reload => {
            let _ = win.location().reload();
        }
        // Tasks 5–7 replace these.
        other => web_sys::console::log_1(&format!("[vmux-vimium] {:?}", other).into()),
    }
}

fn scroll_by(win: &web_sys::Window, dy: f64) {
    win.scroll_by_with_x_and_y(0.0, dy);
}
```

Add to the `web-sys` features in `Cargo.toml` if missing: `"ScrollToOptions"` is not needed; ensure `Window`, `History`, `Location` are present (they are).

- [ ] **Step 3: Build wasm**

Run: `cargo build -p vmux_vimium --target wasm32-unknown-unknown`
Expected: compiles.

- [ ] **Step 4: Manual verify**

`j`/`k` scroll a line, `d`/`u` half-page, `gg` top, `G` bottom, `H`/`L` navigate history, `r` reloads.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_vimium/src/scroll.rs crates/vmux_vimium/src/runtime.rs crates/vmux_vimium/src/lib.rs
git commit -m "feat(vimium): scroll, reload, history actions"
```

---

## Task 5: Link hints (`f`)

Enumerate visible clickables, render labels in a shadow root, filter by typed letters, activate on unique match. Label generation is pure + tested; DOM parts are wasm.

**Files:**
- Create: `crates/vmux_vimium/src/hints.rs`
- Modify: `crates/vmux_vimium/src/overlay.rs`, `crates/vmux_vimium/src/runtime.rs`, `crates/vmux_vimium/src/lib.rs`

- [ ] **Step 1: Pure label generation + tests**

`crates/vmux_vimium/src/hints.rs`:

```rust
const ALPHABET: &[u8] = b"sadfjklewcmpgh";

/// Generate `count` unique hint labels using a fixed alphabet. For small counts
/// labels are single chars; once the alphabet is exhausted they become two-char
/// combinations. Labels never collide and no label is a prefix of another within
/// the same length tier.
pub fn generate_labels(count: usize) -> Vec<String> {
    let n = ALPHABET.len();
    if count == 0 {
        return Vec::new();
    }
    if count <= n {
        return (0..count)
            .map(|i| (ALPHABET[i] as char).to_string())
            .collect();
    }
    // Two-character labels: enough for n*n targets.
    let mut out = Vec::with_capacity(count);
    'outer: for a in 0..n {
        for b in 0..n {
            out.push(format!("{}{}", ALPHABET[a] as char, ALPHABET[b] as char));
            if out.len() == count {
                break 'outer;
            }
        }
    }
    out
}

/// Given typed input and the set of labels, classify the match state.
#[derive(Debug, PartialEq, Eq)]
pub enum HintMatch {
    /// Exactly one label equals the input.
    Activate(usize),
    /// Input is a prefix of >=1 labels; keep waiting.
    Filtering,
    /// No label starts with input.
    NoMatch,
}

pub fn match_hint(labels: &[String], typed: &str) -> HintMatch {
    if let Some(i) = labels.iter().position(|l| l == typed) {
        return HintMatch::Activate(i);
    }
    if labels.iter().any(|l| l.starts_with(typed)) {
        return HintMatch::Filtering;
    }
    HintMatch::NoMatch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_char_labels_for_small_counts() {
        let labels = generate_labels(3);
        assert_eq!(labels, vec!["s", "a", "d"]);
    }

    #[test]
    fn two_char_labels_when_exhausted() {
        let labels = generate_labels(20);
        assert_eq!(labels.len(), 20);
        assert!(labels.iter().all(|l| l.len() == 2));
        // uniqueness
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 20);
    }

    #[test]
    fn exact_match_activates() {
        let labels = generate_labels(3);
        assert_eq!(match_hint(&labels, "a"), HintMatch::Activate(1));
    }

    #[test]
    fn prefix_filters() {
        let labels = generate_labels(20); // two-char
        assert_eq!(match_hint(&labels, "s"), HintMatch::Filtering);
    }

    #[test]
    fn unknown_is_no_match() {
        let labels = generate_labels(3);
        assert_eq!(match_hint(&labels, "z"), HintMatch::NoMatch);
    }
}
```

Run: `cargo test -p vmux_vimium hints::` → expect PASS.

- [ ] **Step 2: Shadow-root overlay helper (wasm)**

Replace `crates/vmux_vimium/src/overlay.rs`:

```rust
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, ShadowRoot, ShadowRootInit, ShadowRootMode};

pub const HOST_ID: &str = "__vmux_vimium_host";

/// Get-or-create the host element + open shadow root that holds all vimium UI.
pub fn shadow(doc: &Document) -> ShadowRoot {
    if let Some(host) = doc.get_element_by_id(HOST_ID) {
        if let Some(sr) = host.shadow_root() {
            return sr;
        }
    }
    let host: Element = doc.create_element("div").unwrap();
    host.set_id(HOST_ID);
    doc.document_element().unwrap().append_child(&host).unwrap();
    let sr = host
        .attach_shadow(&ShadowRootInit::new(ShadowRootMode::Open))
        .unwrap();
    // base styles
    let style = doc.create_element("style").unwrap();
    style.set_text_content(Some(BASE_CSS));
    sr.append_child(&style).unwrap();
    sr
}

pub fn clear(doc: &Document) {
    if let Some(host) = doc.get_element_by_id(HOST_ID) {
        if let Some(sr) = host.shadow_root() {
            // remove everything except the <style> (first child)
            while let Some(last) = sr.last_element_child() {
                if last.tag_name().eq_ignore_ascii_case("style") {
                    break;
                }
                last.remove();
            }
        }
    }
}

const BASE_CSS: &str = "\
.vmux-hint{position:fixed;z-index:2147483647;background:#fffa65;color:#202020;\
border:1px solid #c8a000;border-radius:3px;padding:0 3px;font:bold 11px monospace;\
box-shadow:0 1px 2px rgba(0,0,0,.4);}\
.vmux-hint .typed{color:#b00;}\
.vmux-bar{position:fixed;left:0;right:0;bottom:0;z-index:2147483647;background:#202124;\
color:#eee;font:14px system-ui;padding:8px 12px;display:flex;gap:8px;}\
.vmux-bar input{flex:1;background:#303134;color:#eee;border:0;outline:0;padding:6px 8px;\
border-radius:6px;font:14px system-ui;}\
.vmux-find-hit{background:#fffa65;color:#000;}\
.vmux-find-active{background:#ff9632;color:#000;}";
```

Add web-sys features if missing: `"ShadowRoot"`, `"ShadowRootInit"`, `"ShadowRootMode"` (already listed in Task 1).

- [ ] **Step 3: Hints controller (wasm) — append to `hints.rs`**

Gate the DOM section wasm-only. Append:

```rust
#[cfg(target_arch = "wasm32")]
mod dom {
    use super::{generate_labels, match_hint, HintMatch};
    use crate::overlay;
    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, HtmlElement};

    const SELECTOR: &str = "a[href], button, input:not([type=hidden]), \
        textarea, select, [role=button], [onclick], [tabindex]";

    pub struct Hints {
        labels: Vec<String>,
        targets: Vec<Element>,
        typed: String,
    }

    impl Hints {
        /// Build hints for all currently-visible clickable elements. Returns
        /// None if there are no targets.
        pub fn show(doc: &Document) -> Option<Hints> {
            let nodes = doc.query_selector_all(SELECTOR).ok()?;
            let mut targets = Vec::new();
            for i in 0..nodes.length() {
                let node = nodes.get(i).unwrap();
                let el: Element = node.dyn_into().ok()?;
                if is_visible(&el) {
                    targets.push(el);
                }
            }
            if targets.is_empty() {
                return None;
            }
            let labels = generate_labels(targets.len());
            let sr = overlay::shadow(doc);
            for (label, el) in labels.iter().zip(targets.iter()) {
                let rect = el.get_bounding_client_rect();
                let tag = doc.create_element("div").unwrap();
                tag.set_class_name("vmux-hint");
                tag.set_text_content(Some(label));
                let h: HtmlElement = tag.dyn_into().unwrap();
                let st = h.style();
                let _ = st.set_property("left", &format!("{}px", rect.left().max(0.0)));
                let _ = st.set_property("top", &format!("{}px", rect.top().max(0.0)));
                sr.append_child(&h).unwrap();
            }
            Some(Hints { labels, targets, typed: String::new() })
        }

        /// Feed a typed character. Returns true if hints should stay open.
        pub fn feed(&mut self, doc: &Document, ch: &str) -> bool {
            self.typed.push_str(&ch.to_lowercase());
            match match_hint(&self.labels, &self.typed) {
                HintMatch::Activate(i) => {
                    activate(&self.targets[i]);
                    overlay::clear(doc);
                    false
                }
                HintMatch::Filtering => {
                    redraw(doc, &self.labels, &self.typed);
                    true
                }
                HintMatch::NoMatch => {
                    overlay::clear(doc);
                    false
                }
            }
        }

        pub fn cancel(&self, doc: &Document) {
            overlay::clear(doc);
        }
    }

    fn activate(el: &Element) {
        if let Some(h) = el.dyn_ref::<HtmlElement>() {
            // Focus inputs; click everything else.
            let tag = el.tag_name().to_lowercase();
            if tag == "input" || tag == "textarea" || tag == "select" {
                let _ = h.focus();
            } else {
                h.click();
            }
        }
    }

    fn redraw(doc: &Document, labels: &[String], typed: &str) {
        // hide labels that no longer match; bold the typed prefix on the rest
        let sr = overlay::shadow(doc);
        let tags = sr.query_selector_all(".vmux-hint").unwrap();
        for (i, label) in labels.iter().enumerate() {
            let Some(node) = tags.get(i as u32) else { continue };
            let el: Element = node.dyn_into().unwrap();
            let h: HtmlElement = el.dyn_into().unwrap();
            if label.starts_with(typed) {
                let _ = h.style().set_property("display", "block");
                h.set_inner_html(&format!(
                    "<span class=\"typed\">{}</span>{}",
                    &label[..typed.len()],
                    &label[typed.len()..]
                ));
            } else {
                let _ = h.style().set_property("display", "none");
            }
        }
    }

    fn is_visible(el: &Element) -> bool {
        let rect = el.get_bounding_client_rect();
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return false;
        }
        let win = web_sys::window().unwrap();
        let vw = win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let vh = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        rect.bottom() > 0.0 && rect.top() < vh && rect.right() > 0.0 && rect.left() < vw
    }
}

#[cfg(target_arch = "wasm32")]
pub use dom::Hints;
```

Add web-sys features: `"DomRect"` (listed), `"NodeList"` (listed).

- [ ] **Step 4: Hold hint state in the runtime + route keys**

In `crates/vmux_vimium/src/runtime.rs` add a `thread_local!` for the active hints session and a sub-mode so that while hints are open, keys feed the hint filter instead of the matcher:

```rust
thread_local! {
    static HINTS: RefCell<Option<crate::hints::Hints>> = RefCell::new(None);
}
```

In `on_keydown`, before the normal-mode matcher, add:

```rust
    let hints_open = HINTS.with(|h| h.borrow().is_some());
    if hints_open {
        ev.prevent_default();
        ev.stop_propagation();
        if key == "Escape" {
            HINTS.with(|h| {
                if let Some(hl) = h.borrow().as_ref() { hl.cancel(&doc); }
                *h.borrow_mut() = None;
            });
            return;
        }
        if key.chars().count() == 1 {
            let keep = HINTS.with(|h| h.borrow_mut().as_mut().unwrap().feed(&doc, &key));
            if !keep {
                HINTS.with(|h| *h.borrow_mut() = None);
            }
        }
        return;
    }
```

In `dispatch`, replace the `Action::Hints` arm:

```rust
        Action::Hints => {
            if let Some(h) = crate::hints::Hints::show(doc) {
                HINTS.with(|c| *c.borrow_mut() = Some(h));
            }
        }
```

- [ ] **Step 5: Build wasm + test**

Run: `cargo test -p vmux_vimium hints::` (pure) → PASS.
Run: `cargo build -p vmux_vimium --target wasm32-unknown-unknown` → compiles.

- [ ] **Step 6: Manual verify**

Press `f` on a content page → yellow hint tags appear on links/buttons; typing a label activates it (navigates/clicks); typing a prefix narrows; `Esc` dismisses.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_vimium/src/hints.rs crates/vmux_vimium/src/overlay.rs crates/vmux_vimium/src/runtime.rs crates/vmux_vimium/src/lib.rs
git commit -m "feat(vimium): link hints with shadow-root overlay"
```

---

## Task 6: Find-in-page (`/`, `n`, `N`)

A self-contained find overlay (input in the shadow root) that highlights substring matches and cycles through them. Match-cycling math is pure + tested; DOM is wasm.

**Files:**
- Create: `crates/vmux_vimium/src/find.rs`
- Modify: `crates/vmux_vimium/src/runtime.rs`, `crates/vmux_vimium/src/lib.rs`

- [ ] **Step 1: Pure cycle helper + tests**

`crates/vmux_vimium/src/find.rs`:

```rust
/// Compute the next active match index. `forward=false` goes to previous.
/// Wraps around. Returns None when there are no matches.
pub fn cycle(current: Option<usize>, total: usize, forward: bool) -> Option<usize> {
    if total == 0 {
        return None;
    }
    let next = match current {
        None => {
            if forward { 0 } else { total - 1 }
        }
        Some(i) => {
            if forward {
                (i + 1) % total
            } else {
                (i + total - 1) % total
            }
        }
    };
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_forward_is_zero() {
        assert_eq!(cycle(None, 3, true), Some(0));
    }

    #[test]
    fn first_backward_is_last() {
        assert_eq!(cycle(None, 3, false), Some(2));
    }

    #[test]
    fn forward_wraps() {
        assert_eq!(cycle(Some(2), 3, true), Some(0));
    }

    #[test]
    fn backward_wraps() {
        assert_eq!(cycle(Some(0), 3, false), Some(2));
    }

    #[test]
    fn no_matches_is_none() {
        assert_eq!(cycle(None, 0, true), None);
        assert_eq!(cycle(Some(0), 0, false), None);
    }
}
```

Run: `cargo test -p vmux_vimium find::` → expect PASS.

- [ ] **Step 2: Find controller (wasm) — append to `find.rs`**

```rust
#[cfg(target_arch = "wasm32")]
mod dom {
    use super::cycle;
    use crate::overlay;
    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, HtmlElement, HtmlInputElement};

    pub struct Find {
        matches: Vec<Element>,
        active: Option<usize>,
    }

    impl Find {
        /// Open the find bar (empty). The bar's input is focused so the user
        /// types the query; the runtime forwards Enter/Esc.
        pub fn open(doc: &Document) -> Find {
            let sr = overlay::shadow(doc);
            let bar = doc.create_element("div").unwrap();
            bar.set_class_name("vmux-bar");
            bar.set_inner_html("<input class=\"vmux-find-input\" placeholder=\"find\u{2026}\"/>");
            sr.append_child(&bar).unwrap();
            if let Some(input) = sr.query_selector(".vmux-find-input").unwrap() {
                let _ = input.dyn_ref::<HtmlElement>().unwrap().focus();
            }
            Find { matches: Vec::new(), active: None }
        }

        pub fn query(&self, doc: &Document) -> String {
            overlay::shadow(doc)
                .query_selector(".vmux-find-input")
                .unwrap()
                .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
                .map(|i| i.value())
                .unwrap_or_default()
        }

        /// Re-run search for the current query, highlight all hits, jump to first.
        pub fn search(&mut self, doc: &Document) {
            self.clear_highlights();
            self.matches.clear();
            self.active = None;
            let q = self.query(doc).to_lowercase();
            if q.is_empty() {
                return;
            }
            // Walk text nodes under body; wrap matches in <span class=vmux-find-hit>.
            self.matches = super::dom_search::highlight(doc, &q);
            self.next(doc, true);
        }

        pub fn next(&mut self, doc: &Document, forward: bool) {
            let total = self.matches.len();
            if let Some(prev) = self.active {
                if let Some(el) = self.matches.get(prev) {
                    el.set_class_name("vmux-find-hit");
                }
            }
            self.active = cycle(self.active, total, forward);
            if let Some(i) = self.active {
                let el = &self.matches[i];
                el.set_class_name("vmux-find-hit vmux-find-active");
                if let Some(h) = el.dyn_ref::<HtmlElement>() {
                    h.scroll_into_view();
                }
            }
            let _ = doc;
        }

        fn clear_highlights(&self) {
            // marks are spans inside the page; unwrap them by replacing with text
            for el in &self.matches {
                if let Some(parent) = el.parent_node() {
                    let text = el.text_content().unwrap_or_default();
                    let tn = el.owner_document().unwrap().create_text_node(&text);
                    let _ = parent.replace_child(&tn, el);
                }
            }
        }

        pub fn close(&self, doc: &Document) {
            self.clear_highlights();
            overlay::clear(doc);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod dom_search {
    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, Node};

    /// Naive text-node walk: find case-insensitive substring hits, wrap each in a
    /// span.vmux-find-hit, return the spans in document order.
    pub fn highlight(doc: &Document, query: &str) -> Vec<Element> {
        let mut hits = Vec::new();
        let Some(body) = doc.body() else { return hits };
        collect(doc, body.unchecked_ref::<Node>(), query, &mut hits);
        hits
    }

    fn collect(doc: &Document, node: &Node, query: &str, hits: &mut Vec<Element>) {
        let children = node.child_nodes();
        for i in 0..children.length() {
            let child = children.get(i).unwrap();
            match child.node_type() {
                Node::TEXT_NODE => {
                    let text = child.text_content().unwrap_or_default();
                    if text.to_lowercase().contains(query) {
                        if let Some(parent) = child.parent_node() {
                            let span = doc.create_element("span").unwrap();
                            span.set_class_name("vmux-find-hit");
                            span.set_text_content(Some(&text));
                            let _ = parent.replace_child(&span, &child);
                            hits.push(span);
                        }
                    }
                }
                Node::ELEMENT_NODE => {
                    let tag = child.node_name().to_lowercase();
                    if tag != "script" && tag != "style" && tag != "noscript" {
                        collect(doc, &child, query, hits);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use dom::Find;
```

Add web-sys features: `"Text"` (listed), `"Node"` (listed). Note: this is a deliberately simple highlighter (whole-text-node wrap). Sub-node range highlighting is a future enhancement.

- [ ] **Step 3: Route find keys in the runtime**

In `crates/vmux_vimium/src/runtime.rs`:

```rust
thread_local! {
    static FIND: RefCell<Option<crate::find::Find>> = RefCell::new(None);
}
```

In `on_keydown`, after the hints block, add a find-open block:

```rust
    let find_open = FIND.with(|f| f.borrow().is_some());
    if find_open {
        match key.as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                FIND.with(|f| {
                    if let Some(fd) = f.borrow().as_ref() { fd.close(&doc); }
                    *f.borrow_mut() = None;
                });
            }
            "Enter" => {
                ev.prevent_default();
                FIND.with(|f| f.borrow_mut().as_mut().unwrap().search(&doc));
            }
            _ => { /* let the input receive the keystroke; live search on keyup */ }
        }
        return;
    }
```

Wire `Action::OpenFind`, `FindNext`, `FindPrev` in `dispatch`:

```rust
        Action::OpenFind => {
            FIND.with(|f| *f.borrow_mut() = Some(crate::find::Find::open(doc)));
        }
        Action::FindNext => FIND.with(|f| {
            if let Some(fd) = f.borrow_mut().as_mut() { fd.next(doc, true); }
        }),
        Action::FindPrev => FIND.with(|f| {
            if let Some(fd) = f.borrow_mut().as_mut() { fd.next(doc, false); }
        }),
```

Add `pub mod find;` to `lib.rs` (DOM-free pure part compiles on all targets; the `dom`/`dom_search` submodules are wasm-gated).

- [ ] **Step 4: Build + test**

Run: `cargo test -p vmux_vimium find::` → PASS.
Run: `cargo build -p vmux_vimium --target wasm32-unknown-unknown` → compiles.

- [ ] **Step 5: Manual verify**

`/` opens the find bar; type a word + Enter highlights + jumps to first; `n`/`N` cycle; `Esc` closes and clears highlights.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_vimium/src/find.rs crates/vmux_vimium/src/runtime.rs crates/vmux_vimium/src/lib.rs
git commit -m "feat(vimium): find-in-page overlay with match cycling"
```

---

## Task 7: Open bar (`o`)

In-page URL/search overlay → navigates the current tab. URL-vs-search parsing is pure + tested.

**Files:**
- Create: `crates/vmux_vimium/src/openbar.rs`
- Modify: `crates/vmux_vimium/src/runtime.rs`, `crates/vmux_vimium/src/lib.rs`

- [ ] **Step 1: Pure parse helper + tests**

`crates/vmux_vimium/src/openbar.rs`:

```rust
/// Turn raw input into a navigable URL. If it looks like a URL, normalize it;
/// otherwise build a search URL.
pub fn to_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let looks_like_url = trimmed.contains("://")
        || (trimmed.contains('.') && !trimmed.contains(' ') && !trimmed.starts_with('.'));
    if looks_like_url {
        if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        }
    } else {
        format!(
            "https://duckduckgo.com/?q={}",
            urlencode(trimmed)
        )
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_url_passthrough() {
        assert_eq!(to_url("https://example.com/x"), "https://example.com/x");
    }

    #[test]
    fn bare_domain_gets_https() {
        assert_eq!(to_url("example.com"), "https://example.com");
    }

    #[test]
    fn query_becomes_search() {
        assert_eq!(to_url("hello world"), "https://duckduckgo.com/?q=hello+world");
    }

    #[test]
    fn single_word_with_no_dot_is_search() {
        assert_eq!(to_url("rustlang"), "https://duckduckgo.com/?q=rustlang");
    }
}
```

Run: `cargo test -p vmux_vimium openbar::` → PASS.

- [ ] **Step 2: Open bar controller (wasm) — append to `openbar.rs`**

```rust
#[cfg(target_arch = "wasm32")]
mod dom {
    use super::to_url;
    use crate::overlay;
    use wasm_bindgen::JsCast;
    use web_sys::{Document, HtmlElement, HtmlInputElement};

    pub struct OpenBar;

    impl OpenBar {
        pub fn open(doc: &Document) -> OpenBar {
            let sr = overlay::shadow(doc);
            let bar = doc.create_element("div").unwrap();
            bar.set_class_name("vmux-bar");
            bar.set_inner_html("<input class=\"vmux-open-input\" placeholder=\"open url or search\u{2026}\"/>");
            sr.append_child(&bar).unwrap();
            if let Some(input) = sr.query_selector(".vmux-open-input").unwrap() {
                let _ = input.dyn_ref::<HtmlElement>().unwrap().focus();
            }
            OpenBar
        }

        pub fn submit(&self, doc: &Document) {
            let val = overlay::shadow(doc)
                .query_selector(".vmux-open-input")
                .unwrap()
                .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
                .map(|i| i.value())
                .unwrap_or_default();
            let url = to_url(&val);
            overlay::clear(doc);
            if !url.is_empty() {
                let _ = web_sys::window().unwrap().location().set_href(&url);
            }
        }

        pub fn close(&self, doc: &Document) {
            overlay::clear(doc);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use dom::OpenBar;
```

- [ ] **Step 3: Route open-bar keys in the runtime**

In `crates/vmux_vimium/src/runtime.rs`:

```rust
thread_local! {
    static OPENBAR: RefCell<Option<crate::openbar::OpenBar>> = RefCell::new(None);
}
```

In `on_keydown`, after the find block:

```rust
    let open_active = OPENBAR.with(|o| o.borrow().is_some());
    if open_active {
        match key.as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                OPENBAR.with(|o| {
                    if let Some(b) = o.borrow().as_ref() { b.close(&doc); }
                    *o.borrow_mut() = None;
                });
            }
            "Enter" => {
                ev.prevent_default();
                OPENBAR.with(|o| {
                    if let Some(b) = o.borrow().as_ref() { b.submit(&doc); }
                    *o.borrow_mut() = None;
                });
            }
            _ => {}
        }
        return;
    }
```

Wire `Action::OpenBar` in `dispatch`:

```rust
        Action::OpenBar => {
            OPENBAR.with(|o| *o.borrow_mut() = Some(crate::openbar::OpenBar::open(doc)));
        }
```

Add `pub mod openbar;` to `lib.rs`. Remove the now-unreachable catch-all `other => console::log` arm in `dispatch` once all actions are handled (every `Action` variant now has an arm; the match becomes exhaustive).

- [ ] **Step 4: Build + test**

Run: `cargo test -p vmux_vimium openbar::` → PASS.
Run: `cargo build -p vmux_vimium --target wasm32-unknown-unknown` → compiles, no non-exhaustive-match or unused warnings.

- [ ] **Step 5: Manual verify**

`o` opens the bar; typing a domain + Enter navigates; typing words + Enter searches; `Esc` closes.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_vimium/src/openbar.rs crates/vmux_vimium/src/runtime.rs crates/vmux_vimium/src/lib.rs
git commit -m "feat(vimium): open bar (url/search) navigating current tab"
```

---

## Task 8: Full verification + cleanup

- [ ] **Step 1: Full workspace checks**

Run: `cargo fmt --all` then `git checkout -- patches/` (per the cargo-fmt-patches rule: fmt reformats vendored patches; keep only `crates/` formatting).
Run: `cargo clippy -p vmux_vimium -p vmux_browser -p vmux_setting --all-targets` → no warnings.
Run: `cargo test -p vmux_vimium -p vmux_browser` → all pass.
Run: `cargo build -p vmux_vimium --target wasm32-unknown-unknown` → compiles.

- [ ] **Step 2: End-to-end manual matrix**

In a windowed build, on a normal site AND github.com (CSP), confirm:
- `f` hints → click; `j`/`k`/`d`/`u`/`gg`/`G` scroll; `H`/`L` history; `r` reload; `/`+`n`/`N` find; `o` open.
- Focused `<input>`: typing works, vimium keys do NOT fire; `Esc` blurs → normal mode resumes.
- Terminal tab, agent pane, and a `vmux://` page (e.g. settings/history): vimium keys do nothing (no injection / no marker / no key capture).
- Set `browser.vimium_enabled: false` in settings.ron, relaunch → no injection anywhere.

- [ ] **Step 3: Strip any leftover debug logging**

Grep `console::log` in `crates/vmux_vimium/src` — remove any temporary diagnostics (per the debugging rule). The only remaining console output should be the bootstrap's CSP-failure `console.warn`.

- [ ] **Step 4: Delete the plan file**

Per the docs rule, delete this plan once fully implemented:

```bash
git rm docs/plans/2026-06-25-vimium-browsing.md
git commit -m "chore: remove implemented vimium plan"
```

- [ ] **Step 5: Open PR**

Use the open-new-pr skill / `gh pr create` directly (per the create-PR-directly rule) and return the URL.

---

## Self-review (plan vs spec)

**Spec coverage:**
- Link hints (`f`) → Task 5. Scroll (`j`/`k`/`d`/`u`/`gg`/`G`) → Task 4. History (`H`/`L`) → Task 4. Reload (`r`) → Task 4. Find (`/`,`n`,`N`) → Task 6. Open (`o`) → Task 7. ✓
- Auto modal (insert on text-field focus, `i`/`Esc`) → Task 3 (`resolve_mode` + runtime). ✓
- Real http/https pages only → Task 1 host system scheme gate + bootstrap protocol gate. ✓
- Windowed only → relies on bare keys reaching the page natively; no OSR/native-monitor changes. ✓
- Rust→WASM (web-sys), new `vmux_vimium` cdylib → Tasks 1–7. ✓
- Zero page→host IPC → all actions in-page; no `cef.emit`. ✓
- CSP risk gated first → Task 1 Step 9. ✓
- `browser.vimium.enabled` kill switch → Task 1 Step 7 (implemented as flat `browser.vimium_enabled`; deviates from the spec's dotted `browser.vimium.enabled` to avoid a nested settings struct — behavior identical). ✓
- Tests: pure-logic unit tests (keymap, mode, scroll, hints labels, find cycle, openbar parse) + host system test → throughout. ✓

**Deferred (matches spec non-goals):** OSR/3D, `F`/`O` new-tab, command-bar `o` integration, settings-driven custom keymaps, cross-iframe hints, regex find.

**Type consistency:** `Action` variants defined in Task 2 are exactly the arms handled in Tasks 4–7 `dispatch`; `Matcher`/`MatchResult`/`HintMatch`/`Mode` names are stable across tasks. `preload_script()` (host) is the single host entry point used by `vmux_browser`.
