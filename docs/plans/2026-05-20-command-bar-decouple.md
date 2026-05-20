# Command Bar Decouple Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the command-bar Dioxus UI and the 2725-line handler out of `vmux_desktop` into `vmux_layout`, and decouple the handler from `vmux_agent` / `vmux_setting` / `vmux_space` / `vmux_terminal` by introducing snapshot-resource indirection in `vmux_command`.

**Architecture:**
- `vmux_command` (DAG bottom) owns 4 new snapshot resource types (`CommandBarAgentsSnapshot`, `CommandBarSettingsSnapshot`, `CommandBarSpacesSnapshot`, `CommandBarTerminalsSnapshot`). Each owning domain crate runs ONE updater system that writes its slice when source data changes.
- The command-bar Dioxus app moves from `vmux_command` (loses bin target) into `vmux_layout` as a sibling bin (`vmux_command_bar_app`) using companion-file modules under `command_bar/`.
- The handler (`handle_open_command_bar` + observers + helpers) is rewritten to read snapshots instead of cross-crate types, then physically moves into `vmux_layout/src/command_bar.rs` and is registered by `LayoutPlugin`. `vmux_desktop` drops `command_bar.rs` and `CommandBarInputPlugin` entirely.
- Writes that currently call `vmux_agent::AgentLaunchRequested` and `new_terminal_bundle*` are converted to existing `AppCommand::*` vocab variants (adding `AppCommand::Agent(AgentCommand::Launch{..})` and `TerminalCommand::SpawnAt{..}` if missing). Responders in the owning domain crates consume these.

**Tech Stack:** Rust 2024, Bevy 0.18, Dioxus (wasm32), CEF, single workspace.

**DAG after refactor (unchanged shape, command-bar logic relocated):**
```
vmux_desktop                       (drops command_bar.rs)
   ↓
vmux_agent → vmux_terminal → vmux_space → vmux_setting → vmux_layout
                                                          ↑
                                                  command-bar UI + handler live here
   ↓
vmux_command (snapshot resources + AppCommand vocab + event wire types)
```

---

## Phase A — Relocate Command-Bar UI into `vmux_layout`

Mechanical move. No behavior change. After Phase A: command-bar Dioxus bin is built by `vmux_layout`, `vmux_command` is host-only.

### Task A.1 — Add wasm/web-sys features and second bin target to `vmux_layout/Cargo.toml`

**Files:**
- Modify: `crates/vmux_layout/Cargo.toml`

- [ ] **Step 1: Read current Cargo.toml**

Run: `read_file crates/vmux_layout/Cargo.toml`

- [ ] **Step 2: Add second bin target after the existing `[[bin]]` block**

Add this block immediately after the existing `[[bin]] name = "vmux_layout_app"` entry:

```toml
[[bin]]
name = "vmux_command_bar_app"
path = "src/command_bar_app.rs"
required-features = ["web"]
```

- [ ] **Step 3: Extend wasm32 web-sys features**

Locate the existing `[target.'cfg(target_arch = "wasm32")'.dependencies] web-sys = { ..., features = [...] }` array. Union it with the features `vmux_command` currently requires (verify against `crates/vmux_command/Cargo.toml`):

```toml
"Window",
"Document",
"Element",
"HtmlElement",
"HtmlInputElement",
"EventTarget",
"Event",
"EventInit",
"KeyboardEvent",
"KeyboardEventInit",
"AddEventListenerOptions",
"ScrollIntoViewOptions",
"ScrollLogicalPosition",
```

Add any missing ones; keep existing ones. Sort the array alphabetically afterwards if the file already sorts them.

- [ ] **Step 4: Verify the new bin path compiles after move (deferred to A.5)**

No command yet — file `src/command_bar_app.rs` does not exist yet.

- [ ] **Step 5: Commit (deferred — combine with A.5)**

### Task A.2 — Create companion-file module skeleton in `vmux_layout/src/`

**Files:**
- Create: `crates/vmux_layout/src/command_bar.rs` (companion top file)
- Create: `crates/vmux_layout/src/command_bar/page.rs`
- Create: `crates/vmux_layout/src/command_bar/keyboard.rs`
- Create: `crates/vmux_layout/src/command_bar/results.rs`
- Create: `crates/vmux_layout/src/command_bar/shortcut.rs`
- Create: `crates/vmux_layout/src/command_bar/style.rs`

- [ ] **Step 1: Copy file bodies verbatim from `vmux_command`**

For each pair, copy the file content unchanged:

```bash
cp crates/vmux_command/src/page.rs     crates/vmux_layout/src/command_bar/page.rs
cp crates/vmux_command/src/keyboard.rs crates/vmux_layout/src/command_bar/keyboard.rs
cp crates/vmux_command/src/results.rs  crates/vmux_layout/src/command_bar/results.rs
cp crates/vmux_command/src/shortcut.rs crates/vmux_layout/src/command_bar/shortcut.rs
cp crates/vmux_command/src/style.rs    crates/vmux_layout/src/command_bar/style.rs
```

- [ ] **Step 2: Create companion `command_bar.rs` re-exporting submodules**

Write `crates/vmux_layout/src/command_bar.rs`:

```rust
pub mod keyboard;
pub mod page;
pub mod results;
pub mod style;

#[cfg(not(target_arch = "wasm32"))]
pub mod shortcut;
```

(If `shortcut.rs` is wasm-only in `vmux_command`, invert the cfg — check the original.)

- [ ] **Step 3: Update intra-module `use` paths inside the copied files**

In each of the 5 copied files, replace `use crate::keyboard::` → `use crate::command_bar::keyboard::`, similarly for `results`, `style`, `shortcut`, `event`. Run:

