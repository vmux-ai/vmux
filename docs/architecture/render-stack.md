# 2D / 3D renderer: one window, many web surfaces

> Part of the [Vmux Architecture](../architecture.md) overview.

We've covered the logical tree; here's how it actually paints. Vmux composites several
independent CEF web views into one window. The everyday path is native and fast:

```
Vmux window
│
├─ content page    ─►  native windowed CEF view, framed to its pane
│  (User mode,         (Chrome-parity CPU — native corners + focus ring,
│   macOS)              no offscreen copy)
│
└─ the layout      ─►  a transparent CEF overlay (header · URL bar · sidebar ·
                       command bar) running a Rust→WASM Dioxus app, composited on top
```

- **Content** is what you browse: in the everyday flat mode on macOS, each page is a
  *native windowed* CEF view positioned over its pane — scrolling costs what Chrome costs.
- **The layout** — header, URL bar, sidebars, command bar — is itself one transparent CEF
  view running a Dioxus app, composited over the content.

A second backend renders a page **offscreen** into a GPU texture instead of a native view;
Vmux falls back to it where a native overlay won't do — non-macOS, and the spatial mode at
the very end. Switching backends tears down and recreates the CEF browser
(`crates/vmux_browser/src/lib.rs`).

## Zero-copy host interop

Host and web views don't talk over JSON-on-IPC. They exchange zero-copy, binary **rkyv**
messages:

- **Host → web view** — structural workspace state (open tabs, layout mutations).
- **Web view → host** — operational commands (open the command bar, trigger a layout
  action).

## UI architecture: Rust all the way down

The layout crate compiles to two targets from one source:

1. `native` — the Bevy systems driving the desktop host.
2. `wasm32` — the UI rendered *inside* CEF via a **[Dioxus](https://dioxuslabs.com)** app.

Dioxus is React-shaped, so web devs are instantly at home: declarative UI through an
`rsx!` macro (JSX-like markup) and `use_state` / `use_effect`-style **hooks** for state
and side effects — the same component-and-hook model, in Rust.

Our UI toolkit (`crates/vmux_ui`) is a design system matching **shadcn/ui** (dialogs,
dropdowns, popovers, calendars) on Dioxus primitives, styled with **Tailwind** and
shadcn design tokens (`bg-background`, `text-foreground`, themeable light/dark). Classic
web paradigms, standard utility classes, reactive components — expressed in
strongly-typed Rust, with no TypeScript boundary across the app loop.

"All the way down" is about Vmux's *own* surfaces — the header, command bar, settings,
error pages. The content you open is full Chromium: any React, Vue, Svelte, or plain-JS
app renders exactly as it would in Chrome, with zero special-casing. Rust on the outside;
at the end of the day, on the inside it's just Chromium — and Vmux talks to any page the
way the web already does: JS message passing. (Vmux's own WASM surfaces get the typed
rkyv bridge; arbitrary pages use plain JS messaging.)

## One more thing — your workspace in 3D

The whole workspace already lives in a Bevy 3D scene, so flipping to **Player mode** tilts
your panes into a spatial, depth-sorted view of the very same workspace — pages still live,
still interactive, still scrolling. It was never why we reached for a game engine; it's a
fun side effect of having one. Sometimes the best features are the ones you get for free.
