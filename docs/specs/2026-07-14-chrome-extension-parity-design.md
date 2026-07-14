# Chrome Extension Behavioral Parity

Date: 2026-07-14
Status: approved design; implementation planning pending
Branch: `fix/ext-window-shim`

Supersedes:

- `docs/specs/2026-04-26-chrome-extensions-design.md`
- `docs/specs/2026-06-25-chrome-extensions-design.md`

## Decision

vmux will target behavioral and API compatibility with desktop Chrome extensions.
Extensions should run unchanged. vmux-native surfaces may replace Chrome's toolbar,
menus, notifications, permission prompts, and side panels, but observable extension API
behavior must match Chrome.

The implementation will use a central Rust compatibility service backed by canonical
window and tab models. A generated extension runtime will bridge only APIs that CEF does
not implement correctly. APIs that already behave correctly in CEF will remain native.

The existing `ext-window-shim` implementation is a useful experiment, not the parity
architecture. Its loader-generation work may be retained. Its synthetic window, inferred
`lastTab`, hard-coded bounds, and fake-success API behavior must be removed as the central
service replaces them.

## Current State

vmux already implements:

- Chrome Web Store URL and extension-ID parsing.
- CRX2 and CRX3 unpacking.
- Manifest parsing and extension metadata storage.
- Enable, disable, uninstall, and relaunch-required state.
- `--load-extension` and `--disable-extensions-except` wiring at CEF startup.
- `vmux://extensions`, Web Store injection, MCP installation, header action icons, and
  extension popup pages.
- MV3 classic and module service-worker loader generation.

The current shim patches parts of `chrome.windows` and `chrome.tabs` inside MV3 service
workers. It can generate and reload those workers, but its tests verify file rewriting,
not Chrome-compatible runtime behavior. Current local extension state has Bitwarden and
Vimium disabled, so there is no fresh end-to-end runtime verification.

Two prior design assumptions are obsolete:

1. Alloy-style browsers are not completely extension-free. Content scripts, service
   workers, storage, runtime messaging, and other process-level features can operate,
   while APIs that depend on Chromium's `Browser` and `TabStripModel` remain absent or
   incomplete.
2. A CEF V8 extension cannot be assumed to inject into MV3 service-worker contexts. The
   generated service-worker entry point is the reliable bootstrap location.

CEF does not expose a general-purpose Alloy tab model. vmux must provide the missing
browser semantics from its own ECS and layout state.

## Goal

For desktop extension APIs relevant to general browsing:

- Extensions require no vmux-specific source changes or setup.
- Return values, callbacks, Promises, events, errors, permissions, and lifecycle behavior
  match the embedded Chromium version.
- vmux layout state projects into a stable Chrome-like window and tab model.
- Extension-created browser operations mutate production vmux state through typed Bevy
  messages and systems.
- Every API method and event has an explicit, machine-readable support status.
- Support claims require differential tests against matching Chromium.

## Non-goals

- Reproducing Chrome's visual UI exactly.
- ChromeOS-only APIs.
- Enterprise-policy APIs when vmux has no equivalent managed-browser environment.
- Pretending unavailable platform functionality succeeded.
- Implementing all namespaces in one PR.
- Replacing native CEF behavior that already matches Chrome.

## Compatibility Contract

Each API method, property, and event is classified as one of:

- `Native`: CEF behavior matches Chrome and remains untouched.
- `Bridged`: vmux supplies behavior through the compatibility service.
- `Unsupported(reason)`: the platform cannot provide the behavior; calls fail with a
  Chrome-compatible error.
- `Untested`: behavior has not been compared against Chromium and cannot be advertised as
  supported.

API presence does not count as support. A namespace is complete only when every in-scope
entry is `Native`, `Bridged`, or explicitly `Unsupported`, with no `Untested` entries.

The capability matrix lives at
`crates/vmux_browser/src/extensions/capabilities.ron`, is parsed by Rust, and is consumed
by adapter generation and conformance tests. Every entry identifies the Chromium major,
platform, namespace, member, member kind, status, implementation owner, and conformance
scenario. Native behavior can change when CEF is upgraded, so a new Chromium major starts
with audited or `Untested` entries instead of inheriting old claims.

## Architecture

```text
Extension worker, page, popup, side panel, or content script
                         │
                         │ Chrome-compatible generated runtime
                         ▼
             Per-extension bridge context
                         │
                         │ authenticated request/event protocol
                         ▼
          Extension Compatibility Service (`vmux_browser`)
             ├── capability registry
             ├── API broker
             ├── permission enforcement
             ├── event router
             ├── Chrome window model
             ├── Chrome tab model
             └── extension surface state
                         │
                         │ typed Bevy messages and systems
                         ▼
          vmux ECS, layout, CEF browsers, and native UI
```