```bash
for f in [crates/vmux_layout/src/command_bar/page.rs crates/vmux_layout/src/command_bar/keyboard.rs crates/vmux_layout/src/command_bar/results.rs crates/vmux_layout/src/command_bar/shortcut.rs crates/vmux_layout/src/command_bar/style.rs] {
    ^sed -i "" 's|use crate::keyboard|use crate::command_bar::keyboard|g' $f
    ^sed -i "" 's|use crate::results|use crate::command_bar::results|g' $f
    ^sed -i "" 's|use crate::style|use crate::command_bar::style|g' $f
    ^sed -i "" 's|use crate::shortcut|use crate::command_bar::shortcut|g' $f
}
```

Event imports (`use crate::event::*`) must become `use vmux_command::event::*` since the wire types stay in `vmux_command`:

```bash
for f in [crates/vmux_layout/src/command_bar/page.rs crates/vmux_layout/src/command_bar/keyboard.rs crates/vmux_layout/src/command_bar/results.rs crates/vmux_layout/src/command_bar/shortcut.rs crates/vmux_layout/src/command_bar/style.rs] {
    ^sed -i "" 's|use crate::event|use vmux_command::event|g' $f
}
```

- [ ] **Step 4: Verify with rg there are no remaining `crate::keyboard` / `crate::event` etc. references in the new files**

Run:
```bash
^rg -n "crate::(keyboard|results|style|shortcut|event)" crates/vmux_layout/src/command_bar/
```
Expected: no matches.

### Task A.3 — Create bin entry point in `vmux_layout`

**Files:**
- Create: `crates/vmux_layout/src/command_bar_app.rs`

- [ ] **Step 1: Write the bin entry**

```rust
use vmux_layout::command_bar::page;

fn main() {
    dioxus::launch(page::Page);
}
```

(If `vmux_command/src/main.rs` calls a differently named root component, mirror it. Verify against the original.)

### Task A.4 — Register `command_bar` module in `vmux_layout/src/lib.rs`

**Files:**
- Modify: `crates/vmux_layout/src/lib.rs`

- [ ] **Step 1: Add `pub mod command_bar;` near other `pub mod` lines**

Place it alphabetically; keep wasm gating consistent. Inspect the file first to find the right place.

- [ ] **Step 2: Run `cargo check -p vmux_layout`**

```bash
^env -u CEF_PATH cargo check -p vmux_layout
```
Expected: success. If wasm-only modules fail on host, add `#[cfg(target_arch = "wasm32")]` where needed (verify against `vmux_command/src/lib.rs` cfg pattern).

### Task A.5 — Move `CommandPlugin::PageConfig` registration into a `CommandBarPagePlugin` in `vmux_layout`

The current `CommandPlugin` does two unrelated things: registers `AppCommand` (vocab; stays in `vmux_command`) and registers the `command-bar` `PageConfig` with the page registry (UI host registration; belongs with the UI in `vmux_layout`). Split them.

**Files:**
- Create: `crates/vmux_layout/src/command_bar/plugin.rs`
- Modify: `crates/vmux_layout/src/command_bar.rs` (add `#[cfg(not(target_arch = "wasm32"))] pub mod plugin;`)
- Modify: `crates/vmux_command/src/plugin.rs` (remove PageRegistry calls — see Task A.7)

- [ ] **Step 1: Write the new plugin**

```rust
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_page::{PageConfig, PageRegistry};

pub struct CommandBarPagePlugin;

impl Plugin for CommandBarPagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PageRegistry>();
        app.world_mut().resource_mut::<PageRegistry>().register(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            &PageConfig::with_custom_host("command-bar"),
        );
    }
}
```

Note: `env!("CARGO_MANIFEST_DIR")` now resolves to `vmux_layout`'s manifest dir, which is correct because the bin (`vmux_command_bar_app`) lives there.

- [ ] **Step 2: Register `CommandBarPagePlugin` inside `LayoutPlugin`**

In `crates/vmux_layout/src/plugin.rs`, add `crate::command_bar::plugin::CommandBarPagePlugin` to the `app.add_plugins((..))` tuple at the bottom. Place it after `HeaderLayoutPlugin` (or wherever fits alphabetically).

### Task A.6 — Delete UI files from `vmux_command`, prune Cargo manifest

**Files:**
- Delete: `crates/vmux_command/src/page.rs`
- Delete: `crates/vmux_command/src/keyboard.rs`
- Delete: `crates/vmux_command/src/results.rs`
- Delete: `crates/vmux_command/src/shortcut.rs`
- Delete: `crates/vmux_command/src/style.rs`
- Delete: `crates/vmux_command/src/main.rs`
- Modify: `crates/vmux_command/Cargo.toml`

- [ ] **Step 1: Remove the files**

```bash
^rm crates/vmux_command/src/page.rs crates/vmux_command/src/keyboard.rs crates/vmux_command/src/results.rs crates/vmux_command/src/shortcut.rs crates/vmux_command/src/style.rs crates/vmux_command/src/main.rs
```

- [ ] **Step 2: Remove `[[bin]]` entry, `build-deps`, all wasm32 deps, and `[features]` web entry from `vmux_command/Cargo.toml`**

