# Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a unified command palette (Cmd+L / Cmd+K) that opens a centered modal overlay for URL navigation, command execution, and tab search.

**Architecture:** New `vmux_command_palette` Dioxus WASM crate rendered in a CEF webview, same pattern as `vmux_header`. Bevy-side `CommandPalettePlugin` handles open/close, keyboard routing, and command dispatch. The existing `Modal` entity in `window.rs` hosts the palette webview.

**Tech Stack:** Bevy 0.18, bevy_cef (CEF webview), Dioxus 0.7.4 (WASM), Tailwind CSS

---

### Task 1: Create `vmux_command_palette` crate scaffold

**Files:**
- Create: `crates/vmux_command_palette/Cargo.toml`
- Create: `crates/vmux_command_palette/Dioxus.toml`
- Create: `crates/vmux_command_palette/build.rs`
- Create: `crates/vmux_command_palette/src/lib.rs`
- Create: `crates/vmux_command_palette/src/main.rs`
- Create: `crates/vmux_command_palette/src/event.rs`
- Create: `crates/vmux_command_palette/src/app.rs`
- Create: `crates/vmux_command_palette/assets/tailwind.css`
- Create: `crates/vmux_command_palette/tailwind.config.js`

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "vmux_command_palette"
description = "Bevy + CEF + Dioxus command palette webview"
version.workspace = true
edition.workspace = true
publish = false
build = "build.rs"

[features]
default = []
debug = []
web = []

[[bin]]
name = "vmux_command_palette_app"
path = "src/main.rs"
required-features = ["web"]

[lib]
path = "src/lib.rs"

[build-dependencies]
vmux_webview_app = { path = "../vmux_webview_app", features = ["build"] }

[dependencies]
serde = { workspace = true }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
bevy = { workspace = true }
bevy_cef = { workspace = true, features = ["debug"] }
bevy_ecs = { workspace = true }
vmux_webview_app = { path = "../vmux_webview_app" }

[target.'cfg(target_arch = "wasm32")'.dependencies]
dioxus = { workspace = true }
vmux_ui = { path = "../vmux_ui", default-features = false }
wasm-bindgen = "0.2.115"
```

- [ ] **Step 2: Create `Dioxus.toml`**

```toml
[application]
name = "vmux_command_palette"
default_platform = "web"

[web.app]
title = "vmux command palette"
```

- [ ] **Step 3: Create `build.rs`**

```rust
use std::path::PathBuf;
use vmux_webview_app::build::{CefEmbeddedWebviewFinalize, WebviewAppBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_command_palette", "vmux_command_palette_app")
        .track_manifest_rel_paths(&["tailwind.config.js", "../vmux_ui/assets/theme.css"])
        .dx_extra_args(&["--bin", "vmux_command_palette_app", "--features", "web"])
        .cef_finalize(CefEmbeddedWebviewFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxh", "command_palette-dxh"])
        .run("vmux_command_palette");
}
```

- [ ] **Step 4: Create `src/event.rs`**

```rust
pub const PALETTE_OPEN_EVENT: &str = "palette_open";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PaletteOpenEvent {
    pub url: String,
    pub tabs: Vec<PaletteTab>,
    pub commands: Vec<PaletteCommandEntry>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaletteTab {
    pub title: String,
    pub url: String,
    pub pane_id: u64,
    pub tab_index: usize,
    pub is_active: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaletteCommandEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaletteActionEvent {
    pub action: String,
    pub value: String,
}
```

- [ ] **Step 5: Create `src/lib.rs`**

```rust
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;

#[cfg(not(target_arch = "wasm32"))]
pub use bundle::{CommandPalette, COMMAND_PALETTE_WEBVIEW_URL};

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
```

- [ ] **Step 6: Create `src/bundle.rs`**

```rust
use bevy::prelude::*;
use bevy_cef::prelude::*;

pub const COMMAND_PALETTE_WEBVIEW_URL: &str = "vmux://command-palette/";

#[derive(Component)]
pub struct CommandPalette;

#[derive(Bundle)]
pub struct CommandPaletteBundle {
    pub marker: CommandPalette,
    pub source: WebviewSource,
    pub mesh: Mesh3d,
    pub material: MeshMaterial3d<WebviewExtendStandardMaterial>,
    pub webview_size: WebviewSize,
}
```

- [ ] **Step 7: Create `src/plugin.rs`**

```rust
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

pub struct CommandPalettePlugin;

impl Plugin for CommandPalettePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("command-palette"),
            );
    }
}
```

- [ ] **Step 8: Create `src/main.rs`**

```rust
mod app;

