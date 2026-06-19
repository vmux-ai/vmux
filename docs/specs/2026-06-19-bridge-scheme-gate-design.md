# Bridge scheme gate — restrict the Bevy↔Dioxus bridge to `vmux://` frames

Date: 2026-06-19

## Problem / threat model

The `window.cef` bridge (`brp` / `emit` / `binEmit` / `listen` / `binListen`) is injected
into **every** CEF frame as a global V8 extension (`v8/bevy-cef-api`, registered in
`on_web_kit_initialized`). Nothing checks the source frame's URL. So untrusted web content
loaded in a browse tab (or an iframe inside any page) can drive the bridge:

- `window.cef.brp({...})` proxies the **full Bevy Remote Protocol** → arbitrary ECS
  read/write (history, spaces, settings, terminals). Info theft + manipulation.
- `window.cef.binEmit("…RestartRequestEvent", …)` / `HistoryClearAllRequest` /
  `ProcessKillAllEvent` fire global observers that ignore the source entity.

Inbound messages are only loosely scoped: the browser process stamps the source `Entity`
and observers *may* read `trigger.event_target()`, but several global-action observers
ignore it entirely, and the decode step keys purely on the rkyv type-name id — never on the
origin scheme.

A page is **trusted iff its frame URL is `vmux://<known-host>/`**. The `vmux://` scheme is a
registered custom scheme served only from bundled assets, so a web page can never *be* at a
`vmux://` URL — scheme is a sound trust boundary. Per-frame URL means an `evil.com` iframe
inside a vmux page is correctly rejected.

No production code calls `window.cef` from http(s)/browse pages, so gating breaks nothing
legitimate. All bridge-using vmux pages are served from `vmux://<host>/`.

## Shared predicate (`bevy_cef_core/src/util.rs`)

Pure, unit-tested, explicit-arg core + thin wrappers over `resolved_cef_embedded_page_config()`:

```rust
pub fn url_has_embedded_scheme(url: &str, scheme_prefix: &str) -> bool;          // scheme-only
pub fn url_is_trusted_embedded_page(url, scheme_prefix, hosts) -> bool;          // scheme + host allowlist
pub fn has_embedded_scheme(url: &str) -> bool;                                   // render wrapper
pub fn is_trusted_embedded_page(url: &str) -> bool;                              // browser wrapper
```

Host is parsed as the segment after the prefix up to the first `/`, `?`, or `#`. Empty
prefix → always false.

| url | `has_embedded_scheme` | `is_trusted_embedded_page` |
|---|---|---|
| `vmux://history/` | ✓ | ✓ |
| `vmux://history/sub?x=1` | ✓ | ✓ |
| `vmux://history?x=1` | ✓ | ✓ |
| `vmux://unknown/` | ✓ | ✗ (allowlist) |
| `vmux://` (bare) | ✓ | ✗ |
| `vmux:evil` (no `//`) | ✗ | ✗ |
| `https://evil.com/` | ✗ | ✗ |
| `about:blank` / `""` | ✗ | ✗ |

Browser process (A1, A2) uses the host allowlist (`CefEmbeddedHosts` populated there).
Render process (B1, B2, B2b) uses scheme-only — the host list is not populated in the render
process, and the browser side enforces hosts authoritatively.

## Gate map (all in `bevy_cef_core`)

### Inbound — page → Bevy
- **A1 — browser, authoritative.** `browser_process/client_handler.rs::on_process_message_received`:
  resolve `frame.url()`; if `!is_trusted_embedded_page`, `webview_debug_log` and return without
  dispatching. One chokepoint covering `bin_emit` / `js_emit` / `brp` handlers.
- **B1 — render, defense in depth.** `render_process/cef_api_handler.rs` `execute_emit` /
  `execute_bin_emit` / `execute_brp`: after `context.frame()`, no-op if `!has_embedded_scheme`.

### Outbound — Bevy → page (the listener side)
- **A2 — browser, authoritative.** `browser_process/browsers.rs` `emit_event` /
  `emit_event_raw_json` / `emit_event_bytes`: each resolves `browser.client.main_frame()`;
  skip `send_process_message` if that frame's URL is not trusted. Mirror of A1.
- **B2 — render, defense in depth (delivery chokepoint).** `render_process/render_process_handler.rs::on_process_message_received`:
  has the destination `frame`; drop `handle_listen_message` / `handle_bin_listen_message` /
  brp-response delivery if `!has_embedded_scheme`.
- **B2b — render, registration.** `cef_api_handler.rs::execute_listen`: look up the current
  frame; no-op registering a listener on an untrusted frame.

BRP is fully covered inbound at A1 (no accepted request ⇒ no response). Outbound is already
entity-targeted; A2/B2 are defense in depth against future mis-targeting.

## Per-page message ownership (least privilege)

The scheme/host gate above lets *any* trusted `vmux://` page emit *any* registered message
type. A second layer binds each message type to the page(s) that legitimately emit it, so a
compromised vmux page cannot pivot to another page's handlers (e.g. a history page triggering
`RestartRequestEvent`).

Mechanism:
- `BinIpcEventRaw` carries the source `host` (stamped in `bin_emit_event_handler` from
  `frame.url()`).
- `BinEventEmitterPlugin::for_hosts(&["..."])` declares owner host(s); `receive_bin_events`
  drops a decoded event whose `host` is not in the owner set. No owner set (`default()` /
  `with_id`) = unrestricted (shared types like `PageReady`).

Owner is the *emitting* page, derived from the `try_cef_bin_emit_rkyv` call sites:

| Owner host(s) | Types |
|---|---|
| settings | `SettingsCommandEvent` |
| agent | `VibeInstallRunRequest` |
| spaces, layout | `SpaceCommandEvent` |
| terminal | `TermResizeEvent`, `TermMouseEvent`, `TermKeyEvent` |
| services | `ProcessNavigateEvent`, `ProcessKillEvent`, `ProcessKillAllEvent` |
| debug, layout | `RestartRequestEvent` |
| layout | `HeaderCommandEvent`, `SideSheetCommandEvent`, `TabsCommandEvent` |
| debug | `DebugUpdateReady`, `DebugUpdateClear` |
| command-bar | `CommandBarActionEvent`, `PathCompleteRequest`, `CommandBarReadyEvent`, `CommandBarRenderedEvent`, `CommandBarSizeEvent`, `HistorySuggestionsRequest` |
| history | `HistoryQueryRequest`, `HistoryDeleteRequest`, `HistoryClearAllRequest`, `HistoryOpenRequest`, `HistoryChangedEvent` |
| (unrestricted) | `PageReady`, `AgentToast` |

`HistorySuggestionsRequest` is emitted by the command-bar page, so it is split out of the
history registration into its own command-bar-owned registration.

BRP lockdown: A1 additionally drops `PROCESS_MESSAGE_BRP` from any host other than `debug`
(no production page uses `window.cef.brp`).

## Non-goals
- No change to per-entity routing of legitimate messages.
- No change to outbound targeting logic (Bevy still chooses target entities).

## Tests
- Unit (`cargo test -p bevy_cef_core`): the predicate table above (explicit-arg forms, no CEF
  runtime needed) — mirrors the existing `decode_bin_event` tests.
- Manual (runtime): `VMUX_WEBVIEW_DEBUG=1`, open a browse tab → devtools console →
  `window.cef.brp({...})` / `window.cef.binEmit(...)` → confirm debug log shows the drop and
  there is no ECS effect. Then confirm command-bar, history, settings, terminal, spaces still
  work normally.
