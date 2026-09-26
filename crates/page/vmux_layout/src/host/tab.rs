#[cfg(test)]
use crate::event::TabDropPlacement;
use crate::event::{TabActivateRequest, TabCloseRequest, TabCreateRequest, TabReorderRequest};
use crate::{
    TabLayoutSpawnContent, TabLayoutSpawnRequest,
    host::swap::{find_kind_index, move_sibling, resolve_next, resolve_prev, swap_siblings},
};
#[cfg(test)]
use bevy::window::PrimaryWindow;
use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
#[cfg(test)]
use vmux_command::CommandDefinition;
use vmux_command::{CommandDefinitions, CommandInvocation, CommandRequest, CommandTypePlugin};
use vmux_core::Order;
pub use vmux_core::workspace::TabCommandSet;
use vmux_flex::prelude::*;
use vmux_history::LastActivatedAt;

use super::{command::LayoutRequestSet, target::SiblingDirection};

pub struct TabPlugin;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            CommandTypePlugin::<OpenRequest>::default(),
            CommandTypePlugin::<CreateRequest>::default(),
            CommandTypePlugin::<CloseRequest>::default(),
            CommandTypePlugin::<FocusRequest>::default(),
            CommandTypePlugin::<MoveRequest>::default(),
        ))
        .register_type::<Tab>()
        .register_type::<Option<String>>()
        .register_type::<TabWorkspace>()
        .register_type::<TabWorktree>()
        .register_type::<TabDirDecided>()
        .init_resource::<crate::window::FocusedWindow>()
        .add_message::<CloseTabRequest>()
        .add_message::<crate::NewTabRequest>()
        .add_plugins(UiEventPlugin::<(
            TabCreateRequest,
            TabCloseRequest,
            TabActivateRequest,
            TabReorderRequest,
        )>::default())
        .add_observer(on_tab_create_request)
        .add_observer(on_tab_close_request)
        .add_observer(on_tab_activate_request)
        .add_observer(on_tab_reorder_request)
        .add_systems(
            Update,
            (
                handle_open_requests,
                handle_create_requests,
                handle_close_requests,
                handle_focus_requests,
                handle_move_requests,
                handle_new_tab_requests,
            )
                .chain()
                .in_set(LayoutRequestSet::Handle)
                .in_set(TabCommandSet)
                .after(crate::settings::EffectiveStartupDirSet),
        )
        .add_systems(
            Update,
            crate::archive::handle_close_tab_requests
                .in_set(LayoutRequestSet::Handle)
                .after(TabCommandSet)
                .after(crate::stack::StackCommandSet),
        )
        .add_systems(
            PostUpdate,
            sync_tab_visibility.before(LayoutSystems::Layout),
        )
        .add_systems(PostUpdate, sync_tab_order)
        .add_systems(Update, dismiss_launcher_over_new_surfaces);
    }
}

fn dismiss_launcher_over_new_surfaces(
    opened: Query<
        Entity,
        Or<(
            Added<Tab>,
            Added<crate::pane::Pane>,
            Added<crate::stack::Stack>,
        )>,
    >,
    pending_launch: Option<ResMut<vmux_core::launcher::PendingLaunch>>,
) {
    let Some(mut pending_launch) = pending_launch else {
        return;
    };
    if !opened.is_empty() {
        pending_launch.opened_elsewhere();
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub url: Option<String>,
}

impl CommandRequest for OpenRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        CommandDefinitions::from_ron(include_str!("tab.ron")).select(&["open_in_new_tab"])
    }
}

impl TryFrom<&CommandInvocation> for OpenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "open_in_new_tab" => Ok(Self {
                url: invocation.argument("url"),
            }),
            _ => Err(()),
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateRequest;

impl CommandRequest for CreateRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        CommandDefinitions::from_ron(include_str!("tab.ron")).select(&["new_task"])
    }
}

impl TryFrom<&CommandInvocation> for CreateRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "new_task").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest;

impl CommandRequest for CloseRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        CommandDefinitions::from_ron(include_str!("tab.ron")).select(&["close_tab"])
    }
}

impl TryFrom<&CommandInvocation> for CloseRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "close_tab").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusRequest(pub TabFocus);

impl CommandRequest for FocusRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        CommandDefinitions::from_ron(include_str!("tab.ron")).select(&[
            "next_tab",
            "prev_tab",
            "tab_select_1",
            "tab_select_2",
            "tab_select_3",
            "tab_select_4",
            "tab_select_5",
            "tab_select_6",
            "tab_select_7",
            "tab_select_8",
            "tab_select_last",
        ])
    }
}

