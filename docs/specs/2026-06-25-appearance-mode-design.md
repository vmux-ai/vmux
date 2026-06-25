# Appearance Mode (Light / Dark / Device)

Date: 2026-06-25

## Goal

Add a Chrome-style **Mode** setting — Light / Dark / Device — under a new **Appearance** section in vmux settings. The chosen mode drives the CEF color scheme for **all web content** (every tab + the layout/header/command-bar chrome share one `RequestContext`), so pages observe the matching `prefers-color-scheme`. Device follows the OS.

## Scope

- **In scope:** web content color scheme via CEF `set_chrome_color_scheme`. Default `Device`. Live update on change. Dropdown in settings.
- **Out of scope:** re-theming vmux's own UI (header, tabs, settings pages). Those pages are hardcoded dark and do not read `prefers-color-scheme`; in Light mode they stay dark while external sites flip. (Confirmed scope decision.)
- **Out of scope:** Chrome's "Theme" and "Customise your toolbar" rows from the reference screenshot.

## Architecture

Four pieces across three crates plus one vendored patch.

```
AppSettings.appearance.mode  ──(vmux bridge system on change)──▶  CefColorScheme resource (patch)
        ▲                                                                    │
        │ settings page dropdown                                            │ (patch system on change)
        │                                                                    ▼
   user picks mode ───▶ SettingsCommandEvent ───▶ apply_settings_update     Browsers.set_color_scheme()
                                                                             ├─ stores variant for future contexts
                                                                             └─ set_chrome_color_scheme() on shared
                                                                                & live (open) contexts
```

### 1. Settings model — `crates/vmux_setting/src/plugin/runtime.rs`

- New enum:
  ```rust
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
  #[serde(rename_all = "lowercase")]
  pub enum ColorScheme {
      Light,
      Dark,
      #[default]
      Device,
  }
  ```
- New section struct:
  ```rust
  #[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
  pub struct AppearanceSettings {
      #[serde(default)]
      pub mode: ColorScheme,
  }
  ```
- Add to `AppSettings` (runtime.rs:17): `#[serde(default)] pub appearance: AppearanceSettings`.
- Add to `PartialAppSettings` (runtime.rs:417): `#[serde(default)] appearance: Option<AppearanceSettings>`.
- Add merge arm in `merge_over_embedded` (runtime.rs:439).
- Add `differs("appearance")` arm in `sparse_settings_ron` (runtime.rs:632) so it persists only when changed.
- **Do not** add `appearance` to embedded `settings.ron` — default `Device` comes from the `Default` impl, matching the no-auto-seed convention (cf. `editor`/`recording`/`spaces`, which are also absent from the file).
- Re-export `ColorScheme` / `AppearanceSettings` in `crates/vmux_setting/src/lib.rs`.

### 2. Settings dropdown — schema-driven page

- `crates/vmux_setting/src/schema.rs`:
  - Add `WidgetKind::Select` (keep enum `Copy`).
  - Add `pub options: Vec<SelectOption>` to `FieldSpec` (`#[serde(default)]`), where `SelectOption { value: String, label: String }`. Options live on the field, not the widget, so `WidgetKind` stays `Copy`.
- `crates/vmux_setting/src/plugin/view.rs` — `build_settings_schema()` (view.rs:204):
  - Register an `"appearance"` `SectionSpec` (title "Appearance", `synthetic_keys: ["mode"]`, `root_path: "appearance"`), mirroring the existing `browser` section (view.rs:237).
  - Register the `appearance.mode` `FieldSpec`: `label: "Mode"`, `widget: Select`, `options: [Device, Light, Dark]`.
- `crates/vmux_setting/src/page.rs`:
  - In `WidgetView` (page.rs:~402), handle `WidgetKind::Select`: render the dropdown (reuse the `Select` primitive family from `crates/vmux_ui/src/components/select.rs` / `dioxus_primitives::select`). On change, `emit_update(path, Value::String(value))` (page.rs:96) — same write path as other fields.
  - Pass `spec.options` into `WidgetView` (extend its props) so the Select knows its choices.
- Section ordering: place "Appearance" near the top of the settings list (it is the most-touched setting), following how sections are ordered in `build_settings_schema`.

### 3. CEF bridge — patch `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`