Open `crates/vmux_command/Cargo.toml`. Delete:
- The entire `[[bin]] name = "vmux_command_app" path = "src/main.rs" required-features = ["web"]` block
- `[build-dependencies] vmux_page = { ..., features = ["build"] }` (verify it isn't used by another build.rs concern)
- The entire `[target.'cfg(target_arch = "wasm32")'.dependencies]` table (dioxus, js-sys, vmux_ui, wasm-bindgen, web-sys)
- `[features] web = [...]` if it exists
- `build = "build.rs"` line and `build.rs` itself if it only existed for the UI bundle

If `vmux_command` still needs `vmux_page` host-side for something (it should NOT — verify with `^rg -n "vmux_page" crates/vmux_command/src/`), remove that dep too.

- [ ] **Step 3: Verify `vmux_command` still builds host-only**

```bash
^env -u CEF_PATH cargo build -p vmux_command
```
Expected: success.

### Task A.7 — Strip `PageRegistry` registration from `CommandPlugin`

**Files:**
- Modify: `crates/vmux_command/src/plugin.rs`

- [ ] **Step 1: Rewrite `plugin.rs`**

```rust
use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .configure_sets(Update, ReadAppCommands.after(WriteAppCommands));
    }
}
```

- [ ] **Step 2: Drop `COMMAND_BAR_PAGE_URL` re-export decision**

`bundle.rs` currently only contains `pub const COMMAND_BAR_PAGE_URL: &str = "vmux://command-bar/";`. Keep it in `vmux_command` (the handler will reference it from `vmux_layout` via `vmux_command::COMMAND_BAR_PAGE_URL`, which is fine because `vmux_layout` already depends on `vmux_command`).

- [ ] **Step 3: Update `vmux_command/src/lib.rs`**

Remove the wasm-only `pub mod page;` (gone), `pub mod keyboard;`, `pub mod results;`, `pub mod style;`, `pub mod shortcut;`. The lib.rs becomes:

```rust
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use bundle::COMMAND_BAR_PAGE_URL;
#[cfg(not(target_arch = "wasm32"))]
pub use command::*;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::CommandPlugin;
```

(Adjust per actual `event.rs` cfg gating — `event.rs` is host+wasm so stays unconditional.)

### Task A.8 — Verify, fmt, clippy, test changed crates; commit Phase A

- [ ] **Step 1: Compute changed-crate set**

```bash
PKGS=(BASE=origin/main ./scripts/changed-crates.sh)
echo $PKGS
```
Expected: `vmux_command vmux_layout` plus anything downstream depending on the removed paths.

- [ ] **Step 2: Build workspace once to flush stale artifacts**

```bash
^env -u CEF_PATH cargo build --workspace --quiet
```
Expected: success.

- [ ] **Step 3: fmt + clippy + test per changed crate**

```bash
for pkg in $PKGS { ^cargo fmt -p $pkg -- --check }
for pkg in $PKGS { ^env -u CEF_PATH cargo clippy -p $pkg --all-targets -- -D warnings }
for pkg in $PKGS { ^env -u CEF_PATH cargo test -p $pkg }
```
Expected: all green.

- [ ] **Step 4: Commit**

```bash
^git add -A
"refactor(command-bar): relocate Dioxus UI from vmux_command into vmux_layout

Move command_bar Dioxus app (page/keyboard/results/shortcut/style) into
crates/vmux_layout/src/command_bar/ as companion-file modules. The
command-bar bin becomes vmux_command_bar_app in vmux_layout. vmux_command
loses its bin target and wasm32 deps; it remains a host-only crate that
owns the AppCommand vocab and command-bar wire event types.

PageRegistry registration moves to CommandBarPagePlugin (registered by
LayoutPlugin) so env!(CARGO_MANIFEST_DIR) resolves to the crate that owns
the bin.

Handler still lives in vmux_desktop (decoupled in later phases)." | save /tmp/commit_msg.txt -f
^git commit -F /tmp/commit_msg.txt --no-verify
```

---

## Phase B — Snapshot Infrastructure in `vmux_command`

Define the 4 snapshot resource types and the marker traits. No producer/consumer yet — pure data definitions with tests.

### Task B.1 — Create `crates/vmux_command/src/snapshot.rs` with 4 resource structs

**Files:**
- Create: `crates/vmux_command/src/snapshot.rs`
- Modify: `crates/vmux_command/src/lib.rs`

- [ ] **Step 1: Write the snapshot module**

```rust
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarAgentsSnapshot {
    pub providers: Vec<AgentProviderSummary>,
    pub strategies: Vec<AgentStrategySummary>,
}

#[derive(Clone, Debug)]
pub struct AgentProviderSummary {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct AgentStrategySummary {
    pub provider: String,
    pub model: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarSettingsSnapshot {
    pub recent_paths: Vec<String>,
    pub settings_webview_url: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarSpacesSnapshot {
    pub spaces: Vec<SpaceSummary>,
    pub active_space_name: String,
    pub spaces_webview_url: String,
}

#[derive(Clone, Debug)]
pub struct SpaceSummary {
    pub entity: Entity,
    pub name: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarTerminalsSnapshot {
    pub pid_to_entity: HashMap<u32, Entity>,
    pub processes: Vec<TerminalProcessSummary>,
    pub agent_session_to_entity: HashMap<String, Entity>,
    pub terminal_webview_url: String,
}

#[derive(Clone, Debug)]
pub struct TerminalProcessSummary {
    pub pid: u32,
    pub label: String,
}
```

Field set is the **minimum** required by the handler. Verify by reading `command_bar.rs` lines 372-845 (handler body) and 847-1670 (action observer) and confirming every cross-crate read maps to a field above. If a field is missing, add it; if a field is unused, delete it.

- [ ] **Step 2: Register in lib.rs**

Add `#[cfg(not(target_arch = "wasm32"))] pub mod snapshot;` and `#[cfg(not(target_arch = "wasm32"))] pub use snapshot::*;` to `crates/vmux_command/src/lib.rs`.

### Task B.2 — Register snapshots as resources in `CommandPlugin`

**Files:**
- Modify: `crates/vmux_command/src/plugin.rs`

- [ ] **Step 1: Init all 4 snapshots**

```rust
use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::snapshot::{
    CommandBarAgentsSnapshot, CommandBarSettingsSnapshot, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot,
};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .init_resource::<CommandBarAgentsSnapshot>()
            .init_resource::<CommandBarSettingsSnapshot>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarTerminalsSnapshot>()
            .configure_sets(Update, ReadAppCommands.after(WriteAppCommands));
    }
}
```

### Task B.3 — Add a `WriteCommandBarSnapshots` system set

**Files:**
- Modify: `crates/vmux_command/src/snapshot.rs`
- Modify: `crates/vmux_command/src/plugin.rs`

- [ ] **Step 1: Define the set**

In `snapshot.rs`:

```rust
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WriteCommandBarSnapshots;
```

In `plugin.rs`, configure ordering so handler reads see fresh writes:

```rust
.configure_sets(
    Update,
    (
        WriteAppCommands,
        WriteCommandBarSnapshots,
        ReadAppCommands,
    )
        .chain(),
)
```

(Adjust the chain to merge with the existing ReadAppCommands ordering — do NOT just replace it.)

### Task B.4 — Tests for snapshot defaults

**Files:**
- Modify: `crates/vmux_command/src/snapshot.rs`

- [ ] **Step 1: Add inline test module**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_snapshot_default_is_empty() {
        let s = CommandBarAgentsSnapshot::default();
        assert!(s.providers.is_empty());
        assert!(s.strategies.is_empty());
    }

    #[test]
    fn terminals_snapshot_default_is_empty() {
        let s = CommandBarTerminalsSnapshot::default();
        assert!(s.pid_to_entity.is_empty());
        assert!(s.processes.is_empty());
        assert!(s.agent_session_to_entity.is_empty());
    }
}
```

- [ ] **Step 2: Run**

```bash
^env -u CEF_PATH cargo test -p vmux_command
```
Expected: pass.

### Task B.5 — Verify, fmt, clippy; commit Phase B

```bash
^cargo fmt -p vmux_command -- --check
^env -u CEF_PATH cargo clippy -p vmux_command --all-targets -- -D warnings
^env -u CEF_PATH cargo test -p vmux_command
^git add -A
"refactor(command): add CommandBarSnapshot resources for handler decoupling

