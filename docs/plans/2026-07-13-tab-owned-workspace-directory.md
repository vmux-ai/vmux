# Tab-Owned Workspace Directory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every tab persist one authoritative workspace path, display that exact path, and rebind it from agent file activity according to read/edit repository boundaries.

**Architecture:** Resolve the active space/settings directory into an `EffectiveStartupDir` resource, copy it into every new `Tab`, and materialize only legacy tabs whose persisted value is absent. Keep rebinding in `vmux_layout::worktree`, but carry an explicit read/edit kind so repository transitions follow the approved policy. Browser UI and future process launches consume the stored tab value instead of recomputing settings.

**Tech Stack:** Rust, Bevy ECS messages/resources/systems, vmux settings/space/layout/agent/browser crates, Git checkout identity, cargo test.

---

### Task 1: Freeze a workspace path into every new tab

**Files:**
- Modify: `crates/vmux_layout/src/settings.rs`
- Modify: `crates/vmux_layout/src/lib.rs`
- Modify: `crates/vmux_layout/src/window.rs`
- Modify: `crates/vmux_layout/src/tab.rs`
- Modify: `crates/vmux_space/src/plugin.rs`
- Modify: `crates/vmux_setting/src/plugin.rs`

- [ ] **Step 1: Write failing tab-spawn tests**

Add tests in `crates/vmux_layout/src/window.rs` that insert a temporary existing directory as `EffectiveStartupDir`, send a `TabLayoutSpawnRequest`, and assert the spawned `Tab.startup_dir` equals that path. Add a second test with an explicit request directory and assert it wins.

```rust
fn tab_spawn_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::NewStackContext>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings { padding: 0.0 },
            pane: crate::settings::PaneSettings { gap: 0.0 },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        })
        .add_systems(Update, spawn_requested_tab_layouts);
    app
}

#[test]
fn spawned_tab_stores_effective_startup_dir() {
    let effective = tempfile::tempdir().unwrap();
    let mut app = tab_spawn_test_app();
    let main = app.world_mut().spawn(Main).id();
    let window = app.world_mut().spawn(PrimaryWindow).id();
    app.world_mut()
        .resource_mut::<Messages<TabLayoutSpawnRequest>>()
        .write(TabLayoutSpawnRequest {
            main,
            primary_window: window,
            name: None,
            startup_dir: effective.path().to_string_lossy().into_owned(),
            content: TabLayoutSpawnContent::StartupUrlOrPrompt,
            clear_pending_stack: false,
            focus: true,
        });
    app.update();
    let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
    assert_eq!(tab.startup_dir.as_deref(), effective.path().to_str());
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test -p vmux_layout spawned_tab_stores_effective_startup_dir -- --exact`

Expected: FAIL because tab requests and spawn logic still permit an absent workspace path.

- [ ] **Step 3: Add the effective directory resource and require resolved spawn paths**

Add to `crates/vmux_layout/src/settings.rs`:

```rust
#[derive(Resource, Clone, Debug)]
pub struct EffectiveStartupDir(pub std::path::PathBuf);

impl Default for EffectiveStartupDir {
    fn default() -> Self {
        Self(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")))
    }
}
```

Change `TabLayoutSpawnRequest.startup_dir` in `crates/vmux_layout/src/lib.rs` from `Option<String>` to `String`. In `spawn_requested_tab_layouts`, always insert the tab model:

```rust
commands.entity(tab_e).insert(Tab {
    name: request.name.clone().unwrap_or_default(),
    startup_dir: Some(request.startup_dir.clone()),
});
```

Initialize `EffectiveStartupDir` in `SettingsPlugin`. Add `update_effective_startup_dir` beside `update_effective_startup_url` in `vmux_space::plugin`:

```rust
fn update_effective_startup_dir(
    settings: Option<Res<vmux_setting::AppSettings>>,
    active: Option<Res<ActiveSpace>>,
    mut effective: ResMut<vmux_layout::settings::EffectiveStartupDir>,
) {
    let (Some(settings), Some(active)) = (settings, active) else {
        return;
    };
    if settings.is_changed() || active.is_changed() {
        effective.0 = vmux_setting::resolve_startup_dir(&settings, &active.record.id);
    }
}
```

Schedule its startup run after `SettingsLoadSet` and before `LayoutStartupSet::DefaultTab`, and its update run after `sync_active_space_record`.

- [ ] **Step 4: Update every tab request producer**

Inject `Res<EffectiveStartupDir>` into `request_default_layout`, `handle_tab_commands`, and `on_tabs_command_emit`; set `startup_dir` to `effective.0.to_string_lossy().into_owned()` for ordinary new/replacement tabs. Preserve the folder chooser path for `TabCommand::New`.

For `on_space_command` and `handle_open_in_new_space`, inject `Res<AppSettings>` and resolve the just-created space directly so same-frame requests cannot inherit the previous space:

```rust
let startup_dir = vmux_setting::resolve_startup_dir(&settings, &id)
    .to_string_lossy()
    .into_owned();
layout_requests.write(TabLayoutSpawnRequest {
    main,
    primary_window: *primary_window,
    name: None,
    startup_dir,
    content,
    clear_pending_stack: true,
    focus: true,
});
```

Update all test requests in `crates/vmux_layout/src/window.rs`, `crates/vmux_layout/src/tab.rs`, and `crates/vmux_space/src/plugin.rs` with an existing temporary directory. Do not create a per-tab directory or worktree.

- [ ] **Step 5: Run focused tests**

Run: `cargo test -p vmux_layout spawned_tab_stores_effective_startup_dir`

Expected: PASS.

Run: `cargo test -p vmux_layout tab::tests`

Expected: PASS.

Run: `cargo test -p vmux_space plugin::tests`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/settings.rs crates/vmux_layout/src/lib.rs crates/vmux_layout/src/window.rs crates/vmux_layout/src/tab.rs crates/vmux_space/src/plugin.rs crates/vmux_setting/src/plugin.rs
git commit -m "feat(layout): freeze workspace directory on tab creation"
```

### Task 2: Materialize legacy tabs once without following later settings changes

**Files:**
- Modify: `crates/vmux_space/src/plugin.rs`
- Test: `crates/vmux_space/src/plugin.rs`

- [ ] **Step 1: Write failing materialization tests**

Add Bevy integration tests for a tab with `startup_dir: None` parented under a space. The first test configures a per-space directory and asserts one update stores it. The second changes settings after materialization and asserts the existing tab does not move.

```rust
#[test]
fn legacy_tab_materializes_space_startup_dir_once() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.spaces.insert(
        "work".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(first.path().to_string_lossy().into_owned()),
        },
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(settings)
        .insert_resource(ActiveSpace {
            record: work_space_record(),
        })
        .add_systems(Update, materialize_tab_startup_dirs);
    let space = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("work".into()),
        ))
        .id();
    let tab = app
        .world_mut()
        .spawn((vmux_layout::tab::Tab::default(), ChildOf(space)))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<vmux_layout::tab::Tab>(tab).unwrap().startup_dir.as_deref(),
        first.path().to_str()
    );
    app.world_mut().resource_mut::<vmux_setting::AppSettings>()
        .spaces.get_mut("work").unwrap().startup_dir =
        Some(second.path().to_string_lossy().into_owned());
    app.update();
    assert_eq!(
        app.world().get::<vmux_layout::tab::Tab>(tab).unwrap().startup_dir.as_deref(),
        first.path().to_str()
    );
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test -p vmux_space legacy_tab_materializes_space_startup_dir_once -- --exact`

Expected: FAIL because no system writes missing persisted tab directories.

- [ ] **Step 3: Implement one-shot legacy materialization**

Add a system that only mutates tabs whose stored path is absent:

```rust
fn materialize_tab_startup_dirs(
    settings: Res<vmux_setting::AppSettings>,
    active: Res<ActiveSpace>,
    spaces: Query<&vmux_layout::space::SpaceId, With<vmux_layout::space::Space>>,
    mut tabs: Query<(&ChildOf, &mut vmux_layout::tab::Tab)>,
) {
    for (parent, mut tab) in &mut tabs {
        if tab.startup_dir.is_some() {
            continue;
        }
        let space_id = spaces
            .get(parent.parent())
            .map(|id| id.0.as_str())
            .unwrap_or(active.record.id.as_str());
        tab.startup_dir = Some(
            vmux_setting::resolve_startup_dir(&settings, space_id)
                .to_string_lossy()
                .into_owned(),
        );
    }
}
```

Register it in `SpacePlugin` Update after `sync_active_space_record`. It must never rewrite `Some`, so later settings changes affect future tabs only. Existing `Changed<Tab>` persistence tracking saves the migration.

- [ ] **Step 4: Run focused tests**

Run: `cargo test -p vmux_space legacy_tab_materializes_space_startup_dir_once`

Expected: PASS.

Run: `cargo test -p vmux_desktop changing_tab_startup_dir_marks_store_dirty`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_space/src/plugin.rs
git commit -m "fix(space): materialize legacy tab workspace paths"
```

