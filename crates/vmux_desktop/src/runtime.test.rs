use super::*;

#[test]
fn native_window_resize_detects_edges_and_corners() {
    let frame = NativeWindowFrame {
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 600.0,
    };

    assert_eq!(
        native_resize_edges(frame, 100.0, 100.0, 8.0),
        NativeResizeEdges {
            left: true,
            bottom: true,
            ..Default::default()
        }
    );
    assert_eq!(
        native_resize_edges(frame, 900.0, 700.0, 8.0),
        NativeResizeEdges {
            right: true,
            top: true,
            ..Default::default()
        }
    );
    assert_eq!(
        native_resize_edges(frame, 500.0, 100.0, 8.0),
        NativeResizeEdges {
            bottom: true,
            ..Default::default()
        }
    );
    assert!(!native_resize_edges(frame, 500.0, 400.0, 8.0).any());
}

#[test]
fn native_corner_resize_updates_both_axes_and_clamps_minimum() {
    let drag = NativeWindowResizeDrag {
        frame: NativeWindowFrame {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        },
        cursor_x: 100.0,
        cursor_y: 100.0,
        min_width: 200.0,
        min_height: 120.0,
        edges: NativeResizeEdges {
            left: true,
            bottom: true,
            ..Default::default()
        },
    };

    assert_eq!(
        resized_native_window_frame(drag, 150.0, 150.0),
        NativeWindowFrame {
            x: 150.0,
            y: 150.0,
            width: 750.0,
            height: 550.0,
        }
    );
    assert_eq!(
        resized_native_window_frame(drag, 850.0, 650.0),
        NativeWindowFrame {
            x: 700.0,
            y: 580.0,
            width: 200.0,
            height: 120.0,
        }
    );
}

#[test]
fn user_mode_skips_bevy_render_when_native_page_is_visible() {
    assert!(!render_frame_should_run(
        InteractionMode::User,
        false,
        false,
        true,
    ));
    assert!(render_frame_should_run(
        InteractionMode::Player,
        false,
        false,
        true,
    ));
    assert!(render_frame_should_run(
        InteractionMode::User,
        true,
        false,
        true,
    ));
    assert!(render_frame_should_run(
        InteractionMode::User,
        false,
        true,
        true,
    ));
    assert!(render_frame_should_run(
        InteractionMode::User,
        false,
        false,
        false,
    ));
}

#[test]
fn render_schedule_runs_only_when_demanded() {
    #[derive(Resource, Default)]
    struct RenderRuns(usize);

    fn count_render(mut runs: ResMut<RenderRuns>) {
        runs.0 += 1;
    }

    let mut world = World::new();
    world.insert_resource(RenderFrameDemand(false));
    world.init_resource::<RenderRuns>();

    let mut render = Schedule::new(Render);
    render.add_systems(count_render);
    world.add_schedule(render);

    let mut demanded = Schedule::new(DemandedRender);
    demanded.add_systems(run_demanded_render);
    world.add_schedule(demanded);

    world.run_schedule(DemandedRender);
    assert_eq!(world.resource::<RenderRuns>().0, 0);

    world.resource_mut::<RenderFrameDemand>().0 = true;
    world.run_schedule(DemandedRender);
    assert_eq!(world.resource::<RenderRuns>().0, 1);
}

#[test]
fn player_frame_demand_only_runs_for_player_or_transition() {
    assert!(!player_frame_should_wake(
        InteractionMode::User,
        false,
        true
    ));
    assert!(player_frame_should_wake(
        InteractionMode::Player,
        false,
        true
    ));
    assert!(player_frame_should_wake(InteractionMode::User, true, true));
    assert!(player_frame_should_wake(
        InteractionMode::Player,
        true,
        true
    ));
    assert!(!player_frame_should_wake(
        InteractionMode::Player,
        false,
        false
    ));
    assert!(!player_frame_should_wake(
        InteractionMode::User,
        true,
        false
    ));
}

