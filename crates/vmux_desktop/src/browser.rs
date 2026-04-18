use crate::{
    command::{AppCommand, BrowserCommand, ReadAppCommands},
    layout::{
        window::{
            VmuxWindow, WEBVIEW_MESH_DEPTH_BIAS, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN,
            WEBVIEW_Z_SIDE_SHEET,
        },
        pane::{Pane, PaneSplit},
        side_sheet::SideSheet,
        tab::{Active, Tab, focused_tab},
    },
    settings::AppSettings,
};
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
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
    event::{HeaderCommandEvent, TABS_EVENT, TabRow, TabsHostEvent},
};
use vmux_side_sheet::event::{
    PANE_TREE_EVENT, PaneNode, PaneTreeEvent, SideSheetCommandEvent,
    TabNode,
};
use vmux_webview_app::{UiReady, WebviewAppRegistry};

pub(crate) struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let embedded_hosts = app
            .world()
            .resource::<WebviewAppRegistry>()
            .embedded_hosts();
        app.configure_sets(
            Update,
            CefSystems::CreateAndResize.after(ReadAppCommands),
        )
        .add_plugins(CefPlugin {
            root_cache_path: cef_root_cache_path(),
            embedded_hosts,
            ..default()
        })
        .add_plugins(JsEmitEventPlugin::<HeaderCommandEvent>::default())
        .add_plugins(JsEmitEventPlugin::<SideSheetCommandEvent>::default())
        .add_observer(on_header_command_emit)
        .add_observer(on_side_sheet_command_emit)
        .add_systems(
            Update,
            handle_browser_commands.in_set(ReadAppCommands),
        )
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
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    browser_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Some(active_tab_entity) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
        return;
    };
    for (browser_e, has_kb) in &browser_q {
        if status_q.contains(browser_e) || side_sheet_q.contains(browser_e) {
            continue;
        }

        let in_active = child_of_q
            .get(browser_e)
            .ok()
            .map(|co| co.get() == active_tab_entity)
            .unwrap_or(false);

        if in_active {
            if !has_kb {
                commands.entity(browser_e).insert(CefKeyboardTarget);
            }
        } else {
            if has_kb {
                commands.entity(browser_e).remove::<CefKeyboardTarget>();
            }
        }
    }
}

