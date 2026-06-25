# Appearance Mode (Light / Dark / Device) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. NOTE: do **not** subagent-drive this plan — CEF builds are huge and long-lived agents drop sockets. Execute inline in one session with a warm target dir.

**Goal:** Add a Chrome-style appearance **Mode** (Light / Dark / Device) to vmux settings that drives the CEF color scheme (`prefers-color-scheme`) for all web content.

**Architecture:** A new `appearance.mode` enum in `AppSettings` is rendered as a dropdown via a new schema `Select` widget. On change, a vmux bridge system maps the mode into a patch-defined `CefColorScheme` resource; a patch system applies it to the single shared `RequestContext` via `set_chrome_color_scheme`, covering every tab + the layout/header/command-bar chrome. New contexts inherit the mode at creation.

**Tech Stack:** Rust, Bevy ECS, serde/RON, Dioxus (WASM page), dioxus-primitives `Select`, vendored `bevy_cef` / `bevy_cef_core` patches, `cef` 145.6.1.

**Scope:** web content only. vmux's own pages stay dark (they don't read `prefers-color-scheme`). Default `Device`.

---

### Task 1: Settings model — `ColorScheme` + `appearance` section

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs` (`AppSettings` :17, `PartialAppSettings` :417, `merge_over_embedded` :439, `sparse_settings_ron` :632)
- Modify: `crates/vmux_setting/src/lib.rs:27` (re-exports)
- Test: inline `#[cfg(test)]` in `runtime.rs`

- [ ] **Step 1: Write failing tests** (append to the existing test module in `runtime.rs`, or add one)

```rust
#[cfg(test)]
mod appearance_tests {
    use super::*;

    #[test]
    fn color_scheme_defaults_to_device() {
        assert_eq!(ColorScheme::default(), ColorScheme::Device);
    }

    #[test]
    fn appearance_absent_falls_back_to_device() {
        let s = parse_settings("()").expect("parse empty");
        assert_eq!(s.appearance.mode, ColorScheme::Device);
    }

    #[test]
    fn appearance_round_trips_through_ron() {
        let s = parse_settings("(appearance: (mode: light))").expect("parse light");
        assert_eq!(s.appearance.mode, ColorScheme::Light);
        let s = parse_settings("(appearance: (mode: dark))").expect("parse dark");
        assert_eq!(s.appearance.mode, ColorScheme::Dark);
    }

    #[test]
    fn sparse_omits_default_appearance_and_emits_changed() {
        let mut s = load_embedded_settings();
        assert!(!sparse_settings_ron(&s).unwrap().contains("appearance"));
        s.appearance.mode = ColorScheme::Dark;
        let out = sparse_settings_ron(&s).unwrap();
        assert!(out.contains("appearance"));
        assert!(out.contains("dark"));
        // round-trips back
        assert_eq!(parse_settings(&out).unwrap().appearance.mode, ColorScheme::Dark);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_setting appearance_tests 2>&1 | tail -20`
Expected: FAIL — `ColorScheme` / `appearance` not found.

- [ ] **Step 3: Add the model** (in `runtime.rs`, near the other section structs)

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    Light,
    Dark,
    #[default]
    Device,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct AppearanceSettings {
    #[serde(default)]
    pub mode: ColorScheme,
}
```

Add to `AppSettings` (after `editor`, runtime.rs:36):
```rust
    #[serde(default)]
    pub appearance: AppearanceSettings,
```

Add to `PartialAppSettings` (after `editor`, runtime.rs:436):
```rust
    #[serde(default)]
    appearance: Option<AppearanceSettings>,
```

Add to `merge_over_embedded` (after the `editor` arm, runtime.rs:467):
```rust
    if let Some(appearance) = partial.appearance {
        settings.appearance = appearance;
    }
```

Add to `sparse_settings_ron` (after the `recording` arm, before the `parts.is_empty()` check, runtime.rs:669):
```rust
    if differs("appearance") {
        parts.push(format!(
            "    appearance: {},",
            section_ron(&settings.appearance)?
        ));
    }
