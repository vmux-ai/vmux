# Per-space startup_url and startup_dir Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let each space override `startup_url` (browser) and `startup_dir` (terminal/agent cwd), falling back to a global default then the built-in, all editable by the agent via the existing `settings.ron` tools.

**Architecture:** Per-space overrides live in `AppSettings.spaces` (a `BTreeMap<space_id, SpaceOverrides>`) in `settings.ron`. Pure resolver fns in `vmux_setting` apply `per-space → global → built-in`. The `EffectiveStartupUrl` resource's updater moves to `vmux_space` (the only crate that sees both `AppSettings` and `ActiveSpace`) and recomputes on space switch. A reconcile system in `vmux_space` seeds `settings.spaces` from the space registry so `set_at_path` (strict about unknown paths) lets the agent create per-space entries.

**Tech Stack:** Rust, Bevy ECS, serde + RON, `vmux_setting` / `vmux_space` / `vmux_terminal` / `vmux_agent` crates.

**Spec:** `docs/specs/2026-06-12-per-space-startup-url-dir-design.md`

---

## File Structure

- `crates/vmux_setting/src/plugin/runtime.rs` — data model (`SpaceOverrides`, `spaces`, `browser.startup_dir`), resolvers (`resolve_startup_url` gains `space_id`, new `resolve_startup_dir`), `serialize_settings_to_ron`; remove the local `update_effective_startup_url`.
- `crates/vmux_setting/src/lib.rs` — re-export new items.
- `crates/vmux_setting/src/plugin.rs` — drop the `update_effective_startup_url` registration (keep `EffectiveStartupUrl` init).
- `crates/vmux_space/src/plugin.rs` — space-aware `update_effective_startup_url` + `reconcile_space_overrides` systems + `seed_missing_overrides` helper.
- `crates/vmux_terminal/src/plugin.rs` — two cwd default sites call `resolve_startup_dir`.
- `crates/vmux_agent/src/plugin.rs` — one cwd default site calls `resolve_startup_dir`.
- Struct-literal fixups (compile-only) in: `vmux_terminal`, `vmux_desktop` (os_menu, persistence, shortcut), `vmux_agent`, `vmux_browser`, `vmux_space`, `vmux_setting` tests.

**Note on tests:** `vmux_space`/`vmux_terminal`/`vmux_agent` pull in `bevy_cef`; the first `cargo test` build is slow. Run with `cargo test -p <crate> <name> -- --nocapture` and allow time.

---

## Task 1: Data model — `SpaceOverrides`, `spaces` map, global `browser.startup_dir`

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs` (`AppSettings` ~27-41, `BrowserSettings` ~199-209, `base_settings` ~663-686)
- Modify: `crates/vmux_setting/src/lib.rs:24-29`
- Modify (compile fixups): `crates/vmux_terminal/src/plugin.rs:2999-3022`, `crates/vmux_desktop/src/os_menu.rs:311-320`, `crates/vmux_desktop/src/persistence.rs:564-573`, `crates/vmux_desktop/src/shortcut.rs:290-300`, `crates/vmux_agent/src/plugin.rs:1145-1160`, `crates/vmux_browser/src/lib.rs:3485-3495 & 4264-4275`, `crates/vmux_space/src/plugin.rs:572-595`
- Test: `crates/vmux_setting/src/plugin/runtime.rs` (tests module)

- [ ] **Step 1: Add the `spaces` field + `SpaceOverrides` type + global `startup_dir`**

In `runtime.rs`, add `spaces` to `AppSettings` (after the `agent` field, before the closing brace of the struct ~line 40):

```rust
    #[serde(default)]
    pub spaces: std::collections::BTreeMap<String, SpaceOverrides>,
```

Add the new type just below the `AppSettings` struct:

```rust
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SpaceOverrides {
    #[serde(default)]
    pub startup_url: Option<String>,
    #[serde(default)]
    pub startup_dir: Option<String>,
}
```

Replace `BrowserSettings` (~199-209) with:

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowserSettings {
    #[serde(default = "default_browser_startup_url")]
    pub startup_url: String,
    #[serde(default)]
    pub startup_dir: Option<String>,
}

fn default_browser_settings() -> BrowserSettings {
    BrowserSettings {
        startup_url: default_browser_startup_url(),
        startup_dir: None,
    }
}
```

