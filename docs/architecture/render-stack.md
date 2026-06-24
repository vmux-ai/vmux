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

A surface is a tree of components; state lives in hooks, and the view re-renders reactively
when it changes — you describe the UI, not the DOM mutations:

```rust
#[component]
fn UrlBar(initial: String) -> Element {
    let mut url = use_signal(|| initial);
    rsx! {
        input { value: "{url}", oninput: move |e| url.set(e.value()) }
        button { onclick: move |_| navigate(url()), "Go" }
    }
}
```

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

## The `vmux://` URL scheme

Vmux's own pages — history, settings, the layout overlay, the `debug` console, the services
monitor — aren't fetched over the network. They're bundled into the app and served from a
registered custom scheme, `vmux://`, addressed by host: `vmux://history`, `vmux://settings`,
`vmux://debug`, one per page crate. The scheme is registered as **standard** and **secure**
(`CEF_SCHEME_OPTION_STANDARD | …_SECURE`) in both the browser and renderer processes, and a
scheme handler answers each request straight from embedded assets — so a Vmux page loads
instantly, offline, with no server in the loop. (The name is `vmux` by default, overridable via
`BEVY_CEF_EMBEDDED_SCHEME`.)

### A privileged bridge, gated on the scheme

The scheme is also the app's security boundary. "The workspace is an API" — every action is
reachable over [MCP](agent-first.md) — invites the obvious question: can a random website drive
it? No.

The host bridge (`window.cef` — the messaging that lets a page read or command the workspace)
only works for **trusted frames**: a page is trusted *iff* its URL is `vmux://<known-host>/`.
Because `vmux://` is served only from bundled assets, no web page can ever *be* at a `vmux://`
URL — an unforgeable boundary, checked **per frame** (an `evil.com` iframe inside a trusted page
is still rejected). Anything you browse over `https://` gets zero bridge access; calls are
dropped before they reach the ECS, enforced in the browser process with a defense-in-depth check
in the renderer.

A second layer adds **least privilege** among trusted pages: each message type is bound to the
page that may emit it (`for_hosts(&["history"])`, …), so a compromised Vmux page can't pivot to
another's handlers — and the full Bevy Remote Protocol is locked to the `debug` page alone. The
predicate lives in the patched `bevy_cef_core` (`url_is_trusted_embedded_page`), unit-tested
against `https://evil.com`, `about:blank`, and bare `vmux://`.

## One more thing — your workspace in 3D

The whole workspace already lives in a Bevy 3D scene, so flipping to **Player mode** tilts
your panes into a spatial, depth-sorted view of the very same workspace — pages still live,
still interactive, still scrolling. It was never why we reached for a game engine; it's a
fun side effect of having one. Sometimes the best features are the ones you get for free.
