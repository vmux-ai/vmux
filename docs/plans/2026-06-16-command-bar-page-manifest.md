# Command Bar Page Manifest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the command bar's hardcoded Spaces/Settings special-casing with a declarative per-page manifest that every internal page exposes and the bar suggests generically.

**Architecture:** Enrich the existing `PageManifest` (already spawned per crate) with display metadata + an opt-in flag. A collector system in `vmux_command` snapshots the command-bar pages; the snapshot ships in `CommandBarOpenEvent`; the WASM command bar matches/renders them uniformly.

**Tech Stack:** Rust, Bevy ECS, rkyv (binary IPC), Dioxus (WASM page), bevy_cef.

**Working dir:** `.worktrees/command-bar-page-manifest` (branch `command-bar-page-manifest`, off `origin/main`).

**Build note:** This workspace links CEF; native `vmux_layout`/desktop builds are heavy. Keep a warm target dir. Light crates (`vmux_core`, `vmux_command`) test fast. The WASM page is checked with a `wasm32-unknown-unknown` target build, not a native build. Per AGENTS.md: no code comments; chain consecutive `App` builder calls; never edit the main worktree.

---

### Task 1: Extend `PageManifest` with command-bar metadata

Adds `title`/`keywords`/`icon`/`command_bar` fields + a `url()` helper, and updates every `PAGE_MANIFEST` const so the workspace still compiles. One commit (the struct change breaks all call sites until they are updated together).

**Files:**
- Modify: `crates/vmux_core/src/page.rs:15-33` (struct + impl) and its tests at `:249` and `:264`
- Modify: `crates/vmux_terminal/src/lib.rs:35-36`
- Modify: `crates/vmux_setting/src/lib.rs:18-19`
- Modify: `crates/vmux_space/src/lib.rs:23-24`
- Modify: `crates/vmux_history/src/lib.rs:19-20`
- Modify: `crates/vmux_service/src/lib.rs:38-39`
- Modify: `crates/vmux_vibe_setup/src/lib.rs:9-10`
- Modify: `crates/vmux_layout/src/lib.rs:71-77`

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `crates/vmux_core/src/page.rs`:

```rust
    #[test]
    fn page_manifest_url_derives_from_host() {
        let manifest = PageManifest {
            host: "settings",
            title: "Settings",
            keywords: &["preferences"],
            icon: "settings",
            command_bar: true,
        };
        assert_eq!(manifest.url(), "vmux://settings/");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core page_manifest_url_derives_from_host`
Expected: FAIL — `PageManifest` has no fields `title/keywords/icon/command_bar` and no method `url`.

- [ ] **Step 3: Extend the struct + add `url()`**

In `crates/vmux_core/src/page.rs`, replace the struct (lines 15-18) and add `url()` inside the existing `impl PageManifest` (after `embedded_host`, before `bundle_root`):

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageManifest {
    pub host: &'static str,
    pub title: &'static str,
    pub keywords: &'static [&'static str],
    pub icon: &'static str,
    pub command_bar: bool,
}
```

```rust
    pub fn url(&self) -> String {
        let host = self.host.trim().trim_matches('/');
        format!("vmux://{host}/")
    }
```

- [ ] **Step 4: Fix the two in-file test literals**

In `crates/vmux_core/src/page.rs`, the test at `page_manifest_registers_host` (line ~249) and `registered_hosts_use_vmux_server_dist` (line ~264) construct `PageManifest { host: "history" }`. Replace both literals with:

```rust
PageManifest {
    host: "history",
    title: "History",
    keywords: &["recent", "visited"],
    icon: "clock",
    command_bar: true,
}
```

(`page_manifest_registers_host` uses `app.world_mut().spawn(PageManifest { ... });`; `registered_hosts_use_vmux_server_dist` uses `let manifest = PageManifest { ... };`. Update each in place.)

- [ ] **Step 5: Update every downstream `PAGE_MANIFEST` const**

Replace each const body with the enriched form (keep the surrounding `#[cfg(...)]` and `pub const` lines unchanged):

`crates/vmux_terminal/src/lib.rs`:
```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "terminal",
    title: "Terminal",
    keywords: &["shell", "console"],
    icon: "terminal",
    command_bar: true,
};
```

