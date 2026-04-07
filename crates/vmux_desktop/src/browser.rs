use std::path::PathBuf;

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy_cef::prelude::*;
use vmux_webview_app::JsEmitUiReadyPlugin;

use crate::layout_next::{LayoutPlane, Tab, TabLayoutSync};
use crate::settings::AppSettings;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            CefPlugin {
                root_cache_path: cef_root_cache_path(),
                ..default()
            },
        ))
        .add_observer(spawn_browser_on_tab_added)
        .add_systems(
            PostUpdate,
            sync_browser_plane_to_window
                .after(TabLayoutSync)
                .after(CameraUpdateSystems),
        );
    }
}

#[derive(Bundle)]
struct BrowserBundle {
    browser: Browser,
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
    webview_size: WebviewSize,
}

#[derive(Component)]
struct Browser;

pub struct BrowserPlugin;

fn spawn_browser_on_tab_added(
    add: On<Add, Tab>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: Res<AppSettings>,
    layout_on_tab: Query<&LayoutPlane, With<Tab>>,
    children_q: Query<&Children>,
    browser_q: Query<(), With<Browser>>,
) {
    let tab = add.entity;
    if let Ok(children) = children_q.get(tab) {
        for child in children.iter() {
            if browser_q.contains(child) {
                return;
            }
        }
    }
    let Ok(layout_plane) = layout_on_tab.get(tab) else {
        return;
    };
    let layout = *layout_plane;
    let mut mat = WebviewExtendStandardMaterial::default();
    mat.base.unlit = true;
    mat.base.alpha_mode = AlphaMode::Blend;
    mat.extension.pane_corner_clip = webview_corner_clip_uniform(&layout);
    let inner_mesh = meshes.add(Plane3d::new(Vec3::Z, layout.inner_world_half));
    let browser_id = commands
        .spawn(BrowserBundle {
            browser: Browser,
            source: WebviewSource::new(settings.browser.startup_url.as_str()),
            mesh: Mesh3d(inner_mesh),
            material: MeshMaterial3d(materials.add(mat)),
            webview_size: WebviewSize(layout.inner_px),
        })
        .id();
    commands.entity(tab).add_child(browser_id);
}

fn sync_browser_plane_to_window(
    tabs: Query<Ref<LayoutPlane>, With<Tab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut browsers: Query<
        (
            &ChildOf,
            &mut WebviewSize,
            &mut Mesh3d,
            &MeshMaterial3d<WebviewExtendStandardMaterial>,
        ),
        With<Browser>,
    >,
) {
    for (child_of, mut webview_size, mesh_3d, mat_h) in browsers.iter_mut() {
        let tab = child_of.parent();
        let Ok(plane_ref) = tabs.get(tab) else {
            continue;
        };
        if !plane_ref.is_changed() {
            continue;
        }
        let layout = *plane_ref;
        webview_size.0 = layout.inner_px;
        if let Some(mesh) = meshes.get_mut(&mesh_3d.0) {
            *mesh = Mesh::from(Plane3d::new(Vec3::Z, layout.inner_world_half));
        }
        if let Some(mat) = materials.get_mut(&mat_h.0) {
            mat.base.unlit = true;
            mat.extension.pane_corner_clip = webview_corner_clip_uniform(&layout);
        }
    }
}

fn webview_corner_clip_uniform(layout: &LayoutPlane) -> Vec4 {
    let w = layout.inner_px.x.max(1.0e-6);
    let h = layout.inner_px.y.max(1.0e-6);
    Vec4::new(layout.r_px, w, h, 0.0)
}

fn cef_root_cache_path() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library/Application Support/vmux")
                .to_string_lossy()
                .into_owned()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir()
            .to_str()
            .map(|p| format!("{p}/vmux_cef"))
    }
}
