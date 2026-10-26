use super::Header;
use crate::Open;
use crate::event::CEF_RESERVED_HEIGHT_PX;
use crate::settings::LayoutSettings;
use crate::side_sheet::{SideSheet, SideSheetPosition};
use crate::window::VmuxWindow;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::HostWindow;
#[cfg(test)]
use vmux_feature::AppFeatureExt;
use vmux_flex::prelude::*;

use super::command::LayoutRequestSet;

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.register_feature::<crate::Feature>();
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.add_message::<ToggleLayoutRequest>()
            .add_systems(
                Update,
                (
                    handle_visibility_requests.in_set(LayoutRequestSet::Handle),
                    animate,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                sync_window_padding.before(LayoutSystems::Layout),
            );
    }
}

#[vmux_command::command(message)]
#[derive(Message)]
struct ToggleLayoutRequest;

impl TryFrom<&vmux_command::CommandInvocation> for ToggleLayoutRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "toggle_layout").then_some(Self).ok_or(())
    }
}

#[derive(Component, Default, Debug)]
pub struct LayoutHidden;

#[derive(Component)]
struct LayoutTransition {
    hide: bool,
    elapsed: f32,
    header_start: f32,
    side_sheet_start: f32,
    pad_top_start: f32,
    pad_left_start: f32,
}

impl LayoutTransition {
    const DURATION: f32 = 0.2;

    fn progress(&mut self, delta: f32) -> f32 {
        self.elapsed = (self.elapsed + delta).min(Self::DURATION);
        let linear = self.elapsed / Self::DURATION;
        1.0 - (1.0 - linear).powi(3)
    }

    fn complete(&self) -> bool {
        self.elapsed >= Self::DURATION
    }

    fn value(start: f32, end: f32, progress: f32) -> f32 {
        start + (end - start) * progress
    }
}

fn sync_window_padding(
    settings: Res<LayoutSettings>,
    hidden_windows: Query<(), With<LayoutHidden>>,
    transitions: Query<(), With<LayoutTransition>>,
    mut window_q: Query<(&HostWindow, &mut Node), With<VmuxWindow>>,
) {
    for (host, mut node) in &mut window_q {
        if transitions.contains(host.0) {
            continue;
        }
        let (top, left) = if hidden_windows.contains(host.0) {
            (settings.window.pad_top(), settings.window.pad_left())
        } else {
            (0.0, 0.0)
        };
        let want_top = Val::Px(top);
        let want_left = Val::Px(left);
        if node.padding.top != want_top || node.padding.left != want_left {
            node.padding.top = want_top;
            node.padding.left = want_left;
        }
    }
}

fn handle_visibility_requests(
    mut reader: MessageReader<ToggleLayoutRequest>,
    focused_window: crate::window::FocusedWindow,
    hidden_windows: Query<(), With<LayoutHidden>>,
    header_q: Query<(Entity, &Node), With<Header>>,
    sidesheet_q: Query<(Entity, &SideSheetPosition, &Node), With<SideSheet>>,
    roots: Query<(&HostWindow, &Node), With<VmuxWindow>>,
    hierarchy: crate::window::WindowHierarchy,
    mut commands: Commands,
) {
    for _ in reader.read() {
        let Some(window) = focused_window.entity() else {
            continue;
        };
        let hide = !hidden_windows.contains(window);
        if hide {
            commands.entity(window).insert(LayoutHidden);
        } else {
            commands.entity(window).remove::<LayoutHidden>();
        }
        let header_start = header_q
            .iter()
            .find(|(entity, _)| hierarchy.get(*entity) == Some(window))
            .and_then(|(_, node)| match node.height {
                Val::Px(height) => Some(height),
                _ => None,
            })
            .unwrap_or(if hide { CEF_RESERVED_HEIGHT_PX } else { 0.0 });
        let side_sheet_start = sidesheet_q
            .iter()
            .find(|(entity, position, _)| {
                **position == SideSheetPosition::Left && hierarchy.get(*entity) == Some(window)
            })
            .and_then(|(_, _, node)| match node.width {
                Val::Px(width) => Some(width),
                _ => None,
            })
            .unwrap_or_default();
        let (pad_top_start, pad_left_start) = roots
            .iter()
            .find(|(host, _)| host.0 == window)
            .map(|(_, node)| {
                let top = match node.padding.top {
                    Val::Px(value) => value,
                    _ => 0.0,
                };
                let left = match node.padding.left {
                    Val::Px(value) => value,
                    _ => 0.0,
                };
                (top, left)
            })
            .unwrap_or_default();
        commands.entity(window).insert(LayoutTransition {
            hide,
            elapsed: 0.0,
            header_start,
            side_sheet_start,
            pad_top_start,
            pad_left_start,
        });
        if !hide {
            for (entity, _) in &header_q {
                if hierarchy.get(entity) == Some(window) {
                    commands.entity(entity).insert(Open);
                }
            }
            for (entity, position, _) in &sidesheet_q {
                if *position == SideSheetPosition::Left && hierarchy.get(entity) == Some(window) {
                    commands.entity(entity).insert(Open);
                }
            }
        }
    }
}

