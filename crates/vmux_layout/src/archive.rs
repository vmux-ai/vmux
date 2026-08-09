use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::HostWindow;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, StackCommand};
use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
use vmux_core::terminal::{TerminalLaunch, TerminalSpawnRequest};
use vmux_core::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, CreatedAt, PageArchiveRequest,
    PageMetadata, PageOpenRequest, PageOpenTarget, PaneStep, SplitAxis, now_millis,
};

use crate::event::TERMINAL_PAGE_URL;
use crate::pane::{
    Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection, leaf_pane_bundle, split_root_bundle,
};
use crate::settings::LayoutSettings;
use crate::space::{ActiveSpaceEntity, Space, SpaceId, space_of};
use crate::stack::{ActiveTabParam, FocusedStack, Stack, StackCommandSet, stack_bundle};
use crate::tab::{
    CloseTabRequest, LastTabCloseAt, Tab, active_tab_siblings, pick_after_close, tab_bundle,
};
use crate::window::spawn_tab_scaffold_in_space;
use crate::{TabLayoutSpawnContent, TabLayoutSpawnRequest};

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

const MAX_ARCHIVE_ENTRIES: usize = 25;
const ARCHIVE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

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
        spawn_archived_page(&mut commands, req, now_millis(), None);
    }
}

fn spawn_archived_page(
    commands: &mut Commands,
    req: &PageArchiveRequest,
    closed_at: i64,
    tab: Option<ArchivedTabPage>,
) {
    if req.url.is_empty() && tab.is_none() {
        return;
    }
    let mut entity = commands.spawn((
        ArchivedPage {
            url: req.url.clone(),
            title: req.title.clone(),
            space_id: req.space_id.clone(),
            closed_at,
            launch: req.launch.clone(),
            tab_index: req.tab_index,
        },
        ArchivedPagePosition {
            leaf_pane_id: req.leaf_pane_id.clone(),
            stack_index: req.stack_index,
            pane_path: req.pane_path.clone(),
        },
    ));
    if let Some(tab) = tab {
        entity.insert(tab);
    }
}