`crates/vmux_setting/src/lib.rs`:
```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "settings",
    title: "Settings",
    keywords: &["preferences", "config"],
    icon: "settings",
    command_bar: true,
};
```

`crates/vmux_space/src/lib.rs`:
```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "spaces",
    title: "Spaces",
    keywords: &["space"],
    icon: "layers",
    command_bar: true,
};
```

`crates/vmux_history/src/lib.rs`:
```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "history",
    title: "History",
    keywords: &["recent", "visited"],
    icon: "clock",
    command_bar: true,
};
```

`crates/vmux_service/src/lib.rs`:
```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "services",
    title: "Services",
    keywords: &["processes", "monitor"],
    icon: "activity",
    command_bar: true,
};
```

`crates/vmux_vibe_setup/src/lib.rs`:
```rust
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant"],
    icon: "sparkles",
    command_bar: true,
};
```

`crates/vmux_layout/src/lib.rs` (both consts — infra, hidden):
```rust
pub const LAYOUT_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "layout",
    title: "Layout",
    keywords: &[],
    icon: "",
    command_bar: false,
};
#[cfg(not(target_arch = "wasm32"))]
pub const COMMAND_BAR_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "command-bar",
    title: "Command Bar",
    keywords: &[],
    icon: "",
    command_bar: false,
};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vmux_core`
Expected: PASS (including `page_manifest_url_derives_from_host`).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_core/src/page.rs crates/vmux_terminal/src/lib.rs crates/vmux_setting/src/lib.rs crates/vmux_space/src/lib.rs crates/vmux_history/src/lib.rs crates/vmux_service/src/lib.rs crates/vmux_vibe_setup/src/lib.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(core): add command-bar metadata to PageManifest"
```

---

### Task 2: Add `CommandBarPage` wire type + `pages` field

**Files:**
- Modify: `crates/vmux_command/src/event.rs:8-31` (CommandBarOpenEvent) and add new struct after it
- Test: same file (`mod tests`)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vmux_command/src/event.rs`:

```rust
    #[test]
    fn command_bar_open_event_carries_pages() {
        let event = CommandBarOpenEvent {
            pages: vec![CommandBarPage {
                host: "settings".to_string(),
                url: "vmux://settings/".to_string(),
                title: "Settings".to_string(),
                keywords: vec!["preferences".to_string()],
                icon: "settings".to_string(),
            }],
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.pages.len(), 1);
        assert_eq!(recovered.pages[0].title, "Settings");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_command command_bar_open_event_carries_pages`
Expected: FAIL — `CommandBarPage` undefined, `pages` field missing.

- [ ] **Step 3: Add the struct + field**

In `crates/vmux_command/src/event.rs`, add `pages` to `CommandBarOpenEvent` (after the `commands` field, before `target`):

```rust
    pub commands: Vec<CommandBarCommandEntry>,
    #[serde(default)]
    pub pages: Vec<CommandBarPage>,
    pub target: Option<crate::open_target::OpenTarget>,
```

Add the new struct immediately after the `CommandBarOpenEvent` definition:

```rust
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
pub struct CommandBarPage {
    pub host: String,
    pub url: String,
    pub title: String,
    pub keywords: Vec<String>,
    pub icon: String,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_command command_bar_open_event_carries_pages`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_command/src/event.rs
git commit -m "feat(command): add CommandBarPage wire type and pages field"
```

---

### Task 3: Collect command-bar pages into a snapshot

A system queries every `PageManifest`, keeps the opt-in ones, and writes `CommandBarPagesSnapshot`. Registered in `CommandPlugin` under `WriteCommandBarSnapshots`.

**Files:**
- Modify: `crates/vmux_command/src/snapshot.rs:1-3` (imports), add resource + system + test
- Modify: `crates/vmux_command/src/plugin.rs:4-7,13-21` (import + init + system)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vmux_command/src/snapshot.rs`:

```rust
    #[test]
    fn pages_snapshot_collects_only_command_bar_pages() {
        let mut app = App::new();
        app.init_resource::<CommandBarPagesSnapshot>()
            .add_systems(Update, update_pages_snapshot);
        app.world_mut().spawn(PageManifest {
            host: "settings",
            title: "Settings",
            keywords: &["preferences"],
            icon: "settings",
            command_bar: true,
        });
        app.world_mut().spawn(PageManifest {
            host: "layout",
            title: "Layout",
            keywords: &[],
            icon: "",
            command_bar: false,
        });

        app.update();

        let snap = app.world().resource::<CommandBarPagesSnapshot>();
        assert_eq!(snap.pages.len(), 1);
        assert_eq!(snap.pages[0].host, "settings");
        assert_eq!(snap.pages[0].url, "vmux://settings/");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_command pages_snapshot_collects_only_command_bar_pages`
