# vmux_settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a settings webview app at `vmux://settings/` (Dioxus WASM via vmux_ui) that displays every field of `AppSettings` as a form with auto-save, plus two MCP tools (`get_settings`, `update_settings`) so bots edit through the same path.

**Architecture:** New crate `vmux_settings` mirroring `vmux_space` (Dioxus WASM bundle registered with `WebviewAppRegistry` at host `settings`). New `SettingsViewPlugin` in `vmux_desktop` broadcasts `AppSettings` JSON to the view and handles edits coming back. Both view + MCP edits funnel into a single shared function `apply_settings_update` that mutates the resource and queues a disk write; a `LastSelfWriteHash` resource prevents the existing file watcher from looping.

**Tech Stack:** Bevy 0.18, Dioxus 0.7.4, bevy_cef 0.5.2, rkyv (binary IPC), serde/serde_json, ron, tempfile, notify, JsonRPC over stdio (MCP).

**Spec:** `docs/specs/2026-05-15-vmux-settings-design.md`

---

## Setup

- [ ] **Step 0a: Move Linear issue to In Progress** (if there is one — skip if not)

- [ ] **Step 0b: Create worktree per AGENTS.md**

```bash
git fetch origin main
git worktree add .worktrees/vmux-settings -b feat/vmux-settings origin/main
cd .worktrees/vmux-settings
```

All subsequent steps run inside `.worktrees/vmux-settings/`.

- [ ] **Step 0c: Verify changed-crates script exists**

```bash
ls scripts/changed-crates.sh
```

Expected: file exists. This script is used at the end of every task to decide which crates need fmt + clippy + test before committing.

---

## Task 1: Add `Serialize` derives to settings structs

The plan needs `AppSettings` to round-trip through `serde_json::Value`. Currently every settings struct only derives `Deserialize`. Add `Serialize` to each.

**Files:**
- Modify: `crates/vmux_layout/src/settings.rs`
- Modify: `crates/vmux_desktop/src/settings.rs`
- Modify: `crates/vmux_desktop/src/themes.rs` (TerminalColorScheme is referenced from `TerminalSettings.custom_themes`)

- [ ] **Step 1.1: Write failing roundtrip test**

Add to the bottom of `crates/vmux_desktop/src/settings.rs` `mod tests`:

```rust
    #[test]
    fn app_settings_roundtrips_through_json() {
        let original = base_settings();
        let value = serde_json::to_value(&original).expect("serialize");
        let recovered: AppSettings = serde_json::from_value(value).expect("deserialize");
        assert_eq!(recovered.layout.window.padding, original.layout.window.padding);
        assert_eq!(recovered.layout.pane.gap, original.layout.pane.gap);
        assert_eq!(recovered.shortcuts.chord_timeout_ms, original.shortcuts.chord_timeout_ms);
        assert_eq!(recovered.auto_update, original.auto_update);
    }
```

- [ ] **Step 1.2: Run test to confirm it fails (compile error: AppSettings doesn't implement Serialize)**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib settings::tests::app_settings_roundtrips_through_json 2>&1 | tail -30
```

Expected: compile error mentioning `the trait Serialize is not implemented for AppSettings` (or similar).

- [ ] **Step 1.3: Add `Serialize` to every settings struct in `crates/vmux_layout/src/settings.rs`**

For each struct currently `#[derive(Clone, Debug, Deserialize, ...)]` add `Serialize` to the derive list and `use serde::Serialize;` at the top:

Edit `crates/vmux_layout/src/settings.rs` — replace top imports:

```rust
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
```

Then update every `derive(...)` containing `Deserialize` to also include `Serialize` for these structs:
- `LayoutSettings`
- `SideSheetSettings`
- `WindowSettings`
- `FocusRingColor`
- `FocusRingGlow`
- `FocusRingGradient`
- `FocusRingSettings`
- `PaneSettings`

Keep `ConfirmCloseSettings` and `EffectiveStartupUrl` as-is (they aren't in `AppSettings`).

- [ ] **Step 1.4: Add `Serialize` in `crates/vmux_desktop/src/settings.rs`**

Replace the import:

```rust
use serde::{Deserialize, Serialize};
```

Add `Serialize` to derive lines for:
- `AppSettings`
- `ShortcutSettings`
- `ShortcutEntry`
- `ShortcutDef`
- `KeyComboDef`
- `BrowserSettings`
- `TerminalSettings`
- `TerminalTheme`

- [ ] **Step 1.5: Add `Serialize` to `TerminalColorScheme` in `crates/vmux_desktop/src/themes.rs`**

Find the `TerminalColorScheme` struct and add `Serialize` to its derive (alongside existing `Deserialize`). Add `use serde::{Deserialize, Serialize};` if needed.

- [ ] **Step 1.6: Run test to confirm it passes**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib settings::tests::app_settings_roundtrips_through_json 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 1.7: Run fmt + clippy + full test on changed crates**

```bash
for pkg in vmux_layout vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_layout vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_layout vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 1.8: Commit**

```bash
git add crates/vmux_layout/src/settings.rs crates/vmux_desktop/src/settings.rs crates/vmux_desktop/src/themes.rs
git -c commit.gpgsign=false commit -m "feat(settings): derive Serialize for round-tripping through JSON"
```

---

## Task 2: `set_at_path` helper (TDD)

Walks a dot-separated path with `[i]` array indexing and replaces a leaf in a `serde_json::Value`.

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`

- [ ] **Step 2.1: Write failing tests**

Append to `mod tests` in `crates/vmux_desktop/src/settings.rs`:

```rust
    #[test]
    fn set_at_path_replaces_nested_object_value() {
        let mut root = serde_json::json!({"layout": {"pane": {"gap": 8.0}}});
        set_at_path(&mut root, "layout.pane.gap", serde_json::json!(12.0)).unwrap();
        assert_eq!(root["layout"]["pane"]["gap"], serde_json::json!(12.0));
    }

    #[test]
    fn set_at_path_replaces_array_element_field() {
        let mut root = serde_json::json!({
            "terminal": {"themes": [{"name": "default", "font_size": 14.0}]}
        });
        set_at_path(&mut root, "terminal.themes[0].font_size", serde_json::json!(16.0)).unwrap();
        assert_eq!(root["terminal"]["themes"][0]["font_size"], serde_json::json!(16.0));
    }

    #[test]
    fn set_at_path_top_level_leaf() {
        let mut root = serde_json::json!({"auto_update": true});
        set_at_path(&mut root, "auto_update", serde_json::json!(false)).unwrap();
        assert_eq!(root["auto_update"], serde_json::json!(false));
    }

    #[test]
    fn set_at_path_unknown_key_errors() {
        let mut root = serde_json::json!({"layout": {}});
        let err = set_at_path(&mut root, "layout.nope", serde_json::json!(1)).unwrap_err();
        assert!(err.contains("layout.nope"), "error must mention path: {err}");
    }

    #[test]
    fn set_at_path_array_out_of_bounds_errors() {
        let mut root = serde_json::json!({"themes": [{"font_size": 14.0}]});
        let err = set_at_path(&mut root, "themes[5].font_size", serde_json::json!(16.0)).unwrap_err();
        assert!(err.contains("themes[5]"), "error must mention path: {err}");
    }

    #[test]
    fn set_at_path_empty_path_errors() {
        let mut root = serde_json::json!({});
        assert!(set_at_path(&mut root, "", serde_json::json!(1)).is_err());
    }
```

- [ ] **Step 2.2: Run tests to confirm they fail (no `set_at_path`)**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib settings::tests::set_at_path 2>&1 | tail -20
```

Expected: compile error `cannot find function set_at_path`.

- [ ] **Step 2.3: Implement `set_at_path`**

Add inside `crates/vmux_desktop/src/settings.rs` (above `mod tests`):

```rust
pub(crate) fn set_at_path(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("empty settings path".to_string());
    }
    let segments = parse_path_segments(path)?;
    let (last, parents) = segments
        .split_last()
        .ok_or_else(|| "empty settings path".to_string())?;

    let mut cursor = root;
    let mut walked = String::new();
    for segment in parents {
        append_segment(&mut walked, segment);
        cursor = descend(cursor, segment, &walked)?;
    }
    append_segment(&mut walked, last);
    set_leaf(cursor, last, &walked, value)
}

#[derive(Debug)]
enum PathSegment {
    Field(String),
    Index(usize),
}

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, String> {
    let mut out = Vec::new();
    for raw in path.split('.') {
        if raw.is_empty() {
            return Err(format!("empty segment in path: {path}"));
        }
        let mut chars = raw.chars();
        let mut name = String::new();
        for ch in chars.by_ref() {
            if ch == '[' {
                break;
            }
            name.push(ch);
        }
        if name.is_empty() {
            return Err(format!("missing field name before '[' in {raw}"));
        }
        out.push(PathSegment::Field(name));
        let mut tail: String = chars.collect();
        while !tail.is_empty() {
            let close = tail.find(']').ok_or_else(|| format!("unclosed '[' in {raw}"))?;
            let idx_str = &tail[..close];
            let idx: usize = idx_str
                .parse()
                .map_err(|_| format!("non-integer index '[{idx_str}]' in {raw}"))?;
            out.push(PathSegment::Index(idx));
            tail = tail[close + 1..].to_string();
            if !tail.is_empty() && !tail.starts_with('[') {
                return Err(format!("unexpected text after ']' in {raw}: {tail}"));
            }
        }
    }
    Ok(out)
}

fn append_segment(walked: &mut String, segment: &PathSegment) {
    match segment {
        PathSegment::Field(name) => {
            if !walked.is_empty() {
                walked.push('.');
            }
            walked.push_str(name);
        }
        PathSegment::Index(i) => {
            walked.push_str(&format!("[{i}]"));
        }
    }
}

fn descend<'a>(
    cursor: &'a mut serde_json::Value,
    segment: &PathSegment,
    walked: &str,
) -> Result<&'a mut serde_json::Value, String> {
    match segment {
        PathSegment::Field(name) => cursor
            .get_mut(name.as_str())
            .ok_or_else(|| format!("unknown setting path: {walked}")),
        PathSegment::Index(i) => cursor
            .get_mut(*i)
            .ok_or_else(|| format!("unknown setting path: {walked}")),
    }
}