The browser process is authoritative. JavaScript adapters never directly mutate vmux
state and never invent successful results.

## Components

### Extension Registry

The existing extension store remains responsible for installation and enablement. It is
extended to track:

- Extension ID, version, manifest version, and package checksum.
- Declared permissions, optional permissions, and host permissions.
- Generated-runtime version and capability-matrix version.
- Enabled state per profile.
- Loaded package version and whether relaunch is required.

Installed source packages are immutable. Generated vmux files must not be written into
the source package.

```text
~/.vmux/extensions/
├── index.json
├── packages/
│   └── <extension-id>/<version>/source/
└── runtime/
    └── <profile>/<extension-id>/<runtime-hash>/
```

The runtime directory is generated from the immutable source package and contains the
patched manifest, adapter runtime, bridge page, and service-worker loader. CEF loads the
runtime directory. Updating an extension or adapter generates a new runtime hash instead
of chaining modifications onto prior generated files.

Existing directories containing `vmux_shim.json` are migrated by restoring the recorded
original worker. If the original package cannot be verified, vmux re-downloads the same
extension version or current Web Store version before generating a clean runtime. The
extension ID remains stable so Chromium-managed extension storage is preserved.

### Chrome Model

`ChromeModel` is a Rust resource containing the extension-visible browser state:

- `ChromeWindow`
- `ChromeTab`
- focus and last-focus history
- stable ID allocation
- tab ordering
- extension surface state

The model is projected from production ECS state. Projection systems observe layout,
navigation, title, loading, audio, focus, visibility, and browser lifecycle changes.

Stable extension IDs map to logical vmux entities, not `CefBrowser` instances. Recreating
a CEF browser for the same page must not change its extension-visible tab ID.

### API Broker

All bridged calls use one envelope:

```text
ApiRequest {
  protocol_version,
  request_id,
  extension_id,
  profile_id,
  context_id,
  namespace,
  method,
  arguments,
}
```

Responses contain either a serialized Chrome result or a structured Chrome error.
Mutation request IDs are deduplicated so reconnects cannot execute the same mutation
twice.

The broker:

1. Authenticates the bridge context.
2. Validates extension, profile, and context identity.
3. Checks manifest and host permissions.
4. Checks the capability matrix.
5. Dispatches a typed Bevy message to a namespace handler.
6. Returns the committed result after production state changes.

Namespace handlers live in focused modules such as `api/tabs.rs`, `api/windows.rs`, and
`api/action.rs`, with `api.rs` as their parent module. No handler directly edits unrelated
layout state.

### Generated Extension Runtime

The generated runtime loads before extension code and supports both MV3 worker forms:

- Classic worker: `importScripts` adapter, then original worker.
- Module worker: static adapter import, then original worker import.

The runtime:

- Overrides only capability entries classified as `Bridged` or `Unsupported`.
- Preserves native API objects and methods classified as `Native`.
- Supports callback and Promise call styles.
- Implements callback-scoped `chrome.runtime.lastError` behavior.
- Parses Chrome's optional argument forms.
- Registers bridged event listeners synchronously during worker startup.
- Reconnects after service-worker restart without relying on global worker state.
- Filters reserved bridge messages from extension-owned runtime listeners.

Unsupported methods reject or set `runtime.lastError`; they never resolve with invented
objects.

Injection is context-specific:

- Service workers use the generated classic/module entry point.
- Extension pages, popups, options pages, and side panels use CEF render-context injection
  before page scripts execute.
- When a bridged API is exposed to content scripts, the generated manifest prepends a
  `document_start` adapter in the extension's isolated world.

All three paths load the same generated protocol and capability definitions.

### Bridge Transport

CEF renderer contexts that support native process messaging use the existing CEF IPC
path. MV3 workers cannot depend on that path, so each enabled extension gets a hidden
generated bridge page under its own extension origin.

The bridge page:

- Maintains an authenticated loopback WebSocket to a listener owned by `vmux_browser`.
- Uses native `chrome.runtime` messaging to communicate with the extension worker and
  other extension contexts.
- Can wake a dormant worker by delivering a reserved runtime event.
- Does not keep the worker alive solely to maintain transport.

After CEF reports an enabled extension ready, vmux creates one hidden browser for its
bridge page. That browser is infrastructure: it is excluded from the Chrome tab model,
layout persistence, history, and user-facing page lists.

The generated manifest permits only the loopback connection required by the bridge. The
listener binds to loopback, selects an ephemeral port, and requires a per-launch,
per-profile, per-extension token. The token grants only that extension's declared API
permissions.

Wire messages use a versioned schema shared by CEF IPC and the WebSocket transport so API
handlers do not depend on transport details.

### Event Router