impl TryFrom<&CommandInvocation> for FocusRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "next_tab" => Ok(Self(TabFocus::Sibling(SiblingDirection::Next))),
            "prev_tab" => Ok(Self(TabFocus::Sibling(SiblingDirection::Previous))),
            "tab_select_1" => Ok(Self(TabFocus::Index(0))),
            "tab_select_2" => Ok(Self(TabFocus::Index(1))),
            "tab_select_3" => Ok(Self(TabFocus::Index(2))),
            "tab_select_4" => Ok(Self(TabFocus::Index(3))),
            "tab_select_5" => Ok(Self(TabFocus::Index(4))),
            "tab_select_6" => Ok(Self(TabFocus::Index(5))),
            "tab_select_7" => Ok(Self(TabFocus::Index(6))),
            "tab_select_8" => Ok(Self(TabFocus::Index(7))),
            "tab_select_last" => Ok(Self(TabFocus::Last)),
            _ => Err(()),
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveRequest(pub SiblingDirection);

impl CommandRequest for MoveRequest {
    fn definitions() -> Vec<vmux_command::CommandDefinition> {
        CommandDefinitions::from_ron(include_str!("tab.ron"))
            .select(&["swap_tab_prev", "swap_tab_next"])
    }
}

impl TryFrom<&CommandInvocation> for MoveRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "swap_tab_prev" => Ok(Self(SiblingDirection::Previous)),
            "swap_tab_next" => Ok(Self(SiblingDirection::Next)),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabFocus {
    Sibling(SiblingDirection),
    Index(usize),
    Last,
}

#[derive(Message, Clone, Copy)]
pub struct CloseTabRequest {
    pub tab: Entity,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct Tab {
    pub name: String,
    pub startup_dir: Option<String>,
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabWorkspace {
    pub project_dir: String,
}

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

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct TabWorktreeUnavailable {
    pub message: String,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::tab"]
#[require(Save)]
pub struct TabDirDecided;

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

#[derive(Event, Clone, Copy, Debug)]
pub struct TabClosed;

pub fn tab_bundle() -> impl Bundle {
    (
        Tab::default(),
        Transform::default(),
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

fn handle_open_requests(
    mut requests: MessageReader<OpenRequest>,
    tabs: Query<Entity, With<Tab>>,
    focused_window: Res<crate::window::FocusedWindow>,
    effective_startup_url: Option<Res<vmux_core::EffectiveStartupUrl>>,
    effective_startup_dir: Option<Res<crate::settings::EffectiveStartupDir>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
) {
    for request in requests.read() {
        let Some(window) = focused_window.0 else {
            continue;
        };
        let Some((space, startup_dir)) = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.clone())
        else {
            continue;
        };
        let requested = request
            .url
            .as_deref()
            .filter(|url| !url.is_empty())
            .or_else(|| {
                effective_startup_url
                    .as_deref()
                    .map(|startup| startup.0.as_str())
                    .filter(|startup| !startup.is_empty())
            });
        let content = match requested {
            Some(url) => TabLayoutSpawnContent::Url {
                url: url.to_string(),
                pending_prompt: None,
            },
            None => TabLayoutSpawnContent::StartupUrlOrPrompt,
        };
        layout_requests.write(TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: Some(format!("Tab {}", tabs.iter().count() + 1)),
            startup_dir,
            content,
            clear_pending_stack: true,
            focus: true,
        });
    }
}

fn handle_create_requests(
    mut requests: MessageReader<CreateRequest>,
    tabs: Query<Entity, With<Tab>>,
    focused_window: Res<crate::window::FocusedWindow>,
    effective_startup_dir: Option<Res<crate::settings::EffectiveStartupDir>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
) {
    for _ in requests.read() {
        let Some(window) = focused_window.0 else {
            continue;
        };
        let Some((space, startup_dir)) = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.clone())
        else {
            continue;
        };
        layout_requests.write(TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: Some(format!("Tab {}", tabs.iter().count() + 1)),
            startup_dir,
            content: TabLayoutSpawnContent::StartupUrlOrPrompt,
            clear_pending_stack: true,
            focus: true,
        });
    }
}

fn handle_close_requests(
    mut requests: MessageReader<CloseRequest>,
    active_tab: crate::stack::ActiveTabParam,
    mut close_requests: MessageWriter<CloseTabRequest>,
) {
    for _ in requests.read() {
        let Some(tab) = active_tab.get() else {
            continue;
        };
        close_requests.write(CloseTabRequest { tab });
    }
}

fn handle_focus_requests(
    mut requests: MessageReader<FocusRequest>,
    active_tab: crate::stack::ActiveTabParam,
    tabs: Query<Entity, With<Tab>>,
    child_of: Query<&ChildOf>,
    children: Query<&Children>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some(active) = active_tab.get() else {
            continue;
        };
        let siblings = active_tab_siblings(active, &child_of, &children, &tabs);
        if siblings.is_empty() {
            continue;
        }
        let target_index = match request.0 {
            TabFocus::Sibling(direction) => {
                if siblings.len() <= 1 {
                    continue;
                }
                let Some(index) = siblings.iter().position(|entity| *entity == active) else {
                    continue;
                };
                if direction == SiblingDirection::Next {
                    (index + 1) % siblings.len()
                } else {
                    (index + siblings.len() - 1) % siblings.len()
                }
            }
            TabFocus::Index(index) => index,
            TabFocus::Last => siblings.len() - 1,
        };
        if target_index >= siblings.len() {
            continue;
        }
        let target = siblings[target_index];
        if target != active {
            commands.entity(target).insert(LastActivatedAt::now());
        }
    }
}

