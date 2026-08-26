use bevy::{ecs::relationship::Relationship, prelude::*, window::PrimaryWindow};
use bevy_cef::prelude::*;
use vmux_core::{PageIdentity, PageMetadata, page::PageReady};
use vmux_history::LastActivatedAt;
use vmux_layout::{Browser, Loading};
use vmux_layout::{
    Header, LayoutCef, NavigationState, Open, UpdateState,
    event::{
        HEADER_HEIGHT_PX, LAYOUT_STATE_EVENT, LayoutStateEvent, PANE_TREE_EVENT, PaneNode,
        PaneTreeEvent, STACKS_EVENT, StackNode, StackRow, StacksHostEvent, TAB_BOUNDARY_EVENT,
        TABS_EVENT, TabBoundary, TabBoundaryEvent, TabRow, TabsHostEvent, UPDATE_CLEARED_EVENT,
        UPDATE_PROGRESS_EVENT, UPDATE_READY_EVENT, UpdateClearedEvent, UpdateProgressEvent,
        UpdateReadyEvent,
    },
    pane::{Pane, PaneSplit, SideSheetCardCollapsed},
    side_sheet::{SideSheet, SideSheetPosition, SideSheetSectionsExpanded, SideSheetWidth},
    stack::{Stack, active_stack_in_pane, collect_leaf_panes},
    tab::{Tab, TabWorktree},
    window::VmuxWindow,
};

use vmux_setting::AppSettings;

use crate::{
    LayoutFixedOffsets, abbreviate_home, active_stack_in_tab, first_browser_meta,
    layout_window_padding_from_node, layout_window_padding_from_settings,
    should_emit_cached_payload, should_emit_update, tab_boundary_dir, tab_of,
};
use vmux_flex::prelude::*;

pub(crate) struct PageStatePlugin;

impl Plugin for PageStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                push_layout_state_emit,
                push_stacks_host_emit,
                push_pane_tree_emit,
                push_tabs_host_emit,
                push_bookmarks_host_emit,
                push_update_notice_emit,
                push_tab_boundary_emit,
            )
                .after(vmux_layout::apply_cef_state_from_webview)
                .after(vmux_layout::stack::ComputeFocusSet),
        );
    }
}

fn push_layout_state_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    header_q: Query<(Has<Open>, Option<&ComputedNode>), With<Header>>,
    side_sheet_q: Query<(&SideSheetPosition, Has<Open>), With<SideSheet>>,
    window_q: Query<&Node, With<VmuxWindow>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    side_sheet_width: Res<SideSheetWidth>,
    settings: Res<AppSettings>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }
    let window_padding = window_q
        .single()
        .ok()
        .map(layout_window_padding_from_node)
        .unwrap_or_else(|| layout_window_padding_from_settings(&settings));
    let header_open = header_q.iter().any(|(is_open, _)| is_open);
    let window_width_px = windows
        .single()
        .ok()
        .map(|window| window.resolution.physical_width() as f32)
        .unwrap_or(0.0);
    let header_offsets = header_q
        .iter()
        .find_map(|(_, computed)| LayoutFixedOffsets::of(computed?, window_width_px));

    let payload = LayoutStateEvent {
        header_open,
        side_sheet_open: side_sheet_q
            .iter()
            .any(|(pos, is_open)| *pos == SideSheetPosition::Left && is_open),
        header_height: header_offsets
            .map(|offsets| offsets.height)
            .unwrap_or(HEADER_HEIGHT_PX),
        side_sheet_width: side_sheet_width.0,
        pane_gap: vmux_layout::event::PANE_GAP_PX,
        radius: settings.layout.radius,
        header_left: header_offsets.map(|offsets| offsets.left),
        header_top: header_offsets.map(|offsets| offsets.top),
        header_right: header_offsets.map(|offsets| offsets.right),
        window_pad_top: window_padding.top,
        window_pad_right: window_padding.right,
        window_pad_bottom: window_padding.bottom,
        window_pad_left: window_padding.left,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        LAYOUT_STATE_EVENT,
        &payload,
    ));
    *last = body;
}