fn main() {
    dioxus::launch(app::App);
}
```

- [ ] **Step 9: Create `src/app.rs`** (minimal placeholder)

```rust
#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command_palette::event::{
    PaletteActionEvent, PaletteOpenEvent, PALETTE_OPEN_EVENT,
};
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener};

#[component]
pub fn App() -> Element {
    let mut state = use_signal(PaletteOpenEvent::default);
    let _listener = use_event_listener::<PaletteOpenEvent, _>(
        PALETTE_OPEN_EVENT,
        move |data| {
            state.set(data);
        },
    );

    let PaletteOpenEvent { url, tabs, commands } = state();

    rsx! {
        div { class: "flex h-full w-full items-start justify-center bg-black/50 pt-[20%]",
            div { class: "flex w-full max-w-lg flex-col rounded-xl border border-border bg-card shadow-2xl",
                div { class: "p-2",
                    input {
                        r#type: "text",
                        class: "w-full rounded-lg bg-muted px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground",
                        placeholder: "Type a URL or search...",
                        value: "{url}",
                        autofocus: true,
                    }
                }
                div { class: "max-h-64 overflow-y-auto border-t border-border p-1",
                    p { class: "px-3 py-2 text-xs text-muted-foreground", "Palette connected. {tabs.len()} tabs, {commands.len()} commands." }
                }
            }
        }
    }
}
```

- [ ] **Step 10: Create `assets/tailwind.css`**

Copy from `crates/vmux_header/assets/tailwind.css`:

```css
@import "../../vmux_ui/assets/theme.css";
@tailwind base;
@tailwind components;
@tailwind utilities;
```

- [ ] **Step 11: Create `tailwind.config.js`**

Copy from `crates/vmux_header/tailwind.config.js` (check the file and replicate it).

- [ ] **Step 12: Verify the crate compiles for the native target**

Run: `cargo check -p vmux_command_palette --lib`

- [ ] **Step 13: Commit**

```bash
git add crates/vmux_command_palette/
git commit -m "feat: scaffold vmux_command_palette crate"
```

---

### Task 2: Register plugin and spawn palette webview on Modal entity

**Files:**
- Create: `crates/vmux_desktop/src/command_palette.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`
- Modify: `crates/vmux_desktop/src/layout/window.rs`

- [ ] **Step 1: Create `crates/vmux_desktop/src/command_palette.rs`**

This is the Bevy-side handler. Start with just the open/close toggle and keyboard routing:

```rust
use crate::{
    command::{AppCommand, BrowserCommand, ReadAppCommands},
    layout::{
        pane::{Pane, PaneSplit},
        space::Space,
        tab::{Active, Tab, focused_tab},
    },
};
use bevy::{
    ecs::message::MessageReader,
    prelude::*,
};
use bevy_cef::prelude::*;
use vmux_command_palette::{
    CommandPalette, COMMAND_PALETTE_WEBVIEW_URL,
    event::{
        PaletteActionEvent, PaletteCommandEntry, PaletteOpenEvent, PaletteTab,
        PALETTE_OPEN_EVENT,
    },
};
use vmux_header::{Header, PageMetadata};
use vmux_side_sheet::SideSheet;

pub(crate) struct CommandPaletteHandlerPlugin;

impl Plugin for CommandPaletteHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<PaletteActionEvent>::default())
            .add_observer(on_palette_action_emit)
            .add_systems(Update, handle_open_palette.in_set(ReadAppCommands));
    }
}