- [ ] **Step 2: Fix the in-crate test literal (`base_settings`)**

In `runtime.rs` `base_settings()` (~664), add `startup_dir: None,` to the `BrowserSettings { … }` literal and `spaces: Default::default(),` to the `AppSettings { … }` literal (after `agent: …`).

- [ ] **Step 3: Re-export `SpaceOverrides`**

In `lib.rs`, add `SpaceOverrides` to the `pub use plugin::runtime::{…}` list (keep alphabetical-ish, e.g. after `ShortcutSettings,`).

- [ ] **Step 4: Add the roundtrip tests**

Append to the `runtime.rs` tests module:

```rust
    #[test]
    fn app_settings_spaces_roundtrip_through_ron() {
        let mut s = base_settings();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: Some("/tmp/work".into()),
            },
        );
        let ron = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: AppSettings = ron::de::from_str(&ron).unwrap();
        assert_eq!(
            back.spaces["work"].startup_url.as_deref(),
            Some("https://work.example")
        );
        assert_eq!(back.spaces["work"].startup_dir.as_deref(), Some("/tmp/work"));
    }

    #[test]
    fn embedded_settings_have_empty_spaces_and_no_global_startup_dir() {
        let s = load_embedded_settings();
        assert!(s.spaces.is_empty());
        assert!(s.browser.startup_dir.is_none());
    }
```

- [ ] **Step 5: Run the new tests to verify they pass**

Run: `cargo test -p vmux_setting app_settings_spaces_roundtrip_through_ron embedded_settings_have_empty`
Expected: PASS (2 tests).

- [ ] **Step 6: Fix remaining struct literals across crates (compiler-driven)**

For every `AppSettings { … }` literal add `spaces: Default::default(),`; for every `BrowserSettings { … }` literal add `startup_dir: None,`. Known sites:
- `vmux_terminal/src/plugin.rs:3000` (`AppSettings`) + `:3001` (`BrowserSettings`)
- `vmux_desktop/src/os_menu.rs:312` + `:313`
- `vmux_desktop/src/persistence.rs:565` + `:566`
- `vmux_desktop/src/shortcut.rs:291` + `:292`
- `vmux_agent/src/plugin.rs:1146` + `:1147`
- `vmux_browser/src/lib.rs:3486` + `:3487`, and `:4265` + `:4266`
- `vmux_space/src/plugin.rs:573` + `:574`

Then build the workspace to catch any literal missed:

Run: `cargo build --workspace --tests 2>&1 | rg "missing field|E0063" || echo CLEAN`
Expected: `CLEAN` (fix any reported literal, re-run until clean).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_setting crates/vmux_terminal crates/vmux_desktop crates/vmux_agent crates/vmux_browser crates/vmux_space
git commit -m "feat(settings): add per-space overrides and global startup_dir to model"
```

---

## Task 2: Resolver `resolve_startup_dir` + `serialize_settings_to_ron`

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs` (add fns near `resolve_startup_url` ~77)
- Modify: `crates/vmux_setting/src/lib.rs:24-29`
- Test: `crates/vmux_setting/src/plugin/runtime.rs` (tests module)

- [ ] **Step 1: Write the failing tests**

Append to the `runtime.rs` tests module (`tempfile` is already a dependency):