fn set_leaf(
    cursor: &mut serde_json::Value,
    segment: &PathSegment,
    walked: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    match segment {
        PathSegment::Field(name) => {
            let map = cursor
                .as_object_mut()
                .ok_or_else(|| format!("cannot index field on non-object at {walked}"))?;
            if !map.contains_key(name) {
                return Err(format!("unknown setting path: {walked}"));
            }
            map.insert(name.clone(), value);
            Ok(())
        }
        PathSegment::Index(i) => {
            let arr = cursor
                .as_array_mut()
                .ok_or_else(|| format!("cannot index by [{i}] on non-array at {walked}"))?;
            if *i >= arr.len() {
                return Err(format!("unknown setting path: {walked}"));
            }
            arr[*i] = value;
            Ok(())
        }
    }
}
```

- [ ] **Step 2.4: Run tests to confirm they pass**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib settings::tests::set_at_path 2>&1 | tail -15
```

Expected: 6 passed.

- [ ] **Step 2.5: Commit**

```bash
git add crates/vmux_desktop/src/settings.rs
git -c commit.gpgsign=false commit -m "feat(settings): add set_at_path helper for dot-path JSON edits"
```

---

## Task 3: `apply_settings_update` (TDD)

Mutates `AppSettings` in place via JSON path; returns the RON bytes to write to disk.

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`

- [ ] **Step 3.1: Write failing tests**

Append to `mod tests`:

```rust
    #[test]
    fn apply_settings_update_changes_pane_gap_and_returns_ron() {
        let mut settings = base_settings();
        let ron_bytes = apply_settings_update(
            &mut settings,
            "layout.pane.gap",
            serde_json::json!(16.0),
        )
        .expect("apply ok");
        assert_eq!(settings.layout.pane.gap, 16.0);
        assert!(ron_bytes.contains("gap"));
        assert!(ron_bytes.contains("16"));
        // round-trip the RON back to confirm validity
        let reparsed: AppSettings = ron::de::from_str(&ron_bytes).expect("RON parses");
        assert_eq!(reparsed.layout.pane.gap, 16.0);
    }

    #[test]
    fn apply_settings_update_changes_top_level_bool() {
        let mut settings = base_settings();
        apply_settings_update(&mut settings, "auto_update", serde_json::json!(true)).unwrap();
        assert!(settings.auto_update);
    }

    #[test]
    fn apply_settings_update_unknown_path_errors_without_mutating() {
        let mut settings = base_settings();
        let original_gap = settings.layout.pane.gap;
        let err = apply_settings_update(
            &mut settings,
            "layout.nope",
            serde_json::json!(1),
        )
        .unwrap_err();
        assert!(err.contains("layout.nope"));
        assert_eq!(settings.layout.pane.gap, original_gap);
    }

    #[test]
    fn apply_settings_update_type_mismatch_errors_without_mutating() {
        let mut settings = base_settings();
        let original_auto = settings.auto_update;
        let err = apply_settings_update(
            &mut settings,
            "auto_update",
            serde_json::json!("yes"),
        )
        .unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(settings.auto_update, original_auto);
    }
```

- [ ] **Step 3.2: Run tests to confirm they fail**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib settings::tests::apply_settings_update 2>&1 | tail -15
```

Expected: compile error `cannot find function apply_settings_update`.

- [ ] **Step 3.3: Implement `apply_settings_update`**

Add inside `crates/vmux_desktop/src/settings.rs` (above `mod tests`):

```rust
/// Apply a single dot-path update to `AppSettings`. On success, returns the
/// pretty-printed RON bytes that callers should write to disk via the
/// `SettingsWriteRequest` event. On failure, leaves `settings` unchanged and
/// returns an error string.
pub(crate) fn apply_settings_update(
    settings: &mut AppSettings,
    path: &str,
    value: serde_json::Value,
) -> Result<String, String> {
    let mut value_json =
        serde_json::to_value(&*settings).map_err(|e| format!("settings to JSON failed: {e}"))?;
    set_at_path(&mut value_json, path, value)?;
    let new_settings: AppSettings = serde_json::from_value(value_json)
        .map_err(|e| format!("invalid value for path '{path}': {e}"))?;
    let ron_bytes =
        ron::ser::to_string_pretty(&new_settings, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("RON serialize failed: {e}"))?;
    *settings = new_settings;
    Ok(ron_bytes)
}

pub(crate) fn serialize_settings_to_json(settings: &AppSettings) -> String {
    serde_json::to_string(settings).unwrap_or_else(|_| "{}".to_string())
}
```

- [ ] **Step 3.4: Run tests to confirm they pass**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib settings::tests::apply_settings_update 2>&1 | tail -15
```

Expected: 4 passed.

- [ ] **Step 3.5: Commit**

```bash
git add crates/vmux_desktop/src/settings.rs
git -c commit.gpgsign=false commit -m "feat(settings): add apply_settings_update helper"
```

---

## Task 4: `LastSelfWriteHash` + `SettingsWriteRequest` + persist system

The desktop must write the RON bytes to disk and remember the hash so the file watcher skips its own writes.

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`

- [ ] **Step 4.1: Add new resource and event types**

Below the existing `SettingsPlugin` `impl`, add (or near top — keep with related types):

```rust
#[derive(Resource, Default, Debug)]
pub struct LastSelfWriteHash(pub Option<u64>);

#[derive(Message, Debug, Clone)]
pub struct SettingsWriteRequest {
    pub ron_bytes: String,
}

fn settings_content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
```

`Message` must already be in scope from the existing `use bevy::prelude::*;` at the top of the file. If not, add `use bevy::ecs::message::Message;`.

- [ ] **Step 4.2: Wire the resource and message into `SettingsPlugin::build`**

Edit `SettingsPlugin::build`. The current body is:

```rust
fn build(&self, app: &mut App) {
    app.configure_sets(
        Startup,
        SettingsLoadSet.before(vmux_layout::LayoutStartupSet::Window),
    )
    .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
    .add_systems(Startup, load_settings.in_set(SettingsLoadSet))
    .add_systems(
        Startup,
        update_effective_startup_url
            .after(SettingsLoadSet)
            .before(vmux_layout::LayoutStartupSet::Post),
    )
    .add_systems(Update, reload_settings_on_change)
    .add_systems(Update, update_effective_startup_url);
}
```

Replace with:

```rust
fn build(&self, app: &mut App) {
    app.init_resource::<LastSelfWriteHash>()
        .add_message::<SettingsWriteRequest>()
        .configure_sets(
            Startup,
            SettingsLoadSet.before(vmux_layout::LayoutStartupSet::Window),
        )
        .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
        .add_systems(Startup, load_settings.in_set(SettingsLoadSet))
        .add_systems(
            Startup,
            update_effective_startup_url
                .after(SettingsLoadSet)
                .before(vmux_layout::LayoutStartupSet::Post),
        )
        .add_systems(
            Update,
            (persist_settings_to_disk, reload_settings_on_change).chain(),
        )
        .add_systems(Update, update_effective_startup_url);
}
```

- [ ] **Step 4.3: Implement `persist_settings_to_disk` system**

Add at top-level in `crates/vmux_desktop/src/settings.rs`:

```rust
fn persist_settings_to_disk(
    mut reader: MessageReader<SettingsWriteRequest>,
    watcher: Option<Res<SettingsWatcher>>,
    mut last_hash: ResMut<LastSelfWriteHash>,
) {
    for request in reader.read() {
        let Some(watcher) = watcher.as_deref() else {
            bevy::log::warn!("settings: no watcher path; cannot persist");
            continue;
        };
        let bytes = request.ron_bytes.as_bytes();
        let hash = settings_content_hash(bytes);
        last_hash.0 = Some(hash);
        if let Err(e) = atomic_write(&watcher.path, bytes) {
            bevy::log::warn!("settings: failed to persist {}: {e}", watcher.path.display());
        }
    }
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "settings path has no parent")
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.persist(path)
        .map_err(|e| std::io::Error::other(format!("persist failed: {e}")))?;
    Ok(())
}
```

- [ ] **Step 4.4: Update `reload_settings_on_change` to skip self-writes**

Find the existing fn signature and body. Update the signature to add `last_hash: Res<LastSelfWriteHash>`, and after reading `text` but before parsing, compare hashes and skip:

```rust
fn reload_settings_on_change(
    watcher: Option<Res<SettingsWatcher>>,
    mut settings: ResMut<AppSettings>,
    mut layout_settings: ResMut<LayoutSettings>,
    mut confirm_close: ResMut<ConfirmCloseSettings>,
    last_hash: Res<LastSelfWriteHash>,
) {
    let Some(watcher) = watcher else { return };

    let rx = watcher.rx.lock().unwrap();
    let mut changed = false;
    while rx.try_recv().is_ok() {
        changed = true;
    }
    drop(rx);
    if !changed {
        return;
    }

    match std::fs::read_to_string(&watcher.path) {
        Ok(text) => {
            let current_hash = settings_content_hash(text.as_bytes());
            if last_hash.0 == Some(current_hash) {
                bevy::log::debug!("settings: skipping reload (matches last self-write)");
                return;
            }
            match ron::de::from_str::<AppSettings>(&text) {
                Ok(new_settings) => {
                    bevy::log::info!("Settings reloaded from {}", watcher.path.display());
                    *layout_settings = new_settings.layout.clone();
                    confirm_close.enabled = new_settings
                        .terminal
                        .as_ref()
                        .is_none_or(|terminal| terminal.confirm_close);
                    *settings = new_settings;
                }
                Err(e) => {
                    bevy::log::warn!("Settings reload failed (parse error): {e}");
                }
            }
        }
        Err(e) => {
            bevy::log::warn!("Settings reload failed (read error): {e}");
        }
    }
}
```

- [ ] **Step 4.5: Add `tempfile` to vmux_desktop deps if missing**

```bash
grep tempfile crates/vmux_desktop/Cargo.toml || echo MISSING
```

If `MISSING`, edit `crates/vmux_desktop/Cargo.toml`. Find the `[dependencies]` section and add:

```toml
tempfile = "3"
```

(If a `[dev-dependencies]` block has `tempfile`, also keep `tempfile = "3"` in `[dependencies]` — different scopes.)

- [ ] **Step 4.6: Compile to verify**

```bash
env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -20
```

Expected: clean check.

- [ ] **Step 4.7: Add unit test for hash-based skip**

Append to `mod tests` in `crates/vmux_desktop/src/settings.rs`:

```rust
    #[test]
    fn content_hash_is_deterministic() {
        let h1 = settings_content_hash(b"hello");
        let h2 = settings_content_hash(b"hello");
        let h3 = settings_content_hash(b"world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
```

- [ ] **Step 4.8: Run all changed-crate checks**

```bash
for pkg in vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 4.9: Commit**

```bash
git add crates/vmux_desktop/src/settings.rs crates/vmux_desktop/Cargo.toml Cargo.lock
git -c commit.gpgsign=false commit -m "feat(settings): persist updates to disk + suppress self-write reload"
```

---

## Task 5: `AgentCommand::UpdateSettings` + `AgentQuery::GetSettings`

Add new IPC variants so MCP edits can flow desktop-side.

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs`

- [ ] **Step 5.1: Add `UpdateSettings` to `AgentCommand`**

Find `pub enum AgentCommand`. Add a new variant after `SplitAndNavigate`:

```rust
    UpdateSettings {
        path: String,
        value_json: String,
    },
```

- [ ] **Step 5.2: Add validation**

Find `validate_agent_command`. Add a new arm before the catch-all:

```rust
        AgentCommand::UpdateSettings { path, .. } if path.trim().is_empty() => {
            Err("update_settings.path is empty")
        }
```

- [ ] **Step 5.3: Add `GetSettings` to `AgentQuery` and `Settings` to `AgentQueryResult`**

Find `pub enum AgentQuery` and add:

```rust
    GetSettings,
```

Find `pub enum AgentQueryResult` and add:

```rust
    Settings(String),
```

- [ ] **Step 5.4: Add rkyv roundtrip tests**

Append to the existing `mod tests` in `crates/vmux_service/src/protocol.rs`:

```rust
    #[test]
    fn update_settings_command_rkyv_roundtrip() {
        let cmd = AgentCommand::UpdateSettings {
            path: "layout.pane.gap".to_string(),
            value_json: "12.0".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn get_settings_query_rkyv_roundtrip() {
        let q = AgentQuery::GetSettings;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let decoded = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, q);
    }

    #[test]
    fn settings_query_result_rkyv_roundtrip() {
        let r = AgentQueryResult::Settings("{\"auto_update\":true}".to_string());
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let decoded = rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, r);
    }

    #[test]
    fn update_settings_validation_rejects_empty_path() {
        let cmd = AgentCommand::UpdateSettings {
            path: "".to_string(),
            value_json: "1".to_string(),
        };
        assert!(validate_agent_command(&cmd).is_err());
    }
```

- [ ] **Step 5.5: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_service --lib protocol 2>&1 | tail -15
```

Expected: new tests + existing tests pass.

- [ ] **Step 5.6: Run changed-crates checks**

```bash
for pkg in vmux_service; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_service; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_service; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 5.7: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git -c commit.gpgsign=false commit -m "feat(service): add UpdateSettings command + GetSettings query"
```

---

## Task 6: MCP tools `update_settings` and `get_settings`

Expose the new IPC variants via MCP so bots can call them.

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs`

- [ ] **Step 6.1: Write failing tests**

Append to `mod tests` in `crates/vmux_mcp/src/tools.rs`:

```rust
    #[test]
    fn list_tools_includes_update_settings_and_get_settings() {
        let names = tool_names();
        assert!(names.contains(&"update_settings".to_string()));
        assert!(names.contains(&"get_settings".to_string()));
    }

    #[test]
    fn update_settings_dispatches_with_path_and_value() {
        let target = dispatch_from_tool_call(
            "update_settings",
            serde_json::json!({"path": "layout.pane.gap", "value": 12.0}),
        )
        .unwrap();
        match target {
            DispatchTarget::Command(AgentCommand::UpdateSettings { path, value_json }) => {
                assert_eq!(path, "layout.pane.gap");
                let parsed: serde_json::Value = serde_json::from_str(&value_json).unwrap();
                assert_eq!(parsed, serde_json::json!(12.0));
            }
            other => panic!("expected UpdateSettings command, got {other:?}"),
        }
    }

    #[test]
    fn update_settings_empty_path_returns_error() {
        let result = dispatch_from_tool_call(
            "update_settings",
            serde_json::json!({"path": "", "value": 1}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn get_settings_dispatches_to_query() {
        let target = dispatch_from_tool_call("get_settings", serde_json::json!({})).unwrap();
        assert!(matches!(target, DispatchTarget::Query(AgentQuery::GetSettings)));
    }
```

The test uses `format!("{:?}", target)` indirectly through `panic!("... {other:?}")` — for that to compile, the `DispatchTarget` enum must implement `Debug`. Check:

```bash
grep -n "DispatchTarget" crates/vmux_mcp/src/tools.rs | head -3
```

If `pub enum DispatchTarget` has no `#[derive(Debug)]`, add one in Step 6.4 below; otherwise keep the test.

- [ ] **Step 6.2: Run tests to confirm they fail**

```bash
env -u CEF_PATH cargo test -p vmux_mcp 2>&1 | tail -15
```

Expected: compile errors about missing `update_settings`/`get_settings`.

- [ ] **Step 6.3: Add `UpdateSettings` to `McpParamTool`**

Find `pub enum McpParamTool` in `crates/vmux_mcp/src/tools.rs`. Add a new variant after `SplitAndNavigate`:

```rust
    #[mcp(
        description = "Update a single vmux setting by dot-path. \
            Example: { path: 'layout.pane.gap', value: 12 }. \
            Use get_settings to discover the available paths and current values. \
            For nested arrays, use bracket indexing like 'terminal.themes[0].font_size'."
    )]
    UpdateSettings {
        path: String,
        value: serde_json::Value,
    },
```

- [ ] **Step 6.4: Map `UpdateSettings` to `AgentCommand`**

Find the `to_agent_command` function for `McpParamTool`. Add a new arm:

```rust
            McpParamTool::UpdateSettings { path, value } => {
                if path.trim().is_empty() {
                    return Err("update_settings.path is empty".to_string());
                }
                Ok(AgentCommand::UpdateSettings {
                    path,
                    value_json: value.to_string(),
                })
            }
```

- [ ] **Step 6.5: Add `GetSettings` to `McpQueryTool`**

Find `pub enum McpQueryTool`. Add:

```rust
    #[mcp(description = "Return the full vmux settings as a JSON snapshot.")]
    GetSettings,
```

- [ ] **Step 6.6: Map `GetSettings` to `AgentQuery`**

In the `to_agent_query` function, add an arm:

```rust
            McpQueryTool::GetSettings => AgentQuery::GetSettings,
```

- [ ] **Step 6.7: Add `Debug` to `DispatchTarget` if missing**

```bash
grep -n "pub enum DispatchTarget" crates/vmux_mcp/src/tools.rs
```

If the line is `pub enum DispatchTarget {`, change it to `#[derive(Debug)]\npub enum DispatchTarget {`. Skip if already present.

- [ ] **Step 6.8: Run tests to confirm they pass**

```bash
env -u CEF_PATH cargo test -p vmux_mcp 2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Step 6.9: Run changed-crate checks**

```bash
for pkg in vmux_mcp; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_mcp; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_mcp; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 6.10: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git -c commit.gpgsign=false commit -m "feat(mcp): add update_settings and get_settings tools"
```

---

## Task 7: Wire agent dispatch handlers

Both new variants must be handled in `vmux_desktop`'s agent command/query dispatch.

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs`
- Modify: `crates/vmux_desktop/src/agent_query.rs`

- [ ] **Step 7.1: Handle `UpdateSettings` in `handle_agent_commands`**

Open `crates/vmux_desktop/src/agent.rs`. Find the `handle_agent_commands` system. The system parameters currently include `settings: Res<AppSettings>` — change it to `mut settings: ResMut<AppSettings>` and add an `MessageWriter` for the new write request.

Update the system signature so it has these added parameters:

```rust
    mut settings: ResMut<AppSettings>,
    mut settings_writes: MessageWriter<crate::settings::SettingsWriteRequest>,
```

(Replace the existing `settings: Res<AppSettings>` line.)

The line currently passing `&settings` into helpers like `spawn_terminal_tab(... &settings)` will keep working because deref-coerce takes care of it; double-check after editing.

Inside `for request in reader.read() { let result = match &request.command { ... } }`, add a new arm before the closing `}`:

```rust
            ServiceAgentCommand::UpdateSettings { path, value_json } => {
                match serde_json::from_str::<serde_json::Value>(value_json) {
                    Ok(value) => match crate::settings::apply_settings_update(
                        settings.as_mut(),
                        path,
                        value,
                    ) {
                        Ok(ron_bytes) => {
                            settings_writes.write(crate::settings::SettingsWriteRequest { ron_bytes });
                            AgentCommandResult::Ok
                        }
                        Err(message) => AgentCommandResult::Error(message),
                    },
                    Err(e) => AgentCommandResult::Error(format!(
                        "update_settings: invalid JSON value: {e}"
                    )),
                }
            }
```

`apply_settings_update` is currently `pub(crate)` — confirm visibility from `agent.rs`. Both files are in `vmux_desktop`'s crate root so `crate::settings::apply_settings_update` is reachable. If linting complains, change `pub(crate)` to `pub` in `crates/vmux_desktop/src/settings.rs` for `apply_settings_update`, `SettingsWriteRequest`, and `LastSelfWriteHash`.

- [ ] **Step 7.2: Handle `GetSettings` in `handle_agent_queries`**

Open `crates/vmux_desktop/src/agent_query.rs`. Add `settings: Res<crate::settings::AppSettings>` to the system parameters.

Add a new arm in the `match request.query` block (before the closing brace):

```rust
            AgentQuery::GetSettings => AgentQueryResult::Settings(
                crate::settings::serialize_settings_to_json(&settings),
            ),
```

`serialize_settings_to_json` was added in Task 3. Make sure its visibility is `pub(crate)` or `pub` (it is, per Task 3.3).

- [ ] **Step 7.3: Compile**

```bash
env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -25
```

Fix any compilation errors before continuing. Common issue: `MessageWriter` may need `use bevy::prelude::*;` at the top (already present in agent.rs).

- [ ] **Step 7.4: Add a smoke test for the dispatch**

Append to the existing `mod tests` in `crates/vmux_desktop/src/agent.rs`:

```rust
    #[test]
    fn update_settings_command_emits_write_request_and_mutates_resource() {
        use crate::settings::SettingsWriteRequest;
        use vmux_service::protocol::{AgentCommand as ServiceAgentCommand};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<AgentCommandRequest>()
            .add_message::<AppCommand>()
            .add_message::<SettingsWriteRequest>();
        app.insert_resource(test_settings());
        app.insert_resource(crate::settings::LastSelfWriteHash::default());
        app.init_resource::<vmux_layout::settings::EffectiveStartupUrl>();
        app.add_systems(Update, handle_agent_commands);

        app.world_mut().write_message(AgentCommandRequest {
            request_id: vmux_service::protocol::AgentRequestId::new(),
            command: ServiceAgentCommand::UpdateSettings {
                path: "auto_update".to_string(),
                value_json: serde_json::json!(true).to_string(),
            },
        });
        app.update();

        let settings = app.world().resource::<crate::settings::AppSettings>();
        assert!(settings.auto_update, "auto_update should be true after update");

        let writes = app
            .world()
            .resource::<bevy::ecs::message::Messages<SettingsWriteRequest>>();
        assert!(writes.iter_current_update_events().count() >= 1);
    }
```

