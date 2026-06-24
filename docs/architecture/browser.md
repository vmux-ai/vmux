# Browser: Chromium as a guest surface

> Part of the [Vmux Architecture](../architecture.md) overview. One of the
> [pages](pages.md) Vmux renders in a pane.

The pages you browse are full **Chromium**, embedded with the **Chromium Embedded Framework
(CEF)** — the same engine Chrome ships, not a system WebView wrapper. Vmux drives it from Rust
through **[`bevy_cef`](https://crates.io/crates/bevy_cef)** (and `bevy_cef_core`), a Bevy plugin
layer vendored and patched in-tree (`crates/vmux_browser` + `patches/bevy_cef*`). CEF runs
multi-process exactly like Chrome: the Vmux app is the **browser process**, and a separate
**render process** runs each page's V8/JavaScript, talking back over CEF's process messages.

## Two ways to paint a page

Vmux renders each web view one of two ways and swaps between them at runtime:

- **Native windowed CEF** *(macOS, browse mode)* — a real native `NSView` is positioned over the
  pane. The page gets its own first responder, native scrolling, and **Chrome-parity CPU** —
  scrolling costs exactly what Chrome costs. The 3D mesh that normally carries the page drops its
  alpha to reveal the native view underneath.
- **Offscreen rendering (OSR) into a GPU texture** *(everywhere else — Linux, the layout overlay,
  3D mode)* — CEF paints the page offscreen and hands Vmux an accelerated frame. On macOS the
  shared GPU texture (an `IOSurface`) is imported **straight into Bevy's `wgpu` device** and
  composited onto a quad — no CPU copy. Running on Bevy `0.19` is what unlocks this zero-copy path.

Switching backends tears the CEF browser down and recreates it in the other mode, so a page can
move between a native overlay and a GPU texture as you change modes. How these surfaces composite
into one window is **[the render stack](render-stack.md)**.

## Talking to a page

Vmux's own pages get a typed, zero-copy bridge; arbitrary websites get none.

- **rkyv messages** — host→page state (tabs, layout, theme) travels as zero-copy **rkyv** binary
  buffers, not JSON-on-IPC; pages send commands back the same way.
- **`window.cef`** — the render process injects a small JS API (`emit` / `listen` / `brp`) that
  Vmux's WASM pages use to read and command the workspace.
- **Trust is gated on the scheme** — the bridge only answers **trusted frames** served from bundled
  assets; anything over `https://` gets zero access. See **[Pages](pages.md)** for the full model.

Keyboard focus across windowed pages, OSR terminals, and the host window is reconciled in
`crates/vmux_browser/src/host_focus.rs`, so typing always lands in the surface you're looking at.
