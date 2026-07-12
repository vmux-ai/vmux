# Dynamic Tab Directory Rebinding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebind a tab to the same-repository checkout observed in agent file activity so its sidebar state and future terminal commands use the active checkout.

**Architecture:** `vmux_git` provides stable repository identity through Git's common directory. `vmux_agent` converts anchored `FileTouched` requests into a typed layout observation, and `vmux_layout` validates same-repository identity before updating `Tab.startup_dir` and clearing stale `TabWorktree` metadata. Agent-created run terminals resolve the mutable tab directory before the immutable agent launch directory.

**Tech Stack:** Rust 2024, Bevy ECS messages and systems, Git CLI, Cargo test, rustfmt.

---

### Task 1: Identify one repository across main and linked worktrees

**Files:**
- Modify: `crates/vmux_git/src/worktree.rs`
- Test: `crates/vmux_git/src/worktree.rs`

- [ ] **Step 1: Write the failing repository-identity test**

Add this test beside `info_exclude_path_shared_across_main_and_linked_worktree`:

```rust
#[test]
fn common_dir_identifies_repository_across_worktrees() {
    let repo = test_repo::init();
    commit_initial(repo.path());
    let wt = repo.path().join(".worktrees/feat");
    worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();

    let other = test_repo::init();
    commit_initial(other.path());
    let not_repo = tempfile::tempdir().unwrap();

    let main_common = common_dir_of(repo.path()).unwrap();
    assert_eq!(common_dir_of(&wt).unwrap(), main_common);
    assert_ne!(common_dir_of(other.path()).unwrap(), main_common);
    assert!(common_dir_of(not_repo.path()).is_err());
}
```

- [ ] **Step 2: Run the test and verify the missing helper fails compilation**

Run:

```bash
cargo test -p vmux_git common_dir_identifies_repository_across_worktrees -- --nocapture
```

Expected: compilation fails because `common_dir_of` is undefined.

- [ ] **Step 3: Implement common-directory resolution**

Add after `repo_root_of`:

```rust
/// The absolute common Git directory shared by a repository's main and linked worktrees.
pub fn common_dir_of(dir: &Path) -> Result<PathBuf, GitError> {
    let (stdout, stderr, ok) = git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ],
    )?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    let path = PathBuf::from(stdout.trim());
    if path.as_os_str().is_empty() {
        return Err(GitError("git common dir is empty".to_string()));
    }
    Ok(path.canonicalize().unwrap_or(path))
}
```

- [ ] **Step 4: Run the test and verify it passes**

Run:

```bash
cargo test -p vmux_git common_dir_identifies_repository_across_worktrees -- --nocapture
```

Expected: `1 passed; 0 failed`.

- [ ] **Step 5: Commit repository identity**

```bash
git add crates/vmux_git/src/worktree.rs
git commit -m "feat(git): identify repositories across worktrees"
```

### Task 2: Rebind tab state from a validated directory observation

**Files:**
- Modify: `crates/vmux_layout/src/worktree.rs`
- Test: `crates/vmux_layout/src/worktree.rs`

- [ ] **Step 1: Define the observation message and write failing rebind tests**

Add this public message after `WorktreePlugin`:

```rust
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct TabDirectoryObserved {
    pub tab: Entity,
    pub path: PathBuf,
}
```

Add this helper inside the test module:

```rust
fn observe(app: &mut App, tab: Entity, path: &Path) {
    app.world_mut()
        .resource_mut::<Messages<TabDirectoryObserved>>()
        .write(TabDirectoryObserved {
            tab,
            path: path.to_path_buf(),
        });
    app.update();
}
```

Add these tests after `reconcile_drops_worktree_when_path_missing`:

```rust
#[test]
fn observation_rebinds_managed_tab_to_same_repo_checkout() {
    let repo = init_repo();
    let managed = create_worktree_blocking(repo.path(), "managed").unwrap();
    let touched = repo.path().join("seed.txt");
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn((
            Tab {
                name: "tab".into(),
                startup_dir: Some(managed.path.to_string_lossy().into_owned()),
            },
            TabWorktree {
                repo_root: managed.repo_root.to_string_lossy().into_owned(),
                branch: managed.branch,
                base_ref: managed.base_ref,
            },
        ))
        .id();

    observe(&mut app, tab, &touched);

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(repo.path().to_string_lossy().as_ref())
    );
    assert!(app.world().get::<TabWorktree>(tab).is_none());
    assert!(managed.path.is_dir(), "old checkout is preserved");
}

#[test]
fn observation_rebinds_repeatedly_within_same_repo() {
    let repo = init_repo();
    let first = create_worktree_blocking(repo.path(), "first").unwrap();
    let second_path = repo.path().join(".worktrees/second");
    worktree::worktree_add(repo.path(), &second_path, "vmux/second", "main").unwrap();
    let second_file = second_path.join("seed.txt");
    let main_file = repo.path().join("seed.txt");
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(first.path.to_string_lossy().into_owned()),
        })
        .id();

    observe(&mut app, tab, &second_file);
    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(second_path.to_string_lossy().as_ref())
    );

    observe(&mut app, tab, &main_file);
    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(repo.path().to_string_lossy().as_ref())
    );
}

#[test]
fn observation_ignores_unrelated_and_invalid_paths() {
    let repo = init_repo();
    let other = init_repo();
    let non_git = tempfile::tempdir().unwrap();
    let non_git_file = non_git.path().join("file.txt");
    std::fs::write(&non_git_file, "x").unwrap();
    let missing = repo.path().join("missing.txt");
    let original = repo.path().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe(&mut app, tab, &other.path().join("seed.txt"));
    observe(&mut app, tab, &non_git_file);
    observe(&mut app, tab, &missing);

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}
```

- [ ] **Step 2: Register the message without implementing the consumer**

Change the plugin builder to:

```rust
impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TabDirectoryObserved>()
            .add_systems(Update, reconcile_tab_worktrees);
    }
}
```

- [ ] **Step 3: Run the layout tests and verify the rebind assertion fails**

Run:

```bash
cargo test -p vmux_layout observation_ -- --nocapture
```

Expected: tests compile, then `observation_rebinds_managed_tab_to_same_repo_checkout` fails because `startup_dir` remains the managed worktree.

- [ ] **Step 4: Implement best-effort same-repository rebinding**

Change the plugin builder to register both systems:

```rust
impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TabDirectoryObserved>().add_systems(
            Update,
            (reconcile_tab_worktrees, rebind_tab_directories),
        );
    }
}
```

Add before the test module:

```rust
fn observed_checkout_root(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let start = if path.is_dir() { path } else { path.parent()? };
    worktree::repo_root_of(start).ok()
}

fn rebind_tab_directories(
    mut reader: MessageReader<TabDirectoryObserved>,
    mut tabs: Query<&mut Tab>,
    managed: Query<(), With<TabWorktree>>,
    mut commands: Commands,
) {
    for observed in reader.read() {
        let Some(observed_root) = observed_checkout_root(&observed.path) else {
            continue;
        };
        let Ok(mut tab) = tabs.get_mut(observed.tab) else {
            continue;
        };
        let Some(current) = tab.startup_dir.as_deref() else {
            continue;
        };
        let Ok(current_common) = worktree::common_dir_of(Path::new(current)) else {
            continue;
        };
        let Ok(observed_common) = worktree::common_dir_of(&observed_root) else {
            continue;
        };
        if current_common != observed_common || Path::new(current) == observed_root.as_path() {
            continue;
        }
        tab.startup_dir = Some(observed_root.to_string_lossy().into_owned());
        if managed.contains(observed.tab) {
            commands.entity(observed.tab).remove::<TabWorktree>();
        }
    }
}
```

- [ ] **Step 5: Run all worktree layout tests**