Expected: FAIL — `CommandBarPagesSnapshot`/`update_pages_snapshot`/`PageManifest` not in scope.

- [ ] **Step 3: Add imports, resource, and collector**

At the top of `crates/vmux_command/src/snapshot.rs`, add after the existing `use` lines:

```rust
use crate::event::CommandBarPage;
use vmux_core::page::PageManifest;
```

Append the resource + system (before the `#[cfg(test)] mod tests` block):

```rust
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarPagesSnapshot {
    pub pages: Vec<CommandBarPage>,
}

pub fn update_pages_snapshot(
    manifests: Query<&PageManifest>,
    mut snapshot: ResMut<CommandBarPagesSnapshot>,
) {
    if !snapshot.pages.is_empty() {
        return;
    }
    let mut pages: Vec<CommandBarPage> = manifests
        .iter()
        .filter(|manifest| manifest.command_bar)
        .map(|manifest| CommandBarPage {
            host: manifest.host.to_string(),
            url: manifest.url(),
            title: manifest.title.to_string(),
            keywords: manifest.keywords.iter().map(|k| k.to_string()).collect(),
            icon: manifest.icon.to_string(),
        })
        .collect();
    pages.sort_by(|a, b| a.title.cmp(&b.title));
    snapshot.pages = pages;
}
```

- [ ] **Step 4: Wire into `CommandPlugin`**

In `crates/vmux_command/src/plugin.rs`, extend the snapshot import (lines 4-7) to include the new items:

```rust
use crate::snapshot::{
    CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSettingsSnapshot,
    CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot, WriteCommandBarSnapshots,
    update_pages_snapshot,
};
```

In `impl Plugin for CommandPlugin`, add the init + system to the existing builder chain (after `.init_resource::<CommandBarTerminalsSnapshot>()`):

```rust
            .init_resource::<CommandBarPagesSnapshot>()
            .add_systems(
                Update,
                update_pages_snapshot.in_set(WriteCommandBarSnapshots),
            )
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vmux_command`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_command/src/snapshot.rs crates/vmux_command/src/plugin.rs
git commit -m "feat(command): collect command-bar pages into a snapshot"
```

---

### Task 4: Generic page matching + uniform rendering

Replaces the hardcoded Spaces/Settings logic with a `Page` result variant matched generically and rendered with a per-page icon. `results.rs` (native logic + tests) and the WASM `page.rs` change together so both the native and wasm builds stay green.

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/results.rs` (imports, enum, helpers, `filter_results`, tests — full rewrite of the matching region)
- Modify: `crates/vmux_layout/src/command_bar/page.rs` (import, destructure, `display_text`, icon detection, `execute`, results row, add `page_icon`)

- [ ] **Step 1: Write the failing tests**

In `crates/vmux_layout/src/command_bar/results.rs`, add a `sample_pages()` helper and new tests inside `mod tests` (keep the existing `space()` helper):

```rust
    fn sample_pages() -> Vec<CommandBarPage> {
        vec![
            CommandBarPage {
                host: "settings".into(),
                url: "vmux://settings/".into(),
                title: "Settings".into(),
                keywords: vec!["preferences".into()],
                icon: "settings".into(),
            },
            CommandBarPage {
                host: "spaces".into(),
                url: "vmux://spaces/".into(),
                title: "Spaces".into(),
                keywords: vec!["space".into()],
                icon: "layers".into(),
            },
            CommandBarPage {
                host: "history".into(),
                url: "vmux://history/".into(),
                title: "History".into(),
                keywords: vec!["recent".into()],
                icon: "clock".into(),
            },
        ]
    }

    #[test]
    fn page_matched_by_keyword() {
        let results = filter_results("preferences", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://settings/".into(),
            title: "Settings".into(),
            icon: "settings".into(),
        }));
    }

    #[test]
    fn settings_page_reachable_by_name() {
        let results = filter_results("setti", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(results.iter().any(|r| matches!(
            r,
            CommandBarResultItem::Page { title, .. } if title == "Settings"
        )));
    }

    #[test]
    fn empty_query_has_no_pages() {
        let results = filter_results("", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(!results.iter().any(|r| matches!(r, CommandBarResultItem::Page { .. })));
    }

    #[test]
    fn command_prefix_excludes_pages() {
        let results = filter_results("> set", &[], &[], &[], &sample_pages(), false, &[]);
        assert!(!results.iter().any(|r| matches!(r, CommandBarResultItem::Page { .. })));
    }
```

