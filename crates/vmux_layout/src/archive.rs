use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, StackCommand};
use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
use vmux_core::terminal::{TerminalLaunch, TerminalSpawnRequest};
use vmux_core::{
    ArchivedPage, ArchivedPagePosition, PageArchiveRequest, PageMetadata, PageOpenRequest,
    PageOpenTarget, PaneStep, SplitAxis, now_millis,
};

use crate::event::TERMINAL_PAGE_URL;
use crate::pane::{
    Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection, leaf_pane_bundle, split_root_bundle,
};
use crate::settings::LayoutSettings;
use crate::space::{ActiveSpaceEntity, Space, SpaceId, space_of};
use crate::stack::{FocusedStack, Stack, StackCommandSet, stack_bundle};
use crate::tab::Tab;
use crate::window::spawn_tab_scaffold_in_space;

const MAX_ARCHIVE_ENTRIES: usize = 25;
const ARCHIVE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub struct ArchivePlugin;

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, (capture_archived_pages, maintain_archive))
            .add_systems(
                Update,
                (
                    archive_on_stack_close.before(StackCommandSet),
                    handle_reopen_closed_page,
                )
                    .in_set(ReadAppCommands),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn archive_on_stack_close(
    mut reader: MessageReader<AppCommand>,
    focused: Res<FocusedStack>,
    stack_pages: Query<(&PageMetadata, Option<&TerminalLaunch>), With<Stack>>,
    child_of: Query<&ChildOf>,
    children_q: Query<&Children>,
    spaces: Query<(), With<Space>>,
    space_ids: Query<&SpaceId>,
    tabs: Query<(), With<Tab>>,
    stacks: Query<(), With<Stack>>,
    pane_ids: Query<&PaneId>,
    splits: Query<&PaneSplit>,
    pane_sizes: Query<&PaneSize>,
    panes: Query<(), With<Pane>>,
    mut writer: MessageWriter<PageArchiveRequest>,
) {
    let mut closing = false;
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close))
        ) {
            closing = true;
        }
    }
    if !closing {
        return;
    }
    let Some(stack) = focused.stack else {
        return;
    };
    let Ok((meta, launch)) = stack_pages.get(stack) else {
        return;
    };
    if meta.url.is_empty() {
        return;
    }
    let space = space_of(stack, &child_of, &spaces);
    let space_id = space
        .and_then(|s| space_ids.get(s).ok())
        .map(|id| id.0.clone())
        .unwrap_or_default();
    let tab_index = space.and_then(|s| tab_index_of(stack, s, &child_of, &children_q, &tabs));
    let (leaf_pane_id, stack_index, pane_path) = pane_path_of(
        stack,
        &child_of,
        &children_q,
        &pane_ids,
        &splits,
        &pane_sizes,
        &panes,
        &stacks,
        &tabs,
    )
    .unwrap_or_default();
    writer.write(PageArchiveRequest {
        url: meta.url.clone(),
        title: meta.title.clone(),
        space_id,
        launch: launch.cloned(),
        tab_index,
        leaf_pane_id,
        stack_index,
        pane_path,
    });
}

fn tab_index_of(
    stack: Entity,
    space: Entity,
    child_of: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    tabs: &Query<(), With<Tab>>,
) -> Option<usize> {
    let mut cur = stack;
    let tab = loop {
        if tabs.get(cur).is_ok() {
            break cur;
        }
        cur = child_of.get(cur).ok()?.parent();
    };
    children_q
        .get(space)
        .ok()?
        .iter()
        .filter(|e| tabs.get(*e).is_ok())
        .position(|e| e == tab)
}

