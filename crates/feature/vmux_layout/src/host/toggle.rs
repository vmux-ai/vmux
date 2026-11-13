use super::Header;
use super::deadline::{DeadlineOwner, DeadlineScheduler};
use crate::Open;
use crate::event::CEF_RESERVED_HEIGHT_PX;
use crate::settings::LayoutSettings;
use crate::side_sheet::{SideSheet, SideSheetPosition};
use crate::window::VmuxWindow;
use bevy::prelude::*;
use bevy::prelude::{Commands, Window};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::HostWindow;
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;
use vmux_flex::prelude::*;

use super::command::LayoutRequestSet;

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.init_resource::<DeadlineScheduler>()
            .add_message::<ToggleLayoutRequest>()
            .add_message::<ToggleUrlBarRequest>()
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

#[vmux_command::command(message)]
#[derive(Message)]
struct ToggleUrlBarRequest;

impl TryFrom<&vmux_command::CommandInvocation> for ToggleUrlBarRequest {
    type Error = ();

    fn try_from(invocation: &vmux_command::CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "toggle_url_bar")
            .then_some(Self)
            .ok_or(())
    }
}

#[derive(Component, Default, Debug)]
pub struct UrlBarHidden;

#[derive(Component, Default, Debug)]
pub struct LayoutHidden;

#[derive(Component)]
pub(super) struct LayoutTransition {
    hide: bool,
    elapsed: f32,
    started: std::time::Duration,
    header_start: f32,
    side_sheet_start: f32,
    pad_top_start: f32,
    pad_left_start: f32,
    gap_start: f32,
    header_end: f32,
    side_sheet_end: f32,
    pad_top_end: f32,
    pad_left_end: f32,
    gap_end: f32,
}

impl LayoutTransition {
    const DURATION: f32 = vmux_ui::motion::Motion::LAYOUT_MS as f32 / 1000.0;

    fn progress(&mut self, now: std::time::Duration) -> f32 {
        self.elapsed = now
            .saturating_sub(self.started)
            .as_secs_f32()
            .min(Self::DURATION);
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
    mut url_reader: MessageReader<ToggleUrlBarRequest>,
    url_hidden: Query<(), With<UrlBarHidden>>,
    existing: Query<&LayoutTransition>,
    settings: Res<LayoutSettings>,
    time: Res<Time>,
    focused_window: crate::window::FocusedWindow,
    hidden_windows: Query<(), With<LayoutHidden>>,
    header_q: Query<(Entity, &Node), With<Header>>,
    sidesheet_q: Query<(Entity, &SideSheetPosition, &Node), With<SideSheet>>,
    roots: Query<(&HostWindow, &Node), With<VmuxWindow>>,
    hierarchy: crate::window::WindowHierarchy,
    mut commands: Commands,
) {
    for url_only in reader
        .read()
        .map(|_| false)
        .chain(url_reader.read().map(|_| true))
    {
        let Some(window) = focused_window.entity() else {
            continue;
        };
        let hide = if url_only {
            hidden_windows.contains(window)
        } else {
            !hidden_windows.contains(window)
        };
        let hide_url = if url_only {
            !url_hidden.contains(window)
        } else {
            hide
        };
        if !url_only {
            if hide {
                commands.entity(window).insert(LayoutHidden);
            } else {
                commands.entity(window).remove::<LayoutHidden>();
            }
        }
        if hide_url {
            commands.entity(window).insert(UrlBarHidden);
        } else {
            commands.entity(window).remove::<UrlBarHidden>();
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
        let (pad_top_start, pad_left_start, gap_start) = roots
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
                let gap = match node.column_gap {
                    Val::Px(value) => value,
                    _ => 0.0,
                };
                (top, left, gap)
            })
            .unwrap_or_default();
        let previous = if url_only {
            existing.get(window).ok()
        } else {
            None
        };
        commands.entity(window).insert(LayoutTransition {
            hide: if url_only {
                previous.is_some_and(|transition| transition.hide)
            } else {
                hide
            },
            elapsed: 0.0,
            started: time.elapsed(),
            header_start,
            side_sheet_start,
            pad_top_start,
            pad_left_start,
            gap_start,
            header_end: if hide_url {
                36.0
            } else {
                CEF_RESERVED_HEIGHT_PX
            },
            side_sheet_end: if url_only {
                previous
                    .map(|transition| transition.side_sheet_end)
                    .unwrap_or(side_sheet_start)
            } else if hide {
                0.0
            } else {
                crate::event::SideSheetResizeEvent::live(settings.side_sheet.width).clamped()
            },
            pad_top_end: if url_only {
                previous
                    .map(|transition| transition.pad_top_end)
                    .unwrap_or(pad_top_start)
            } else if hide {
                settings.window.pad_top()
            } else {
                0.0
            },
            pad_left_end: if url_only {
                previous
                    .map(|transition| transition.pad_left_end)
                    .unwrap_or(pad_left_start)
            } else if hide {
                settings.window.pad_left()
            } else {
                0.0
            },
            gap_end: if url_only {
                previous
                    .map(|transition| transition.gap_end)
                    .unwrap_or(gap_start)
            } else if hide {
                0.0
            } else {
                crate::event::SIDE_SHEET_GAP_PX
            },
        });
        for (entity, _) in &header_q {
            if hierarchy.get(entity) == Some(window) {
                commands.entity(entity).insert(Open);
            }
        }
        if !url_only && !hide {
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
    scheduler: Res<DeadlineScheduler>,
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
        let progress = transition.progress(time.elapsed());
        let header_end = transition.header_end;
        let side_sheet_end = transition.side_sheet_end;
        let pad_top_end = transition.pad_top_end;
        let pad_left_end = transition.pad_left_end;
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
                    node.column_gap = Val::Px(LayoutTransition::value(
                        transition.gap_start,
                        transition.gap_end,
                        progress,
                    ));
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
            scheduler.cancel(DeadlineOwner::LayoutAnimation(window));
        } else {
            scheduler.schedule(
                DeadlineOwner::LayoutAnimation(window),
                std::time::Instant::now() + std::time::Duration::from_millis(16),
                proxy.as_deref(),
            );
        }
    }
}