Update the four existing tests to pass `&sample_pages()` (the new 5th arg, after `spaces`) and assert on `Page` instead of the removed `Navigate { url: SPACES_PAGE_URL }`:

```rust
    #[test]
    fn spaces_url_lists_all_spaces() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("work", "Work", true),
        ];

        let results = filter_results("vmux://spaces/", &[], &[], &spaces, &sample_pages(), false, &[]);

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: "layers".into(),
        }));
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "space-1"
        )));
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "work"
        )));
    }

    #[test]
    fn spaces_url_includes_normal_commands() {
        let commands = vec![CommandBarCommandEntry {
            id: "browser_open_command_bar".to_string(),
            name: "Command Bar".to_string(),
            shortcut: "super+k".to_string(),
        }];

        let results = filter_results("vmux://spaces/", &[], &commands, &[], &sample_pages(), false, &[]);

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: "layers".into(),
        }));
        assert!(results.contains(&CommandBarResultItem::Command {
            id: "browser_open_command_bar".to_string(),
            name: "Command Bar".to_string(),
            shortcut: "super+k".to_string(),
        }));
    }

    #[test]
    fn spaces_query_includes_spaces_page_and_command() {
        let commands = vec![CommandBarCommandEntry {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }];

        let results = filter_results("spaces", &[], &commands, &[], &sample_pages(), false, &[]);

        assert!(results.contains(&CommandBarResultItem::Page {
            url: "vmux://spaces/".into(),
            title: "Spaces".into(),
            icon: "layers".into(),
        }));
        assert!(results.contains(&CommandBarResultItem::Command {
            id: "space_open".to_string(),
            name: "Spaces".to_string(),
            shortcut: "<leader> s".to_string(),
        }));
    }

    #[test]
    fn space_names_are_searchable() {
        let spaces = vec![
            space("space-1", "Space 1", false),
            space("client", "Client Work", false),
        ];
        let tabs: Vec<CommandBarTab> = Vec::new();

        let results = filter_results("client", &tabs, &[], &spaces, &sample_pages(), false, &[]);

        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::Space { id, .. } if id == "client"
        )));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_layout --lib command_bar::results`
Expected: FAIL — `filter_results` arity is wrong, `CommandBarPage`/`Page` variant unknown.

- [ ] **Step 3: Update imports + enum (results.rs)**

Replace the import (line 1) and delete the URL consts (lines 3-9):

```rust
use vmux_command::event::{
    CommandBarCommandEntry, CommandBarPage, CommandBarSpace, CommandBarTab, HistoryEntry,
};
```

Add a `Page` variant to `CommandBarResultItem` (after `Command { .. }`, before `Navigate`):

```rust
    Page {
        url: String,
        title: String,
        icon: String,
    },
```

- [ ] **Step 4: Replace helpers (results.rs)**

Delete `space_query`, `settings_query`, `spaces_page_matches`, `settings_page_matches`, and `space_results`. Keep `looks_like_path`, `space_result`, `space_matches`, `command_results`. Add:

```rust
fn page_matches(page: &CommandBarPage, search_lower: &str) -> bool {
    search_lower.is_empty()
        || page.title.to_lowercase().contains(search_lower)
        || page.url.to_lowercase().contains(search_lower)
        || page
            .keywords
            .iter()
            .any(|k| k.to_lowercase().contains(search_lower))
}

fn page_results(pages: &[CommandBarPage], search_lower: &str) -> Vec<CommandBarResultItem> {
    pages
        .iter()
        .filter(|page| page_matches(page, search_lower))
        .map(|page| CommandBarResultItem::Page {
            url: page.url.clone(),
            title: page.title.clone(),
            icon: page.icon.clone(),
        })
        .collect()
}

fn space_list_items(spaces: &[CommandBarSpace], search_lower: &str) -> Vec<CommandBarResultItem> {
    spaces
        .iter()
        .filter(|space| space_matches(space, search_lower))
        .map(space_result)
        .collect()
}

fn query_targets_spaces_page(q: &str, pages: &[CommandBarPage]) -> bool {
    let Some(url) = pages.iter().find(|p| p.host == "spaces").map(|p| p.url.as_str()) else {
        return false;
    };
    q == url || q == url.trim_end_matches('/') || q.starts_with(url)
}
```