Introduce CommandBarAgentsSnapshot, CommandBarSettingsSnapshot,
CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot resources owned by
vmux_command (DAG bottom). Domain crates fill them via updater systems
ordered in the WriteCommandBarSnapshots set so the handler (still in
vmux_desktop) can read them in ReadAppCommands without importing
agent/setting/space/terminal types." | save /tmp/commit_msg.txt -f
^git commit -F /tmp/commit_msg.txt --no-verify
```

---

## Phase C — Per-Domain Snapshot Updater Systems

One commit per domain (4 commits). Each adds ONE system + ONE test in the owning crate.

### Task C.1 — `vmux_agent` updater

**Files:**
- Create: `crates/vmux_agent/src/snapshot_updater.rs`
- Modify: `crates/vmux_agent/src/plugin.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the updater**

```rust
use bevy::prelude::*;
use vmux_command::snapshot::{
    AgentProviderSummary, AgentStrategySummary, CommandBarAgentsSnapshot,
    WriteCommandBarSnapshots,
};

use crate::plugin::AgentProviders;
use crate::strategy::AgentStrategies;

pub fn update_agents_snapshot(
    providers: Option<Res<AgentProviders>>,
    strategies: Option<Res<AgentStrategies>>,
    mut snapshot: ResMut<CommandBarAgentsSnapshot>,
) {
    let prov_changed = providers
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    let strat_changed = strategies
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !prov_changed && !strat_changed && !snapshot.providers.is_empty() {
        return;
    }

    let providers_vec = providers
        .as_ref()
        .map(|p| {
            p.command_entries()
                .into_iter()
                .map(|e| AgentProviderSummary {
                    id: e.id,
                    name: e.name,
                    provider: e.provider,
                    model: e.model,
                })
                .collect()
        })
        .unwrap_or_default();

    let strategies_vec = strategies
        .as_ref()
        .map(|s| {
            s.page_strategies()
                .map(|st| AgentStrategySummary {
                    provider: st.provider().to_string(),
                    model: st.model().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    snapshot.providers = providers_vec;
    snapshot.strategies = strategies_vec;
}
```

(`AgentCommandEntry` fields are read from `command_bar.rs:8` — confirm the field names match `id/name/provider/model`.)

- [ ] **Step 2: Register module and system**

Add `pub mod snapshot_updater;` to `lib.rs`. In `plugin.rs` (inside `AgentPage::build` or sub-plugin), add:

```rust
.add_systems(
    Update,
    snapshot_updater::update_agents_snapshot
        .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
)
```

- [ ] **Step 3: Inline test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    fn writes_empty_snapshot_when_no_providers() {
        let mut app = App::new();
        app.init_resource::<CommandBarAgentsSnapshot>();
        app.add_systems(Update, update_agents_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarAgentsSnapshot>();
        assert!(snap.providers.is_empty());
        assert!(snap.strategies.is_empty());
    }
}
```

- [ ] **Step 4: Verify, commit**

```bash
^env -u CEF_PATH cargo test -p vmux_agent
^cargo fmt -p vmux_agent -- --check
^env -u CEF_PATH cargo clippy -p vmux_agent --all-targets -- -D warnings
^git add -A
"refactor(agent): write CommandBarAgentsSnapshot from AgentProviders+AgentStrategies" | save /tmp/commit_msg.txt -f
^git commit -F /tmp/commit_msg.txt --no-verify
```

### Task C.2 — `vmux_setting` updater

**Files:**
- Create: `crates/vmux_setting/src/snapshot_updater.rs`
- Modify: `crates/vmux_setting/src/lib.rs`
- Modify: `crates/vmux_setting/src/plugin.rs` (whichever plugin file exists)

- [ ] **Step 1: Write the updater**

```rust
use bevy::prelude::*;
use vmux_command::snapshot::{CommandBarSettingsSnapshot, WriteCommandBarSnapshots};

use crate::AppSettings;
use crate::event::SETTINGS_PAGE_URL;

