# Vimium-like Browsing — Design

Date: 2026-06-25
Status: Approved (pending spec review)

## Summary

Add keyboard-driven web navigation ("vimium-like") to vmux's browser pages: link
hints, scrolling, history, reload, find-in-page, and open. The behavior is
implemented as an **in-page content script authored in Rust and compiled to
WebAssembly** (`web-sys`/`wasm-bindgen`), injected into real web pages only. It
mirrors how the real Vimium extension works — all logic runs in the page, with
**zero page→host IPC** for the first cut.

## Goals

- Vimium-style *normal mode* on web pages, with automatic *insert mode* when a
  text field is focused.
- Feature set: link hints (`f`), scroll (`j`/`k`/`d`/`u`/`gg`/`G`), history
  (`H`/`L`), reload (`r`), find (`/` + `n`/`N`), open (`o`).
- Applies **only to real http/https browser pages** — never terminals, agent
  panes, or `vmux://` internal UI.
- Works in **windowed (User-mode) CEF** browsing.
- Authored in Rust→WASM, consistent with vmux's existing WASM page crates.

## Non-goals (deferred)

- OSR / 3D (Player) mode. (Nearly free under this design, but out of first cut.)
- `F`/`O` open-in-new-tab and hint-in-new-tab (needs host tab creation).
- Mapping `o` to vmux's native command bar (needs a trusted page→host channel).
- User-configurable keymaps via settings (defaults baked in for first cut;
  config pushed host→page later).
- Cross-iframe hints (top frame only for first cut).
- Visual/caret mode, marks, yank, multi-search-engine vomnibar.

## Locked decisions

| Decision | Choice |
|---|---|
| Feature scope | hints + scroll + `H`/`L` + `r` + `/` + `o` |
| Modal model | Auto (normal default; insert on text-field focus; `Esc`→normal) |
| Surface | real http/https pages only (not terminal/agent/`vmux://`) |
| Render mode | windowed only |
| Implementation | in-page content script, **Rust→WASM (web-sys)** |
| Crate | new `crates/vmux_vimium` (cdylib) |
| Page→host IPC | none in first cut |

## Architecture

### Overview

```
crates/vmux_vimium (Rust, wasm32 cdylib)
        │  cargo build --target wasm32-unknown-unknown
        │  wasm-bindgen --target no-modules
        ▼
   vimium_bg.wasm + vimium.js (glue)
        │  build step: base64(wasm) + glue → vimium_preload.js (single string)
        ▼
   embedded into vmux host binary (include_str!)
        │  host sets PreloadScripts = [vimium_preload.js]
        │  ONLY on http/https page webviews (scheme-gated)
        ▼
   CEF render process: on_context_created evals preload
        │  bootstrap: guard top-frame + double-init → instantiate wasm → start()
        ▼
   WASM content script runs in page:
     - capturing keydown listener on document
     - focusin/focusout → mode tracking
     - hint/find/open overlays in a shadow root
     - actions via DOM/BOM APIs (click, scrollBy, history, location)
```

### Crate: `crates/vmux_vimium`

- `crate-type = ["cdylib"]`, edition 2021 (match workspace).
- Dependencies: `wasm-bindgen`, `web-sys` (feature-gated DOM interfaces),
  `js-sys`. **No host/workspace deps** — avoids dependency cycles (the
  no-new-crates rule targets cycle risk; a pure web-sys cdylib has none).
- Entry: `#[wasm_bindgen(start)]` `pub fn start()` — installs listeners, returns.
- Module layout (filename-based, no `mod.rs` per project rule):
  - `lib.rs` — `start()`, top-level wiring, global state cell.
  - `mode.rs` — Normal/Insert state machine + text-field detection.
  - `keymap.rs` — key/sequence matcher (handles multi-key `gg`), action enum.
  - `hints.rs` — clickable enumeration, label generation, overlay render, filter.
  - `scroll.rs` — scroll actions (`scrollBy`/`scrollTo`).
  - `find.rs` — in-page find overlay + match navigation (`n`/`N`).
  - `openbar.rs` — in-page URL/search overlay → current-tab navigation.
  - `overlay.rs` — shared shadow-root container + styling helpers.
- Pure logic (label generation, key-sequence matching, mode transitions) lives
  in functions testable without a DOM, under `#[cfg(test)]`.

### Build pipeline (`crates/vmux_server/build.rs`)

Add a build step that runs **before** the existing page build:

1. `cargo build -p vmux_vimium --target wasm32-unknown-unknown --release`
   (debug in dev profile to keep iteration fast).
2. `wasm-bindgen --target no-modules --no-typescript --out-dir <OUT>
   <wasm artifact>` → produces `vmux_vimium.js` (defines global
   `wasm_bindgen`) + `vmux_vimium_bg.wasm`.
