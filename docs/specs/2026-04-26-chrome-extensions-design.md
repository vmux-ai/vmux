# Chrome Extensions Support Design

## Goal

Enable Chrome Web Store extension support in vmux. Users install extensions from the Web Store via an in-app flow, extension icons appear in the header toolbar with badge support, and extensions like Bitwarden and Vimium work via a chrome.tabs bridge that maps CEF browsers to the extension tab API.

## Constraints

- vmux renders all browsers as Alloy-style OSR (off-screen rendering) to Bevy textures. This cannot change.
- Alloy-style browsers are not Chrome tabs. `chrome.tabs` API does not work natively with them.
- CEF 145 supports Chrome bootstrap + Alloy-style browsers (since M125). Chrome bootstrap enables extension loading at the process level while Alloy-style preserves OSR rendering.
- `chrome.storage` and `chrome.runtime` are process-level APIs expected to work natively with Chrome bootstrap.
- `chrome.tabs` requires a V8 polyfill + IPC bridge to vmux's ECS.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    CEF Process                       │
│  CefSettings { chrome_runtime: true }                │
│  --load-extension=/path/to/ext1,/path/to/ext2        │
│                                                       │
│  ┌──────────────┐  ┌──────────────┐                  │
│  │ Alloy OSR    │  │ Alloy OSR    │  <- web pages    │
│  │ Browser 1    │  │ Browser 2    │                   │
│  │ (content     │  │ (content     │                   │
│  │  scripts     │  │  scripts     │                   │
│  │  injected)   │  │  injected)   │                   │
│  └──────────────┘  └──────────────┘                  │
│                                                       │
│  Extension Service Workers                            │
│  ├── chrome.storage   (native, Chrome bootstrap)     │
│  ├── chrome.runtime   (native, Chrome bootstrap)     │
│  └── chrome.tabs      (V8 polyfill -> vmux IPC)      │
└─────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────┐
│  vmux_header (Dioxus WASM)                           │
│  ┌──────────────────────────────┬───────────────────┐│
│  │  [address / controls]        │ [ext1] [ext2 3]   ││
│  └──────────────────────────────┴────────┬──────────┘│
│                                          │ click      │
│                                          ▼            │
│                                   ┌────────────┐     │
│                                   │ popup.html │     │
│                                   │ (CEF OSR)  │     │
│                                   │ dropdown   │     │
│                                   └────────────┘     │
└─────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────┐
│  Extension Manager (vmux_desktop ECS resource)       │
│  ~/Library/Application Support/vmux/extensions/      │
│  ├── Extension entities (ECS, persisted via moonshine-save) │
│  ├── <extension_id>/manifest.json, icons/, ...        │
│  ├── CRX download + unpack                            │
│  └── Builds --load-extension from ExtensionEnabled query │
└─────────────────────────────────────────────────────┘
```

## CEF Configuration Changes

### Settings (message_loop.rs)

Enable Chrome bootstrap. The `chrome_runtime` field was removed from CefSettings in CEF 128+. Instead, pass `--enable-chrome-runtime` via `CommandLineConfig`, or check if the `cef` v145.6.1 Rust crate exposes a `chrome_runtime` field on `Settings`. If neither works, Chrome bootstrap may be the default in CEF 145 (Alloy bootstrap was deleted in M128).

Verification step: build with no changes and check if `--load-extension` works. If CEF 145 already uses Chrome bootstrap by default, no settings change is needed.

### WindowInfo (browsers.rs)

Set `runtime_style: RuntimeStyle::ALLOY` explicitly on all `WindowInfo` structs. Currently defaults to `zeroed()` which maps to `CEF_RUNTIME_STYLE_DEFAULT(0)`. Making it explicit ensures OSR behavior is preserved regardless of bootstrap mode.

```rust
WindowInfo {
    windowless_rendering_enabled: true as _,
    external_begin_frame_enabled: false as _,
    runtime_style: RuntimeStyle::ALLOY,
    ..Default::default()
}
```

### CommandLineConfig (browser.rs)

Pass installed extension paths:

```rust
CefPlugin {
    command_line_config: CommandLineConfig::default()
        .with_switch_value("load-extension", extension_manager.load_extension_paths()),
    ..
}
```

## Extension Manager

New module: `crates/vmux_desktop/src/extension.rs`

### Data Model (ECS)

Each installed extension is an ECS entity, persisted via moonshine-save like tabs/panes/spaces.

```rust
/// Persisted components (saved/loaded via moonshine-save)