```rust
    #[test]
    fn resolve_startup_dir_prefers_per_space_then_global_then_builtin() {
        let per = tempfile::tempdir().unwrap();
        let glob = tempfile::tempdir().unwrap();
        let mut s = base_settings();
        s.browser.startup_dir = Some(glob.path().to_string_lossy().into());
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(per.path().to_string_lossy().into()),
            },
        );
        assert_eq!(resolve_startup_dir(&s, "work"), per.path());
        assert_eq!(resolve_startup_dir(&s, "other"), glob.path());
        s.browser.startup_dir = None;
        assert_eq!(
            resolve_startup_dir(&s, "space-1"),
            vmux_core::profile::space_dir("space-1")
        );
    }

    #[test]
    fn resolve_startup_dir_falls_through_on_nonexistent_dir() {
        let mut s = base_settings();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some("/no/such/dir/xyz-vmux".into()),
            },
        );
        assert_eq!(
            resolve_startup_dir(&s, "work"),
            vmux_core::profile::space_dir("work")
        );
    }

    #[test]
    fn serialize_settings_to_ron_reparses() {
        let s = base_settings();
        let ron = serialize_settings_to_ron(&s).unwrap();
        let _back: AppSettings = ron::de::from_str(&ron).unwrap();
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vmux_setting resolve_startup_dir serialize_settings_to_ron`
Expected: FAIL (compile error: `resolve_startup_dir` / `serialize_settings_to_ron` not found).

- [ ] **Step 3: Implement the resolver + serializer**

In `runtime.rs`, just below `resolve_startup_url` (~line 84), add:

```rust
pub fn resolve_startup_dir(settings: &AppSettings, space_id: &str) -> std::path::PathBuf {
    let candidate = settings
        .spaces
        .get(space_id)
        .and_then(|o| o.startup_dir.as_deref())
        .or(settings.browser.startup_dir.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(dir) = candidate {
        let path = std::path::PathBuf::from(dir);
        if path.is_dir() {
            return path;
        }
    }
    vmux_core::profile::space_dir(space_id)
}

pub fn serialize_settings_to_ron(settings: &AppSettings) -> Result<String, String> {
    ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("RON serialize failed: {e}"))
}
```

- [ ] **Step 4: Re-export the new fns**

In `lib.rs`, add `resolve_startup_dir` and `serialize_settings_to_ron` to the `pub use plugin::runtime::{…}` list.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p vmux_setting resolve_startup_dir serialize_settings_to_ron`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_setting
git commit -m "feat(settings): add resolve_startup_dir and serialize_settings_to_ron"
```

---

## Task 3: Dir wiring — terminals + agent open in `resolve_startup_dir`

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs:370`, `:475`
- Modify: `crates/vmux_agent/src/plugin.rs:322-327`
- Test: `crates/vmux_terminal/src/plugin.rs` (tests module)

- [ ] **Step 1: Write the failing test**

Append to the `vmux_terminal` tests module (mirrors `terminal_page_open_accepts_url_without_trailing_slash`). It sets a per-space `startup_dir` for the default active space (`space-1`) and asserts the spawned terminal launches there:

```rust
    #[test]
    fn open_terminal_page_uses_per_space_startup_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "space-1".into(),
            vmux_setting::SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.path().to_string_lossy().into()),
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<vmux_space::spaces::ActiveSpace>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_terminal_page_open);

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://terminal".to_string(),
            request_id: None,
        });

        app.update();

        let mut launches = app
            .world_mut()
            .query_filtered::<&crate::launch::TerminalLaunch, With<Terminal>>();
        let launch = launches.iter(app.world()).next().expect("terminal spawned");
        assert_eq!(launch.cwd, dir.path().to_string_lossy());
    }
```

(The default `ActiveSpace` record id is `space-1`; confirm via `vmux_space::model::BOOTSTRAP_SPACE_ID`.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vmux_terminal open_terminal_page_uses_per_space_startup_dir`
Expected: FAIL (launch.cwd is `~/.vmux/space-1`, not the temp dir).

- [ ] **Step 3: Wire `resolve_startup_dir` at the terminal sites**

`plugin.rs:370` — replace:

```rust
                let cwd = vmux_space::cwd::space_dir(&active_space.record.id);
```

with:

```rust
                let cwd = vmux_setting::resolve_startup_dir(&settings, &active_space.record.id);
```

`plugin.rs:475` (the `else` default branch of `open_terminal_page`) — replace:

```rust
        Some(vmux_space::cwd::space_dir(&active_space.record.id))
```

with:

