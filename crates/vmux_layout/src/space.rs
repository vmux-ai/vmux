use crate::event::TabsCommandEvent;
use crate::{
    NewStackContext,
    pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    settings::LayoutSettings,
    stack::stack_bundle,
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    window::Main as MainNode,
};
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    prelude::*,
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_command::open::OpenCommand;
use vmux_command::{AppCommand, BrowserCommand, LayoutCommand, ReadAppCommands, TabCommand};
use vmux_history::{CreatedAt, LastActivatedAt};

pub struct SpacePlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpaceCommandSet;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .add_plugins(BinEventEmitterPlugin::<(TabsCommandEvent,)>::default())
            .add_observer(on_tabs_command_emit)
            .add_systems(
                Update,
                handle_space_commands
                    .in_set(ReadAppCommands)
                    .in_set(SpaceCommandSet),
            )
            .add_systems(PostUpdate, sync_space_visibility);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Space {
    pub name: String,
}

pub fn space_bundle() -> impl Bundle {
    (
        Space::default(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_new_space(
    main: Entity,
    pw: Entity,
    name: String,
    settings: &LayoutSettings,
    effective_startup_url: Option<&crate::settings::EffectiveStartupUrl>,
    new_stack_ctx: &mut NewStackContext,
    spawn_requests: &mut MessageWriter<crate::LayoutSpawnRequest>,
    commands: &mut Commands,
) -> Entity {
    let space_e = commands
        .spawn((
            Space { name },
            Transform::default(),
            GlobalTransform::default(),
            Visibility::default(),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(main),
        ))
        .id();

    let gap = pane_split_gaps(PaneSplitDirection::Row, settings.pane.gap);
    let split_root = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            HostWindow(pw),
            ZIndex(0),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
                ..default()
            },
            ChildOf(space_e),
        ))
        .id();

    let leaf = commands
        .spawn((
            leaf_pane_bundle(),
            LastActivatedAt::now(),
            ChildOf(split_root),
        ))
        .id();

    let stack = commands
        .spawn((
            stack_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(leaf),
        ))
        .id();

    if let Some(old_stack) = new_stack_ctx.stack.take() {
        commands.entity(old_stack).despawn();
    }
    new_stack_ctx.previous_stack = None;
    new_stack_ctx.dismiss_modal = false;

    let url = effective_startup_url
        .map(|u| u.0.clone())
        .unwrap_or_default();
    if url.is_empty() {
        new_stack_ctx.stack = Some(stack);
        new_stack_ctx.needs_open = true;
    } else {
        spawn_requests.write(crate::LayoutSpawnRequest::OpenUrl { stack, url });
    }

    space_e
}