#[allow(clippy::too_many_arguments)]
fn pane_path_of(
    stack: Entity,
    child_of: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    pane_ids: &Query<&PaneId>,
    splits: &Query<&PaneSplit>,
    pane_sizes: &Query<&PaneSize>,
    panes: &Query<(), With<Pane>>,
    stacks: &Query<(), With<Stack>>,
    tabs: &Query<(), With<Tab>>,
) -> Option<(String, usize, Vec<PaneStep>)> {
    let leaf = child_of.get(stack).ok()?.parent();
    if !panes.contains(leaf) {
        return None;
    }
    let leaf_pane_id = pane_ids.get(leaf).ok()?.0.clone();
    let stack_index = children_q
        .get(leaf)
        .ok()?
        .iter()
        .filter(|&e| stacks.contains(e))
        .position(|e| e == stack)?;

    let mut steps_rev: Vec<PaneStep> = Vec::new();
    let mut cur = leaf;
    loop {
        let parent = child_of.get(cur).ok()?.parent();
        if tabs.contains(parent) {
            break;
        }
        let Ok(split) = splits.get(parent) else {
            return None;
        };
        let pane_children: Vec<Entity> = children_q
            .get(parent)
            .map(|c| c.iter().filter(|&e| panes.contains(e)).collect())
            .unwrap_or_default();
        let child_index = pane_children.iter().position(|&e| e == cur)?;
        let flex_weights = pane_children
            .iter()
            .map(|&e| pane_sizes.get(e).map(|s| s.flex_grow).unwrap_or(1.0))
            .collect();
        steps_rev.push(PaneStep {
            split_id: pane_ids.get(parent).ok()?.0.clone(),
            axis: match split.direction {
                PaneSplitDirection::Row => SplitAxis::Row,
                PaneSplitDirection::Column => SplitAxis::Column,
            },
            child_index,
            flex_weights,
        });
        cur = parent;
    }
    steps_rev.reverse();
    Some((leaf_pane_id, stack_index, steps_rev))
}

fn capture_archived_pages(mut reader: MessageReader<PageArchiveRequest>, mut commands: Commands) {
    for req in reader.read() {
        if req.url.is_empty() {
            continue;
        }
        commands.spawn((
            ArchivedPage {
                url: req.url.clone(),
                title: req.title.clone(),
                space_id: req.space_id.clone(),
                closed_at: now_millis(),
                launch: req.launch.clone(),
                tab_index: req.tab_index,
            },
            ArchivedPagePosition {
                leaf_pane_id: req.leaf_pane_id.clone(),
                stack_index: req.stack_index,
                pane_path: req.pane_path.clone(),
            },
        ));
    }
}