fn handle_open_palette(
    mut reader: MessageReader<AppCommand>,
    mut palette_q: Query<(Entity, &mut Node), With<CommandPalette>>,
    browsers: NonSend<Browsers>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    browser_meta: Query<(&PageMetadata, &ChildOf), With<Browser>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    tab_q: Query<Entity, With<Tab>>,
    all_children: Query<&Children>,
    child_of_q: Query<&ChildOf>,
    content_browsers: Query<Entity, (With<Browser>, Without<Header>, Without<SideSheet>, Without<CommandPalette>)>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(BrowserCommand::FocusAddressBar) = *cmd else {
            continue;
        };

        let Ok((palette_e, mut palette_node)) = palette_q.single_mut() else {
            continue;
        };

        // Toggle: if already open, close it
        if palette_node.display != Display::None {
            palette_node.display = Display::None;
            commands.entity(palette_e).remove::<CefKeyboardTarget>();
            // Restore keyboard to active content browser
            if let Some(active_tab) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) {
                for browser_e in &content_browsers {
                    if child_of_q.get(browser_e).ok().map(|co| co.get()) == Some(active_tab) {
                        commands.entity(browser_e).insert(CefKeyboardTarget);
                    }
                }
            }
            continue;
        }

        // Open palette
        palette_node.display = Display::Flex;

        // Remove keyboard target from all content browsers
        for browser_e in &content_browsers {
            commands.entity(browser_e).remove::<CefKeyboardTarget>();
        }
        // Give keyboard to palette
        commands.entity(palette_e).insert(CefKeyboardTarget);

        // Gather current URL
        let current_url = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs)
            .and_then(|tab| {
                browser_meta.iter().find(|(_, co)| co.get() == tab).map(|(meta, _)| meta.url.clone())
            })
            .unwrap_or_default();

        // Gather all tabs
        let mut palette_tabs = Vec::new();
        for pane_e in &leaf_panes {
            let is_active_pane = active_pane.contains(pane_e);
            if let Ok(children) = pane_children.get(pane_e) {
                let mut tab_index = 0usize;
                for child in children.iter() {
                    if !tab_q.contains(child) {
                        continue;
                    }
                    let tab_is_active = active_tabs.contains(child) && is_active_pane;
                    if let Ok(tab_kids) = all_children.get(child) {
                        for browser_e in tab_kids.iter() {
                            if let Ok((meta, _)) = browser_meta.get(browser_e) {
                                palette_tabs.push(PaletteTab {
                                    title: meta.title.clone(),
                                    url: meta.url.clone(),
                                    pane_id: pane_e.to_bits(),
                                    tab_index,
                                    is_active: tab_is_active,
                                });
                            }
                        }
                    }
                    tab_index += 1;
                }
            }
        }

        // Build command list (auto-generated from #[menu] attrs via CommandPalette derive)
        let palette_commands: Vec<PaletteCommandEntry> = crate::palette::command_list()
            .into_iter()
            .map(|e| PaletteCommandEntry {
                id: e.id.into(),
                name: e.name.into(),
                shortcut: e.shortcut.into(),
            })
            .collect();

        // Send open event to palette webview
        if browsers.has_browser(palette_e) && browsers.host_emit_ready(&palette_e) {
            let payload = PaletteOpenEvent {
                url: current_url,
                tabs: palette_tabs,
                commands: palette_commands,
            };
            let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
            commands.trigger(HostEmitEvent::new(palette_e, PALETTE_OPEN_EVENT, &ron_body));
        }
    }
}