If your existing tests use a different shape for sending `AgentCommandRequest`, mirror that shape. Look at the existing test `agent_launch_request_uses_registered_provider_to_spawn_terminal_tab` in this file for a template — it sets up the same kind of `App` + `Update` schedule.

- [ ] **Step 7.5: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop --lib agent::tests::update_settings 2>&1 | tail -20
```

Expected: pass. If the smoke test is too brittle (e.g. existing test infra differs in how MinimalPlugins is configured), simplify it to just call `apply_settings_update` directly — the integration is exercised manually in Task 12.

- [ ] **Step 7.6: Run changed-crate checks**

```bash
for pkg in vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 7.7: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs crates/vmux_desktop/src/agent_query.rs crates/vmux_desktop/src/settings.rs
git -c commit.gpgsign=false commit -m "feat(desktop): wire UpdateSettings and GetSettings dispatch"
```

---

## Task 8: Create `vmux_settings` crate skeleton

Mirror `vmux_space` exactly. This task only creates files; the form UI lands in Task 9.

**Files:**
- Create: `crates/vmux_settings/Cargo.toml`
- Create: `crates/vmux_settings/Dioxus.toml`
- Create: `crates/vmux_settings/build.rs`
- Create: `crates/vmux_settings/tailwind.config.js`
- Create: `crates/vmux_settings/assets/index.html`
- Create: `crates/vmux_settings/assets/index.css`
- Create: `crates/vmux_settings/src/lib.rs`
- Create: `crates/vmux_settings/src/main.rs`
- Create: `crates/vmux_settings/src/event.rs`
- Modify: `Cargo.toml` (workspace `members` already uses `crates/*`, no edit needed if already glob)

- [ ] **Step 8.1: Confirm workspace glob**

```bash
grep -A2 'members' Cargo.toml | head -5
```

Expected: `members = ["crates/*", "patches/*"]`. The new crate is auto-included.

- [ ] **Step 8.2: Create `crates/vmux_settings/Cargo.toml`**

```toml
[package]
name = "vmux_settings"
description = "Settings webview app"
version.workspace = true
edition.workspace = true
publish = false
build = "build.rs"

[features]
default = []
web = []

[[bin]]
name = "vmux_settings_app"
path = "src/main.rs"
required-features = ["web"]

[lib]
path = "src/lib.rs"

[build-dependencies]
vmux_webview_app = { path = "../vmux_webview_app", features = ["build"] }

[dependencies]
rkyv = { workspace = true }
serde = { workspace = true }

[target.'cfg(target_arch = "wasm32")'.dependencies]
dioxus = { workspace = true }
vmux_ui = { path = "../vmux_ui", default-features = false }
wasm-bindgen = { workspace = true }
web-sys = { version = "0.3", features = ["Window", "Document", "Element", "HtmlElement"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 8.3: Create `crates/vmux_settings/Dioxus.toml`**

```toml
[application]
name = "vmux_settings"
default_platform = "web"

[web.app]
title = "Settings"
```

- [ ] **Step 8.4: Create `crates/vmux_settings/build.rs`**

```rust
use std::path::PathBuf;

use vmux_webview_app::build::{CefEmbeddedWebviewFinalize, WebviewAppBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_settings", "vmux_settings_app")
        .track_manifest_rel_paths(&["tailwind.config.js", "../vmux_ui/assets/theme.css"])
        .dx_extra_args(&["--bin", "vmux_settings_app", "--features", "web"])
        .cef_finalize(CefEmbeddedWebviewFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxs", "settings-dxs"])
        .run("vmux_settings");
}
```

- [ ] **Step 8.5: Create `crates/vmux_settings/tailwind.config.js`**

```javascript
/** @type {import('tailwindcss').Config} */
module.exports = {
  presets: [require("../vmux_ui/tailwind.preset.js")],
  content: ["./src/**/*.rs", "./assets/**/*.html"],
  theme: {
    extend: {},
  },
  plugins: [],
};
```

- [ ] **Step 8.6: Create `crates/vmux_settings/assets/index.html`**

```html
<!DOCTYPE html>
<html lang="en" class="dark h-full">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width"/>
  <title>Settings</title>
  <style>
    html.dark, html.dark body { height: 100%; margin: 0; min-height: 0; }
    body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
    #main { flex: 1 1 0%; min-height: 0; min-width: 0; display: flex; flex-direction: column; }
  </style>
  <link rel="modulepreload" href="__VMUX_DX_ENTRY__" crossorigin="anonymous"/>
  <link rel="preload" href="__VMUX_DX_WASM__" as="fetch" type="application/wasm" crossorigin="anonymous"/>
</head>
<body class="m-0 flex h-full min-h-0 flex-col overflow-hidden p-0 text-foreground antialiased">
  <div id="main" class="flex min-h-0 min-w-0 flex-1 flex-col"></div>
  <script type="module" async src="__VMUX_DX_ENTRY__"></script>