struct AddressRoots<'a> {
    repos: Option<&'a mut vmux_git::RepoInfoCache>,
    home: std::path::PathBuf,
}

impl AddressRoots<'_> {
    fn of(&mut self, url: &str, title: &str) -> vmux_layout::event::AddressParts {
        let Some(path) = vmux_core::file_url::FileUrl::parse(url).and_then(|url| url.path()) else {
            return match url.starts_with("vmux://") {
                true => vmux_layout::event::AddressParts::internal(url),
                false => vmux_layout::event::AddressParts::web(url, title),
            };
        };
        if let Some(info) = self.checkout_of(&path) {
            return vmux_layout::event::AddressParts::in_repo(
                &path,
                &info.repo_root,
                &info.name,
                &info.branch,
            );
        }
        vmux_layout::event::AddressParts::on_disk(&path, &self.home)
    }

    fn checkout_of(&mut self, path: &std::path::Path) -> Option<vmux_git::worktree::RepoInfo> {
        let dir = match path.is_dir() {
            true => path,
            false => path.parent()?,
        };
        self.repos.as_mut()?.get(dir)
    }
}

#[allow(clippy::too_many_arguments)]
fn push_stacks_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    browser_q: Query<
        (
            &PageMetadata,
            &ChildOf,
            Option<&NavigationState>,
            Option<&PageIdentity>,
        ),
        With<Browser>,
    >,
    stack_q: Query<(), With<Stack>>,
    zoomed_q: Query<(), With<vmux_layout::pane::Zoomed>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    child_of_q: Query<&ChildOf>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }
    let active_pane = focus.pane;
    let active_stack_opt = focus.stack;
    if let Some(active_stack_entity) = active_stack_opt
        && !stack_q.contains(active_stack_entity)
    {
        return;
    }
    let mut rows: Vec<StackRow> = Vec::new();
    let mut can_go_back = false;
    let mut can_go_forward = false;
    let mut roots = AddressRoots {
        repos: repo_info
            .as_mut()
            .map(|cache| cache.bypass_change_detection()),
        home: std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default(),
    };
    if let Some(active_stack_entity) = active_stack_opt {
        for (meta, child_of, nav_state, osc) in &browser_q {
            let stack_entity = child_of.get();
            let stack_pane = child_of_q.get(stack_entity).ok().map(|co| co.get());
            if stack_pane != active_pane {
                continue;
            }
            let is_active = stack_entity == active_stack_entity;
            if is_active && let Some(ns) = nav_state {
                can_go_back = ns.can_go_back;
                can_go_forward = ns.can_go_forward;
            }
            let title = meta.title_with(osc).to_string();
            rows.push(StackRow {
                address: roots.of(&meta.url, &title),
                title,
                url: meta.url.clone(),
                icon: meta.icon.clone(),
                is_active,
                bg_color: meta.bg_color.clone(),
            });
        }
    }
    if active_stack_opt.is_some() && rows.is_empty() {
        return;
    }
    let is_zoomed = focus.tab.map(|t| zoomed_q.get(t).is_ok()).unwrap_or(false);
    let payload = StacksHostEvent {
        stacks: rows,
        can_go_back,
        can_go_forward,
        is_zoomed,
    };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, STACKS_EVENT, &payload));
    *last = ron_body;
}