```

- [ ] **Step 4: Re-export** in `crates/vmux_setting/src/lib.rs` — add `AppearanceSettings, ColorScheme,` to the `pub use plugin::runtime::{...}` list (runtime block, lib.rs:27).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vmux_setting appearance_tests 2>&1 | tail -20`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_setting/src/plugin/runtime.rs crates/vmux_setting/src/lib.rs
git commit -m "feat(settings): add appearance.mode (Light/Dark/Device) model"
```

---

### Task 2: Schema `Select` widget

**Files:**
- Modify: `crates/vmux_setting/src/schema.rs`
- Test: inline `#[cfg(test)]` in `schema.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod select_widget_tests {
    use super::*;

    #[test]
    fn select_field_with_options_round_trips_json() {
        let spec = FieldSpec {
            label: Some("Mode".into()),
            widget: Some(WidgetKind::Select),
            options: vec![
                SelectOption { value: "device".into(), label: "Device".into() },
                SelectOption { value: "light".into(), label: "Light".into() },
            ],
            ..Default::default()
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.widget, Some(WidgetKind::Select));
        assert_eq!(back.options.len(), 2);
        assert_eq!(back.options[0].value, "device");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_setting select_widget_tests 2>&1 | tail -20`
Expected: FAIL — `WidgetKind::Select` / `SelectOption` / `options` not found.

- [ ] **Step 3: Implement** — in `schema.rs`:

Add the option type:
```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}
```

Add the `Select` variant (keeps the enum `Copy` — it's a unit variant):
```rust
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WidgetKind {
    LeaderKbd,
    BindingsList,
    Select,
}
```

Add `options` to `FieldSpec` (after `step`, schema.rs:46):
```rust
    #[serde(default)]
    pub options: Vec<SelectOption>,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_setting select_widget_tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_setting/src/schema.rs
