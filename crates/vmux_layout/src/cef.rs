use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_cef::prelude::*;

use crate::event::LAYOUT_PAGE_URL;

pub struct LayoutCefPlugin;

impl Plugin for LayoutCefPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::LAYOUT_PAGE_MANIFEST);
        app.world_mut().spawn(crate::COMMAND_BAR_PAGE_MANIFEST);
        app.world_mut().spawn(crate::DEBUG_PAGE_MANIFEST);
        app.world_mut().spawn(crate::ERROR_PAGE_MANIFEST);
    }
}

const LAYOUT_OSR_MAX_FRAME_RATE: i32 = 30;

#[derive(Component)]
pub struct Browser;

#[derive(Component)]
pub struct LayoutCef;

#[derive(Component)]
pub struct Loading;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct NavigationState {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub fn mirror_metadata_to_url(
    cef_q: Query<
        &vmux_core::PageMetadata,
        (Without<vmux_core::Url>, Changed<vmux_core::PageMetadata>),
    >,
    mut urls: Query<&mut vmux_core::PageMetadata, With<vmux_core::Url>>,
) {
    for tab_meta in cef_q.iter() {
        if tab_meta.url.is_empty() {
            continue;
        }
        for mut url_meta in urls.iter_mut() {
            if url_meta.url == tab_meta.url {
                if !tab_meta.title.is_empty() {
                    url_meta.title.clone_from(&tab_meta.title);
                }
                if !tab_meta.icon.is_none() {
                    url_meta.icon.clone_from(&tab_meta.icon);
                }
                if tab_meta.bg_color.is_some() {
                    url_meta.bg_color.clone_from(&tab_meta.bg_color);
                }
                break;
            }
        }
    }
}

pub fn apply_cef_state_from_webview(
    cef_rx: Res<WebviewCefStateReceiver>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata>,
) {
    while let Ok(ev) = cef_rx.0.try_recv() {
        let Ok(mut meta) = browser_meta.get_mut(ev.webview) else {
            continue;
        };
        apply_cef_state_to_meta(&mut meta, ev);
    }
}

pub(crate) fn apply_cef_state_to_meta(
    meta: &mut vmux_core::PageMetadata,
    ev: bevy_cef_core::prelude::WebviewCefStateEvent,
) {
    let on_native_view = meta.url.starts_with("vmux://");
    let accepts_dynamic_title = meta.url.starts_with("vmux://agent/");
    let navigating_away = ev.url.as_deref().is_some_and(|u| !u.starts_with("vmux://"));
    if on_native_view && !navigating_away {
        if accepts_dynamic_title && let Some(title) = ev.title {
            meta.title = title;
        }
        return;
    }
    if let Some(url) = ev.url {
        meta.url = url;
        meta.icon = vmux_core::PageIcon::None;
    }
    if let Some(title) = ev.title {
        meta.title = title;
    }
    if let Some(favicon) = ev.favicon_url {
        meta.icon = vmux_core::PageIcon::favicon(favicon);
    }
}

impl Browser {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        url: &str,
    ) -> impl Bundle {
        (
            Self,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            vmux_core::PageMetadata {
                title: url.to_string(),
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        )
    }

    pub fn new_with_title(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        url: &str,
        title: &str,
    ) -> impl Bundle {
        (
            Self,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            vmux_core::PageMetadata {
                title: title.to_string(),
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        )
    }

    pub fn new_error(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        source_url: &str,
        display_url: &str,
        title: &str,
    ) -> impl Bundle {
        (
            Self,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            vmux_core::PageMetadata {
                title: title.to_string(),
                url: display_url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSource::new(source_url),
            ResolvedWebviewUri(source_url.to_string()),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        )
    }
}

pub fn layout_cef_bundle(
    host_window: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> impl Bundle {
    (
        (
            LayoutCef,
            Browser,
            HostWindow(host_window),
            WebviewTransparent,
            WebviewMaxFrameRate(LAYOUT_OSR_MAX_FRAME_RATE),
            bevy_cef::prelude::CefIgnorePinchZoom,
        ),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        ZIndex(2),
        WebviewSource::new(LAYOUT_PAGE_URL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
        WebviewSize(Vec2::new(1280.0, 720.0)),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Inherited,
        Pickable::IGNORE,
    )
}

#[cfg(test)]
#[path = "cef.apply_cef_state.test.rs"]
mod apply_cef_state_tests;
#[cfg(test)]
#[path = "cef.test.rs"]
mod tests;
#[cfg(test)]
#[path = "cef.url_mirror.test.rs"]
mod url_mirror_tests;
