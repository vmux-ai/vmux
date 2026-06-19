# ECS ↔ Website Bridge — Security Audit & Model

Date: 2026-06-19

Scope: the bidirectional bridge between the Bevy ECS (browser process) and web
content rendered in CEF webviews. Question answered: **can a malicious website
loaded in a browse tab steal/modify ECS state or run shell commands locally?**

Companion document: [`2026-06-19-bridge-scheme-gate-design.md`](2026-06-19-bridge-scheme-gate-design.md)
describes the *design* of the scheme/host gate. This document is the *audit* — the
consolidated threat model, the layered defenses as actually implemented, the inventory
of dangerous sinks, and the residual risks.

## Verdict

**Well sandboxed.** A hostile `https://` page cannot read or modify ECS, and cannot
reach any shell/process sink. The bridge is not exposed to arbitrary web content at
all — it is gated to vmux's own `vmux://` pages by a sound trust boundary (custom
scheme), with the authoritative check living in the trusted browser process. There is
no network-reachable control plane. The only realistic break path is a Chromium
renderer-sandbox escape (a CEF/Chromium 0-day), which is inherent to embedding a
browser and is mitigated by Chromium site isolation.

## Architecture: two kinds of web content

vmux renders two distinct classes of content in CEF webviews:

1. **Trusted first-party pages** — vmux's own Dioxus/WASM UI (layout, command-bar,
   terminal, settings, spaces, history, services, debug, agent, header). Served from
   the custom `vmux://<host>/` scheme out of bundled assets. These legitimately use
   the bridge.
2. **Untrusted browse content** — arbitrary `https://…` sites the user navigates to in
   a browse tab. These must have **no** bridge access.

The trust boundary between them is the **URL scheme**: a web page can never *be* served
from `vmux://` (see Layer 0), so "frame URL starts with `vmux://<known-host>/`" is a
sound test for "this is trusted first-party UI."

## Bridge data path (inbound: page → ECS)

```
page JS  window.cef.binEmit("<type-id>", arrayBuffer)
  → render process  CefApiHandler::execute_bin_emit   [Layer 2 gate]
  → CEF process message (PROCESS_MESSAGE_BIN_JS_EMIT)
  → browser process  ClientHandlerBuilder::on_process_message_received   [Layer 1 gate]
  → BinEmitEventHandler::handle_message  (stamps authoritative host)   [Layer 3]
  → async channel → BinIpcEventRawBuffer
  → receive_bin_events::<T>  (rkyv CheckBytes decode + host_allowed)   [Layer 4 + safe decode]
  → commands.trigger(BinReceive<T> { webview, payload })
  → per-module observer (the actual handler / sink)
```

`window.cef.brp(...)` follows the same render→browser hop via `PROCESS_MESSAGE_BRP` and
is additionally locked down at Layer 1 (see below). `window.cef.emit` (JSON variant)
mirrors `binEmit`.

## Threat model

- **Attacker**: a malicious website rendered in a browse tab, running arbitrary JS,
  optionally with attacker-controlled iframes, links, redirects, page titles, and
  `fetch` requests. No local code execution assumed yet — that is what we are trying to
  prevent.
- **Assets**: ECS state (history, spaces, settings, terminals, layout), and shell
  execution via the terminal PTY / process control / app relaunch / installer.