#[derive(Component, Serialize, Deserialize)]
struct Extension {
    id: String,             // Chrome Web Store extension ID
    name: String,
    version: String,
    manifest_version: u8,   // 2 or 3
    installed_at: String,    // ISO 8601
}

#[derive(Component, Serialize, Deserialize)]
struct ExtensionEnabled;    // marker component — absence means disabled

/// Runtime-only components (re-parsed from manifest.json on startup)

#[derive(Component)]
struct ExtensionManifest {
    permissions: Vec<String>,
    content_scripts: Vec<ContentScriptEntry>,
    action: Option<ActionEntry>,   // MV3 action / MV2 browser_action
    background: Option<BackgroundEntry>,
    icons: HashMap<String, String>, // size -> path
}

struct ActionEntry {
    default_popup: Option<String>,
    default_icon: Option<HashMap<String, String>>,
    default_title: Option<String>,
}

#[derive(Component, Default)]
struct ExtensionBadge {
    text: String,
    color: [u8; 4],
}
```

Persistence strategy: only `Extension` and `ExtensionEnabled` are saved via moonshine-save. `ExtensionManifest` and `ExtensionBadge` are runtime-only, reconstructed on startup by scanning the extensions directory and parsing each `manifest.json`.

Querying for the extension toolbar:

```rust
fn extension_toolbar_system(
    extensions: Query<(
        Entity,
        &Extension,
        &ExtensionManifest,
        Option<&ExtensionBadge>,
    ), With<ExtensionEnabled>>,
) { /* build toolbar state for vmux_header */ }
```

### Directory Layout

```
~/Library/Application Support/vmux/extensions/
├── nkbihfbeogaeaoehlefnkodbefgpgknn/    # Bitwarden
│   ├── manifest.json
│   ├── background.js
│   ├── content_scripts/
│   ├── popup/
│   └── images/
└── dbepggeogbaibhgnhhndojpepiihcmeb/    # Vimium
    ├── manifest.json
    ├── content_scripts/
    └── icons/
```

### Operations

- `scan_extensions_dir()` -- Read extensions directory, parse each manifest.json, return list of installed extensions.
- `download_crx(extension_id: &str)` -- Download CRX from Google's update API:
  ```
  https://clients2.google.com/service/update2/crx
    ?response=redirect
    &prod=chromiumcrx
    &prodversion=145.0
    &x=id%3D{extension_id}%26uc
  ```
- `unpack_crx(crx_path: &Path)` -- CRX files are ZIP with a header. Strip the CRX header (magic bytes + version + lengths), then unzip to `extensions/<id>/`.
- `enable(id)` / `disable(id)` -- Add/remove `ExtensionEnabled` marker component on the extension entity.
- `remove(id)` -- Despawn extension entity and delete directory from disk.
- `load_extension_paths()` -- Return comma-separated paths of enabled extensions for `--load-extension`.

### CRX Format

CRX3 header (CEF 145 / Chrome 145):
```
Cr24 (4 bytes magic)
version (4 bytes LE u32, value 3)
header_size (4 bytes LE u32)
header (header_size bytes, protobuf)
ZIP data (rest of file)
```

Strip the header, extract ZIP contents.

### Restart Requirement

`--load-extension` is a process-level switch read at CEF initialization. Installing a new extension requires restarting the CEF process. The UI should:
1. Show a notification: "Extension installed. Restart to activate."
2. Provide a restart action (or auto-restart if no unsaved state).

Restart mechanism: vmux_desktop triggers `AppExit` event, then re-launches itself via `std::process::Command::new(std::env::current_exe())`. Alternatively, if CEF supports re-initialization within the same process, call `cef_shutdown()` then `cef_initialize()` with updated switches. The simpler AppExit + relaunch approach is preferred for v1.

Future improvement: investigate `CefRequestContext::LoadExtension()` for hot-loading (not available in current Rust bindings).

## Web Store Install Flow

### Trigger

`ExtensionCommand::AddExtension` opens a new tab navigating to `https://chromewebstore.google.com/category/extensions`.

### Content Script Injection

When a browser navigates to `chromewebstore.google.com`, vmux injects a content script via `CefFrame::ExecuteJavaScript`. This script:

1. Uses `MutationObserver` to detect "Add to Chrome" buttons as they appear (Web Store is a SPA).
2. Replaces button text with "Add to Vmux".
3. Intercepts click events on these buttons.
4. Extracts extension ID from the page URL (format: `/detail/<name>/<extension_id>`).
5. Sends install request to vmux via `CefProcessMessage` IPC.