fn maintain_archive(archived: Query<(Entity, &ArchivedPage)>, mut commands: Commands) {
    let now = now_millis();
    let mut live: Vec<(Entity, i64)> = Vec::new();
    for (entity, page) in &archived {
        if now - page.closed_at > ARCHIVE_TTL_MS {
            commands.entity(entity).despawn();
        } else {
            live.push((entity, page.closed_at));
        }
    }
    if live.len() > MAX_ARCHIVE_ENTRIES {
        live.sort_by_key(|(_, closed_at)| *closed_at);
        let overflow = live.len() - MAX_ARCHIVE_ENTRIES;
        for (entity, _) in live.into_iter().take(overflow) {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(SystemParam)]
struct ReopenLayout<'w, 's> {
    pane_ids: Query<'w, 's, (Entity, &'static PaneId)>,
    leaf_panes: Query<'w, 's, (), (With<Pane>, Without<PaneSplit>)>,
    child_of: Query<'w, 's, &'static ChildOf>,
    children_q: Query<'w, 's, &'static Children>,
    stacks_q: Query<'w, 's, (), With<Stack>>,
    tabs: Query<'w, 's, (), With<Tab>>,
}

#[allow(clippy::too_many_arguments)]
fn handle_reopen_closed_page(
    mut reader: MessageReader<AppCommand>,
    archived: Query<(Entity, &ArchivedPage)>,
    positions: Query<&ArchivedPagePosition>,
    spaces: Query<(Entity, &SpaceId), With<Space>>,
    any_space: Query<Entity, With<Space>>,
    layout: ReopenLayout,
    active_space: Res<ActiveSpaceEntity>,
    settings: Res<LayoutSettings>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut terminal_spawn: MessageWriter<TerminalSpawnRequest>,
    mut commands: Commands,
) {
    let mut reopen = false;
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Reopen))
        ) {
            reopen = true;
        }
    }
    if !reopen {
        return;
    }

    let Some((entry_entity, page)) = archived
        .iter()
        .max_by_key(|(_, p)| p.closed_at)
        .map(|(e, p)| (e, p.clone()))
    else {
        return;
    };

    let origin_space = spaces
        .iter()
        .find(|(_, id)| id.0 == page.space_id)
        .map(|(e, _)| e);
    let target_space = origin_space
        .or_else(|| active_space.0.filter(|e| any_space.get(*e).is_ok()))
        .or_else(|| any_space.iter().next());
    let Some(space) = target_space else {
        return;
    };

    let position = positions.get(entry_entity).ok().cloned();
    let (stack, focus_anchor) = resolve_reopen_stack(
        space,
        origin_space == Some(space),
        page.tab_index,
        position.as_ref(),
        &layout,
        &mut commands,
        *primary_window,
        settings.pane.gap,
    );
    commands.entity(stack).insert(PageMetadata {
        url: page.url.clone(),
        title: page.title.clone(),
        ..default()
    });
    commands
        .entity(space)
        .insert(vmux_history::LastActivatedAt::now());
    commands
        .entity(stack)
        .insert(vmux_history::LastActivatedAt::now());
    focus_reopened_ancestors(focus_anchor, &layout, &mut commands);

    if let Some(kind) = AgentKind::all()
        .into_iter()
        .find(|k| page.url.starts_with(&k.cli_url_prefix()))
    {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| PathBuf::from(&l.cwd))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let id_part = page.url.strip_prefix(&kind.cli_url_prefix()).unwrap_or("");
        let session_id = (!id_part.is_empty()).then(|| id_part.to_string());
        spawn_agent.write(SpawnAgentInStackRequest {
            kind,
            cwd,
            session_id,
            stack,
            initial_prompt: None,
        });
    } else if page.url.starts_with(TERMINAL_PAGE_URL) {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| l.cwd.clone())
            .filter(|c| !c.is_empty())
            .map(PathBuf::from);
        terminal_spawn.write(TerminalSpawnRequest {
            cwd,
            target_stack: Some(stack),
        });
    } else {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: page.url.clone(),
            request_id: None,
        });
    }

    commands.entity(entry_entity).despawn();
}

#[allow(clippy::too_many_arguments)]
fn resolve_reopen_stack(
    space: Entity,
    origin_matches: bool,
    tab_index: Option<usize>,
    position: Option<&ArchivedPagePosition>,
    layout: &ReopenLayout,
    commands: &mut Commands,
    primary_window: Entity,
    gap: f32,
) -> (Entity, Entity) {
    if let Some(pos) = position.filter(|p| !p.leaf_pane_id.is_empty()) {
        if let Some(leaf) = layout
            .pane_ids
            .iter()
            .find(|(e, id)| id.0 == pos.leaf_pane_id && layout.leaf_panes.contains(*e))
            .map(|(e, _)| e)
            .filter(|&leaf| pane_in_space(leaf, space, &layout.child_of))
        {
            return (
                spawn_stack_in_leaf(leaf, pos.stack_index, layout, commands),
                leaf,
            );
        }
        if let Some((leaf, anchor)) = reattach_along_path(space, pos, layout, commands) {
            return (
                spawn_stack_in_leaf(leaf, pos.stack_index, layout, commands),
                anchor,
            );
        }
    }

    let scaffold = spawn_tab_scaffold_in_space(commands, space, primary_window, gap);
    if origin_matches && let Some(idx) = tab_index {
        commands.entity(space).insert_children(idx, &[scaffold.tab]);
    }
    (scaffold.stack, scaffold.tab)
}

fn pane_in_space(pane: Entity, space: Entity, child_of: &Query<&ChildOf>) -> bool {
    let mut cur = pane;
    while let Ok(rel) = child_of.get(cur) {
        let parent = rel.parent();
        if parent == space {
            return true;
        }
        cur = parent;
    }
    false
}