The router converts committed model changes into Chrome events. Events carry a monotonic
sequence number per profile and are emitted after state mutation, preserving causal
ordering.

Worker-directed events remain queued until the generated runtime acknowledges delivery.
Queues are bounded. Coalescible state updates may collapse to their newest value; tab
creation, removal, command invocation, permission changes, and user interactions are
never silently dropped.

Worker restart re-registers listeners synchronously, reconnects the bridge, and resumes
eligible pending events. The runtime does not replay events Chrome would not replay.

### Extension Surface Service

Chrome-owned surfaces map to vmux-native UI:

| Chrome surface | vmux surface |
|---|---|
| Action icon and badge | Header extension bar |
| Action popup | Anchored overlay containing an extension page |
| Context menu | Browser context menu section |
| Notification | Native vmux notification |
| Permission prompt | Modal vmux permission prompt |
| Side panel | vmux side sheet containing an extension page |
| Options page | Normal extension page stack |

Surface state is held in Rust so worker suspension does not discard badge, menu, popup,
or side-panel state.

## Browser Semantics

### Windows

Each native vmux window within a profile maps to one `ChromeWindow`.

- `id` is stable for the native window lifetime.
- Bounds come from the real native window.
- `focused` follows OS focus.
- `getCurrent` resolves from the caller context.
- `getLastFocused` uses recorded native focus history.
- Creating a Chrome window creates a vmux native window when supported.
- Unsupported window types or states return explicit errors.

No hard-coded ID, bounds, focus, or screen size is permitted.

### Tabs

Each web-page entry in a vmux stack maps to one `ChromeTab`. All web pages owned by the
native vmux window are visible to the API, including pages in background spaces and saved
vmux tabs. Background pages report `active: false`.

Included:

- HTTP pages.
- HTTPS pages.
- `chrome-extension://` pages.

Excluded:

- Terminals.
- Agents.
- Editors and file previews.
- Internal `vmux://` and `cef://` pages.

The active Chrome tab is the focused web page. When focus moves to a non-browser pane,
the most recently focused web page remains active for extension queries. This preserves
Chrome's invariant that a window with tabs has one active tab. If the window owns no web
pages, active-tab queries return an empty result.

Tab order is deterministic:

1. Space persistence order.
2. vmux tab order within the space.
3. Pane-tree visual order.
4. Stack order within each pane.

Core mutations map to production behavior:

- `tabs.create`: create a web page in the focused stack unless a target window or index
  specifies another placement. If no browser stack is focused, use the active pane's
  stack or create a browser stack through the normal layout command flow.
- `tabs.remove`: close the logical web page through the normal close flow.
- `tabs.move`: reorder within a stack or reparent to the stack represented by the target
  flattened index.
- `tabs.update`: navigate, activate, pin, mute, or update supported tab state.
- `tabs.duplicate`: create a new page with matching URL and supported state.
- `tabs.reload`: issue the production CEF reload request.
- `tabs.sendMessage`: use native runtime messaging when correct; bridge only divergent
  routing or sender metadata.

Content-script sender metadata is derived from the originating logical web page. It is
never inferred from the most recently observed sender.

### Extension Pages

Popups, side panels, options pages, offscreen documents, and hidden bridge pages are real
extension-origin contexts. They receive correct caller window and tab semantics. Popup
and side-panel contexts are not normal tabs unless explicitly opened as pages.

Opening an action popup as a normal page remains available as a diagnostic fallback, not
the primary user interaction.

## Error Semantics

The compatibility service distinguishes:

- Unknown or destroyed tab/window IDs.
- Missing manifest permission.
- Missing host permission.
- User-denied optional permission.
- Unsupported platform operation.
- Destroyed caller context.
- Bridge disconnection.
- Timeout.
- Invalid arguments.

Callback calls expose `chrome.runtime.lastError` only while the callback executes and
clear it immediately afterward. Promise calls reject with the corresponding error.

Queries may reconnect and retry when no observable mutation occurred. Mutations are not
blindly retried; deduplication by request ID permits safe response recovery.

## Security

- The loopback listener accepts no non-loopback connections.
- Tokens are random, scoped to one launch, profile, and extension, and rotated on relaunch.
- Bridge credentials are injected through a non-reflectable native preload scoped to the hidden
  bridge page's initial main-frame URL. They never appear in extension resources, page URLs,
  process arguments, subframes, later navigations, or logs.
- The broker derives authority from the installed manifest, never from request claims.
- Profile and extension IDs are validated before dispatch.
- The bridge exposes no arbitrary filesystem, process, shell, or raw ECS access.
- Native and optional permissions are enforced before invoking handlers.
- Sensitive tab fields follow Chrome's `tabs` and host-permission rules.
- Generated runtime directories are private to the local user and replaced atomically.
- Protocol versions reject incompatible clients instead of interpreting unknown fields.