#[cfg(test)]
impl LayoutTransition {
    pub(super) fn finished() -> Self {
        Self {
            hide: false,
            elapsed: Self::DURATION,
            started: std::time::Duration::ZERO,
            header_start: 0.0,
            side_sheet_start: 0.0,
            pad_top_start: 0.0,
            pad_left_start: 0.0,
            gap_start: 0.0,
            header_end: CEF_RESERVED_HEIGHT_PX,
            side_sheet_end: 0.0,
            pad_top_end: 0.0,
            pad_left_end: 0.0,
            gap_end: 0.0,
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
    fn layout_animation_starts_at_input_time_after_an_idle_gap() {
        let started = std::time::Duration::from_secs(10);
        let mut transition = LayoutTransition {
            hide: true,
            elapsed: 0.0,
            started,
            header_start: 80.0,
            side_sheet_start: 240.0,
            pad_top_start: 0.0,
            pad_left_start: 0.0,
            gap_start: 8.0,
            header_end: 36.0,
            side_sheet_end: 0.0,
            pad_top_end: 0.0,
            pad_left_end: 0.0,
            gap_end: 0.0,
        };
        let first = transition.progress(started);
        assert_eq!(LayoutTransition::value(80.0, 0.0, first), 80.0);
        let middle = transition.progress(started + std::time::Duration::from_millis(60));
        assert_eq!(LayoutTransition::value(80.0, 0.0, middle), 10.0);
        assert!(!transition.complete());
        transition.progress(started + std::time::Duration::from_millis(120));
        assert!(transition.complete());
    }

    #[test]
    fn visible_fullscreen_layout_clears_top_left_padding() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings::default(),
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
                pane: PaneSettings::default(),
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
                pane: PaneSettings::default(),
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
        app.add_plugins((MinimalPlugins, TogglePlugin))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(60),
            ))
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings::default(),
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            });
        let first_window = app.world_mut().spawn(Window::default()).id();
        let second_window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .entity_mut(first_window)
            .insert(vmux_ecs::Active);
        let first_root = app
            .world_mut()
            .spawn((
                HostWindow(first_window),
                VmuxWindow,
                Node {
                    column_gap: Val::Px(8.0),
                    ..default()
                },
            ))
            .id();
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
        app.update();
        app.update();
        assert!(app.world().entity(first_header).contains::<Open>());
        assert!(!app.world().entity(first_sheet).contains::<Open>());
        assert_eq!(
            app.world().get::<Node>(first_header).unwrap().height,
            Val::Px(36.0)
        );
        assert_eq!(
            app.world().get::<Node>(first_root).unwrap().column_gap,
            Val::Px(0.0)
        );
        let padding = app.world().get::<Node>(first_root).unwrap().padding;
        app.world_mut()
            .resource_mut::<Messages<ToggleUrlBarRequest>>()
            .write(ToggleUrlBarRequest);
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().entity(first_window).contains::<LayoutHidden>());
        assert!(!app.world().entity(first_window).contains::<UrlBarHidden>());
        assert!(!app.world().entity(first_sheet).contains::<Open>());
        assert_eq!(
            app.world().get::<Node>(first_header).unwrap().height,
            Val::Px(CEF_RESERVED_HEIGHT_PX)
        );
        assert_eq!(
            app.world().get::<Node>(first_root).unwrap().padding,
            padding
        );
        app.world_mut().entity_mut(first_sheet).insert(Open);
        app.world_mut().get_mut::<Node>(first_sheet).unwrap().width = Val::Px(120.0);
        app.world_mut()
            .resource_mut::<Messages<ToggleUrlBarRequest>>()
            .write(ToggleUrlBarRequest);
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().entity(first_sheet).contains::<Open>());
        assert_eq!(
            app.world().get::<Node>(first_sheet).unwrap().width,
            Val::Px(120.0)
        );
    }
}