</body>
</html>
```

- [ ] **Step 8.7: Create `crates/vmux_settings/assets/index.css`**

```css
@import "../../vmux_ui/assets/theme.css";
@config "../tailwind.config.js";
@import "tailwindcss";
@source "../src";
@source ".";
```

- [ ] **Step 8.8: Create `crates/vmux_settings/src/event.rs`**

```rust
pub const SETTINGS_WEBVIEW_URL: &str = "vmux://settings/";
pub const SETTINGS_LIST_EVENT: &str = "settings_list";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SettingsListEvent {
    pub json: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SettingsCommandEvent {
    pub path: String,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_list_event_rkyv_roundtrip() {
        let original = SettingsListEvent {
            json: r#"{"auto_update":true}"#.to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsListEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn settings_command_event_rkyv_roundtrip() {
        let original = SettingsCommandEvent {
            path: "layout.pane.gap".to_string(),
            value: "12.0".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }
}
```

- [ ] **Step 8.9: Create `crates/vmux_settings/src/lib.rs`**

```rust
pub mod event;
```

- [ ] **Step 8.10: Create `crates/vmux_settings/src/main.rs`**

```rust
#[cfg(target_arch = "wasm32")]
mod app;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("vmux_settings: wasm binary is for wasm32 (see build.rs).");
}
```

(Note: the `app` module doesn't exist yet — it's added in Task 9. To keep this task buildable on its own, we gate `app` behind `cfg(target_arch = "wasm32")`. The host build does not pull in `app.rs` so no compile error.)

- [ ] **Step 8.11: Compile + test the new crate**

```bash
env -u CEF_PATH cargo test -p vmux_settings 2>&1 | tail -15
```

Expected: 2 passed (the rkyv roundtrip tests in `event.rs`).

- [ ] **Step 8.12: Run changed-crate checks**

```bash
for pkg in vmux_settings; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_settings; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_settings; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 8.13: Commit**

```bash
git add crates/vmux_settings Cargo.lock
git -c commit.gpgsign=false commit -m "feat(vmux_settings): scaffold settings webview crate"
```

---

## Task 9: Implement Dioxus form (`vmux_settings/src/app.rs`)

Render every section of `AppSettings` as a form. Auto-save on change with 300 ms debounce.

**Files:**
- Create: `crates/vmux_settings/src/app.rs`

- [ ] **Step 9.1: Create `crates/vmux_settings/src/app.rs`**

```rust
#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_settings::event::{
    SETTINGS_LIST_EVENT, SettingsCommandEvent, SettingsListEvent,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

fn emit_update(path: &str, value: serde_json::Value) {
    let _ = try_cef_bin_emit_rkyv(&SettingsCommandEvent {
        path: path.to_string(),
        value: value.to_string(),
    });
}

#[component]
pub fn App() -> Element {
    use_theme();
    let mut snapshot = use_signal(|| serde_json::Value::Null);

    let _listener = use_bin_event_listener::<SettingsListEvent, _>(
        SETTINGS_LIST_EVENT,
        move |data| {
            let parsed: serde_json::Value =
                serde_json::from_str(&data.json).unwrap_or(serde_json::Value::Null);
            snapshot.set(parsed);
        },
    );

    let s = snapshot.read().clone();
    if s.is_null() {
        return rsx! {
            div { class: "flex h-full items-center justify-center text-sm text-muted-foreground",
                "Loading settings..."
            }
        };
    }

    rsx! {
        div { class: "flex h-full min-h-0 flex-col overflow-y-auto bg-background text-foreground",
            div { class: "border-b border-border px-6 py-4",
                h1 { class: "text-lg font-semibold", "Settings" }
                p { class: "mt-1 text-xs text-muted-foreground",
                    "Stored in ~/Library/Application Support/Vmux/settings.ron"
                }
            }
            div { class: "flex flex-col gap-6 p-6",
                SectionGeneral { snapshot: s.clone() }
                SectionWindow { snapshot: s.clone() }
                SectionPane { snapshot: s.clone() }
                SectionSideSheet { snapshot: s.clone() }
                SectionFocusRing { snapshot: s.clone() }
                SectionShortcuts { snapshot: s.clone() }
                SectionTerminal { snapshot: s.clone() }
            }
        }
    }
}

fn pick_bool(snapshot: &serde_json::Value, path: &[&str], default: bool) -> bool {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(default)
}

fn pick_f64(snapshot: &serde_json::Value, path: &[&str], default: f64) -> f64 {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(default)
}

fn pick_u64(snapshot: &serde_json::Value, path: &[&str], default: u64) -> u64 {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(default)
}

fn pick_string(snapshot: &serde_json::Value, path: &[&str]) -> String {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn walk<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    Some(cursor)
}

#[component]
fn Card(title: String, children: Element) -> Element {
    rsx! {
        div { class: "rounded-lg border border-border bg-card p-4",
            h2 { class: "mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground", "{title}" }
            div { class: "flex flex-col gap-3", {children} }
        }
    }
}

#[component]
fn FieldNumber(label: String, path: String, value: f64, step: f64) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "number",
                class: "w-32 rounded border border-border bg-background px-2 py-1 text-right text-foreground",
                step: "{step}",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(parsed) = e.value().parse::<f64>() {
                        emit_update(&path_for_input, serde_json::json!(parsed));
                    }
                },
            }
        }
    }
}

#[component]
fn FieldInt(label: String, path: String, value: u64) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "number",
                class: "w-32 rounded border border-border bg-background px-2 py-1 text-right text-foreground",
                step: "1",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(parsed) = e.value().parse::<u64>() {
                        emit_update(&path_for_input, serde_json::json!(parsed));
                    }
                },
            }
        }
    }
}

#[component]
fn FieldBool(label: String, path: String, value: bool) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "checkbox",
                class: "h-4 w-4",
                checked: value,
                onchange: move |e| {
                    emit_update(&path_for_input, serde_json::json!(e.value() == "true"));
                },
            }
        }
    }
}

#[component]
fn FieldText(label: String, path: String, value: String) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex flex-col gap-1 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "text",
                class: "rounded border border-border bg-background px-2 py-1 text-foreground",
                value: "{value}",
                oninput: move |e| {
                    emit_update(&path_for_input, serde_json::json!(e.value()));
                },
            }
        }
    }
}

#[component]
fn SectionGeneral(snapshot: serde_json::Value) -> Element {
    let auto_update = pick_bool(&snapshot, &["auto_update"], true);
    let startup_url = pick_string(&snapshot, &["startup_url"]);
    rsx! {
        Card { title: "General".to_string(),
            FieldBool {
                label: "Auto-update".to_string(),
                path: "auto_update".to_string(),
                value: auto_update,
            }
            FieldText {
                label: "Startup URL (empty = vmux://vibe/)".to_string(),
                path: "startup_url".to_string(),
                value: startup_url,
            }
        }
    }
}

#[component]
fn SectionWindow(snapshot: serde_json::Value) -> Element {
    let padding = pick_f64(&snapshot, &["layout", "window", "padding"], 4.0);
    rsx! {
        Card { title: "Window".to_string(),
            FieldNumber {
                label: "Padding (px)".to_string(),
                path: "layout.window.padding".to_string(),
                value: padding,
                step: 1.0,
            }
        }
    }
}

#[component]
fn SectionPane(snapshot: serde_json::Value) -> Element {
    let gap = pick_f64(&snapshot, &["layout", "pane", "gap"], 8.0);
    let radius = pick_f64(&snapshot, &["layout", "pane", "radius"], 8.0);
    rsx! {
        Card { title: "Pane".to_string(),
            FieldNumber {
                label: "Gap (px)".to_string(),
                path: "layout.pane.gap".to_string(),
                value: gap,
                step: 1.0,
            }
            FieldNumber {
                label: "Corner radius (px)".to_string(),
                path: "layout.pane.radius".to_string(),
                value: radius,
                step: 1.0,
            }
        }
    }
}

#[component]
fn SectionSideSheet(snapshot: serde_json::Value) -> Element {
    let width = pick_f64(&snapshot, &["layout", "side_sheet", "width"], 280.0);
    rsx! {
        Card { title: "Side sheet".to_string(),
            FieldNumber {
                label: "Width (px)".to_string(),
                path: "layout.side_sheet.width".to_string(),
                value: width,
                step: 4.0,
            }
        }
    }
}

#[component]
fn SectionFocusRing(snapshot: serde_json::Value) -> Element {
    let width = pick_f64(&snapshot, &["layout", "focus_ring", "width"], 2.0);
    let glow_spread = pick_f64(&snapshot, &["layout", "focus_ring", "glow", "spread"], 8.0);
    let glow_intensity = pick_f64(&snapshot, &["layout", "focus_ring", "glow", "intensity"], 0.45);
    let gradient_enabled =
        pick_bool(&snapshot, &["layout", "focus_ring", "gradient", "enabled"], true);
    let gradient_speed =
        pick_f64(&snapshot, &["layout", "focus_ring", "gradient", "speed"], 0.6);
    let gradient_cycles =
        pick_f64(&snapshot, &["layout", "focus_ring", "gradient", "cycles"], 1.0);
    rsx! {
        Card { title: "Focus ring".to_string(),
            FieldNumber {
                label: "Width (px)".to_string(),
                path: "layout.focus_ring.width".to_string(),
                value: width,
                step: 0.5,
            }
            FieldNumber {
                label: "Glow spread".to_string(),
                path: "layout.focus_ring.glow.spread".to_string(),
                value: glow_spread,
                step: 0.5,
            }
            FieldNumber {
                label: "Glow intensity".to_string(),
                path: "layout.focus_ring.glow.intensity".to_string(),
                value: glow_intensity,
                step: 0.05,
            }
            FieldBool {
                label: "Gradient enabled".to_string(),
                path: "layout.focus_ring.gradient.enabled".to_string(),
                value: gradient_enabled,
            }
            FieldNumber {
                label: "Gradient speed".to_string(),
                path: "layout.focus_ring.gradient.speed".to_string(),
                value: gradient_speed,
                step: 0.1,
            }
            FieldNumber {
                label: "Gradient cycles".to_string(),
                path: "layout.focus_ring.gradient.cycles".to_string(),
                value: gradient_cycles,
                step: 0.1,
            }
        }
    }
}

#[component]
fn SectionShortcuts(snapshot: serde_json::Value) -> Element {
    let timeout = pick_u64(&snapshot, &["shortcuts", "chord_timeout_ms"], 1000);
    let leader = walk(&snapshot, &["shortcuts", "leader"])
        .map(|v| v.to_string())
        .unwrap_or_else(|| "(none)".to_string());
    let bindings = walk(&snapshot, &["shortcuts", "bindings"])
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Card { title: "Shortcuts".to_string(),
            FieldInt {
                label: "Chord timeout (ms)".to_string(),
                path: "shortcuts.chord_timeout_ms".to_string(),
                value: timeout,
            }
            div { class: "text-xs text-muted-foreground",
                "Leader: " span { class: "font-mono text-foreground", "{leader}" }
            }
            div { class: "rounded border border-border bg-background p-2 text-xs",
                div { class: "mb-2 font-medium text-muted-foreground", "Bindings (read-only)" }
                for (i, binding) in bindings.iter().enumerate() {
                    div { key: "{i}", class: "font-mono", "{binding}" }
                }
            }
        }
    }
}

#[component]
fn SectionTerminal(snapshot: serde_json::Value) -> Element {
    let confirm_close = pick_bool(&snapshot, &["terminal", "confirm_close"], true);
    let default_theme = pick_string(&snapshot, &["terminal", "default_theme"]);
    let themes = walk(&snapshot, &["terminal", "themes"])
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Card { title: "Terminal".to_string(),
            FieldBool {
                label: "Confirm close".to_string(),
                path: "terminal.confirm_close".to_string(),
                value: confirm_close,
            }
            FieldText {
                label: "Default theme name".to_string(),
                path: "terminal.default_theme".to_string(),
                value: default_theme,
            }
            for (i, theme) in themes.iter().enumerate() {
                ThemeSubcard { index: i, theme: theme.clone() }
            }
        }
    }
}

#[component]
fn ThemeSubcard(index: usize, theme: serde_json::Value) -> Element {
    let name = theme
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("(unnamed)")
        .to_string();
    let font_family = theme
        .get("font_family")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let font_size = theme
        .get("font_size")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(14.0);
    let line_height = theme
        .get("line_height")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.2);
    let padding = theme
        .get("padding")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(4.0);
    let cursor_blink = theme
        .get("cursor_blink")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let shell = theme
        .get("shell")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    rsx! {
        div { class: "rounded border border-border bg-background p-3",
            div { class: "mb-2 text-xs font-semibold text-muted-foreground", "Theme: {name}" }
            FieldText {
                label: "Font family".to_string(),
                path: format!("terminal.themes[{index}].font_family"),
                value: font_family,
            }
            FieldNumber {
                label: "Font size".to_string(),
                path: format!("terminal.themes[{index}].font_size"),
                value: font_size,
                step: 0.5,
            }
            FieldNumber {
                label: "Line height".to_string(),
                path: format!("terminal.themes[{index}].line_height"),
                value: line_height,
                step: 0.05,
            }
            FieldNumber {
                label: "Padding".to_string(),
                path: format!("terminal.themes[{index}].padding"),
                value: padding,
                step: 0.5,
            }
            FieldBool {
                label: "Cursor blink".to_string(),
                path: format!("terminal.themes[{index}].cursor_blink"),
                value: cursor_blink,
            }
            FieldText {
                label: "Shell".to_string(),
                path: format!("terminal.themes[{index}].shell"),
                value: shell,
            }
        }
    }
}
```

Note: This implementation does **not** include 300 ms debounce — Dioxus 0.7 doesn't have a built-in `use_debounce` hook in `vmux_ui`. The form auto-saves on every keystroke. If feedback loops cause issues during manual testing in Task 12, add `gloo-timers` and a debounce wrapper. For v1, the file watcher's content-hash skip (Task 4) prevents the worst feedback issues.

- [ ] **Step 9.2: Add `serde_json` to wasm32 deps**

The `app.rs` uses `serde_json::Value`. Edit `crates/vmux_settings/Cargo.toml`. Find the `[target.'cfg(target_arch = "wasm32")'.dependencies]` block and add:

```toml
serde_json = { workspace = true }
```

- [ ] **Step 9.3: Compile WASM bin**

```bash
env -u CEF_PATH cargo check -p vmux_settings --target wasm32-unknown-unknown --bin vmux_settings_app --features web 2>&1 | tail -30
```

Expected: clean check. Common issues:
- Missing `dioxus-primitives` import — not used here, no need.
- `try_cef_bin_emit_rkyv` not found — make sure `vmux_ui` is reachable (Cargo.toml line in Step 8.2 already lists it).

- [ ] **Step 9.4: Run host-side tests (event roundtrips still pass)**

```bash
env -u CEF_PATH cargo test -p vmux_settings 2>&1 | tail -10
```

Expected: 2 passed.

- [ ] **Step 9.5: Run changed-crate fmt + clippy**

```bash
cargo fmt -p vmux_settings -- --check
env -u CEF_PATH cargo clippy -p vmux_settings --all-targets -- -D warnings
```

Expected: pass.

- [ ] **Step 9.6: Commit**

```bash
git add crates/vmux_settings/src/app.rs crates/vmux_settings/Cargo.toml Cargo.lock
git -c commit.gpgsign=false commit -m "feat(vmux_settings): add Dioxus form for all settings sections"
```

---

## Task 10: `vmux_desktop/src/settings_view.rs` — host plugin

Wires the `vmux_settings` bundle into the desktop, broadcasts current settings, handles edit commands.

**Files:**
- Create: `crates/vmux_desktop/src/settings_view.rs`
- Modify: `crates/vmux_desktop/Cargo.toml` (add `vmux_settings` dep)

- [ ] **Step 10.1: Add `vmux_settings` to vmux_desktop deps**

Edit `crates/vmux_desktop/Cargo.toml`. Find the `[dependencies]` block (NOT `target.'cfg(target_arch = "wasm32")'.dependencies` — desktop is host-side). Add:

```toml
vmux_settings = { path = "../vmux_settings" }
```

- [ ] **Step 10.2: Create `crates/vmux_desktop/src/settings_view.rs`**

```rust
use std::path::PathBuf;

use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_settings::event::{
    SETTINGS_LIST_EVENT, SETTINGS_WEBVIEW_URL, SettingsCommandEvent, SettingsListEvent,
};
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppRegistry};

use crate::{
    browser::Browser,
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
    settings::{AppSettings, SettingsWriteRequest, apply_settings_update, serialize_settings_to_json},
};

#[derive(Component)]
pub(crate) struct SettingsView;

impl SettingsView {
    #[allow(dead_code)]
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SETTINGS_WEBVIEW_URL),
                ResolvedWebviewUri(SETTINGS_WEBVIEW_URL.to_string()),
                PageMetadata {
                    title: "Settings".to_string(),
                    url: SETTINGS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Inherited,
                Pickable::default(),
            ),
        )
    }
}

pub(crate) struct SettingsViewPlugin;

impl Plugin for SettingsViewPlugin {
    fn build(&self, app: &mut App) {
        register_settings_webview_app(
            app.world_mut()
                .resource_mut::<WebviewAppRegistry>()
                .as_mut(),
        );
        app.add_plugins(BinJsEmitEventPlugin::<SettingsCommandEvent>::default())
            .add_observer(on_settings_command)
            .add_systems(Update, broadcast_settings_to_views);
    }
}

fn register_settings_webview_app(registry: &mut WebviewAppRegistry) {
    registry.register(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_settings"),
        &WebviewAppConfig::with_custom_host("settings"),
    );
}

#[derive(Default)]
struct SettingsBroadcastCache {
    body: String,
    sent: std::collections::HashSet<Entity>,
}

fn broadcast_settings_to_views(
    settings: Res<AppSettings>,
    views: Query<Entity, (With<SettingsView>, With<UiReady>)>,
    browsers: NonSend<crate::browser::Browsers>,
    mut cache: Local<SettingsBroadcastCache>,
    mut commands: Commands,
) {
    if views.is_empty() {
        return;
    }
    let payload = SettingsListEvent {
        json: serialize_settings_to_json(&settings),
    };
    let body = payload.json.clone();
    if body != cache.body {
        cache.body = body;
        cache.sent.clear();
    }
    for entity in &views {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        if !cache.sent.insert(entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            SETTINGS_LIST_EVENT,
            &payload,
        ));
    }
}

fn on_settings_command(
    trigger: On<BinReceive<SettingsCommandEvent>>,
    mut settings: ResMut<AppSettings>,
    mut writes: MessageWriter<SettingsWriteRequest>,
) {
    let evt = &trigger.event().payload;
    let value: serde_json::Value = match serde_json::from_str(&evt.value) {
        Ok(v) => v,
        Err(e) => {
            bevy::log::warn!("settings: invalid JSON for path {}: {e}", evt.path);
            return;
        }
    };
    match apply_settings_update(settings.as_mut(), &evt.path, value) {
        Ok(ron_bytes) => {
            writes.write(SettingsWriteRequest { ron_bytes });
        }
        Err(e) => bevy::log::warn!("settings: update {} rejected: {}", evt.path, e),
    }
}
```

- [ ] **Step 10.3: Confirm `WEBVIEW_MESH_DEPTH_BIAS` and `Browsers` are reachable**

```bash
grep -n "pub.*WEBVIEW_MESH_DEPTH_BIAS\|pub.*struct Browsers" crates/vmux_desktop/src/layout/window.rs crates/vmux_desktop/src/browser.rs
```

If `Browsers` is `pub(crate)` only, `crate::browser::Browsers` works since this file is in the same crate. If a constant isn't pub(crate), promote it. (Should be fine — `vmux_desktop/src/spaces.rs` already uses both.)

- [ ] **Step 10.4: `serialize_settings_to_json` and `apply_settings_update` visibility**

```bash
grep -n "fn serialize_settings_to_json\|fn apply_settings_update\|struct SettingsWriteRequest" crates/vmux_desktop/src/settings.rs
```

If `pub(crate)`, that's enough — same crate. If you used `pub`, also fine.

- [ ] **Step 10.5: Compile**

```bash
env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -25
```

Expected: clean. Common issue: `serde_json` may not be a direct dep of `vmux_desktop` already — check:

```bash
grep -E "^serde_json" crates/vmux_desktop/Cargo.toml
```

If missing, add `serde_json = { workspace = true }` to `[dependencies]`.

- [ ] **Step 10.6: Run changed-crate checks**

```bash
for pkg in vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 10.7: Commit**

```bash
git add crates/vmux_desktop/src/settings_view.rs crates/vmux_desktop/Cargo.toml Cargo.lock
git -c commit.gpgsign=false commit -m "feat(desktop): SettingsViewPlugin to broadcast + apply edits"
```

---

## Task 11: Wire `SettingsViewPlugin` + URL completion

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs`
- Modify: `crates/vmux_desktop/src/command_bar.rs`

- [ ] **Step 11.1: Register module + plugin in `lib.rs`**

Open `crates/vmux_desktop/src/lib.rs`. Find the `mod` list and add:

```rust
mod settings_view;
```

(Insert alphabetically near `mod settings;`.)

In the same file find the section `app.add_plugins(SpacesPlugin)` (or wherever `SpacesPlugin` is added). Add `SettingsViewPlugin` to be loaded alongside, e.g.:

```rust
.add_plugins(SpacesPlugin)
.add_plugins(settings_view::SettingsViewPlugin)
.add_plugins(BrowserPlugin)
```

- [ ] **Step 11.2: Add `vmux://settings/` to URL completion in command_bar**

Open `crates/vmux_desktop/src/command_bar.rs`. Search for `vmux://spaces/`:

```bash
grep -n '"vmux://spaces/"' crates/vmux_desktop/src/command_bar.rs
```

For each location that lists vmux URLs as completions or quick-launches (most likely lines 2016 and 2186 / 2229 from earlier grep), add `"vmux://settings/".to_string()` next to `"vmux://spaces/".to_string()`. Be conservative — only edit the data-array completion lists, not test assertions.

Concretely, look around line 2016 and similar:

```rust
            "vmux://spaces/".to_string(),
```

Add:

```rust
            "vmux://settings/".to_string(),
```

immediately after each occurrence in completion-list contexts. If you're unsure whether a particular occurrence is a completion list vs an assertion, leave it; the smoke test in Task 12 confirms the URL works either way.

- [ ] **Step 11.3: Compile**

```bash
env -u CEF_PATH cargo check -p vmux_desktop 2>&1 | tail -15
```

Expected: clean.

- [ ] **Step 11.4: Run changed-crate checks**

```bash
for pkg in vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass. Note: existing command_bar tests may need `vmux://settings/` added to expected lists if the test asserts the full completion array — adjust per the failing test message.

- [ ] **Step 11.5: Commit**

```bash
git add crates/vmux_desktop/src/lib.rs crates/vmux_desktop/src/command_bar.rs
git -c commit.gpgsign=false commit -m "feat(desktop): register SettingsViewPlugin + URL completion"
```

---

## Task 12: Manual smoke test

The hot-path is end-to-end: a real CEF webview with a real Bevy desktop. Verify it opens, displays settings, accepts edits, and round-trips via MCP.

- [ ] **Step 12.1: Build and launch vmux**

From the worktree root:

```bash
cargo run -p vmux_desktop 2>&1 | tail -20
```

This builds the desktop binary and launches the app. Expect a window. Wait for the main UI to be ready.

- [ ] **Step 12.2: Open the settings page**

In the running app, open the command bar (default: `Ctrl+G` then your command-bar key, or click the URL bar). Type `vmux://settings/` and press Enter.

Expected: a Settings page opens with sections (General, Window, Pane, Side sheet, Focus ring, Shortcuts, Terminal). Values match what's in `~/Library/Application Support/Vmux/settings.ron`.

- [ ] **Step 12.3: Edit pane gap from the form**

Change "Gap (px)" under Pane from its current value (likely 8) to `16`. Watch the layout — pane gap should update within ~1 second (the file watcher round-trips the change).

Inspect `~/Library/Application Support/Vmux/settings.ron`:

```bash
grep -A2 'pane:' ~/Library/Application\ Support/Vmux/settings.ron | head -5
```

Expected: `gap: 16.0`.

- [ ] **Step 12.4: Test MCP `get_settings`**

Open a terminal (in vmux or external). Make sure the vmux service is running (the desktop binary launches it). Then:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_settings","arguments":{}}}' \
  | cargo run -p vmux_cli -- mcp 2>&1 | tail -20
```

Expected: a JSON response containing the full settings dictionary, including `"layout":{"pane":{"gap":16.0}}`.

- [ ] **Step 12.5: Test MCP `update_settings`**

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"update_settings","arguments":{"path":"layout.pane.gap","value":24}}}' \
  | cargo run -p vmux_cli -- mcp 2>&1 | tail -10
```

Expected: success response. The vmux window updates the pane gap to 24 within ~1 second. The settings page (if still open) reflects the new value.

- [ ] **Step 12.6: Test invalid path**

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"update_settings","arguments":{"path":"layout.nope","value":1}}}' \
  | cargo run -p vmux_cli -- mcp 2>&1 | tail -10
