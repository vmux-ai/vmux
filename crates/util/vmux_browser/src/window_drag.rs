use bevy::ecs::system::Commands;
use bevy::prelude::*;
use bevy::window::Window;
use bevy_cef::prelude::{HostWindow, UiInput};
use std::sync::{LazyLock, Mutex};
use vmux_ecs::overlay::{OverlayShownInline, OverlayState, WindowOverlay};
use vmux_ecs::page::HostsPage;
use vmux_flex::prelude::{ComputedNode, Display, LayoutSystems, Node, Visibility};
use vmux_layout::event::WindowDragRegionEvent;
use vmux_layout::{
    LayoutCef,
    window::{VmuxWindow, WindowHierarchy},
};

use crate::host::LayoutPointerCapture;

impl Plugin for WindowDragPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, initialize_regions)
            .add_observer(region)
            .add_systems(PostUpdate, publish_region.after(LayoutSystems::Layout));
    }
}

pub(crate) struct WindowDragPlugin;

static REGIONS: LazyLock<Mutex<PublishedWindowDragRegions>> =
    LazyLock::new(|| Mutex::new(PublishedWindowDragRegions::default()));

#[derive(Default)]
struct PublishedWindowDragRegions {
    allowed: Vec<(Entity, WindowDragRegion)>,
    blocked: Vec<(Entity, WindowDragRegion)>,
}

