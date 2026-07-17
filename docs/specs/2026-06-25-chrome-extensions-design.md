# Chrome extension support — side-loaded Web Store installer + top-right surface

> Superseded by
> [`2026-07-14-chrome-extension-parity-design.md`](2026-07-14-chrome-extension-parity-design.md).
> The installer and UI work remain relevant, but the claim that Alloy panes have no
> extension support is obsolete.

Date: 2026-06-25
Branch: `chrome-extensions`
Status: design — approved (UI shape + scope confirmed); pending spike + spec review

## Motivation

vmux is a CEF-based browser but cannot run Chrome extensions. Users want common
extensions (ad-blockers, dev tools, password managers, dark-mode) in their browse
panes. The ask: install an extension from the **Chrome Web Store**, drive the install
**via an MCP tool** (so the agent can install on command), and surface installed
extensions in the **top-right of the header, right of the profile avatars** — one
action icon per extension, plus a manager entry point.

This mirrors the proven `vmux://lsp` Mason-manager pattern (download → unpack →
managed dir → manager page over the rkyv bin-event channel), applied to extensions.

## Feasibility (verified 2026-06-25) — this shapes the whole design

CEF here is **148** (`cef`/`cef-dll-sys` `148.2.0+148.0.8`), i.e. past **M128**.

- The **Alloy bootstrap was removed at M128**; only the **Chrome bootstrap** remains
  (vmux already runs it). The legacy `CefRequestContext::LoadExtension` (Alloy-only)
  is **gone**.
- **Chrome extensions are supported — but only in Chrome-style browsers.**
  Windowless/OSR forces **Alloy style**, which has **no extension support**.
- vmux's **windowed browse panes (macOS, `InteractionMode::User`)** are Chrome-style
  → extensions run there. **OSR/3D panes and Linux are Alloy/windowless → no
  extensions.** This is an accepted, documented limitation, not a bug to fix.
- Chrome-runtime extension loading is via the **`--load-extension=<dirs>` command-line
  switch read at CEF init**. There is **no sanctioned runtime "install a `.crx` from
  the Web Store" API**, and CEF disables the in-app Web Store / `chrome://extensions`
  install flow. So we **side-load unpacked extensions**, and a freshly installed
  extension **activates after relaunch**.

Sources:
- cef-announce: Alloy bootstrap deprecated/removed (M128); Chrome extension API is
  Chrome-style only — <https://groups.google.com/g/cef-announce/c/s1WaovAopFo>
- cef#3685 (Delete Alloy bootstrap, M128) — <https://github.com/chromiumembedded/cef/issues/3685>
- cef#3529 (Alloy extension handler vs `--enable-chrome-runtime`) — <https://github.com/chromiumembedded/cef/issues/3529>

### Consequences

1. **Install ≠ activate.** Install downloads + unpacks; activation needs a relaunch
   (the dir set must be present at CEF init). The UI makes "Relaunch to apply"
   explicit and tracks a pending/dirty set.
2. **Scope is windowed browse panes.** Icons and popups only make sense where
   extensions actually run. We do not pretend support in OSR/3D/Linux.
3. **No Chrome toolbar.** vmux draws its own header, not Chrome's toolbar, so
   extension *action popups* never auto-render. We surface our own action icons and
   open each extension's popup **as a page** (`chrome-extension://<id>/<popup>`).

## Goals

- **MCP installer**: `browser_install_extension { source }` (Web Store URL or 32-char
  ID) installs an extension; `browser_list_extensions` reports installed state.
- **Web Store source**: resolve URL/ID → download the `.crx` from Google's CRX
  endpoint → parse the **CRX3** container → unpack to a managed dir.
- **Managed store**: `~/.vmux/extensions/<id>/` + an index (name, version, action
  icon, popup path, enabled). Mirrors the `~/.vmux/lsp/` store shape.
- **CEF load**: pass enabled extension dirs via `--load-extension` at init; they run
  in Chrome-style windowed browse panes.
- **Top-right header surface**: one **action icon per enabled extension** (from its
  manifest), then a **puzzle "manage" button**, placed right of the avatars.
  - Click an extension icon → open `chrome-extension://<id>/<default_popup>` in a new
    stack (its popup UI, as a page).
  - Click the puzzle → open the **`vmux://extensions`** manager page.