#[allow(clippy::too_many_arguments)]
fn handle_space_commands(
    mut reader: MessageReader<AppCommand>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    space_q: Query<Entity, With<Space>>,
    main_q: Query<Entity, With<MainNode>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    settings: Res<LayoutSettings>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut spawn_requests: MessageWriter<crate::LayoutSpawnRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let active_space = spaces.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);

        match cmd {
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url })) => {
                let Ok(main) = main_q.single() else { continue };
                let count = spaces.iter().count();
                let name = format!("Tab {}", count + 1);
                let startup = effective_startup_url.as_deref().map(|u| u.0.as_str());
                let resolved = vmux_command::open::handler::resolve_url(url.as_deref(), startup);
                let override_startup = crate::settings::EffectiveStartupUrl(resolved);
                spawn_new_space(
                    main,
                    *primary_window,
                    name,
                    &settings,
                    Some(&override_startup),
                    &mut new_stack_ctx,
                    &mut spawn_requests,
                    &mut commands,
                );
            }
            AppCommand::Layout(LayoutCommand::Tab(tab_cmd)) => match tab_cmd {
                TabCommand::Close => {
                    let Some(active) = active_space else { continue };
                    close_space_entity(
                        active,
                        active_space,
                        spaces.iter().count(),
                        &space_q,
                        &main_q,
                        *primary_window,
                        &child_of_q,
                        &all_children,
                        &settings,
                        effective_startup_url.as_deref(),
                        &mut new_stack_ctx,
                        &mut spawn_requests,
                        &mut commands,
                    );
                }
                TabCommand::Next | TabCommand::Previous => {
                    let Some(active) = active_space else { continue };
                    let siblings =
                        active_space_siblings(active, &child_of_q, &all_children, &space_q);
                    if siblings.len() <= 1 {
                        continue;
                    }
                    let Some(idx) = siblings.iter().position(|e| *e == active) else {
                        continue;
                    };
                    let target_idx = if *tab_cmd == TabCommand::Next {
                        (idx + 1) % siblings.len()
                    } else {
                        (idx + siblings.len() - 1) % siblings.len()
                    };
                    let target = siblings[target_idx];
                    if target != active {
                        commands.entity(target).insert(LastActivatedAt::now());
                    }
                }
                TabCommand::Rename => {}
                TabCommand::SelectIndex1
                | TabCommand::SelectIndex2
                | TabCommand::SelectIndex3
                | TabCommand::SelectIndex4
                | TabCommand::SelectIndex5
                | TabCommand::SelectIndex6
                | TabCommand::SelectIndex7
                | TabCommand::SelectIndex8
                | TabCommand::SelectLast => {
                    let Some(active) = active_space else { continue };
                    let siblings =
                        active_space_siblings(active, &child_of_q, &all_children, &space_q);
                    if siblings.is_empty() {
                        continue;
                    }
                    let target_idx = match tab_cmd {
                        TabCommand::SelectIndex1 => 0,
                        TabCommand::SelectIndex2 => 1,
                        TabCommand::SelectIndex3 => 2,
                        TabCommand::SelectIndex4 => 3,
                        TabCommand::SelectIndex5 => 4,
                        TabCommand::SelectIndex6 => 5,
                        TabCommand::SelectIndex7 => 6,
                        TabCommand::SelectIndex8 => 7,
                        TabCommand::SelectLast => siblings.len() - 1,
                        _ => continue,
                    };
                    if target_idx >= siblings.len() {
                        continue;
                    }
                    let target = siblings[target_idx];
                    if target != active {
                        commands.entity(target).insert(LastActivatedAt::now());
                    }
                }
                TabCommand::SwapPrev | TabCommand::SwapNext => {
                    let Some(active) = active_space else { continue };
                    let Ok(co) = child_of_q.get(active) else {
                        continue;
                    };
                    let parent = co.get();
                    let Ok(children) = all_children.get(parent) else {
                        continue;
                    };
                    let kind_positions: Vec<usize> = children
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| space_q.contains(*e))
                        .map(|(i, _)| i)
                        .collect();
                    let Some(active_idx) = find_kind_index(active, children, &kind_positions)
                    else {
                        continue;
                    };
                    let pair = if *tab_cmd == TabCommand::SwapPrev {
                        resolve_prev(active_idx)
                    } else {
                        resolve_next(active_idx, kind_positions.len())
                    };
                    if let Some((a, b)) = pair {
                        swap_siblings(&mut commands, parent, children, &kind_positions, a, b);
                    }
                }
            },
            _ => continue,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn close_space_entity(
    target: Entity,
    active_space: Option<Entity>,
    space_count: usize,
    space_q: &Query<Entity, With<Space>>,
    main_q: &Query<Entity, With<MainNode>>,
    primary_window: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    settings: &LayoutSettings,
    effective_startup_url: Option<&crate::settings::EffectiveStartupUrl>,
    new_stack_ctx: &mut NewStackContext,
    spawn_requests: &mut MessageWriter<crate::LayoutSpawnRequest>,
    commands: &mut Commands,
) {
    let siblings = active_space_siblings(target, child_of_q, all_children, space_q);
    if siblings.len() <= 1 {
        let Ok(main) = main_q.single() else { return };
        let name = format!("Tab {}", space_count + 1);
        spawn_new_space(
            main,
            primary_window,
            name,
            settings,
            effective_startup_url,
            new_stack_ctx,
            spawn_requests,
            commands,
        );
    } else if active_space == Some(target)
        && let Some(next) = pick_after_close(target, &siblings)
    {
        commands.entity(next).insert(LastActivatedAt::now());
    }
    commands.entity(target).despawn();
}

pub fn active_space_siblings(
    active: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    space_q: &Query<Entity, With<Space>>,
) -> Vec<Entity> {
    let Ok(co) = child_of_q.get(active) else {
        return vec![active];
    };
    let parent = co.get();
    let Ok(children) = all_children.get(parent) else {
        return vec![active];
    };
    children
        .iter()
        .filter(|e| space_q.contains(*e))
        .collect::<Vec<_>>()
}

fn pick_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() { idx + 1 } else { 0 };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
}

