use super::*;
use crate::{
    settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    },
    window::VmuxWindow,
};
use bevy::window::{Monitor, MonitorSelection, PrimaryWindow, WindowMode};

#[test]
fn window_padding_sync_runs_before_ui_layout() {
    let source = include_str!("toggle.rs");
    let plugin_build = source
        .split("impl Plugin for TogglePlugin")
        .nth(1)
        .and_then(|tail| tail.split("/// When").next())
        .unwrap_or_default();

    assert!(plugin_build.contains(".add_systems(\n                PostUpdate,"));
    assert!(
        plugin_build
            .contains("sync_window_padding_to_layout_hidden.before(bevy::ui::UiSystems::Layout)")
    );
}

#[test]
fn hidden_layout_padding_uses_layout_window_settings() {
    let source = include_str!("toggle.rs");
    let sync_fn = source
        .split("fn sync_window_padding_to_layout_hidden")
        .nth(1)
        .and_then(|tail| tail.split("fn handle_toggle").next())
        .unwrap_or_default();

    assert!(sync_fn.contains("settings.window.pad_top()"));
    assert!(sync_fn.contains("settings.window.pad_left()"));
}

#[test]
fn visible_fullscreen_layout_clears_top_left_padding() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(LayoutHidden(false))
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 16.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        })
        .add_systems(Update, sync_window_padding_to_layout_hidden);
    app.world_mut().spawn((
        Window {
            mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            ..default()
        },
        PrimaryWindow,
    ));
    let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

    app.update();

    let node = app.world().get::<Node>(root).expect("window node");
    assert_eq!(node.padding.top, Val::Px(0.0));
    assert_eq!(node.padding.left, Val::Px(0.0));
}

#[test]
fn visible_maximized_layout_clears_top_left_padding() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(LayoutHidden(false))
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 16.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        })
        .add_systems(Update, sync_window_padding_to_layout_hidden);
    app.world_mut().spawn((
        Window {
            resolution: (1200, 800).into(),
            ..default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn(Monitor {
        name: None,
        physical_width: 1200,
        physical_height: 800,
        physical_position: IVec2::ZERO,
        refresh_rate_millihertz: None,
        scale_factor: 1.0,
        video_modes: Vec::new(),
    });
    let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

    app.update();

    let node = app.world().get::<Node>(root).expect("window node");
    assert_eq!(node.padding.top, Val::Px(0.0));
    assert_eq!(node.padding.left, Val::Px(0.0));
}

#[test]
fn hidden_layout_uses_top_left_padding() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(LayoutHidden(true))
        .insert_resource(LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 16.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        })
        .add_systems(Update, sync_window_padding_to_layout_hidden);
    app.world_mut().spawn((
        Window {
            resolution: (1200, 800).into(),
            ..default()
        },
        PrimaryWindow,
    ));
    let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

    app.update();

    let node = app.world().get::<Node>(root).expect("window node");
    assert_eq!(node.padding.top, Val::Px(16.0));
    assert_eq!(node.padding.left, Val::Px(16.0));
}