- **Manager page `vmux://extensions`**: install field (paste URL/ID), installed list
  with enable/disable + uninstall, streamed install progress, "Relaunch to apply".

## Non-goals (v1)

- **Auto-update.** Side-loaded extensions are frozen; updating = re-fetch (manual).
- **Catalog browsing.** Install is by URL/ID only — no curated/searchable catalog
  (unlike `vmux://lsp`, which reuses mason-registry). May come later.
- **Options pages / permissions UI / sync.** (`options.html` could be opened as a page
  later, same mechanism as popups.)
- **Extensions in OSR/3D panes or on Linux.** Not possible with the Chrome-style
  constraint; explicitly out.
- **Chrome Web Store in-app install UX** (the real "Add to Chrome" flow) — disabled by
  CEF; we side-load instead.

## Architecture

Same host/page split as other `vmux://` pages, and the same
spawn→thread→outbox→drain→emit install pipeline as `vmux://lsp`. Transport is the
existing rkyv bin-event channel; downloads use a **blocking** HTTP client on a worker
thread (reuse the LSP `download`/`archive` stack). **No new crate.**

### Crate placement

- **Native (install engine + CEF load wiring + manager backend)**: `vmux_browser`
  (it owns CEF / `Browsers` / the `CefPlugin` config site). New module tree
  `vmux_browser/src/extensions/` (filename-based modules, no `mod.rs`).
- **WASM (manager page + header `ExtensionBar`)**: `vmux_layout` — it is dual-target,
  owns the header (`page.rs`) where the icons live, and already builds a wasm page.
  Keeping the action-icon bar and the manager page together (both browser-chrome UI)
  is cohesive.
- **Contract**: `vmux_core/src/event.rs` (rkyv + serde). `vmux_layout` and
  `vmux_browser` communicate only through these typed events — serde field contract,
  **no `vmux_browser ↔ vmux_layout` dependency cycle**.

(Alternative considered: reuse `vmux_editor`'s Mason scaffolding for the page. Rejected
— extensions are browser-domain, not editor; `vmux_layout` keeps header + page
together. Final call at spec review.)

### Modules (native, `cfg(not(wasm32))`, `vmux_browser/src/extensions/`)

- `webstore.rs` — resolve a Web Store URL or bare 32-char ID → the CRX download URL
  (`https://clients2.google.com/service/update2/crx?response=redirect&acceptformat=crx2,crx3&prodversion=<ver>&x=id%3D<ID>%26installsource%3Dondemand%26uc`).
- `crx.rs` — parse the **CRX3** container: magic `Cr24` (`0x43723234`), `u32` version
  (== 3), `u32` header length → skip the signed header → the remaining bytes are a ZIP;
  unzip to the package dir. (CRX2 fallback: magic `Cr24`, version 2, `pubkey_len` +
  `sig_len` → skip → ZIP.)
- `manifest.rs` — parse the extension `manifest.json`: `name`, `version`,
  `action.default_icon` (or `browser_action` MV2) → best icon path, `action.default_popup`,
  top-level `icons`. Resolve the icon to a path under the package dir.
- `store.rs` — managed layout `~/.vmux/extensions/{ <id>/, staging/, index.json }`;
  install/uninstall; enable/disable; `enabled_dirs()` for the `--load-extension` switch;
  `installed()` for the list. `index.json`: `id → { name, version, enabled, popup,
  icon_rel }`.
- `install.rs` — the engine: resolve → download (blocking, worker thread) → parse CRX
  → unzip into `staging/` → read manifest → atomically move into `<id>/` → update
  index → emit progress/status. Failures clean up `staging/`.
- `load.rs` — produce the `--load-extension` value from `store::enabled_dirs()` and
  inject it into CEF's browser-process command line at init.
- `manager_page.rs` — `PageManifest { host: "extensions", title: "Extensions",
  command_bar: true }`; claim `vmux://extensions/` in `PageOpenSet::HandleKnownPages`;
  observers for install/list/uninstall/toggle/relaunch; an outbox drained each frame →
  `BinHostEmitEvent`. (Mirrors `vmux_editor/src/lsp/manager_page.rs`.)

### CEF load wiring (`vmux_browser`)