```rust
        Some(vmux_setting::resolve_startup_dir(&settings, &active_space.record.id))
```

- [ ] **Step 4: Wire `resolve_startup_dir` at the agent site**

`vmux_agent/src/plugin.rs:322-327` — replace the `space_dir` default:

```rust
                        let cwd_path = cwd_opt.unwrap_or_else(|| {
                            active_space
                                .as_ref()
                                .map(|s| vmux_setting::resolve_startup_dir(&sp.settings, &s.record.id))
                                .unwrap_or_else(default_space_dir)
                        });
```

(`sp.settings` is the `ResMut<AppSettings>` in `SettingsParams`; `active_space` is the local `lookups.active_space.as_deref()`.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p vmux_terminal open_terminal_page_uses_per_space_startup_dir`
Expected: PASS.

- [ ] **Step 6: Verify the agent crate still builds**

Run: `cargo build -p vmux_agent`
Expected: builds clean.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_terminal crates/vmux_agent
git commit -m "feat(terminal): open new terminals and agent tabs in resolved startup_dir"
```

---

## Task 4: URL wiring — per-space `EffectiveStartupUrl`, updater moves to `vmux_space`

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs` (`resolve_startup_url` ~77; remove `update_effective_startup_url` ~13-22; update 5 tests ~692-718)
- Modify: `crates/vmux_setting/src/plugin.rs:11,33-36,41` (drop the updater; keep `init_resource`)
- Modify: `crates/vmux_space/src/plugin.rs` (add system + registration + imports)
- Test: both `runtime.rs` and `vmux_space/src/plugin.rs` tests modules

- [ ] **Step 1: Make `resolve_startup_url` space-aware (update fn + existing tests)**

Replace `resolve_startup_url` (~77-84) with:

```rust
pub fn resolve_startup_url(settings: &AppSettings, space_id: &str) -> String {
    let per_space = settings
        .spaces
        .get(space_id)
        .and_then(|o| o.startup_url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let chosen = per_space.unwrap_or_else(|| settings.browser.startup_url.trim());
    if chosen.is_empty() || chosen == "vmux://agent/" || chosen == "vmux://agent" {
        default_browser_startup_url()
    } else {
        chosen.to_string()
    }
}
```

Update the 5 existing call sites in the tests module (~692-718) to pass a space id, e.g. `resolve_startup_url(&s, "space-1")` (the `spaces` map is empty there, so all fall through to global exactly as before).

- [ ] **Step 2: Add per-space url resolver tests**

Append to the `runtime.rs` tests module:

```rust
    #[test]
    fn resolve_startup_url_prefers_per_space_override() {
        let mut s = base_settings();
        s.browser.startup_url = "https://global.example".into();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: None,
            },
        );
        assert_eq!(resolve_startup_url(&s, "work"), "https://work.example");
        assert_eq!(resolve_startup_url(&s, "other"), "https://global.example");
    }

    #[test]
    fn resolve_startup_url_blank_per_space_falls_to_global() {
        let mut s = base_settings();
        s.browser.startup_url = "https://global.example".into();
        s.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("   ".into()),
                startup_dir: None,
            },
        );
        assert_eq!(resolve_startup_url(&s, "work"), "https://global.example");
    }
