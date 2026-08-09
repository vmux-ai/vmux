use super::*;
use bevy::{
    asset::Assets,
    picking::Pickable,
    prelude::{App, MinimalPlugins, Startup, With},
};

fn test_layout_settings() -> LayoutSettings {
    LayoutSettings {
        radius: 0.0,
        window: crate::settings::WindowSettings { padding: 0.0 },
        pane: crate::settings::PaneSettings { gap: 0.0 },
        side_sheet: crate::settings::SideSheetSettings::default(),
        focus_ring: crate::settings::FocusRingSettings::default(),
    }
}

#[test]
fn mesh_focus_ring_hidden_in_windowed_user_mode() {
    let src = include_str!("focus_ring.rs");
    let sync = src
        .split("fn sync_focus_ring_to_active_pane")
        .nth(1)
        .and_then(|t| t.split("fn tick_focus_ring_gradient_time").next())
        .unwrap_or_default();
    assert!(sync.contains("InteractionMode::User"));
    assert!(sync.contains("target_os = \"macos\""));
}

#[test]
fn focus_ring_does_not_capture_pointer_events() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_layout_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<FocusRingMaterial>>()
        .add_systems(Startup, spawn_focus_ring);
    app.update();

    let pickable = app
        .world_mut()
        .query_filtered::<&Pickable, With<FocusRing>>()
        .single(app.world())
        .expect("focus ring pickable");

    assert_eq!(pickable, &Pickable::IGNORE);
}

#[test]
fn hidden_focus_ring_does_not_advance_gradient_time() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<FocusRingMaterial>>()
        .add_systems(Update, tick_focus_ring_gradient_time);

    let mut material = build_focus_ring_material(320.0, 240.0, &test_layout_settings(), 7.0, false);
    material.gradient_params.w = 7.0;
    let handle = app
        .world_mut()
        .resource_mut::<Assets<FocusRingMaterial>>()
        .add(material);
    app.world_mut().spawn((
        FocusRing,
        MeshMaterial3d(handle.clone()),
        Visibility::Hidden,
    ));

    app.update();

    let material = app
        .world()
        .resource::<Assets<FocusRingMaterial>>()
        .get(handle.id())
        .expect("focus ring material");
    assert_eq!(material.gradient_params.w, 7.0);
}

#[test]
fn visible_focus_ring_advances_gradient_time() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<FocusRingMaterial>>()
        .add_systems(Update, tick_focus_ring_gradient_time);

    let mut material =
        build_focus_ring_material(320.0, 240.0, &test_layout_settings(), -1.0, false);
    material.gradient_params.w = -1.0;
    let handle = app
        .world_mut()
        .resource_mut::<Assets<FocusRingMaterial>>()
        .add(material);
    app.world_mut().spawn((
        FocusRing,
        MeshMaterial3d(handle.clone()),
        Visibility::Visible,
    ));

    app.update();

    let material = app
        .world()
        .resource::<Assets<FocusRingMaterial>>()
        .get(handle.id())
        .expect("focus ring material");
    assert_ne!(material.gradient_params.w, -1.0);
}
