use bevy::{prelude::*, window::WindowResized};
use bevy_cef::prelude::{Browsers, HostWindow, WebviewWindowed};
use vmux_flex::prelude::LayoutSystems;

pub struct PopupWebviewsPlugin;

impl Plugin for PopupWebviewsPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_non_send::<Browsers>() {
            app.insert_non_send(Browsers::default());
        }
        app.add_message::<WindowResized>().add_systems(
            PostUpdate,
            present
                .in_set(crate::BrowserOverlaySet)
                .after(LayoutSystems::Layout),
        );
        app.add_observer(hide).add_observer(script);
    }
}

#[derive(EntityEvent, Clone, Copy)]
pub struct PopupHideRequest {
    #[event_target]
    pub webview: Entity,
}

#[derive(EntityEvent, Clone)]
pub struct PopupScriptRequest {
    #[event_target]
    pub webview: Entity,
    pub script: String,
}

fn hide(
    request: On<PopupHideRequest>,
    browsers: NonSend<Browsers>,
    popups: Query<(), With<crate::PopupWebview>>,
) {
    let entity = request.event_target();
    if popups.contains(entity) {
        browsers.hide_child_window(&entity);
    }
}

fn script(
    request: On<PopupScriptRequest>,
    browsers: NonSend<Browsers>,
    popups: Query<(), With<crate::PopupWebview>>,
) {
    let entity = request.event_target();
    if popups.contains(entity) {
        browsers.execute_js(&entity, &request.event().script);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PopupBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl PopupBounds {
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Option<Self> {
        if !left.is_finite()
            || !top.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }
        Some(Self {
            left,
            top,
            width,
            height,
        })
    }
}

#[derive(Component)]
pub struct PopupPresented;

fn present(
    browsers: NonSend<Browsers>,
    popups: Query<
        (Entity, &PopupBounds, &HostWindow, Has<PopupPresented>),
        (
            With<crate::PopupWebview>,
            With<vmux_ecs::WindowOverlay>,
            With<WebviewWindowed>,
        ),
    >,
    windows: Query<&Window>,
    mut resized: MessageReader<WindowResized>,
    mut commands: Commands,
) {
    let resized_windows: Vec<_> = resized.read().map(|event| event.window).collect();
    for (entity, bounds, host, presented) in &popups {
        if resized_windows.contains(&host.0) {
            continue;
        }
        let Ok(window) = windows.get(host.0) else {
            continue;
        };
        if !browsers.has_browser(entity) {
            continue;
        }
        let scale = window.resolution.scale_factor();
        browsers.resize(&entity, Vec2::new(bounds.width, bounds.height), scale);
        browsers.set_windowed_corner_radius(&entity, 14.0 * scale, scale, true);
        if !browsers.host_in_child_window(
            &entity,
            bounds.left as f64,
            bounds.top as f64,
            bounds.width as f64,
            bounds.height as f64,
        ) {
            continue;
        }
        browsers.set_windowed_hidden(&entity, false);
        if presented {
            continue;
        }
        browsers.set_windowed_focus(&entity, true);
        browsers.nudge_windowed_repaint(&entity);
        commands.entity(entity).insert(PopupPresented);
    }
}