fn on_palette_action_emit(
    trigger: On<Receive<PaletteActionEvent>>,
    mut palette_q: Query<(Entity, &mut Node), With<CommandPalette>>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    content_browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>, Without<CommandPalette>)>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    tab_q: Query<Entity, With<Tab>>,
    mut messages: ResMut<bevy::ecs::message::Messages<AppCommand>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;

    match evt.action.as_str() {
        "navigate" => {
            let url = if evt.value.contains("://") {
                evt.value.clone()
            } else if evt.value.contains('.') && !evt.value.contains(' ') {
                format!("https://{}", evt.value)
            } else {
                format!("https://www.google.com/search?q={}", evt.value)
            };
            if let Some(active_tab) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) {
                for (browser_e, co) in &content_browsers {
                    if co.get() == active_tab {
                        commands.entity(browser_e).insert(WebviewSource::new(&url));
                    }
                }
            }
        }
        "command" => {
            if let Some(cmd) = crate::palette::match_command(&evt.value) {
                messages.write(cmd);
            }
        }
        "switch_tab" => {
            // value format: "pane_id:tab_index"
            if let Some((pane_bits, tab_idx)) = evt.value.split_once(':') {
                if let (Ok(pane_id), Ok(tab_index)) = (pane_bits.parse::<u64>(), tab_idx.parse::<usize>()) {
                    if let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id) {
                        // Deactivate current pane
                        if let Ok(old_pane) = active_pane.single() {
                            if old_pane != target_pane {
                                commands.entity(old_pane).remove::<Active>();
                            }
                        }
                        commands.entity(target_pane).insert(Active);

                        // Activate target tab
                        if let Ok(children) = pane_children.get(target_pane) {
                            let tabs: Vec<Entity> = children.iter().filter(|&e| tab_q.contains(e)).collect();
                            // Deactivate old active tab in this pane
                            for &t in &tabs {
                                if active_tabs.contains(t) {
                                    commands.entity(t).remove::<Active>();
                                }
                            }
                            if let Some(&target_tab) = tabs.get(tab_index) {
                                commands.entity(target_tab).insert(Active);
                            }
                        }
                    }
                }
            }
        }
        _ => {} // "dismiss" and unknown
    }

    // Close palette and restore keyboard
    if let Ok((palette_e, mut palette_node)) = palette_q.single_mut() {
        palette_node.display = Display::None;
        commands.entity(palette_e).remove::<CefKeyboardTarget>();
    }
    if let Some(active_tab) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) {
        for (browser_e, co) in &content_browsers {
            if co.get() == active_tab {
                commands.entity(browser_e).insert(CefKeyboardTarget);
            }
        }
    }
}

// command_list() and match_command() are provided by crate::palette,
// powered by the CommandPalette derive macro on AppCommand.
// See palette.rs and vmux_macro for details.
```

- [ ] **Step 2: Register the plugin in `lib.rs`**

In `crates/vmux_desktop/src/lib.rs`, add `mod command_palette;` and add the plugin. Find the existing plugin registration block and add:

```rust
mod command_palette;
```

And in the `add_plugins` call, add:

```rust
vmux_command_palette::CommandPalettePlugin,
command_palette::CommandPaletteHandlerPlugin,
```

- [ ] **Step 3: Attach palette webview to Modal entity in `window.rs`**

In `crates/vmux_desktop/src/layout/window.rs`, find the `Modal` spawn (the tuple with `Modal` marker + `Node`). Add `CommandPalette` and webview components. The spawn needs access to `meshes` and `webview_mt` resources.

Replace the Modal spawn tuple:

```rust
use vmux_command_palette::{CommandPalette, COMMAND_PALETTE_WEBVIEW_URL};
```

Change the Modal child from:

```rust
(
    Modal,
    Node { ... display: Display::None, ... },
),
```

To spawn it with a webview. Since the Modal is a child of VmuxWindow, and Browser::new() needs `&mut meshes` and `&mut webview_mt`, add those parameters to the setup system if not already present. Then:

```rust
(
    Modal,
    CommandPalette,
    WebviewSource::new(COMMAND_PALETTE_WEBVIEW_URL),
    WebviewSize(Vec2::new(600.0, 400.0)),
    Node {
        width: Val::Px(600.0),
        height: Val::Px(400.0),
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Percent(50.0),
        margin: UiRect {
            left: Val::Px(-300.0),
            top: Val::Px(-200.0),
            ..default()
        },
        display: Display::None,
        ..default()
    },
),
```

Note: the palette also needs `Mesh3d`, `MeshMaterial3d<WebviewExtendStandardMaterial>`, `Transform`, `GlobalTransform`, `Visibility`, and `Pickable` — the same components that `Browser::new()` provides. Check how Header and SideSheet are spawned in `window.rs` for the exact pattern and replicate it.

- [ ] **Step 4: Guard `sync_keyboard_target` when palette is open**

In `crates/vmux_desktop/src/browser.rs`, modify `sync_keyboard_target` to skip reassignment when the palette is visible. Add a query parameter:

```rust
palette_q: Query<&Node, With<CommandPalette>>,
```

At the top of the function, after the existing code, add:

```rust
if palette_q.iter().any(|node| node.display != Display::None) {
    return;
}
```

Add the import at the top of `browser.rs`:

```rust
use vmux_command_palette::CommandPalette;
```

- [ ] **Step 5: Remove the stub handler for `FocusAddressBar`**

In `crates/vmux_desktop/src/browser.rs`, find the match arm in `handle_browser_commands`:

```rust
BrowserCommand::FocusAddressBar => {}
```

This can remain as-is since `handle_open_palette` in `command_palette.rs` reads the same `AppCommand` independently via its own `MessageReader`.

Actually, check: `MessageReader` in Bevy 0.18 — each reader gets its own cursor, so multiple systems can read the same messages. The `handle_open_palette` system reads `AppCommand` messages and filters for `FocusAddressBar`. The existing stub in `handle_browser_commands` is harmless. Leave it.

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p vmux_desktop --lib`

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/src/command_palette.rs crates/vmux_desktop/src/lib.rs crates/vmux_desktop/src/browser.rs crates/vmux_desktop/src/layout/window.rs
git commit -m "feat: wire up command palette open/close with keyboard routing"
```

---

### Task 3: Build the Dioxus palette UI with filtering and keyboard navigation

**Files:**
- Modify: `crates/vmux_command_palette/src/app.rs`

- [ ] **Step 1: Implement the full palette UI**

Replace the placeholder `app.rs` with the complete implementation:

```rust
#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command_palette::event::{
    PaletteActionEvent, PaletteCommandEntry, PaletteOpenEvent, PaletteTab, PALETTE_OPEN_EVENT,
};
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener};