### Task 3: Make the sidebar consume only the stored tab path

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs`
- Test: `crates/vmux_browser/src/lib.rs`

- [ ] **Step 1: Write a failing authoritative-path unit test**

Extract a small resolver and test that it returns the stored tab path verbatim and returns `None` when the legacy value is absent:

```rust
fn stored_tab_dir(tab: &Tab) -> Option<std::path::PathBuf> {
    tab.startup_dir.as_deref().map(std::path::PathBuf::from)
}

#[test]
fn stored_tab_dir_is_sidebar_source_of_truth() {
    let tab = Tab {
        name: "test".into(),
        startup_dir: Some("/tmp/agent-checkout".into()),
    };
    assert_eq!(stored_tab_dir(&tab), Some(std::path::PathBuf::from("/tmp/agent-checkout")));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test -p vmux_browser stored_tab_dir_is_sidebar_source_of_truth -- --exact`

Expected: FAIL because the helper does not exist and the sidebar recomputes the path through settings.

- [ ] **Step 3: Remove sidebar fallback resolution**

In `push_tab_boundary_emit`, remove the `ActiveSpace` parameter and the tab/space/global/default resolver call. Build a boundary only when the focused tab has a stored path:

```rust
let boundary = focus.tab.and_then(|tab_e| {
    let tab = tabs.get(tab_e).ok()?;
    let path = stored_tab_dir(tab)?;
    let dir_key = path.to_string_lossy().into_owned();
    let now = time.elapsed_secs();
    if git_cache.0 != dir_key || now - git_cache.1 > 3.0 {
        *git_cache = (dir_key, now, vmux_git::worktree::repo_info(&path));
    }
    let info = git_cache.2.clone();
    let wt = worktrees.get(tab_e).ok();
    let branch = info.as_ref().map(|i| i.branch.clone()).unwrap_or_default();
    let base_ref = wt.map(|w| w.base_ref.clone()).unwrap_or_default();
    let mut leaves = Vec::new();
    collect_leaf_panes(tab_e, &all_children, &leaf_pane_q, &mut leaves);
    Some(TabBoundary {
        effective_dir: abbreviate_home(&path),
        source: "tab".to_string(),
        is_git_repo: info.is_some(),
        is_worktree: info.as_ref().is_some_and(|i| i.is_worktree),
        branch,
        base_ref,
        uncommitted: info.as_ref().map(|i| i.uncommitted).unwrap_or(0),
        ahead: info.as_ref().map(|i| i.ahead).unwrap_or(0),
        pane_count: leaves.len() as u32,
    })
});
```

Delete `dir_source_label` and remove only the now-unused `DirSource`, `resolve_startup_dir_for_tab_with_source`, and `ActiveSpace` imports. Keep `AppSettings` imports used by unrelated browser systems.

- [ ] **Step 4: Run focused tests**

Run: `cargo test -p vmux_browser stored_tab_dir_is_sidebar_source_of_truth`

Expected: PASS.

Run: `cargo check -p vmux_browser`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "fix(browser): show authoritative tab workspace path"
```

### Task 4: Rebind tabs using repository identity and touch kind

**Files:**
- Modify: `crates/vmux_layout/src/worktree.rs`
- Modify: `crates/vmux_agent/src/plugin.rs`
- Test: `crates/vmux_layout/src/worktree.rs`
- Test: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Write failing policy tests**

Add `TabDirectoryObservationKind::{Read, Edit}` to test call sites first, then add these cases in `vmux_layout::worktree::tests`:

```rust
fn observe(
    app: &mut App,
    tab: Entity,
    path: &Path,
    kind: TabDirectoryObservationKind,
) {
    app.world_mut()
        .resource_mut::<Messages<TabDirectoryObserved>>()
        .write(TabDirectoryObserved {
            tab,
            path: path.to_path_buf(),
            kind,
        });
    app.update();
}

#[test]
fn cross_repo_read_does_not_rebind() {
    let current = init_repo();
    let observed = init_repo();
    let original = current.path().canonicalize().unwrap().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app.world_mut().spawn(Tab {
        name: "tab".into(),
        startup_dir: Some(original.clone()),
    }).id();
    observe(
        &mut app,
        tab,
        &observed.path().join("seed.txt"),
        TabDirectoryObservationKind::Read,
    );
    assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(), Some(original.as_str()));
}

#[test]
fn cross_repo_edit_rebinds() {
    let current = init_repo();
    let observed = init_repo();
    let expected = observed.path().canonicalize().unwrap().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app.world_mut().spawn(Tab {
        name: "tab".into(),
        startup_dir: Some(current.path().to_string_lossy().into_owned()),
    }).id();
    observe(
        &mut app,
        tab,
        &observed.path().join("seed.txt"),
        TabDirectoryObservationKind::Edit,
    );
    assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(), Some(expected.as_str()));
}

#[test]
fn non_git_current_read_does_not_rebind() {
    let current = tempfile::tempdir().unwrap();
    let observed = init_repo();
    let original = current.path().canonicalize().unwrap().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app.world_mut().spawn(Tab {
        name: "tab".into(),
        startup_dir: Some(original.clone()),
    }).id();
    observe(
        &mut app,
        tab,
        &observed.path().join("seed.txt"),
        TabDirectoryObservationKind::Read,
    );
    assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(), Some(original.as_str()));
}

#[test]
fn non_git_current_edit_rebinds() {
    let current = tempfile::tempdir().unwrap();
    let observed = init_repo();
    let expected = observed.path().canonicalize().unwrap().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app.world_mut().spawn(Tab {
        name: "tab".into(),
        startup_dir: Some(current.path().to_string_lossy().into_owned()),
    }).id();
    observe(
        &mut app,
        tab,
        &observed.path().join("seed.txt"),
        TabDirectoryObservationKind::Edit,
    );
    assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(), Some(expected.as_str()));
}
```

Update the existing same-repository worktree test to run once with `Read` and once with `Edit`; both must rebind. Keep non-Git observed paths as no-ops for both kinds.

- [ ] **Step 2: Run policy tests and verify failure**

Run: `cargo test -p vmux_layout worktree::tests::cross_repo -- --nocapture`

Expected: FAIL because observations have no kind and different repositories are always ignored.

- [ ] **Step 3: Carry touch kind into layout observations**

Add in `crates/vmux_layout/src/worktree.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDirectoryObservationKind {
    Read,
    Edit,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct TabDirectoryObserved {
    pub tab: Entity,
    pub path: PathBuf,
    pub kind: TabDirectoryObservationKind,
}
```

In `handle_agent_file_touch`, bind `kind` from `ServiceAgentCommand::FileTouched` and map it:

```rust
let observation_kind = match kind {
    vmux_service::protocol::FileTouchKind::Read =>
        vmux_layout::worktree::TabDirectoryObservationKind::Read,
    vmux_service::protocol::FileTouchKind::Edit =>
        vmux_layout::worktree::TabDirectoryObservationKind::Edit,
};
resolve.observations.write(TabDirectoryObserved {
    tab,
    path: PathBuf::from(path),
    kind: observation_kind,
});
```

Update the agent observation test to assert the kind survives even when file-follow previews are disabled.

- [ ] **Step 4: Implement the approved policy**

After resolving `observed_info`, decide whether to rebind:

```rust
let current_info = cached_checkout_info(
    &mut checkout_cache,
    observed.tab,
    &current,
    |path| worktree::checkout_info(path).ok(),
);
let should_rebind = match current_info.as_ref() {
    Some(current_info) if current_info.root == observed_info.root => false,
    Some(current_info) if current_info.common_dir == observed_info.common_dir => true,
    Some(_) | None => observed.kind == TabDirectoryObservationKind::Edit,
};
if !should_rebind {
    continue;
}
```

Retain the fast same-checkout/nested-boundary checks, cache update, `TabWorktree` removal, and old checkout preservation.

- [ ] **Step 5: Run focused tests**

Run: `cargo test -p vmux_layout worktree::tests`

Expected: PASS.

Run: `cargo test -p vmux_agent file_touch_emits_tab_directory_observation_when_file_follow_is_disabled`

Expected: PASS.

Run: `cargo test -p vmux_agent run_terminal`

Expected: PASS; future run terminals still use the stored/rebound tab path and reject stale-cwd reuse.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/worktree.rs crates/vmux_agent/src/plugin.rs
git commit -m "feat(layout): rebind tab workspace from agent edits"
```

### Task 5: Verify persistence, scheduling, and complete integration

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`
- Test: `crates/vmux_agent/src/plugin.rs`
- Delete: `docs/plans/2026-07-13-tab-owned-workspace-directory.md`

- [ ] **Step 1: Add or update same-frame integration coverage**

Extend the existing file-touch-plus-run test so an `Edit` observation into a different repository and a run command in the same frame launch the terminal from the new repository root. Assert `TerminalLaunch.cwd` equals the rebound path.

```rust
assert_eq!(
    launch.cwd,
    edited_repo.path().canonicalize().unwrap().to_string_lossy()
);
```

- [ ] **Step 2: Run the integration test and verify behavior**

Run: `cargo test -p vmux_agent same_frame`

Expected: PASS with the existing observation producer before `TabDirectoryRebindSet` and run consumer after it.

- [ ] **Step 3: Run package verification**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo clippy -p vmux_layout -p vmux_space -p vmux_setting -p vmux_browser -p vmux_agent -p vmux_desktop --all-targets -- -D warnings`

Expected: PASS.

Run: `cargo test -p vmux_layout -p vmux_space -p vmux_setting -p vmux_browser -p vmux_agent -p vmux_desktop`

Expected: PASS.

- [ ] **Step 4: Delete the completed plan**

Run: `git rm docs/plans/2026-07-13-tab-owned-workspace-directory.md`

- [ ] **Step 5: Commit final integration**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "test: cover tab workspace directory integration"
```

- [ ] **Step 6: Push and monitor the existing PR**

Run: `git push origin fix/tab-directory-rebind`

Run: `gh pr checks 246 --watch`

Expected: all required checks PASS. Read every review thread, reply to every CodeRabbit thread with the fix commit or triage reason, and leave the PR unmerged until explicit authorization.