```

- [ ] **Step 3: Remove the updater from `vmux_setting`**

In `runtime.rs`, delete the whole `update_effective_startup_url` fn (~13-22).

In `plugin.rs`:
- Remove `update_effective_startup_url` from the `use runtime::{…}` import (line 11).
- Delete the `Startup` registration block (~31-36) that adds `update_effective_startup_url`.
- Delete the `.add_systems(Update, update_effective_startup_url)` line (~41).
- Keep `.init_resource::<vmux_layout::settings::EffectiveStartupUrl>()` (line 29).

- [ ] **Step 4: Run the setting tests + build**

Run: `cargo test -p vmux_setting resolve_startup_url`
Expected: PASS (all resolve_startup_url tests, old + new).
Run: `cargo build -p vmux_setting`
Expected: clean (no unused-import warning for the removed updater).

- [ ] **Step 5: Add the space-aware updater test in `vmux_space` (failing)**

Append to the `vmux_space/src/plugin.rs` tests module (uses the existing `test_settings()` and `work_space_record()` helpers there; `work_space_record()` has id `"work"`):

```rust
    #[test]
    fn effective_startup_url_reflects_active_space_override() {
        let mut settings = test_settings();
        settings.browser.startup_url = "https://global.example".into();
        settings.spaces.insert(
            "work".into(),
            vmux_setting::SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: None,
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
            .insert_resource(ActiveSpace {
                record: work_space_record(),
            })
            .add_systems(Update, update_effective_startup_url);

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_layout::settings::EffectiveStartupUrl>()
                .0,
            "https://work.example"
        );
    }
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo test -p vmux_space effective_startup_url_reflects_active_space_override`
Expected: FAIL (compile error: `update_effective_startup_url` not defined in `vmux_space`).

- [ ] **Step 7: Add the updater system in `vmux_space`**

In `vmux_space/src/plugin.rs`, add the system (near the other free systems):

```rust
fn update_effective_startup_url(
    settings: Option<Res<vmux_setting::AppSettings>>,
    active: Option<Res<ActiveSpace>>,
    mut effective: ResMut<vmux_layout::settings::EffectiveStartupUrl>,
) {
    let (Some(settings), Some(active)) = (settings, active) else {
        return;
    };
    if settings.is_changed() || active.is_changed() || effective.0.is_empty() {
        effective.0 = vmux_setting::resolve_startup_url(&settings, &active.record.id);
    }
}
```

Register it in `SpacePlugin::build` (chain onto the existing builder), matching the ordering the old `vmux_setting` system used:

```rust
            .add_systems(
                Startup,
                update_effective_startup_url
                    .after(vmux_setting::SettingsLoadSet)
                    .before(vmux_layout::LayoutStartupSet::Post),
            )
            .add_systems(Update, update_effective_startup_url)