```javascript
// Simplified webstore_content_script.js
const observer = new MutationObserver(() => {
  const buttons = document.querySelectorAll('button');
  buttons.forEach(btn => {
    if (btn.textContent?.includes('Add to Chrome')) {
      btn.textContent = 'Add to Vmux';
      btn.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const extId = window.location.pathname.split('/').pop();
        // Send to vmux via custom scheme or CefProcessMessage
        window.__vmux__.installExtension(extId);
      });
    }
  });
});
observer.observe(document.body, { childList: true, subtree: true });
```

The `window.__vmux__` object is registered via `CefExtensions` V8 extension, providing a bridge from JS to `CefProcessMessage` IPC. This is the same mechanism used by the chrome.tabs polyfill (`__vmux_ipc__`). Both are thin wrappers around CEF's `CefV8Context::GetBrowser().GetMainFrame().SendProcessMessage()` for sending, and `CefRenderProcessHandler::OnProcessMessageReceived()` for receiving responses.

### vmux Install Handler

On receiving the install message in `OnProcessMessageReceived`:

1. Download CRX using extension ID.
2. Unpack to extensions directory.
3. Spawn extension ECS entity with `Extension` + `ExtensionEnabled` components.
4. Show "restart to activate" notification in header.

## chrome.tabs Bridge

### V8 Polyfill

Registered via `CefExtensions` in the render process. Intercepts `chrome.tabs` methods and routes them through `CefProcessMessage` IPC to vmux's browser process.

```javascript
// Simplified chrome_tabs_polyfill.js
// Registered as a V8 extension, runs in extension service worker context

(function() {
  const pendingCallbacks = new Map();
  let callId = 0;

  // Override chrome.tabs methods
  const originalTabs = chrome.tabs;

  chrome.tabs.query = function(queryInfo) {
    return new Promise((resolve) => {
      const id = callId++;
      pendingCallbacks.set(id, resolve);
      // Send IPC to browser process
      __vmux_ipc__.send('tabs:query', { id, queryInfo });
    });
  };

  chrome.tabs.get = function(tabId) {
    return new Promise((resolve) => {
      const id = callId++;
      pendingCallbacks.set(id, resolve);
      __vmux_ipc__.send('tabs:get', { id, tabId });
    });
  };

  chrome.tabs.sendMessage = function(tabId, message, options) {
    return new Promise((resolve) => {
      const id = callId++;
      pendingCallbacks.set(id, resolve);
      __vmux_ipc__.send('tabs:sendMessage', { id, tabId, message, options });
    });
  };

  // Event emitters for onUpdated, onActivated
  const onUpdatedListeners = [];
  const onActivatedListeners = [];

  chrome.tabs.onUpdated = {
    addListener: (fn) => onUpdatedListeners.push(fn),
    removeListener: (fn) => {
      const i = onUpdatedListeners.indexOf(fn);
      if (i >= 0) onUpdatedListeners.splice(i, 1);
    }
  };

  chrome.tabs.onActivated = {
    addListener: (fn) => onActivatedListeners.push(fn),
    removeListener: (fn) => {
      const i = onActivatedListeners.indexOf(fn);
      if (i >= 0) onActivatedListeners.splice(i, 1);
    }
  };

  // Receive responses from browser process
  __vmux_ipc__.onMessage((type, data) => {
    if (type === 'tabs:response') {
      const cb = pendingCallbacks.get(data.id);
      if (cb) { cb(data.result); pendingCallbacks.delete(data.id); }
    } else if (type === 'tabs:onUpdated') {
      onUpdatedListeners.forEach(fn => fn(data.tabId, data.changeInfo, data.tab));
    } else if (type === 'tabs:onActivated') {
      onActivatedListeners.forEach(fn => fn(data.activeInfo));
    }
  });
})();
```

### Browser Process Handler (Rust)

In `OnProcessMessageReceived`, handle `vmux:tabs:*` messages:

```
"tabs:query" { active, currentWindow }
  → Query ECS:
    1. Find entity with Active + Pane components
    2. Find Active Tab child
    3. Get associated CefBrowser → browser.get_main_frame().get_url()
    4. Map CEF browser identifier → tab ID
  → Return: [{ id: browser_id, url, title, active: true }]

"tabs:get" { tabId }
  → Look up CefBrowser by identifier
  → Return: { id, url, title, active }

"tabs:sendMessage" { tabId, message }
  → Find CefBrowser by identifier
  → browser.get_main_frame().execute_javascript(
      `chrome.runtime.onMessage.dispatch(${JSON.stringify(message)})`
    )
  → Return: response from content script (via another IPC round-trip)
```