fn animate(
    time: Res<Time>,
    settings: Res<LayoutSettings>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut transitions: Query<(Entity, &mut LayoutTransition), With<Window>>,
    mut layout: ParamSet<(
        Query<(Entity, &mut Node), With<Header>>,
        Query<(Entity, &SideSheetPosition, &mut Node), With<SideSheet>>,
        Query<(&HostWindow, &mut Node), With<VmuxWindow>>,
    )>,
    hierarchy: crate::window::WindowHierarchy,
    mut commands: Commands,
) {
    for (window, mut transition) in &mut transitions {
        let progress = transition.progress(time.delta_secs());
        let header_end = if transition.hide {
            0.0
        } else {
            CEF_RESERVED_HEIGHT_PX
        };
        let side_sheet_end = if transition.hide {
            0.0
        } else {
            crate::event::SideSheetResizeEvent::live(settings.side_sheet.width).clamped()
        };
        let pad_top_end = if transition.hide {
            settings.window.pad_top()
        } else {
            0.0
        };
        let pad_left_end = if transition.hide {
            settings.window.pad_left()
        } else {
            0.0
        };
        {
            let mut headers = layout.p0();
            for (entity, mut node) in &mut headers {
                if hierarchy.get(entity) == Some(window) {
                    node.height = Val::Px(LayoutTransition::value(
                        transition.header_start,
                        header_end,
                        progress,
                    ));
                }
            }
        }
        {
            let mut side_sheets = layout.p1();
            for (entity, position, mut node) in &mut side_sheets {
                if *position == SideSheetPosition::Left && hierarchy.get(entity) == Some(window) {
                    node.width = Val::Px(LayoutTransition::value(
                        transition.side_sheet_start,
                        side_sheet_end,
                        progress,
                    ));
                }
            }
        }
        {
            let mut roots = layout.p2();
            for (host, mut node) in &mut roots {
                if host.0 == window {
                    node.padding.top = Val::Px(LayoutTransition::value(
                        transition.pad_top_start,
                        pad_top_end,
                        progress,
                    ));
                    node.padding.left = Val::Px(LayoutTransition::value(
                        transition.pad_left_start,
                        pad_left_end,
                        progress,
                    ));
                }
            }
        }
        if transition.complete() {
            if transition.hide {
                {
                    let mut headers = layout.p0();
                    for (entity, _) in &mut headers {
                        if hierarchy.get(entity) == Some(window) {
                            commands.entity(entity).remove::<Open>();
                        }
                    }
                }
                {
                    let mut side_sheets = layout.p1();
                    for (entity, position, _) in &mut side_sheets {
                        if *position == SideSheetPosition::Left
                            && hierarchy.get(entity) == Some(window)
                        {
                            commands.entity(entity).remove::<Open>();
                        }
                    }
                }
            }
            commands.entity(window).remove::<LayoutTransition>();
        } else if let Some(proxy) = proxy.as_deref() {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        settings::{
            FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
        },
        window::VmuxWindow,
    };
    use bevy::window::{Monitor, MonitorSelection, PrimaryWindow, WindowMode};

    #[test]
    fn visible_fullscreen_layout_clears_top_left_padding() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((VmuxWindow, HostWindow(window), Node::default()))
            .id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(0.0));
        assert_eq!(node.padding.left, Val::Px(0.0));
    }

    #[test]
    fn visible_maximized_layout_clears_top_left_padding() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (1200, 800).into(),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        app.world_mut().spawn(Monitor {
            name: None,
            physical_width: 1200,
            physical_height: 800,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: None,
            scale_factor: 1.0,
            video_modes: Vec::new(),
        });
        let root = app
            .world_mut()
            .spawn((VmuxWindow, HostWindow(window), Node::default()))
            .id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(0.0));
        assert_eq!(node.padding.left, Val::Px(0.0));
    }

    #[test]
    fn hidden_layout_uses_top_left_padding() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (1200, 800).into(),
                    ..default()
                },
                PrimaryWindow,
                LayoutHidden,
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((VmuxWindow, HostWindow(window), Node::default()))
            .id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(16.0));
        assert_eq!(node.padding.left, Val::Px(16.0));
    }

    #[test]
    fn toggle_changes_only_the_focused_window() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ToggleLayoutRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, (handle_visibility_requests, animate).chain());
        let first_window = app.world_mut().spawn(Window::default()).id();
        let second_window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .entity_mut(first_window)
            .insert(vmux_ecs::Active);
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_header = app
            .world_mut()
            .spawn((Header, Open, Node::default(), ChildOf(first_root)))
            .id();
        let first_sheet = app
            .world_mut()
            .spawn((
                SideSheet,
                SideSheetPosition::Left,
                Open,
                Node::default(),
                ChildOf(first_root),
            ))
            .id();
        let second_header = app
            .world_mut()
            .spawn((Header, Open, Node::default(), ChildOf(second_root)))
            .id();
        let second_sheet = app
            .world_mut()
            .spawn((
                SideSheet,
                SideSheetPosition::Left,
                Open,
                Node::default(),
                ChildOf(second_root),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<ToggleLayoutRequest>>()
            .write(ToggleLayoutRequest);

        app.update();

        assert!(app.world().entity(first_window).contains::<LayoutHidden>());
        assert!(
            app.world()
                .entity(first_window)
                .contains::<LayoutTransition>()
        );
        assert!(!app.world().entity(second_window).contains::<LayoutHidden>());
        assert!(app.world().entity(first_header).contains::<Open>());
        assert!(app.world().entity(first_sheet).contains::<Open>());
        assert!(app.world().entity(second_header).contains::<Open>());
        assert!(app.world().entity(second_sheet).contains::<Open>());
    }
}