fn sync_space_visibility(
    mut spaces: Query<(Entity, &LastActivatedAt, &mut Node, &mut Visibility), With<Space>>,
) {
    let active = spaces
        .iter()
        .max_by_key(|(_, ts, _, _)| ts.0)
        .map(|(e, _, _, _)| e);
    for (entity, _, mut node, mut vis) in &mut spaces {
        let is_active = Some(entity) == active;
        let target_display = if is_active {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != target_display {
            node.display = target_display;
        }
        let target_vis = if is_active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

fn on_tabs_command_emit(
    trigger: On<BinReceive<TabsCommandEvent>>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    space_q: Query<Entity, With<Space>>,
    main_q: Query<Entity, With<MainNode>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    settings: Res<LayoutSettings>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut spawn_requests: MessageWriter<crate::LayoutSpawnRequest>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let active_space = spaces.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);
    match evt.command.as_str() {
        "new" => {
            messages.write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));
        }
        "close" => {
            let target = space_target(
                evt.tab_id.as_deref(),
                spaces.iter().map(|(entity, _)| entity),
            )
            .or(active_space);
            let Some(target) = target else { return };
            close_space_entity(
                target,
                active_space,
                spaces.iter().count(),
                &space_q,
                &main_q,
                *primary_window,
                &child_of_q,
                &all_children,
                &settings,
                effective_startup_url.as_deref(),
                &mut new_stack_ctx,
                &mut spawn_requests,
                &mut commands,
            );
        }
        "switch" => {
            let Some(id_str) = evt.tab_id.as_deref() else {
                return;
            };
            let Ok(bits) = id_str.parse::<u64>() else {
                return;
            };
            let Some((target, _)) = spaces.iter().find(|(e, _)| e.to_bits() == bits) else {
                return;
            };
            commands.entity(target).insert(LastActivatedAt::now());
        }
        _ => {}
    }
}

fn space_target(id: Option<&str>, spaces: impl IntoIterator<Item = Entity>) -> Option<Entity> {
    let bits = id?.parse::<u64>().ok()?;
    spaces.into_iter().find(|e| e.to_bits() == bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_command::CommandPlugin;

    #[test]
    fn space_target_uses_event_tab_id() {
        let target = Entity::from_bits(42);
        let other = Entity::from_bits(7);
        let id = target.to_bits().to_string();

        assert_eq!(space_target(Some(&id), [other, target]), Some(target));
    }

    fn test_settings() -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: WindowSettings {
                padding: 0.0,
                padding_top: None,
                padding_right: None,
                padding_bottom: None,
                padding_left: None,
            },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        }
    }

    #[derive(Resource, Default)]
    struct CollectedSpawns(Vec<crate::LayoutSpawnRequest>);

    fn collect_spawn_requests(
        mut reader: MessageReader<crate::LayoutSpawnRequest>,
        mut collected: ResMut<CollectedSpawns>,
    ) {
        for req in reader.read() {
            collected.0.push(req.clone());
        }
    }

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin));
        app.add_message::<crate::LayoutSpawnRequest>();
        app.init_resource::<NewStackContext>();
        app.insert_resource(test_settings());
        app.init_resource::<CollectedSpawns>();
        app.add_systems(
            Update,
            (
                handle_space_commands.in_set(ReadAppCommands),
                collect_spawn_requests.after(handle_space_commands),
            ),
        );
        app
    }

    fn spawn_main_and_space(app: &mut App) -> Entity {
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let main = app.world_mut().spawn(MainNode).id();
        app.world_mut().spawn((
            Space {
                name: "Tab 1".into(),
            },
            LastActivatedAt::now(),
            ChildOf(main),
        ));
        let _ = window;
        main
    }

    #[test]
    fn open_in_new_tab_explicit_url_spawns_new_space_with_url() {
        let mut app = build_app();
        spawn_main_and_space(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab {
                    url: Some("https://example.com".into()),
                },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        match &collected.0[0] {
            crate::LayoutSpawnRequest::OpenUrl { url, .. } => {
                assert_eq!(url, "https://example.com");
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }

        let space_count = app.world_mut().query::<&Space>().iter(app.world()).count();
        assert_eq!(space_count, 2, "expected two spaces after InNewTab");
    }

    #[test]
    fn open_in_new_tab_none_url_falls_back_to_startup() {
        let mut app = build_app();
        app.insert_resource(crate::settings::EffectiveStartupUrl(
            "https://startup.test".into(),
        ));
        spawn_main_and_space(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        match &collected.0[0] {
            crate::LayoutSpawnRequest::OpenUrl { url, .. } => {
                assert_eq!(url, "https://startup.test");
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }
    }

    #[test]
    fn open_in_new_tab_none_url_no_startup_falls_back_to_default() {
        let mut app = build_app();
        spawn_main_and_space(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        match &collected.0[0] {
            crate::LayoutSpawnRequest::OpenUrl { url, .. } => {
                assert_eq!(url, vmux_command::open::handler::DEFAULT_NEW_PAGE_URL);
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }
    }
}