fn push_pane_tree_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    tab_q: Query<(), With<Tab>>,
    tab_sections: Query<&SideSheetSectionsExpanded, With<Tab>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    collapsed_panes: Query<(), With<SideSheetCardCollapsed>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: Query<Entity, With<Stack>>,
    stack_children: Query<&Children>,
    browser_meta: Query<(&PageMetadata, Has<Loading>, Option<&PageIdentity>), With<Browser>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }

    let active_pane = focus.pane;

    let Some(tab_e) = focus.tab else {
        return;
    };
    if !tab_q.contains(tab_e) {
        return;
    }
    let sections = tab_sections.get(tab_e).copied().unwrap_or_default();
    let mut tab_leaf_panes = Vec::new();
    collect_leaf_panes(tab_e, &all_children, &leaf_pane_q, &mut tab_leaf_panes);

    let mut panes: Vec<PaneNode> = Vec::new();
    for &pane_entity in &tab_leaf_panes {
        let is_active = active_pane == Some(pane_entity);
        let active_stack = active_stack_in_pane(pane_entity, &pane_children, &stack_ts);
        let mut stacks: Vec<StackNode> = Vec::new();
        let mut stack_index: usize = 0;
        if let Ok(children) = pane_children.get(pane_entity) {
            for child in children.iter() {
                if !stack_q.contains(child) {
                    continue;
                }
                let stack_is_active = active_stack == Some(child);
                let mut found_browser = false;
                if let Ok(stack_kids) = stack_children.get(child) {
                    for browser_e in stack_kids.iter() {
                        if let Ok((meta, loading, osc)) = browser_meta.get(browser_e) {
                            let is_new_stack = false;
                            stacks.push(StackNode {
                                title: if is_new_stack {
                                    "New Stack".to_string()
                                } else {
                                    meta.title_with(osc).to_string()
                                },
                                url: if is_new_stack {
                                    String::new()
                                } else {
                                    meta.url.clone()
                                },
                                icon: if is_new_stack {
                                    vmux_core::PageIcon::None
                                } else {
                                    meta.icon.clone()
                                },
                                is_active: stack_is_active,
                                stack_index: stack_index as u32,
                                is_loading: loading,
                                bg_color: meta.bg_color.clone(),
                            });
                            found_browser = true;
                        }
                    }
                }
                if !found_browser {
                    stacks.push(StackNode {
                        title: "New Stack".to_string(),
                        url: String::new(),
                        icon: vmux_core::PageIcon::None,
                        is_active: stack_is_active,
                        stack_index: stack_index as u32,
                        is_loading: false,
                        bg_color: None,
                    });
                }
                stack_index += 1;
            }
        }
        panes.push(PaneNode {
            id: pane_entity.to_bits(),
            is_active,
            collapsed: collapsed_panes.contains(pane_entity),
            projects_expanded: sections.projects,
            bookmarks_expanded: sections.bookmarks,
            knowledge_expanded: sections.knowledge,
            tools_expanded: sections.tools,
            stacks,
        });
    }
    let payload = PaneTreeEvent { panes };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        PANE_TREE_EVENT,
        &payload,
    ));
    *last = ron_body;
}

#[allow(clippy::too_many_arguments)]
fn push_tab_boundary_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    tabs: Query<&Tab>,
    worktrees: Query<&TabWorktree>,
    settings: Res<AppSettings>,
    active_space: Option<Res<vmux_space::spaces::ActiveSpace>>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut last: Local<String>,
    mut repo_info: Option<ResMut<vmux_git::RepoInfoCache>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }
    let boundary = focus.tab.and_then(|tab_e| {
        let tab = tabs.get(tab_e).ok()?;
        let (path, source) = tab_boundary_dir(tab, &settings, active_space.as_deref())?;
        let info = repo_info
            .as_mut()
            .and_then(|cache| cache.bypass_change_detection().get(&path));
        let wt = worktrees.get(tab_e).ok();
        let branch = info.as_ref().map(|i| i.branch.clone()).unwrap_or_default();
        let base_ref = wt.map(|w| w.base_ref.clone()).unwrap_or_default();
        let mut leaves = Vec::new();
        collect_leaf_panes(tab_e, &all_children, &leaf_pane_q, &mut leaves);
        Some(TabBoundary {
            effective_dir: abbreviate_home(&path),
            source: match source {
                vmux_setting::DirSource::Tab => "tab",
                vmux_setting::DirSource::Space => "space",
                vmux_setting::DirSource::Global => "global",
            }
            .to_string(),
            is_git_repo: info.is_some(),
            is_worktree: info.as_ref().is_some_and(|i| i.is_worktree),
            branch,
            base_ref,
            uncommitted: info.as_ref().map(|i| i.uncommitted).unwrap_or(0),
            ahead: info.as_ref().map(|i| i.ahead).unwrap_or(0),
            pane_count: leaves.len() as u32,
        })
    });
    let mut projects = active_space
        .as_deref()
        .and_then(|space| settings.space(&space.record.id))
        .map(vmux_setting::SpaceOverrides::project_rows)
        .unwrap_or_default();
    if let Some(cache) = repo_info.as_mut() {
        let cache = cache.bypass_change_detection();
        for row in &mut projects {
            if row.missing {
                continue;
            }
            if let Some(info) = cache.get(std::path::Path::new(&row.path)) {
                row.branch = info.branch.clone();
            }
        }
    }
    for row in &mut projects {
        row.display_path = abbreviate_home(std::path::Path::new(&row.path));
    }
    let payload = TabBoundaryEvent { boundary, projects };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if !should_emit_cached_payload(&ron_body, &last, page_ready.is_changed()) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        TAB_BOUNDARY_EVENT,
        &payload,
    ));
    *last = ron_body;
}