- [ ] **Step 5: Rewrite `filter_results` (results.rs)**

Replace the entire `filter_results` function with:

```rust
pub fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    spaces: &[CommandBarSpace],
    pages: &[CommandBarPage],
    new_tab: bool,
    history: &[HistoryEntry],
) -> Vec<CommandBarResultItem> {
    let q = query.trim();

    if query_targets_spaces_page(q, pages) {
        let mut items = page_results(pages, &q.to_lowercase());
        items.extend(space_list_items(spaces, ""));
        items.extend(command_results(commands));
        return items;
    }

    if q.is_empty() {
        let mut items: Vec<CommandBarResultItem> = Vec::new();
        items.push(CommandBarResultItem::Navigate { url: String::new() });
        if new_tab {
            items.push(CommandBarResultItem::Terminal {
                path: String::new(),
            });
        }
        items.extend(tabs.iter().map(|t| CommandBarResultItem::Stack {
            title: t.title.clone(),
            url: t.url.clone(),
            pane_id: t.pane_id,
            tab_index: t.tab_index as usize,
        }));
        items.extend(command_results(commands));
        return items;
    }

    let starts_with_cmd = q.starts_with('>');
    let search = if starts_with_cmd { q[1..].trim() } else { q };
    let search_lower = search.to_lowercase();

    let mut items = Vec::new();

    let is_path = looks_like_path(search);

    if !starts_with_cmd && is_path {
        items.push(CommandBarResultItem::Terminal {
            path: search.to_string(),
        });
    }

    if !starts_with_cmd && !is_path && new_tab && "terminal".contains(&search_lower) {
        items.push(CommandBarResultItem::Terminal {
            path: String::new(),
        });
    }

    if starts_with_cmd {
        for c in commands {
            if search.is_empty()
                || c.name.to_lowercase().contains(&search_lower)
                || c.id.contains(&search_lower)
            {
                items.push(CommandBarResultItem::Command {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    shortcut: c.shortcut.clone(),
                });
            }
        }
    }

    if !starts_with_cmd && !is_path {
        items.extend(page_results(pages, &search_lower));
        items.extend(space_list_items(spaces, &search_lower));
    }

    if !starts_with_cmd || !search.is_empty() {
        for t in tabs {
            if search.is_empty()
                || t.title.to_lowercase().contains(&search_lower)
                || t.url.to_lowercase().contains(&search_lower)
            {
                items.push(CommandBarResultItem::Stack {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index as usize,
                });
            }
        }
    }

    if !starts_with_cmd {
        for h in history.iter().take(5) {
            items.push(CommandBarResultItem::History {
                url: h.url.clone(),
                title: h.title.clone(),
                favicon_url: h.favicon_url.clone(),
                visit_count: h.visit_count,
                last_visited_at: h.last_visited_at,
            });
        }
    }

    if !starts_with_cmd {
        for c in commands {
            if c.name.to_lowercase().contains(&search_lower) || c.id.contains(&search_lower) {
                items.push(CommandBarResultItem::Command {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    shortcut: c.shortcut.clone(),
                });
            }
        }
    }

    if !search.is_empty() {
        items.push(CommandBarResultItem::Navigate {
            url: search.to_string(),
        });
    }

    items
}
```

- [ ] **Step 6: Run results tests to verify they pass**

Run: `cargo test -p vmux_layout --lib command_bar::results`
Expected: PASS.

- [ ] **Step 7: Update the WASM command bar (page.rs)**

In `crates/vmux_layout/src/command_bar/page.rs`:

(a) Drop the removed consts from the import (line 7-9). New form:

```rust
use crate::command_bar::results::{CommandBarResultItem as ResultItem, filter_results};
```

(b) Destructure `pages` and pass it to `filter_results`. In the `let CommandBarOpenEvent { .. } = state();` block add `pages,` before `..`; then change the `filter_results` call to:

```rust
        let mut r = filter_results(&q, &tabs, &commands, &spaces, &pages, is_new_tab, &history);
```

(c) `display_text` match (nav mode) — add a `Page` arm before `None`:

```rust
            Some(ResultItem::Page { title, .. }) => title.clone(),
```

(d) Input-row icon detection (the `match &active_item` inside the `nav` branch) — add a `Page` arm:

```rust
                                    Some(ResultItem::Page { .. }) => (false, false, false),
```

(e) `execute` closure — add a `Page` arm:

```rust
            ResultItem::Page { url, .. } => {
                if !url.is_empty() {
                    emit_action_with_target("open", url, open_target);
                }
            }
```

(f) Results-list row `match item` — add a `Page` arm and simplify the `Navigate` arm (remove the SPACES/SETTINGS branches). Page arm:

```rust
                                    ResultItem::Page { url, title, icon } => rsx! {
                                        div { class: result_content_row_class(),
                                            {page_icon(icon)}
                                            div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                                span { class: result_primary_text_class(), "{title}" }
                                                span { class: result_secondary_text_class(), "{url}" }
                                            }
                                        }
                                        span { class: result_trailing_slot_class(), "New tab" }
                                    },
```

Replace the whole `ResultItem::Navigate { url } => rsx! { .. }` arm with:

```rust
                                    ResultItem::Navigate { url } => rsx! {
                                        div { class: result_content_row_class(),
                                            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                                circle { cx: "11", cy: "11", r: "8" }
                                                path { d: "m21 21-4.3-4.3" }
                                            }
                                            if url.is_empty() {
                                                span { class: "text-base text-foreground", "Search" }
                                            } else if looks_like_url(url) {
                                                span { class: result_primary_text_class(), "Open \"{url}\"" }
                                            } else {
                                                span { class: result_primary_text_class(), "Search \"{url}\"" }
                                            }
                                        }
                                        if !url.is_empty() {
                                            span { class: result_trailing_slot_class(), "\u{21b5}" }
                                        } else {
                                            span { class: result_trailing_slot_class() }
                                        }
                                    },
```

(g) Add the `page_icon` helper near the other free functions (e.g. after `looks_like_path`):

```rust
fn page_icon(icon: &str) -> Element {
    let icon_class = "h-4 w-4 shrink-0 text-muted-foreground";
    match icon {
        "settings" => rsx! { Icon { class: icon_class,
            circle { cx: "12", cy: "12", r: "3" }
            path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" }
        } },
        "layers" => rsx! { Icon { class: icon_class,
            path { d: "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z" }
            path { d: "m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65" }
            path { d: "m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65" }
        } },
        "clock" => rsx! { Icon { class: icon_class,
            circle { cx: "12", cy: "12", r: "10" }
            path { d: "M12 6v6l4 2" }
        } },
        "activity" => rsx! { Icon { class: icon_class,
            path { d: "M22 12h-4l-3 9L9 3l-3 9H2" }
        } },
        "sparkles" => rsx! { Icon { class: icon_class,
            path { d: "m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" }
        } },
        "terminal" => rsx! { Icon { class: icon_class,
            path { d: "m4 17 6-6-6-6" }
            path { d: "M12 19h8" }
        } },
        _ => rsx! { Icon { class: icon_class,
            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
        } },
    }
}
```

- [ ] **Step 8: Verify the wasm build compiles**

Run: `cargo check -p vmux_layout --target wasm32-unknown-unknown`
Expected: PASS (all `ResultItem` matches exhaustive; `page_icon` resolves). If `vmux_layout` cannot check standalone for wasm in this workspace, fall back to `cargo check -p vmux_server --target wasm32-unknown-unknown --no-default-features --features web`.

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_layout/src/command_bar/results.rs crates/vmux_layout/src/command_bar/page.rs
git commit -m "feat(command-bar): suggest pages generically from manifests"
```

---

### Task 5: Ship pages from the handler into the open payload

Reads `CommandBarPagesSnapshot` and threads it into `CommandBarOpenEvent.pages` so the WASM bar receives real page suggestions.

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/handler.rs` (imports ~17-27, `handle_open_command_bar` params/body, `command_bar_open_payload`, two payload tests)

- [ ] **Step 1: Add imports**

