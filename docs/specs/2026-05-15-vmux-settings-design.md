# vmux_settings — design

## Goal

Ship a settings webview app at `vmux://settings/` that displays every field in `AppSettings` as a form, lets the user edit values with auto-save, and exposes the same edit surface to bots via two MCP tools (`get_settings`, `update_settings`).

## Architecture

```
┌──────────────────────┐  SettingsListEvent (JSON snapshot)  ┌──────────────────┐
│  vmux_desktop        │ ──────────────────────────────────▶ │  vmux_settings   │
│  AppSettings (Bevy)  │                                     │  (Dioxus form)   │
│                      │ ◀────── SettingsCommandEvent ────── │                  │
└──────────────────────┘    { path: "layout.pane.gap",       └──────────────────┘
         ▲                    value: "12.0" }
         │
         │ AgentCommand::UpdateSettings { path, value_json }
         │ AgentQuery::GetSettings
         │
┌────────┴───────┐
│  vmux_service  │ ◀── MCP `update_settings` / `get_settings` (vmux_mcp)
└────────────────┘
```

Both edit paths (form + MCP) funnel into one shared function `apply_settings_update(settings, path, value)` in `vmux_desktop::settings`. That function:

1. Serializes the current `AppSettings` to `serde_json::Value`.
2. Walks `path` (dot-separated, with `[i]` for array indexing) and replaces the leaf with `value`.
3. Deserializes the mutated value back into `AppSettings`. On failure: return error, no resource change.
4. Replaces `*settings` in place (so Bevy marks the resource `is_changed`).
5. Returns the serialized RON bytes to write.

A separate Bevy system `persist_settings_to_disk` reads a `SettingsWriteRequest` event (queued by the observer/dispatcher that called `apply_settings_update`), writes `~/Library/Application Support/Vmux/settings.ron` atomically (`tempfile::NamedTempFile::persist`), hashes the bytes, and stores the hash in a `LastSelfWriteHash` resource. The existing `notify`-based file watcher's reload handler checks the new file's content hash against `LastSelfWriteHash` and skips reload if equal — preventing a feedback loop on our own writes.

## Components

### New crate `crates/vmux_settings/`

Mirrors `crates/vmux_space/` exactly:

- `Cargo.toml` — `vmux_settings_app` bin (web feature), library
- `Dioxus.toml`
- `tailwind.config.js`
- `build.rs` — `WebviewAppBuilder::new(manifest_dir, "vmux_settings", "vmux_settings_app")` with `tailwind_postprocess_after_dx(&["index-dxs", "settings-dxs"])`
- `assets/`, `dist/` (gitignored)
- `src/lib.rs` — declares `event` module
- `src/event.rs`:
  - `pub const SETTINGS_WEBVIEW_URL: &str = "vmux://settings/";`
  - `pub const SETTINGS_LIST_EVENT: &str = "settings_list";`
  - `SettingsListEvent { json: String }` — snapshot pushed to view
  - `SettingsCommandEvent { path: String, value: String }` — `value` is JSON-encoded so the rkyv schema stays simple (no nested `serde_json::Value`)
  - rkyv roundtrip tests for both
- `src/main.rs` — `dioxus::launch(app::App)` (wasm32 only)
- `src/app.rs` — Dioxus `App`:
  - `use_theme()` from `vmux_ui::hooks`
  - `use_signal::<Option<AppSettingsJson>>` — parsed snapshot
  - `use_bin_event_listener::<SettingsListEvent>(SETTINGS_LIST_EVENT, ..)` updates the signal
  - Sectioned form (one collapsible card per section):
    - **General** — `auto_update` (toggle), `startup_url` (text)
    - **Window** — `layout.window.padding` and per-side overrides (number)
    - **Pane** — `layout.pane.gap`, `layout.pane.radius` (number)
    - **Side sheet** — `layout.side_sheet.width` (number)
    - **Focus ring** — `layout.focus_ring.{width, color, glow.{spread,intensity}, gradient.{enabled,speed,cycles,accent}}`
    - **Shortcuts** — `shortcuts.leader` (key combo input), `shortcuts.chord_timeout_ms` (number), `shortcuts.bindings` (read-only list for v1)
    - **Terminal** — `terminal.confirm_close` (toggle), `terminal.default_theme` (text), `terminal.themes[i]` (each theme as a sub-card with font_family/font_size/line_height/padding/cursor_style/cursor_blink/shell)
  - Each input wraps a `use_debounced_emit(path, 300ms)` hook that calls `try_cef_bin_emit_rkyv(SettingsCommandEvent { path, value: json_value.to_string() })`
  - Suppress reload-on-edit: keep `last_emitted_path` in a signal; when a new snapshot arrives that matches the path the user is currently editing, ignore that field's reload until 500 ms after last keystroke

### `vmux_desktop/src/settings_view.rs` (new)

Mirrors `vmux_desktop/src/spaces.rs`:

- `SettingsView` component (web entity)
- `SettingsViewPlugin`:
  - `register_settings_webview_app(registry)` — `WebviewAppConfig::with_custom_host("settings")`, manifest dir `../vmux_settings`
  - `BinJsEmitEventPlugin::<SettingsCommandEvent>::default()`
  - `add_observer(on_settings_command)` — handles `BinReceive<SettingsCommandEvent>`
  - `add_systems(Update, broadcast_settings_to_views)` — pushes snapshot when `AppSettings` changed or a view first becomes ready
- `on_settings_command(trigger: On<BinReceive<SettingsCommandEvent>>, world: ...)`:
  - Parse `value` JSON → call `apply_settings_update(world, &path, value)`
- `broadcast_settings_to_views`:
  - Query `(Entity, With<SettingsView>, With<UiReady>)`
  - When `AppSettings.is_changed()` OR a new ready view appears, emit `SettingsListEvent { json }` to each entity via `JsEmitEventPlugin`

### `vmux_desktop/src/settings.rs` (extend existing)

New helpers:

```rust
pub fn serialize_settings(settings: &AppSettings) -> String {
    serde_json::to_string(settings).expect("AppSettings serializes")
}

pub fn apply_settings_update(
    settings: &mut AppSettings,
    path: &str,
    value: serde_json::Value,
) -> Result<String, String> { /* returns serialized RON to write */ }

fn set_at_path(root: &mut serde_json::Value, path: &str, value: serde_json::Value)
    -> Result<(), String>;
```

`set_at_path` splits on `.`, walks objects and arrays (`themes[0].font_size` → `["themes", 0, "font_size"]`), errors on missing intermediate keys.

A new system `persist_settings_to_disk` consumes a `SettingsWriteRequest { ron_bytes: String }` event (queued by `apply_settings_update` callers in `on_settings_command` / agent dispatcher), writes RON atomically, hashes the bytes, stores hash in `LastSelfWriteHash`. `reload_settings_on_change` checks the new file's hash against `LastSelfWriteHash` and skips reload if equal.

All settings structs in `vmux_layout::settings` (`LayoutSettings`, `WindowSettings`, `PaneSettings`, `SideSheetSettings`, `FocusRingSettings`, `FocusRingColor`, `FocusRingGlow`, `FocusRingGradient`) and `vmux_desktop::settings` (`AppSettings`, `BrowserSettings`, `ShortcutSettings`, `KeyComboDef`, `ShortcutEntry`, `ShortcutDef`, `TerminalSettings`, `TerminalTheme`) get `Serialize` derives added alongside the existing `Deserialize`.

### `vmux_service/src/protocol.rs`

New variants:

```rust
pub enum AgentCommand {
    // ...existing...
    UpdateSettings { path: String, value_json: String },
}

pub enum AgentQuery {
    // ...existing...
    GetSettings,
}

pub enum AgentQueryResult {
    // ...existing...
    Settings(String),
}
```

Validation in `validate_agent_command`: `UpdateSettings { path, .. }` rejects empty path. `value_json` is parsed downstream — service does not validate JSON.

### `vmux_mcp/src/tools.rs`

```rust
#[derive(Debug, McpTool)]
pub enum McpParamTool {
    // ...existing...
    #[mcp(description = "Update a single vmux setting by dot-path. \
        Example: { path: 'layout.pane.gap', value: 12 }. \
        Use get_settings to discover paths and current values.")]
    UpdateSettings {
        path: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, McpTool)]
pub enum McpQueryTool {
    // ...existing...
    #[mcp(description = "Return the full vmux settings as JSON.")]
    GetSettings,
}
```

`UpdateSettings::to_agent_command` rejects empty path, then maps to `AgentCommand::UpdateSettings { path, value_json: value.to_string() }`.

`GetSettings` maps to `AgentQuery::GetSettings`.

### `vmux_desktop/src/agent.rs`

In the agent dispatch system that handles `AgentCommand` and `AgentQuery`:

- `AgentCommand::UpdateSettings { path, value_json }` → `serde_json::from_str(&value_json)` → `apply_settings_update`. Map error to `AgentCommandResult::Error(msg)`.
- `AgentQuery::GetSettings` → `serialize_settings(&app_settings)` → `AgentQueryResult::Settings(json)`.

### `vmux_desktop/src/lib.rs`

Add `mod settings_view;` and register `SettingsViewPlugin` after `SpacesPlugin`.

### `vmux_desktop/src/command_bar.rs`

Add `vmux://settings/` to the URL completion list (next to `vmux://spaces/`) so users can open it from the command bar.

## Data flow

### Initial load

1. User opens `vmux://settings/` (command bar, MCP `browser_navigate`, or new tab).
2. `SettingsView` entity spawned with `WebviewSource::new("vmux://settings/")`.
3. WASM bundle loads, `App` mounts, `try_cef_bin_emit_rkyv(UiReady {})`.
4. Desktop `mark_webview_ui_ready_on_js_emit` adds `UiReady` to the entity.
5. `broadcast_settings_to_views` sees the new `UiReady` entity, emits `SettingsListEvent { json }`.
6. WASM listener parses JSON, populates form.

