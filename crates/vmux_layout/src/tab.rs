use crate::event::TabsCommandEvent;
use crate::{
    TabLayoutSpawnContent, TabLayoutSpawnRequest,
    swap::{find_kind_index, move_child_to_parent, resolve_next, resolve_prev, swap_siblings},
};
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    prelude::*,
    ui::UiSystems,
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use std::time::Instant;
use vmux_command::open::OpenCommand;
use vmux_command::{AppCommand, BrowserCommand, LayoutCommand, ReadAppCommands, TabCommand};
use vmux_core::Order;
use vmux_history::LastActivatedAt;

pub struct TabPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TabCommandSet;

#[derive(Message, Clone, Copy)]
pub struct CloseTabRequest {
    pub tab: Entity,
}

#[derive(Message, Clone, Copy)]
pub struct MoveTabRequest {
    pub tab: Entity,
    pub target: Entity,
    pub after: bool,
}

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Tab>()
            .register_type::<Option<String>>()
            .register_type::<TabWorkspace>()
            .register_type::<TabWorktree>()
            .register_type::<TabDirDecided>()
            .init_resource::<LastTabCloseAt>()
            .add_message::<CloseTabRequest>()
            .add_message::<MoveTabRequest>()
            .add_plugins(BinEventEmitterPlugin::<(TabsCommandEvent,)>::for_hosts(&[
                "layout",
            ]))
            .add_observer(on_tabs_command_emit)
            .add_systems(
                Update,
                (
                    handle_tab_commands
                        .in_set(ReadAppCommands)
                        .in_set(TabCommandSet)
                        .after(crate::settings::EffectiveStartupDirSet),
                    handle_move_tab_requests.in_set(ReadAppCommands),
                ),
            )
            .add_systems(
                Update,
                crate::archive::handle_close_tab_requests
                    .in_set(ReadAppCommands)
                    .after(TabCommandSet),
            )
            .add_systems(PostUpdate, sync_tab_visibility.before(UiSystems::Layout))
            .add_systems(PostUpdate, sync_tab_order);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Tab {
    pub name: String,
    pub startup_dir: Option<String>,
}

/// Stable project directory for a tab. Unlike [`Tab::startup_dir`], this does not change when
/// the tab is rebound to a managed worktree.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabWorkspace {
    pub project_dir: String,
}

/// Present iff a tab's `startup_dir` points at a vmux-managed git worktree.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabWorktree {
    pub repo_root: String,
    #[reflect(default)]
    pub checkout_dir: String,
    pub branch: String,
    pub base_ref: String,
}

/// Runtime failure state for a persisted managed worktree. Ownership metadata remains attached.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct TabWorktreeUnavailable {
    pub message: String,
}

/// Marks that the worktree/work-here decision has been made for a tab, so the isolate offer
/// never fires again for it.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabDirDecided;

/// Walk up from `entity` to its ancestor [`Tab`] and return that tab's `startup_dir` override.
///
/// Everything spawned inside a tab (the ACP agent session and the user's terminals) shares the
/// tab's working directory; this resolves that override for a given stack/pane entity.
pub fn ancestor_tab_startup_dir(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&Tab>,
) -> Option<String> {
    let mut cur = entity;
    loop {
        if let Ok(tab) = tabs.get(cur) {
            return tab.startup_dir.clone();
        }
        cur = child_of.get(cur).ok()?.parent();
    }
}

#[derive(Resource, Default)]
pub struct LastTabCloseAt(pub Option<Instant>);