Add `CommandBarPage` to the `vmux_command::event` import group, and `CommandBarPagesSnapshot` to the `vmux_command::snapshot` import group:

```rust
use vmux_command::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent,
    CommandBarPage, CommandBarReadyEvent, CommandBarRenderedEvent, CommandBarSizeEvent,
    CommandBarSpace, CommandBarTab, PATH_COMPLETE_RESPONSE, PathCompleteRequest,
    PathCompleteResponse, PathEntry,
};
use vmux_command::snapshot::{
    AgentProviderSummary, CommandBarAgentsSnapshot, CommandBarPagesSnapshot,
    CommandBarSettingsSnapshot, CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot,
};
```

- [ ] **Step 2: Add the snapshot to `handle_open_command_bar`**

In the `snapshot_params: ParamSet<( .. )>` declaration, add a new member as `p5`:

```rust
    mut snapshot_params: ParamSet<(
        Res<CommandBarAgentsSnapshot>,
        Res<CommandBarSpacesSnapshot>,
        ResMut<NewStackContext>,
        Option<Res<crate::settings::EffectiveStartupUrl>>,
        MessageWriter<PageOpenRequest>,
        Res<CommandBarPagesSnapshot>,
    )>,
```

After the existing `let startup_url = snapshot_params.p3().map(|url| url.0.clone());` line, add:

```rust
    let pages = snapshot_params.p5().pages.clone();
```

- [ ] **Step 3: Pass `pages` into the payload builder**

At the `command_bar_open_payload( .. )` call, add `pages` as the final argument (after `target`):

```rust
    let payload = command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        current_url,
        bar_spaces,
        bar_tabs,
        bar_commands,
        target,
        pages,
    );
```

- [ ] **Step 4: Extend `command_bar_open_payload`**

Add the parameter and set the field:

```rust
fn command_bar_open_payload(
    open_id: u64,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces: Vec<CommandBarSpace>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    target: Option<vmux_command::open_target::OpenTarget>,
    pages: Vec<CommandBarPage>,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        native_windowed,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        pages,
        target,
    }
}
```

- [ ] **Step 5: Fix the two payload tests**

The tests `command_bar_payload_includes_space_name` and `command_bar_payload_includes_spaces` call `command_bar_open_payload(...)` ending in `None,`. Add `Vec::new()` as the final argument to each call:

```rust
        let payload = command_bar_open_payload(
            7,
            false,
            "Work".to_string(),
            "https://example.com".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
        );
```

```rust
        let payload = command_bar_open_payload(
            8,
            true,
            "Work".to_string(),
            "vmux://spaces/".to_string(),
            spaces.clone(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
        );
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vmux_layout --lib command_bar::handler`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_layout/src/command_bar/handler.rs
git commit -m "feat(command-bar): send page suggestions in the open payload"
```

---

### Task 6: Remove the redundant settings snapshot

`CommandBarSettingsSnapshot` only carried `settings_page_url`, now superseded by the page manifest. Removing it touches `vmux_command`, `vmux_setting`, and the handler's action `ParamSet` (whose members renumber). All in one commit so every crate compiles.

**Files:**
- Modify: `crates/vmux_command/src/snapshot.rs` (delete struct)
- Modify: `crates/vmux_command/src/plugin.rs` (drop import + init)
- Delete: `crates/vmux_setting/src/snapshot_updater.rs`
- Modify: `crates/vmux_setting/src/lib.rs` (drop `pub mod snapshot_updater;`)
- Modify: `crates/vmux_setting/src/plugin.rs` (drop the `update_settings_snapshot` system)
- Modify: `crates/vmux_layout/src/command_bar/handler.rs` (drop import + the action `ParamSet` member, renumber `.pN()`)

- [ ] **Step 1: Confirm the field is unused**

Run: `rg -n "CommandBarSettingsSnapshot|settings_page_url|update_settings_snapshot" crates/`
Expected: references only in the files listed above (no read of `settings_page_url` outside the snapshot updater / struct). If a real read exists, stop and keep the struct, removing only the updater — but per exploration the only `resource_params` reads in `on_command_bar_action` are `.p2()` (terminals) and `.p3()` (agents); `p1()` (settings) is never read.

- [ ] **Step 2: Delete the struct (vmux_command/snapshot.rs)**

Remove the `CommandBarSettingsSnapshot` definition:

```rust
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarSettingsSnapshot {
    pub settings_page_url: String,
}
```

- [ ] **Step 3: Drop it from `CommandPlugin` (vmux_command/plugin.rs)**

Remove `CommandBarSettingsSnapshot` from the `use crate::snapshot::{ .. }` import and remove the line:

```rust
            .init_resource::<CommandBarSettingsSnapshot>()