impl PublishedWindowDragRegions {
    fn contains(&self, window: Entity, x_px: f32, y_px: f32) -> bool {
        self.allowed
            .iter()
            .any(|(owner, region)| *owner == window && region.contains(x_px, y_px))
            && !self
                .blocked
                .iter()
                .any(|(owner, region)| *owner == window && region.contains(x_px, y_px))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowDragRegion {
    left_px: f32,
    top_px: f32,
    right_px: f32,
    bottom_px: f32,
}

impl WindowDragRegion {
    pub fn contains_point(window: Entity, x_px: f32, y_px: f32) -> bool {
        let published = REGIONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        published.contains(window, x_px, y_px)
    }

    fn from_report(reported: WindowDragRegionEvent, header: ComputedNode) -> Option<Self> {
        if header.is_empty() || !reported.is_finite() {
            return None;
        }
        let scale = header.scale();
        let left_px = reported.left * scale;
        let top_px = reported.top * scale;
        let region = Self {
            left_px: left_px.max(header.min().x),
            top_px: top_px.max(header.min().y),
            right_px: (left_px + reported.width * scale).min(header.max().x),
            bottom_px: (top_px + reported.height * scale).min(header.max().y),
        };
        if region.right_px <= region.left_px || region.bottom_px <= region.top_px {
            return None;
        }
        Some(region)
    }

    fn contains(self, x_px: f32, y_px: f32) -> bool {
        x_px >= self.left_px
            && x_px <= self.right_px
            && y_px >= self.top_px
            && y_px <= self.bottom_px
    }

    fn publish(regions: PublishedWindowDragRegions) {
        let mut published = REGIONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *published = regions;
    }
}

#[derive(Component, Default)]
struct ReportedWindowDragRegions(std::collections::BTreeMap<String, WindowDragRegionEvent>);

impl ReportedWindowDragRegions {
    fn update(&mut self, region: WindowDragRegionEvent) {
        let key = region.id.clone();
        if region.removed {
            self.0.remove(&key);
        } else {
            self.0.insert(key, region);
        }
    }
}

fn initialize_regions(
    webviews: Query<
        Entity,
        (
            Or<(Added<bevy_cef::prelude::WebviewSource>, Added<HostsPage>)>,
            Without<ReportedWindowDragRegions>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &webviews {
        commands
            .entity(entity)
            .insert(ReportedWindowDragRegions::default());
    }
}

fn region(
    trigger: On<UiInput<WindowDragRegionEvent>>,
    mut reported: Query<&mut ReportedWindowDragRegions>,
) {
    let Ok(mut reported) = reported.get_mut(trigger.event().webview) else {
        return;
    };
    reported.update(trigger.event().payload.clone());
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct WindowDragRegions<'w, 's> {
    reported: Query<'w, 's, (Entity, &'static ReportedWindowDragRegions)>,
    viewports: Query<'w, 's, (Entity, &'static ComputedNode), With<VmuxWindow>>,
    hierarchy: WindowHierarchy<'w, 's>,
    overlays: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static Node>,
            Option<&'static Visibility>,
            Has<vmux_ecs::KeyboardOwner>,
            Has<OverlayShownInline>,
        ),
        With<WindowOverlay>,
    >,
    captures: Query<'w, 's, &'static HostWindow, (With<LayoutCef>, LayoutPointerCapture)>,
}

impl WindowDragRegions<'_, '_> {
    fn blocked(&self, window: Entity) -> bool {
        if self.captures.iter().any(|host| host.0 == window) {
            return true;
        }
        for (entity, node, visibility, keyboard, inline) in &self.overlays {
            if self
                .hierarchy
                .get(entity)
                .is_some_and(|owner| owner != window)
            {
                continue;
            }
            if OverlayState::resolve(
                node.map_or(Display::Flex, |node| node.display),
                visibility.copied().unwrap_or_default(),
                keyboard,
                inline,
            )
            .owns_input()
            {
                return true;
            }
        }
        false
    }

    pub fn contains(&self, window: Entity, position: Vec2) -> bool {
        if self.blocked(window) {
            return false;
        }
        let mut allowed = false;
        for (viewport_entity, viewport) in &self.viewports {
            if self.hierarchy.get(viewport_entity) != Some(window) {
                continue;
            }
            for (page, reported) in &self.reported {
                if self.hierarchy.get(page) != Some(window) {
                    continue;
                }
                for report in reported.0.values() {
                    let Some(region) = WindowDragRegion::from_report(report.clone(), *viewport)
                    else {
                        continue;
                    };
                    if !region.contains(position.x, position.y) {
                        continue;
                    }
                    if report.blocked {
                        return false;
                    }
                    allowed = true;
                }
            }
        }
        allowed
    }
}

fn publish_region(regions: WindowDragRegions, windows: Query<Entity, With<Window>>) {
    let mut published = PublishedWindowDragRegions::default();
    for window in &windows {
        if regions.blocked(window) {
            continue;
        }
        for (viewport_entity, viewport) in &regions.viewports {
            if regions.hierarchy.get(viewport_entity) != Some(window) {
                continue;
            }
            for (page, reported) in &regions.reported {
                if regions.hierarchy.get(page) != Some(window) {
                    continue;
                }
                for report in reported.0.values() {
                    let Some(region) = WindowDragRegion::from_report(report.clone(), *viewport)
                    else {
                        continue;
                    };
                    if report.blocked {
                        published.blocked.push((window, region));
                    } else {
                        published.allowed.push((window, region));
                    }
                }
            }
        }
    }
    WindowDragRegion::publish(published);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    struct HeaderNode;

    impl HeaderNode {
        fn at(min: Vec2, size: Vec2, inverse_scale_factor: f32) -> ComputedNode {
            ComputedNode {
                size,
                center: min + size * 0.5,
                inverse_scale_factor,
                ..ComputedNode::default()
            }
        }
    }

    #[test]
    fn a_reported_region_becomes_physical_pixels_in_window_space() {
        let header = HeaderNode::at(Vec2::new(16.0, 16.0), Vec2::new(2000.0, 168.0), 0.5);
        let reported = WindowDragRegionEvent {
            id: "trailing".to_string(),
            removed: false,
            blocked: false,
            left: 300.0,
            top: 8.0,
            width: 400.0,
            height: 40.0,
        };

        let region = WindowDragRegion::from_report(reported, header).expect("region");

        assert_eq!(region.left_px, 600.0);
        assert_eq!(region.top_px, 16.0);
        assert_eq!(region.right_px, 1400.0);
        assert_eq!(region.bottom_px, 96.0);
        assert!(region.contains(700.0, 20.0));
        assert!(!region.contains(599.0, 20.0));
        assert!(!region.contains(700.0, 97.0));
    }

    #[test]
    fn a_region_reaching_past_the_header_is_clipped_to_it() {
        let header = HeaderNode::at(Vec2::ZERO, Vec2::new(1000.0, 168.0), 0.5);

        let region = WindowDragRegion::from_report(
            WindowDragRegionEvent {
                id: "trailing".to_string(),
                removed: false,
                blocked: false,
                left: 400.0,
                top: 0.0,
                width: 400.0,
                height: 40.0,
            },
            header,
        )
        .expect("region");

        assert_eq!(region.right_px, 1000.0);
        assert!(!region.contains(1001.0, 20.0));
    }

    #[test]
    fn a_region_the_header_has_squeezed_away_is_not_draggable() {
        let header = HeaderNode::at(Vec2::ZERO, Vec2::new(200.0, 84.0), 1.0);

        assert_eq!(
            WindowDragRegion::from_report(
                WindowDragRegionEvent {
                    id: "trailing".to_string(),
                    removed: false,
                    blocked: false,
                    left: 400.0,
                    top: 0.0,
                    width: 100.0,
                    height: 40.0,
                },
                header
            ),
            None
        );
        assert_eq!(
            WindowDragRegion::from_report(
                WindowDragRegionEvent {
                    id: "trailing".to_string(),
                    removed: false,
                    blocked: false,
                    left: 10.0,
                    top: 0.0,
                    width: 0.0,
                    height: 40.0,
                },
                header
            ),
            None
        );
    }

    #[test]
    fn removing_a_reported_region_preserves_the_same_region_in_another_window() {
        let mut reported = ReportedWindowDragRegions::default();
        let mut other = ReportedWindowDragRegions::default();
        reported.update(WindowDragRegionEvent {
            id: "leading".to_string(),
            removed: false,
            blocked: false,
            left: 10.0,
            top: 0.0,
            width: 20.0,
            height: 40.0,
        });
        other.update(WindowDragRegionEvent {
            id: "leading".to_string(),
            removed: false,
            blocked: false,
            left: 30.0,
            top: 0.0,
            width: 20.0,
            height: 40.0,
        });

        reported.update(WindowDragRegionEvent {
            id: "leading".to_string(),
            removed: true,
            ..Default::default()
        });

        assert!(reported.0.is_empty());
        assert_eq!(other.0.len(), 1);
        assert!(other.0.contains_key("leading"));
    }

    #[test]
    fn blocked_regions_override_allowed_regions() {
        let window = Entity::PLACEHOLDER;
        let regions = PublishedWindowDragRegions {
            allowed: vec![(
                window,
                WindowDragRegion {
                    left_px: 0.0,
                    top_px: 0.0,
                    right_px: 500.0,
                    bottom_px: 40.0,
                },
            )],
            blocked: vec![(
                window,
                WindowDragRegion {
                    left_px: 100.0,
                    top_px: 0.0,
                    right_px: 300.0,
                    bottom_px: 40.0,
                },
            )],
        };

        assert!(regions.contains(window, 50.0, 20.0));
        assert!(!regions.contains(window, 200.0, 20.0));
        assert!(regions.contains(window, 400.0, 20.0));
    }

    #[test]
    fn overlay_blocking_follows_its_window_and_unbound_overlays_remain_global() {
        let mut app = App::new();
        app.add_plugins(WindowDragPlugin);
        let first = app.world_mut().spawn(Window::default()).id();
        let second = app.world_mut().spawn(Window::default()).id();
        for window in [first, second] {
            app.world_mut().spawn((
                VmuxWindow,
                HostWindow(window),
                HeaderNode::at(Vec2::ZERO, Vec2::new(500.0, 40.0), 1.0),
            ));
            let mut reports = ReportedWindowDragRegions::default();
            reports.update(WindowDragRegionEvent {
                id: "titlebar".into(),
                left: 0.0,
                top: 0.0,
                width: 500.0,
                height: 40.0,
                ..default()
            });
            app.world_mut().spawn((HostWindow(window), reports));
        }
        let parent = app.world_mut().spawn(HostWindow(first)).id();
        let overlay = app
            .world_mut()
            .spawn((
                WindowOverlay,
                vmux_ecs::KeyboardOwner,
                Node::default(),
                Visibility::Visible,
                ChildOf(parent),
            ))
            .id();
        app.update();
        let allowed = app
            .world_mut()
            .run_system_once(move |regions: WindowDragRegions| {
                [
                    regions.contains(first, Vec2::new(50.0, 20.0)),
                    regions.contains(second, Vec2::new(50.0, 20.0)),
                ]
            })
            .unwrap();
        assert_eq!(allowed, [false, true]);
        app.world_mut().entity_mut(overlay).remove::<ChildOf>();
        app.update();
        let allowed = app
            .world_mut()
            .run_system_once(move |regions: WindowDragRegions| {
                [
                    regions.contains(first, Vec2::new(50.0, 20.0)),
                    regions.contains(second, Vec2::new(50.0, 20.0)),
                ]
            })
            .unwrap();
        assert_eq!(allowed, [false, false]);
    }
}