## Differential Conformance Harness

A purpose-built MV3 extension runs identical scenarios in matching Chromium and vmux.
The embedded Chromium major version selects the baseline.

The harness records normalized:

- Return values.
- Callback versus Promise behavior.
- `runtime.lastError` lifetime and message.
- Event order and payloads.
- Permission behavior.
- Caller and sender metadata.
- Service-worker suspend, wake, and restart behavior.
- State observed before and after mutations.

Normalization removes only documented nondeterminism such as generated IDs, timestamps,
and platform-dependent pixel rounding. It must not hide missing fields, wrong ordering,
or incorrect errors.

Support for a capability entry requires a matching Chromium scenario or an explicit
reason why differential execution is impossible.

## Additional Testing

### Unit

- Stable ID allocation and entity reuse.
- ECS-to-Chrome model projection.
- Tab flattening and reorder rules.
- Permission and host-permission evaluation.
- Capability-matrix parsing and coverage.
- Protocol serialization and version rejection.
- Callback/Promise/error adapter behavior.
- Immutable source and generated-runtime migration.

### ECS integration

- Typed API request messages invoke production systems.
- Tab/window mutations produce committed model state and correctly ordered events.
- Destroyed entities produce Chrome errors.
- Surface API calls update header, menu, notification, prompt, and side-sheet state.

### Transport

- Authentication and extension/profile isolation.
- Reconnect, timeout, cancellation, and mutation deduplication.
- Worker suspension and wake delivery.
- Queue bounds and non-droppable event behavior.

### End to end

Representative smoke coverage includes:

- Bitwarden.
- Vimium.
- uBlock Origin.
- React Developer Tools.

These are regression fixtures, not the definition of compatibility. The capability
matrix and differential harness define compatibility.

## Delivery Stages

This design is a parent architecture, not one implementation PR. Each stage receives its
own implementation plan and review.

### Stage 0 — Foundation

- Capability matrix.
- Chromium differential harness.
- Immutable source/runtime store split.
- Versioned protocol.
- Authenticated bridge page and transport.
- Canonical window and tab model.

Exit criteria: a fixture extension can query model state, receive a bridged event after a
worker restart, and produce Chromium-comparable recordings.

### Stage 1 — Browser core

- Audit native `runtime`, `storage`, `scripting`, and messaging behavior.
- Bridge divergent `runtime` behavior.
- Complete `tabs` and `windows` methods and events.
- Implement `action` state, anchored popup, badge, and icon behavior.
- Integrate extension `commands` with vmux keybindings.

Exit criteria: no `Untested` entries in these namespaces and matching conformance results
for all in-scope entries.

### Stage 2 — User interaction

- `contextMenus`.
- `notifications`.
- `permissions`.
- `sidePanel`.
- Options-page behavior.

Exit criteria: extension interactions use vmux-native surfaces with matching API state and
events.

The current synthetic shim is removed when Stage 1 covers its call paths. It must not
remain as a fallback that silently returns fake data.

### Stage 3 — Browser data and navigation

- `webNavigation`.
- `webRequest` and `declarativeNetRequest` audit and bridging.
- `downloads`.
- `bookmarks`.
- `history`.
- `sessions`.

Exit criteria: browsing, blocker, download, and navigation extensions operate through
documented compatible APIs.

### Stage 4 — Specialized desktop APIs

- DevTools extension surfaces.
- Offscreen documents.
- Native messaging.
- Incognito/profile behavior.
- Remaining desktop namespaces.

ChromeOS-only and enterprise-only APIs remain explicitly unsupported unless vmux later
adds the required platform model.

## Acceptance Criteria

- Extensions run without vmux-specific source changes.
- No bridged API returns invented success.
- Window and tab responses come from committed production state.
- Every completed namespace has zero `Untested` entries.
- Every bridged capability has differential or equivalent contract coverage.
- Worker restart does not lose durable extension surface state or eligible pending events.
- Extension/profile isolation tests pass.
- Representative smoke extensions pass their documented scenarios.
- Existing Web Store install, manager, MCP, and header behavior continue to work.
- macOS and Linux behavior is separately classified and tested.

## Sources

- CEF issue: Alloy-style browsers need a general-purpose Tab API —
  <https://github.com/chromiumembedded/cef/issues/4011>
- Chrome Extensions API reference —
  <https://developer.chrome.com/docs/extensions/reference/api>
- Extension service-worker lifecycle —
  <https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle>
- WebSockets in extension service workers —
  <https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets>
- Extension service-worker script loading —
  <https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/basics>
- Extension permissions —
  <https://developer.chrome.com/docs/extensions/develop/concepts/declare-permissions>
