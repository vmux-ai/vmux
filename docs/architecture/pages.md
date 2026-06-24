# Pages: webview apps on the `vmux://` scheme

> Part of the [Vmux Architecture](../architecture.md) overview.

In Vmux, a **page** is whatever fills a pane. Some pages are the open web — full Chromium. The
rest are Vmux's *own* surfaces — the [Browser](browser.md) frame, the [Terminal](terminal.md), the
files [Editor](editor.md), history, settings, spaces, the command bar, even the layout overlay
itself — and those are **webview apps**: Dioxus/WASM apps Vmux ships and renders inside CEF.

Vmux's pages aren't fetched from a server. They're bundled into the app and served from a
registered custom scheme, **`vmux://`**, addressed by host — `vmux://terminal`, `vmux://files`,
`vmux://history`, `vmux://settings`, `vmux://debug`, one per page. The scheme is registered as
**standard** and **secure** in both the browser and renderer processes, and a handler answers each
request straight from embedded assets, so a page loads **instantly, offline, with no server in the
loop**. (The name is `vmux` by default, overridable via `BEVY_CEF_EMBEDDED_SCHEME`.)

## A privileged bridge, gated on the scheme

The scheme is also the app's security boundary. "The workspace is an API" — every action is
reachable over [MCP](agent-first.md) — invites the obvious question: can a random website drive
it? No.

The host bridge (`window.cef` — the messaging that lets a page read or command the workspace) only
works for **trusted frames**: a page is trusted *iff* its URL is `vmux://<known-host>/`. Because
`vmux://` is served only from bundled assets, no web page can ever *be* at a `vmux://` URL — an
unforgeable boundary, checked **per frame** (an `evil.com` iframe inside a trusted page is still
rejected). Anything you browse over `https://` gets zero bridge access; calls are dropped before
they reach the ECS, enforced in the browser process with a defense-in-depth check in the renderer.

A second layer adds **least privilege** among trusted pages: each message type is bound to the page
that may emit it (`for_hosts(&["history"])`, …), so a compromised Vmux page can't pivot to
another's handlers — and the full Bevy Remote Protocol is locked to the `debug` page alone. The
predicate lives in the patched `bevy_cef_core` (`url_is_trusted_embedded_page`), unit-tested against
`https://evil.com`, `about:blank`, and bare `vmux://`.

## The pages, one by one

- **[Browser](browser.md)** — the open web: full Chromium, embedded via CEF.
- **[Terminal](terminal.md)** — a real PTY parsed in the daemon, streamed to a Dioxus grid.
- **[Editor](editor.md)** — a syntect-highlighted files surface with typed previews.

…plus the smaller built-ins — history, settings, spaces, services, debug — cut from the same cloth.