```

- [ ] **Step 4: Remove the settings updater (vmux_setting)**

Delete the file:

```bash
git rm crates/vmux_setting/src/snapshot_updater.rs
```

In `crates/vmux_setting/src/lib.rs`, delete:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot_updater;
```

In `crates/vmux_setting/src/plugin.rs`, delete the registered system block:

```rust
            .add_systems(
                Update,
                crate::snapshot_updater::update_settings_snapshot
                    .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
            )
```

- [ ] **Step 5: Renumber the action `ParamSet` (handler.rs)**

In `on_command_bar_action`, remove the settings member from `resource_params` so it becomes:

```rust
    mut resource_params: ParamSet<(
        Res<CommandBarSpacesSnapshot>,
        Res<CommandBarTerminalsSnapshot>,
        Res<CommandBarAgentsSnapshot>,
    )>,
```

Update the two reads: terminals moves to `p1`, agents to `p2`.

- `let terminals_snapshot = resource_params.p2().clone();` → `resource_params.p1().clone();`
- `} else if let Some(url) = resource_params` … `.p3()` … → `.p2()`

Remove `CommandBarSettingsSnapshot` from the handler's `vmux_command::snapshot` import (added back-reference from Task 5 leaves the rest intact).

- [ ] **Step 6: Build the affected crates**

Run: `cargo test -p vmux_command -p vmux_setting && cargo build -p vmux_layout`
Expected: PASS / clean build (no references to `CommandBarSettingsSnapshot`).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_command/src/snapshot.rs crates/vmux_command/src/plugin.rs crates/vmux_setting/src/lib.rs crates/vmux_setting/src/plugin.rs crates/vmux_layout/src/command_bar/handler.rs
git commit -m "refactor(command): drop redundant settings page-url snapshot"
```

---

## Self-Review

- **Spec coverage:**
  - "Kill hardcoded special-casing" → Task 4 (delete consts + `*_query`/`*_matches`), Task 6 (delete settings snapshot).
  - "Surface more pages" → Task 1 (`command_bar: true` for terminal/history/services/agent), Task 3 (collector), Task 4 (`page_results`).
  - "Per-page opt-in control" → Task 1 (`command_bar` flag), Task 3 (filter).
  - "Consistent rendering" → Task 4 (single `Page` arm + `page_icon`).
  - Empty-query parity, spaces expansion preserved → Task 4 (`empty_query_has_no_pages`, `query_targets_spaces_page`).
- **Placeholder scan:** none — every step has concrete code/commands.
- **Type consistency:** `CommandBarPage { host, url, title, keywords, icon }` is identical across Task 2 (def), Task 3 (collector), Task 4 (matching/tests), Task 5 (payload). `CommandBarResultItem::Page { url, title, icon }` is identical in results.rs and page.rs. `filter_results` 7-arg order (`query, tabs, commands, spaces, pages, new_tab, history`) matches the page.rs call. `command_bar_open_payload` 9-arg order matches its call site and both tests.

## Manual verification (after Task 6)

Build + run the desktop app (heavy CEF build): `make run` (or the project's usual run target). Open the command bar (Cmd+K), type `s` → Settings/Spaces/Services rows appear with their icons; type `hist` → History; type `vmux://spaces/` → Spaces page row + the full space list. Confirm `layout`/`command-bar` never appear. Selecting a page row opens that page.

## Cleanup

Per AGENTS.md, delete this plan file once fully implemented:

```bash
git rm docs/plans/2026-06-16-command-bar-page-manifest.md
git commit -m "chore: remove implemented command-bar page manifest plan"
```

## Execution Handoff

**Plan complete and saved to `docs/plans/2026-06-16-command-bar-page-manifest.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration. NOTE: vmux's CEF builds are large and long-running agents have historically dropped IPC sockets, so for this repo prefer inline execution with a warm target dir.

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints for review.

**Which approach?**