fn maintain_archive(
    archived: Query<(Entity, &ArchivedPage, Option<&ArchivedTabPage>)>,
    mut commands: Commands,
) {
    let now = now_millis();
    let mut groups: HashMap<String, (i64, Vec<Entity>)> = HashMap::new();
    let mut singles = Vec::new();
    for (entity, page, tab) in &archived {
        if let Some(tab) = tab.filter(|tab| !tab.group_id.is_empty()) {
            let group = groups
                .entry(tab.group_id.clone())
                .or_insert_with(|| (page.closed_at, Vec::new()));
            group.0 = group.0.max(page.closed_at);
            group.1.push(entity);
        } else {
            singles.push((page.closed_at, vec![entity]));
        }
    }

    let mut live: Vec<(i64, Vec<Entity>)> = groups.into_values().collect();
    live.extend(singles);
    for (closed_at, entities) in &live {
        if now - *closed_at <= ARCHIVE_TTL_MS {
            continue;
        }
        for &entity in entities {
            commands.entity(entity).despawn();
        }
    }
    live.retain(|(closed_at, _)| now - *closed_at <= ARCHIVE_TTL_MS);
    if live.len() <= MAX_ARCHIVE_ENTRIES {
        return;
    }
    live.sort_by_key(|(closed_at, _)| *closed_at);
    let overflow = live.len() - MAX_ARCHIVE_ENTRIES;
    for (_, entities) in live.into_iter().take(overflow) {
        for entity in entities {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct TabArchiveLayout<'w, 's> {
    stack_pages: Query<
        'w,
        's,
        (
            Entity,
            &'static PageMetadata,
            Option<&'static TerminalLaunch>,
            Option<&'static vmux_history::LastActivatedAt>,
        ),
        With<Stack>,
    >,
    child_of: Query<'w, 's, &'static ChildOf>,
    children_q: Query<'w, 's, &'static Children>,
    spaces: Query<'w, 's, (), With<Space>>,
    space_ids: Query<'w, 's, &'static SpaceId>,
    tabs: Query<'w, 's, (), With<Tab>>,
    stacks: Query<'w, 's, (), With<Stack>>,
    pane_ids: Query<'w, 's, &'static PaneId>,
    splits: Query<'w, 's, &'static PaneSplit>,
    pane_sizes: Query<'w, 's, &'static PaneSize>,
    panes: Query<'w, 's, (), With<Pane>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_close_tab_requests(
    mut reader: MessageReader<CloseTabRequest>,
    active_tab_param: ActiveTabParam,
    tab_data: Query<&Tab>,
    tab_q: Query<Entity, With<Tab>>,
    layout: TabArchiveLayout,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut last_tab_close: ResMut<LastTabCloseAt>,
    mut commands: Commands,
) {
    let mut seen = HashSet::new();
    let requests: Vec<Entity> = reader
        .read()
        .filter_map(|request| seen.insert(request.tab).then_some(request.tab))
        .filter(|tab| tab_data.contains(*tab))
        .collect();
    let closing: HashSet<Entity> = requests.iter().copied().collect();
    let mut replacement_spaces = HashSet::new();
    for requested_tab in requests {
        let request = CloseTabRequest { tab: requested_tab };
        let Ok(tab) = tab_data.get(request.tab) else {
            continue;
        };
        let siblings =
            active_tab_siblings(request.tab, &layout.child_of, &layout.children_q, &tab_q);
        let surviving_siblings: Vec<Entity> = siblings
            .iter()
            .copied()
            .filter(|sibling| !closing.contains(sibling))
            .collect();
        if surviving_siblings.is_empty() {
            let Ok(tab_space) = layout
                .child_of
                .get(request.tab)
                .map(|parent| parent.parent())
            else {
                continue;
            };
            let preferred_source = active_tab_param
                .get()
                .filter(|active| siblings.contains(active) && closing.contains(active))
                .unwrap_or(request.tab);
            if request.tab == preferred_source && !replacement_spaces.contains(&tab_space) {
                layout_requests.write(TabLayoutSpawnRequest {
                    space: tab_space,
                    primary_window: *primary_window,
                    name: Some("Tab 1".to_string()),
                    startup_dir: None,
                    content: TabLayoutSpawnContent::StartupUrlOrPrompt,
                    clear_pending_stack: true,
                    focus: true,
                });
                replacement_spaces.insert(tab_space);
            }
        } else if active_tab_param.get() == Some(request.tab)
            && let Some(next) = pick_after_close(
                request.tab,
                &siblings
                    .iter()
                    .copied()
                    .filter(|sibling| *sibling == request.tab || !closing.contains(sibling))
                    .collect::<Vec<_>>(),
            )
        {
            commands
                .entity(next)
                .insert(vmux_history::LastActivatedAt::now());
        }

        archive_tab(request.tab, tab, &layout, &mut commands);
        last_tab_close.0 = Some(std::time::Instant::now());
        commands.entity(request.tab).despawn();
    }
}

fn archive_tab(tab_entity: Entity, tab: &Tab, layout: &TabArchiveLayout, commands: &mut Commands) {
    let Some(space) = space_of(tab_entity, &layout.child_of, &layout.spaces) else {
        return;
    };
    let space_id = layout
        .space_ids
        .get(space)
        .map(|id| id.0.clone())
        .unwrap_or_default();
    let tab_index = layout.children_q.get(space).ok().and_then(|children| {
        children
            .iter()
            .filter(|entity| layout.tabs.contains(*entity))
            .position(|entity| entity == tab_entity)
    });
    let mut stacks = Vec::new();
    collect_descendant_stacks(tab_entity, &layout.children_q, &layout.stacks, &mut stacks);
    let active_stack = stacks.iter().copied().max_by_key(|stack| {
        layout
            .stack_pages
            .get(*stack)
            .ok()
            .and_then(|(_, _, _, activated)| activated)
            .map(|activated| activated.0)
            .unwrap_or_default()
    });
    let group_id = uuid::Uuid::new_v4().to_string();
    let closed_at = now_millis();

    for stack in stacks {
        let Ok((_, metadata, launch, _)) = layout.stack_pages.get(stack) else {
            continue;
        };
        let (leaf_pane_id, stack_index, pane_path) = pane_path_of(
            stack,
            &layout.child_of,
            &layout.children_q,
            &layout.pane_ids,
            &layout.splits,
            &layout.pane_sizes,
            &layout.panes,
            &layout.stacks,
            &layout.tabs,
        )
        .unwrap_or_default();
        let request = PageArchiveRequest {
            url: metadata.url.clone(),
            title: metadata.title.clone(),
            space_id: space_id.clone(),
            launch: launch.cloned(),
            tab_index,
            leaf_pane_id,
            stack_index,
            pane_path,
        };
        spawn_archived_page(
            commands,
            &request,
            closed_at,
            Some(ArchivedTabPage {
                group_id: group_id.clone(),
                tab_name: tab.name.clone(),
                tab_startup_dir: tab.startup_dir.clone(),
                active: active_stack == Some(stack),
            }),
        );
    }
}

fn collect_descendant_stacks(
    entity: Entity,
    children_q: &Query<&Children>,
    stacks: &Query<(), With<Stack>>,
    result: &mut Vec<Entity>,
) {
    if stacks.contains(entity) {
        result.push(entity);
        return;
    }
    let Ok(children) = children_q.get(entity) else {
        return;
    };
    for child in children.iter() {
        collect_descendant_stacks(child, children_q, stacks, result);
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

#[derive(Clone)]
struct ReopenEntry {
    entity: Entity,
    page: ArchivedPage,
    position: Option<ArchivedPagePosition>,
    tab: Option<ArchivedTabPage>,
}

#[allow(clippy::too_many_arguments)]
fn handle_reopen_closed_page(
    mut reader: MessageReader<AppCommand>,
    archived: Query<(Entity, &ArchivedPage, Option<&ArchivedTabPage>)>,
    positions: Query<&ArchivedPagePosition>,
    spaces: Query<(Entity, &SpaceId), With<Space>>,
    any_space: Query<Entity, With<Space>>,
    layout: ReopenLayout,
    active_space: Res<ActiveSpaceEntity>,
    settings: Res<LayoutSettings>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
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

    let Some((entry_entity, page, archived_tab)) = archived
        .iter()
        .max_by_key(|(_, page, _)| page.closed_at)
        .map(|(entity, page, tab)| (entity, page.clone(), tab.cloned()))
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

    if let Some(tab) = archived_tab.filter(|tab| !tab.group_id.is_empty()) {
        let entries: Vec<ReopenEntry> = archived
            .iter()
            .filter(|(_, _, candidate)| {
                candidate.is_some_and(|candidate| candidate.group_id == tab.group_id)
            })
            .map(|(entity, page, tab)| ReopenEntry {
                entity,
                page: page.clone(),
                position: positions.get(entity).ok().cloned(),
                tab: tab.cloned(),
            })
            .collect();
        let restored = restore_archived_tab(
            space,
            origin_space == Some(space),
            &tab,
            entries,
            &mut commands,
            *primary_window,
        );
        for (entry, stack) in restored {
            reopen_page_content(&entry.page, stack, &mut commands);
            commands.entity(entry.entity).despawn();
        }
        return;
    }

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

    reopen_page_content(&page, stack, &mut commands);

    commands.entity(entry_entity).despawn();
}

fn reopen_page_content(page: &ArchivedPage, stack: Entity, commands: &mut Commands) {
    if page.url.is_empty() {
        return;
    }
    // CLI agent urls are `<kind>/cli` (fresh) or `<kind>/cli/<sid>` (resume). A plain
    // `<kind>/<sid>` (no `cli` marker) is an ACP session and falls through to `PageOpenRequest`,
    // which reconstructs it via the runtime agent handler. ("cli" is `url::CLI_FRESH_SID`, not
    // imported here to avoid a vmux_layout -> vmux_agent dependency cycle.)
    let agent_cli = AgentKind::all().into_iter().find_map(|k| {
        let rest = page.url.strip_prefix(&k.cli_url_prefix())?;
        if rest == "cli" {
            Some((k, None))
        } else {
            rest.strip_prefix("cli/")
                .map(|sid| (k, Some(sid.to_string())))
        }
    });
    if let Some((kind, session_id)) = agent_cli {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| PathBuf::from(&l.cwd))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let request = SpawnAgentInStackRequest {
            kind,
            cwd,
            session_id,
            stack,
            initial_prompt: None,
            initial_attachments: Vec::new(),
        };
        commands.queue(move |world: &mut World| {
            world.write_message(request);
        });
    } else if page.url.starts_with(TERMINAL_PAGE_URL) {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| l.cwd.clone())
            .filter(|c| !c.is_empty())
            .map(PathBuf::from);
        let request = TerminalSpawnRequest {
            cwd,
            target_stack: Some(stack),
        };
        commands.queue(move |world: &mut World| {
            world.write_message(request);
        });
    } else {
        let request = PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: page.url.clone(),
            request_id: None,
        };
        commands.queue(move |world: &mut World| {
            world.write_message(request);
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ArchivedPaneChild {
    Split(String),
    Leaf(String),
}

struct ArchivedSplit {
    axis: SplitAxis,
    flex_weights: Vec<f32>,
    children: BTreeMap<usize, ArchivedPaneChild>,
}

struct ArchivedPaneTree {
    root_id: String,
    splits: HashMap<String, ArchivedSplit>,
}

fn restore_archived_tab(
    space: Entity,
    origin_matches: bool,
    archived_tab: &ArchivedTabPage,
    mut entries: Vec<ReopenEntry>,
    commands: &mut Commands,
    primary_window: Entity,
) -> Vec<(ReopenEntry, Entity)> {
    if entries.is_empty() {
        return Vec::new();
    }
    let active_entry = entries
        .iter()
        .find(|entry| entry.tab.as_ref().is_some_and(|tab| tab.active))
        .map(|entry| entry.entity)
        .unwrap_or(entries[0].entity);
    let tab = commands
        .spawn((
            tab_bundle(),
            vmux_history::LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(space),
        ))
        .id();
    commands.entity(tab).insert(Tab {
        name: archived_tab.tab_name.clone(),
        startup_dir: archived_tab.tab_startup_dir.clone(),
    });
    if origin_matches && let Some(index) = entries.first().and_then(|entry| entry.page.tab_index) {
        commands.entity(space).insert_children(index, &[tab]);
    }

    let tree = build_archived_pane_tree(&entries);
    let mut pane_entities = HashMap::new();
    let mut leaf_entities = HashMap::new();
    let fallback_leaf = if let Some(tree) = tree.as_ref() {
        spawn_archived_split(
            &tree.root_id,
            tab,
            None,
            true,
            tree,
            commands,
            primary_window,
            &mut pane_entities,
            &mut leaf_entities,
        );
        None
    } else {
        let root_id = uuid::Uuid::new_v4().to_string();
        let root = commands
            .spawn((
                split_root_bundle(PaneSplitDirection::Row),
                PaneId(root_id.clone()),
                vmux_history::LastActivatedAt(0),
                HostWindow(primary_window),
                ChildOf(tab),
            ))
            .id();
        pane_entities.insert(root_id, root);
        let leaf_id = uuid::Uuid::new_v4().to_string();
        let leaf = commands
            .spawn((
                leaf_pane_bundle(),
                PaneId(leaf_id.clone()),
                vmux_history::LastActivatedAt(0),
                ChildOf(root),
            ))
            .id();
        pane_entities.insert(leaf_id.clone(), leaf);
        leaf_entities.insert(leaf_id, leaf);
        Some(leaf)
    };

    entries.sort_by(|left, right| {
        let left_position = left.position.as_ref();
        let right_position = right.position.as_ref();
        left_position
            .map(|position| (&position.leaf_pane_id, position.stack_index))
            .cmp(&right_position.map(|position| (&position.leaf_pane_id, position.stack_index)))
    });

    let mut restored = Vec::with_capacity(entries.len());
    for entry in entries {
        let leaf = entry
            .position
            .as_ref()
            .and_then(|position| leaf_entities.get(&position.leaf_pane_id).copied())
            .or(fallback_leaf)
            .or_else(|| leaf_entities.values().next().copied());
        let Some(leaf) = leaf else {
            continue;
        };
        let active = entry.entity == active_entry;
        let stack = commands
            .spawn((
                stack_bundle(),
                vmux_history::LastActivatedAt(if active { now_millis() } else { 0 }),
                CreatedAt::now(),
                ChildOf(leaf),
            ))
            .id();
        commands.entity(stack).insert(PageMetadata {
            url: entry.page.url.clone(),
            title: entry.page.title.clone(),
            ..default()
        });
        if active {
            commands
                .entity(leaf)
                .insert(vmux_history::LastActivatedAt::now());
            if let Some(position) = entry.position.as_ref() {
                for step in &position.pane_path {
                    if let Some(entity) = pane_entities.get(&step.split_id) {
                        commands
                            .entity(*entity)
                            .insert(vmux_history::LastActivatedAt::now());
                    }
                }
            }
        }
        restored.push((entry, stack));
    }
    restored
}

fn build_archived_pane_tree(entries: &[ReopenEntry]) -> Option<ArchivedPaneTree> {
    let mut root_id = None;
    let mut splits: HashMap<String, ArchivedSplit> = HashMap::new();
    for entry in entries {
        let position = entry.position.as_ref()?;
        if position.leaf_pane_id.is_empty() || position.pane_path.is_empty() {
            return None;
        }
        let entry_root = &position.pane_path[0].split_id;
        if root_id
            .as_ref()
            .is_some_and(|root: &String| root != entry_root)
        {
            return None;
        }
        root_id.get_or_insert_with(|| entry_root.clone());
        for (index, step) in position.pane_path.iter().enumerate() {
            let child = if let Some(next) = position.pane_path.get(index + 1) {
                ArchivedPaneChild::Split(next.split_id.clone())
            } else {
                ArchivedPaneChild::Leaf(position.leaf_pane_id.clone())
            };
            let split = splits
                .entry(step.split_id.clone())
                .or_insert_with(|| ArchivedSplit {
                    axis: step.axis,
                    flex_weights: step.flex_weights.clone(),
                    children: BTreeMap::new(),
                });
            if split.axis != step.axis {
                return None;
            }
            if let Some(existing) = split.children.get(&step.child_index)
                && existing != &child
            {
                return None;
            }
            split.children.insert(step.child_index, child);
        }
    }
    Some(ArchivedPaneTree {
        root_id: root_id?,
        splits,
    })
}

#[allow(clippy::too_many_arguments)]
fn spawn_archived_split(
    id: &str,
    parent: Entity,
    flex_grow: Option<f32>,
    root: bool,
    tree: &ArchivedPaneTree,
    commands: &mut Commands,
    primary_window: Entity,
    pane_entities: &mut HashMap<String, Entity>,
    leaf_entities: &mut HashMap<String, Entity>,
) -> Option<Entity> {
    let split = tree.splits.get(id)?;
    let direction = match split.axis {
        SplitAxis::Row => PaneSplitDirection::Row,
        SplitAxis::Column => PaneSplitDirection::Column,
    };
    let entity = commands
        .spawn((
            split_root_bundle(direction),
            PaneId(id.to_string()),
            vmux_history::LastActivatedAt(0),
            ChildOf(parent),
        ))
        .id();
    if let Some(flex_grow) = flex_grow {
        commands.entity(entity).insert(PaneSize { flex_grow });
    }
    if root {
        commands.entity(entity).insert(HostWindow(primary_window));
    }
    pane_entities.insert(id.to_string(), entity);

    for (child_index, child) in &split.children {
        let flex_grow = split.flex_weights.get(*child_index).copied().unwrap_or(1.0);
        match child {
            ArchivedPaneChild::Split(child_id) => {
                spawn_archived_split(
                    child_id,
                    entity,
                    Some(flex_grow),
                    false,
                    tree,
                    commands,
                    primary_window,
                    pane_entities,
                    leaf_entities,
                );
            }
            ArchivedPaneChild::Leaf(child_id) => {
                let leaf = commands
                    .spawn((
                        leaf_pane_bundle(),
                        PaneId(child_id.clone()),
                        vmux_history::LastActivatedAt(0),
                        ChildOf(entity),
                    ))
                    .id();
                commands.entity(leaf).insert(PaneSize { flex_grow });
                pane_entities.insert(child_id.clone(), leaf);
                leaf_entities.insert(child_id.clone(), leaf);
            }
        }
    }
    Some(entity)
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
        let leaf = if layout.leaf_panes.contains(parent) {
            parent
        } else if let Some(leaf) = first_leaf_descendant(parent, layout) {
            leaf
        } else {
            commands
                .spawn((
                    leaf_pane_bundle(),
                    vmux_history::LastActivatedAt::now(),
                    ChildOf(parent),
                ))
                .id()
        };
        return Some((leaf, anchor));
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

fn first_leaf_descendant(root: Entity, layout: &ReopenLayout) -> Option<Entity> {
    if layout.leaf_panes.contains(root) {
        return Some(root);
    }
    for child in layout.children_q.get(root).ok()?.iter() {
        if let Some(leaf) = first_leaf_descendant(child, layout) {
            return Some(leaf);
        }
    }
    None
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
#[path = "archive.test.rs"]
mod tests;