fn handle_move_requests(
    mut requests: MessageReader<MoveRequest>,
    active_tab: crate::stack::ActiveTabParam,
    tabs: Query<Entity, With<Tab>>,
    child_of: Query<&ChildOf>,
    children: Query<&Children>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some(active) = active_tab.get() else {
            continue;
        };
        let Ok(child_of) = child_of.get(active) else {
            continue;
        };
        let parent = child_of.get();
        let Ok(children) = children.get(parent) else {
            continue;
        };
        let positions = children
            .iter()
            .enumerate()
            .filter(|(_, entity)| tabs.contains(*entity))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some(active_index) = find_kind_index(active, children, &positions) else {
            continue;
        };
        let pair = if request.0 == SiblingDirection::Previous {
            resolve_prev(active_index)
        } else {
            resolve_next(active_index, positions.len())
        };
        if let Some((left, right)) = pair {
            swap_siblings(&mut commands, parent, children, &positions, left, right);
        }
    }
}

fn handle_new_tab_requests(
    mut requests: MessageReader<crate::NewTabRequest>,
    tabs: Query<Entity, With<Tab>>,
    focused_window: Res<crate::window::FocusedWindow>,
    effective_startup_dir: Option<Res<crate::settings::EffectiveStartupDir>>,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
) {
    for request in requests.read() {
        let Some(window) = focused_window.0 else {
            continue;
        };
        let Some((space, startup_dir)) = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.clone())
        else {
            continue;
        };
        let name = format!("Tab {}", tabs.iter().count() + 1);
        layout_requests.write(TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: Some(name),
            startup_dir,
            content: TabLayoutSpawnContent::Url {
                url: request.url.clone(),
                pending_prompt: request.pending_prompt.clone(),
            },
            clear_pending_stack: true,
            focus: true,
        });
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