fn spawn_stack_in_leaf(
    leaf: Entity,
    stack_index: usize,
    layout: &ReopenLayout,
    commands: &mut Commands,
) -> Entity {
    let stack = commands
        .spawn((
            stack_bundle(),
            vmux_history::LastActivatedAt::now(),
            ChildOf(leaf),
        ))
        .id();
    let stack_count = layout
        .children_q
        .get(leaf)
        .map(|c| c.iter().filter(|&e| layout.stacks_q.contains(e)).count())
        .unwrap_or(0);
    let idx = stack_index.min(stack_count);
    commands.entity(leaf).insert_children(idx, &[stack]);
    stack
}

fn focus_reopened_ancestors(anchor: Entity, layout: &ReopenLayout, commands: &mut Commands) {
    commands
        .entity(anchor)
        .insert(vmux_history::LastActivatedAt::now());
    let mut cur = anchor;
    while let Ok(rel) = layout.child_of.get(cur) {
        let parent = rel.parent();
        commands
            .entity(parent)
            .insert(vmux_history::LastActivatedAt::now());
        if layout.tabs.contains(parent) {
            break;
        }
        cur = parent;
    }
}

fn reattach_along_path(
    space: Entity,
    pos: &ArchivedPagePosition,
    layout: &ReopenLayout,
    commands: &mut Commands,
) -> Option<(Entity, Entity)> {
    let path = &pos.pane_path;
    let root_step = path.first()?;
    let root = layout
        .pane_ids
        .iter()
        .find(|(_, id)| id.0 == root_step.split_id)
        .map(|(e, _)| e)?;
    if !pane_in_space(root, space, &layout.child_of) {
        return None;
    }

    let node_id = |i: usize| -> String {
        if i + 1 < path.len() {
            path[i + 1].split_id.clone()
        } else {
            pos.leaf_pane_id.clone()
        }
    };
    let find_child_by_id = |parent: Entity, id: &str| -> Option<Entity> {
        layout.children_q.get(parent).ok()?.iter().find(|&child| {
            layout
                .pane_ids
                .iter()
                .any(|(e, pid)| e == child && pid.0 == id)
        })
    };

    let mut parent = root;
    let mut depth = 0usize;
    while depth < path.len() {
        match find_child_by_id(parent, &node_id(depth)) {
            Some(child) => {
                parent = child;
                depth += 1;
            }
            None => break,
        }
    }
    let anchor = parent;
    if depth == path.len() {
        return Some((parent, anchor));
    }

    if layout.leaf_panes.contains(parent) {
        promote_leaf_to_split(parent, path[depth].axis, layout, commands);
    }

    for level in depth..path.len() {
        let step = &path[level];
        let is_last = level + 1 == path.len();
        let child_id = node_id(level);
        let flex = step
            .flex_weights
            .get(step.child_index)
            .copied()
            .unwrap_or(1.0);
        let new_child = if is_last {
            commands
                .spawn((
                    leaf_pane_bundle(),
                    PaneId(child_id),
                    vmux_history::LastActivatedAt::now(),
                    ChildOf(parent),
                ))
                .id()
        } else {
            let axis = match path[level + 1].axis {
                SplitAxis::Row => PaneSplitDirection::Row,
                SplitAxis::Column => PaneSplitDirection::Column,
            };
            commands
                .spawn((
                    split_root_bundle(axis),
                    PaneId(child_id),
                    vmux_history::LastActivatedAt::now(),
                    ChildOf(parent),
                ))
                .id()
        };
        commands
            .entity(new_child)
            .insert(PaneSize { flex_grow: flex });
        let insert_at = clamp_child_index(parent, step.child_index, &layout.children_q);
        commands
            .entity(parent)
            .insert_children(insert_at, &[new_child]);
        parent = new_child;
    }
    Some((parent, anchor))
}

fn clamp_child_index(parent: Entity, idx: usize, children_q: &Query<&Children>) -> usize {
    let count = children_q
        .get(parent)
        .map(|c| c.iter().count())
        .unwrap_or(0);
    idx.min(count)
}