pub fn tab_bundle() -> impl Bundle {
    (
        Tab::default(),
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
fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_param: crate::stack::ActiveTabParam,
    tab_q: Query<Entity, With<Tab>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    effective_startup_dir: Option<Res<crate::settings::EffectiveStartupDir>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut close_requests: MessageWriter<CloseTabRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let active_tab = active_tab_param.get();

        match cmd {
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url })) => {
                let Some((space, startup_dir)) = effective_startup_dir
                    .as_deref()
                    .and_then(|effective| effective.0.clone())
                else {
                    continue;
                };
                let count = tabs.iter().count();
                let name = format!("Tab {}", count + 1);
                let content = url
                    .as_deref()
                    .filter(|url| !url.is_empty())
                    .map(|url| TabLayoutSpawnContent::Url(url.to_string()))
                    .or_else(|| {
                        effective_startup_url
                            .as_deref()
                            .map(|startup| startup.0.as_str())
                            .filter(|startup| !startup.is_empty())
                            .map(|startup| TabLayoutSpawnContent::Url(startup.to_string()))
                    })
                    .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt);
                layout_requests.write(TabLayoutSpawnRequest {
                    space,
                    primary_window: *primary_window,
                    name: Some(name),
                    startup_dir: startup_dir.clone(),
                    content,
                    clear_pending_stack: true,
                    focus: true,
                });
            }
            AppCommand::Layout(LayoutCommand::Tab(tab_cmd)) => match tab_cmd {
                TabCommand::Close => {
                    let Some(active) = active_tab else { continue };
                    close_requests.write(CloseTabRequest { tab: active });
                }
                TabCommand::New => {
                    let Some((space, startup_dir)) = effective_startup_dir
                        .as_deref()
                        .and_then(|effective| effective.0.clone())
                    else {
                        continue;
                    };
                    let name = format!("Tab {}", tabs.iter().count() + 1);
                    layout_requests.write(TabLayoutSpawnRequest {
                        space,
                        primary_window: *primary_window,
                        name: Some(name),
                        startup_dir: startup_dir.clone(),
                        content: TabLayoutSpawnContent::StartupUrlOrPrompt,
                        clear_pending_stack: true,
                        focus: true,
                    });
                }
                TabCommand::Next | TabCommand::Previous => {
                    let Some(active) = active_tab else { continue };
                    let siblings = active_tab_siblings(active, &child_of_q, &all_children, &tab_q);
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
                    let Some(active) = active_tab else { continue };
                    let siblings = active_tab_siblings(active, &child_of_q, &all_children, &tab_q);
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
                    let Some(active) = active_tab else { continue };
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
                        .filter(|(_, e)| tab_q.contains(*e))
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

pub fn active_tab_siblings(
    active: Entity,
    child_of_q: &Query<&ChildOf>,
    all_children: &Query<&Children>,
    tab_q: &Query<Entity, With<Tab>>,
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
        .filter(|e| tab_q.contains(*e))
        .collect::<Vec<_>>()
}

pub(crate) fn pick_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
    if siblings.len() <= 1 {
        return None;
    }
    let idx = siblings.iter().position(|e| *e == active)?;
    let next_idx = if idx + 1 < siblings.len() {
        idx + 1
    } else {
        idx - 1
    };
    let target = siblings[next_idx];
    if target == active { None } else { Some(target) }
}

fn sync_tab_visibility(
    mut tabs: Query<(&mut Node, &mut Visibility, Has<vmux_core::Active>), With<Tab>>,
) {
    for (mut node, mut vis, active) in &mut tabs {
        let target_display = if active { Display::Flex } else { Display::None };
        if node.display != target_display {
            node.display = target_display;
        }
        let target_vis = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

fn sync_tab_order(
    spaces: Query<&Children, (With<crate::space::Space>, Changed<Children>)>,
    tab_q: Query<(), With<Tab>>,
    mut order_q: Query<&mut Order>,
    mut commands: Commands,
) {
    for children in &spaces {
        let mut idx = 0u32;
        for child in children.iter() {
            if !tab_q.contains(child) {
                continue;
            }
            match order_q.get_mut(child) {
                Ok(mut order) => {
                    if order.0 != idx {
                        order.0 = idx;
                    }
                }
                Err(_) => {
                    commands.entity(child).insert(Order(idx));
                }
            }
            idx += 1;
        }
    }
}

fn on_tabs_command_emit(
    trigger: On<BinReceive<TabsCommandEvent>>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_param: crate::stack::ActiveTabParam,
    mut messages: ResMut<Messages<AppCommand>>,
    mut issued: ResMut<Messages<vmux_command::CommandIssued>>,
    user_q: Query<Entity, With<vmux_core::team::User>>,
    mut close_requests: MessageWriter<CloseTabRequest>,
    mut move_requests: Option<MessageWriter<MoveTabRequest>>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let active_tab = active_tab_param.get();
    let caller = user_q.single().unwrap_or(Entity::PLACEHOLDER);
    match evt.command.as_str() {
        "new" => {
            let cmd =
                AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url: None }));
            issued.write(vmux_command::CommandIssued {
                caller,
                command: cmd.clone(),
            });
            messages.write(cmd);
        }
        "close" => {
            let target = tab_target(evt.tab_id.as_deref(), tabs.iter().map(|(entity, _)| entity))
                .or(active_tab);
            let Some(target) = target else { return };
            close_requests.write(CloseTabRequest { tab: target });
        }
        "switch" => {
            let Some(id_str) = evt.tab_id.as_deref() else {
                return;
            };
            let Ok(bits) = id_str.parse::<u64>() else {
                return;
            };
            let Some((target, _)) = tabs.iter().find(|(e, _)| e.to_bits() == bits) else {
                return;
            };
            commands.entity(target).insert(LastActivatedAt::now());
        }
        "move" => {
            let Some(move_requests) = move_requests.as_mut() else {
                return;
            };
            let Some(tab) =
                tab_target(evt.tab_id.as_deref(), tabs.iter().map(|(entity, _)| entity))
            else {
                return;
            };
            let Some(target) = tab_target(
                evt.target_tab_id.as_deref(),
                tabs.iter().map(|(entity, _)| entity),
            ) else {
                return;
            };
            move_requests.write(MoveTabRequest {
                tab,
                target,
                after: evt.drop_after,
            });
        }
        _ => {}
    }
}