pub fn pick_after_close(active: Entity, siblings: &[Entity]) -> Option<Entity> {
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
            Visibility::Visible
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

fn on_tab_create_request(
    _trigger: On<UiInput<TabCreateRequest>>,
    mut requests: MessageWriter<OpenRequest>,
) {
    requests.write(OpenRequest { url: None });
}

fn on_tab_close_request(
    trigger: On<UiInput<TabCloseRequest>>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    active_tab_param: crate::stack::ActiveTabParam,
    mut close_requests: MessageWriter<CloseTabRequest>,
) {
    let active_tab = active_tab_param.get();
    let target = tab_target(
        trigger.event().payload.tab_id.as_deref(),
        tabs.iter().map(|(entity, _)| entity),
    )
    .or(active_tab);
    let Some(target) = target else { return };
    close_requests.write(CloseTabRequest { tab: target });
}

fn on_tab_activate_request(
    trigger: On<UiInput<TabActivateRequest>>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    mut commands: Commands,
) {
    let Ok(bits) = trigger.event().payload.tab_id.parse::<u64>() else {
        return;
    };
    let Some((target, _)) = tabs.iter().find(|(entity, _)| entity.to_bits() == bits) else {
        return;
    };
    commands.entity(target).insert(LastActivatedAt::now());
}

fn on_tab_reorder_request(
    trigger: On<UiInput<TabReorderRequest>>,
    tabs: Query<(Entity, &LastActivatedAt), With<Tab>>,
    child_of: Query<&ChildOf>,
    children: Query<&Children>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some(source) = tab_target(
        Some(request.tab_id.as_str()),
        tabs.iter().map(|(entity, _)| entity),
    ) else {
        return;
    };
    let Some(target) = tab_target(
        Some(request.target_tab_id.as_str()),
        tabs.iter().map(|(entity, _)| entity),
    ) else {
        return;
    };
    if source == target {
        return;
    }
    let Ok(source_parent) = child_of.get(source) else {
        return;
    };
    let Ok(target_parent) = child_of.get(target) else {
        return;
    };
    if source_parent.parent() != target_parent.parent() {
        return;
    }
    let parent = source_parent.parent();
    let Ok(siblings) = children.get(parent) else {
        return;
    };
    let kind_positions = siblings
        .iter()
        .enumerate()
        .filter(|(_, entity)| tabs.contains(*entity))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let Some(from) = find_kind_index(source, siblings, &kind_positions) else {
        return;
    };
    let Some(to) = find_kind_index(target, siblings, &kind_positions) else {
        return;
    };
    let destination = request
        .drop_placement
        .destination(from, to, kind_positions.len());
    move_sibling(
        &mut commands,
        parent,
        siblings,
        &kind_positions,
        from,
        destination,
    );
}

fn tab_target(id: Option<&str>, tabs: impl IntoIterator<Item = Entity>) -> Option<Entity> {
    let bits = id?.parse::<u64>().ok()?;
    tabs.into_iter().find(|e| e.to_bits() == bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use crate::window::Main as MainNode;
    use bevy::reflect::{FromReflect, TypeRegistry, serde::TypedReflectDeserializer};
    use serde::de::DeserializeSeed;
    use vmux_command::CommandPlugin;
    use vmux_core::PageOpenRequest;

    #[test]
    fn tab_mcp_definition_dispatches_to_the_typed_request() {
        let mut definitions = Vec::new();
        definitions.extend(OpenRequest::definitions());
        definitions.extend(CreateRequest::definitions());
        definitions.extend(CloseRequest::definitions());
        definitions.extend(FocusRequest::definitions());
        definitions.extend(MoveRequest::definitions());
        let tools = definitions
            .iter()
            .filter_map(CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["open_in_new_tab"],
        );
        let invocation = CommandInvocation::new(Entity::PLACEHOLDER, "open_in_new_tab")
            .with_arguments(serde_json::json!({"url": "https://vmux.ai"}));
        assert!(OpenRequest::try_from(&invocation).is_ok());
    }

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

    #[derive(Resource, Default)]
    struct ClosedTabs(usize);

    fn record_closed_tab(_trigger: On<TabClosed>, mut closed: ResMut<ClosedTabs>) {
        closed.0 += 1;
    }

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
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenRequest>()
            .add_message::<CreateRequest>()
            .add_message::<CloseRequest>()
            .add_message::<FocusRequest>()
            .add_message::<MoveRequest>()
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<crate::NewTabRequest>()
            .add_message::<CloseTabRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .init_resource::<crate::PendingLaunch>()
            .init_resource::<crate::window::FocusedWindow>()
            .insert_resource(test_settings())
            .init_resource::<CollectedSpawns>()
            .add_systems(
                Update,
                (
                    handle_open_requests,
                    handle_create_requests,
                    handle_close_requests,
                    handle_focus_requests,
                    handle_move_requests,
                    handle_new_tab_requests,
                    crate::window::spawn_requested_tab_layouts,
                    collect_spawn_requests,
                )
                    .chain(),
            );
        app
    }

    fn build_main_and_tab(app: &mut App) -> Entity {
        let window = app.world_mut().spawn(PrimaryWindow).id();
        app.insert_resource(crate::window::FocusedWindow(Some(window)));
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
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("https://example.com".into()),
            });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        assert_eq!(collected.0[0].url, "https://example.com");

        let tab_count = app.world_mut().query::<&Tab>().iter(app.world()).count();
        assert_eq!(tab_count, 2, "expected two tabs after InNewTab");
    }

    #[test]
    fn a_new_tab_carries_its_pending_prompt_onto_the_stack() {
        let mut app = build_app();
        build_main_and_tab(&mut app);

        app.world_mut()
            .resource_mut::<Messages<crate::NewTabRequest>>()
            .write(crate::NewTabRequest {
                url: "vmux://sessions/codex/cli".to_string(),
                pending_prompt: Some("continue from my phone".to_string()),
            });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        assert_eq!(collected.0[0].url, "vmux://sessions/codex/cli");
        let prompts = app
            .world_mut()
            .query::<&vmux_core::PendingPrompt>()
            .iter(app.world())
            .map(|prompt| prompt.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(prompts, vec!["continue from my phone"]);
        assert_eq!(app.world_mut().query::<&Tab>().iter(app.world()).count(), 2);
    }

    #[test]
    fn open_in_new_tab_none_url_falls_back_to_startup() {
        let mut app = build_app();
        app.insert_resource(vmux_core::EffectiveStartupUrl(
            "https://startup.test".into(),
        ));
        build_main_and_tab(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest { url: None });

        app.update();

        let collected = app.world().resource::<CollectedSpawns>();
        assert_eq!(collected.0.len(), 1, "expected one spawn request");
        assert_eq!(collected.0[0].url, "https://startup.test");
    }

    #[test]
    fn open_in_new_tab_none_url_opens_the_start_page() {
        let mut app = build_app();
        build_main_and_tab(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest { url: None });

        app.update();

        let opened = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            opened.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(),
            [vmux_core::EffectiveStartupUrl::START_PAGE]
        );
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
            .resource_mut::<Messages<CreateRequest>>()
            .write(CreateRequest);

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
            .resource_mut::<Messages<CreateRequest>>()
            .write(CreateRequest);

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
            .add_plugins((crate::space::SpaceLayoutPlugin, crate::stack::StackPlugin));
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
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest { url: None });

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
    fn tabs_close_event_emits_tab_closed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, TabPlugin))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .init_resource::<ClosedTabs>()
            .add_observer(record_closed_tab);

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

        app.world_mut().trigger(UiInput::<TabCloseRequest> {
            webview,
            payload: TabCloseRequest {
                tab_id: Some(tab.to_bits().to_string()),
            },
        });
        app.update();

        assert!(app.world().get_entity(tab).is_err());
        assert!(app.world().get_entity(other_tab).is_ok());
        assert_eq!(app.world().resource::<ClosedTabs>().0, 1);
    }

    #[test]
    fn tabs_close_event_without_target_does_not_emit_tab_closed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, TabPlugin))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .init_resource::<ClosedTabs>()
            .add_observer(record_closed_tab);
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PrimaryWindow);

        app.world_mut().trigger(UiInput::<TabCloseRequest> {
            webview,
            payload: TabCloseRequest { tab_id: None },
        });
        app.update();

        assert_eq!(app.world().resource::<ClosedTabs>().0, 0);
    }

    #[test]
    fn tabs_reorder_event_moves_the_source_to_the_requested_index() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, TabPlugin))
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_message::<crate::TabLayoutSpawnRequest>();
        let webview = app.world_mut().spawn_empty().id();
        let space = app.world_mut().spawn(crate::space::Space).id();
        let first = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let second = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(2), ChildOf(space)))
            .id();
        let third = app
            .world_mut()
            .spawn((tab_bundle(), LastActivatedAt(3), ChildOf(space)))
            .id();

        app.world_mut().trigger(UiInput::<TabReorderRequest> {
            webview,
            payload: TabReorderRequest {
                tab_id: first.to_bits().to_string(),
                target_tab_id: third.to_bits().to_string(),
                drop_placement: TabDropPlacement::After,
            },
        });
        app.update();

        assert_eq!(
            app.world()
                .get::<Children>(space)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [second, third, first]
        );
    }

    #[test]
    fn closing_active_rightmost_tab_activates_left_neighbor_not_first() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::space::SpaceLayoutPlugin))
            .add_message::<CloseRequest>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<crate::NewTabRequest>()
            .add_message::<CloseTabRequest>()
            .add_systems(
                Update,
                (
                    handle_close_requests,
                    crate::archive::handle_close_tab_requests,
                )
                    .chain(),
            );

        let window = app.world_mut().spawn(PrimaryWindow).id();
        app.insert_resource(crate::window::FocusedWindow(Some(window)));
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
            .resource_mut::<Messages<CloseRequest>>()
            .write(CloseRequest);

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
            .add_systems(Update, crate::archive::handle_close_tab_requests)
            .add_observer(on_tab_close_request);

        let webview = app.world_mut().spawn_empty().id();
        let window = app.world_mut().spawn(PrimaryWindow).id();
        app.insert_resource(crate::window::FocusedWindow(Some(window)));
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

        app.world_mut().trigger(UiInput::<TabCloseRequest> {
            webview,
            payload: TabCloseRequest {
                tab_id: Some(d.to_bits().to_string()),
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
        app.add_plugins((
            MinimalPlugins,
            CommandPlugin,
            crate::host::command::LayoutRequestPlugin,
            crate::space::SpaceLayoutPlugin,
            TabPlugin,
        ))
        .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
        .add_message::<crate::TabLayoutSpawnRequest>()
        .add_message::<PageOpenRequest>();

        let window = app.world_mut().spawn(PrimaryWindow).id();
        app.insert_resource(crate::window::FocusedWindow(Some(window)));
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
            .resource_mut::<Messages<FocusRequest>>()
            .write(FocusRequest(TabFocus::Sibling(SiblingDirection::Next)));

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