- At browser-process command-line processing, append
  `--load-extension=<dir1,dir2,...>` from `store::enabled_dirs()` (the `bevy_cef`
  browser-process `OnBeforeCommandLineProcessing` path; today the only CEF config site
  is `CefPlugin { .. }` in `vmux_browser/src/lib.rs`). Optionally also
  `--disable-extensions-except=<same dirs>` to keep the set deterministic.
- Extensions attach at the **request-context** level (the shared disk context), so a
  single load covers all browse panes in the profile. They are inert in
  windowless/Alloy panes — acceptable.

### Header surface (`vmux_layout/src/page.rs`, after `TeamFacepile` at ~`:274`)

- New `#[component] ExtensionBar { extensions: Vec<ExtPackage> }`: render one `Icon`
  (or `<img>` from the manifest icon bytes/path) per **enabled** extension, then a
  puzzle "manage" button. Flex, `shrink-0`, consistent with the facepile/nav buttons.
- Extension icon `onclick` → `try_cef_bin_emit_rkyv(&ExtActionEvent { id })`.
- Puzzle `onclick` → a header/command event that opens `vmux://extensions/`.
- The header page receives the enabled-extension list the same way it receives `team`
  members: a host-pushed `ExtListEvent` (drained from the store, emitted to the layout
  CEF shell). Extensions with no `action`/popup still load but show **no icon**.

### Host handlers (`vmux_browser`)

- `ExtActionEvent { id }` → look up `popup` in the store → `AppCommand::Browser(
  BrowserCommand::Open(OpenCommand::InNewStack { url: Some("chrome-extension://<id>/<popup>") }))`.