fn sync_children_to_ui(
    mut browser_q: Query<
        (
            &mut Transform,
            &ComputedNode,
            &UiGlobalTransform,
            &ChildOf,
            &mut WebviewSize,
            Option<&Header>,
            Option<&SideSheet>,
        ),
        With<Browser>,
    >,
    child_of_q: Query<&ChildOf>,
    pane_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<VmuxWindow>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;
    let pad = glass_node.padding;
    let glass_size_px = glass_node.size + pad.min_inset + pad.max_inset;

    for (mut tf, self_computed, self_ui_gt, child_of, mut webview_size, status, side_sheet) in
        browser_q.iter_mut()
    {
        let parent = child_of.get();
        let pane_entity = child_of_q
            .get(parent)
            .map(|co| co.get())
            .unwrap_or(parent);
        let (computed, ui_gt) = match pane_rect.get(pane_entity) {
            Ok((cn, gt)) => (cn, gt),
            Err(_) => (self_computed, self_ui_gt),
        };

        if glass_size_px.x <= 0.0 || glass_size_px.y <= 0.0 {
            continue;
        }

        let size_px = computed.size;
        if size_px.x <= 0.0 || size_px.y <= 0.0 {
            continue;
        }

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
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,

    mut ready: Local<Vec<Entity>>,
    mut auxiliary: Local<Vec<Entity>>,
    mut last_active: Local<Option<Entity>>,
    mut last_ready_set: Local<Vec<Entity>>,
) {
    ready.clear();
    ready.extend(webviews.iter().filter(|&e| browsers.has_browser(e)));
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|e| e.to_bits());

    let active = focused_tab(&active_pane, &pane_children_q, &active_tabs)
        .and_then(|tab| {
            ready.iter().copied().find(|&b| {
                child_of_q.get(b).ok().map(|co| co.get()) == Some(tab)
            })
        })
        .unwrap_or(ready[0]);

    if *last_active == Some(active) && *last_ready_set == *ready {
    } else {
        auxiliary.clear();
        auxiliary.extend(ready.iter().copied().filter(|&e| e != active));
        browsers.sync_osr_focus_to_active_pane(Some(active), auxiliary.as_slice());
        *last_active = Some(active);
        last_ready_set.clone_from(&ready);
    }
    for &e in ready.iter() {
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
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    mut last: Local<String>,
) {
    let status_e = *status;
    if !browsers.has_browser(status_e) || !browsers.host_emit_ready(&status_e) {
        return;
    }
    let Some(active_tab_entity) = focused_tab(&active_pane_q, &pane_children_q, &active_tabs) else {
        return;
    };
    let active_pane = active_pane_q.single().ok();
    let mut rows: Vec<TabRow> = Vec::new();
    for (meta, child_of) in &browser_q {
        let tab_entity = child_of.get();
        let tab_pane = child_of_q.get(tab_entity).ok().map(|co| co.get());
        if tab_pane != active_pane {
            continue;
        }
        rows.push(TabRow {
            title: meta.title.clone(),
            url: meta.url.clone(),
            favicon_url: meta.favicon_url.clone(),
            is_active: tab_entity == active_tab_entity,
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
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    active_tab_q: Query<Entity, (With<Active>, With<Tab>)>,
    pane_children: Query<&Children, With<Pane>>,
    tab_q: Query<Entity, With<Tab>>,
    tab_children: Query<&Children>,
    browser_meta: Query<&PageMetadata, With<Browser>>,
    mut last: Local<String>,
) {
    let Some(side_sheet) = side_sheet else {
        return;
    };
    let side_sheet_e = *side_sheet;
    if !browsers.has_browser(side_sheet_e) || !browsers.host_emit_ready(&side_sheet_e) {
        return;
    }
    let active_pane = active_pane_q.single().ok();

    let mut panes: Vec<PaneNode> = Vec::new();
    for pane_entity in &leaf_panes {
        let is_active = active_pane == Some(pane_entity);
        let mut tabs: Vec<TabNode> = Vec::new();
        let mut tab_index: usize = 0;
        if let Ok(children) = pane_children.get(pane_entity) {
            for child in children.iter() {
                if !tab_q.contains(child) {
                    continue;
                }
                let tab_is_active = active_tab_q.contains(child);
                if let Ok(tab_kids) = tab_children.get(child) {
                    for browser_e in tab_kids.iter() {
                        if let Ok(meta) = browser_meta.get(browser_e) {
                            tabs.push(TabNode {
                                title: meta.title.clone(),
                                url: meta.url.clone(),
                                favicon_url: meta.favicon_url.clone(),
                                is_active: tab_is_active,
                                tab_index,
                            });
                        }
                    }
                }
                tab_index += 1;
            }
        }
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

fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(browser_cmd) = *cmd else {
            continue;
        };
        let Some(active) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
            continue;
        };
        let Some(webview) = browsers
            .iter()
            .find(|(_, co)| co.get() == active)
            .map(|(e, _)| e)
        else {
            continue;
        };
        match browser_cmd {
            BrowserCommand::PrevPage => commands.trigger(RequestGoBack { webview }),
            BrowserCommand::NextPage => commands.trigger(RequestGoForward { webview }),
            BrowserCommand::Reload => commands.trigger(RequestReload { webview }),
        }
    }
}

fn on_header_command_emit(
    trigger: On<Receive<HeaderCommandEvent>>,
    mut messages: ResMut<Messages<AppCommand>>,
) {
    let cmd = match trigger.event().payload.header_command.as_str() {
        "prev_page" => BrowserCommand::PrevPage,
        "next_page" => BrowserCommand::NextPage,
        "reload" => BrowserCommand::Reload,
        _ => return,
    };
    messages.write(AppCommand::Browser(cmd));
}

fn on_side_sheet_command_emit(
    trigger: On<Receive<SideSheetCommandEvent>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    active_tab_q: Query<Entity, (With<Active>, With<Tab>)>,
    pane_children: Query<&Children, With<Pane>>,
    tab_q: Query<Entity, With<Tab>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    if evt.command != "activate_tab" {
        return;
    }
    let Ok(pane_id) = evt.pane_id.parse::<u64>() else {
        return;
    };
    let target_pane = leaf_panes
        .iter()
        .find(|e| e.to_bits() == pane_id);
    let Some(target_pane) = target_pane else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let tab_entities: Vec<Entity> = children
        .iter()
        .filter(|&e| tab_q.contains(e))
        .collect();
    let Some(&target_tab) = tab_entities.get(evt.tab_index) else {
        return;
    };
    if let Ok(old_pane) = active_pane_q.single() {
        if old_pane != target_pane {
            commands.entity(old_pane).remove::<Active>();
        }
    }
    let old_tab_in_pane = pane_children
        .get(target_pane)
        .ok()
        .and_then(|ch| ch.iter().find(|&e| active_tab_q.contains(e)));
    if let Some(old_tab) = old_tab_in_pane {
        if old_tab != target_tab {
            commands.entity(old_tab).remove::<Active>();
        }
    }
    commands.entity(target_pane).insert(Active);
    commands.entity(target_tab).insert(Active);
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