3. Base64-encode the `.wasm`, template both into a `vimium_preload.js`
   bootstrap string (see Injection), write to `OUT_DIR`.
4. Add `crates/vmux_vimium/src` to the build's tracked manifest paths (the same
   list used by `PageBuilder`) so edits trigger a rebuild. (Per the WASM
   rebuild-tracking gotcha: missing src tracking = stale artifacts.)

The host embeds the bootstrap via `include_str!(concat!(env!("OUT_DIR"), "/vimium_preload.js"))`.

Note: `wasm-bindgen-cli` must be available in CI/build env. Pin its version to
the `wasm-bindgen` crate version (mismatch is a hard error). Document in the
crate and, if needed, add to the build toolchain.

### Injection bootstrap (`vimium_preload.js`)

A small JS template (the only hand-written JS — it cannot be WASM since it
bootstraps WASM):

```js
(function () {
  if (window.top !== window) return;            // top frame only (first cut)
  if (window.__vmuxVimium) return;              // double-init guard
  window.__vmuxVimium = true;
  var wasmB64 = "%%WASM_B64%%";                 // injected at build
  %%GLUE_JS%%                                   // wasm-bindgen no-modules glue
  var bytes = Uint8Array.from(atob(wasmB64), function (c) { return c.charCodeAt(0); });
  wasm_bindgen(bytes).then(function () { wasm_bindgen.start(); })
    .catch(function (e) { /* CSP/instantiate failure — see Risks */ });
})();
```

Self-contained: no `fetch`, no network, no `vmux://` cross-origin request.

### Host integration (Rust, native side)

- **Scheme gating**: set the `PreloadScripts` component to `[VIMIUM_PRELOAD]`
  only when constructing **web page** webview bundles. Target
  `crates/vmux_layout/src/cef.rs` (`Browser::new` / `layout_cef_bundle`). Do not
  set it for the `LayoutCef` shell, command-bar, terminal, agent, or any
  `vmux://`/`file://` webview. Concretely: only attach when the resolved URL
  scheme is `http`/`https`.
- **Enable setting**: `browser.vimium.enabled` (bool, default `true`) in the
  settings schema. When false, do not attach `PreloadScripts` (no injection at
  all) — clean kill switch with no page cost. (Absent key falls back to default
  `true`; do not auto-seed the config — per the no-config-auto-seed rule.)
- No new commands, no `AppCommand` entries, no native-monitor changes for the
  first cut: bare keys already pass through the macOS native monitor to the page
  (the monitor only consumes modified keys), so the content script receives
  `f`/`j`/`k`/etc. directly in windowed mode. vmux's own shortcuts (all
  modified, e.g. `cmd+k`) are unaffected.

## Modes

- **Normal** (default on web pages). Bare keys are commands; the capturing
  `keydown` listener calls `preventDefault()` + `stopPropagation()` for handled
  keys so the page never sees them.
- **Insert** entered automatically when `document.activeElement` is
  `input`/`textarea`/`select`/`[contenteditable]`, or explicitly via `i`. In
  insert mode the listener intercepts **only** `Esc` (→ normal + `blur()`);
  everything else types normally.
- Focus tracking via `focusin`/`focusout` on `document` plus an `activeElement`
  check at keydown time (covers programmatic focus).

## Keymap (vimium defaults, first cut)

| Key | Action |
|---|---|
| `f` | Link hints → click target in current tab |
| `j` / `k` | Scroll down / up (line) |
| `d` / `u` | Scroll half-page down / up |
| `gg` / `G` | Scroll to top / bottom |
| `H` / `L` | History back / forward |
| `r` | Reload |
| `/` | Open find overlay |
| `n` / `N` | Next / previous find match |
| `o` | Open URL/search overlay (navigates current tab) |
| `i` | Enter insert mode |
| `Esc` | Exit hints/find/open overlay; else exit insert → normal |

Multi-key sequences (`gg`) handled by a small timeout-bounded matcher in
`keymap.rs`. Digit prefixes (counts) are out of scope for first cut.

## Features

- **Hints (`f`)**: enumerate clickable elements (`a[href]`, `button`,
  `[role=button]`, `input`, `[onclick]`, `[tabindex]`, etc.) intersected with
  the viewport and visible (non-zero box, not `display:none`/`visibility:hidden`).
  Generate short labels from a home-row alphabet. Render label tags in a shadow
  root anchored at each element's `getBoundingClientRect()`. Typing filters;
  unique match → `element.click()` (or `.focus()` for inputs). `Esc` cancels.