- The vendored crate owns CEF objects (`RequestContext` is `!Send`) and already holds the single `shared_disk_context` reused by every browser (browsers.rs:108). This is the one place a color scheme must be applied.
- Add patch-local, vmux-agnostic mode type + resource (no dependency on `vmux_setting`):
  ```rust
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
  pub enum CefColorMode { Light, Dark, #[default] System }

  #[derive(Resource, Clone, Copy, Debug, Default)]
  pub struct CefColorScheme(pub CefColorMode);
  ```
  Map `CefColorMode` → `cef::ColorVariant` (`System`=0, `Light`=1, `Dark`=2) at the call site.
- `Browsers` gets a `color_scheme: CefColorMode` field (default `System`).
- Apply at context creation:
  - In `ensure_shared_disk_context` (browsers.rs:1451) and `ephemeral_request_context` (browsers.rs:1480), after the `context` is built and handlers registered, call `context.set_chrome_color_scheme(self.color_scheme.into(), 0)`. (`ephemeral_request_context` is a static fn — give it a `mode` param, passed from `self.color_scheme` by its caller in `create_browser`.)
- Live update:
  - `pub fn set_color_scheme(&mut self, mode: CefColorMode)`: store on `self`; if `self.shared_disk_context` is `Some`, call `set_chrome_color_scheme` on it immediately so already-open browsers reflow.
  - Add a patch system (registered in the `bevy_cef_core` plugin) that runs when `Res<CefColorScheme>` changed and calls `browsers.set_color_scheme(...)`. `Browsers` is accessed as the existing `NonSendMut` so the CEF call stays on the main thread.
  - `init_resource::<CefColorScheme>()` in the plugin.
- **Runtime-reflow caveat:** CEF is expected to apply `set_chrome_color_scheme` live to open browsers in the context. If a given page caches `prefers-color-scheme` and does not reflow, it picks up the new scheme on next navigation/reload. To verify at runtime; if live reflow is unreliable, the create-time path still guarantees correctness for new/reloaded tabs.

### 4. vmux → patch bridge — `crates/vmux_browser` (or wherever `AppSettings` is read for CEF, cf. `on_webview_ready_send_theme`, `vmux_browser/src/lib.rs:205`)

- One system: on `AppSettings` changed (`settings.is_changed()`), map `appearance.mode` → `CefColorMode` and write `ResMut<CefColorScheme>` (only when the value actually changes, to avoid needless reflows). Also runs once at startup so the initial mode is applied before/at first context creation.
- Mapping: `Light→Light`, `Dark→Dark`, `Device→System`.

## Data flow

1. User opens Settings → Appearance → Mode, picks a value.
2. Page `emit_update("appearance.mode", "light")` → `SettingsCommandEvent` → `apply_settings_update` mutates `AppSettings` and persists sparsely.
3. vmux bridge system observes the change → writes `CefColorScheme`.
4. Patch system observes `CefColorScheme` change → `Browsers::set_color_scheme` → `set_chrome_color_scheme` on the shared context.
5. New contexts created afterward inherit `self.color_scheme` at creation.
6. On next launch, the saved mode loads from `settings.ron`, the startup bridge run applies it before first browser paints.

## Testing

Native unit tests (run with `cargo test -p vmux_setting`; patch tests with the patched-crate package):

- `ColorScheme` ⇄ RON round-trips to `"light"`/`"dark"`/`"device"`; absent `appearance` defaults to `Device`.
- `sparse_settings_ron`: omits `appearance` when `Device`; emits it when changed.
- `CefColorMode → ColorVariant` mapping (System/Light/Dark = 0/1/2).
- Schema: `build_settings_schema` exposes the `appearance` section + `mode` Select field with three options.

Manual runtime test (single pass at the end, per finish-then-test workflow):

- Open a light/dark-aware site (e.g. a page using `prefers-color-scheme`); switch Mode Light↔Dark↔Device and confirm the page flips live (or on reload), and Device follows OS appearance.
- Confirm the choice persists across relaunch.
- Confirm vmux's own chrome is unchanged (expected: stays dark).

## Risks / Open items

- **Live reflow** of already-open browsers depends on CEF behavior — verified at runtime; create-time path is the guaranteed fallback.
- **Patch edit:** touches a vendored crate. `cargo fmt` may reformat other files under `patches/` — restore those (`git checkout -- patches/` for unrelated noise) and commit only the intended changes.
- **`ephemeral_request_context` is a static fn** — must thread the mode through its caller; minor signature change.