### Edit from form

1. User changes pane gap field.
2. Debounced 300 ms after last keystroke, view emits `SettingsCommandEvent { path: "layout.pane.gap", value: "12.0" }`.
3. Desktop `on_settings_command` parses value JSON, calls `apply_settings_update`.
4. `apply_settings_update` mutates resource, writes RON to disk, updates `LastSelfWriteHash`.
5. Resource is_changed → broadcast pushes new snapshot.
6. View receives snapshot; suppresses reload of any field whose path matches `last_emitted_path` (set during step 2 and cleared 500 ms after last keystroke).

### Edit from MCP

1. Bot calls MCP `update_settings { path: "auto_update", value: false }`.
2. `vmux_mcp::tools::dispatch_from_tool_call` → `AgentCommand::UpdateSettings { path, value_json: "false" }`.
3. Sent over IPC to `vmux_service`, forwarded to desktop.
4. Desktop dispatches → `apply_settings_update` (same code path as form edit).
5. Resource changes → broadcast pushes new snapshot to any open settings tab in real-time.

### Read from MCP

1. Bot calls MCP `get_settings`.
2. → `AgentQuery::GetSettings` → `AgentQueryResult::Settings(json)` → returned to bot.

## Edge cases

- **Unknown path** (`"layout.nope"`): `set_at_path` errors, no disk write, no resource change. MCP returns tool error; form silently ignores (validation should catch client-side first).
- **Type mismatch** (`{ path: "auto_update", value: "yes" }`): `from_value` fails on the deserialize step; same handling as unknown path.
- **Disk write failure**: log warn, leave resource changed. Next successful edit retries.
- **Concurrent edits** (two open settings tabs, or tab + MCP simultaneously): Bevy systems are single-threaded per resource; updates serialize naturally. Last write wins. The losing tab gets the winner's snapshot via broadcast.
- **External edit to settings.ron** (user edits in their editor): file watcher fires, `LastSelfWriteHash` doesn't match, normal reload runs, broadcast pushes new state to settings tab. Form re-syncs.

## Testing

- `vmux_settings::event` — rkyv roundtrip for `SettingsListEvent` and `SettingsCommandEvent`.
- `vmux_desktop::settings::set_at_path` — unit tests:
  - nested object: `"layout.pane.gap"` → 12.0
  - array indexing: `"terminal.themes[0].font_size"` → 16.0
  - unknown key: `"layout.nope"` → error
  - type mismatch handled at `from_value` boundary, not in `set_at_path` itself
- `vmux_desktop::settings::apply_settings_update` — load defaults, apply updates, assert resource + serialized RON match expected.
- `vmux_desktop::settings::reload_skip_on_self_write` — write via apply_settings_update, observe no reload triggered.
- `vmux_mcp::tools` — `update_settings` and `get_settings` appear in `tool_definitions`; dispatch routes to correct `AgentCommand` / `AgentQuery`; empty path rejected.
- `vmux_service::protocol` — rkyv roundtrip for new `AgentCommand`/`AgentQuery`/`AgentQueryResult` variants.
- Manual: open `vmux://settings/`, edit pane gap, observe layout updates immediately. Run MCP `update_settings { path: "layout.pane.gap", value: 16 }`, observe form reflects new value.

## Out of scope (v1)

- Reset-to-defaults button — delete `settings.ron` manually for now.
- Custom terminal color scheme editor (per-ANSI-slot color pickers) — themes are shown as a read-only list; only top-level theme fields editable.
- Shortcut chord recorder UI — bindings shown read-only; edit by raw RON for now.
- Theme switcher in settings UI — vmux already follows system theme via `use_theme`.
- Validation feedback in the form — bad input silently fails to emit; v2 should surface errors.

## File summary

New files:
- `crates/vmux_settings/Cargo.toml`
- `crates/vmux_settings/Dioxus.toml`
- `crates/vmux_settings/tailwind.config.js`
- `crates/vmux_settings/build.rs`
- `crates/vmux_settings/src/lib.rs`
- `crates/vmux_settings/src/event.rs`
- `crates/vmux_settings/src/main.rs`
- `crates/vmux_settings/src/app.rs`
- `crates/vmux_desktop/src/settings_view.rs`
- `docs/specs/2026-05-15-vmux-settings-design.md` (this file)

Modified files:
- `Cargo.toml` (workspace members)
- `crates/vmux_layout/src/settings.rs` (add `Serialize`)
- `crates/vmux_desktop/src/settings.rs` (add `Serialize`, `apply_settings_update`, `set_at_path`, `LastSelfWriteHash`)
- `crates/vmux_desktop/src/lib.rs` (register `SettingsViewPlugin`)
- `crates/vmux_desktop/src/command_bar.rs` (URL completion)
- `crates/vmux_desktop/src/agent.rs` (dispatch new variants)
- `crates/vmux_service/src/protocol.rs` (new variants)
- `crates/vmux_mcp/src/tools.rs` (new tools)