pub fn update_settings_snapshot(
    settings: Res<AppSettings>,
    mut snapshot: ResMut<CommandBarSettingsSnapshot>,
) {
    if !settings.is_changed() && !settings.is_added() && !snapshot.settings_webview_url.is_empty() {
        return;
    }
    snapshot.recent_paths = settings.recent_paths_for_command_bar();
    snapshot.settings_webview_url = SETTINGS_PAGE_URL.to_string();
}
```

If `AppSettings` does not expose `recent_paths_for_command_bar()`, inline the existing logic from `command_bar.rs` where `settings.recent_paths` is currently read. Add the method to `AppSettings` if helpful for re-use.

- [ ] **Step 2: Register, test, commit (same shape as C.1)**

### Task C.3 — `vmux_space` updater

**Files:**
- Create: `crates/vmux_space/src/snapshot_updater.rs`
- Modify: `crates/vmux_space/src/lib.rs` / plugin

- [ ] **Step 1: Write the updater**

```rust
use bevy::prelude::*;
use vmux_command::snapshot::{
    CommandBarSpacesSnapshot, SpaceSummary, WriteCommandBarSnapshots,
};

use crate::event::SPACES_PAGE_URL;
use crate::{ActiveSpace, Spaces};

pub fn update_spaces_snapshot(
    spaces: Res<Spaces>,
    active: Res<ActiveSpace>,
    mut snapshot: ResMut<CommandBarSpacesSnapshot>,
) {
    if !spaces.is_changed() && !active.is_changed() && !snapshot.spaces_webview_url.is_empty() {
        return;
    }
    snapshot.spaces = spaces
        .iter()
        .map(|(entity, record)| SpaceSummary { entity, name: record.name.clone() })
        .collect();
    snapshot.active_space_name = active.record.name.clone();
    snapshot.spaces_webview_url = SPACES_PAGE_URL.to_string();
}
```

(Verify `Spaces::iter` shape — adjust mapping as needed.)

- [ ] **Step 2: Register, test, commit**

### Task C.4 — `vmux_terminal` updater

**Files:**
- Create: `crates/vmux_terminal/src/snapshot_updater.rs`
- Modify: `crates/vmux_terminal/src/lib.rs` / plugin

- [ ] **Step 1: Write the updater**

```rust
use bevy::prelude::*;
use vmux_command::snapshot::{
    CommandBarTerminalsSnapshot, TerminalProcessSummary, WriteCommandBarSnapshots,
};

use crate::pid::PidToEntity;
use crate::processes_monitor::ProcessesMonitor;
use vmux_layout::event::TERMINAL_PAGE_URL;

