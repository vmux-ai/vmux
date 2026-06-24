# Global Git Footer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the repo-wide commit bar (commit message · Commit · Push · branch · ahead/behind · error) out of the editor page into a global footer that reserves a row beneath all panes, spanning the same horizontal extent as the header, shown only when the active pane's repo has changes.

**Architecture:** The layout overlay page (`vmux_layout` WASM) derives a repo path from the active stack's `file:` URL, drives git status via the existing webview-routed git backend, renders the reused `GitFooter` as a bottom-anchored island, and emits a new `FooterStateRequest{open}` to Bevy. Bevy reserves/releases a `Footer` flex node in `main_column` (after `Main`) so panes reflow. No git-backend changes.

**Tech Stack:** Rust, Bevy 0.19-rc UI, `bevy_cef` bin events (rkyv), Dioxus (WASM pages).

---

## File Structure

- `crates/vmux_layout/src/event.rs` — add `FOOTER_HEIGHT_PX`, `FooterStateRequest`, pure helpers `file_url_to_path` / `should_show_footer` (+ private `percent_decode_path`). Compiled both targets → native-testable.
- `crates/vmux_layout/src/footer.rs` — **new** (native only). `Footer` component, `FooterLayoutPlugin` (`for_hosts(["layout"])` emitter + `on_footer_state_emit` observer + `sync_footer_visibility`). Mirrors `header.rs`.
- `crates/vmux_layout/src/lib.rs` — register `mod footer;` + `pub use footer::Footer;` (native section).
- `crates/vmux_layout/src/plugin.rs` — add `FooterLayoutPlugin` to `LayoutPlugin`.
- `crates/vmux_layout/src/window.rs` — spawn the `Footer` node in `main_column` after `Main`, starting collapsed.
- `crates/vmux_layout/Cargo.toml` — add `vmux_git` under the wasm target deps.
- `crates/vmux_layout/src/page.rs` — **new** `FooterView` component (WASM); mount it in the overlay.
- `crates/vmux_editor/src/page.rs` — remove the `GitFooter` usage + import (keep `GitBar`, `DiffView`).

---

## Task 1: Footer events + pure helpers (`event.rs`)

**Files:**
- Modify: `crates/vmux_layout/src/event.rs` (add const near `HEADER_HEIGHT_PX` ~line 124; add struct near `HeaderCommandEvent` ~line 271; add fns + tests in the existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add inside the existing `#[cfg(test)] mod tests { ... }` in `event.rs`:

```rust
    #[test]
    fn file_url_to_path_decodes_file_scheme() {
        assert_eq!(
            file_url_to_path("file:///Users/x/a.rs").as_deref(),
            Some("/Users/x/a.rs")
        );
        assert_eq!(
            file_url_to_path("file:///Users/x/a%20b.rs").as_deref(),
            Some("/Users/x/a b.rs")
        );
    }

    #[test]
    fn file_url_to_path_rejects_non_file() {
        assert_eq!(file_url_to_path("https://example.com"), None);
        assert_eq!(file_url_to_path("vmux://terminal/1"), None);
        assert_eq!(file_url_to_path(""), None);
    }

    #[test]
    fn should_show_footer_truth_table() {
        assert!(!should_show_footer(0, 0, false));
        assert!(should_show_footer(1, 0, false));
        assert!(should_show_footer(0, 1, false));
        assert!(should_show_footer(0, 0, true));
    }

    #[test]
    fn footer_state_request_rkyv_roundtrip() {
        let original = FooterStateRequest { open: true };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let recovered =
            rkyv::from_bytes::<FooterStateRequest, rkyv::rancor::Error>(&bytes).expect("de");
        assert!(recovered.open);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_layout --lib event:: 2>&1 | tail -20`
Expected: FAIL — `cannot find function file_url_to_path` / `cannot find ... FooterStateRequest`.

- [ ] **Step 3: Add the const, event, and helpers**

In `event.rs`, after `pub const HEADER_HEIGHT_PX: f32 = 84.0;` (line 124) add:

```rust
pub const FOOTER_HEIGHT_PX: f32 = 28.0;
```

After the `HeaderCommandEvent` struct (ends ~line 282) add:

```rust
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FooterStateRequest {
    pub open: bool,
}

pub fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    Some(percent_decode_path(rest))
}

fn percent_decode_path(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn should_show_footer(staged_count: u32, ahead: u32, has_error: bool) -> bool {
    staged_count > 0 || ahead > 0 || has_error
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout --lib event:: 2>&1 | tail -20`
Expected: PASS (all four new tests + existing event tests).

- [ ] **Step 5: Commit**

```bash
cd .worktrees/global-git-footer
git add crates/vmux_layout/src/event.rs
git commit -m "feat(layout): footer state event + path/visibility helpers"
```

---

## Task 2: Bevy `Footer` reservation module (`footer.rs`)

**Files:**
- Create: `crates/vmux_layout/src/footer.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (add `mod footer;` + `pub use footer::Footer;` in the native section)
- Modify: `crates/vmux_layout/src/plugin.rs` (register `FooterLayoutPlugin`)

- [ ] **Step 1: Create `footer.rs` with the failing tests**

Create `crates/vmux_layout/src/footer.rs`:

```rust
use crate::Open;
use crate::event::{FOOTER_HEIGHT_PX, FooterStateRequest};
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

#[derive(Component)]
pub struct Footer;

pub(crate) struct FooterLayoutPlugin;

impl Plugin for FooterLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(FooterStateRequest,)>::for_hosts(&[
            "layout",
        ]))
        .add_observer(on_footer_state_emit)
        .add_systems(PostUpdate, sync_footer_visibility.before(UiSystems::Layout));
    }
}

