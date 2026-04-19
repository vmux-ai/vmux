use crate::{
    command::{AppCommand, BrowserCommand, ReadAppCommands},
    layout::{
        window::{
            VmuxWindow, WEBVIEW_MESH_DEPTH_BIAS, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN,
            WEBVIEW_Z_SIDE_SHEET,
        },
        pane::{Pane, PaneHoverIntent, PaneSplit, first_leaf_descendant, first_tab_in_pane},
        side_sheet::SideSheet,
        space::Space,
        tab::{Tab, tab_bundle, focused_tab, active_among,
              active_tab_in_pane, collect_leaf_panes},
    },
    settings::AppSettings,
};
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    picking::Pickable,
    prelude::*,
    render::alpha::AlphaMode,
    ui::{UiGlobalTransform, UiSystems},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::RenderTextureMessage;
use std::path::PathBuf;
use vmux_header::{
    Header, PageMetadata,
    event::{HeaderCommandEvent, RELOAD_EVENT, TABS_EVENT, TabRow, TabsHostEvent},
};
use vmux_history::{CreatedAt, LastActivatedAt, Visit};
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
        .add_observer(on_reload_notify_header)
        .add_observer(on_hard_reload_notify_header)
        .add_systems(
            Update,
            (
                handle_browser_commands.in_set(ReadAppCommands),
                drain_loading_state,
            ),
        )
        .add_systems(
            Update,
            (sync_page_metadata_to_tab, spawn_visit_on_navigation)
                .chain()
                .after(vmux_header::system::apply_chrome_state_from_cef),
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

#[derive(Component)]
pub(crate) struct Loading;

impl Browser {
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        url: &str,
    ) -> impl Bundle {
        (
            Self,
        vmux_header::PageMetadata {
            title: url.to_string(),
            url: url.to_string(),
            favicon_url: String::new(),
        },
        WebviewSource::new(url),
        ResolvedWebviewUri(url.to_string()),
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
            Pickable::default(),
        )
    }
}