#[derive(Clone, PartialEq)]
enum ResultItem {
    Tab { title: String, url: String, pane_id: u64, tab_index: usize },
    Command { id: String, name: String, shortcut: String },
    Navigate { url: String },
}

fn filter_results(query: &str, tabs: &[PaletteTab], commands: &[PaletteCommandEntry]) -> Vec<ResultItem> {
    let q = query.trim();
    if q.is_empty() {
        let mut items: Vec<ResultItem> = tabs.iter().map(|t| ResultItem::Tab {
            title: t.title.clone(),
            url: t.url.clone(),
            pane_id: t.pane_id,
            tab_index: t.tab_index,
        }).collect();
        items.extend(commands.iter().map(|c| ResultItem::Command {
            id: c.id.clone(),
            name: c.name.clone(),
            shortcut: c.shortcut.clone(),
        }));
        return items;
    }

    let commands_only = q.starts_with('>');
    let search = if commands_only { q[1..].trim() } else { q };
    let search_lower = search.to_lowercase();

    let mut items = Vec::new();

    if !commands_only {
        for t in tabs {
            if t.title.to_lowercase().contains(&search_lower) || t.url.to_lowercase().contains(&search_lower) {
                items.push(ResultItem::Tab {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index,
                });
            }
        }
    }

    for c in commands {
        if c.name.to_lowercase().contains(&search_lower) || c.id.contains(&search_lower) {
            items.push(ResultItem::Command {
                id: c.id.clone(),
                name: c.name.clone(),
                shortcut: c.shortcut.clone(),
            });
        }
    }

    if !commands_only && !search.is_empty() {
        items.push(ResultItem::Navigate { url: search.to_string() });
    }

    items
}

fn emit_action(action: &str, value: &str) {
    let _ = try_cef_emit_serde(&PaletteActionEvent {
        action: action.to_string(),
        value: value.to_string(),
    });
}