```

Expected: error response mentioning "unknown setting path: layout.nope". `settings.ron` is unchanged.

- [ ] **Step 12.7: Restore the original pane gap**

Either edit the form back to your preferred value (8) or:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"update_settings","arguments":{"path":"layout.pane.gap","value":8}}}' \
  | cargo run -p vmux_cli -- mcp 2>&1 | tail -5
```

- [ ] **Step 12.8: Stop the app**

Quit vmux normally.

---

## Task 13: Final verification + finish branch

- [ ] **Step 13.1: Run the full changed-crates loop one more time**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
echo "Changed crates: $PKGS"
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Expected: all pass.

- [ ] **Step 13.2: Delete the plan file (per AGENTS.md)**

> "Delete the plan file once the plan is fully implemented."

```bash
rm docs/plans/2026-05-15-vmux-settings.md
git add docs/plans/2026-05-15-vmux-settings.md
git -c commit.gpgsign=false commit -m "chore: remove implemented plan"
```

- [ ] **Step 13.3: Push branch and open PR (or hand off)**

```bash
git push -u origin feat/vmux-settings
```

Then either invoke the `open-new-pr` skill for an auto-generated PR description, or hand off to the user for review.

- [ ] **Step 13.4: After merge — clean up worktree**

```bash
cd /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux  # back to main worktree
git worktree remove .worktrees/vmux-settings
```