fn promote_leaf_to_split(
    parent: Entity,
    axis: SplitAxis,
    layout: &ReopenLayout,
    commands: &mut Commands,
) {
    let direction = match axis {
        SplitAxis::Row => PaneSplitDirection::Row,
        SplitAxis::Column => PaneSplitDirection::Column,
    };
    let stacks: Vec<Entity> = layout
        .children_q
        .get(parent)
        .map(|c| c.iter().filter(|&e| layout.stacks_q.contains(e)).collect())
        .unwrap_or_default();
    let survivor = commands
        .spawn((
            leaf_pane_bundle(),
            vmux_history::LastActivatedAt::now(),
            ChildOf(parent),
        ))
        .id();
    for s in stacks {
        commands.entity(s).insert(ChildOf(survivor));
    }
    commands.entity(parent).insert(split_root_bundle(direction));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::relationship::Relationship;
    use vmux_core::terminal::TerminalKind;

    fn page(url: &str, closed_at: i64) -> ArchivedPage {
        ArchivedPage {
            url: url.to_string(),
            title: String::new(),
            space_id: "s".to_string(),
            closed_at,
            launch: None,
            tab_index: None,
        }
    }

    #[test]
    fn capture_spawns_archived_page() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: "https://a.example".to_string(),
                title: "A".to_string(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: None,
                leaf_pane_id: String::new(),
                stack_index: 0,
                pane_path: Vec::new(),
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let all: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://a.example");
    }

    #[test]
    fn capture_skips_empty_url() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: String::new(),
                title: String::new(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: None,
                leaf_pane_id: String::new(),
                stack_index: 0,
                pane_path: Vec::new(),
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn capture_spawns_position_component() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: "https://a.example".to_string(),
                title: "A".to_string(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: Some(0),
                leaf_pane_id: "leaf-1".to_string(),
                stack_index: 2,
                pane_path: vec![vmux_core::PaneStep {
                    split_id: "root".to_string(),
                    axis: vmux_core::SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 2.0],
                }],
            });
        app.update();
        let mut q = app
            .world_mut()
            .query::<(&ArchivedPage, &ArchivedPagePosition)>();
        let (page, pos) = q.single(app.world()).expect("archived page + position");
        assert_eq!(page.url, "https://a.example");
        assert_eq!(pos.leaf_pane_id, "leaf-1");
        assert_eq!(pos.stack_index, 2);
        assert_eq!(pos.pane_path.len(), 1);
        assert_eq!(pos.pane_path[0].child_index, 1);
    }

    #[test]
    fn maintain_enforces_cap_dropping_oldest() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        for i in 0..(MAX_ARCHIVE_ENTRIES as i64 + 1) {
            app.world_mut().spawn(page(&format!("u{i}"), now - i));
        }
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls.len(), MAX_ARCHIVE_ENTRIES);
        let oldest = format!("u{}", MAX_ARCHIVE_ENTRIES);
        assert!(!urls.contains(&oldest));
    }

    #[test]
    fn maintain_purges_expired() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        app.world_mut().spawn(page("fresh", now));
        app.world_mut()
            .spawn(page("stale", now - ARCHIVE_TTL_MS - 1));
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls, vec!["fresh".to_string()]);
    }

    fn drain_archive_reqs(app: &mut App) -> Vec<PageArchiveRequest> {
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn close_command_archives_focused_stack() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    title: "Gone".to_string(),
                    ..default()
                },
                ChildOf(space),
            ))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        let reqs = drain_archive_reqs(&mut app);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://gone.example");
        assert_eq!(reqs[0].space_id, "s1");
    }

    #[test]
    fn close_command_skips_empty_url_stack() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let stack = app
            .world_mut()
            .spawn((Stack::default(), PageMetadata::default()))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        assert!(drain_archive_reqs(&mut app).is_empty());
    }

    #[test]
    fn close_records_tab_index_of_closing_stack() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        app.world_mut().spawn((Tab::default(), ChildOf(space)));
        let tab1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let pane = app.world_mut().spawn(ChildOf(tab1)).id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    ..default()
                },
                ChildOf(pane),
            ))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        let reqs = drain_archive_reqs(&mut app);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].tab_index, Some(1));
    }

    #[test]
    fn close_records_pane_path_and_leaf() {
        use crate::pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection};
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        let leaf0 = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("leaf0".to_string()),
                PaneSize { flex_grow: 1.0 },
                ChildOf(root),
            ))
            .id();
        let leaf1 = app
            .world_mut()
            .spawn((
                Pane,
                PaneId("leaf1".to_string()),
                PaneSize { flex_grow: 3.0 },
                ChildOf(root),
            ))
            .id();
        let _ = leaf0;
        app.world_mut().spawn((Stack::default(), ChildOf(leaf1)));
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://z.example".to_string(),
                    ..default()
                },
                ChildOf(leaf1),
            ))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        let reqs = drain_archive_reqs(&mut app);
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];
        assert_eq!(req.leaf_pane_id, "leaf1");
        assert_eq!(req.stack_index, 1);
        assert_eq!(req.pane_path.len(), 1);
        assert_eq!(req.pane_path[0].split_id, "root");
        assert_eq!(req.pane_path[0].child_index, 1);
        assert_eq!(req.pane_path[0].flex_weights, vec![1.0, 3.0]);
        assert!(matches!(req.pane_path[0].axis, SplitAxis::Row));
    }

    fn reopen_app() -> App {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageOpenRequest>()
            .add_message::<SpawnAgentInStackRequest>()
            .add_message::<TerminalSpawnRequest>()
            .init_resource::<crate::space::ActiveSpaceEntity>()
            .init_resource::<crate::settings::LayoutSettings>()
            .add_systems(Update, super::handle_reopen_closed_page);
        app.world_mut()
            .spawn((bevy::window::Window::default(), bevy::window::PrimaryWindow));
        app
    }

    fn dispatch_reopen(app: &mut App) {
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Reopen,
            )));
        app.update();
    }

    fn drain_opens(app: &mut App) -> Vec<PageOpenRequest> {
        app.world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn reopen_web_opens_in_origin_space_and_consumes_entry() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: None,
        });
        dispatch_reopen(&mut app);

        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://a.example");
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(_)));
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
        let mut metas = app
            .world_mut()
            .query::<(&crate::stack::Stack, &PageMetadata)>();
        assert!(
            metas
                .iter(app.world())
                .any(|(_, m)| m.url == "https://a.example")
        );
    }

    #[test]
    fn reopen_picks_newest_first() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://old.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 1,
            launch: None,
            tab_index: None,
        });
        app.world_mut().spawn(ArchivedPage {
            url: "https://new.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 2,
            launch: None,
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://new.example");
    }

    #[test]
    fn reopen_terminal_respawns_at_cwd() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "vmux://terminal/".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "/bin/zsh".to_string(),
                args: vec![],
                cwd: "/work".to_string(),
                env: vec![],
                kind: TerminalKind::Plain,
            }),
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns: Vec<TerminalSpawnRequest> = app
            .world_mut()
            .resource_mut::<Messages<TerminalSpawnRequest>>()
            .drain()
            .collect();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].cwd, Some(PathBuf::from("/work")));
        assert!(spawns[0].target_stack.is_some());
    }

    fn drain_agent_spawns(app: &mut App) -> Vec<SpawnAgentInStackRequest> {
        app.world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn reopen_agent_starts_fresh_when_no_session_id() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: AgentKind::Claude.cli_url_prefix(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "claude".to_string(),
                args: vec![],
                cwd: "/proj".to_string(),
                env: vec![],
                kind: TerminalKind::Claude,
            }),
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns = drain_agent_spawns(&mut app);
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, AgentKind::Claude);
        assert_eq!(spawns[0].cwd, PathBuf::from("/proj"));
        assert!(spawns[0].session_id.is_none());
    }

    #[test]
    fn reopen_agent_recovers_session_id_from_url() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: format!("{}sess-123", AgentKind::Claude.cli_url_prefix()),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "claude".to_string(),
                args: vec![],
                cwd: "/proj".to_string(),
                env: vec![],
                kind: TerminalKind::Claude,
            }),
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns = drain_agent_spawns(&mut app);
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, AgentKind::Claude);
        assert_eq!(spawns[0].session_id.as_deref(), Some("sess-123"));
    }

    #[test]
    fn reopen_empty_archive_is_noop() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
    }

    #[test]
    fn reopen_falls_back_to_active_space_when_origin_gone() {
        let mut app = reopen_app();
        let active = app
            .world_mut()
            .spawn((
                crate::space::Space,
                crate::space::SpaceId("active".to_string()),
            ))
            .id();
        app.world_mut()
            .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
        app.world_mut().spawn(ArchivedPage {
            url: "https://x.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        let mut tabs = app.world_mut().query::<(&crate::tab::Tab, &ChildOf)>();
        assert!(tabs.iter(app.world()).any(|(_, co)| co.get() == active));
    }

    #[test]
    fn reopen_restores_tab_at_original_index() {
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let t0 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let t1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://z.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: Some(0),
        });
        dispatch_reopen(&mut app);

        let tabs_q = app.world().entity(space).get::<Children>().unwrap();
        let tab_order: Vec<Entity> = tabs_q.iter().collect();
        assert_eq!(tab_order.len(), 3);
        assert_ne!(tab_order[0], t0);
        assert_ne!(tab_order[0], t1);
        assert_eq!(tab_order[1], t0);
        assert_eq!(tab_order[2], t1);
    }

    #[test]
    fn reopen_appends_when_origin_space_gone() {
        let mut app = reopen_app();
        let active = app
            .world_mut()
            .spawn((Space, SpaceId("active".to_string())))
            .id();
        app.world_mut()
            .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
        let t0 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(active)))
            .id();
        let t1 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(active)))
            .id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://z.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: Some(0),
        });
        dispatch_reopen(&mut app);

        let tabs_q = app.world().entity(active).get::<Children>().unwrap();
        let tab_order: Vec<Entity> = tabs_q.iter().collect();
        assert_eq!(tab_order.len(), 3);
        assert_eq!(tab_order[0], t0);
        assert_eq!(tab_order[1], t1);
        assert_ne!(tab_order[2], t0);
        assert_ne!(tab_order[2], t1);
    }

    #[test]
    fn reopen_into_surviving_leaf_pane_at_index() {
        use crate::pane::{Pane, PaneId};
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let leaf = app
            .world_mut()
            .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
            .id();
        app.world_mut().spawn((Stack::default(), ChildOf(leaf)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z.example".to_string(),
                space_id: "s1".to_string(),
                closed_at: 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "leaf-A".to_string(),
                stack_index: 0,
                pane_path: Vec::new(),
            },
        ));
        dispatch_reopen(&mut app);

        let children = app.world().entity(leaf).get::<Children>().unwrap();
        let stacks: Vec<Entity> = children
            .iter()
            .filter(|&e| app.world().entity(e).contains::<Stack>())
            .collect();
        assert_eq!(stacks.len(), 2, "stack added into the existing leaf pane");
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://z.example");
    }

    #[test]
    fn reopen_without_position_recreates_tab() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://a.example".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        let mut tabs = app.world_mut().query::<&crate::tab::Tab>();
        assert_eq!(tabs.iter(app.world()).count(), 1, "a tab was recreated");
    }

    #[test]
    fn reopen_readds_leaf_under_surviving_split() {
        use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        app.world_mut()
            .spawn((Pane, PaneId("survivor".to_string()), ChildOf(root)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "gone-leaf".to_string(),
                stack_index: 0,
                pane_path: vec![PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                }],
            },
        ));
        dispatch_reopen(&mut app);

        let root_children = app.world().entity(root).get::<Children>().unwrap();
        let panes: Vec<Entity> = root_children
            .iter()
            .filter(|&e| app.world().entity(e).contains::<Pane>())
            .collect();
        assert_eq!(
            panes.len(),
            2,
            "reopened leaf re-added under surviving split"
        );
        let has_stack = panes.iter().any(|&p| {
            app.world()
                .entity(p)
                .get::<Children>()
                .map(|c| c.iter().any(|e| app.world().entity(e).contains::<Stack>()))
                .unwrap_or(false)
        });
        assert!(has_stack);
        assert_eq!(drain_opens(&mut app).len(), 1);
    }

    #[test]
    fn reopen_reconstructs_collapsed_split_level() {
        use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        app.world_mut()
            .spawn((Pane, PaneId("root-leaf".to_string()), ChildOf(root)));
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "deep-leaf".to_string(),
                stack_index: 0,
                pane_path: vec![
                    PaneStep {
                        split_id: "root".to_string(),
                        axis: SplitAxis::Row,
                        child_index: 1,
                        flex_weights: vec![1.0, 1.0],
                    },
                    PaneStep {
                        split_id: "nested".to_string(),
                        axis: SplitAxis::Column,
                        child_index: 0,
                        flex_weights: vec![1.0, 1.0],
                    },
                ],
            },
        ));
        dispatch_reopen(&mut app);

        let mut ids = app.world_mut().query::<&crate::pane::PaneId>();
        let recreated_nested = ids.iter(app.world()).any(|id| id.0 == "nested");
        assert!(recreated_nested, "nested split recreated by id");
        let stack_count = app.world_mut().query::<&Stack>().iter(app.world()).count();
        assert_eq!(stack_count, 1);
        assert_eq!(drain_opens(&mut app).len(), 1);
    }

    #[test]
    fn reopen_focuses_restored_stack_and_ancestors() {
        use crate::pane::{Pane, PaneId};
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let leaf = app
            .world_mut()
            .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "leaf-A".to_string(),
                stack_index: 0,
                pane_path: Vec::new(),
            },
        ));
        dispatch_reopen(&mut app);
        assert!(
            app.world()
                .entity(leaf)
                .get::<vmux_history::LastActivatedAt>()
                .is_some()
        );
        assert!(
            app.world()
                .entity(tab)
                .get::<vmux_history::LastActivatedAt>()
                .is_some()
        );
    }

    #[test]
    fn reopen_focus_propagates_through_reattached_splits() {
        use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneId("root".to_string()),
                ChildOf(tab),
            ))
            .id();
        let mid = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                PaneId("mid".to_string()),
                ChildOf(root),
            ))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "leaf-deep".to_string(),
                stack_index: 0,
                pane_path: vec![
                    PaneStep {
                        split_id: "root".to_string(),
                        axis: SplitAxis::Row,
                        child_index: 1,
                        flex_weights: vec![1.0, 1.0],
                    },
                    PaneStep {
                        split_id: "mid".to_string(),
                        axis: SplitAxis::Column,
                        child_index: 1,
                        flex_weights: vec![1.0, 1.0],
                    },
                ],
            },
        ));
        dispatch_reopen(&mut app);
        assert!(
            app.world()
                .entity(mid)
                .get::<vmux_history::LastActivatedAt>()
                .is_some(),
            "reattached intermediate split is activated through the restored chain"
        );
    }

    #[test]
    fn reopen_resplits_collapsed_two_pane() {
        use crate::pane::{Pane, PaneId, PaneSplit};
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let root = app
            .world_mut()
            .spawn((Pane, PaneId("root".to_string()), ChildOf(tab)))
            .id();
        let survivor_stack = app
            .world_mut()
            .spawn((Stack::default(), ChildOf(root)))
            .id();
        app.world_mut().spawn((
            ArchivedPage {
                url: "https://z".to_string(),
                space_id: "s1".to_string(),
                closed_at: 5,
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "paneR".to_string(),
                stack_index: 0,
                pane_path: vec![PaneStep {
                    split_id: "root".to_string(),
                    axis: SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                }],
            },
        ));
        dispatch_reopen(&mut app);

        assert!(
            app.world().entity(root).get::<PaneSplit>().is_some(),
            "root was re-split"
        );
        let panes: Vec<Entity> = app
            .world()
            .entity(root)
            .get::<Children>()
            .unwrap()
            .iter()
            .filter(|&e| app.world().entity(e).contains::<Pane>())
            .collect();
        assert_eq!(panes.len(), 2, "two panes under the restored split");
        let total_stacks = app.world_mut().query::<&Stack>().iter(app.world()).count();
        assert_eq!(total_stacks, 2);
        let survivor_pane = app
            .world()
            .entity(survivor_stack)
            .get::<ChildOf>()
            .unwrap()
            .parent();
        assert!(
            panes.contains(&survivor_pane),
            "survivor re-homed into a pane"
        );
        assert_eq!(drain_opens(&mut app).len(), 1);
    }
}