#[component]
pub fn App() -> Element {
    let mut state = use_signal(PaletteOpenEvent::default);
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);

    let _listener = use_event_listener::<PaletteOpenEvent, _>(
        PALETTE_OPEN_EVENT,
        move |data| {
            query.set(data.url.clone());
            selected.set(0);
            state.set(data);
        },
    );

    let PaletteOpenEvent { url: _, tabs, commands } = state();
    let q = query();
    let results = filter_results(&q, &tabs, &commands);
    let sel = selected().min(results.len().saturating_sub(1));

    let execute = move |item: &ResultItem| {
        match item {
            ResultItem::Tab { pane_id, tab_index, .. } => {
                emit_action("switch_tab", &format!("{pane_id}:{tab_index}"));
            }
            ResultItem::Command { id, .. } => {
                emit_action("command", id);
            }
            ResultItem::Navigate { url } => {
                emit_action("navigate", url);
            }
        }
    };

    rsx! {
        div {
            class: "flex h-full w-full items-start justify-center bg-black/50 pt-[15%]",
            onclick: move |_| { emit_action("dismiss", ""); },
            div {
                class: "flex w-full max-w-lg flex-col rounded-xl border border-border bg-card shadow-2xl",
                onclick: move |e| { e.stop_propagation(); },
                div { class: "p-2",
                    input {
                        r#type: "text",
                        class: "w-full rounded-lg bg-muted px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground",
                        placeholder: "Type a URL, search tabs, or > for commands...",
                        value: "{q}",
                        autofocus: true,
                        oninput: move |e| {
                            query.set(e.value());
                            selected.set(0);
                        },
                        onkeydown: move |e| {
                            match e.key() {
                                Key::Escape => { emit_action("dismiss", ""); }
                                Key::ArrowDown => {
                                    let max = results.len().saturating_sub(1);
                                    selected.set((sel + 1).min(max));
                                }
                                Key::ArrowUp => {
                                    selected.set(sel.saturating_sub(1));
                                }
                                Key::Enter => {
                                    if let Some(item) = results.get(sel) {
                                        execute(item);
                                    } else if !q.is_empty() {
                                        emit_action("navigate", &q);
                                    }
                                }
                                _ => {}
                            }
                        },
                    }
                }
                if !results.is_empty() {
                    div { class: "max-h-64 overflow-y-auto border-t border-border p-1",
                        for (i, item) in results.iter().enumerate() {
                            div {
                                key: "{i}",
                                class: if i == sel { "flex cursor-pointer items-center justify-between rounded-lg bg-muted px-3 py-1.5" } else { "flex cursor-pointer items-center justify-between rounded-lg px-3 py-1.5 hover:bg-muted/50" },
                                onclick: {
                                    let item = item.clone();
                                    move |_| { execute(&item); }
                                },
                                match item {
                                    ResultItem::Tab { title, url, .. } => rsx! {
                                        div { class: "flex min-w-0 flex-col",
                                            span { class: "truncate text-sm text-foreground", "{title}" }
                                            span { class: "truncate text-xs text-muted-foreground", "{url}" }
                                        }
                                        span { class: "ml-2 shrink-0 text-xs text-muted-foreground", "Tab" }
                                    },
                                    ResultItem::Command { name, shortcut, .. } => rsx! {
                                        span { class: "text-sm text-foreground", "{name}" }
                                        span { class: "ml-2 shrink-0 rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground", "{shortcut}" }
                                    },
                                    ResultItem::Navigate { url } => rsx! {
                                        span { class: "text-sm text-foreground", "Navigate to {url}" }
                                        span { class: "ml-2 shrink-0 text-xs text-muted-foreground", "\u{21b5}" }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Verify the WASM crate compiles**

Run: `cargo check -p vmux_command_palette --lib`

(Full WASM build requires `dx build` which runs via `build.rs` during `cargo build -p vmux_desktop`.)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_command_palette/src/app.rs
git commit -m "feat: implement command palette UI with filtering and keyboard navigation"
```

---

### Task 4: Full integration build and smoke test

**Files:**
- None new — verify everything works together

- [ ] **Step 1: Full native build**

Run: `cargo build -p vmux_desktop --features debug`

This triggers `build.rs` for `vmux_command_palette`, building the Dioxus WASM app and embedding it via `vmux_webview_app`.

Fix any compilation errors.

- [ ] **Step 2: Run the app**

Run: `make run-mac`

Test:
1. Press `Cmd+L` — palette should open with current URL in the input
2. Type a URL, press Enter — should navigate
3. Type `>reload`, press Enter — should reload the page
4. Press Esc — palette should close
5. Click outside the palette card — should close
6. Arrow keys should navigate the result list
7. While palette is open, keyboard should NOT go to the content browser
8. After closing, keyboard should work in content browser again

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration fixes for command palette"
```