### Tab ID Mapping

CEF browsers have integer identifiers (`CefBrowser::get_identifier()`). Use these directly as tab IDs for the extension API. The mapping is:

```
vmux ECS entity (Tab)
  └── has BrowserView component
       └── references CefBrowser
            └── get_identifier() → i32 → used as chrome.tabs tab ID
```

### Events

Fire `tabs:onUpdated` when:
- `OnLoadEnd` callback fires on any browser (URL change)
- Browser title changes

Fire `tabs:onActivated` when:
- `Active` component moves between Tab entities in ECS

Both events are sent as `CefProcessMessage` from browser process to render process, where the V8 polyfill dispatches them to registered listeners.

## chrome.action Bridge

For badge text/color and popup management:

```
"action:setBadgeText" { extensionId, text }
  → Store in ExtensionManager state
  → Send update to vmux_header via ECS event

"action:setBadgeBackgroundColor" { extensionId, color }
  → Store in ExtensionManager state
  → Send update to vmux_header via ECS event

"action:setIcon" { extensionId, imageData | path }
  → Update icon in ExtensionManager state
  → Send update to vmux_header via ECS event
```

## Extension Toolbar in Header

### vmux_header Changes

Add an extension icon bar to the right side of the header. The header communicates with vmux_desktop via the existing message bridge (postMessage between Dioxus WASM and CEF host).

Data flow:
```
ExtensionManager (ECS resource)
  → ExtensionToolbarState { extensions: Vec<ExtensionIcon> }
  → serialized to JSON
  → sent to vmux_header via init script / message bridge

struct ExtensionIcon {
    id: String,
    name: String,
    icon_url: String,     // chrome-extension://<id>/icons/icon16.png
    badge_text: String,
    badge_color: [u8; 4],
    has_popup: bool,
    popup_url: String,    // chrome-extension://<id>/popup/index.html
}
```

### Icon Rendering

- Icons parsed from `manifest.json` `action.default_icon` (prefer 16px or 32px).
- Rendered as `<img>` elements in Dioxus.
- Badge text overlaid as a small colored label (bottom-right of icon).
- Tooltip from `action.default_title`.

### Popup Dropdown

On icon click:
1. vmux_header sends message to vmux_desktop: `{ type: "open_extension_popup", extensionId, popupUrl }`.
2. vmux_desktop creates a small Alloy OSR browser loading the popup URL (`chrome-extension://<id>/popup/index.html`).
3. Browser is positioned as a dropdown below the icon (absolute position in the Bevy scene, overlaying the main content).
4. Dismissed on click-outside or Escape.

Popup browser size: read from extension's popup HTML dimensions, or default to 400x600px.

## Commands

New `ExtensionCommand` enum in `command.rs`:

| Variant | menu id | label | accel | bind | Handler |
|---------|---------|-------|-------|------|---------|
| AddExtension | add_extension | Add Extension | | | Opens Web Store tab |
| RemoveExtension | remove_extension | Remove Extension | | | Shows extension picker, removes selected |
| ToggleExtension | toggle_extension | Toggle Extension | | | Enable/disable without removing |
| ManageExtensions | manage_extensions | Manage Extensions | | | Opens extension management UI |

## Extension API Support Matrix

| API | Support | Implementation |
|-----|---------|----------------|
| content_scripts | Native | Chrome bootstrap handles injection |
| chrome.storage.local | Native | Chrome bootstrap |
| chrome.storage.sync | Native | Chrome bootstrap |
| chrome.runtime.sendMessage | Native | Chrome bootstrap |
| chrome.runtime.onMessage | Native | Chrome bootstrap |
| chrome.tabs.query | V8 polyfill | IPC to ECS |
| chrome.tabs.get | V8 polyfill | IPC to ECS |
| chrome.tabs.sendMessage | V8 polyfill | IPC to CefBrowser |
| chrome.tabs.onUpdated | V8 polyfill | ECS event -> IPC |
| chrome.tabs.onActivated | V8 polyfill | ECS event -> IPC |
| chrome.action.setBadgeText | V8 polyfill | IPC to header state |
| chrome.action.setBadgeBackgroundColor | V8 polyfill | IPC to header state |
| chrome.action.setIcon | V8 polyfill | IPC to header state |
| chrome.scripting | Native | Chrome bootstrap |
| chrome.webNavigation | Native | Chrome bootstrap |
| chrome.notifications | Not supported | Future consideration |
| chrome.contextMenus | Not supported | vmux has no right-click menu system |
| chrome.bookmarks | Not supported | vmux has no bookmark system |
| chrome.history | Not supported | Future consideration |

