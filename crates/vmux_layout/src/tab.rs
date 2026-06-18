use crate::event::TabsCommandEvent;
use crate::{
    TabLayoutSpawnContent, TabLayoutSpawnRequest,
    swap::{find_kind_index, resolve_next, resolve_prev, swap_siblings},
    window::Main as MainNode,
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

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Tab>()
            .init_resource::<LastTabCloseAt>()
            .add_plugins(BinEventEmitterPlugin::<(TabsCommandEvent,)>::default())
            .add_observer(on_tabs_command_emit)
            .add_systems(
                Update,
                handle_tab_commands
                    .in_set(ReadAppCommands)
                    .in_set(TabCommandSet),
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
    tab_q: Query<Entity, With<Tab>>,
    tab_space: Query<&crate::space::SpaceId>,
    main_q: Query<Entity, With<MainNode>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let active_tab = tabs.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);

        match cmd {
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url })) => {
                let Ok(main) = main_q.single() else { continue };
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
                    main,
                    primary_window: *primary_window,
                    name: Some(name),
                    content,
                    clear_pending_stack: true,
                    focus: true,
                });
            }
            AppCommand::Layout(LayoutCommand::Tab(tab_cmd)) => match tab_cmd {
                TabCommand::Close => {
                    let Some(active) = active_tab else { continue };
                    let siblings =
                        active_tab_siblings(active, &child_of_q, &all_children, &tab_q, &tab_space);
                    if siblings.len() <= 1 {
                        let Ok(main) = main_q.single() else { continue };
                        layout_requests.write(TabLayoutSpawnRequest {
                            main,
                            primary_window: *primary_window,
                            name: Some(format!("Tab {}", tabs.iter().count() + 1)),
                            content: TabLayoutSpawnContent::StartupUrlOrPrompt,
                            clear_pending_stack: true,
                            focus: true,
                        });
                    } else if let Some(next) = pick_after_close(active, &siblings) {
                        commands.entity(next).insert(LastActivatedAt::now());
                    }
                    commands.entity(active).despawn();
                }
                TabCommand::Next | TabCommand::Previous => {
                    let Some(active) = active_tab else { continue };
                    let siblings =
                        active_tab_siblings(active, &child_of_q, &all_children, &tab_q, &tab_space);
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
                    let siblings =
                        active_tab_siblings(active, &child_of_q, &all_children, &tab_q, &tab_space);
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
    tab_space: &Query<&crate::space::SpaceId>,
) -> Vec<Entity> {
    let Ok(co) = child_of_q.get(active) else {
        return vec![active];
    };
    let parent = co.get();
    let Ok(children) = all_children.get(parent) else {
        return vec![active];
    };
    let active_space = tab_space.get(active).ok();
    children
        .iter()
        .filter(|e| tab_q.contains(*e))
        .filter(|e| crate::space::same_space(tab_space.get(*e).ok(), active_space))
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

fn sync_tab_visibility(
    mut tabs: Query<(Entity, &LastActivatedAt, &mut Node, &mut Visibility), With<Tab>>,
) {
    let active = tabs
        .iter()
        .max_by_key(|(_, ts, _, _)| ts.0)
        .map(|(e, _, _, _)| e);
    for (entity, _, mut node, mut vis) in &mut tabs {
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

fn sync_tab_order(
    main_children: Query<&Children, (With<MainNode>, Changed<Children>)>,
    tab_q: Query<(), With<Tab>>,
    tab_space: Query<&crate::space::SpaceId>,
    mut order_q: Query<&mut Order>,
    mut commands: Commands,
) {
    for children in &main_children {
        let mut counters: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for child in children.iter() {
            if !tab_q.contains(child) {
                continue;
            }
            let key = tab_space
                .get(child)
                .map(|space| space.0.clone())
                .unwrap_or_default();
            let idx = *counters.get(&key).unwrap_or(&0);
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
            counters.insert(key, idx + 1);
        }
    }
}

fn on_tabs_command_emit(
    trigger: On<BinReceive<TabsCommandEvent>>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
    tab_space: Query<&crate::space::SpaceId>,
    main_q: Query<Entity, With<MainNode>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    mut messages: ResMut<Messages<AppCommand>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut last_tab_close: ResMut<LastTabCloseAt>,
    mut commands: Commands,
) {
    let evt = &trigger.event().payload;
    let active_tab = tabs.iter().max_by_key(|(_, ts)| ts.0).map(|(e, _)| e);
    match evt.command.as_str() {
        "new" => {
            messages.write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None },
            )));
        }
        "close" => {
            last_tab_close.0 = Some(Instant::now());
            let target = tab_target(evt.tab_id.as_deref(), tabs.iter().map(|(entity, _)| entity))
                .or(active_tab);
            let Some(target) = target else { return };
            let siblings =
                active_tab_siblings(target, &child_of_q, &all_children, &tab_q, &tab_space);
            if siblings.len() <= 1 {
                let Ok(main) = main_q.single() else { return };
                layout_requests.write(TabLayoutSpawnRequest {
                    main,
                    primary_window: *primary_window,
                    name: Some(format!("Tab {}", tabs.iter().count() + 1)),
                    content: TabLayoutSpawnContent::StartupUrlOrPrompt,
                    clear_pending_stack: true,
                    focus: true,
                });
            } else if active_tab == Some(target)
                && let Some(next) = pick_after_close(target, &siblings)
            {
                commands.entity(next).insert(LastActivatedAt::now());
            }
            commands.entity(target).despawn();
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
        _ => {}
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
    use vmux_command::CommandPlugin;
    use vmux_core::PageOpenRequest;

    #[test]
    fn tab_target_uses_event_tab_id() {
        let target = Entity::from_bits(42);
        let other = Entity::from_bits(7);
        let id = target.to_bits().to_string();

        assert_eq!(tab_target(Some(&id), [other, target]), Some(target));
    }

    #[test]
    fn active_tab_siblings_scopes_to_space() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let main = app.world_mut().spawn(MainNode).id();
        let a1 = app
            .world_mut()
            .spawn((
                Tab::default(),
                crate::space::SpaceId("a".into()),
                ChildOf(main),
            ))
            .id();
        let a2 = app
            .world_mut()
            .spawn((
                Tab::default(),
                crate::space::SpaceId("a".into()),
                ChildOf(main),
            ))
            .id();
        let b1 = app
            .world_mut()
            .spawn((
                Tab::default(),
                crate::space::SpaceId("b".into()),
                ChildOf(main),
            ))
            .id();
        let siblings = app
            .world_mut()
            .run_system_once(
                move |child_of_q: Query<&ChildOf>,
                      all_children: Query<&Children>,
                      tab_q: Query<Entity, With<Tab>>,
                      tab_space: Query<&crate::space::SpaceId>| {
                    active_tab_siblings(a1, &child_of_q, &all_children, &tab_q, &tab_space)
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
        let main = app.world_mut().spawn(MainNode).id();
        let a = app
            .world_mut()
            .spawn((Tab { name: "a".into() }, ChildOf(main)))
            .id();
        let b = app
            .world_mut()
            .spawn((Tab { name: "b".into() }, ChildOf(main)))
            .id();
        let c = app
            .world_mut()
            .spawn((Tab { name: "c".into() }, ChildOf(main)))
            .id();

        app.update();

        assert_eq!(app.world().get::<Order>(a), Some(&Order(0)));
        assert_eq!(app.world().get::<Order>(b), Some(&Order(1)));
        assert_eq!(app.world().get::<Order>(c), Some(&Order(2)));
    }

    #[test]
    fn sync_tab_order_updates_after_reorder() {
        let mut app = order_app();
        let main = app.world_mut().spawn(MainNode).id();
        let a = app
            .world_mut()
            .spawn((Tab { name: "a".into() }, ChildOf(main)))
            .id();
        let b = app
            .world_mut()
            .spawn((Tab { name: "b".into() }, ChildOf(main)))
            .id();
        let c = app
            .world_mut()
            .spawn((Tab { name: "c".into() }, ChildOf(main)))
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
            .add_message::<PageOpenRequest>()
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
        app.world_mut().spawn((
            Tab {
                name: "Tab 1".into(),
            },
            LastActivatedAt::now(),
            ChildOf(main),
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
    fn tabs_close_event_records_recent_tab_close() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, TabPlugin))
            .add_message::<crate::TabLayoutSpawnRequest>();

        let webview = app.world_mut().spawn_empty().id();
        let main = app.world_mut().spawn(MainNode).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Tab 1".into(),
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
            },
        });
        app.world_mut().flush();

        assert!(app.world().get_entity(tab).is_err());
        assert!(app.world().get_entity(other_tab).is_ok());
        assert!(app.world().resource::<LastTabCloseAt>().0.is_some());
    }
}
