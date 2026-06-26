# Media Permissions (camera / microphone / screenshare)

Date: 2026-06-25
Status: Approved — implementation in progress
Branch: `feat/media-permissions`

## Goal

Make `getUserMedia` / `getDisplayMedia` work in vmux's CEF webviews (Google Meet,
Zoom web, etc.) with **real per-origin permission prompts and persistence** —
behaving like Brave/Edge/Chrome rather than a blanket allow-all.

## Background

- CEF 148, Alloy runtime (`windowless_rendering_enabled: true`,
  `external_message_pump: true`). The CEF UI thread **is** the Bevy main thread
  (macOS pumps CEF from a main-thread CFRunLoop timer; `Browsers` is `NonSend`).
- vmux's CEF client (`patches/bevy_cef_core-0.5.2/.../client_handler.rs`)
  implements render/display/load/lifespan/request/focus handlers but **no
  permission handler**, so CEF denies all media access by default.
- No camera/mic usage strings or entitlements in the macOS bundle, so even a
  granted request would be killed by TCC.

## Scope

**In:** media access only — camera, microphone, screen share via
`on_request_media_access_permission`.

**Out (YAGNI):**
- Generic permission prompts (notifications, geolocation, clipboard) via
  `on_show_permission_prompt`.
- Per-window/tab screenshare picker. Alloy `getDisplayMedia` grants the **whole
  primary screen**; that is acceptable for v1 (Meet "present screen" works).

## Architecture

### Flow (the round-trip)

```
getUserMedia / getDisplayMedia (page)
  └─ CEF on_request_media_access_permission        [main thread, inside pump]
       ├─ PENDING.insert(id, callback)             // thread_local; callback is !Send, stays here
       ├─ channel.send(MediaPermissionRequest{ id, webview, origin, mask })
       └─ return 1                                 // resolved asynchronously
  └─ Bevy drain system                             [main thread]
       ├─ persisted grant for origin? ── yes ─→ resolve(id, granted_mask)
       └─ no ─→ BinEmit MediaPromptEvent → layout host renders bubble
  └─ user clicks Allow / Block (layout Dioxus)
       └─ js_emit MediaPermissionDecision{ id, allow } → Bevy observer
            ├─ persist per-origin
            └─ resolve(id, granted_mask): PENDING.remove(id).cont(mask)
```

`resolve` runs inside a Bevy system, i.e. on the same OS thread that owns the
`!Send` `MediaAccessCallback`. No cross-thread movement, no `unsafe`. The handler
**always** notifies Bevy (even for remembered origins); Bevy owns the
prompt-vs-auto-resolve decision so there is a single code path.

### 1. CEF layer — `patches/bevy_cef_core-0.5.2/`

- New `browser_process/permission_handler.rs`, mirroring `display_handler.rs`:
  - `PermissionHandlerBuilder { object, webview, sender }`.
  - `thread_local!` registry `HashMap<u64, MediaAccessCallback>` for pending callbacks.
  - `impl ImplPermissionHandler::on_request_media_access_permission`: allocate an
    id, stash the callback, send `MediaPermissionRequest`, return 1.
  - `pub fn resolve_media_permission(id, granted_mask)`: pop callback, `cont(mask)`.
  - `MediaPermissionRequest { webview: Entity, request_id: u64, origin: String, requested: u32 }`
    (mask uses `CEF_MEDIA_PERMISSION_DEVICE_AUDIO/VIDEO_CAPTURE` + `DESKTOP_*`).
- `client_handler.rs`: add `permission_handler: Option<PermissionHandler>` field,
  `with_permission_handler`, clone, and return from
  `ImplClient::permission_handler()` — mirrors the existing `request_handler`
  plumbing (fields ~104, builder ~153, clone ~199, getter ~222).
- `browsers.rs::client_handler()` (~1506): add a `media_permission_sender` param,
  wire `.with_permission_handler(PermissionHandlerBuilder::build(webview, sender))`.

### 2. bevy_cef glue — `patches/bevy_cef-0.5.2/`

- Expose the sender as a Bevy resource and the receiver for draining — mirror
  `cef_state.rs` `WebviewCefStateSender` (create `async_channel`, insert sender
  resource, hand sender to `client_handler`, expose receiver).

### 3. Persistence — per-origin store

- Profile-scoped store: `origin -> { camera, microphone, screen }`, each
  `Allow | Block`. Its own `.ron` (profile state like cookies, **not** user
  config). No auto-seed: absent origin/kind ⇒ prompt (per "No config auto-seed").
- Allow/Block writes the grant on click (Chrome-style remember).

### 4. Bevy round-trip — `crates/vmux_browser/`

- Drain `MediaPermissionRequest` each frame; consult the store; auto-resolve or
  `BinEmit` a `MediaPromptEvent` to the `layout` host (mirror
  `on_webview_ready_send_theme`, `lib.rs:92` emitter registration).
- Observer for `MediaPermissionDecision` from the page (mirror
  `on_header_command_emit`, `lib.rs:2707`): persist, then `resolve`.

### 5. Prompt UI — `crates/vmux_layout/`

- New Dioxus permission-bubble component in the layout page (soft-glass:
  translucent rounded panel, accent buttons): origin + requested devices +
  Allow/Block. Emits the decision via the existing `try_cef_bin_emit_rkyv`
  round-trip (`page.rs:303`).

### 6. macOS TCC — `packaging/macos/`

- `Info.plist`: `NSCameraUsageDescription`, `NSMicrophoneUsageDescription`.
- `Vmux.entitlements` **and** `VmuxDev.entitlements`:
  `com.apple.security.device.camera`, `.microphone`, `.audio-input`.
  `sign-and-notarize.sh` applies entitlements to helper apps (lines ~102-110),
  covering CEF's child-process device access; `sign-dev-mac.sh` uses
  `VmuxDev.entitlements` for local runs.
- Screenshare reuses the existing Screen Recording TCC grant (screenshot feature).

## Testing

- Native unit: mask mapping (requested↔granted), persistence get/set,
  auto-resolve-vs-prompt decision logic.
- Page-source assert tests for the bubble (`include_str!` pattern in
  `vmux_layout`).
- Final single manual pass (user): Meet — grant cam/mic, present screen; reload →
  no re-prompt; second origin → prompts fresh.

## Risks / assumptions

- **Main-thread resolution** (verified): if CEF ever moves off the external pump,
  the thread_local breaks. Debug-assert thread identity at resolve time.
- **Helper entitlement inheritance**: must confirm a signed/dev build actually
  receives the camera in a child process — verify in the final pass.
- Touching the patched CEF crate forces a large rebuild; implement directly
  (no subagents) with a warm target dir.