fn on_footer_state_emit(
    trigger: On<BinReceive<FooterStateRequest>>,
    footer_q: Query<Entity, With<Footer>>,
    mut commands: Commands,
) {
    let open = trigger.event().payload.open;
    for entity in &footer_q {
        if open {
            commands.entity(entity).insert(Open);
        } else {
            commands.entity(entity).remove::<Open>();
        }
    }
}

fn sync_footer_visibility(
    mut footer_q: Query<(&mut Visibility, &mut Node), With<Footer>>,
    added: Query<Entity, (With<Footer>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    for entity in &added {
        if let Ok((mut vis, mut node)) = footer_q.get_mut(entity) {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.height = Val::Px(FOOTER_HEIGHT_PX);
        }
    }

    for entity in removed.read() {
        if let Ok((mut vis, mut node)) = footer_q.get_mut(entity) {
            *vis = Visibility::Hidden;
            node.display = Display::None;
            node.height = Val::Px(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_reserves_then_collapses_on_open_toggle() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PostUpdate, sync_footer_visibility);
        let footer = app
            .world_mut()
            .spawn((
                Footer,
                Visibility::Hidden,
                Node {
                    height: Val::Px(0.0),
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.world_mut().entity_mut(footer).insert(Open);
        app.update();
        assert_eq!(
            app.world().get::<Node>(footer).unwrap().height,
            Val::Px(FOOTER_HEIGHT_PX)
        );

        app.world_mut().entity_mut(footer).remove::<Open>();
        app.update();
        assert_eq!(app.world().get::<Node>(footer).unwrap().height, Val::Px(0.0));
    }

    #[test]
    fn footer_state_request_toggles_open() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, FooterLayoutPlugin));
        let footer = app
            .world_mut()
            .spawn((
                Footer,
                Visibility::Hidden,
                Node {
                    height: Val::Px(0.0),
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.world_mut().trigger(BinReceive::<FooterStateRequest> {
            webview: Entity::PLACEHOLDER,
            payload: FooterStateRequest { open: true },
        });
        app.world_mut().flush();
        assert!(app.world().entity(footer).contains::<Open>());

        app.world_mut().trigger(BinReceive::<FooterStateRequest> {
            webview: Entity::PLACEHOLDER,
            payload: FooterStateRequest { open: false },
        });
        app.world_mut().flush();
        assert!(!app.world().entity(footer).contains::<Open>());
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`**

In `crates/vmux_layout/src/lib.rs`, after the `mod header;` block (lines 26-27):

```rust
#[cfg(not(target_arch = "wasm32"))]
mod footer;
```

After `pub use header::Header;` (line 76):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use footer::Footer;
```

- [ ] **Step 3: Register the plugin in `plugin.rs`**

In `crates/vmux_layout/src/plugin.rs`, after `use crate::header::HeaderLayoutPlugin;` (line 8):

```rust
use crate::footer::FooterLayoutPlugin;
```

In the `add_plugins((...))` tuple, after `HeaderLayoutPlugin,` (line 82):

```rust
                FooterLayoutPlugin,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout --lib footer:: 2>&1 | tail -20`
Expected: PASS (`sync_reserves_then_collapses_on_open_toggle`, `footer_state_request_toggles_open`).

- [ ] **Step 5: Commit**

```bash
cd .worktrees/global-git-footer
git add crates/vmux_layout/src/footer.rs crates/vmux_layout/src/lib.rs crates/vmux_layout/src/plugin.rs
git commit -m "feat(layout): Footer node reservation plugin"
```

---

## Task 3: Spawn the `Footer` node in the layout tree (`window.rs`)

**Files:**
- Modify: `crates/vmux_layout/src/window.rs` (spawn `Footer` after `Main`, ~line 354; add a source-wiring test in the existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing wiring test**

Add to `window.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn window_reserves_collapsed_footer_after_main() {
        let src = include_str!("window.rs");
        assert!(src.contains("crate::Footer"), "Footer node must be spawned");
        let footer_pos = src.find("crate::Footer").unwrap();
        let main_pos = src.find("        Main,\n").expect("Main spawn present");
        assert!(footer_pos > main_pos, "Footer must be spawned after Main");
        let footer_tail = &src[footer_pos..];
        assert!(
            footer_tail.contains("Display::None"),
            "Footer must start collapsed"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout --lib window::tests::window_reserves_collapsed_footer_after_main 2>&1 | tail -20`
Expected: FAIL — `Footer node must be spawned`.

- [ ] **Step 3: Spawn the Footer node**

In `crates/vmux_layout/src/window.rs`, immediately after the `Main` spawn block (the `commands.spawn(( Main, ... ChildOf(main_column), ));` ending ~line 354) add:

```rust
    commands.spawn((
        crate::Footer,
        ZIndex(1),
        Visibility::Hidden,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            height: Val::Px(0.0),
            display: Display::None,
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(main_column),
    ));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout --lib window:: 2>&1 | tail -20`
Expected: PASS (new test + existing window tests).

- [ ] **Step 5: Commit**

```bash
cd .worktrees/global-git-footer
git add crates/vmux_layout/src/window.rs
git commit -m "feat(layout): spawn collapsed Footer node beneath Main"
```

---

## Task 4: `FooterView` in the overlay page (`page.rs` + `Cargo.toml`)

**Files:**
- Modify: `crates/vmux_layout/Cargo.toml` (wasm dep on `vmux_git`)
- Modify: `crates/vmux_layout/src/page.rs` (imports; mount `FooterView`; add `FooterView` component)

> WASM glue — pure logic is already covered by Task 1's native tests. Verify by compiling for wasm + the manual checklist in Task 6.

- [ ] **Step 1: Add the wasm dependency**

In `crates/vmux_layout/Cargo.toml`, inside `[target.'cfg(target_arch = "wasm32")'.dependencies]` (after line 57, `vmux_ui = ...`):

```toml
vmux_git = { path = "../vmux_git" }
```

- [ ] **Step 2: Extend the page imports**

In `crates/vmux_layout/src/page.rs`, extend the `crate::event::{...}` import (lines 3-7) to include the new items:

```rust
use crate::event::{
    FooterStateRequest, HeaderCommandEvent, LAYOUT_STATE_EVENT, LayoutStateEvent, PANE_TREE_EVENT,
    PaneNode, PaneTreeEvent, RELOAD_EVENT, ReloadEvent, STACKS_EVENT, StackNode, StackRow,
    StacksHostEvent, TABS_EVENT, TabRow, TabsCommandEvent, TabsHostEvent, file_url_to_path,
    should_show_footer,
};
```

Add (after line 13, `use wasm_bindgen::JsCast;`):

```rust
use vmux_git::event::{
    GIT_ERROR_EVENT, GIT_RESULT_EVENT, GIT_STATUS_EVENT, GitErrorEvent, GitResultEvent,
    GitStatusEvent, GitStatusRequest,
};
use vmux_git::ui::GitFooter;
```

- [ ] **Step 3: Mount `FooterView` in the overlay**

In `page.rs`, compute the active stack url inside `Page` (after `let tabs = tabs_state();`, line 81):

```rust
    let active_stack_url = stacks
        .iter()
        .find(|s| s.is_active)
        .map(|s| s.url.clone())
        .unwrap_or_default();
```

In the overlay `rsx!`, add `FooterView` as a child of the root `div { class: "fixed inset-0 ..." }`, after the header `if` block (after line 156's closing `}`), still inside the root div:

```rust
            if overlay_ready {
                FooterView {
                    active_stack_url,
                    header_left: state.header_left(),
                    header_right: state.header_right(),
                    window_pad_bottom: state.window_pad_bottom,
                }
            }
```

- [ ] **Step 4: Add the `FooterView` component**

Add to `page.rs` (e.g., after the `HeaderView` component, ~line 279):

```rust
#[component]
fn FooterView(
    active_stack_url: String,
    header_left: f32,
    header_right: f32,
    window_pad_bottom: f32,
) -> Element {
    let mut git_path = use_signal(String::new);
    let mut branch = use_signal(String::new);
    let mut ahead = use_signal(|| 0u32);
    let mut behind = use_signal(|| 0u32);
    let mut staged = use_signal(|| 0u32);
    let mut message = use_signal(String::new);
    let mut nonce = use_signal(|| 0u32);

    use_effect(use_reactive!(|active_stack_url| {
        git_path.set(file_url_to_path(&active_stack_url).unwrap_or_default());
    }));

    let _status = use_bin_event_listener::<GitStatusEvent, _>(GIT_STATUS_EVENT, move |s| {
        branch.set(s.branch);
        ahead.set(s.ahead);
        behind.set(s.behind);
        staged.set(s.staged_count);
    });
    let _result = use_bin_event_listener::<GitResultEvent, _>(GIT_RESULT_EVENT, move |r| {
        message.set(if r.ok { String::new() } else { r.message });
        nonce.set(nonce() + 1);
    });
    let _error = use_bin_event_listener::<GitErrorEvent, _>(GIT_ERROR_EVENT, move |e| {
        message.set(e.message);
    });

    use_effect(move || {
        let p = git_path();
        let _ = nonce();
        if p.is_empty() {
            branch.set(String::new());
            ahead.set(0);
            behind.set(0);
            staged.set(0);
            message.set(String::new());
        } else {
            let _ = try_cef_bin_emit_rkyv(&GitStatusRequest { path: p });
        }
    });

    use_effect(move || {
        let open = should_show_footer(staged(), ahead(), !message().is_empty());
        let _ = try_cef_bin_emit_rkyv(&FooterStateRequest { open });
    });

    if !should_show_footer(staged(), ahead(), !message().is_empty()) {
        return rsx! {};
    }

    let footer_style = format!(
        "left:{header_left}px;right:{header_right}px;bottom:{window_pad_bottom}px;height:{}px;",
        crate::event::FOOTER_HEIGHT_PX,
    );

    rsx! {
        div {
            class: "pointer-events-auto fixed",
            style: "{footer_style}",
            GitFooter {
                path: git_path,
                branch,
                ahead,
                behind,
                staged_count: staged,
                message,
            }
        }
    }
}
```

- [ ] **Step 5: Compile for wasm**

Run: `cargo check -p vmux_layout --target wasm32-unknown-unknown 2>&1 | tail -25`
Expected: no errors. (If the target is missing: `rustup target add wasm32-unknown-unknown`. If `use_reactive!`/`GitFooter` prop types mismatch, adjust signal vs ReadSignal coercions until it compiles.)

- [ ] **Step 6: Commit**

```bash
cd .worktrees/global-git-footer
git add crates/vmux_layout/Cargo.toml crates/vmux_layout/src/page.rs
git commit -m "feat(layout): render global git footer in overlay"
```

---

## Task 5: Remove `GitFooter` from the editor page

**Files:**
- Modify: `crates/vmux_editor/src/page.rs` (drop `GitFooter` import symbol + the usage block)

- [ ] **Step 1: Drop the `GitFooter` import**

In `crates/vmux_editor/src/page.rs` line 8, change:

```rust
use vmux_git::ui::{DiffView, GitBar, GitFooter};
```

to:

```rust
use vmux_git::ui::{DiffView, GitBar};
```

- [ ] **Step 2: Remove the `GitFooter` usage**

Delete the `GitFooter { ... }` block at lines 765-772:

```rust
            GitFooter {
                path: git_path,
                branch: git_branch,
                ahead: git_ahead,
                behind: git_behind,
                staged_count: git_staged,
                message: git_message,
            }
```

(Keep all `git_*` signals — `GitBar` still consumes them as props at lines 654-664. Keep `DiffView`.)

- [ ] **Step 3: Compile for wasm**

Run: `cargo check -p vmux_editor --target wasm32-unknown-unknown 2>&1 | tail -25`
Expected: no errors, no unused-variable warnings for the `git_*` signals (still passed to `GitBar`).

- [ ] **Step 4: Commit**

```bash
cd .worktrees/global-git-footer
git add crates/vmux_editor/src/page.rs
git commit -m "refactor(editor): drop per-pane GitFooter (now global)"
```

---

## Task 6: Full checks + manual verification

**Files:** none (verification only)

- [ ] **Step 1: Format + clippy + native tests**

Run:
```bash
cd .worktrees/global-git-footer
cargo fmt
git checkout -- patches/ 2>/dev/null || true   # cargo fmt may touch vendored patches; keep only crates/ changes
cargo clippy -p vmux_layout -p vmux_editor --all-targets 2>&1 | tail -25
cargo test -p vmux_layout 2>&1 | tail -25
```
Expected: fmt clean (only `crates/` reformatted), clippy no warnings, tests pass.

- [ ] **Step 2: Commit any fmt changes**

```bash
cd .worktrees/global-git-footer
git add -A ':!patches'
git commit -m "style: cargo fmt" || echo "nothing to commit"
```

- [ ] **Step 3: Manual verification (user runs the app)**

Verify in the running app:
- Open a file editor pane, stage a change → footer appears as its own row beneath the panes (panes shrink up, nothing covered); branch + `Commit(N)` shown.
- Type a message, Commit → commit succeeds, message clears, footer collapses (if nothing left), panes restore full height.
- `ahead > 0` with nothing staged → footer shows branch + `Push` only.
- Focus a terminal/browser/agent pane → footer hides, panes restore.
- Open the side sheet → footer's left edge matches the header's left edge.
- Tune `FOOTER_HEIGHT_PX` / island styling for visual parity with the header glass if needed.

> If a page edit does not show after rebuild, confirm `crates/vmux_layout/src` (and `crates/vmux_git/src`) are tracked in `crates/vmux_server/build.rs` `track_manifest_rel_paths`.

- [ ] **Step 4: Delete the plan file (per AGENTS.md) once fully implemented**

```bash
cd .worktrees/global-git-footer
git rm docs/plans/2026-06-24-global-git-footer.md
git commit -m "chore: remove completed plan"
```
