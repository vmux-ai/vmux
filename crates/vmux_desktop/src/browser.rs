use crate::{
    layout::{
        display::{
            DisplayGlass, WEBVIEW_MESH_DEPTH_BIAS, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN,
            WEBVIEW_Z_SIDE_SHEET,
        },
        pane::{Active, Pane, PaneSplit},
        side_sheet::SideSheet,
    },
    settings::AppSettings,
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    render::alpha::AlphaMode,
    ui::{UiGlobalTransform, UiSystems},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::RenderTextureMessage;
use std::{collections::HashSet, path::PathBuf};
use vmux_header::{
    Header, PageMetadata,
    event::{TABS_EVENT, TabRow, TabsHostEvent},
};
use vmux_side_sheet::event::{PANE_TREE_EVENT, PaneNode, PaneTreeEvent, TabNode};
use vmux_webview_app::{UiReady, WebviewAppRegistry};

pub(crate) struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let embedded_hosts = app
            .world()
            .resource::<WebviewAppRegistry>()
            .embedded_hosts();
        app.add_plugins(CefPlugin {
            root_cache_path: cef_root_cache_path(),
            embedded_hosts,
            ..default()
        })
        .add_systems(
            Update,
            (push_tabs_host_emit, push_pane_tree_emit)
                .after(vmux_header::system::apply_chrome_state_from_cef),
        )
        .add_systems(
            PostUpdate,
            (
                sync_keyboard_target,
                sync_children_to_ui,
                sync_cef_webview_resize_after_ui,
                sync_webview_pane_corner_clip,
                sync_osr_webview_focus,
                kick_tab_startup_navigation,
                flush_pending_osr_textures,
            )
                .chain()
                .after(UiSystems::Layout)
                .before(render_standard_materials),
        );
    }
}

#[derive(Component)]
pub(crate) struct Browser;

pub(crate) fn browser_bundle(
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    url: &str,
) -> impl Bundle {
    (
        Browser,
        vmux_header::PageMetadata {
            title: url.to_string(),
            url: url.to_string(),
            favicon_url: String::new(),
        },
        WebviewSource::new(url),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                ..default()
            },
            ..default()
        })),
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
    )
}

fn sync_keyboard_target(
    browsers: NonSend<Browsers>,
    active_pane: Query<Entity, With<Active>>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut browser_q: Query<(Entity, &mut Visibility, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Ok(active_entity) = active_pane.single() else {
        return;
    };
    for (browser_e, mut visibility, has_kb) in &mut browser_q {
        if status_q.contains(browser_e) || side_sheet_q.contains(browser_e) {
            continue;
        }
        *visibility = Visibility::Inherited;
        browsers.set_osr_not_hidden(&browser_e);

        let in_active = child_of_q
            .get(browser_e)
            .ok()
            .map(|co| co.get() == active_entity)
            .unwrap_or(false);

        if in_active && !has_kb {
            commands.entity(browser_e).insert(CefKeyboardTarget);
        } else if !in_active && has_kb {
            commands.entity(browser_e).remove::<CefKeyboardTarget>();
        }
    }
}

fn sync_children_to_ui(
    mut browser_q: Query<
        (
            &mut Transform,
            &mut Visibility,
            &ComputedNode,
            &UiGlobalTransform,
            &ChildOf,
            &mut WebviewSize,
            Option<&Header>,
            Option<&SideSheet>,
        ),
        With<Browser>,
    >,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<DisplayGlass>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;
    let pad = glass_node.padding;
    let glass_size_px = glass_node.size + pad.min_inset + pad.max_inset;

    for (mut tf, mut visibility, self_computed, self_ui_gt, child_of, mut webview_size, status, side_sheet) in
        browser_q.iter_mut()
    {
        let parent = child_of.get();
        let (computed, ui_gt) = match pane_rect.get(parent) {
            Ok((cn, gt)) => (cn, gt),
            Err(_) => (self_computed, self_ui_gt),
        };

        if glass_size_px.x <= 0.0 || glass_size_px.y <= 0.0 {
            continue;
        }

        let size_px = computed.size;
        if size_px.x <= 0.0 || size_px.y <= 0.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Inherited;

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        tf.scale = Vec3::new(sx, sy, 1.0);

        let center_ui = ui_gt.transform_point2(Vec2::ZERO);
        let glass_center_ui = glass_ui_gt.transform_point2(Vec2::ZERO);
        let delta_px = center_ui - glass_center_ui;

        let tx = delta_px.x / glass_size_px.x;
        let ty = -delta_px.y / glass_size_px.y;
        let z = if status.is_some() {
            WEBVIEW_Z_HEADER
        } else if side_sheet.is_some() {
            WEBVIEW_Z_SIDE_SHEET
        } else if parent != glass_entity {
            WEBVIEW_Z_MAIN
        } else {
            0.01 + self_computed.stack_index as f32 * 0.001
        };
        tf.translation = Vec3::new(tx, ty, z);

        let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
        if webview_size.0 != dip {
            webview_size.0 = dip;
        }
    }
}

fn sync_cef_webview_resize_after_ui(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), (Changed<WebviewSize>, With<Browser>)>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    for (entity, size) in webviews.iter() {
        if !browsers.has_browser(entity) {
            continue;
        }
        let window_entity = host_window
            .get(entity)
            .ok()
            .map(|h| h.0)
            .or_else(|| primary_window.single().ok());
        let device_scale_factor = window_entity
            .and_then(|e| windows.get(e).ok())
            .map(|w| w.resolution.scale_factor() as f32)
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        browsers.resize(&entity, size.0, device_scale_factor);
    }
}