fn sync_keyboard_target(
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    browser_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let (_, _, active_tab_opt) = focused_tab(
        &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
    );
    let Some(active_tab_entity) = active_tab_opt else {
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
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
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

        // Check if this browser's parent tab is the active tab in its pane
        let is_active_tab = if parent != glass_entity && status.is_none() && side_sheet.is_none() {
            active_tab_in_pane(pane_entity, &pane_children, &tab_ts) == Some(parent)
        } else {
            true
        };

        let is_inactive_tab = parent != glass_entity
            && status.is_none()
            && side_sheet.is_none()
            && !is_active_tab;

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        let new_scale = if is_inactive_tab {
            Vec3::splat(1e-6)
        } else {
            Vec3::new(sx, sy, 1.0)
        };
        if parent != glass_entity && status.is_none() && side_sheet.is_none() && (tf.scale - new_scale).length() > 0.01 {
            info!("[ui] browser child_of={:?} scale {:?} -> {:?} (inactive={})", parent, tf.scale, new_scale, is_inactive_tab);
        }
        tf.scale = new_scale;

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
            if is_active_tab {
                WEBVIEW_Z_MAIN
            } else {
                WEBVIEW_Z_MAIN - 0.01
            }
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
    webviews: Query<(Entity, &WebviewSize), With<Browser>>,
    host_window: Query<&HostWindow>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut last_sizes: Local<Vec<(u64, Vec2)>>,
) {
    for (entity, size) in webviews.iter() {
        if !browsers.has_browser(entity) {
            continue;
        }
        let key = entity.to_bits();
        if last_sizes.iter().any(|(k, s)| *k == key && *s == size.0) {
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
        if let Some(entry) = last_sizes.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = size.0;
        } else {
            last_sizes.push((key, size.0));
        }
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
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 0.0);
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
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children_q: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
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

    let (_, _, active_tab_opt) = focused_tab(
        &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children_q, &tab_ts,
    );
    let active = active_tab_opt
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

fn drain_loading_state(
    receiver: Res<WebviewLoadingStateReceiver>,
    mut commands: Commands,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if ev.is_loading {
            commands.entity(ev.webview).insert(Loading);
        } else {
            commands.entity(ev.webview).remove::<Loading>();
        }
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
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children_q: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of_q: Query<&ChildOf>,
    mut last: Local<String>,
) {
    let status_e = *status;
    if !browsers.has_browser(status_e) || !browsers.host_emit_ready(&status_e) {
        return;
    }
    let (_, active_pane, active_tab_opt) = focused_tab(
        &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children_q, &tab_ts,
    );
    let Some(active_tab_entity) = active_tab_opt else {
        return;
    };
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
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
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

    let (_, active_pane, _) = focused_tab(
        &spaces, &all_children, &leaf_pane_q, &pane_ts, &pane_children, &tab_ts,
    );

    let active_space = active_among(spaces.iter());
    let Some(space) = active_space else {
        return;
    };
    let mut space_leaf_panes = Vec::new();
    collect_leaf_panes(space, &all_children, &leaf_pane_q, &mut space_leaf_panes);

    let mut panes: Vec<PaneNode> = Vec::new();
    for &pane_entity in &space_leaf_panes {
        let is_active = active_pane == Some(pane_entity);
        let active_tab = active_tab_in_pane(pane_entity, &pane_children, &tab_ts);
        let mut tabs: Vec<TabNode> = Vec::new();
        let mut tab_index: usize = 0;
        if let Ok(children) = pane_children.get(pane_entity) {
            for child in children.iter() {
                if !tab_q.contains(child) {
                    continue;
                }
                let tab_is_active = active_tab == Some(child);
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
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut zoom_q: Query<&mut ZoomLevel, With<Browser>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let AppCommand::Browser(browser_cmd) = *cmd else {
            continue;
        };
        let (_, _, active_tab_opt) = focused_tab(
            &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
        );
        let Some(active) = active_tab_opt else {
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
            BrowserCommand::HardReload => commands.trigger(RequestReloadIgnoreCache { webview }),
            BrowserCommand::Stop => {}
            BrowserCommand::FocusAddressBar => {}
            BrowserCommand::Find => {}
            BrowserCommand::ZoomIn => {
                if let Ok(mut z) = zoom_q.get_mut(webview) {
                    z.0 += 0.5;
                }
            }
            BrowserCommand::ZoomOut => {
                if let Ok(mut z) = zoom_q.get_mut(webview) {
                    z.0 -= 0.5;
                }
            }
            BrowserCommand::ZoomReset => {
                if let Ok(mut z) = zoom_q.get_mut(webview) {
                    z.0 = 0.0;
                }
            }
            BrowserCommand::DevTools => commands.trigger(RequestShowDevTool { webview }),
            BrowserCommand::ViewSource => {}
            BrowserCommand::Print => {}
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

fn on_reload_notify_header(
    _trigger: On<RequestReload>,
    header: Option<Single<Entity, (With<Header>, With<UiReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(header) = header else { return };
    let header_e = *header;
    if browsers.has_browser(header_e) && browsers.host_emit_ready(&header_e) {
        commands.trigger(HostEmitEvent::new(header_e, RELOAD_EVENT, &"()"));
    }
}

fn on_hard_reload_notify_header(
    _trigger: On<RequestReloadIgnoreCache>,
    header: Option<Single<Entity, (With<Header>, With<UiReady>)>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(header) = header else { return };
    let header_e = *header;
    if browsers.has_browser(header_e) && browsers.host_emit_ready(&header_e) {
        commands.trigger(HostEmitEvent::new(header_e, RELOAD_EVENT, &"()"));
    }
}

fn on_side_sheet_command_emit(
    trigger: On<Receive<SideSheetCommandEvent>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
    child_of_q: Query<&ChildOf>,
    split_q: Query<(), With<PaneSplit>>,
    pane_ui_q: Query<&UiGlobalTransform, With<Pane>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut hover_intent: ResMut<PaneHoverIntent>,
    settings: Res<AppSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let Ok(pane_id) = evt.pane_id.parse::<u64>() else {
        return;
    };
    let Some(target_pane) = leaf_panes.iter().find(|e| e.to_bits() == pane_id) else {
        return;
    };
    let Ok(children) = pane_children.get(target_pane) else {
        return;
    };
    let tab_entities: Vec<Entity> = children
        .iter()
        .filter(|&e| tab_q.contains(e))
        .collect();

    match evt.command.as_str() {
        "activate_tab" => {
            let Some(&target_tab) = tab_entities.get(evt.tab_index) else {
                return;
            };
            commands.entity(target_pane).insert(LastActivatedAt::now());
            commands.entity(target_tab).insert(LastActivatedAt::now());

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());

            if let Ok(ui_gt) = pane_ui_q.get(target_pane) {
                let center = ui_gt.transform_point2(Vec2::ZERO);
                if let Ok(mut window) = windows.single_mut() {
                    window.set_physical_cursor_position(Some(center.as_dvec2()));
                }
            }
        }
        "close_tab" => {
            let Some(&target_tab) = tab_entities.get(evt.tab_index) else {
                return;
            };

            if tab_entities.len() > 1 {
                let is_active = active_tab_in_pane(target_pane, &pane_children, &tab_ts) == Some(target_tab);
                commands.entity(target_tab).despawn();
                if is_active {
                    let next = tab_entities.iter().copied().find(|&e| e != target_tab).unwrap();
                    commands.entity(next).insert(LastActivatedAt::now());
                }
            } else if leaf_panes.iter().count() <= 1 {
                let startup_url = settings.browser.startup_url.as_str();
                commands.entity(target_tab).despawn();
                let tab = commands
                    .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(target_pane)))
                    .id();
                commands.spawn((
                    Browser::new(&mut meshes, &mut webview_mt, startup_url),
                    ChildOf(tab),
                ));
            } else {
                let Ok(pane_co) = child_of_q.get(target_pane) else {
                    return;
                };
                let parent = pane_co.get();
                let Ok(parent_children) = pane_children.get(parent) else {
                    return;
                };
                let is_pane =
                    |e: Entity| leaf_panes.contains(e) || split_q.contains(e);
                let Some(sibling) = parent_children
                    .iter()
                    .find(|&e| e != target_pane && is_pane(e))
                else {
                    return;
                };

                let (_, current_active_pane, _) = focused_tab(
                    &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
                );
                let target_pane_is_active = current_active_pane == Some(target_pane);
                let sibling_is_active = current_active_pane == Some(sibling);

                let sibling_children: Vec<Entity> = pane_children
                    .get(sibling)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                for &child in &sibling_children {
                    commands.entity(child).insert(ChildOf(parent));
                }

                let (new_active_pane, tab_to_activate);
                if split_q.contains(sibling) {
                    new_active_pane =
                        first_leaf_descendant(sibling, &pane_children, &leaf_panes);
                    tab_to_activate = active_tab_in_pane(
                        new_active_pane,
                        &pane_children,
                        &tab_ts,
                    )
                    .or_else(|| {
                        first_tab_in_pane(new_active_pane, &pane_children, &tab_q)
                    });
                    commands.entity(sibling).remove::<ChildOf>();
                    commands.queue(move |world: &mut World| {
                        world.despawn(sibling);
                    });
                } else {
                    new_active_pane = parent;
                    tab_to_activate = sibling_children
                        .iter()
                        .copied()
                        .find(|&e| {
                            tab_ts.get(e).is_ok()
                        })
                        .or_else(|| {
                            sibling_children
                                .iter()
                                .copied()
                                .find(|&e| tab_q.contains(e))
                        });
                    commands.entity(parent).remove::<PaneSplit>();
                    commands.entity(parent).insert(Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        align_items: AlignItems::Stretch,
                        justify_content: JustifyContent::Stretch,
                        ..default()
                    });
                    commands.entity(sibling).despawn();
                }

                commands.entity(target_pane).despawn();

                if target_pane_is_active || sibling_is_active {
                    commands.entity(new_active_pane).insert(LastActivatedAt::now());
                    if let Some(tab) = tab_to_activate {
                        commands.entity(tab).insert(LastActivatedAt::now());
                    }
                }
            }

            hover_intent.target = None;
            hover_intent.last_activation = Some(std::time::Instant::now());
        }
        _ => {}
    }
}

fn spawn_visit_on_navigation(
    changed_tabs: Query<(Entity, &PageMetadata), (With<Tab>, Changed<PageMetadata>)>,
    mut last_urls: Local<std::collections::HashMap<u64, String>>,
    mut commands: Commands,
) {
    for (entity, meta) in &changed_tabs {
        if meta.url.is_empty() || meta.url == "about:blank" {
            continue;
        }

        let key = entity.to_bits();
        let is_new = last_urls
            .get(&key)
            .map(|prev| prev != &meta.url)
            .unwrap_or(true);

        if is_new {
            last_urls.insert(key, meta.url.clone());
            commands.spawn((
                Visit,
                meta.clone(),
                CreatedAt::now(),
            ));
        }
    }
}

fn sync_page_metadata_to_tab(
    browser_q: Query<(&PageMetadata, &ChildOf), (With<Browser>, Changed<PageMetadata>)>,
    tab_q: Query<(), With<Tab>>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut commands: Commands,
) {
    for (meta, child_of) in &browser_q {
        let parent = child_of.get();
        if !tab_q.contains(parent) || status_q.contains(parent) || side_sheet_q.contains(parent) {
            continue;
        }
        commands.entity(parent).insert(meta.clone());
    }
}

fn cef_root_cache_path() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library/Application Support/vmux/profiles/default")
                .to_string_lossy()
                .into_owned()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir()
            .to_str()
            .map(|p| format!("{p}/vmux_cef/profiles/default"))
    }
}
