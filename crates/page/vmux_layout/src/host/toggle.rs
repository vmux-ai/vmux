use crate::Open;
use crate::host::header::Header;
use crate::settings::LayoutSettings;
use crate::side_sheet::SideSheet;
use crate::window::VmuxWindow;
use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_flex::prelude::*;

use super::command::LayoutRequestSet;

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        ToggleLayoutRequest::register(app);
        app.init_resource::<LayoutHidden>()
            .add_systems(
                Update,
                handle_visibility_requests.in_set(LayoutRequestSet::Handle),
            )
            .add_systems(
                PostUpdate,
                sync_window_padding_to_layout_hidden.before(LayoutSystems::Layout),
            );
    }
}

#[derive(vmux_macro::CommandBar)]
#[shortcut(direct = "Super+Shift+S")]
struct ToggleLayoutRequest;

#[derive(Resource, Default, Debug)]
pub struct LayoutHidden(std::collections::HashSet<Entity>);

impl LayoutHidden {
    pub fn is_hidden(&self, window: Entity) -> bool {
        self.0.contains(&window)
    }

    fn toggle(&mut self, window: Entity) -> bool {
        if self.0.remove(&window) {
            false
        } else {
            self.0.insert(window);
            true
        }
    }
}

fn sync_window_padding_to_layout_hidden(
    hidden: Res<LayoutHidden>,
    settings: Res<LayoutSettings>,
    mut window_q: Query<(&HostWindow, &mut Node), With<VmuxWindow>>,
) {
    for (host, mut node) in &mut window_q {
        let (top, left) = if hidden.is_hidden(host.0) {
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
    mut hidden: ResMut<LayoutHidden>,
    focused_window: Res<crate::window::FocusedWindow>,
    header_q: Query<Entity, With<Header>>,
    sidesheet_q: Query<Entity, With<SideSheet>>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    mut commands: Commands,
) {
    for _ in reader.read() {
        let Some(window) = focused_window.0 else {
            continue;
        };
        let is_hidden = hidden.toggle(window);

        if is_hidden {
            for entity in header_q.iter().chain(sidesheet_q.iter()).filter(|entity| {
                crate::window::host_window_of(*entity, &child_of, &host_windows) == Some(window)
            }) {
                commands.entity(entity).remove::<Open>();
            }
        } else {
            for entity in header_q.iter().chain(sidesheet_q.iter()).filter(|entity| {
                crate::window::host_window_of(*entity, &child_of, &host_windows) == Some(window)
            }) {
                commands.entity(entity).insert(Open);
            }
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
            .init_resource::<LayoutHidden>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding_to_layout_hidden);
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
            .init_resource::<LayoutHidden>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding_to_layout_hidden);
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
            .init_resource::<LayoutHidden>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 16.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            })
            .add_systems(Update, sync_window_padding_to_layout_hidden);
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
        app.world_mut()
            .resource_mut::<LayoutHidden>()
            .toggle(window);
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
            .init_resource::<LayoutHidden>()
            .add_systems(Update, handle_visibility_requests);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.insert_resource(crate::window::FocusedWindow(Some(first_window)));
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_header = app
            .world_mut()
            .spawn((Header, Open, ChildOf(first_root)))
            .id();
        let first_sheet = app
            .world_mut()
            .spawn((SideSheet, Open, ChildOf(first_root)))
            .id();
        let second_header = app
            .world_mut()
            .spawn((Header, Open, ChildOf(second_root)))
            .id();
        let second_sheet = app
            .world_mut()
            .spawn((SideSheet, Open, ChildOf(second_root)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<ToggleLayoutRequest>>()
            .write(ToggleLayoutRequest);

        app.update();

        assert!(
            app.world()
                .resource::<LayoutHidden>()
                .is_hidden(first_window)
        );
        assert!(
            !app.world()
                .resource::<LayoutHidden>()
                .is_hidden(second_window)
        );
        assert!(!app.world().entity(first_header).contains::<Open>());
        assert!(!app.world().entity(first_sheet).contains::<Open>());
        assert!(app.world().entity(second_header).contains::<Open>());
        assert!(app.world().entity(second_sheet).contains::<Open>());
    }
}