#[test]
fn player_frame_demand_runs_in_last() {
    let source = include_str!("runtime.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default();
    let plugin_build = source
        .split("impl Plugin for RuntimePlugin")
        .nth(1)
        .and_then(|tail| tail.split("#[cfg(target_os = \"macos\")]").next())
        .unwrap_or_default();

    assert!(plugin_build.contains(".add_systems(Last, keep_awake_while_player_active)"));
}

#[test]
fn command_bar_wake_covers_defer_and_active_reveal() {
    assert!(command_bar_should_wake(true, false));
    assert!(command_bar_should_wake(false, true));
    assert!(command_bar_should_wake(true, true));
    assert!(!command_bar_should_wake(false, false));
}

#[test]
fn handle_lifecycle_events_uses_world_for_confirm_dialog() {
    let source = include_str!("runtime.rs");
    let exclusive_marker = ["world", ": ", "&mut", " World"].concat();
    assert!(
        source.contains(&exclusive_marker),
        "handle_lifecycle_events must be an exclusive &mut World system to call confirm_quit_dialog"
    );
    let confirm_call = ["confirm", "_quit_dialog"].concat();
    assert!(
        source.contains(&confirm_call),
        "QuitVmux arm must call terminal::confirm_quit_dialog"
    );
}

#[test]
fn no_continuous_update_mode_anywhere_in_workspace() {
    use std::path::Path;
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let banned = ["UpdateMode", "::", "Continuous"].concat();
    let mut offenders = Vec::new();
    for root in ["crates", "patches"] {
        let dir = workspace_root.join(root);
        if !dir.exists() {
            continue;
        }
        walk_rs_files(&dir, &mut |path, source| {
            // This file spells the banned pattern out in its own failure message, and runtime.rs
            // is where the sanctioned Reactive setup lives.
            if path.ends_with("runtime.rs") || path.ends_with("runtime.test.rs") {
                return;
            }
            for (lineno, line) in source.lines().enumerate() {
                let stripped = line.trim_start();
                if stripped.starts_with("//") || stripped.starts_with("///") {
                    continue;
                }
                if line.contains(&banned) {
                    offenders.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        lineno + 1,
                        line.trim()
                    ));
                }
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "Bevy `UpdateMode::Continuous` is banned in vmux (causes 100-200% idle CPU). Use `UpdateMode::Reactive` and route missing wake sources via `EventLoopProxy::send_event(WinitUserEvent::WakeUp)`. See AGENTS.md. Offenders:\n{}",
        offenders.join("\n")
    );
}

fn walk_rs_files(dir: &std::path::Path, visit: &mut dyn FnMut(&std::path::Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            walk_rs_files(&path, visit);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
            && let Ok(source) = std::fs::read_to_string(&path)
        {
            visit(&path, &source);
        }
    }
}

#[test]
fn foreground_power_mode_is_reactive_when_focused() {
    let settings = foreground_winit_settings(false, false, false);

    let UpdateMode::Reactive {
        wait,
        react_to_device_events,
        react_to_user_events,
        react_to_window_events,
    } = settings.focused_mode
    else {
        panic!(
            "focused mode must be Reactive, got {:?}",
            settings.focused_mode
        );
    };
    assert_eq!(wait, Duration::from_secs(1));
    assert!(react_to_user_events);
    assert_eq!(
        settings.unfocused_mode,
        UpdateMode::reactive_low_power(Duration::from_secs(1))
    );
    // At rest, window-event wakes are on so the layout mesh + camera respond to window events;
    // device-event wakes stay off in browse mode.
    assert!(!react_to_device_events);
    assert!(react_to_window_events);

    // During a live resize, the loop is paced by a ~16ms timer (window-event reaction off) to cap
    // the render rate to ~60Hz instead of the 120Hz display refresh.
    let UpdateMode::Reactive {
        wait: resize_wait,
        react_to_window_events: resize_window,
        ..
    } = foreground_winit_settings(false, true, false).focused_mode
    else {
        panic!("focused mode must be Reactive");
    };
    assert_eq!(resize_wait, Duration::from_millis(16));
    assert!(!resize_window);

    let player = foreground_winit_settings(true, false, false);
    let UpdateMode::Reactive {
        react_to_device_events: player_device,
        react_to_window_events: player_window,
        ..
    } = player.focused_mode
    else {
        panic!("focused mode must be Reactive");
    };
    assert!(player_device);
    assert!(player_window);

    let layout_hover = foreground_winit_settings(false, false, true);
    let UpdateMode::Reactive {
        react_to_window_events: layout_window,
        ..
    } = layout_hover.focused_mode
    else {
        panic!("focused mode must be Reactive");
    };
    assert!(!layout_window);
}

#[test]
fn scroll_preserves_windowed_page_pointer_ownership() {
    assert!(windowed_pointer_inside_after_event(false, true, false));
    assert!(!windowed_pointer_inside_after_event(false, false, true));
    assert!(!windowed_pointer_inside_after_event(true, true, false));
    assert!(windowed_pointer_inside_after_event(true, false, true));
}

#[test]
fn native_scroll_wakes_bevy_only_for_layout_or_non_windowed_content() {
    assert!(!native_scroll_should_wake(false, true));
    assert!(native_scroll_should_wake(true, true));
    assert!(native_scroll_should_wake(false, false));
}

#[test]
fn native_mouse_motion_publishes_latest_sample_before_waking() {
    let source = include_str!("runtime.rs");
    let monitor = source
        .split("fn install_native_mouse_wake_monitor")
        .nth(1)
        .and_then(|tail| tail.split("fn foreground_winit_settings").next())
        .unwrap_or_default();

    assert!(monitor.contains("NSEventMask::MouseMoved"));
    assert!(monitor.contains("NSEventMask::LeftMouseDown"));
    assert!(monitor.contains("WinitUserEvent::WakeUp"));
    assert!(monitor.contains("vmux_layout::native_pointer::publish"));
    assert!(monitor.contains("vmux_browser::queue_native_layout_pointer_move"));
    assert!(monitor.contains("flush_layout(interval)"));
    assert!(monitor.contains("if result.region_changed"));
    assert!(monitor.contains("vmux_browser::flush_native_layout_pointer_move()"));
    assert!(!monitor.contains("forward_native_layout_pointer_move"));
    assert!(!monitor.contains("vmux_layout::pane::wake_on_move"));
    assert!(monitor.contains("let global_mask = NSEventMask::LeftMouseDown"));
}

#[test]
fn native_mouse_wake_throttle_has_a_trailing_wake() {
    let source = include_str!("runtime.rs");
    let throttle = source
        .split("fn native_throttle")
        .nth(1)
        .and_then(|tail| tail.split("fn install_native_mouse_wake_monitor").next())
        .unwrap_or_default();

    assert!(throttle.contains("sync_channel::<()>(1)"));
    assert!(throttle.contains("recv_timeout"));
    assert!(throttle.contains("pending_interval_ns.fetch_min"));
    assert!(!throttle.contains("while wake_rx.try_recv().is_ok()"));
    assert!(!throttle.contains("thread_pending_interval_ns.store"));
    assert!(!throttle.contains("LAST_NATIVE_MOUSE_WAKE.lock()"));
}

#[test]
fn native_mouse_monitor_tracks_left_button_state() {
    let source = include_str!("runtime.rs");
    let monitor = source
        .split("fn install_native_mouse_wake_monitor")
        .nth(1)
        .and_then(|tail| tail.split("fn foreground_winit_settings").next())
        .unwrap_or_default();

    assert!(monitor.contains("vmux_browser::set_native_left_mouse_down(true)"));
    assert!(monitor.contains("vmux_browser::set_native_left_mouse_down(false)"));
}

#[test]
fn startup_activates_primary_window_on_macos() {
    let source = include_str!("runtime.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default();
    let plugin_build = source
        .split("impl Plugin for RuntimePlugin")
        .nth(1)
        .and_then(|tail| tail.split("#[cfg(target_os = \"macos\")]").next())
        .unwrap_or_default();

    assert!(plugin_build.contains("install_native_mouse_wake_monitor"));
    assert!(plugin_build.contains("activate_primary_window_on_startup"));
    assert!(source.contains("activateIgnoringOtherApps"));
    assert!(source.contains("makeKeyAndOrderFront"));
}

#[test]
fn native_mouse_monitor_does_not_wait_for_window_creation() {
    let source = include_str!("runtime.rs");
    let monitor = source
        .split("fn install_native_mouse_wake_monitor")
        .nth(1)
        .and_then(|tail| tail.split("fn foreground_winit_settings").next())
        .unwrap_or_default();

    assert!(monitor.contains("proxy: Option<Res<EventLoopProxyWrapper>>"));
    assert!(!monitor.contains("PrimaryWindow"));
    assert!(!monitor.contains("appkit_window_ptr"));
}

#[test]
fn startup_activation_waits_for_visible_window() {
    let source = include_str!("runtime.rs")
        .split("fn activate_primary_window_on_startup")
        .nth(1)
        .and_then(|tail| tail.split("#[cfg(not(target_os = \"macos\"))]").next())
        .unwrap_or_default();

    assert!(source.contains("if !window.visible"));
}

#[test]
fn app_activation_starts_during_boot() {
    let source = include_str!("runtime.rs");
    let plugin_build = source
        .split("impl Plugin for RuntimePlugin")
        .nth(1)
        .and_then(|tail| tail.split("#[cfg(target_os = \"macos\")]").next())
        .unwrap_or_default();
    assert!(plugin_build.contains("activate_app_during_boot"));

    let boot = source
        .split("fn activate_app_during_boot")
        .nth(1)
        .and_then(|tail| tail.split("#[cfg(not(target_os = \"macos\"))]").next())
        .unwrap_or_default();
    assert!(boot.contains("APP_ACTIVATION_BUDGET"));
    assert!(boot.contains("WinitUserEvent::WakeUp"));
}

#[test]
fn native_mouse_down_requests_command_bar_dismiss() {
    let source = include_str!("runtime.rs");
    let monitor = source
        .split("fn install_native_mouse_wake_monitor")
        .nth(1)
        .and_then(|tail| tail.split("fn foreground_winit_settings").next())
        .unwrap_or_default();

    assert!(monitor.contains("event_location_in_window_physical_px"));
    assert!(monitor.contains("request_native_command_bar_dismiss_for_mouse_down"));
}

#[test]
fn hidden_power_mode_ignores_stale_window_focus() {
    let settings = hidden_winit_settings();

    assert_eq!(
        settings.focused_mode,
        UpdateMode::reactive_low_power(Duration::from_secs(60))
    );
    assert_eq!(
        settings.unfocused_mode,
        UpdateMode::reactive_low_power(Duration::from_secs(60))
    );
}

#[test]
fn cef_wake_policy_matches_display_refresh() {
    assert_eq!(
        foreground_cef_wake_interval([Some(60_000)]),
        Duration::from_nanos(16_666_666)
    );
    assert!(foreground_cef_wake_interval([Some(144_000)]) < Duration::from_millis(8));
    assert_eq!(
        cef_wake_interval(false, true, true, Duration::from_millis(7)),
        Duration::from_millis(7)
    );
}

#[test]
fn cef_wake_policy_throttles_visible_unfocused() {
    assert_eq!(
        cef_wake_interval(false, true, false, Duration::from_millis(7)),
        Duration::from_secs(1)
    );
}

#[test]
fn cef_wake_policy_throttles_hidden() {
    assert_eq!(
        cef_wake_interval(false, false, true, Duration::from_millis(7)),
        Duration::from_secs(1)
    );
    assert_eq!(
        cef_wake_interval(true, true, true, Duration::from_millis(7)),
        Duration::from_secs(1)
    );
}

#[test]
fn hide_lifecycle_suspends_osr_webviews() {
    let source = include_str!("runtime.rs");

    assert!(source.contains("hide_all_osr_webviews(world)"));
    assert!(source.contains("set_all_osr_hidden"));
}