#[allow(clippy::too_many_arguments)]
fn push_bookmarks_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    pins: Query<
        (
            &vmux_core::Uuid,
            &PageMetadata,
            &vmux_core::BookmarkOrder,
            Has<vmux_core::Bookmark>,
        ),
        With<vmux_core::Pin>,
    >,
    folders: Query<
        (
            Entity,
            &vmux_core::Uuid,
            &Name,
            Option<&Children>,
            Has<vmux_core::Collapsed>,
            &vmux_core::BookmarkOrder,
            Option<&ChildOf>,
        ),
        With<vmux_core::Folder>,
    >,
    top_bookmarks: Query<
        (
            &vmux_core::Uuid,
            &PageMetadata,
            &vmux_core::BookmarkOrder,
            Has<vmux_core::Pin>,
        ),
        (With<vmux_core::Bookmark>, Without<ChildOf>),
    >,
    child_bookmarks: Query<
        (
            &vmux_core::Uuid,
            &PageMetadata,
            &vmux_core::BookmarkOrder,
            Has<vmux_core::Pin>,
        ),
        With<vmux_core::Bookmark>,
    >,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }

    let row = |uuid: &vmux_core::Uuid, meta: &PageMetadata, bookmarked: bool, pinned: bool| {
        vmux_layout::event::BookmarkRow {
            uuid: uuid.0.clone(),
            metadata: meta.clone(),
            bookmarked,
            pinned,
        }
    };

    let mut pin_entries: Vec<(u32, vmux_layout::event::BookmarkRow)> = pins
        .iter()
        .map(|(u, m, o, bookmarked)| (o.0, row(u, m, bookmarked, true)))
        .collect();
    pin_entries.sort_by_key(|(order, _)| *order);
    let pin_rows: Vec<vmux_layout::event::BookmarkRow> =
        pin_entries.into_iter().map(|(_, r)| r).collect();

    let mut roots: Vec<(u32, vmux_layout::event::BookmarkNode)> = Vec::new();
    for (_, uuid, name, children, collapsed, order, parent) in folders.iter() {
        let mut kids = Vec::new();
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok((uuid, meta, order, pinned)) = child_bookmarks.get(child) {
                    kids.push((order.0, row(uuid, meta, true, pinned)));
                }
            }
        }
        kids.sort_by_key(|(order, _)| *order);
        let parent = parent.and_then(|parent| {
            folders
                .get(parent.get())
                .ok()
                .map(|(_, uuid, _, _, _, _, _)| uuid.0.clone())
        });
        roots.push((
            order.0,
            vmux_layout::event::BookmarkNode::Folder(vmux_layout::event::FolderRow {
                uuid: uuid.0.clone(),
                name: name.as_str().to_string(),
                collapsed,
                parent,
                children: kids.into_iter().map(|(_, row)| row).collect(),
            }),
        ));
    }
    for (uuid, meta, order, pinned) in top_bookmarks.iter() {
        roots.push((
            order.0,
            vmux_layout::event::BookmarkNode::Entry(row(uuid, meta, true, pinned)),
        ));
    }
    roots.sort_by_key(|(o, _)| *o);
    let roots: Vec<vmux_layout::event::BookmarkNode> = roots.into_iter().map(|(_, n)| n).collect();

    let payload = vmux_layout::event::BookmarksHostEvent {
        pins: pin_rows,
        roots,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !page_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        vmux_layout::event::BOOKMARKS_EVENT,
        &payload,
    ));
    *last = body;
}

fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    tabs: Query<(Entity, &Tab, &LastActivatedAt)>,
    tab_q: Query<Entity, With<Tab>>,
    active_tab_param: vmux_layout::stack::ActiveTabParam,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_children: Query<&Children>,
    browser_meta: Query<(&PageMetadata, Option<&PageIdentity>), With<Browser>>,
    done_agents: Query<Entity, With<vmux_core::notify::AgentDoneUnseen>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }

    let active_tab = active_tab_param.get();

    let done_tabs: std::collections::HashSet<Entity> = done_agents
        .iter()
        .filter_map(|agent| tab_of(agent, &child_of_q, &tab_q))
        .collect();

    let ordered = if let Some(anchor) = active_tab {
        vmux_layout::tab::active_tab_siblings(anchor, &child_of_q, &all_children, &tab_q)
    } else {
        Vec::new()
    };

    let rows: Vec<TabRow> = ordered
        .iter()
        .filter_map(|e| tabs.get(*e).ok())
        .map(|(entity, tab, _)| {
            let active_stack = active_stack_in_tab(
                entity,
                &all_children,
                &leaf_pane_q,
                &pane_children,
                &stack_ts,
            );
            let found =
                active_stack.and_then(|s| first_browser_meta(s, &stack_children, &browser_meta));
            let title = found
                .map(|(meta, osc)| meta.title_with(osc).to_string())
                .unwrap_or_default();
            let (url, icon, bg_color) = found
                .map(|(meta, _)| (meta.url.clone(), meta.icon.clone(), meta.bg_color.clone()))
                .unwrap_or_default();
            let name = if tab.name.is_empty() {
                "Tab".to_string()
            } else {
                tab.name.clone()
            };
            TabRow {
                id: entity.to_bits().to_string(),
                name,
                is_active: Some(entity) == active_tab,
                bg_color,
                title,
                url,
                icon,
                is_done_unseen: done_tabs.contains(&entity),
            }
        })
        .collect();

    let payload = TabsHostEvent { tabs: rows };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !page_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, TABS_EVENT, &payload));
    *last = body;
}

fn push_update_notice_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    state: Res<UpdateState>,
    mut last: Local<Option<UpdateState>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.can_emit_to(&cef_e) {
        return;
    }
    if !should_emit_update(&state, &last, page_ready.is_changed()) {
        return;
    }
    match &*state {
        UpdateState::Idle => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_CLEARED_EVENT,
            &UpdateClearedEvent,
        )),
        UpdateState::Downloading {
            version,
            downloaded,
            total,
        } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_PROGRESS_EVENT,
            &UpdateProgressEvent {
                version: version.clone(),
                downloaded: *downloaded,
                total: *total,
                installing: false,
            },
        )),
        UpdateState::Installing { version } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_PROGRESS_EVENT,
            &UpdateProgressEvent {
                version: version.clone(),
                downloaded: 0,
                total: 0,
                installing: true,
            },
        )),
        UpdateState::Ready { version } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_READY_EVENT,
            &UpdateReadyEvent {
                version: version.clone(),
            },
        )),
    }
    *last = Some(state.clone());
}
