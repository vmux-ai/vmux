use super::*;
use vmux_layout::scene::InteractionMode;

#[test]
fn glass_backdrop_is_hidden_in_player_mode() {
    assert!(!glass_backdrop_visible(InteractionMode::Player));
    assert!(glass_backdrop_visible(InteractionMode::User));
}

#[test]
fn glass_install_does_not_reveal_window() {
    let source = include_str!("glass.rs");
    let install = source
        .split("fn install_window_glass")
        .nth(1)
        .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
        .unwrap_or_default();

    assert!(!install.contains("window.visible = true"));
    assert!(!install.contains("activate_native_window"));
}

#[test]
fn window_backdrop_uses_clear_glass_style() {
    let source = include_str!("glass.rs");
    let install = source
        .split("fn install_window_glass")
        .nth(1)
        .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
        .unwrap_or_default();

    assert!(install.contains("NSGlassEffectViewStyle::Clear"));
    assert!(!install.contains("NSGlassEffectViewStyle::Regular"));
}

#[test]
fn window_backdrop_uses_clear_glass_tint() {
    let source = include_str!("glass.rs");
    let install = source
        .split("fn install_window_glass")
        .nth(1)
        .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
        .unwrap_or_default();

    assert!(install.contains("glass.setTintColor(Some(&NSColor::clearColor()))"));
}

#[test]
fn window_backdrop_lives_in_nonactivating_child_window() {
    let source = include_str!("glass.rs");
    let install = source
        .split("fn install_window_glass")
        .nth(1)
        .and_then(|tail| tail.split("fn reveal_window_after_layout_ready").next())
        .unwrap_or_default();

    assert!(install.contains("NSPanel"));
    assert!(install.contains("NSWindowStyleMask::NonactivatingPanel"));
    assert!(install.contains("setIgnoresMouseEvents(true)"));
    assert!(install.contains("addChildWindow_ordered"));
    assert!(install.contains("NSWindowOrderingMode::Below"));
}

#[test]
fn window_backdrop_tracks_parent_window_frame() {
    let source = include_str!("glass.rs");
    let sync = source
        .split("fn sync_window_glass_visibility")
        .nth(1)
        .and_then(|tail| {
            tail.split("#[derive(Default)]\nstruct LayoutOverlay")
                .next()
        })
        .unwrap_or_default();

    assert!(sync.contains("backdrop_window.setFrame_display(parent_window.frame(), false)"));
}

#[test]
fn desktop_enables_nspanel_binding_for_glass_backdrop() {
    let manifest = include_str!("../Cargo.toml");

    assert!(manifest.contains("\"objc2-app-kit/NSPanel\""));
}

#[test]
fn layout_overlay_uses_layer_for_hit_test_passthrough() {
    let source = include_str!("glass.rs");
    let overlay = source
        .split("fn sync_layout_overlay")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_command_bar_overlay").next())
        .unwrap_or_default();

    assert!(overlay.contains("Retained<objc2_quartz_core::CALayer>"));
    assert!(overlay.contains("CALayer::new()"));
    assert!(overlay.contains("addSublayer"));
    assert!(overlay.contains("layer.setContents"));
    assert!(!overlay.contains("NSView::initWithFrame"));
}