- **Out of scope**: a Chromium renderer-sandbox-escape 0-day (treated as a residual
  assumption, not an app-level defense), and a malicious *local* process running as the
  user (it already has the user's privileges).

## Defenses (as implemented)

All file references are to the vendored `patches/` copies of `bevy_cef` /
`bevy_cef_core`, which is where the gates live.

### Layer 0 — Chromium scheme isolation (the trust boundary itself)

`vmux://` is registered as a custom scheme with flags
`STANDARD | SECURE | LOCAL | CORS_ENABLED | FETCH_ENABLED`
in **both** processes:
- `bevy_cef_core/src/util.rs:206` (`cef_scheme_flags()`)
- `bevy_cef_core/src/browser_process/app.rs:96` and `render_process/app.rs:45`
  (`on_register_custom_schemes`)

Consequences:
- `LOCAL` makes `vmux://` behave like `file:` — normal `http(s)` pages **cannot**
  navigate to, iframe, or `fetch()` `vmux://` resources. So a malicious site can neither
  load itself into a `vmux://` origin nor read the contents of a vmux page.
- `STANDARD` + host means each `vmux://<host>/` is a **distinct origin**, so e.g.
  `vmux://command-bar` cannot DOM-script `vmux://terminal`. Cross-page isolation is
  enforced at the IPC layer by Layer 4 (host allow-lists), not by shared-origin trust.
- Assets are served only from the embedded asset registry, populated at startup from
  vmux's own bundle dirs (`vmux_core/src/page.rs` `embed_page_static_assets`). Request
  paths resolve against embedded keys, not the live filesystem — no path-traversal file
  read via `vmux://`.

### Layer 1 (A1) — browser-process authoritative inbound gate

`bevy_cef_core/src/browser_process/client_handler.rs:231`
(`ClientHandlerBuilder::on_process_message_received`):

```rust
let url = frame.url().into_string();
if !crate::util::is_trusted_embedded_page(&url) {
    // dropped inbound '<name>' from untrusted url
    return 1;
}
if name == PROCESS_MESSAGE_BRP
    && embedded_page_host_of(&url).as_deref() != Some("debug")
{
    // dropped BRP from non-debug
    return 1;
}
```

This is the **authoritative** chokepoint, running in the **trusted** browser process,
covering `bin_emit` / `js_emit` / `brp` in one place:
- Every inbound process message is dropped unless its source frame URL is a trusted
  `vmux://<known-host>/` page (scheme + host allow-list via `is_trusted_embedded_page`).
- BRP — which proxies the **full Bevy Remote Protocol** (arbitrary ECS read/write/query/
  spawn) — is further restricted to the `debug` host only. No production page uses
  `window.cef.brp`.

The `url`/`host` is taken from the frame's real committed URL as known to the browser
process. JS cannot spoof it.

### Layer 2 (B1) — render-process scheme gate (defense in depth)

`bevy_cef_core/src/render_process/cef_api_handler.rs` — `execute_emit` (:142),
`execute_bin_emit` (:170), `execute_brp` (:102), `execute_listen` (:236):

```rust
if !crate::util::has_embedded_scheme(&frame.url().into_string()) {
    return 1; // no-op
}
```

`window.cef.*` is registered as a **global** V8 extension (`render_process_handler.rs:201`,
`CEF_API_EXTENSION_CODE`), so the `cef` object exists in *every* frame including
`https://` browse content — but each native call is inert unless the frame's URL uses the
embedded scheme. A malicious site that calls `window.cef.binEmit(...)` /
`window.cef.brp(...)` gets a silent no-op.

This layer lives in the *untrusted* render process and would fall to a renderer exploit;
it is backstopped by Layer 1, which is authoritative.

### Layer 3 — authoritative host attribution

`bevy_cef_core/src/browser_process/client_handler/bin_emit_event_handler.rs:84`:

```rust
let host = crate::util::embedded_page_host_of(&frame.url().into_string())
    .unwrap_or_default();
```

The source `host` carried on every `BinIpcEventRaw` is derived from the real frame URL by
the trusted browser process — never from a JS-supplied value. A non-`vmux://` frame
yields `host == ""`.

### Layer 4 — per-event host allow-list (least privilege)

`bevy_cef/src/common/ipc/bin_js_emit.rs:101,192` (`host_allowed` / `receive_bin_events`):
a decoded event is dropped unless its stamped `host` is in the owner set declared by
`BinEventEmitterPlugin::for_hosts(&[...])`. This binds each message type to the page(s)
allowed to emit it, so even a compromised *vmux* page cannot pivot into another page's
handlers.

Owner map (cross-checked against registrations in the workspace):

| Owner host(s)     | Event types                                                                                                   | Notable sink |
|-------------------|---------------------------------------------------------------------------------------------------------------|--------------|
| `terminal`        | `TermResizeEvent`, `TermMouseEvent`, `TermKeyEvent`                                                            | PTY write = shell exec |
| `services`        | `ProcessNavigateEvent`, `ProcessKillEvent`, `ProcessKillAllEvent`                                              | kill processes |
| `agent`           | `VibeInstallRunRequest`                                                                                        | runs a **constant** install command |
| `debug`, `layout` | `RestartRequestEvent`                                                                                          | relaunch via `sh` (fixed exe path) |
| `layout`          | `HeaderCommandEvent`, `SideSheetCommandEvent`, `TabsCommandEvent`                                              | nav / tab / pane commands |
| `spaces`, `layout`| `SpaceCommandEvent`                                                                                            | space CRUD |
| `settings`        | `SettingsCommandEvent`                                                                                         | settings mutate |
| `history`         | `HistoryQueryRequest`, `HistoryDeleteRequest`, `HistoryClearAllRequest`, `HistoryOpenRequest`, `HistoryChangedEvent` | history DB |
| `command-bar`     | `CommandBarActionEvent`, `PathCompleteRequest`, `CommandBarReadyEvent`, `CommandBarRenderedEvent`, `CommandBarSizeEvent`, `HistorySuggestionsRequest` | command-bar actions |
| `debug`           | `DebugUpdateReady`, `DebugUpdateClear`                                                                         | debug page |
| *(unrestricted — `owner_hosts: None`)* | `PageReady`, `AgentToast`                                                                 | harmless marker / toast text |

A malicious site produces `host == ""`, which is in **none** of the owner sets, so every
dangerous event is rejected at this layer even if Layers 1–2 were somehow bypassed.

### Outbound gates (Bevy → page) — defense in depth

The listener side is already entity-targeted (Bevy chooses the destination webview), and
is additionally scheme-gated so a stray message can never be delivered into untrusted
content:
- A2 (browser, authoritative): `browser_process/browsers.rs` `emit_event*` skip
  delivery if the destination main-frame URL is not trusted.
- B2 (render): `render_process/render_process_handler.rs:141`
  `on_process_message_received` drops listen/bin-listen/brp-response delivery unless
  `has_embedded_scheme`.
- B2b (render): `cef_api_handler.rs::execute_listen` refuses to register a listener on an
  untrusted frame.

### Safe decode

`receive_bin_events` decodes payloads with `rkyv` under the `CheckBytes` validator
(`bin_js_emit.rs:112,177`), so a malformed/hostile byte buffer cannot cause unchecked
deserialization / memory unsafety — it simply fails to decode and is dropped.

## No network-reachable control plane

A website's only remote-attack primitive is `fetch`/`WebSocket` to a local port. None of
the bridge control planes are reachable that way:

- **Bevy Remote Protocol (BRP)**: only `RemotePlugin::default()` is added; **no**
  `RemoteHttpPlugin` (`bevy_cef/src/lib.rs:95`). BRP has **no TCP listener** — it is
  reachable only via `window.cef.brp`, which is gated to the `debug` host. A site cannot
  `fetch` it.
- **vmux_service IPC**: a **Unix domain socket** in the profile dir
  (`vmux_service/src/service.rs:27`, `UnixListener::bind`). Browsers cannot open UDS.
- **MCP server** (`vmux_mcp`): **stdio** JSON-RPC, spawned as a subprocess of an agent
  (`vmux_mcp/src/protocol.rs:28` `run_stdio`). It exposes a `run` tool that can execute
  commands, but it is reachable only over the stdio pipe vmux hands to the agent it
  spawned — not over any socket, and not from web content.

## Shell / RCE sink inventory

The process/shell sinks, and what gates reaching them from the bridge:

| Sink | Code | Reachable from bridge via | Gate |
|------|------|---------------------------|------|
| Terminal PTY (write bytes = run shell) | `vmux_terminal/src/plugin.rs` (PTY spawn ~:872; `TermKeyEvent` handler ~:2432) | `TermKeyEvent` | host `terminal` only |
| Process kill | `vmux_terminal/src/processes_monitor.rs` (`on_process_kill*`) | `ProcessKillEvent` / `ProcessKillAllEvent` | host `services` only |
| App relaunch (`sh -c`) | `vmux_desktop/src/updater.rs:161` | `RestartRequestEvent` | hosts `debug`/`layout`; command built from real `current_exe()` path, no page input |
| Vibe installer | `vmux_vibe_setup/src/plugin.rs:22` → `RunShellRequest` | `VibeInstallRunRequest` | host `agent`; command is the **constant** `VIBE_INSTALL_COMMAND` |
| Agent runner (claude/codex/vibe CLIs) | `vmux_agent` → `vmux_service` | not directly; via service UDS / MCP stdio | not web-reachable |

`RunShellRequest { command, … }` is the one message carrying a free-form command string.
Its bridge-reachable writer (`VibeInstallRunRequest`) only ever passes a compile-time
constant; other writers are internal (agent/service). No bridge event lets web content
choose an arbitrary `command`.

## No privileged injection into browse pages

- vmux sets **no** `PreloadScripts` and **no** per-browser init script on browse
  webviews (none found in the workspace). The only injected global is the inert
  `window.cef` extension.
- No `innerHTML` / `dangerous_inner_html` / `execute_javascript` in app code; Dioxus
  escapes text by default, so attacker-influenced data shown on vmux pages (e.g. visited
  page titles/URLs on the history page) is not a script-injection vector today.

## Residual risks & assumptions

1. **Renderer sandbox escape (primary residual, inherent).** Layers 0/2 partly rely on
   the render process behaving. A V8/Chromium RCE in the *browse-tab* renderer could
   forge process messages — but Layer 1 (authoritative, browser-side host attribution)
   and Layer 4 still hold, because the browser process derives the host from the real
   frame URL, which a compromised non-`vmux://` renderer does not control. With Chromium
   **site isolation**, the `evil.com` renderer and the `vmux://` renderers are separate
   processes, so compromising the former does not yield control of the latter. To
   actually drive a dangerous handler an attacker would need to compromise the renderer
   of a specific trusted `vmux://` page (which loads only first-party content).
   *Mitigation:* keep CEF/Chromium current; do not disable site isolation.
2. **`owner_hosts: None` on `PageReady` and `AgentToast`.** Weaker than their peers (any
   trusted host accepted), but the payloads are non-dangerous (an empty readiness marker;
   toast text). Recommend declaring explicit `for_hosts` for consistency and to prevent a
   future dangerous field from inheriting the unrestricted default.
3. **History page is the highest-value injection target.** It renders attacker-influenced
   strings (titles/URLs of visited sites) on a privileged `vmux://history` origin. Safe
   today via Dioxus escaping — guard against any future raw-HTML rendering there.
4. **Trust in first-party page content.** The whole model assumes `vmux://` pages contain
   only vmux code. An XSS/content-injection bug in a vmux page would let injected script
   emit that page's allowed events. Keep vmux pages free of unsanitized HTML sinks.

## Invariants to preserve (checklist for future changes)

- Do **not** add `RemoteHttpPlugin` or any TCP listener for BRP/service/MCP.
- Keep the service transport a Unix domain socket; keep MCP on stdio.
- New bridge events that reach a sensitive sink **must** be registered with
  `for_hosts(&[…])` naming the minimum set of owner pages. Never use the unrestricted
  default for a dangerous type.
- Never relax the `vmux://` scheme flags away from `LOCAL` (it is the http→local
  isolation), and never serve attacker-controllable bytes under `vmux://`.
- Do not inject `PreloadScripts` / init scripts into browse (non-`vmux://`) webviews.
- Do not introduce `innerHTML` / `dangerous_inner_html` / `execute_javascript` with
  page-derived data.
- Keep CEF current; keep Chromium site isolation enabled.

## How to verify

- Unit (`cargo test -p bevy_cef_core` / `-p bevy_cef`): the scheme/host predicate table
  in the gate-design doc, plus `host_allowed` / `decode_bin_event` tests in
  `bin_js_emit.rs`.
- Manual (runtime): launch with `VMUX_WEBVIEW_DEBUG=1`, open a browse tab, and in
  devtools run `window.cef.brp({...})` and `window.cef.binEmit("…TermKeyEvent", …)`.
  Confirm the debug log shows the drop ("dropped inbound … from untrusted url" / host
  drop) and that there is no ECS/terminal effect. Then confirm the trusted pages
  (terminal, command-bar, history, settings, spaces) still function.
