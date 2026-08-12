//! Fixtures shared by more than one slice's tests.
//!
//! A fixture used by a single slice belongs in that slice; these are here because the world they
//! build is the same one several features need to observe.

use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_layout::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

pub(crate) fn init_worktree_test_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
    git(&["add", "seed.txt"]);
    git(&["commit", "-qm", "init"]);
    repo
}

pub(crate) fn test_settings() -> AppSettings {
    AppSettings {
        browser: BrowserSettings {
            startup_url: "about:blank".to_string(),
            ..Default::default()
        },
        layout: LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 0.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        },
        shortcuts: ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        agent: vmux_setting::AgentSettings::default(),
        spaces: Default::default(),
        recording: Default::default(),
        editor: Default::default(),
        appearance: Default::default(),
    }
}

pub(crate) fn spawn_stack_in_pane(app: &mut App, pane: Entity, url: &str) -> Entity {
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    app.world_mut().entity_mut(stack).insert(PageMetadata {
        url: url.to_string(),
        ..default()
    });
    stack
}

pub(crate) fn close_stack_requests(app: &App) -> Vec<Entity> {
    let messages = app
        .world()
        .resource::<bevy::ecs::message::Messages<vmux_layout::CloseStackRequest>>();
    let mut cursor = messages.get_cursor();
    cursor.read(messages).map(|m| m.stack).collect()
}

pub(crate) fn spawn_file_preview_stack(app: &mut App, pane: Entity, ts: i64, url: &str) -> Entity {
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::LastActivatedAt(ts),
            ChildOf(pane),
        ))
        .id();
    app.world_mut().spawn((
        PageMetadata {
            url: url.to_string(),
            ..default()
        },
        ChildOf(stack),
    ));
    stack
}