git commit -m "feat(settings): add Select widget kind + field options to schema"
```

---

### Task 3: Register Appearance section + Mode field

**Files:**
- Modify: `crates/vmux_setting/src/plugin/view.rs` (`build_settings_schema` :204)
- Test: inline `#[cfg(test)]` in `view.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod appearance_schema_tests {
    use super::*;

    #[test]
    fn schema_exposes_appearance_mode_select() {
        let schema = build_settings_schema();
        assert!(schema.sections.iter().any(|s| s.id == "appearance"));
        let mode = schema.field("appearance.mode").expect("mode field");
        assert_eq!(mode.widget, Some(crate::schema::WidgetKind::Select));
        assert_eq!(mode.options.len(), 3);
        let vals: Vec<_> = mode.options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(vals, vec!["device", "light", "dark"]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_setting appearance_schema_tests 2>&1 | tail -20`
Expected: FAIL — no `appearance` section.

- [ ] **Step 3: Implement** — in `build_settings_schema`:

Add to `sections` (first entry, so Appearance sorts to the top, before `general` at view.rs:207):
```rust
            SectionSpec {
                id: "appearance".to_string(),
                title: "Appearance".to_string(),
                description: None,
                synthetic_keys: vec!["mode".to_string()],
                root_path: "appearance".to_string(),
            },
```

Add to `fields` (alongside the others):
```rust
            field(
                "appearance.mode",
                FieldSpec {
                    label: Some("Mode".into()),
                    hint: Some("Color scheme for web pages. Device follows your system.".into()),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        crate::schema::SelectOption { value: "device".into(), label: "Device".into() },
                        crate::schema::SelectOption { value: "light".into(), label: "Light".into() },
                        crate::schema::SelectOption { value: "dark".into(), label: "Dark".into() },
                    ],
                    ..Default::default()
                },
            ),
```

Ensure `WidgetKind` is imported in `view.rs` (it already imports `schema::{... WidgetKind}` for the existing widgets — confirm/add `SelectOption` or use the `crate::schema::` path as written above).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_setting appearance_schema_tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_setting/src/plugin/view.rs
git commit -m "feat(settings): register Appearance section with Mode dropdown"
```

---

### Task 4: Settings page — render the Select (WASM)

**Files:**
- Modify: `crates/vmux_setting/src/page.rs` (`FieldView` :236, `WidgetView` :402)

No native unit test (WASM/Dioxus UI). Verified by `cargo check --target wasm32` and the end-of-plan runtime test.

- [ ] **Step 1: Add the select import** at the top of `page.rs` (with the other `vmux_ui::components` imports):

```rust
use vmux_ui::components::select::{
    Select, SelectItemIndicator, SelectList, SelectOption, SelectTrigger, SelectValue,
};
```

(Note the name clash: `crate::schema::SelectOption` is the data type; the import above is the UI component. In the code below, the data type is referenced as `crate::schema::SelectOption`.)

- [ ] **Step 2: Thread options into `WidgetView`** — change its signature (page.rs:402) to add `options`:

```rust
#[component]
fn WidgetView(
    widget: WidgetKind,
    path: String,
    value: Value,
    label: String,
    hint: Option<String>,
    options: Vec<crate::schema::SelectOption>,
) -> Element {
```

And add the `Select` arm to the `match widget` (alongside `LeaderKbd` / `BindingsList`):

```rust
        WidgetKind::Select => {
            let current = value.as_str().unwrap_or_default().to_string();
            let path_for_change = path.clone();
            rsx! {
                Row { label, hint,
                    control: rsx! {
                        Select::<String> {
                            default_value: current.clone(),
                            on_value_change: move |v: Option<String>| {
                                if let Some(v) = v {
                                    emit_update(&path_for_change, serde_json::json!(v));
                                }
                            },
                            SelectTrigger { SelectValue {} }
                            SelectList {
                                for (i, opt) in options.iter().enumerate() {
                                    SelectOption::<String> {
                                        key: "{opt.value}",
                                        index: i,
                                        value: opt.value.clone(),
                                        text_value: opt.label.clone(),
                                        span { "{opt.label}" }
                                        SelectItemIndicator {}
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
```

- [ ] **Step 3: Pass `options` from `FieldView`** — at the `WidgetView` call site (page.rs:257):

```rust
    if let Some(widget) = spec.widget {
        return rsx! {
            WidgetView { widget, path, value, label, hint, options: spec.options.clone() }
        };
    }
```

- [ ] **Step 4: Typecheck the WASM page**

Run: `cargo check -p vmux_setting --target wasm32-unknown-unknown 2>&1 | tail -30`
Expected: compiles (adjust to the exact dioxus-primitives `Select` prop spelling if the compiler flags a field; `default_value`, `on_value_change`, `index`, `value`, `text_value` are the confirmed names).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_setting/src/page.rs
git commit -m "feat(settings): render Mode dropdown via Select widget"
```

---

### Task 5: CEF color scheme in `bevy_cef_core` patch

**Files:**
- Modify: `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs`
- Modify: `patches/bevy_cef_core-0.5.2/src/lib.rs` (prelude re-export)
- Test: inline `#[cfg(test)]` in `browsers.rs` (pure value-map test; no CEF init)

- [ ] **Step 1: Write failing test** (append in `browsers.rs`)

```rust
#[cfg(test)]
mod color_scheme_tests {
    use super::*;
    use cef_dll_sys::cef_color_variant_t::*;

    #[test]
    fn mode_maps_to_cef_variant() {
        assert_eq!(cef_color_variant_t::from(CefColorMode::System.variant()), CEF_COLOR_VARIANT_SYSTEM);
        assert_eq!(cef_color_variant_t::from(CefColorMode::Light.variant()), CEF_COLOR_VARIANT_LIGHT);
        assert_eq!(cef_color_variant_t::from(CefColorMode::Dark.variant()), CEF_COLOR_VARIANT_DARK);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p bevy_cef_core color_scheme_tests 2>&1 | tail -20`
Expected: FAIL — `CefColorMode` not found.

- [ ] **Step 3: Implement** — in `browsers.rs`:

Extend the `cef` import to include `ColorVariant` and the `cef_dll_sys` import to include `cef_color_variant_t` (browsers.rs:16-23):
```rust
use cef::{ /* …existing… */ ColorVariant};
use cef_dll_sys::{cef_color_variant_t, cef_event_flags_t, cef_mouse_button_type_t};
```

Add the mode type + resource + mapping (near `CefDiskProfileRoot`, browsers.rs:66):
```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CefColorMode {
    Light,
    Dark,
    #[default]
    System,
}

impl CefColorMode {
    pub fn variant(self) -> ColorVariant {
        let v = match self {
            CefColorMode::Light => cef_color_variant_t::CEF_COLOR_VARIANT_LIGHT,
            CefColorMode::Dark => cef_color_variant_t::CEF_COLOR_VARIANT_DARK,
            CefColorMode::System => cef_color_variant_t::CEF_COLOR_VARIANT_SYSTEM,
        };
        ColorVariant::from(v)
    }
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct CefColorScheme(pub CefColorMode);
```

Add a field to `Browsers` (browsers.rs:108) and its `Default` (browsers.rs:121):
```rust
    color_scheme: CefColorMode,
```
```rust
            color_scheme: CefColorMode::default(),
```

Apply at shared-context creation — in `ensure_shared_disk_context`, after the `if let Some(context) = context.as_mut() { … }` block registers handlers, add inside that block:
```rust
            context.set_chrome_color_scheme(self.color_scheme.variant(), 0);
```

Make `ephemeral_request_context` take the mode and apply it (browsers.rs:1480):
```rust
    fn ephemeral_request_context(requester: Requester, mode: CefColorMode) -> Option<RequestContext> {
```
…and inside its `if let Some(context) = context.as_mut()` block:
```rust
            context.set_chrome_color_scheme(mode.variant(), 0);
```
Update its caller in `create_browser` (the `ephemeral_request_context(...)` call ~browsers.rs:249-262) to pass `self.color_scheme`.

Add the live-update method (in `impl Browsers`):
```rust
    pub fn set_color_scheme(&mut self, mode: CefColorMode) {
        self.color_scheme = mode;
        if let Some(context) = self.shared_disk_context.as_ref() {
            context.set_chrome_color_scheme(mode.variant(), 0);
        }
    }
```

- [ ] **Step 4: Re-export** in `patches/bevy_cef_core-0.5.2/src/lib.rs` prelude — add `CefColorMode, CefColorScheme` next to `CefDiskProfileRoot`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p bevy_cef_core color_scheme_tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs patches/bevy_cef_core-0.5.2/src/lib.rs
git commit -m "feat(cef): apply chrome color scheme on shared request context"
```

---

### Task 6: Wire resource + sync system into `CefPlugin`

**Files:**
- Modify: `patches/bevy_cef-0.5.2/src/lib.rs` (prelude :36, `CefPlugin::build` :67)

No unit test (requires CEF runtime); covered by build + runtime test.

- [ ] **Step 1: Re-export** in the `bevy_cef` prelude (lib.rs:36) — add `CefColorMode, CefColorScheme,` to the `pub use bevy_cef_core::prelude::{…}` list.

- [ ] **Step 2: Add resource init + sync system** in `CefPlugin::build` — extend the `app` builder chain (after the `add_plugins((...))` block, before the `RemotePlugin` check, lib.rs:94):

```rust
        app.init_resource::<bevy_cef_core::prelude::CefColorScheme>()
            .add_systems(Update, sync_color_scheme.run_if(resource_changed::<bevy_cef_core::prelude::CefColorScheme>));
```

Add the system (free fn in `lib.rs`, mirroring `zoom.rs::sync_zoom`):
```rust
fn sync_color_scheme(
    mut browsers: NonSendMut<bevy_cef_core::prelude::Browsers>,
    scheme: Res<bevy_cef_core::prelude::CefColorScheme>,
) {
    browsers.set_color_scheme(scheme.0);
}
```

Ensure `NonSendMut` / `resource_changed` are in scope (via `use bevy::prelude::*;` already present in the patch).

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p bevy_cef 2>&1 | tail -30`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add patches/bevy_cef-0.5.2/src/lib.rs
git commit -m "feat(cef): init CefColorScheme resource + sync to browsers on change"
```

---

### Task 7: vmux bridge — `AppSettings.appearance.mode` → `CefColorScheme`

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs` (plugin `build` :88-202; add a system + the mapping fn)
- Test: inline `#[cfg(test)]` in `vmux_browser/src/lib.rs` (pure mapping fn)

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod appearance_bridge_tests {
    use super::*;
    use vmux_setting::ColorScheme;
    use bevy_cef::prelude::CefColorMode;

    #[test]
    fn maps_color_scheme_to_cef_mode() {
        assert_eq!(map_color_scheme(ColorScheme::Light), CefColorMode::Light);
        assert_eq!(map_color_scheme(ColorScheme::Dark), CefColorMode::Dark);
        assert_eq!(map_color_scheme(ColorScheme::Device), CefColorMode::System);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_browser appearance_bridge_tests 2>&1 | tail -20`
Expected: FAIL — `map_color_scheme` not found.

- [ ] **Step 3: Implement** — in `vmux_browser/src/lib.rs`:

Mapping fn + bridge system (free fns):
```rust
fn map_color_scheme(mode: vmux_setting::ColorScheme) -> bevy_cef::prelude::CefColorMode {
    match mode {
        vmux_setting::ColorScheme::Light => bevy_cef::prelude::CefColorMode::Light,
        vmux_setting::ColorScheme::Dark => bevy_cef::prelude::CefColorMode::Dark,
        vmux_setting::ColorScheme::Device => bevy_cef::prelude::CefColorMode::System,
    }
}

fn sync_appearance_to_cef(
    settings: Res<AppSettings>,
    mut scheme: ResMut<bevy_cef::prelude::CefColorScheme>,
) {
    let mode = map_color_scheme(settings.appearance.mode);
    if scheme.0 != mode {
        scheme.0 = mode;
    }
}
```

Register in the plugin `build` (add to an existing `Update` `add_systems`, gated to run when settings change). `resource_changed::<AppSettings>` fires on initial insert too, so startup is covered:
```rust
            .add_systems(
                Update,
                sync_appearance_to_cef.run_if(resource_changed::<AppSettings>),
            )
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_browser appearance_bridge_tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): bridge appearance.mode to CEF color scheme"
```

---

### Task 8: Full build, format, lint, and runtime verification

- [ ] **Step 1: Format** (note: `cargo fmt` also reformats vendored `patches/`; keep only intended changes)

```bash
cargo fmt
git status --short patches/    # if unrelated patch files changed, restore them:
# git checkout -- <unrelated patch files>
```

- [ ] **Step 2: Clippy + tests on touched crates**

```bash
cargo clippy -p vmux_setting -p bevy_cef_core -p bevy_cef -p vmux_browser 2>&1 | tail -30
cargo test -p vmux_setting -p bevy_cef_core -p vmux_browser 2>&1 | tail -30
```
Expected: no warnings/errors; all tests pass.

- [ ] **Step 3: Full app build**

```bash
cargo build -p vmux_desktop 2>&1 | tail -30
```
Expected: builds.

- [ ] **Step 4: Commit any fmt/clippy fixups**

```bash
git add -A && git commit -m "chore(appearance): fmt + clippy"
```

- [ ] **Step 5: Manual runtime test** (user-driven, single pass)

  - Launch vmux. Open Settings → **Appearance → Mode**; confirm the dropdown shows Device/Light/Dark with the current value selected.
  - Open a `prefers-color-scheme`-aware site (e.g. https://web.dev or a small test page). Switch Light ↔ Dark and confirm web content flips (live, or on reload if a page caches the query). Set Device and confirm it follows the OS appearance.
  - Confirm the choice persists across relaunch (check `~/.vmux/settings.ron` contains `appearance: (mode: …)` only when non-Device).
  - Confirm vmux's own chrome (header/tabs/settings) is unchanged — expected per scope.

- [ ] **Step 6: Delete this plan file** (per AGENTS.md) once fully implemented, then open the PR.

---

## Self-Review

- **Spec coverage:** model (T1) ✓, sparse persist + no-seed (T1) ✓, Select widget (T2) ✓, Appearance/Mode registration (T3) ✓, dropdown render (T4) ✓, `set_chrome_color_scheme` on shared + ephemeral + live (T5) ✓, resource + sync system (T6) ✓, settings→CEF bridge incl. startup (T7) ✓, tests + runtime (T8) ✓.
- **Placeholders:** none — all steps carry concrete code/commands.
- **Type consistency:** `ColorScheme` (vmux_setting) vs `CefColorMode`/`CefColorScheme` (patch) kept distinct; `map_color_scheme` bridges them; `WidgetKind::Select` + `FieldSpec.options` + `crate::schema::SelectOption` used consistently across T2/T3/T4; `set_color_scheme`/`variant()` names match across T5/T6.
- **Risk:** live reflow of already-open browsers depends on CEF; create-time path guarantees new/reloaded tabs. dioxus-primitives `Select` prop spelling verified (`default_value`, `on_value_change`, `index`, `value`, `text_value`).