- Puzzle/open → `BrowserCommand::Open(InNewStack { url: "vmux://extensions/" })`
  (mirrors `vmux_team`'s `on_team_command` "open").
- Stack-icon arm for `url.starts_with("vmux://extensions")` (and a generic icon for
  `chrome-extension://`).

### MCP tool (`vmux_mcp/src/tools.rs`)

- `browser_install_extension { source: String }` — `McpParamTool` variant +
  `to_agent_command` arm (validate non-empty) → `AgentCommand::BrowserInstallExtension
  { source }`.
- `browser_list_extensions` — query → `AgentQuery::ListExtensions` →
  `ExtListEvent`-shaped result.
- `vmux_agent::handle_agent_commands` gains arms writing `ExtInstallRequest` /
  reading the store, exactly like the existing browser command fan-out.

### Data / event contract (`vmux_core/src/event.rs`, rkyv + serde)

- `ExtInstallRequest { source: String }` (URL or ID).
- `ExtInstallProgress { id_or_source, phase (Resolving/Downloading/Unpacking/
  Done/Failed), pct: Option<u8>, message }`.
- `ExtStatusEvent { id, status (Installing/Installed/Failed/Enabled/Disabled),
  version: Option<String> }`.
- `ExtListEvent { extensions: Vec<ExtPackage> }`;
  `ExtPackage { id, name, version, icon: Option<String>, popup: Option<String>,
  enabled: bool, status }`.
- `ExtActionEvent { id }` (header icon click → open popup page).
- `ExtToggleRequest { id, enabled }`, `ExtUninstallRequest { id }`,
  `ExtRelaunchRequest`.
- Event-name consts alongside the existing `LSP_*` ones
  (`EXT_LIST_EVENT`, `EXT_INSTALL_PROGRESS_EVENT`, `EXT_STATUS_EVENT`).

### Page UX (`vmux://extensions`)

- **Install row**: a text field ("paste Chrome Web Store URL or extension ID") + Add →
  `ExtInstallRequest`.
- **Installed list**: per row — icon, name, version, enable/disable toggle, uninstall;
  status badge (Installing w/ progress / Installed / Disabled / Failed).
- **Pending banner**: when the enabled set differs from what's loaded → "N changes
  pending — Relaunch to apply" + a Relaunch button.
- **Style**: soft-glass (translucent rounded panes, accent pills, SVG icons),
  keyboard column navigation — consistent with other vmux pages.

### Relaunch

- Install / toggle / uninstall sets a **dirty** flag (enabled set ≠ loaded set).
- "Relaunch to apply" → `ExtRelaunchRequest`. If vmux exposes an app-relaunch path,
  call it; otherwise the banner instructs the user to restart vmux. (Confirm whether a
  relaunch command exists during the plan; if not, v1 ships the instruction + a clean
  restart, relaunch automation is a follow-up.)

### Error handling

- URL/ID parse failure → page error, no install.
- CRX endpoint failure / non-CRX response → surfaced with retry; offline → clear error.
- CRX3 magic/version mismatch or truncated header → abort, error badge.
- Unzip/permission/partial failures → `staging/` cleanup, error surfaced, never crash
  (fault-isolated on the worker thread).
- Popup nav to a `chrome-extension://` URL for a not-yet-loaded (pending) extension →
  the page should explain "relaunch to activate" rather than show a blank tab.

### Testing

- **webstore**: URL/ID → CRX download URL builder (accepts
  `https://chromewebstore.google.com/detail/<slug>/<id>`, legacy
  `chrome.google.com/webstore/detail/...`, and bare 32-char IDs; rejects junk).
- **crx**: a CRX3 fixture (header + tiny ZIP) → unpacks to the expected files; magic /
  version / header-length validation; CRX2 fallback.
- **manifest**: parse MV3 `action.default_icon`/`default_popup` and MV2
  `browser_action`; pick best icon; missing-action → no icon.
- **store**: index round-trip; enable/disable changes `enabled_dirs()`; uninstall
  removes dir + index entry; dirty-flag logic.
- **message/system (ECS)**: `ExtInstallRequest` → install system → `ExtStatusEvent`
  (fixture-served CRX over local HTTP); `ExtActionEvent { id }` → asserts a
  `BrowserCommand::Open(InNewStack)` with the correct `chrome-extension://<id>/<popup>`
  URL; puzzle-open → `vmux://extensions/`. (Per project rule: typed messages + systems,
  not ad hoc helper calls.)
- **manual (user, end)**: install uBlock Origin from a Web Store URL → "relaunch to
  apply" → relaunch → icon appears top-right → click icon → popup page renders →
  content blocking works in a windowed browse pane; OSR/3D pane shows no extension
  (expected).

## Build order (single PR on `chrome-extensions`)

Spike first; it gates everything. Then build bottom-up, automated-test-heavy (the user
runtime-tests once at the end).

- **B0 — SPIKE (do before committing to the rest):** hardcode one unpacked extension
  dir into `--load-extension` at init and confirm it **loads and runs** in a vmux
  windowed browse pane on this CEF 148 build (content script fires; `chrome-extension://
  <id>/<popup>` renders as a page). If it does not work windowed/Chrome-style as
  expected, stop and reshape (e.g. Views-based Chrome-style browser view, or accept
  "install-only, no in-app run"). **Everything below assumes B0 passes.**
- **B1 — install engine + store**: `webstore` + `crx` + `manifest` + `store` +
  `install` (download → CRX3 unpack → managed dir → index), with unit/fixture tests.
- **B2 — CEF load**: `--load-extension` from `enabled_dirs()` at init; relaunch
  applies the set.
- **B3 — MCP tool**: `browser_install_extension` / `browser_list_extensions` →
  `AgentCommand`/`AgentQuery` → `vmux_agent` fan-out → install/list.
- **B4 — manager page `vmux://extensions`**: backend manifest + claim + observers +
  outbox; wasm Dioxus page (install field, list, toggle, uninstall, relaunch banner);
  stack icon; `vmux_server` `web_pages!` registration.
- **B5 — header `ExtensionBar`**: action icons + puzzle, right of the avatars; host
  push of `ExtListEvent` to the layout shell; icon→popup-page, puzzle→manager page.

## Open items for the plan

- **B0 spike result** — confirm windowed Chrome-style extension loading; record the
  exact CEF flags/style needed. Reshape scope if it fails.
- Exact `bevy_cef` command-line injection point for `--load-extension` (patched crate).
- Whether an app-relaunch command exists (else ship the restart instruction).
- How the layout CEF shell currently receives `team` members, to mirror that push for
  `ExtListEvent` (enabled-extension list → header).
- Icon delivery to the wasm header: serve the manifest icon via the embedded-asset /
  `chrome-extension://` path vs. inlining bytes in `ExtPackage`.
- Final crate placement decision (`vmux_layout` vs `vmux_editor`) for the page.
- Confirm the CRX `prodversion` value to send (derive from the embedded CEF/Chromium
  version) so the endpoint serves a current CRX3.