#[test]
fn layout_overlay_keeps_host_and_overlay_layers_transparent() {
    let source = include_str!("glass.rs");
    let overlay = source
        .split("fn sync_layout_overlay")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_command_bar_overlay").next())
        .unwrap_or_default();

    assert!(overlay.contains("host_layer.setOpaque(false)"));
    assert!(overlay.contains("host_layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
    assert!(overlay.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
}

#[test]
fn native_overlay_blits_only_dirty_regions() {
    let dirty = vec![
        WebviewDirtyRect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        },
        WebviewDirtyRect {
            x: 50,
            y: 60,
            width: 70,
            height: 80,
        },
    ];

    let damage = OverlayDamage::from_frame(200, 100, &dirty);

    assert_eq!(
        native_overlay_blit_regions(200, 100, &damage, true).as_ref(),
        [
            dirty[0],
            WebviewDirtyRect {
                height: 40,
                ..dirty[1]
            },
        ]
    );
}

#[test]
fn native_overlay_resize_forces_full_blit() {
    let dirty = vec![WebviewDirtyRect {
        x: 10,
        y: 20,
        width: 30,
        height: 40,
    }];

    let damage = OverlayDamage::from_frame(200, 100, &dirty);

    assert_eq!(
        native_overlay_blit_regions(200, 100, &damage, false).as_ref(),
        [WebviewDirtyRect {
            x: 0,
            y: 0,
            width: 200,
            height: 100,
        }]
    );
}

#[test]
fn native_overlay_coalescing_unions_dirty_regions() {
    let previous = vec![WebviewDirtyRect {
        x: 10,
        y: 20,
        width: 30,
        height: 40,
    }];
    let latest = vec![WebviewDirtyRect {
        x: 100,
        y: 20,
        width: 30,
        height: 40,
    }];

    let dirty = coalesced_overlay_dirty(200, 100, &previous, &latest, true);

    assert_eq!(
        dirty.as_slice(),
        &[
            WebviewDirtyRect {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
            },
            WebviewDirtyRect {
                x: 100,
                y: 20,
                width: 30,
                height: 40,
            },
        ]
    );
}

#[test]
fn native_overlay_coalescing_merges_overlapping_regions() {
    let previous = vec![WebviewDirtyRect {
        x: 10,
        y: 20,
        width: 30,
        height: 40,
    }];
    let latest = vec![WebviewDirtyRect {
        x: 20,
        y: 30,
        width: 40,
        height: 50,
    }];

    assert_eq!(
        coalesced_overlay_dirty(200, 100, &previous, &latest, true).as_slice(),
        &[WebviewDirtyRect {
            x: 10,
            y: 20,
            width: 50,
            height: 60,
        }]
    );
}

#[test]
fn native_overlay_full_damage_survives_coalescing() {
    let latest = vec![WebviewDirtyRect {
        x: 20,
        y: 30,
        width: 40,
        height: 50,
    }];

    assert!(coalesced_overlay_dirty(200, 100, &[], &latest, true).is_empty());
    assert!(coalesced_overlay_dirty(200, 100, &latest, &latest, false).is_empty());
}

#[test]
fn native_overlay_metal_completion_is_asynchronous() {
    let source = include_str!("glass.rs");
    let presenter = source
        .split("fn present_native_overlay_dirty")
        .nth(1)
        .and_then(|tail| tail.split("fn primary_content_view_ptr").next())
        .unwrap_or_default();

    assert!(presenter.contains("addCompletedHandler"));
    assert!(!presenter.contains("waitUntilCompleted"));
}

fn reveal_test_app(reveal_ready: bool) -> App {
    let mut app = App::new();
    app.add_systems(Update, reveal_window_after_layout_ready);
    app.world_mut().insert_non_send(GlassState {
        installed: true,
        ..default()
    });
    app.world_mut().spawn((
        Window {
            visible: false,
            ..default()
        },
        bevy::window::PrimaryWindow,
    ));
    app.insert_resource(crate::boot_status::SplashStatus {
        phase: crate::boot_status::BootPhase::Starting,
        reveal_ready,
    });
    app
}

#[test]
fn startup_window_stays_hidden_until_reveal_ready() {
    let mut app = reveal_test_app(false);

    app.update();

    let window = app
        .world_mut()
        .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
        .single(app.world())
        .expect("primary window");
    assert!(!window.visible);
}

#[test]
fn startup_window_reveals_after_reveal_ready() {
    let mut app = reveal_test_app(true);

    app.update();

    let window = app
        .world_mut()
        .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
        .single(app.world())
        .expect("primary window");
    assert!(window.visible);
}

#[test]
fn no_activation_before_reveal() {
    assert!(!should_attempt_activation(false, false, None));
}

#[test]
fn activates_immediately_after_reveal() {
    assert!(should_attempt_activation(true, false, None));
    assert!(should_attempt_activation(true, false, Some(Duration::ZERO)));
}

#[test]
fn stops_once_confirmed() {
    assert!(!should_attempt_activation(
        true,
        true,
        Some(Duration::from_millis(10))
    ));
}

#[test]
fn retries_within_budget_then_gives_up() {
    assert!(should_attempt_activation(
        true,
        false,
        Some(ACTIVATION_RETRY_BUDGET - Duration::from_millis(1))
    ));
    assert!(!should_attempt_activation(
        true,
        false,
        Some(ACTIVATION_RETRY_BUDGET)
    ));
}

#[test]
fn reveal_does_not_activate_inline() {
    let source = include_str!("glass.rs");
    let reveal = source
        .split("fn reveal_window_after_layout_ready")
        .nth(1)
        .and_then(|tail| tail.split("fn should_attempt_activation").next())
        .unwrap_or_default();

    assert!(!reveal.contains("activate_native_window"));
    assert!(reveal.contains("state.revealed_at = Some(Instant::now())"));
}

#[test]
fn activation_retry_system_is_registered() {
    let source = include_str!("glass.rs");
    let build = source
        .split("fn build(&self, app: &mut App)")
        .nth(1)
        .and_then(|tail| tail.split("#[derive(Default)]").next())
        .unwrap_or_default();

    assert!(build.contains("ensure_window_active_after_reveal"));
}

#[test]
fn surface_transparency_system_is_registered() {
    let source = include_str!("glass.rs");
    let build = source
        .split("fn build(&self, app: &mut App)")
        .nth(1)
        .and_then(|tail| tail.split("#[derive(Default)]").next())
        .unwrap_or_default();

    assert!(build.contains("keep_window_surface_layer_transparent"));
}

#[test]
fn surface_layer_kept_non_opaque_and_clear() {
    let source = include_str!("glass.rs");
    let func = source
        .split("fn keep_window_surface_layer_transparent")
        .nth(1)
        .and_then(|tail| tail.split("fn sync_layout_overlay").next())
        .unwrap_or_default();

    assert!(func.contains("layer.setOpaque(false)"));
    assert!(func.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
}