fn sync_webview_pane_corner_clip(
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    tabs: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<Browser>>,
    status: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<Header>>,
    side_sheet: Query<
        (&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>),
        With<SideSheet>,
    >,
) {
    let r = settings.layout.pane.radius;
    for (size, mat_h) in &tabs {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
        }
    }
    for (size, mat_h) in &status {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 2.0);
        }
    }
    for (size, mat_h) in &side_sheet {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
        }
    }
}

fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    keyboard_target: Query<Entity, (With<WebviewSource>, With<CefKeyboardTarget>)>,
    status_chrome: Query<Entity, (With<Header>, With<Browser>)>,
    side_sheet_chrome: Query<Entity, (With<SideSheet>, With<Browser>)>,
    mut ready: Local<Vec<Entity>>,
    mut auxiliary: Local<Vec<Entity>>,
) {
    ready.clear();
    ready.extend(webviews.iter().filter(|&e| browsers.has_browser(e)));
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|e| e.to_bits());

    let active = keyboard_target
        .iter()
        .filter(|&k| ready.iter().any(|&e| e == k))
        .min_by_key(|e| e.to_bits())
        .unwrap_or(ready[0]);

    auxiliary.clear();
    auxiliary.extend(ready.iter().copied().filter(|&e| e != active));
    browsers.sync_osr_focus_to_active_pane(Some(active), auxiliary.as_slice());
    for e in status_chrome.iter() {
        browsers.set_osr_not_hidden(&e);
    }
    for e in side_sheet_chrome.iter() {
        browsers.set_osr_not_hidden(&e);
    }
}

fn kick_tab_startup_navigation(
    browsers: NonSend<Browsers>,
    q: Query<(Entity, &WebviewSource), With<Browser>>,
    mut kicked: Local<HashSet<u64>>,
) {
    for (entity, source) in &q {
        let WebviewSource::Url(url) = source else {
            continue;
        };
        let key = entity.to_bits();
        if kicked.contains(&key) {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        browsers.navigate(&entity, url);
        kicked.insert(key);
    }
}

fn flush_pending_osr_textures(
    mut ew: MessageWriter<RenderTextureMessage>,
    browsers: NonSend<Browsers>,
) {
    while let Ok(texture) = browsers.try_receive_texture() {
        ew.write(texture);
    }
}

fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    status: Single<Entity, (With<Header>, With<UiReady>)>,
    browser_q: Query<(&PageMetadata, &ChildOf), With<Browser>>,
    active_pane: Query<(), With<Active>>,
    mut last: Local<String>,
) {
    let status_e = *status;
    if !browsers.has_browser(status_e) || !browsers.host_emit_ready(&status_e) {
        return;
    }
    let mut rows: Vec<TabRow> = Vec::new();
    for (meta, child_of) in &browser_q {
        if !active_pane.contains(child_of.get()) {
            continue;
        }
        rows.push(TabRow {
            title: meta.title.clone(),
            url: meta.url.clone(),
            favicon_url: meta.favicon_url.clone(),
            is_active: true,
        });
    }
    let payload = TabsHostEvent { tabs: rows };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if ron_body.as_str() == last.as_str() {
        return;
    }
    commands.trigger(HostEmitEvent::new(status_e, TABS_EVENT, &ron_body));
    *last = ron_body;
}

fn push_pane_tree_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    side_sheet: Option<Single<Entity, (With<SideSheet>, With<UiReady>)>>,
    leaf_panes: Query<(Entity, Has<Active>), (With<Pane>, Without<PaneSplit>)>,
    browser_q: Query<(&PageMetadata, &ChildOf), With<Browser>>,
    mut last: Local<String>,
) {
    let Some(side_sheet) = side_sheet else {
        return;
    };
    let side_sheet_e = *side_sheet;
    if !browsers.has_browser(side_sheet_e) || !browsers.host_emit_ready(&side_sheet_e) {
        return;
    }
    let mut panes: Vec<PaneNode> = Vec::new();
    for (pane_entity, is_active) in &leaf_panes {
        let tabs: Vec<TabNode> = browser_q
            .iter()
            .filter(|(_, child_of)| child_of.get() == pane_entity)
            .map(|(meta, _)| TabNode {
                title: meta.title.clone(),
                url: meta.url.clone(),
                favicon_url: meta.favicon_url.clone(),
            })
            .collect();
        panes.push(PaneNode {
            id: pane_entity.to_bits(),
            is_active,
            tabs,
        });
    }
    panes.sort_by_key(|p| p.id);
    let payload = PaneTreeEvent { panes };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if ron_body.as_str() == last.as_str() {
        return;
    }
    commands.trigger(HostEmitEvent::new(side_sheet_e, PANE_TREE_EVENT, &ron_body));
    *last = ron_body;
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