- **Scroll**: `window.scrollBy`/`scrollTo` with smooth behavior; half-page =
  `innerHeight/2`.
- **History**: `history.back()` / `history.forward()`.
- **Reload**: `location.reload()`.
- **Find (`/`)**: custom in-page overlay (input box in shadow root). Searches
  text nodes, highlights matches, `n`/`N` cycle, `Enter` keeps highlight,
  `Esc` closes. (First cut: simple substring/case-insensitive; regex later.)
- **Open (`o`)**: shadow-root overlay with a single input; on `Enter`, if it
  parses as a URL navigate to it, else treat as a search query
  (`https://duckduckgo.com/?q=...` or a configurable default later). Navigates
  the **current tab** (`location.href = ...`).

## Overlays

All vimium UI (hint tags, find bar, open bar) is rendered inside a single
`#vmux-vimium` host element attached to `document.documentElement`, using an
**open shadow root** to isolate styles from the page. High `z-index`,
`pointer-events` managed per overlay. Torn down on `Esc`/action completion.

## Risks & mitigations

1. **CSP / `wasm-unsafe-eval`** — Host-eval'd JS (via CEF `context.eval` at
   `on_context_created`) bypasses CSP's script restrictions, but
   `WebAssembly.instantiate` may be gated by `wasm-unsafe-eval`/`unsafe-eval` in
   the page's CSP, enforced per-realm by Blink. It is **likely** that raw-V8
   instantiation bypasses this hook, but not guaranteed across Chromium
   versions. **Mitigation**: verify early on a strict-CSP site (github.com). If
   blocked, fallback options (in priority): (a) instantiate via a Blob worker /
   alternate path; (b) ship a minimal JS implementation of the hot paths. Track
   as the first implementation task — it gates the whole approach.
2. **Per-navigation WASM instantiate cost** — Each top-frame navigation
   re-instantiates the module. A small module compiles in single-digit ms;
   acceptable. Keep the wasm lean (avoid heavy deps, enable `opt-level="s"` /
   `wasm-opt` in release).
3. **Key conflicts with the page** — Some sites bind `j`/`k`/`/` themselves.
   Normal mode's capturing listener wins (it runs first and stops propagation).
   Insert-mode detection prevents stealing keys in editors/inputs. Gmail-style
   apps that use bare keys outside inputs will be shadowed in normal mode — this
   matches Vimium's own behavior and is acceptable; a per-site disable is a
   future enhancement.
4. **`wasm-bindgen-cli` version skew** — Pin to the crate version in the build
   toolchain; mismatch fails the build loudly.

## Testing strategy

- **WASM unit tests** (`#[cfg(test)]`, host target where logic is DOM-free):
  hint-label generation (count → labels, no prefix collisions), key-sequence
  matcher (`gg` vs `g`+timeout, `G`), mode transition table.
- **Host test** (native): constructing a web-page bundle attaches
  `PreloadScripts`; constructing `vmux://`/terminal bundles does not; toggling
  `browser.vimium.enabled=false` omits it.
- **Manual** (windowed build): hints/scroll/find/history/open on a normal site;
  **strict-CSP site (github.com)** for the CSP gate; insert-mode in a text field
  (typing not intercepted, `Esc` exits); confirm terminal, agent, and `vmux://`
  pages are completely unaffected.

## File-by-file change list

New:
- `crates/vmux_vimium/Cargo.toml` — cdylib, web-sys/wasm-bindgen/js-sys.
- `crates/vmux_vimium/src/lib.rs` + `mode.rs` `keymap.rs` `hints.rs` `scroll.rs`
  `find.rs` `openbar.rs` `overlay.rs`.

Modified:
- root `Cargo.toml` — add `crates/vmux_vimium` to workspace members.
- `crates/vmux_server/build.rs` — wasm build + wasm-bindgen + base64/template
  step; add `vmux_vimium/src` to tracked paths.
- `crates/vmux_layout/src/cef.rs` — attach `PreloadScripts` for http/https page
  bundles only; respect `browser.vimium.enabled`.
- settings schema (embedded `settings.ron` + setting struct) — add
  `browser.vimium.enabled` (default true; no auto-seed).

## Build sequence (implementation order)

1. **CSP spike**: minimal `vmux_vimium` that, on load, renders a visible marker
   overlay; wire build + injection; verify it appears on a normal site **and**
   github.com (validates the WASM-injection + CSP assumption before building
   features). Gate everything else on this.
2. Mode state machine + capturing keydown + insert detection.
3. Scroll + reload + history (simplest actions; validates keymap).
4. Link hints.
5. Find overlay.
6. Open overlay.
7. `browser.vimium.enabled` setting + scheme gating tests.
8. Manual verification pass.