```

(`ActiveSpace` is already imported via `crate::spaces::ActiveSpace`; `SpaceRecord` via `crate::model`. No new `use` needed beyond fully-qualified `vmux_setting::` / `vmux_layout::` paths.)

- [ ] **Step 8: Run the test to verify it passes**

Run: `cargo test -p vmux_space effective_startup_url_reflects_active_space_override`
Expected: PASS.

- [ ] **Step 9: Build the app crate to confirm the cross-crate move is wired**

Run: `cargo build -p vmux_desktop`
Expected: clean.

- [ ] **Step 10: Commit**

```bash
git add crates/vmux_setting crates/vmux_space
git commit -m "feat(settings): per-space startup_url via space-aware EffectiveStartupUrl"
```

---

## Task 5: Seed `settings.spaces` from the registry (agent-creatable entries)

**Files:**
- Modify: `crates/vmux_space/src/plugin.rs` (add `seed_missing_overrides` helper + `reconcile_space_overrides` system + registration)
- Test: `crates/vmux_space/src/plugin.rs` tests module

- [ ] **Step 1: Write the failing helper tests**

Append to the `vmux_space/src/plugin.rs` tests module:

```rust
    #[test]
    fn seed_adds_missing_and_reports_change() {
        let mut spaces = std::collections::BTreeMap::new();
        let registry = crate::model::SpaceRegistry {
            spaces: vec![
                crate::model::SpaceRecord {
                    id: "a".into(),
                    name: "A".into(),
                    profile: "P".into(),
                },
                crate::model::SpaceRecord {
                    id: "b".into(),
                    name: "B".into(),
                    profile: "P".into(),
                },
            ],
        };
        assert!(seed_missing_overrides(&mut spaces, &registry));
        assert_eq!(spaces.len(), 2);
        assert!(spaces.contains_key("a") && spaces.contains_key("b"));
    }

    #[test]
    fn seed_preserves_existing_and_reports_no_change() {
        let mut spaces = std::collections::BTreeMap::new();
        spaces.insert(
            "a".into(),
            vmux_setting::SpaceOverrides {
                startup_url: Some("x".into()),
                startup_dir: None,
            },
        );
        let registry = crate::model::SpaceRegistry {
            spaces: vec![crate::model::SpaceRecord {
                id: "a".into(),
                name: "A".into(),
                profile: "P".into(),
            }],
        };
        assert!(!seed_missing_overrides(&mut spaces, &registry));
        assert_eq!(spaces["a"].startup_url.as_deref(), Some("x"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p vmux_space seed_`
Expected: FAIL (compile error: `seed_missing_overrides` not found).

- [ ] **Step 3: Implement the helper + system**

In `vmux_space/src/plugin.rs`, add the pure helper:

```rust
pub(crate) fn seed_missing_overrides(
    spaces: &mut std::collections::BTreeMap<String, vmux_setting::SpaceOverrides>,
    registry: &SpaceRegistry,
) -> bool {
    let mut added = false;
    for space in &registry.spaces {
        if !spaces.contains_key(&space.id) {
            spaces.insert(space.id.clone(), vmux_setting::SpaceOverrides::default());
            added = true;
        }
    }
    added
}
```

(`SpaceRegistry` is already imported via `crate::model`.)

Add the system:

```rust
fn reconcile_space_overrides(
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    active: Option<Res<ActiveSpace>>,
    mut writes: MessageWriter<vmux_setting::SettingsWriteRequest>,
) {
    let (Some(mut settings), Some(active)) = (settings, active) else {
        return;
    };
    if !(settings.is_changed() || active.is_changed()) {
        return;
    }
    let registry = read_space_registry_from(&profile::shared_data_dir());
    if seed_missing_overrides(&mut settings.spaces, &registry) {
        match vmux_setting::serialize_settings_to_ron(&settings) {
            Ok(ron_bytes) => {
                writes.write(vmux_setting::SettingsWriteRequest { ron_bytes });
            }
            Err(e) => bevy::log::warn!("reconcile_space_overrides: serialize failed: {e}"),
        }
    }
}
```

Register it in `SpacePlugin::build`:

```rust
            .add_systems(
                Startup,
                reconcile_space_overrides.after(vmux_setting::SettingsLoadSet),
            )
            .add_systems(Update, reconcile_space_overrides)
```

(`MessageWriter`, `Res`, `ResMut` are in the bevy prelude already imported; `read_space_registry_from` and `profile` are already imported at the top of the file.)

- [ ] **Step 4: Run the helper tests to verify they pass**

Run: `cargo test -p vmux_space seed_`
Expected: PASS (2 tests).

- [ ] **Step 5: Build to confirm the system wiring compiles**

Run: `cargo build -p vmux_space`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_space
git commit -m "feat(space): seed settings.spaces from registry for agent edits"
```

---

## Task 6: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Format**

Run: `cargo fmt --all`
Then: `git diff --stat` — if fmt changed files, `git add -u && git commit -m "style: cargo fmt"`.

- [ ] **Step 2: Clippy on the touched crates**

Run: `cargo clippy -p vmux_setting -p vmux_space -p vmux_terminal -p vmux_agent --all-targets -- -D warnings`
Expected: no warnings. Fix any before proceeding.

- [ ] **Step 3: Targeted test sweep**

Run: `cargo test -p vmux_setting -p vmux_space -p vmux_terminal`
Expected: all PASS.

- [ ] **Step 4: Manual smoke (agent path) — note for the human reviewer**

In a dev build: `get_settings` shows a `spaces` map with an entry per space (seeded). `update_settings` with `path: "spaces.space-1.startup_url"`, `value: "https://example.com"` succeeds; reopening the browser startup in that space loads the URL. `update_settings` `path: "spaces.space-1.startup_dir"` to an existing dir → new terminals in that space open there. Switching spaces flips the resolved URL.

- [ ] **Step 5: Delete the plan file (per AGENTS.md)**

```bash
git rm docs/plans/2026-06-12-per-space-startup-url-dir.md
git commit -m "chore: remove completed implementation plan"
```