Run:

```bash
cargo test -p vmux_layout worktree::tests -- --nocapture
```

Expected: all `worktree::tests` pass.

- [ ] **Step 6: Commit layout rebinding**

```bash
git add crates/vmux_layout/src/worktree.rs
git commit -m "feat(layout): rebind tab directory from observations"
```

### Task 3: Emit directory observations from agent file activity

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`
- Test: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Write the failing file-touch integration test**

Add after `file_touch_url_builds_goto_fragment`:

```rust
#[test]
fn file_touch_emits_tab_directory_observation_when_file_follow_is_disabled() {
    let mut settings = test_settings();
    settings.agent.follow_files = false;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_layout::worktree::TabDirectoryObserved>()
        .insert_resource(settings)
        .add_systems(Update, handle_agent_file_touch);

    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab::default())
        .id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((anchor, ChildOf(stack)));
    let path = std::env::temp_dir().join("vmux-observed-file.rs");

    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::FileTouched {
                anchor,
                path: path.to_string_lossy().into_owned(),
                line: None,
                col: None,
                end_col: None,
                kind: vmux_service::protocol::FileTouchKind::Read,
            },
        });

    app.update();

    let messages = app
        .world()
        .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
    let mut cursor = messages.get_cursor();
    let observations: Vec<_> = cursor.read(messages).cloned().collect();
    assert_eq!(
        observations,
        vec![vmux_layout::worktree::TabDirectoryObserved { tab, path }]
    );
    let previews = app
        .world()
        .resource::<Messages<vmux_layout::OpenBesideRequest>>();
    let mut preview_cursor = previews.get_cursor();
    assert_eq!(
        preview_cursor.read(previews).count(),
        0,
        "file-follow setting still controls preview panes"
    );
}
```

- [ ] **Step 2: Run the test and verify no observation is emitted**

Run:

```bash
cargo test -p vmux_agent file_touch_emits_tab_directory_observation_when_file_follow_is_disabled -- --nocapture
```

Expected: assertion fails with an empty observation list.

- [ ] **Step 3: Add observation plumbing to `AgentFileResolve`**

Add fields:

```rust
observations: MessageWriter<'w, vmux_layout::worktree::TabDirectoryObserved>,
tabs: Query<'w, 's, (), With<vmux_layout::tab::Tab>>,
```

Add this method to `impl AgentFileResolve`:

```rust
fn ancestor_tab(&self, entity: Entity) -> Option<Entity> {
    use bevy::ecs::relationship::Relationship;
    let mut current = entity;
    loop {
        if self.tabs.contains(current) {
            return Some(current);
        }
        current = self.child_of.get(current).ok()?.get();
    }
}
```

- [ ] **Step 4: Emit observations independently from file-preview settings**

Replace the early `follow_files` return and the start of the loop in `handle_agent_file_touch` with:

```rust
fn handle_agent_file_touch(
    mut reader: MessageReader<AgentCommandRequest>,
    mut resolve: AgentFileResolve,
    settings: Res<AppSettings>,
) {
    for request in reader.read() {
        let ServiceAgentCommand::FileTouched {
            anchor,
            path,
            line,
            col,
            end_col,
            ..
        } = &request.command
        else {
            continue;
        };
        let Some(agent_pane) = resolve.agent_pane(*anchor) else {
            continue;
        };
        if let Some(tab) = resolve.ancestor_tab(agent_pane) {
            resolve
                .observations
                .write(vmux_layout::worktree::TabDirectoryObserved {
                    tab,
                    path: PathBuf::from(path),
                });
        }
        if !settings.agent.follow_files {
            continue;
        }
        let existing = resolve.file_page_for(agent_pane);
```

Keep the existing `OpenBesideRequest` and active-pane code after `existing` unchanged.

Initialize the message resource in `AgentPlugin::build` beside the existing `OpenBesideRequest` resource:

```rust
.init_resource::<
    bevy::ecs::message::Messages<vmux_layout::worktree::TabDirectoryObserved>,
>()
```

- [ ] **Step 5: Run the file-touch tests**

Run:

```bash
cargo test -p vmux_agent file_touch_ -- --nocapture
```

Expected: all matching tests pass, including the new observation test and existing URL test.

- [ ] **Step 6: Commit agent observations**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): observe checkout from file activity"
```

### Task 4: Use the rebound tab directory for future agent run commands

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`
- Test: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Write a failing tab-priority test and update existing calls**

Add before `run_terminal_cwd_inherits_agent_launch_dir`:

```rust
#[test]
fn run_terminal_cwd_prefers_tab_dir() {
    let tab_dir = std::env::temp_dir().join(format!("vmux-tab-cwd-{}", std::process::id()));
    let agent_dir = std::env::temp_dir().join(format!("vmux-agent-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&tab_dir).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    assert_eq!(
        run_terminal_cwd(
            Some(tab_dir.to_string_lossy().as_ref()),
            Some(agent_dir.to_string_lossy().as_ref()),
            None,
        ),
        tab_dir.clone()
    );
    let _ = std::fs::remove_dir_all(&agent_dir);
    let _ = std::fs::remove_dir_all(&tab_dir);
}

#[test]
fn run_terminal_launch_must_match_rebound_cwd_for_reuse() {
    let current = std::env::temp_dir().join(format!("vmux-current-cwd-{}", std::process::id()));
    let stale = std::env::temp_dir().join(format!("vmux-stale-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&current).unwrap();
    std::fs::create_dir_all(&stale).unwrap();
    assert!(run_terminal_launch_matches_cwd(
        current.to_string_lossy().as_ref(),
        &current,
    ));
    assert!(!run_terminal_launch_matches_cwd(
        stale.to_string_lossy().as_ref(),
        &current,
    ));
    let _ = std::fs::remove_dir_all(&stale);
    let _ = std::fs::remove_dir_all(&current);
}
```

Update the existing tests to call the desired three-argument API:

```rust
let got = run_terminal_cwd(None, Some(&dir.to_string_lossy()), None);
```

```rust
assert_eq!(run_terminal_cwd(None, Some(""), None), default_space_dir());
assert_eq!(run_terminal_cwd(None, None, None), default_space_dir());
```

- [ ] **Step 2: Run the test and verify the signature mismatch fails compilation**

Run:

```bash
cargo test -p vmux_agent run_terminal_cwd_ -- --nocapture
```

Expected: compilation fails because `run_terminal_cwd` accepts two arguments and
`run_terminal_launch_matches_cwd` is undefined.

- [ ] **Step 3: Add tab-directory priority to the cwd resolver**

Replace the helper with:

```rust
fn run_terminal_cwd(
    tab_cwd: Option<&str>,
    agent_launch_cwd: Option<&str>,
    active_space: Option<&ActiveSpace>,
) -> PathBuf {
    if let Some(Ok(Some(path))) = tab_cwd.map(valid_cwd) {
        return path;
    }
    if let Some(Ok(Some(path))) = agent_launch_cwd.map(valid_cwd) {
        return path;
    }
    active_space
        .map(|s| space_dir(&s.record.id))
        .unwrap_or_else(default_space_dir)
}

fn run_terminal_launch_matches_cwd(launch_cwd: &str, desired_cwd: &Path) -> bool {
    let Some(launch_cwd) = valid_cwd(launch_cwd).ok().flatten() else {
        return false;
    };
    let launch_cwd = launch_cwd.canonicalize().unwrap_or(launch_cwd);
    let desired_cwd = desired_cwd
        .canonicalize()
        .unwrap_or_else(|_| desired_cwd.to_path_buf());
    launch_cwd == desired_cwd
}
```

- [ ] **Step 4: Resolve the current ancestor tab and reject stale automatic reuse**

In the new-terminal branch of `ServiceAgentCommand::Run`, immediately after resolving
`self_pane`, add:

```rust
let tab_cwd = {
    let mut current = self_pane;
    loop {
        if let Ok(tab) = tabs.get(current) {
            break tab.startup_dir.clone();
        }
        match ctx.child_of_q.get(current) {
            Ok(child_of) => current = child_of.parent(),
            Err(_) => break None,
        }
    }
};
let agent_cwd = launch_q.get(agent_term).ok().map(|l| l.cwd.clone());
let cwd = run_terminal_cwd(
    tab_cwd.as_deref(),
    agent_cwd.as_deref(),
    active_space.as_deref(),
);
```

After `run_terminal_candidates` returns, retain only terminals launched in the current tab
directory:

```rust
let candidates: Vec<_> = candidates
    .into_iter()
    .filter(|candidate| {
        term_pids
            .iter()
            .find(|(_, pid)| **pid == candidate.pid)
            .and_then(|(entity, _)| launch_q.get(entity).ok())
            .is_some_and(|launch| run_terminal_launch_matches_cwd(&launch.cwd, &cwd))
    })
    .collect();
```

Remove the old `agent_cwd` and `cwd` calculation immediately before
`TerminalStackSpawnRequest`; reuse the values calculated above.

- [ ] **Step 5: Run agent cwd and placement tests**

Run:

```bash
cargo test -p vmux_agent run_terminal_cwd_ -- --nocapture
cargo test -p vmux_agent run_terminal_launch_must_match_rebound_cwd_for_reuse -- --nocapture
cargo test -p vmux_agent agent_run_spawns_terminal_before_next_agent_command_frame -- --nocapture
```

Expected: all matching tests pass.

- [ ] **Step 6: Commit run-terminal rebinding**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "fix(agent): run commands from rebound tab directory"
```

### Task 5: Format, run targeted verification, and remove the completed plan

**Files:**
- Modify: `crates/vmux_git/src/worktree.rs`
- Modify: `crates/vmux_layout/src/worktree.rs`
- Modify: `crates/vmux_agent/src/plugin.rs`
- Delete: `docs/plans/2026-07-12-tab-directory-rebind.md`

- [ ] **Step 1: Format only changed Rust files**

Run:

```bash
rustfmt --edition 2024 crates/vmux_git/src/worktree.rs crates/vmux_layout/src/worktree.rs crates/vmux_agent/src/plugin.rs
```

Expected: command exits successfully.

- [ ] **Step 2: Run targeted package tests**

Run:

```bash
cargo test -p vmux_git common_dir_identifies_repository_across_worktrees -- --nocapture
cargo test -p vmux_layout worktree::tests -- --nocapture
cargo test -p vmux_agent file_touch_ -- --nocapture
cargo test -p vmux_agent run_terminal_cwd_ -- --nocapture
cargo test -p vmux_agent run_terminal_launch_must_match_rebound_cwd_for_reuse -- --nocapture
cargo test -p vmux_agent agent_run_spawns_terminal_before_next_agent_command_frame -- --nocapture
```

Expected: every command reports zero failures.

- [ ] **Step 3: Check the final diff**

Run:

```bash
git diff --check
git status --short
git diff --stat HEAD~3
```

Expected: no whitespace errors; only the design spec, plan, and intended Rust files appear before plan removal.

- [ ] **Step 4: Delete the fully implemented plan**

Delete `docs/plans/2026-07-12-tab-directory-rebind.md` with `apply_patch`, as required by repository instructions.

- [ ] **Step 5: Commit formatting and plan cleanup if needed**

```bash
git add crates/vmux_git/src/worktree.rs crates/vmux_layout/src/worktree.rs crates/vmux_agent/src/plugin.rs docs/plans/2026-07-12-tab-directory-rebind.md
git commit -m "chore: finalize tab directory rebinding"
```

If formatting produced no Rust changes, this commit records only deletion of the completed plan.