pub fn update_terminals_snapshot(
    pid_map: Option<Res<PidToEntity>>,
    procs: Option<Res<ProcessesMonitor>>,
    mut snapshot: ResMut<CommandBarTerminalsSnapshot>,
) {
    let pid_changed = pid_map.as_ref().map(|r| r.is_changed()).unwrap_or(false);
    let procs_changed = procs.as_ref().map(|r| r.is_changed()).unwrap_or(false);
    if !pid_changed && !procs_changed && !snapshot.terminal_webview_url.is_empty() {
        return;
    }

    snapshot.pid_to_entity = pid_map.as_deref().map(|m| m.0.clone()).unwrap_or_default();
    snapshot.processes = procs
        .as_deref()
        .map(|p| {
            p.entries()
                .map(|e| TerminalProcessSummary { pid: e.pid, label: e.label.clone() })
                .collect()
        })
        .unwrap_or_default();
    snapshot.terminal_webview_url = TERMINAL_PAGE_URL.to_string();
}
```

For `agent_session_to_entity`: this comes from `vmux_agent::session::AgentSessionToEntity`, which lives in `vmux_agent`. Move that piece to `Task C.1` (extend the agent updater to also write `CommandBarTerminalsSnapshot.agent_session_to_entity`) OR add a SECOND updater in `vmux_agent`. Choose option 2 for clarity:

In `vmux_agent/src/snapshot_updater.rs`, add:

```rust
pub fn update_agent_sessions_snapshot(
    sessions: Option<Res<crate::session::AgentSessionToEntity>>,
    mut snapshot: ResMut<CommandBarTerminalsSnapshot>,
) {
    let changed = sessions.as_ref().map(|r| r.is_changed()).unwrap_or(false);
    if !changed && !snapshot.agent_session_to_entity.is_empty() {
        return;
    }
    snapshot.agent_session_to_entity =
        sessions.as_deref().map(|m| m.0.clone()).unwrap_or_default();
}
```

Register it in the same `Update` schedule + `WriteCommandBarSnapshots` set as `update_agents_snapshot`.

- [ ] **Step 2: Register, test, commit**

### Task C.5 — Workspace verification commit-gate

After C.1–C.4 all merged:

```bash
PKGS=(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS { ^cargo fmt -p $pkg -- --check }
for pkg in $PKGS { ^env -u CEF_PATH cargo clippy -p $pkg --all-targets -- -D warnings }
for pkg in $PKGS { ^env -u CEF_PATH cargo test -p $pkg }
```

Expected: all green. Snapshots populated, handler not yet using them (still reads original resources). No regression.

---

## Phase D — Handler Query Rewrite (still in `vmux_desktop`)

Replace cross-crate type reads inside `handle_open_command_bar` and `on_command_bar_action` with snapshot reads. Handler **stays in `vmux_desktop`** during this phase; behavior must remain identical. One commit per query group.

### Task D.1 — Replace `AgentProviders` reads with snapshot

**Files:**
- Modify: `crates/vmux_desktop/src/command_bar.rs`

- [ ] **Step 1: In `handle_open_command_bar` (line ~406), replace ParamSet slot `Option<Res<AgentProviders>>` with `Res<vmux_command::snapshot::CommandBarAgentsSnapshot>` as a top-level param**

Before:
```rust
mut space_params: ParamSet<(
    Res<ActiveSpace>,
    Option<Res<AgentProviders>>,
    ResMut<NewStackContext>,
    Option<Res<vmux_agent::strategy::AgentStrategies>>,
)>,
```

After:
```rust
agents_snapshot: Res<vmux_command::snapshot::CommandBarAgentsSnapshot>,
mut space_params: ParamSet<(
    Res<ActiveSpace>,
    ResMut<NewStackContext>,
)>,
```

- [ ] **Step 2: Replace usages**

```rust
let agent_entries: Vec<AgentCommandEntry> = agents_snapshot
    .providers
    .iter()
    .map(|p| AgentCommandEntry {
        id: p.id.clone(),
        name: p.name.clone(),
        provider: p.provider.clone(),
        model: p.model.clone(),
    })
    .collect();

let app_agent_entries: Vec<AppAgentEntry> = agents_snapshot
    .strategies
    .iter()
    .map(|s| AppAgentEntry {
        id: app_agent_id(&s.provider, &s.model),
        name: format!("New {}/{} chat (App)", s.provider, s.model),
        provider: s.provider.clone(),
        model: s.model.clone(),
    })
    .collect();
```

- [ ] **Step 3: Drop the `use vmux_agent::plugin::{AgentCommandEntry, AgentProviders}` import** (keep `AgentCommandEntry` only if locally redefined or alias to snapshot type)

Actually keep `AgentCommandEntry` since the wire format passed to the UI needs it. If `AgentCommandEntry` lives in `vmux_agent::plugin`, move it to `vmux_command::event` (it IS a UI wire payload):

- Verify `AgentCommandEntry` definition (likely in `vmux_agent/src/plugin.rs`).
- Move the struct to `vmux_command/src/event.rs` (it's already wasm-safe shape).
- Re-export from `vmux_agent` for back-compat if needed: `pub use vmux_command::event::AgentCommandEntry;`.

- [ ] **Step 4: Build, fmt, clippy, test, commit**

```bash
^env -u CEF_PATH cargo build -p vmux_desktop
^cargo fmt -p vmux_desktop -p vmux_agent -p vmux_command -- --check
^env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
^env -u CEF_PATH cargo test -p vmux_desktop -p vmux_agent -p vmux_command
"refactor(desktop): handler reads agents via CommandBarAgentsSnapshot" | save /tmp/commit_msg.txt -f
^git commit -F /tmp/commit_msg.txt --no-verify
```

### Task D.2 — Replace `ActiveSpace` + `Spaces` reads with `CommandBarSpacesSnapshot`

Pattern identical to D.1. Replace:
- `space_params.p0().clone()` (ActiveSpace) → `spaces_snapshot.active_space_name.clone()`
- Any `Res<Spaces>` reads → iterate `spaces_snapshot.spaces`
- `SPACES_PAGE_URL` import is replaced with `spaces_snapshot.spaces_webview_url.as_str()`

Drop `use vmux_space::event::SPACES_PAGE_URL;` and `use vmux_space::{ActiveSpace, Spaces};` from `command_bar.rs`. Keep `SpaceCommandEvent` if still needed for writes (handled in Phase E).

Commit:
```
"refactor(desktop): handler reads spaces via CommandBarSpacesSnapshot"
```

### Task D.3 — Replace `AppSettings` reads with `CommandBarSettingsSnapshot`

Replace `resource_params.p0().clone()` (Settings) with `settings_snapshot: Res<CommandBarSettingsSnapshot>`. Use `.recent_paths` and `.settings_webview_url` fields. Drop `vmux_setting::AppSettings` / `vmux_setting::Settings` / `vmux_setting::event::SETTINGS_PAGE_URL` imports from `command_bar.rs`.

Commit:
```
"refactor(desktop): handler reads settings via CommandBarSettingsSnapshot"
```

### Task D.4 — Replace `PidToEntity` + `ProcessesMonitor` + `AgentSessionToEntity` with `CommandBarTerminalsSnapshot`

Replace `resource_params.p2()`, `resource_params.p3()`, and any `Res<ProcessesMonitor>` usage with snapshot fields. Drop:
- `use vmux_terminal::processes_monitor::ProcessesMonitor;`
- `use vmux_terminal::pid::PidToEntity;` (verify exact path)
- `use vmux_agent::session::AgentSessionToEntity;` (verify exact path)
- `use vmux_layout::event::TERMINAL_PAGE_URL;` (use `terminals_snapshot.terminal_webview_url.as_str()`)

Commit:
```
"refactor(desktop): handler reads terminals/sessions via CommandBarTerminalsSnapshot"
```

### Task D.5 — Audit remaining cross-crate type imports

```bash
^rg -n "^use vmux_(agent|setting|space|terminal)::" crates/vmux_desktop/src/command_bar.rs
```
Expected after D.4: only stack-tree types remain (`vmux_layout::pane::*`, `vmux_layout::stack::*`, `vmux_layout::space::*`, `vmux_layout::side_sheet::*`, `vmux_layout::window::*`, `vmux_layout::Header`, `vmux_layout::NewStackContext`, `vmux_layout::event::TERMINAL_PAGE_URL` if still used) and `vmux_command::*`.

If any remain pointing at `vmux_agent`/`vmux_setting`/`vmux_space`/`vmux_terminal`, extend snapshots to cover them and repeat D.1-D.4 for that field.

Commit: only if changes made (likely none). Otherwise skip.

---

## Phase E — Handler Writes → `AppCommand` Vocab Only

Handler currently writes:
1. `MessageWriter<AppCommand>` — vocab, keep.
2. `Option<MessageWriter<AgentLaunchRequested>>` — NOT vocab. Convert.
3. `MessageWriter<vmux_core::agent::SpawnAgentInStackRequest>` — vocab-ish (in `vmux_core`), keep.
4. Direct `new_terminal_bundle(..)` / `new_terminal_bundle_with_cwd(..)` calls + `commands.spawn(..)` — NOT vocab. Convert.

### Task E.1 — Add `AgentCommand::Launch{..}` to `AppCommand`

**Files:**
- Modify: `crates/vmux_command/src/command.rs`
- Modify: `crates/vmux_agent/src/plugin.rs` (responder)

- [ ] **Step 1: Add variant**

In `crates/vmux_command/src/command.rs`, find the existing `AgentCommand` enum (or add one if missing under `AppCommand::Agent`):

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCommand {
    Launch {
        provider: String,
        model: String,
        target_stack: Option<Entity>,
        space_entity: Option<Entity>,
    },
}
```

If `AppCommand` does not yet have an `Agent` variant, add it.

- [ ] **Step 2: Responder system in `vmux_agent`**

In `crates/vmux_agent/src/plugin.rs` add:

```rust
pub fn respond_agent_launch(
    mut reader: MessageReader<AppCommand>,
    mut writer: MessageWriter<AgentLaunchRequested>,
) {
    for cmd in reader.read() {
        if let AppCommand::Agent(AgentCommand::Launch { provider, model, target_stack, space_entity }) = cmd {
            writer.write(AgentLaunchRequested {
                provider: provider.clone(),
                model: model.clone(),
                target_stack: *target_stack,
                space_entity: *space_entity,
            });
        }
    }
}
```

Register in `AgentPage::build`:
```rust
.add_systems(Update, respond_agent_launch.in_set(vmux_command::ReadAppCommands))
```

- [ ] **Step 3: Test responder**

Inline test that writes `AppCommand::Agent(AgentCommand::Launch{..})` and asserts an `AgentLaunchRequested` event appears.

- [ ] **Step 4: Replace handler writes**

In `command_bar.rs`, every `writer_params.p1()` (`AgentLaunchRequested`) call site becomes `writer_params.p0().write(AppCommand::Agent(AgentCommand::Launch{..}))`. Drop the `p1` slot from `writer_params`.

- [ ] **Step 5: Verify, commit**

### Task E.2 — Add `TerminalCommand::SpawnAt{cwd}` and responder

**Files:**
- Modify: `crates/vmux_command/src/command.rs`
- Modify: `crates/vmux_terminal/src/plugin.rs` (or wherever its main plugin is)

- [ ] **Step 1: Add variant** (or extend existing `TerminalCommand`)

```rust
pub enum TerminalCommand {
    // ...existing
    SpawnAt {
        cwd: Option<std::path::PathBuf>,
        target_stack: Option<Entity>,
    },
}
```

- [ ] **Step 2: Responder consumes `TerminalCommand::SpawnAt` and spawns the bundle**

Move the `new_terminal_bundle*` + `commands.spawn(..)` invocation from `command_bar.rs` into a system in `vmux_terminal/src/plugin.rs`:

```rust
pub fn respond_terminal_spawn(
    mut reader: MessageReader<AppCommand>,
    mut commands: Commands,
    /* whatever bundle deps */
) {
    for cmd in reader.read() {
        if let AppCommand::Terminal(TerminalCommand::SpawnAt { cwd, target_stack }) = cmd {
            let bundle = match cwd {
                Some(p) => new_terminal_bundle_with_cwd(p.clone()),
                None => new_terminal_bundle(),
            };
            let term_e = commands.spawn(bundle).id();
            if let Some(stack_e) = target_stack {
                commands.entity(*stack_e).insert(PageMetadata { /* ... */ });
                /* attach term_e to stack_e per existing handler logic */
            }
        }
    }
}
```

(Copy attach logic verbatim from `command_bar.rs` action observer.)

- [ ] **Step 3: Replace handler `commands.spawn(new_terminal_bundle*..)` with `writer.write(AppCommand::Terminal(TerminalCommand::SpawnAt{..}))`**

- [ ] **Step 4: Drop `use vmux_terminal::{new_terminal_bundle, new_terminal_bundle_with_cwd};` from `command_bar.rs`**

- [ ] **Step 5: Test + commit**

### Task E.3 — Final write audit

```bash
^rg -n "vmux_terminal::|vmux_agent::|new_terminal_bundle|AgentLaunchRequested|SpawnAgentInStackRequest" crates/vmux_desktop/src/command_bar.rs
```
Expected: only `vmux_core::agent::SpawnAgentInStackRequest` references remain (this is vocab in `vmux_core`, not a domain crate). If anything else appears, extend Phase E with one more vocab conversion.

Commit any cleanup.

---

## Phase F — Move Handler from `vmux_desktop` to `vmux_layout`

Now the handler reads only `vmux_command` + `vmux_core` + `vmux_layout` + `bevy_cef` + `bevy` and writes only `AppCommand`. It can live in `vmux_layout`.

### Task F.1 — Move `command_bar.rs` body into `vmux_layout/src/command_bar/handler.rs`

**Files:**
- Create: `crates/vmux_layout/src/command_bar/handler.rs`
- Modify: `crates/vmux_layout/src/command_bar.rs` (add `pub mod handler;`)
- Modify: `crates/vmux_layout/src/command_bar/plugin.rs` (extend `CommandBarPagePlugin` or add a new `CommandBarPlugin`)
- Delete: `crates/vmux_desktop/src/command_bar.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Move the file**

```bash
^git mv crates/vmux_desktop/src/command_bar.rs crates/vmux_layout/src/command_bar/handler.rs
```

- [ ] **Step 2: Fix imports inside handler.rs**

`use crate::browser::Browser;` (was in vmux_desktop) → `use vmux_desktop::browser::Browser;` is wrong (vmux_layout cannot depend on vmux_desktop — cycle). Inspect: does the handler actually need `crate::browser::Browser` or can it use the `vmux_layout::cef::Browser` re-export?

```bash
^rg -n "browser::Browser|crate::browser" crates/vmux_layout/src/command_bar/handler.rs
```

If `crate::browser::Browser` was actually `vmux_layout::cef::Browser` aliased, swap to `use crate::cef::Browser;`. If it was a vmux_desktop-specific type, **escalate** — that means the handler isn't fully decoupled and Phase D missed a query. Add another snapshot field.

`use crate::browser::Browsers;` (NonSend) likely also vmux_desktop — same check.

- [ ] **Step 3: Adjust visibility**

`pub(crate)` items in the moved file may now need broader visibility. Audit:

```bash
^rg -n "pub\\(crate\\)" crates/vmux_layout/src/command_bar/handler.rs
```

Promote to `pub` if referenced from outside `command_bar/`, otherwise leave.

- [ ] **Step 4: Build** — `^env -u CEF_PATH cargo build -p vmux_layout`. Fix any path errors iteratively.

### Task F.2 — Move `CommandBarInputPlugin` registration

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/plugin.rs`
- Modify: `crates/vmux_layout/src/plugin.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Move the `CommandBarInputPlugin` struct into `vmux_layout/src/command_bar/plugin.rs`**

Copy the `impl Plugin for CommandBarInputPlugin` block from the old `command_bar.rs` (lines ~71-160). The plugin now lives in `vmux_layout`. Rename it to `CommandBarPlugin` (simpler, since the "Input" qualifier was distinguishing it from the UI plugin and we now have `CommandBarPagePlugin` for that).

- [ ] **Step 2: Register from `LayoutPlugin`**

In `crates/vmux_layout/src/plugin.rs`, add `crate::command_bar::plugin::CommandBarPlugin` to the `add_plugins((..))` tuple at the bottom.

- [ ] **Step 3: Remove from `vmux_desktop/src/lib.rs`**

Drop:
- `mod command_bar;` (line 10)
- `command_bar::CommandBarInputPlugin` from the imports (line 24)
- `CommandBarInputPlugin,` from the plugin tuple (line 92)

### Task F.3 — Verify everything

```bash
PKGS=(BASE=origin/main ./scripts/changed-crates.sh)
^env -u CEF_PATH cargo build --workspace --quiet
for pkg in $PKGS { ^cargo fmt -p $pkg -- --check }
for pkg in $PKGS { ^env -u CEF_PATH cargo clippy -p $pkg --all-targets -- -D warnings }
for pkg in $PKGS { ^env -u CEF_PATH cargo test -p $pkg }
```

Manual smoke test:
- Launch vmux, press cmd+k. Command bar opens.
- Type "term" → terminal entries appear.
- Type a path → opens terminal at path.
- Pick an agent → agent spawns.
- All previous flows behave identically.

If any flow regresses, the snapshot is missing a field; extend the relevant updater and snapshot type.

Commit:
```
"refactor: move command_bar handler from vmux_desktop to vmux_layout

Handler now reads CommandBar*Snapshot resources from vmux_command (no
direct vmux_agent/setting/space/terminal type imports) and writes only
AppCommand vocab. CommandBarPlugin is registered by LayoutPlugin.
vmux_desktop drops command_bar.rs entirely."
```

---

## Phase G — Cleanup and Final DAG Verification

### Task G.1 — Drop unused re-exports

```bash
^rg -n "pub use" crates/vmux_agent/src/lib.rs crates/vmux_setting/src/lib.rs crates/vmux_space/src/lib.rs crates/vmux_terminal/src/lib.rs
```

For each pub-use, run `^rg "<symbol>"` workspace-wide. If only usage was in the deleted `command_bar.rs`, delete the re-export.

### Task G.2 — Verify DAG (no `command_bar` references survive in vmux_desktop)

```bash
^rg -n "command_bar|CommandBarInput|CommandBar" crates/vmux_desktop/src/
```
Expected: no matches (except possibly transient `vmux_command::CommandBarOpenEvent` wire emissions from some other system — verify those are still intentional).

```bash
^cargo tree -p vmux_layout --depth 1 --edges normal | ^rg "vmux_"
```
Expected dependencies: `vmux_command vmux_core vmux_history vmux_page` + bevy/dioxus/etc. NOT `vmux_agent`, `vmux_setting`, `vmux_space`, `vmux_terminal`, `vmux_desktop`. If any appear, snapshot decoupling missed a field.

### Task G.3 — Final fmt + clippy + test sweep on full changed set

```bash
PKGS=(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS { ^cargo fmt -p $pkg -- --check }
for pkg in $PKGS { ^env -u CEF_PATH cargo clippy -p $pkg --all-targets -- -D warnings }
for pkg in $PKGS { ^env -u CEF_PATH cargo test -p $pkg }
```

### Task G.4 — Push and delete this plan

```bash
^git push --force-with-lease --no-verify origin refactor-plugins
^rm docs/plans/2026-05-20-command-bar-decouple.md
^git add -A
"chore(docs): drop completed command-bar decouple plan" | save /tmp/commit_msg.txt -f
^git commit -F /tmp/commit_msg.txt --no-verify
^git push --no-verify origin refactor-plugins
```

---

## Self-Review Notes

- **Spec coverage:** Phases A (UI move), B (snapshot infra), C (4 updaters), D (handler reads), E (handler writes), F (physical move), G (cleanup) cover the stated goal: UI in `vmux_layout`, handler in `vmux_layout`, decoupled via `vmux_command` snapshots.
- **Placeholders:** Some struct field sets (e.g., `AgentProviderSummary`) are "minimum required" pending verification against the 2725-line handler. Each Phase C task explicitly says to verify and extend if missing. This is acceptable because the verification step is concrete (read specific line ranges, run specific rg commands).
- **Type consistency:** `CommandBarAgentsSnapshot.providers: Vec<AgentProviderSummary>` is referenced consistently in B.1, C.1, D.1. Same for the other 3 snapshots. `AgentCommand::Launch` field set in E.1 matches `AgentLaunchRequested` shape (verify when implementing).
- **Risk areas:** Phase D.5 audit and Phase F.1 Step 2 (`crate::browser::Browser` resolution) are the most likely places to discover missed snapshot fields. Both have explicit escalation instructions ("add another snapshot field"). The 2725-line handler is large enough that Phase D will likely surface 1-2 surprise reads — that's why it's split into 5 commits.
- **Granularity:** Phase A is fully detailed (mechanical, low risk). Phases B-G use representative code blocks for the canonical pattern and rely on per-task verification commands rather than spelling out every line — appropriate because the pattern is repetitive and the verification step (rg + cargo build) catches mistakes immediately.