fn handle_move_tab_requests(
    mut reader: MessageReader<MoveTabRequest>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    tab_q: Query<(), With<Tab>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if request.tab == request.target
            || !tab_q.contains(request.tab)
            || !tab_q.contains(request.target)
        {
            continue;
        }
        let Ok(source_parent) = child_of_q.get(request.tab).map(Relationship::get) else {
            continue;
        };
        let Ok(target_parent) = child_of_q.get(request.target).map(Relationship::get) else {
            continue;
        };
        if source_parent != target_parent {
            continue;
        }
        let Ok(children) = all_children.get(target_parent) else {
            continue;
        };
        move_child_to_parent(
            &mut commands,
            request.tab,
            target_parent,
            children,
            Some(request.target),
            request.after,
        );
    }
}

fn tab_target(id: Option<&str>, tabs: impl IntoIterator<Item = Entity>) -> Option<Entity> {
    let bits = id?.parse::<u64>().ok()?;
    tabs.into_iter().find(|e| e.to_bits() == bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewStackContext;
    use crate::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use crate::window::Main as MainNode;
    use bevy::reflect::{FromReflect, TypeRegistry, serde::TypedReflectDeserializer};

    #[test]
    fn move_tab_request_reorders_siblings() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<MoveTabRequest>()
            .add_systems(Update, handle_move_tab_requests);
        let parent = app.world_mut().spawn_empty().id();
        let first = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(parent)))
            .id();
        let second = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(parent)))
            .id();
        let third = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(parent)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<MoveTabRequest>>()
            .write(MoveTabRequest {
                tab: third,
                target: first,
                after: false,
            });

        app.update();

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(parent)
            .unwrap()
            .iter()
            .collect();
        assert_eq!(children, vec![third, first, second]);
    }
    use serde::de::DeserializeSeed;
    use vmux_command::CommandPlugin;
    use vmux_core::PageOpenRequest;

    #[test]
    fn tab_worktree_deserializes_legacy_metadata_without_checkout_dir() {
        let mut registry = TypeRegistry::default();
        registry.register::<TabWorktree>();
        let registration = registry.get(std::any::TypeId::of::<TabWorktree>()).unwrap();
        let mut deserializer = ron::de::Deserializer::from_str(
            r#"(
                repo_root: "/repo",
                branch: "vmux/task",
                base_ref: "main",
            )"#,
        )
        .unwrap();
        let reflected = TypedReflectDeserializer::new(registration, &registry)
            .deserialize(&mut deserializer)
            .unwrap();
        let metadata = TabWorktree::from_reflect(reflected.as_partial_reflect()).unwrap();

        assert_eq!(
            metadata,
            TabWorktree {
                repo_root: "/repo".into(),
                checkout_dir: String::new(),
                branch: "vmux/task".into(),
                base_ref: "main".into(),
            }
        );
    }

    #[test]
    fn tab_target_uses_event_tab_id() {
        let target = Entity::from_bits(42);
        let other = Entity::from_bits(7);
        let id = target.to_bits().to_string();

        assert_eq!(tab_target(Some(&id), [other, target]), Some(target));
    }

    #[test]
    fn pick_after_close_prefers_right_then_left_neighbor() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let c = Entity::from_bits(3);
        let d = Entity::from_bits(4);
        let tabs = [a, b, c, d];

        assert_eq!(pick_after_close(d, &tabs), Some(c));
        assert_eq!(pick_after_close(b, &tabs), Some(c));
        assert_eq!(pick_after_close(a, &tabs), Some(b));
        assert_eq!(pick_after_close(b, &[a, b]), Some(a));
        assert_eq!(pick_after_close(a, &[a]), None);
    }

    #[test]
    fn active_tab_siblings_are_parent_space_tabs() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space_a = app.world_mut().spawn(crate::space::Space).id();
        let space_b = app.world_mut().spawn(crate::space::Space).id();
        let a1 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(space_a)))
            .id();
        let a2 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(space_a)))
            .id();
        let b1 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(space_b)))
            .id();
        let siblings = app
            .world_mut()
            .run_system_once(
                move |child_of_q: Query<&ChildOf>,
                      all_children: Query<&Children>,
                      tab_q: Query<Entity, With<Tab>>| {
                    active_tab_siblings(a1, &child_of_q, &all_children, &tab_q)
                },
            )
            .unwrap();
        assert_eq!(siblings.len(), 2);
        assert!(siblings.contains(&a1));
        assert!(siblings.contains(&a2));
        assert!(!siblings.contains(&b1));
    }

    #[test]
    fn tab_visibility_sync_runs_before_ui_layout() {
        let source = include_str!("tab.rs");
        let plugin = source
            .split("impl Plugin for TabPlugin")
            .nth(1)
            .and_then(|tail| tail.split("#[derive(Component").next())
            .unwrap_or_default();

        assert!(plugin.contains("sync_tab_visibility.before(UiSystems::Layout)"));
    }

    fn order_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_tab_order);
        app
    }

    #[test]
    fn sync_tab_order_stamps_children_index() {
        let mut app = order_app();
        let main = app.world_mut().spawn(crate::space::Space).id();
        let a = app
            .world_mut()
            .spawn((
                Tab {
                    name: "a".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();
        let b = app
            .world_mut()
            .spawn((
                Tab {
                    name: "b".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();
        let c = app
            .world_mut()
            .spawn((
                Tab {
                    name: "c".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();

        app.update();

        assert_eq!(app.world().get::<Order>(a), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(b), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(2)));
    }

    #[test]
    fn sync_tab_order_updates_after_reorder() {
        let mut app = order_app();
        let main = app.world_mut().spawn(crate::space::Space).id();
        let a = app
            .world_mut()
            .spawn((
                Tab {
                    name: "a".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();
        let b = app
            .world_mut()
            .spawn((
                Tab {
                    name: "b".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();
        let c = app
            .world_mut()
            .spawn((
                Tab {
                    name: "c".into(),
                    startup_dir: None,
                },
                ChildOf(main),
            ))
            .id();

        app.update();

        for e in [a, b, c] {
            app.world_mut().entity_mut(e).remove::<ChildOf>();
        }
        for e in [c, a, b] {
            app.world_mut().entity_mut(e).insert(ChildOf(main));
        }

        app.update();

        assert_eq!(app.world().get::<Order>(c), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(a), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(b), Some(&Order(2)));
    }

    fn test_settings() -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 0.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        }
    }

    #[derive(Resource, Default)]
    struct CollectedSpawns(Vec<PageOpenRequest>);

    fn collect_spawn_requests(
        mut reader: MessageReader<PageOpenRequest>,
        mut collected: ResMut<CollectedSpawns>,
    ) {
        for req in reader.read() {
            collected.0.push(req.clone());
        }
    }

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<crate::LayoutSpawnRequest>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .init_resource::<crate::NewStackContext>()
            .insert_resource(test_settings())
            .init_resource::<CollectedSpawns>()
            .add_systems(
                Update,
                (
                    handle_tab_commands.in_set(ReadAppCommands),
                    crate::window::spawn_requested_tab_layouts,
                    collect_spawn_requests,
                )
                    .chain(),
            );
        app
    }

    fn build_main_and_tab(app: &mut App) -> Entity {
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let main = app.world_mut().spawn(MainNode).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();
        app.insert_resource(crate::settings::EffectiveStartupDir(Some((
            space,
            Some(std::env::current_dir().unwrap()),
        ))));
        app.world_mut().spawn((
            Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            },
            LastActivatedAt::now(),
            ChildOf(space),
        ));
        let _ = window;
        main
    }

    #[test]
    fn open_in_new_tab_explicit_url_spawns_new_tab_with_url() {
        let mut app = build_app();
        build_main_and_tab(&mut app);

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
        assert_eq!(collected.0[0].url, "https://example.com");

        let tab_count = app.world_mut().query::<&Tab>().iter(app.world()).count();
        assert_eq!(tab_count, 2, "expected two tabs after InNewTab");
    }

    #[test]
    fn open_in_new_tab_none_url_falls_back_to_startup() {
        let mut app = build_app();
        app.insert_resource(crate::settings::EffectiveStartupUrl(
            "https://startup.test".into(),
        ));
        build_main_and_tab(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        assert_eq!(collected.0[0].url, "https://startup.test");
    }

    #[test]
    fn open_in_new_tab_none_url_no_startup_opens_prompt() {
        let mut app = build_app();
        build_main_and_tab(&mut app);

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert!(collected.0.is_empty(), "expected no spawn request");
        let ctx = app.world().resource::<NewStackContext>();
        assert!(ctx.stack.is_some());
        assert!(ctx.needs_open);
    }

    #[test]
    fn new_tab_without_configured_startup_dir_does_not_inherit_active_tab_workspace() {
        let mut app = build_app();
        let main = build_main_and_tab(&mut app);
        let space = app
            .world()
            .get::<Children>(main)
            .and_then(|children| children.iter().next())
            .unwrap();
        app.insert_resource(crate::settings::EffectiveStartupDir(Some((space, None))));
        let existing_tab = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .single(app.world())
            .unwrap();
        let existing_dir = std::env::current_dir().unwrap();
        app.world_mut().entity_mut(existing_tab).insert((
            Tab {
                name: "vmux".into(),
                startup_dir: Some(existing_dir.to_string_lossy().into_owned()),
            },
            TabWorkspace {
                project_dir: existing_dir.to_string_lossy().into_owned(),
            },
            TabDirDecided,
        ));

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Tab(TabCommand::New)));

        app.update();

        let tabs: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Tab)>()
            .iter(app.world())
            .collect();
        assert_eq!(tabs.len(), 2);
        let (new_tab_entity, new_tab) = tabs.iter().find(|(_, tab)| tab.name == "Tab 2").unwrap();
        assert_eq!(new_tab.startup_dir, None);
        assert!(app.world().get::<TabWorkspace>(*new_tab_entity).is_none());
        assert!(app.world().get::<TabDirDecided>(*new_tab_entity).is_none());
    }

    #[test]
    fn new_tab_uses_only_configured_startup_dir() {
        let mut app = build_app();
        let main = build_main_and_tab(&mut app);
        let space = app
            .world()
            .get::<Children>(main)
            .and_then(|children| children.iter().next())
            .unwrap();
        let configured = tempfile::tempdir().unwrap();
        app.insert_resource(crate::settings::EffectiveStartupDir(Some((
            space,
            Some(configured.path().to_path_buf()),
        ))));

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Tab(TabCommand::New)));

        app.update();

        let tabs: Vec<_> = app.world_mut().query::<&Tab>().iter(app.world()).collect();
        let new_tab = tabs.iter().find(|tab| tab.name == "Tab 2").unwrap();
        assert_eq!(
            new_tab.startup_dir.as_deref(),
            Some(
                configured
                    .path()
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    #[test]
    fn new_tab_becomes_active_in_single_update() {
        let mut app = build_app();
        app.init_resource::<crate::pane::PendingCursorWarp>()
            .add_plugins((crate::space::SpacePlugin, crate::stack::StackPlugin));
        build_main_and_tab(&mut app);
        let old_tab = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .single(app.world())
            .expect("initial tab");
        app.world_mut()
            .entity_mut(old_tab)
            .insert(vmux_core::Active);
        let old_pane = app
            .world_mut()
            .spawn((crate::pane::Pane, LastActivatedAt(1), ChildOf(old_tab)))
            .id();
        let old_stack = app
            .world_mut()
            .spawn((
                crate::stack::stack_bundle(),
                LastActivatedAt(1),
                ChildOf(old_pane),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));

        app.update();

        let new_tab = app
            .world_mut()
            .query_filtered::<Entity, (With<Tab>, With<vmux_core::Active>)>()
            .single(app.world())
            .expect("one active tab");
        assert_ne!(new_tab, old_tab);
        let focused = app.world().resource::<crate::stack::FocusedStack>();
        assert_eq!(focused.tab, Some(new_tab));
        assert!(focused.pane.is_some_and(|pane| pane != old_pane));
        assert!(focused.stack.is_some_and(|stack| stack != old_stack));
    }

    #[test]
    fn new_tab_parents_under_active_space_container() {
        let mut app = build_app();
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let main = app.world_mut().spawn(MainNode).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<crate::TabLayoutSpawnRequest>>()
            .write(crate::TabLayoutSpawnRequest {
                space,
                primary_window: window,
                name: None,
                startup_dir: Some(std::env::current_dir().unwrap()),
                content: crate::TabLayoutSpawnContent::StartupUrlOrPrompt,
                clear_pending_stack: false,
                focus: true,
            });

        app.update();

        let tab = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .iter(app.world())
            .next()
            .expect("tab spawned");
        assert_eq!(
            app.world().get::<ChildOf>(tab).map(|c| c.parent()),
            Some(space)
        );
        assert!(app.world().get::<crate::space::SpaceId>(tab).is_none());
    }

    #[test]
    fn tabs_close_event_records_recent_tab_close() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, TabPlugin))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_message::<crate::TabLayoutSpawnRequest>();

        let webview = app.world_mut().spawn_empty().id();
        let main = app.world_mut().spawn(MainNode).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Tab 1".into(),
                    startup_dir: None,
                },
                LastActivatedAt::now(),
                ChildOf(main),
            ))
            .id();
        let other_tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Tab 2".into(),
                    startup_dir: None,
                },
                LastActivatedAt(1),
                ChildOf(main),
            ))
            .id();
        app.world_mut().spawn(PrimaryWindow);

        app.world_mut().trigger(BinReceive::<TabsCommandEvent> {
            webview,
            payload: TabsCommandEvent {
                command: "close".to_string(),
                tab_id: Some(tab.to_bits().to_string()),
                ..Default::default()
            },
        });
        app.update();

        assert!(app.world().get_entity(tab).is_err());
        assert!(app.world().get_entity(other_tab).is_ok());
        assert!(app.world().resource::<LastTabCloseAt>().0.is_some());
    }

    #[test]
    fn tabs_close_event_without_target_does_not_record_recent_close() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, TabPlugin))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_message::<crate::TabLayoutSpawnRequest>();
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PrimaryWindow);

        app.world_mut().trigger(BinReceive::<TabsCommandEvent> {
            webview,
            payload: TabsCommandEvent {
                command: "close".to_string(),
                tab_id: None,
                ..Default::default()
            },
        });
        app.update();

        assert!(app.world().resource::<LastTabCloseAt>().0.is_none());
    }

    #[test]
    fn closing_active_rightmost_tab_activates_left_neighbor_not_first() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, crate::space::SpacePlugin))
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<CloseTabRequest>()
            .init_resource::<LastTabCloseAt>()
            .add_systems(
                Update,
                (
                    handle_tab_commands,
                    crate::archive::handle_close_tab_requests,
                )
                    .chain()
                    .in_set(ReadAppCommands),
            );

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(MainNode).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();
        let a = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let c = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(3), ChildOf(space)))
            .id();
        let d = app
            .world_mut()
            .spawn((
                tab_bundle(),
                LastActivatedAt(4),
                vmux_core::Active,
                ChildOf(space),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Tab(TabCommand::Close)));

        app.update();
        app.update();

        assert!(
            app.world().get_entity(d).is_err(),
            "the active rightmost tab must be closed"
        );
        assert!(
            app.world().entity(c).contains::<vmux_core::Active>(),
            "left neighbor must become active after closing the rightmost active tab"
        );
        assert!(
            !app.world().entity(a).contains::<vmux_core::Active>(),
            "closing the rightmost tab must not jump to the first tab"
        );
    }

    #[test]
    fn page_close_command_on_active_rightmost_activates_left_neighbor() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<CloseTabRequest>()
            .init_resource::<LastTabCloseAt>()
            .add_systems(Update, crate::archive::handle_close_tab_requests)
            .add_observer(on_tabs_command_emit);

        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(MainNode).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();
        let a = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let c = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(3), ChildOf(space)))
            .id();
        let d = app
            .world_mut()
            .spawn((
                tab_bundle(),
                LastActivatedAt(4),
                vmux_core::Active,
                ChildOf(space),
            ))
            .id();

        app.world_mut().trigger(BinReceive::<TabsCommandEvent> {
            webview,
            payload: TabsCommandEvent {
                command: "close".to_string(),
                tab_id: Some(d.to_bits().to_string()),
                ..Default::default()
            },
        });
        app.update();
        app.world_mut()
            .run_system_once(crate::active::ensure_active_tab)
            .ok();

        assert!(app.world().get_entity(d).is_err(), "active tab closed");
        assert!(
            app.world().entity(c).contains::<vmux_core::Active>(),
            "left neighbor must be active via the page close observer"
        );
        assert!(
            !app.world().entity(a).contains::<vmux_core::Active>(),
            "must not jump to first tab"
        );
    }

    #[test]
    fn tab_next_activates_and_reveals_target_in_a_single_update() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, crate::space::SpacePlugin))
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<CloseTabRequest>()
            .add_systems(Update, handle_tab_commands.in_set(ReadAppCommands))
            .add_systems(PostUpdate, sync_tab_visibility.before(UiSystems::Layout));

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(MainNode).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();
        let tab_a = app
            .world_mut()
            .spawn((
                tab_bundle(),
                LastActivatedAt(2),
                vmux_core::Active,
                ChildOf(space),
            ))
            .id();
        let tab_b = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(1), ChildOf(space)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Tab(TabCommand::Next)));

        app.update();

        assert!(
            app.world().entity(tab_b).contains::<vmux_core::Active>(),
            "target tab must become Active in the same update as the switch command"
        );
        assert_eq!(
            app.world().get::<Node>(tab_b).unwrap().display,
            Display::Flex,
            "target tab must be revealed in the same update (no one-frame lag)"
        );
        assert_eq!(
            app.world().get::<Node>(tab_a).unwrap().display,
            Display::None,
            "previously active tab must hide in the same update"
        );
    }
}
