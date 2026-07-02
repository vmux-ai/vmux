# Auto-tidy Agent File Previews Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **vmux caveat:** CEF builds are large. Warm the target dir with a background `cargo build -p <crate>` before the edit loop, then run the per-crate `cargo test -p <crate>` shown in each task. Do NOT subagent-drive the CEF-heavy tasks. Implement directly. (See memory: subagent CEF fragility, vmux build workflow.)

**Goal:** When an agent finishes a turn and its follow-pane holds more than 5 `file://` previews, close the unchanged ones (keeping git-changed files and the pane's active preview), confirming once with a native dialog that offers "Always tidy".

**Architecture:** A turn-end signal (`AgentAttention`, from the terminal bell) drives a `vmux_agent` system that enumerates the agent's follow-pane file stacks, asks git which are changed (new sync native `runner::dirty_set`), and picks the closable set with a pure `decide_closable`. Closing routes through a new `vmux_layout` `CloseStackRequest` (plain despawn — the active stack is always kept, so the pane never empties, so no pane-collapse logic is touched). First run pops a native `rfd` dialog; "Always tidy" persists `agent.tidy_files_auto`.

**Tech Stack:** Rust, Bevy (0.19-rc), messages + systems, `vmux_git` (git porcelain), `rfd` (native dialog), `vmux_setting` (persisted settings).

**Spec:** `docs/specs/2026-07-02-agent-tidy-previews-design.md`

---

## File Structure

Created:
- `crates/vmux_agent/src/tidy.rs` — pure tidy logic + component: `path_from_file_url`, `TidyAction`, `tidy_choice`, `decide_closable`, `is_changed`, `PendingTidy`, button-label consts.

Modified:
- `crates/vmux_git/src/parse.rs` — `changed_paths()` (repo-wide path collector).
- `crates/vmux_git/src/runner.rs` — `dirty_set()` (repo root + changed set).
- `crates/vmux_setting/src/plugin/runtime.rs` — 3 `AgentSettings` fields + defaults.
- `crates/vmux_setting/src/settings.ron` — embedded defaults for 2 of them.
- `crates/vmux_layout/src/stack.rs` — `CloseStackRequest` message + handler + registration.
- `crates/vmux_layout/src/lib.rs` — re-export `CloseStackRequest`.
- `crates/vmux_agent/Cargo.toml` — add `vmux_git`, `rfd` deps.
- `crates/vmux_agent/src/lib.rs` — `mod tidy;`.
- `crates/vmux_agent/src/plugin.rs` — `AgentFileResolve::file_stacks_for`; `tidy_on_agent_attention` + `process_pending_tidy` systems; `show_tidy_dialog`; registration.

Data flow: `AgentAttention` → `tidy_on_agent_attention` (decide) → `CloseStackRequest` (auto) or `PendingTidy` marker → `process_pending_tidy` (dialog) → `CloseStackRequest` → `handle_close_stack_requests` (despawn).

---

## Task 1: `parse::changed_paths` (repo-wide changed set)

**Files:**
- Modify: `crates/vmux_git/src/parse.rs`
- Test: same file, `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_git/src/parse.rs`:

```rust
    #[test]
    fn changed_paths_collects_all_entry_kinds() {
        let out = "# branch.head main\n\
1 .M N... 100644 100644 100644 aaa bbb src/main.rs\n\
1 M. N... 100644 100644 100644 ccc ddd src/lib.rs\n\
2 R. N... 100644 100644 100644 eee fff R100 new.rs\told.rs\n\
u UU N... 100644 100644 100644 100644 ggg hhh iii conflict.rs\n\
? notes.txt\n";
        let set = changed_paths(out);
        assert!(set.contains("src/main.rs"));
        assert!(set.contains("src/lib.rs"));
        assert!(set.contains("new.rs"));
        assert!(!set.contains("old.rs"));
        assert!(set.contains("conflict.rs"));
        assert!(set.contains("notes.txt"));
        assert_eq!(set.len(), 5);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_git changed_paths_collects_all_entry_kinds`
Expected: FAIL — `cannot find function changed_paths`.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/vmux_git/src/parse.rs` (top-level, reuses the existing private `entry_path`):

```rust
/// Repo-relative paths of every changed entry in `git status --porcelain=v2`
/// output — one per `1 `/`2 `/`u `/`? ` line (untracked files included).
pub fn changed_paths(out: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for line in out.lines() {
        let path = if line.starts_with("1 ") || line.starts_with("2 ") {
            let kind_tokens = if line.starts_with("2 ") { 9 } else { 8 };
            entry_path(line, kind_tokens)
                .split('\t')
                .next()
                .unwrap_or("")
                .to_string()
        } else if line.starts_with("u ") {
            entry_path(line, 10).to_string()
        } else if let Some(rest) = line.strip_prefix("? ") {
            rest.trim().to_string()
        } else {
            continue;
        };
        if !path.is_empty() {
            set.insert(path);
        }
    }
    set
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_git changed_paths_collects_all_entry_kinds`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_git/src/parse.rs
git commit -m "feat(git): changed_paths — repo-wide porcelain path set"
```

---

## Task 2: `runner::dirty_set` (repo root + changed set)

**Files:**
- Modify: `crates/vmux_git/src/runner.rs`
- Test: same file, `#[cfg(test)] mod tests` (reuses `test_repo`)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_git/src/runner.rs`:

```rust
    #[test]
    fn dirty_set_lists_modified_and_untracked_not_clean() {
        let repo = test_repo::init();
        let _clean = test_repo::write(repo.path(), "clean.txt", "x\n");
        let modified = test_repo::write(repo.path(), "mod.txt", "one\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "mod.txt", "two\n");
        test_repo::write(repo.path(), "new.txt", "n\n");

        let (root, set) = dirty_set(&modified).unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
        assert!(set.contains("mod.txt"));
        assert!(set.contains("new.txt"));
        assert!(!set.contains("clean.txt"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_git dirty_set_lists_modified_and_untracked_not_clean`
Expected: FAIL — `cannot find function dirty_set`.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/vmux_git/src/runner.rs` (top-level; `repo_root`, `git`, `parse` are already in scope):

```rust
/// Repo root plus the set of repo-relative paths `git status --porcelain=v2`
/// reports as changed (modified/staged/untracked/renamed/deleted/conflicted).
pub fn dirty_set(
    file: &Path,
) -> Result<(PathBuf, std::collections::HashSet<String>), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git(&root, &["status", "--porcelain=v2"])?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    Ok((root, parse::changed_paths(&stdout)))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_git dirty_set_lists_modified_and_untracked_not_clean`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_git/src/runner.rs
git commit -m "feat(git): dirty_set — native repo-relative changed-path lookup"
```

---

## Task 3: settings fields (`tidy_files`, `tidy_files_max`, `tidy_files_auto`)

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs`
- Modify: `crates/vmux_setting/src/settings.ron`
- Test: `crates/vmux_setting/src/plugin/runtime.rs` `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_setting/src/plugin/runtime.rs`:

```rust
    #[test]
    fn agent_defaults_enable_tidy() {
        let s = default_agent_settings();
        assert!(s.tidy_files);
        assert_eq!(s.tidy_files_max, 5);
        assert!(!s.tidy_files_auto);
    }

    #[test]
    fn apply_update_sets_tidy_auto_without_clobbering_siblings() {
        let mut s: AppSettings = serde_json::from_str("{}").expect("default settings");
        assert!(s.agent.follow_files);
        let ron = apply_settings_update(&mut s, "agent.tidy_files_auto", serde_json::json!(true))
            .expect("update ok");
        assert!(s.agent.tidy_files_auto);
        assert!(s.agent.follow_files, "sibling preserved");
        assert!(ron.contains("tidy_files_auto"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_setting agent_defaults_enable_tidy apply_update_sets_tidy_auto`
Expected: FAIL — `no field tidy_files on AgentSettings`.

- [ ] **Step 3: Write minimal implementation**

In `crates/vmux_setting/src/plugin/runtime.rs`, add fields to `AgentSettings` (after `follow_files`):

```rust
    /// When true (default), an agent finishing a turn tidies clean file previews
    /// in its follow-pane, keeping changed files and the pane's active preview.
    #[serde(default = "default_true")]
    pub tidy_files: bool,
    /// Only tidy when the follow-pane holds more than this many file previews.
    #[serde(default = "default_tidy_files_max")]
    pub tidy_files_max: usize,
    /// When true, tidy without the confirm dialog. Set by the "Always tidy" button.
    #[serde(default)]
    pub tidy_files_auto: bool,
```

Add the default helper near `default_true`:

```rust
fn default_tidy_files_max() -> usize {
    5
}
```

Update `default_agent_settings()` to set the new fields:

```rust
fn default_agent_settings() -> AgentSettings {
    AgentSettings {
        app_providers: vec![AppProviderSettings {
            provider: "stub".to_string(),
            kind: "vibe".to_string(),
            models: vec!["echo".to_string()],
        }],
        follow_files: true,
        tidy_files: true,
        tidy_files_max: 5,
        tidy_files_auto: false,
    }
}
```

In `crates/vmux_setting/src/settings.ron`, inside the `agent: (` block (next to `follow_files: true,`), add the two non-default-`false` keys:

```ron
        tidy_files: true,
        tidy_files_max: 5,
```

(Leave `tidy_files_auto` out of the embedded ron — it defaults `false` via serde and is only ever written on explicit user opt-in.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_setting agent_defaults_enable_tidy apply_update_sets_tidy_auto`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_setting/src/plugin/runtime.rs crates/vmux_setting/src/settings.ron
git commit -m "feat(settings): agent.tidy_files{,_max,_auto} knobs"
```

---

## Task 4: `CloseStackRequest` message + despawn handler (vmux_layout)

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs` (message, handler, registration, tests)
- Modify: `crates/vmux_layout/src/lib.rs` (re-export)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/vmux_layout/src/stack.rs` (mirror the spawn pattern used by existing `closing_last_stack_*` tests in the same module):

```rust
    #[test]
    fn close_stack_request_despawns_target_keeps_siblings() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseStackRequest>()
            .init_resource::<NewStackContext>()
            .add_systems(Update, handle_close_stack_requests);

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let s1 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();
        let s2 = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest { stack: s1 });
        app.update();

        assert!(app.world().get_entity(s1).is_err(), "target despawned");
        assert!(app.world().get_entity(s2).is_ok(), "sibling kept");
    }

    #[test]
    fn close_stack_request_keeps_last_stack_in_pane() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CloseStackRequest>()
            .init_resource::<NewStackContext>()
            .add_systems(Update, handle_close_stack_requests);

        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let only = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt(1), ChildOf(pane)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<CloseStackRequest>>()
            .write(CloseStackRequest { stack: only });
        app.update();

        assert!(app.world().get_entity(only).is_ok(), "never empties a pane");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout close_stack_request`
Expected: FAIL — `cannot find type CloseStackRequest` / `handle_close_stack_requests`.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/vmux_layout/src/stack.rs` (near the other message types; `Relationship`, `NewStackContext`, `Pane`, `Stack`, `ChildOf` are already imported/used in this file):

```rust
/// Close (despawn) a specific stack entity. Used by agent auto-tidy. Ignored if
/// it is the only stack in its pane, so tidy can never empty (and collapse) a pane.
#[derive(Message, Clone, Copy)]
pub struct CloseStackRequest {
    pub stack: Entity,
}

fn handle_close_stack_requests(
    mut reader: MessageReader<CloseStackRequest>,
    child_of_q: Query<&ChildOf>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<Stack>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut commands: Commands,
) {
    for req in reader.read() {
        let Ok(pane) = child_of_q.get(req.stack).map(Relationship::get) else {
            continue;
        };
        let Ok(children) = pane_children.get(pane) else {
            continue;
        };
        let stack_count = children.iter().filter(|&e| stack_q.contains(e)).count();
        if stack_count <= 1 {
            continue;
        }
        if new_stack_ctx.stack == Some(req.stack) {
            new_stack_ctx.stack = None;
        }
        if new_stack_ctx.previous_stack == Some(req.stack) {
            new_stack_ctx.previous_stack = None;
        }
        commands.entity(req.stack).despawn();
    }
}
```

Register in `StackPlugin::build` (add the message + system to the existing builder chain):

```rust
        app.register_type::<Stack>()
            .init_resource::<FocusedStack>()
            .add_message::<CloseStackRequest>()
            .add_systems(
                Update,
                (
                    handle_stack_commands
                        .in_set(ReadAppCommands)
                        .in_set(StackCommandSet),
                    handle_close_stack_requests.in_set(ReadAppCommands),
                ),
            )
            .add_systems(
                Update,
                compute_focused_stack
                    .in_set(ComputeFocusSet)
                    .after(ReadAppCommands),
            )
            .add_systems(PostUpdate, sync_stack_picking);
```

Re-export in `crates/vmux_layout/src/lib.rs` next to the `OpenBesideRequest` re-export:

```rust
pub use stack::CloseStackRequest;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout close_stack_request`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/stack.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): CloseStackRequest — despawn one stack, never empty a pane"
```

---

## Task 5: `path_from_file_url` + agent crate deps + `tidy` module

**Files:**
- Modify: `crates/vmux_agent/Cargo.toml`
- Modify: `crates/vmux_agent/src/lib.rs`
- Create: `crates/vmux_agent/src/tidy.rs`

- [ ] **Step 1: Add deps and module, write the failing test**

In `crates/vmux_agent/Cargo.toml` `[dependencies]`, add (match how `vmux_layout` declares `rfd` — copy that exact version/workspace form):

```toml
vmux_git = { path = "../vmux_git" }
rfd = "0.15"
```

In `crates/vmux_agent/src/lib.rs`, add near the other `mod` declarations:

```rust
mod tidy;
```

Create `crates/vmux_agent/src/tidy.rs` with only the test + a stub so it compiles-then-fails:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_file_url_stripping_scheme_fragment_and_encoding() {
        assert_eq!(
            path_from_file_url("file:///a/b.rs#L3:1-4"),
            Some(PathBuf::from("/a/b.rs"))
        );
        assert_eq!(
            path_from_file_url("file:///a/my%20file.rs"),
            Some(PathBuf::from("/a/my file.rs"))
        );
        assert_eq!(path_from_file_url("file:/rel#x"), Some(PathBuf::from("/rel")));
        assert_eq!(path_from_file_url("https://x/y"), None);
        assert_eq!(path_from_file_url("file://"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent parses_file_url`
Expected: FAIL — `cannot find function path_from_file_url`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/vmux_agent/src/tidy.rs`:

```rust
use std::path::PathBuf;

/// Absolute filesystem path from a `file://` URL: strips the scheme and any
/// `#fragment`, then percent-decodes. `None` for non-`file:` or empty paths.
pub(crate) fn path_from_file_url(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("file://")
        .or_else(|| url.strip_prefix("file:"))?;
    let no_frag = rest.split('#').next().unwrap_or(rest);
    let decoded = percent_decode(no_frag);
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_agent parses_file_url`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/lib.rs crates/vmux_agent/src/tidy.rs
git commit -m "feat(agent): tidy module + path_from_file_url; add vmux_git/rfd deps"
```

---

## Task 6: `decide_closable` + `tidy_choice` + `PendingTidy` + `is_changed`

**Files:**
- Modify: `crates/vmux_agent/src/tidy.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/vmux_agent/src/tidy.rs`:

```rust
    use bevy::prelude::Entity;

    fn e(i: u32) -> Entity {
        Entity::from_raw(i)
    }

    #[test]
    fn decide_closable_below_threshold_is_empty() {
        // 3 stacks, max 5 → nothing, even if clean
        let stacks = vec![(e(1), 10, false), (e(2), 20, false), (e(3), 30, false)];
        assert!(decide_closable(&stacks, 5).is_empty());
    }

    #[test]
    fn decide_closable_keeps_changed_and_active() {
        // 6 stacks, max 5. active = highest ts (e6). changed = e2, e4.
        let stacks = vec![
            (e(1), 10, false),
            (e(2), 20, true),
            (e(3), 30, false),
            (e(4), 40, true),
            (e(5), 50, false),
            (e(6), 60, false), // active
        ];
        let mut got = decide_closable(&stacks, 5);
        got.sort();
        assert_eq!(got, vec![e(1), e(3), e(5)]);
    }

    #[test]
    fn decide_closable_empty_when_all_changed() {
        let stacks = vec![
            (e(1), 10, true),
            (e(2), 20, true),
            (e(3), 30, true),
            (e(4), 40, true),
            (e(5), 50, true),
            (e(6), 60, true),
        ];
        assert!(decide_closable(&stacks, 5).is_empty());
    }

    #[test]
    fn tidy_choice_maps_labels() {
        assert_eq!(tidy_choice(ALWAYS_LABEL), TidyAction::AlwaysClose);
        assert_eq!(tidy_choice(TIDY_LABEL), TidyAction::Close);
        assert_eq!(tidy_choice(NOTNOW_LABEL), TidyAction::Skip);
        assert_eq!(tidy_choice("anything else"), TidyAction::Skip);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_agent decide_closable tidy_choice`
Expected: FAIL — `cannot find function decide_closable` etc.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/vmux_agent/src/tidy.rs` (add `use bevy::prelude::*;` at the top if not present):

```rust
use bevy::prelude::*;

pub(crate) const TIDY_LABEL: &str = "Tidy";
pub(crate) const ALWAYS_LABEL: &str = "Always tidy";
pub(crate) const NOTNOW_LABEL: &str = "Not now";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TidyAction {
    Close,
    AlwaysClose,
    Skip,
}

pub(crate) fn tidy_choice(label: &str) -> TidyAction {
    match label {
        ALWAYS_LABEL => TidyAction::AlwaysClose,
        TIDY_LABEL => TidyAction::Close,
        _ => TidyAction::Skip,
    }
}

/// Marker on a follow-pane: clean previews awaiting the tidy confirm dialog.
#[derive(Component)]
pub(crate) struct PendingTidy {
    pub closable: Vec<Entity>,
}

/// Given `(stack, last_activated, changed)` for every file preview in a pane and
/// the tidy threshold, the stacks to close: clean, not the active (max
/// last_activated) one. Empty if at/below threshold or nothing is closable.
pub(crate) fn decide_closable(stacks: &[(Entity, i64, bool)], max: usize) -> Vec<Entity> {
    if stacks.len() <= max {
        return Vec::new();
    }
    let active = stacks
        .iter()
        .max_by_key(|(_, ts, _)| *ts)
        .map(|(s, _, _)| *s);
    stacks
        .iter()
        .filter(|(s, _, changed)| Some(*s) != active && !changed)
        .map(|(s, _, _)| *s)
        .collect()
}

/// Whether `abs` is git-changed. Memoizes `(repo_root, changed_set)` per repo in
/// `repos`. Files outside any repo (or that error) are treated as clean.
pub(crate) fn is_changed(
    abs: &std::path::Path,
    repos: &mut Vec<(std::path::PathBuf, std::collections::HashSet<String>)>,
) -> bool {
    let abs = abs
        .canonicalize()
        .unwrap_or_else(|_| abs.to_path_buf());
    if let Some((root, set)) = repos.iter().find(|(r, _)| abs.starts_with(r)) {
        return set.contains(&rel_str(root, &abs));
    }
    match vmux_git::runner::dirty_set(&abs) {
        Ok((root, set)) => {
            let changed = set.contains(&rel_str(&root, &abs));
            repos.push((root, set));
            changed
        }
        Err(_) => false,
    }
}

fn rel_str(root: &std::path::Path, abs: &std::path::Path) -> String {
    abs.strip_prefix(root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_agent decide_closable tidy_choice`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/tidy.rs
git commit -m "feat(agent): decide_closable, tidy_choice, PendingTidy, is_changed"
```

---

## Task 7: `AgentFileResolve::file_stacks_for` (enumerate follow-pane file stacks)

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Add the method (no separate unit test — exercised by the manual pass in Task 10; it is a thin generalization of the production `file_page_for`)**

In `crates/vmux_agent/src/plugin.rs`, in the `impl AgentFileResolve<'_, '_>` block (where `file_page_for` lives), make `agent_pane` callable from the tidy systems and add the enumerator:

Change `fn agent_pane` to `pub(crate) fn agent_pane`, then add:

```rust
    /// The agent's follow-pane and every `file://` preview stack in it, with each
    /// stack's URL. Generalizes `file_page_for` (which returns only the first).
    /// `None` when the agent has no file follow-pane yet.
    pub(crate) fn file_stacks_for(
        &self,
        agent_pane: Entity,
    ) -> Option<(Entity, Vec<(Entity, String)>)> {
        use bevy::ecs::relationship::Relationship;
        let agent_parent = self.child_of.get(agent_pane).ok().map(Relationship::get)?;
        let mut follow_pane = None;
        let mut stacks = Vec::new();
        for (_page, page_co, meta) in self.file_pages.iter() {
            if !meta.url.starts_with("file:") {
                continue;
            }
            let stack = page_co.get();
            let Ok(pane) = self.child_of.get(stack).map(Relationship::get) else {
                continue;
            };
            if pane == agent_pane {
                continue;
            }
            if self.child_of.get(pane).ok().map(Relationship::get) != Some(agent_parent) {
                continue;
            }
            follow_pane = Some(pane);
            stacks.push((stack, meta.url.clone()));
        }
        follow_pane.map(|p| (p, stacks))
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_agent`
Expected: no errors (may warn "function is never used" until Task 8 — acceptable this step).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): AgentFileResolve::file_stacks_for enumerates follow-pane files"
```

---

## Task 8: `tidy_on_agent_attention` + `process_pending_tidy` + `show_tidy_dialog`

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`

No automated test here (drives the native `rfd` dialog + full agent/pane/git stack; covered by the Task 10 manual pass). All decision logic it relies on is already unit-tested in Tasks 1–6.

- [ ] **Step 1: Add the decision system**

In `crates/vmux_agent/src/plugin.rs`, add (imports: `use crate::tidy::{self, PendingTidy};` and the fully-qualified paths below):

```rust
fn tidy_on_agent_attention(
    mut reader: MessageReader<vmux_core::notify::AgentAttention>,
    settings: Res<vmux_setting::AppSettings>,
    agents: Query<&vmux_service::protocol::ProcessId, With<vmux_core::team::Agent>>,
    resolve: AgentFileResolve,
    last_activated: Query<&vmux_core::LastActivatedAt>,
    pending: Query<(), With<PendingTidy>>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    if !settings.agent.tidy_files {
        for _ in reader.read() {}
        return;
    }
    for att in reader.read() {
        let Ok(pid) = agents.get(att.entity) else {
            continue;
        };
        let Some(agent_pane) = resolve.agent_pane(*pid) else {
            continue;
        };
        let Some((follow_pane, stacks)) = resolve.file_stacks_for(agent_pane) else {
            continue;
        };
        if pending.get(follow_pane).is_ok() {
            continue;
        }
        let mut repos: Vec<(std::path::PathBuf, std::collections::HashSet<String>)> = Vec::new();
        let rows: Vec<(Entity, i64, bool)> = stacks
            .iter()
            .map(|(stack, url)| {
                let ts = last_activated.get(*stack).map(|t| t.0).unwrap_or(i64::MIN);
                let changed = tidy::path_from_file_url(url)
                    .map(|abs| tidy::is_changed(&abs, &mut repos))
                    .unwrap_or(false);
                (*stack, ts, changed)
            })
            .collect();
        let closable = tidy::decide_closable(&rows, settings.agent.tidy_files_max);
        if closable.is_empty() {
            continue;
        }
        if settings.agent.tidy_files_auto {
            for stack in closable {
                close.write(vmux_layout::CloseStackRequest { stack });
            }
        } else {
            commands
                .entity(follow_pane)
                .insert(PendingTidy { closable });
        }
    }
}
```

- [ ] **Step 2: Add the dialog exclusive system + helper**

Add to `crates/vmux_agent/src/plugin.rs`:

```rust
fn show_tidy_dialog(count: usize) -> String {
    let result = rfd::MessageDialog::new()
        .set_title("Tidy previews?")
        .set_description(format!(
            "Close {count} unchanged file previews in this agent pane?"
        ))
        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            crate::tidy::TIDY_LABEL.to_string(),
            crate::tidy::ALWAYS_LABEL.to_string(),
            crate::tidy::NOTNOW_LABEL.to_string(),
        ))
        .show();
    match result {
        rfd::MessageDialogResult::Custom(label) => label,
        rfd::MessageDialogResult::Yes => crate::tidy::TIDY_LABEL.to_string(),
        rfd::MessageDialogResult::No => crate::tidy::ALWAYS_LABEL.to_string(),
        _ => crate::tidy::NOTNOW_LABEL.to_string(),
    }
}

fn process_pending_tidy(world: &mut World) {
    let jobs: Vec<(Entity, Vec<Entity>)> = world
        .query::<(Entity, &PendingTidy)>()
        .iter(world)
        .map(|(e, p)| (e, p.closable.clone()))
        .collect();
    if jobs.is_empty() {
        return;
    }
    for (pane, closable) in jobs {
        if let Ok(mut e) = world.get_entity_mut(pane) {
            e.remove::<PendingTidy>();
        }
        let action = crate::tidy::tidy_choice(&show_tidy_dialog(closable.len()));
        if action == crate::tidy::TidyAction::Skip {
            continue;
        }
        if action == crate::tidy::TidyAction::AlwaysClose {
            if let Some(mut settings) = world.get_resource_mut::<vmux_setting::AppSettings>() {
                settings.agent.tidy_files_auto = true;
            }
            if let Some(mut save) =
                world.get_resource_mut::<Messages<vmux_setting::SettingsSaveRequest>>()
            {
                save.write(vmux_setting::SettingsSaveRequest);
            }
        }
        if let Some(mut msgs) =
            world.get_resource_mut::<Messages<vmux_layout::CloseStackRequest>>()
        {
            for stack in closable {
                msgs.write(vmux_layout::CloseStackRequest { stack });
            }
        }
    }
}
```

Note: verify the `SettingsSaveRequest` import path — if it is not re-exported at `vmux_setting::SettingsSaveRequest`, use the full path (`vmux_setting::plugin::runtime::SettingsSaveRequest`) or add a re-export in `vmux_setting`. Confirm with `cargo check -p vmux_agent`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_agent`
Expected: no errors (systems unused until Task 9 registers them — acceptable).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): tidy_on_agent_attention + rfd confirm dialog systems"
```

---

## Task 9: register tidy systems + message plumbing in `AgentPlugin`

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (`AgentPlugin::build`)

- [ ] **Step 1: Register**

In `AgentPlugin::build`, add the `CloseStackRequest` message store (mirrors the existing `OpenBesideRequest` `init_resource`) and register the two systems. Insert `tidy_on_agent_attention` into the existing bell/attention chain (so `AgentAttention` is written first), and add `process_pending_tidy` ordered after it:

```rust
            .init_resource::<bevy::ecs::message::Messages<vmux_layout::CloseStackRequest>>()
            .add_systems(
                Update,
                (
                    agent_bell_to_attention,
                    tidy_on_agent_attention,
                    mark_agent_done,
                    clear_agent_done,
                )
                    .chain()
                    .after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                process_pending_tidy.after(tidy_on_agent_attention),
            )
```

(The existing chain currently lists `(agent_bell_to_attention, mark_agent_done, clear_agent_done)` — replace that tuple with the four-system version above. Keep the existing `.init_resource::<Messages<OpenBesideRequest>>()` line; add the `CloseStackRequest` one next to it.)

- [ ] **Step 2: Verify the whole crate builds + existing tests pass**

Run: `cargo test -p vmux_agent`
Expected: PASS (existing + new unit tests; no regressions).

- [ ] **Step 3: Workspace typecheck (catch cross-crate + wasm breakage)**

Run: `cargo check --workspace` then `cargo check -p vmux_git --target wasm32-unknown-unknown`
Expected: no errors. (`vmux_git`/`vmux_core` compile for wasm; the new code is native-only via `runner`/`plugin` gating, but confirm nothing leaked. See memory: vmux_core::event is wasm-compiled.)

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): register auto-tidy systems on the turn-end chain"
```

---

## Task 10: format, clippy, and manual end-to-end verification

**Files:** none (verification only)

- [ ] **Step 1: fmt + clippy (do not reformat vendored patches)**

Run:
```bash
cargo fmt -p vmux_git -p vmux_setting -p vmux_layout -p vmux_agent
git checkout -- patches/ 2>/dev/null || true
cargo clippy -p vmux_git -p vmux_setting -p vmux_layout -p vmux_agent --all-targets
```
Expected: no clippy warnings in the touched crates. Fix any before proceeding.

- [ ] **Step 2: Build and run the app**

Warm then run the dev build (let the user launch if a build is already warm — see memory: no unbounded make dev). Confirm it starts clean (read `~/Library/Application Support/Vmux/dev/logs/vmux-dev.<date>.log` for panics).

- [ ] **Step 3: Manual scenario — confirm dialog + retention**

In a git repo, have an agent (vibe) read ≥6 files without editing, then finish a turn (bell). Verify:
- the native dialog appears: title "Tidy previews?", buttons **Tidy / Always tidy / Not now**;
- **Not now** → nothing closes;
- reopen the pile, click **Tidy** → all clean previews close except the active one; the follow-pane keeps ≥1 tab (no pane collapse);
- edit one file so it has a diff, pile up others, finish turn, **Tidy** → the changed file's preview survives, clean ones close.

- [ ] **Step 4: Manual scenario — "Always tidy" persistence**

Click **Always tidy** once. Confirm: previews close, and `~/Library/Application Support/Vmux/dev/profiles/<profile>/…/settings.ron` (the runtime settings path) now contains `tidy_files_auto: true` under `agent:`. Trigger another over-threshold turn end → tidy happens **silently**, no dialog.

- [ ] **Step 5: Manual scenario — threshold**

With ≤5 previews, finish a turn → nothing closes, no dialog (below `tidy_files_max`).

- [ ] **Step 6: Final commit (if fmt changed anything)**

```bash
git add -A
git commit -m "style: fmt/clippy for agent auto-tidy" || echo "nothing to commit"
```

- [ ] **Step 7: Delete this plan file (per project convention once implemented) and open the PR**

```bash
git rm docs/plans/2026-07-02-agent-tidy-previews.md
git commit -m "chore: remove implemented tidy plan"
```
Then open a PR (`gh pr create`, return the URL) summarizing the feature; keep the spec doc.

---

## Self-Review

**Spec coverage:**
- Trigger (`AgentAttention`) → Task 8/9. Gate (`tidy_files_max`) → `decide_closable` Task 6, wired Task 8. Retention (changed ∪ active) → `decide_closable` + `is_changed` Tasks 6/8. Confirm dialog (rfd, 3 buttons, Always persists) → Tasks 6/8/9. Close = despawn, never empties → Task 4 + active-kept invariant. Diff bridge (`changed_paths`/`dirty_set`, sync) → Tasks 1/2. Config keys → Task 3. Scope (follow-pane only) → `file_stacks_for` Task 7. Edge cases (all-changed, file outside repo, non-file stacks) → `decide_closable`/`is_changed`/`file_stacks_for`. All covered.

**Placeholder scan:** none — every code step has full code; every run step has a command + expected result.

**Type consistency:** `CloseStackRequest { stack }` identical across Tasks 4/8/9. `decide_closable(&[(Entity,i64,bool)], usize)` matches its caller in Task 8. `TidyAction`/`tidy_choice`/label consts consistent Tasks 6/8. `is_changed(&Path, &mut Vec<(PathBuf,HashSet<String>)>)` matches caller. `dirty_set -> (PathBuf, HashSet<String>)` consumed by `is_changed`. `file_stacks_for -> Option<(Entity, Vec<(Entity,String)>)>` matches Task 8 destructure.

**Known verification points (flagged in-task, not blockers):** exact `rfd`/`vmux_layout` dep declaration form to copy (Task 5); `SettingsSaveRequest` re-export path (Task 8); Bevy required-components on `Pane`/`Stack` in the Task 4 test (mirror existing tests in the same module if the compiler asks for more).