## Bitwarden Compatibility

Bitwarden requires:
1. `chrome.tabs.query({active:true, currentWindow:true})` -- get active tab URL for credential matching. **Covered by V8 bridge.**
2. `chrome.tabs.sendMessage(tabId, msg)` -- send autofill command to content script. **Covered by V8 bridge.**
3. `chrome.tabs.onUpdated` -- detect URL changes for auto-fill suggestions. **Covered by V8 bridge.**
4. `chrome.storage` -- vault cache and settings. **Native with Chrome bootstrap.**
5. `chrome.runtime` -- messaging between service worker and content scripts. **Native with Chrome bootstrap.**
6. `content_scripts` -- autofill form detection and injection. **Native with Chrome bootstrap.**
7. `default_popup` -- vault UI (popup/index.html). **Rendered as dropdown CEF browser.**
8. `webRequestBlocking` (MV2) -- Bitwarden's current manifest is MV2 with persistent background page. **May work with Chrome bootstrap; requires testing.**

Risk: Bitwarden is still MV2 (`"manifest_version": 2`). MV2 uses persistent background pages, not service workers. CEF 145 with Chrome bootstrap should support MV2 extensions, but this needs verification.

## Vimium Compatibility

Vimium core features (content-script-based):
- Keyboard navigation (hjkl, gg, G, d, u) -- **works**
- Link hints (f, F) -- **works**
- Find mode (/) -- **works**
- Insert mode (i) -- **works**
- Visual mode (v) -- **works**
- Vomnibar (o, O, b, B) -- **partially works** (no bookmark/history search via chrome.bookmarks/chrome.history)
- Tab commands (t, T, gt, gT, x) -- **works via chrome.tabs bridge**

## Files Changed

| File | Change |
|------|--------|
| `patches/bevy_cef_core-*/message_loop.rs` | Verify chrome_runtime status; add --enable-chrome-runtime if needed |
| `patches/bevy_cef_core-*/browsers.rs` | Set `runtime_style: RuntimeStyle::ALLOY` on WindowInfo; add popup browser creation method |
| `patches/bevy_cef_core-*/client.rs` | Handle `vmux:tabs:*` and `vmux:action:*` IPC in OnProcessMessageReceived |
| `patches/bevy_cef_core-*/lib.rs` | Expose new IPC types |
| `crates/vmux_desktop/src/extension.rs` (new) | ExtensionManager resource, CRX download/unpack, registry, manifest parsing |
| `crates/vmux_desktop/src/extension_bridge.rs` (new) | chrome.tabs ECS query handler, event dispatching |
| `crates/vmux_desktop/src/command.rs` | Add ExtensionCommand enum variants |
| `crates/vmux_desktop/src/browser.rs` | Pass --load-extension from registry; inject Web Store content script on navigation |
| `crates/vmux_desktop/src/lib.rs` | Register ExtensionPlugin |
| `crates/vmux_header/src/` | Extension icon bar component, badge rendering, popup trigger messages |
| `assets/js/webstore_content_script.js` (new) | Web Store "Add to Vmux" button replacement |
| `assets/js/chrome_tabs_polyfill.js` (new) | V8 polyfill for chrome.tabs -> vmux IPC |
| `assets/js/chrome_action_polyfill.js` (new) | V8 polyfill for chrome.action -> vmux IPC |

## Open Questions

1. **Chrome bootstrap default in CEF 145**: Alloy bootstrap was deleted in M128. CEF 145 may already use Chrome bootstrap by default. If so, no `--enable-chrome-runtime` switch is needed. Verify by checking if `--load-extension` works without any settings change.

2. **V8 polyfill injection into extension context**: CefExtensions registers JS in render process V8 contexts. Verify that this includes extension service worker / background page contexts, not just web page contexts. If not, an alternative injection mechanism is needed (e.g., modifying the extension's JS files on disk after unpacking).

3. **MV2 background page support**: Bitwarden is MV2 with a persistent background page. Verify CEF 145 Chrome bootstrap supports MV2 extensions with persistent backgrounds.

4. **Extension popup sizing**: Chrome extensions specify popup dimensions via CSS in popup.html. The CEF browser hosting the popup needs to resize to fit content. May need to inject a ResizeObserver and communicate dimensions back to vmux.

5. **chrome-extension:// protocol in Alloy OSR**: Verify that Alloy-style browsers can load `chrome-extension://` URLs for popup rendering. If not, the popup HTML may need to be served from a local file:// or custom scheme.
