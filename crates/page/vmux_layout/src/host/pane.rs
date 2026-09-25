mod arrangement;
mod close;
mod focus;
mod identity;
mod open;
mod resize;
mod tree;

use crate::host::zoom::PaneZoomPlugin;
pub use crate::host::zoom::Zoomed;
use crate::stack::Stack;
#[cfg(test)]
use crate::{stack::stack_bundle, tab::Tab};
use bevy::prelude::*;
#[cfg(test)]
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    window::PrimaryWindow,
};
use moonshine_save::prelude::*;
use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
use vmux_command::{CommandDefinition, CommandInvocation, CommandRequest, CommandTypePlugin};
#[cfg(test)]
use vmux_core::{PageOpenRequest, PageOpenTarget};
#[cfg(test)]
use vmux_flex::prelude::*;
#[cfg(test)]
use vmux_history::LastActivatedAt;

#[cfg(test)]
use super::command::LayoutRequestPlugin;
use super::target::SiblingDirection;
use arrangement::ArrangementPlugin;
use close::ClosePlugin;
pub use close::{ForcePaneClose, PendingPaneClose};
use focus::FocusPlugin;
pub use focus::{PaneHoverIntent, PendingCursorWarp, pane_hover_cursor_position};
use identity::IdentityPlugin;
pub use identity::{PaneId, SpawnCounter, SpawnSeq};
use open::OpenPlugin;
#[cfg(test)]
use open::{BesideOpenPlugin, DirectionalOpenPlugin};
pub use open::{OpenBesideRequest, PlacementCtx, resolve_spiral_pane, resolve_split_anchor_pane};
use resize::ResizePlugin;
pub use resize::{PaneDrag, PaneSize, PaneSplitGaps, apply_pane_split_gaps, pane_split_gaps};
use tree::TreePlugin;
pub use tree::{
    Pane, PaneSplit, PaneSplitDirection, direction_to_split, first_leaf_descendant,
    leaf_pane_bundle, split_leaf_into_two, split_or_extend, split_root_bundle,
};
pub(crate) use tree::{set_split_direction, spawn_split_from_leaf};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ArrangementSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneFocus {
    Next,
    Direction(PaneDirection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneArrangement {
    Swap(SiblingDirection),
    Rotate(SiblingDirection),
    Mirror(Option<PaneSplitDirection>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneResize {
    Equalize,
    Direction(PaneDirection),
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub direction: PaneDirection,
    pub target: PaneTarget,
    pub mode: PaneOpenMode,
    pub url: Option<String>,
}

impl CommandRequest for OpenRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("open_in_pane_top", "Open in Pane Top", "Browser > Open")
                .direct("Super+Shift+K"),
            CommandDefinition::new("open_in_pane_right", "Open in Pane Right", "Browser > Open")
                .alias("split_v")
                .direct("Super+Shift+L")
                .chord("Ctrl+b, %"),
            CommandDefinition::new(
                "open_in_pane_bottom",
                "Open in Pane Bottom",
                "Browser > Open",
            )
            .alias("split_h")
            .direct("Super+Shift+J")
            .chord("Ctrl+b, \""),
            CommandDefinition::new("open_in_pane_left", "Open in Pane Left", "Browser > Open")
                .direct("Super+Shift+H"),
        ]
    }
}

impl TryFrom<&CommandInvocation> for OpenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "open_in_pane_top" => Ok(Self {
                direction: PaneDirection::Top,
                target: invocation
                    .argument("target")
                    .unwrap_or(PaneTarget::NewSplit),
                mode: invocation
                    .argument("mode")
                    .unwrap_or(PaneOpenMode::NewStack),
                url: invocation.argument("url"),
            }),
            "open_in_pane_right" => Ok(Self {
                direction: PaneDirection::Right,
                target: invocation
                    .argument("target")
                    .unwrap_or(PaneTarget::NewSplit),
                mode: invocation
                    .argument("mode")
                    .unwrap_or(PaneOpenMode::NewStack),
                url: invocation.argument("url"),
            }),
            "open_in_pane_bottom" => Ok(Self {
                direction: PaneDirection::Bottom,
                target: invocation
                    .argument("target")
                    .unwrap_or(PaneTarget::NewSplit),
                mode: invocation
                    .argument("mode")
                    .unwrap_or(PaneOpenMode::NewStack),
                url: invocation.argument("url"),
            }),
            "open_in_pane_left" => Ok(Self {
                direction: PaneDirection::Left,
                target: invocation
                    .argument("target")
                    .unwrap_or(PaneTarget::NewSplit),
                mode: invocation
                    .argument("mode")
                    .unwrap_or(PaneOpenMode::NewStack),
                url: invocation.argument("url"),
            }),
            _ => Err(()),
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest;

impl CommandRequest for CloseRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![CommandDefinition::new("close_pane", "Close Pane", "Layout > Pane").chord("Ctrl+b, x")]
    }
}

impl TryFrom<&CommandInvocation> for CloseRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "close_pane").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusRequest(pub PaneFocus);

impl CommandRequest for FocusRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("toggle_pane", "Next Pane", "Layout > Pane")
                .hidden()
                .chord("Ctrl+b, o"),
            CommandDefinition::new("select_pane_left", "Select Left Pane", "Layout > Pane")
                .chord("Ctrl+b, h")
                .chord("Ctrl+b, ArrowLeft"),
            CommandDefinition::new("select_pane_right", "Select Right Pane", "Layout > Pane")
                .chord("Ctrl+b, l")
                .chord("Ctrl+b, ArrowRight"),
            CommandDefinition::new("select_pane_up", "Select Up Pane", "Layout > Pane")
                .chord("Ctrl+b, k")
                .chord("Ctrl+b, ArrowUp"),
            CommandDefinition::new("select_pane_down", "Select Down Pane", "Layout > Pane")
                .chord("Ctrl+b, j")
                .chord("Ctrl+b, ArrowDown"),
        ]
    }
}

impl TryFrom<&CommandInvocation> for FocusRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "toggle_pane" => Ok(Self(PaneFocus::Next)),
            "select_pane_left" => Ok(Self(PaneFocus::Direction(PaneDirection::Left))),
            "select_pane_right" => Ok(Self(PaneFocus::Direction(PaneDirection::Right))),
            "select_pane_up" => Ok(Self(PaneFocus::Direction(PaneDirection::Top))),
            "select_pane_down" => Ok(Self(PaneFocus::Direction(PaneDirection::Bottom))),
            _ => Err(()),
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrangeRequest(pub PaneArrangement);

impl CommandRequest for ArrangeRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("swap_pane_prev", "Swap Pane Previous", "Layout > Pane")
                .chord("Ctrl+b, {"),
            CommandDefinition::new("swap_pane_next", "Swap Pane Next", "Layout > Pane")
                .chord("Ctrl+b, }"),
            CommandDefinition::new("rotate_forward", "Rotate Forward", "Layout > Pane")
                .hidden()
                .chord("Ctrl+b, r")
                .chord("Ctrl+b, Ctrl+o"),
            CommandDefinition::new("rotate_backward", "Rotate Backward", "Layout > Pane")
                .hidden()
                .chord("Ctrl+b, Shift+r")
                .chord("Ctrl+b, Alt+o"),
            CommandDefinition::new("mirror_panes", "Mirror Panes", "Layout > Pane")
                .chord("Ctrl+b, m"),
            CommandDefinition::new(
                "mirror_panes_horizontal",
                "Mirror Panes Horizontally",
                "Layout > Pane",
            )
            .chord("Ctrl+b, Alt+h"),
            CommandDefinition::new(
                "mirror_panes_vertical",
                "Mirror Panes Vertically",
                "Layout > Pane",
            )
            .chord("Ctrl+b, Alt+v"),
        ]
    }
}

impl TryFrom<&CommandInvocation> for ArrangeRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "swap_pane_prev" => Ok(Self(PaneArrangement::Swap(SiblingDirection::Previous))),
            "swap_pane_next" => Ok(Self(PaneArrangement::Swap(SiblingDirection::Next))),
            "rotate_forward" => Ok(Self(PaneArrangement::Rotate(SiblingDirection::Next))),
            "rotate_backward" => Ok(Self(PaneArrangement::Rotate(SiblingDirection::Previous))),
            "mirror_panes" => Ok(Self(PaneArrangement::Mirror(None))),
            "mirror_panes_horizontal" => {
                Ok(Self(PaneArrangement::Mirror(Some(PaneSplitDirection::Row))))
            }
            "mirror_panes_vertical" => Ok(Self(PaneArrangement::Mirror(Some(
                PaneSplitDirection::Column,
            )))),
            _ => Err(()),
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeRequest(pub PaneResize);

impl CommandRequest for ResizeRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("equalize_pane_size", "Equalize Pane Size", "Layout > Pane")
                .chord("Ctrl+b, Shift+e")
                .chord("Ctrl+b, ="),
            CommandDefinition::new("resize_pane_left", "Resize Pane Left", "Layout > Pane")
                .chord("Ctrl+b, Ctrl+ArrowLeft")
                .chord("Ctrl+b, Alt+ArrowLeft"),
            CommandDefinition::new("resize_pane_right", "Resize Pane Right", "Layout > Pane")
                .chord("Ctrl+b, Ctrl+ArrowRight")
                .chord("Ctrl+b, Alt+ArrowRight"),
            CommandDefinition::new("resize_pane_up", "Resize Pane Up", "Layout > Pane")
                .chord("Ctrl+b, Ctrl+ArrowUp")
                .chord("Ctrl+b, Alt+ArrowUp"),
            CommandDefinition::new("resize_pane_down", "Resize Pane Down", "Layout > Pane")
                .chord("Ctrl+b, Ctrl+ArrowDown")
                .chord("Ctrl+b, Alt+ArrowDown"),
        ]
    }
}

impl TryFrom<&CommandInvocation> for ResizeRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        match invocation.id.as_str() {
            "equalize_pane_size" => Ok(Self(PaneResize::Equalize)),
            "resize_pane_left" => Ok(Self(PaneResize::Direction(PaneDirection::Left))),
            "resize_pane_right" => Ok(Self(PaneResize::Direction(PaneDirection::Right))),
            "resize_pane_up" => Ok(Self(PaneResize::Direction(PaneDirection::Top))),
            "resize_pane_down" => Ok(Self(PaneResize::Direction(PaneDirection::Bottom))),
            _ => Err(()),
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleZoomRequest;

impl CommandRequest for ToggleZoomRequest {
    fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("zoom_pane", "Zoom Pane", "Layout > Pane")
                .hidden()
                .chord("Ctrl+b, z"),
        ]
    }
}

impl TryFrom<&CommandInvocation> for ToggleZoomRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "zoom_pane").then_some(Self).ok_or(())
    }
}

pub struct PanePlugin;

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            CommandTypePlugin::<OpenRequest>::default(),
            CommandTypePlugin::<CloseRequest>::default(),
            CommandTypePlugin::<FocusRequest>::default(),
            CommandTypePlugin::<ArrangeRequest>::default(),
            CommandTypePlugin::<ResizeRequest>::default(),
            CommandTypePlugin::<ToggleZoomRequest>::default(),
        ))
        .register_type::<SideSheetCardCollapsed>()
        .add_plugins((
            TreePlugin,
            IdentityPlugin,
            ArrangementPlugin,
            OpenPlugin,
            PaneZoomPlugin,
            FocusPlugin,
            ResizePlugin,
            ClosePlugin,
        ));
    }
}

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct SideSheetCardCollapsed;

pub fn first_stack_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_q: &Query<Entity, With<Stack>>,
) -> Option<Entity> {
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| tab_q.contains(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PendingLaunch;
    use crate::{
        settings::ConfirmCloseSettings,
        settings::{
            FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
        },
    };
    use vmux_command::CommandPlugin;

    fn test_settings() -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 0.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        }
    }

    #[test]
    fn open_beside_reuses_sibling_pane_as_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_plugins(BesideOpenPlugin);

        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
            ))
            .id();
        let agent_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            LastActivatedAt::now(),
            ChildOf(agent_pane),
        ));
        let other_pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
            .id();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: Some(PaneDirection::Right),
                url: "file:///x.rs".to_string(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(agent_pane).is_none(),
            "agent pane must not be split when a sibling exists"
        );
        let kids: Vec<Entity> = app
            .world()
            .get::<Children>(other_pane)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        let stacks = kids
            .into_iter()
            .filter(|&e| app.world().get::<Stack>(e).is_some())
            .count();
        assert_eq!(
            stacks, 1,
            "page should open as a new stack in the sibling pane"
        );
    }

    #[test]
    fn open_beside_splits_when_no_sibling() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_plugins(BesideOpenPlugin);

        let pane = app.world_mut().spawn((Pane, LastActivatedAt::now())).id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)));

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane,
                direction: Some(PaneDirection::Right),
                url: "file:///x.rs".to_string(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(pane).is_some(),
            "a lone pane should split when there is no sibling"
        );
    }

    fn place_pane_with_url(
        app: &mut App,
        parent: Entity,
        seq: u64,
        size: Vec2,
        url: &str,
    ) -> Entity {
        let pane = app
            .world_mut()
            .spawn((
                Pane,
                SpawnSeq(seq),
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(parent),
                ComputedNode::from_origin(size),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(vmux_core::PageMetadata {
                url: url.to_string(),
                ..default()
            });
        pane
    }

    fn stack_in_pane(app: &App, pane: Entity) -> Entity {
        let stacks: Vec<Entity> = app
            .world()
            .get::<Children>(pane)
            .map(|c| {
                c.iter()
                    .filter(|&e| app.world().get::<Stack>(e).is_some())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(stacks.len(), 1, "expected one stack in pane");
        stacks[0]
    }

    fn page_open_requests(app: &App) -> Vec<PageOpenRequest> {
        let messages = app.world().resource::<Messages<PageOpenRequest>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).cloned().collect()
    }

    fn materialize_page_metadata(app: &mut App) {
        for request in page_open_requests(app) {
            if let PageOpenTarget::Stack(stack) = request.target {
                app.world_mut()
                    .entity_mut(stack)
                    .insert(vmux_core::PageMetadata {
                        url: request.url,
                        ..default()
                    });
            }
        }
    }

    fn open_beside_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_plugins(BesideOpenPlugin);
        app
    }

    #[test]
    fn auto_same_type_adds_tab_without_splitting() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: browser_pane,
                direction: None,
                url: "https://b.com".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "same type must not split"
        );
        let stacks = app
            .world()
            .get::<Children>(browser_pane)
            .map(|c| {
                c.iter()
                    .filter(|&e| app.world().get::<Stack>(e).is_some())
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(
            stacks, 2,
            "new browser page tabs into the existing browser pane"
        );
    }

    #[test]
    fn auto_batched_files_stack_in_first_file_pane() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(800.0, 900.0),
            "vmux://sessions/claude/session",
        );
        place_pane_with_url(
            &mut app,
            tab,
            2,
            Vec2::new(900.0, 400.0),
            "vmux://terminal/123",
        );

        for url in ["file:///repo/a.rs", "file:///repo/b.rs"] {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [0u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let file_stack_parents: Vec<Entity> = requests
            .iter()
            .filter_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url.starts_with("file:") => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .collect();

        assert_eq!(file_stack_parents.len(), 2);
        assert_eq!(
            file_stack_parents[0], file_stack_parents[1],
            "same-frame file opens should stack in one file pane"
        );
    }

    #[test]
    fn auto_batched_new_types_split_from_newest_target() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "vmux://terminal/",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let parent_for = |prefix: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url.starts_with(prefix) => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let browser_parent = parent_for("https:");
        let file_parent = parent_for("file:");
        let terminal_parent = parent_for("vmux://terminal/");

        for parent in [browser_parent, file_parent, terminal_parent] {
            assert!(
                app.world().get::<PaneSplit>(parent).is_none(),
                "new stack must live in a leaf pane, not directly under a split"
            );
        }

        let browser_split = app.world().get::<ChildOf>(browser_parent).unwrap().get();
        let file_split = app.world().get::<ChildOf>(file_parent).unwrap().get();
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_split
        );
        assert_eq!(
            app.world().get::<ChildOf>(file_split).unwrap().get(),
            browser_split
        );
        assert_eq!(
            app.world().get::<PaneSplit>(agent_pane).unwrap().direction,
            PaneSplitDirection::Row
        );
        assert_eq!(
            app.world()
                .get::<PaneSplit>(browser_split)
                .unwrap()
                .direction,
            PaneSplitDirection::Column
        );
        assert_eq!(
            app.world().get::<PaneSplit>(file_split).unwrap().direction,
            PaneSplitDirection::Row
        );
    }

    #[test]
    fn auto_batched_new_browser_stacks_in_existing_browser_bucket_after_other_work() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "vmux://terminal/",
            "https://github.com/vmux-ai/vmux/actions/runs/28544986467",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let pr_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221");
        let ci_parent = parent_for_url("https://github.com/vmux-ai/vmux/actions/runs/28544986467");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert_eq!(
            ci_parent, pr_parent,
            "new CI browser page should tab into the existing browser pane"
        );
        assert_ne!(ci_parent, terminal_parent);
    }

    #[test]
    fn auto_batched_new_browser_stacks_after_nonbrowser_tab_reuse() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "vmux://terminal/",
            "https://github.com/vmux-ai/vmux/actions/runs/28544986467",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "https://github.com/vmux-ai/vmux/pull/221/files",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let ci_parent = parent_for_url("https://github.com/vmux-ai/vmux/actions/runs/28544986467");
        let files_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221/files");

        assert_eq!(
            files_parent, ci_parent,
            "browser pages after file tab reuse should stack in the newest browser pane"
        );
    }

    #[test]
    fn auto_file_bucket_stays_reusable_after_multiple_file_tabs() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "vmux://terminal/",
            "file:///repo/crates/vmux_layout/src/placement.rs",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let plugin_parent = parent_for_url("file:///repo/crates/vmux_agent/src/plugin.rs");
        let pane_parent = parent_for_url("file:///repo/crates/vmux_layout/src/pane.rs");
        let placement_parent = parent_for_url("file:///repo/crates/vmux_layout/src/placement.rs");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert_eq!(pane_parent, plugin_parent);
        assert_eq!(
            placement_parent, plugin_parent,
            "later files should reuse the existing file pane even after it has multiple file tabs"
        );
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            app.world().get::<ChildOf>(plugin_parent).unwrap().get(),
            "terminal should split the current file tail"
        );
    }

    #[test]
    fn auto_terminal_splits_current_file_tail_after_file_bucket_reuse() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "https://github.com/vmux-ai/vmux/pull/221",
            "file:///repo/crates/vmux_layout/src/pane.rs",
            "file:///repo/crates/vmux_agent/src/plugin.rs",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [9; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let pr_parent = parent_for_url("https://github.com/vmux-ai/vmux/pull/221");
        let plugin_parent = parent_for_url("file:///repo/crates/vmux_agent/src/plugin.rs");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            app.world().get::<ChildOf>(plugin_parent).unwrap().get(),
            "terminal should split the current file tail"
        );
        assert_ne!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            app.world().get::<ChildOf>(pr_parent).unwrap().get()
        );
    }

    #[test]
    fn auto_first_file_splits_terminal_when_terminal_is_newer() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );
        let browser_pane = place_pane_with_url(
            &mut app,
            tab,
            10,
            Vec2::new(900.0, 400.0),
            "https://news.ycombinator.com/news",
        );
        let terminal_pane = place_pane_with_url(
            &mut app,
            tab,
            20,
            Vec2::new(900.0, 400.0),
            "vmux://terminal/",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "file:///repo/README.md".into(),
                request_id: [9; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let file_parent = requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack) if request.url == "file:///repo/README.md" => app
                    .world()
                    .get::<ChildOf>(*stack)
                    .map(|parent| parent.get()),
                _ => None,
            })
            .unwrap();

        assert_eq!(
            app.world().get::<ChildOf>(file_parent).unwrap().get(),
            terminal_pane,
            "first file should split the newest terminal pane"
        );
        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "browser pane must not split for first file"
        );
    }

    #[test]
    fn auto_browser_open_after_files_becomes_anchor_for_terminal() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "file:///repo/.git/HEAD",
            "file:///repo/.git/refs/heads/main",
            "https://news.ycombinator.com/news",
            "vmux://terminal/",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let file_parent = parent_for_url("file:///repo/.git/refs/heads/main");
        let browser_parent = parent_for_url("https://news.ycombinator.com/news");
        let terminal_parent = parent_for_url("vmux://terminal/");
        let terminal_split = app.world().get::<ChildOf>(terminal_parent).unwrap().get();

        assert_eq!(
            terminal_split,
            app.world().get::<ChildOf>(browser_parent).unwrap().get(),
            "terminal should split the browser pane when the browser opened after files"
        );
        assert_ne!(
            terminal_split,
            app.world().get::<ChildOf>(file_parent).unwrap().get(),
            "terminal must not split the older file pane"
        );
    }

    #[test]
    fn auto_file_after_terminal_stacks_in_existing_file_bucket() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for (i, url) in [
            "file:///repo/.git/HEAD",
            "file:///repo/.git/refs/heads/main",
            "https://news.ycombinator.com/news",
            "vmux://terminal/",
            "file:///repo/README.md",
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: url.into(),
                    request_id: [i as u8; 16],
                    focus: false,
                });
            app.update();
            materialize_page_metadata(&mut app);
        }

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let stale_file_parent = parent_for_url("file:///repo/.git/refs/heads/main");
        let terminal_parent = parent_for_url("vmux://terminal/");
        let readme_parent = parent_for_url("file:///repo/README.md");

        assert_eq!(
            readme_parent, stale_file_parent,
            "README should tab into the existing file pane"
        );
        assert_ne!(
            readme_parent, terminal_parent,
            "README must not tab into the terminal pane"
        );
    }

    #[test]
    fn auto_duplicate_url_reuses_pending_open_in_same_batch() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        for i in 0..2 {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: agent_pane,
                    direction: None,
                    url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                    request_id: [i; 16],
                    focus: false,
                });
        }
        app.update();

        let requests = page_open_requests(&app);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url == "https://github.com/vmux-ai/vmux/pull/221")
                .count(),
            1
        );
    }

    #[test]
    fn auto_duplicate_url_reuses_pending_page_open_task() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                request_id: [0; 16],
                focus: false,
            });
        app.update();

        let first_stack = page_open_requests(&app)
            .iter()
            .find_map(|request| match request.target {
                PageOpenTarget::Stack(stack)
                    if request.url == "https://github.com/vmux-ai/vmux/pull/221" =>
                {
                    Some(stack)
                }
                _ => None,
            })
            .unwrap();
        app.world_mut().spawn(vmux_core::PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack: first_stack,
            url: "https://github.com/vmux-ai/vmux/pull/221".into(),
            request_id: None,
        });

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux/pull/221".into(),
                request_id: [1; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url == "https://github.com/vmux-ai/vmux/pull/221")
                .count(),
            1
        );
    }

    #[test]
    fn direction_batched_new_type_uses_split_target_size() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(1600.0, 900.0),
            "vmux://sessions/claude/session",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: Some(PaneDirection::Right),
                url: "file:///repo/crates/vmux_agent/src/plugin.rs".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: agent_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [1u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for = |prefix: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url.starts_with(prefix) => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let file_parent = parent_for("file:");
        let terminal_parent = parent_for("vmux://terminal/");
        let file_split = app.world().get::<ChildOf>(file_parent).unwrap().get();

        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_split
        );
        assert_eq!(
            app.world().get::<PaneSplit>(file_split).unwrap().direction,
            PaneSplitDirection::Column,
            "the forced-right target is 800x900, so the next pane should split it vertically"
        );
    }

    #[test]
    fn auto_new_type_splits_anchor() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 5, Vec2::new(1600.0, 900.0), "https://a.com");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: browser_pane,
                direction: None,
                url: "file:///x.rs".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let split = app
            .world()
            .get::<PaneSplit>(browser_pane)
            .expect("a new file type must split the anchor");
        assert_eq!(
            split.direction,
            PaneSplitDirection::Row,
            "wide anchor splits along its longer (x) side => Row"
        );
    }

    #[test]
    fn auto_reuse_focuses_existing_url_without_new_stack() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: browser_pane,
                direction: None,
                url: "https://a.com".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let after = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(
            after, before,
            "reuse focuses the existing page; no new stack spawned"
        );
        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "reuse must not split"
        );
    }

    #[test]
    fn auto_reuse_focuses_existing_file_with_different_fragment() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let after = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(
            after, before,
            "same file with a new fragment focuses the existing page"
        );
    }

    #[test]
    fn auto_reuse_file_with_different_fragment_navigates_existing_stack() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        let stack = stack_in_pane(&app, file_pane);

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let opens = page_open_requests(&app);
        assert_eq!(opens.len(), 1);
        match &opens[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(target),
                url,
                ..
            } => {
                assert_eq!(*target, stack);
                assert_eq!(url, "file:///repo/src/main.rs#L42");
            }
            other => panic!("expected PageOpenRequest for existing stack, got {other:?}"),
        }
    }

    #[test]
    fn explicit_direction_reuse_focuses_existing_file_with_different_fragment() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: Some(PaneDirection::Right),
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let after = app
            .world_mut()
            .query_filtered::<Entity, With<Stack>>()
            .iter(app.world())
            .count();
        assert_eq!(
            after, before,
            "reuse wins before explicit direction can create a duplicate"
        );
        assert!(
            app.world().get::<PaneSplit>(file_pane).is_none(),
            "reuse must not split the existing pane"
        );
    }

    #[test]
    fn reuse_with_focus_false_does_not_activate_existing_tab() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let old_tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt(1),
                ChildOf(space),
            ))
            .id();
        let active_tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(10), ChildOf(space)))
            .id();
        let file_pane = place_pane_with_url(
            &mut app,
            old_tab,
            5,
            Vec2::new(800.0, 600.0),
            "file:///repo/src/main.rs#L10",
        );
        place_pane_with_url(
            &mut app,
            active_tab,
            6,
            Vec2::new(800.0, 600.0),
            "https://active.example",
        );

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "file:///repo/src/main.rs#L42".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        assert_eq!(app.world().get::<LastActivatedAt>(old_tab).unwrap().0, 1);
        assert_eq!(
            app.world().get::<LastActivatedAt>(active_tab).unwrap().0,
            10
        );
    }

    #[test]
    fn auto_browser_reuses_bucket_before_terminal_splits_existing_tail() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [1u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let github_parent = parent_for_url("https://github.com/vmux-ai/vmux");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "new browser URL should stack in the existing browser pane"
        );
        assert_eq!(
            github_parent, browser_pane,
            "new browser URL should stack in the existing browser pane"
        );
        assert!(
            app.world().get::<PaneSplit>(file_pane).is_some(),
            "terminal should split the current file tail"
        );
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_pane,
            "terminal should split the current file tail"
        );
    }

    #[test]
    fn auto_browser_reuses_bucket_before_same_batch_terminal_splits_existing_tail() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "vmux://terminal/".into(),
                request_id: [1u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let parent_for_url = |url: &str| -> Entity {
            requests
                .iter()
                .find_map(|request| match &request.target {
                    PageOpenTarget::Stack(stack) if request.url == url => app
                        .world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get()),
                    _ => None,
                })
                .unwrap()
        };
        let github_parent = parent_for_url("https://github.com/vmux-ai/vmux");
        let terminal_parent = parent_for_url("vmux://terminal/");

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "new browser URL should stack in the existing browser pane"
        );
        assert_eq!(
            github_parent, browser_pane,
            "new browser URL should stack in the existing browser pane"
        );
        assert!(
            app.world().get::<PaneSplit>(file_pane).is_some(),
            "terminal should split the current file tail"
        );
        assert_eq!(
            app.world().get::<ChildOf>(terminal_parent).unwrap().get(),
            file_pane,
            "terminal should split the current file tail"
        );
    }

    #[test]
    fn forced_split_anchor_keeps_current_tail_when_browser_reuses_bucket() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane =
            place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 600.0), "https://a.com");
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let requests = page_open_requests(&app);
        let github_parent = requests
            .iter()
            .find_map(|request| match &request.target {
                PageOpenTarget::Stack(stack)
                    if request.url == "https://github.com/vmux-ai/vmux" =>
                {
                    app.world()
                        .get::<ChildOf>(*stack)
                        .map(|parent| parent.get())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            github_parent, browser_pane,
            "new browser URL should reuse the existing browser pane"
        );

        app.insert_resource(SplitAnchorInput { anchor: file_pane })
            .init_resource::<SplitAnchorOut>()
            .add_systems(Update, split_anchor_test_sys);
        app.update();

        assert_eq!(app.world().resource::<SplitAnchorOut>().0, Some(file_pane));
    }

    #[test]
    fn forced_split_anchor_ignores_exact_reused_browser_page() {
        let mut app = open_beside_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let browser_pane = place_pane_with_url(
            &mut app,
            tab,
            1,
            Vec2::new(800.0, 600.0),
            "https://github.com/vmux-ai/vmux",
        );
        let file_pane =
            place_pane_with_url(&mut app, tab, 9, Vec2::new(800.0, 600.0), "file:///x.rs");

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: file_pane,
                direction: None,
                url: "https://github.com/vmux-ai/vmux".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        app.insert_resource(SplitAnchorInput { anchor: file_pane })
            .init_resource::<SplitAnchorOut>()
            .add_systems(Update, split_anchor_test_sys);
        app.update();

        assert!(
            app.world().get::<PaneSplit>(browser_pane).is_none(),
            "exact browser reuse must not split the existing browser pane"
        );
        assert_eq!(app.world().resource::<SplitAnchorOut>().0, Some(file_pane));
    }

    #[derive(Resource)]
    struct SplitAnchorInput {
        anchor: Entity,
    }

    #[derive(Resource, Default)]
    struct SplitAnchorOut(Option<Entity>);

    fn split_anchor_test_sys(
        input: Res<SplitAnchorInput>,
        ctx: PlacementCtx,
        mut out: ResMut<SplitAnchorOut>,
    ) {
        out.0 = Some(resolve_split_anchor_pane(input.anchor, &ctx));
    }

    #[derive(Resource)]
    struct SpiralInput {
        anchor: Entity,
        url: String,
    }

    #[derive(Resource, Default)]
    struct SpiralOut(Option<Entity>);

    fn spiral_test_sys(
        input: Res<SpiralInput>,
        mut commands: Commands,
        ctx: PlacementCtx,
        mut out: ResMut<SpiralOut>,
    ) {
        let mut batch = std::collections::HashSet::new();
        out.0 = Some(resolve_spiral_pane(
            &mut commands,
            input.anchor,
            &input.url,
            false,
            &mut batch,
            &ctx,
        ));
    }

    fn spiral_app(anchor_url: &str, other: Option<(&str, u64, Vec2)>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SpiralOut>()
            .add_systems(Update, spiral_test_sys);
        let space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                vmux_core::Active,
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();
        let agent = place_pane_with_url(&mut app, tab, 1, Vec2::new(800.0, 900.0), anchor_url);
        if let Some((url, seq, size)) = other {
            place_pane_with_url(&mut app, tab, seq, size, url);
        }
        (app, agent)
    }

    #[test]
    fn run_terminal_spirals_off_newest_nonagent_leaf() {
        let (mut app, agent) = spiral_app(
            "vmux://sessions/vibe/x",
            Some(("https://a.com", 9, Vec2::new(1600.0, 900.0))),
        );
        let browser = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .find(|&e| e != agent)
            .unwrap();
        app.world_mut().insert_resource(SpiralInput {
            anchor: agent,
            url: "vmux://terminal/".into(),
        });
        app.update();

        let split = app
            .world()
            .get::<PaneSplit>(browser)
            .expect("newest non-agent (browser) leaf must split for the new terminal type");
        assert_eq!(
            split.direction,
            PaneSplitDirection::Column,
            "first terminal stacks below the browser"
        );
        assert!(
            app.world().get::<PaneSplit>(agent).is_none(),
            "agent pane untouched"
        );
        let out = app.world().resource::<SpiralOut>().0.unwrap();
        assert_ne!(out, browser, "returns the new leaf, not the split node");
    }

    #[test]
    fn run_terminal_adds_tab_to_existing_terminal_stack() {
        let (mut app, agent) = spiral_app(
            "vmux://sessions/vibe/x",
            Some(("vmux://terminal/7", 9, Vec2::new(1600.0, 900.0))),
        );
        let term_pane = app
            .world_mut()
            .query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>()
            .iter(app.world())
            .find(|&e| e != agent)
            .unwrap();
        app.world_mut().insert_resource(SpiralInput {
            anchor: agent,
            url: "vmux://terminal/".into(),
        });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(term_pane).is_none(),
            "existing terminal stack must not split"
        );
        assert_eq!(
            app.world().resource::<SpiralOut>().0,
            Some(term_pane),
            "new terminal tabs into the existing terminal pane"
        );
    }

    #[test]
    fn zoomed_component_constructs_and_reads_back() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let leaf = app.world_mut().spawn(Pane).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf,
                    hidden: vec![],
                },
            ))
            .id();

        let z = app.world().get::<Zoomed>(tab).expect("Zoomed present");
        assert_eq!(z.leaf, leaf);
        assert!(z.hidden.is_empty());
    }

    #[test]
    fn zoom_command_inserts_zoomed_with_correct_hidden_set() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, LayoutRequestPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<PendingLaunch>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_plugins(PaneZoomPlugin);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);

        app.update();

        let z = app
            .world()
            .get::<Zoomed>(tab)
            .expect("Zoomed inserted on tab");
        assert_eq!(z.leaf, leaf_b);
        assert_eq!(z.hidden, vec![leaf_a]);
    }

    #[test]
    fn zoom_command_on_zoomed_tab_removes_zoomed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, LayoutRequestPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<PendingLaunch>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_plugins(PaneZoomPlugin);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_some());

        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_none());
    }

    #[test]
    fn zoom_command_on_single_pane_tab_is_noop() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, LayoutRequestPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<PendingLaunch>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_plugins(PaneZoomPlugin);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let only = app
            .world_mut()
            .spawn((Pane, Node::default(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(only)));

        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);
        app.update();

        assert!(app.world().get::<Zoomed>(tab).is_none());
    }

    #[test]
    fn removing_zoomed_pane_clears_zoom_state() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            CommandPlugin,
            LayoutRequestPlugin,
            PaneZoomPlugin,
        ));

        let leaf_a = app.world_mut().spawn((Pane, Node::default())).id();
        let leaf_b = app.world_mut().spawn((Pane, Node::default())).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf: leaf_b,
                    hidden: vec![leaf_a],
                },
            ))
            .id();

        app.update();

        app.world_mut().despawn(leaf_b);
        app.update();

        assert!(
            app.world().get::<Zoomed>(tab).is_none(),
            "Zoomed should be cleared when its leaf is despawned"
        );
    }

    #[test]
    fn split_command_auto_unzooms_first() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, LayoutRequestPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<PendingLaunch>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_plugins(PaneZoomPlugin);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_some());

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Bottom,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            });
        app.update();

        assert!(
            app.world().get::<Zoomed>(tab).is_none(),
            "open-in-pane should auto-unzoom"
        );
    }

    #[test]
    fn select_command_auto_unzooms() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, LayoutRequestPlugin))
            .init_resource::<PaneHoverIntent>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<PendingLaunch>()
            .init_resource::<ConfirmCloseSettings>()
            .insert_resource(test_settings())
            .add_plugins(PaneZoomPlugin);

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let leaf_a = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        let leaf_b = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt::now(),
                ChildOf(split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_a)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(leaf_b)));
        app.world_mut()
            .entity_mut(leaf_b)
            .insert(LastActivatedAt::now());

        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);
        app.update();
        assert!(app.world().get::<Zoomed>(tab).is_some());

        app.world_mut()
            .resource_mut::<Messages<FocusRequest>>()
            .write(FocusRequest(PaneFocus::Direction(PaneDirection::Left)));
        app.update();

        assert!(
            app.world().get::<Zoomed>(tab).is_none(),
            "navigation should auto-unzoom"
        );
    }

    #[test]
    fn removing_zoomed_restores_display_flex() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            CommandPlugin,
            LayoutRequestPlugin,
            PaneZoomPlugin,
        ));

        let leaf = app.world_mut().spawn((Pane, Node::default())).id();
        let sib = app
            .world_mut()
            .spawn((
                Pane,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf,
                    hidden: vec![sib],
                },
            ))
            .id();

        app.update();

        app.world_mut().entity_mut(tab).remove::<Zoomed>();
        app.update();

        assert_eq!(app.world().get::<Node>(sib).unwrap().display, Display::Flex);
        let _ = leaf;
    }

    #[test]
    fn sync_zoom_visibility_sets_display_none_on_hidden_entities() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            CommandPlugin,
            LayoutRequestPlugin,
            PaneZoomPlugin,
        ));

        let leaf = app.world_mut().spawn((Pane, Node::default())).id();
        let sib_a = app.world_mut().spawn((Pane, Node::default())).id();
        let sib_b = app.world_mut().spawn((Pane, Node::default())).id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                Zoomed {
                    leaf,
                    hidden: vec![sib_a, sib_b],
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<Node>(sib_a).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(sib_b).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(leaf).unwrap().display,
            Display::Flex
        );

        let _ = tab;
    }

    #[test]
    fn zoom_hides_siblings_at_each_split_ancestor() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            CommandPlugin,
            LayoutRequestPlugin,
            PaneZoomPlugin,
        ));

        let _window = app.world_mut().spawn(PrimaryWindow).id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let split_root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt(2),
                ChildOf(split_root),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(2), ChildOf(left)));
        let right_split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Column,
                },
                ChildOf(split_root),
            ))
            .id();
        let right_top = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt(4),
                ChildOf(right_split),
            ))
            .id();
        let right_bot = app
            .world_mut()
            .spawn((
                Pane,
                Node::default(),
                LastActivatedAt(3),
                ChildOf(right_split),
            ))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(4), ChildOf(right_top)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(3), ChildOf(right_bot)));
        app.world_mut()
            .resource_mut::<Messages<ToggleZoomRequest>>()
            .write(ToggleZoomRequest);

        app.update();

        let zoomed = app.world().get::<Zoomed>(tab).expect("tab is zoomed");
        assert_eq!(zoomed.leaf, right_top);
        assert_eq!(zoomed.hidden.len(), 2);
        assert!(zoomed.hidden.contains(&right_bot));
        assert!(zoomed.hidden.contains(&left));
    }

    #[derive(Resource, Default)]
    struct InPaneCollectedSpawns(Vec<PageOpenRequest>);

    fn collect_in_pane_spawns(
        mut reader: MessageReader<PageOpenRequest>,
        mut collected: ResMut<InPaneCollectedSpawns>,
    ) {
        for req in reader.read() {
            collected.0.push(req.clone());
        }
    }

    fn build_in_pane_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin, LayoutRequestPlugin))
            .add_message::<crate::TerminalLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<PendingCursorWarp>()
            .init_resource::<InPaneCollectedSpawns>()
            .insert_resource(test_settings())
            .add_plugins(DirectionalOpenPlugin)
            .add_systems(PostUpdate, collect_in_pane_spawns);
        let _window = app.world_mut().spawn(PrimaryWindow).id();
        app
    }

    fn build_single_pane(app: &mut App) -> (Entity, Entity, Entity) {
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt::now()))
            .id();
        let pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        (tab, pane, stack)
    }

    fn build_pre_split(app: &mut App) -> (Entity, Entity, Entity, Entity) {
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                PaneSize::default(),
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                ChildOf(tab),
            ))
            .id();
        let left = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt(10), ChildOf(split)))
            .id();
        let right = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt(5), ChildOf(split)))
            .id();
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(10), ChildOf(left)));
        app.world_mut()
            .spawn((Stack::default(), LastActivatedAt(5), ChildOf(right)));
        (tab, split, left, right)
    }
    #[test]
    fn open_beside_splits_the_given_pane() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_plugins(BesideOpenPlugin);
        let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
        let anchor_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: anchor_pane,
                direction: Some(PaneDirection::Right),
                url: "vmux://terminal/".into(),
                request_id: [0u8; 16],
                focus: true,
            });
        app.update();

        let world = app.world_mut();
        assert!(world.get::<PaneSplit>(anchor_pane).is_some());
        let kids = world.entity(anchor_pane).get::<Children>().unwrap();
        assert_eq!(kids.iter().count(), 2);
    }

    #[test]
    fn open_beside_with_focus_false_leaves_new_stack_unactivated() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_plugins(BesideOpenPlugin);
        let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
        let anchor_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

        app.world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .write(OpenBesideRequest {
                pane: anchor_pane,
                direction: Some(PaneDirection::Right),
                url: "vmux://terminal/".into(),
                request_id: [0u8; 16],
                focus: false,
            });
        app.update();

        let world = app.world_mut();
        let mut stacks = world.query_filtered::<&LastActivatedAt, With<Stack>>();
        let unactivated = stacks.iter(world).filter(|la| la.0 == 0).count();
        assert_eq!(
            unactivated, 1,
            "focus:false leaves exactly the new stack un-activated (ts 0) so focus stays put"
        );
    }

    #[test]
    fn batched_open_beside_makes_no_empty_panes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<OpenBesideRequest>()
            .add_message::<PageOpenRequest>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_plugins(BesideOpenPlugin);
        let tab = app.world_mut().spawn(crate::tab::tab_bundle()).id();
        let anchor_pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(tab)))
            .id();
        app.world_mut()
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(anchor_pane)));

        for direction in [
            PaneDirection::Right,
            PaneDirection::Bottom,
            PaneDirection::Left,
        ] {
            app.world_mut()
                .resource_mut::<Messages<OpenBesideRequest>>()
                .write(OpenBesideRequest {
                    pane: anchor_pane,
                    direction: Some(direction),
                    url: "vmux://terminal/".into(),
                    request_id: [0u8; 16],
                    focus: false,
                });
        }
        app.update();

        assert!(
            app.world().get::<PaneSplit>(anchor_pane).is_some(),
            "anchor becomes a split"
        );
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(anchor_pane)
            .expect("anchor has children")
            .iter()
            .collect();
        assert_eq!(
            children.len(),
            4,
            "anchor holds the stack-holder + three terminal leaves, with no orphaned empty panes"
        );
        for child in children {
            let has_stack = app
                .world()
                .get::<Children>(child)
                .is_some_and(|cc| cc.iter().any(|e| app.world().get::<Stack>(e).is_some()));
            assert!(
                has_stack,
                "every child pane has a stack; none is an empty orphan"
            );
        }
    }

    #[test]
    fn in_pane_new_split_right_creates_pane_to_the_right() {
        let mut app = build_in_pane_app();
        let (_tab, pane, _stack) = build_single_pane(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://x".into()),
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(pane).is_some(),
            "original pane should now be a split"
        );
        let ps = app.world().get::<PaneSplit>(pane).unwrap();
        assert_eq!(ps.direction, PaneSplitDirection::Row);

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .filter(|e| app.world().get::<Pane>(*e).is_some())
            .collect();
        assert_eq!(children.len(), 2, "should have two child panes");

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://x");
                let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
                assert_eq!(
                    stack_parent, children[1],
                    "new stack should be in the second (right) pane"
                );
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        }
    }

    #[test]
    fn in_pane_new_split_warps_cursor_to_new_pane() {
        let mut app = build_in_pane_app();
        let (_tab, pane, _stack) = build_single_pane(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: Some("https://x".into()),
            });
        app.update();

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(pane)
            .unwrap()
            .iter()
            .filter(|e| app.world().get::<Pane>(*e).is_some())
            .collect();

        assert_eq!(
            app.world().resource::<PendingCursorWarp>().target,
            Some(children[1]),
            "split should warp cursor to the newly active pane"
        );
    }

    #[test]
    fn in_pane_new_split_without_url_opens_the_start_page() {
        let mut app = build_in_pane_app();
        let (_tab, pane, _stack) = build_single_pane(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            });
        app.update();

        assert!(app.world().get::<PaneSplit>(pane).is_some());
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            opened.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(),
            [vmux_core::EffectiveStartupUrl::START_PAGE]
        );
    }

    #[test]
    fn in_pane_existing_in_place_navigates_neighbor_active_stack() {
        let mut app = build_in_pane_app();
        let (_tab, _split, _left, right) = build_pre_split(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::InPlace,
                url: Some("https://new".into()),
            });
        app.update();

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://new");
                let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
                assert_eq!(
                    stack_parent, right,
                    "should navigate the existing right pane's stack"
                );
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        }
    }

    #[test]
    fn in_pane_existing_new_stack_adds_stack_to_neighbor() {
        let mut app = build_in_pane_app();
        let (_tab, _split, _left, right) = build_pre_split(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::NewStack,
                url: Some("https://x".into()),
            });
        app.update();

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        let new_stack = match &collected.0[0] {
            PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                ..
            } => {
                assert_eq!(url, "https://x");
                let stack_parent = app.world().get::<ChildOf>(*stack).map(|c| c.get()).unwrap();
                assert_eq!(stack_parent, right);
                *stack
            }
            other => panic!("expected PageOpenRequest, got {other:?}"),
        };

        app.update();

        let right_stacks: Vec<Entity> = app
            .world()
            .get::<Children>(right)
            .map(|c| {
                c.iter()
                    .filter(|e| app.world().get::<Stack>(*e).is_some())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(right_stacks.len(), 2, "right pane should now have 2 stacks");
        assert!(right_stacks.contains(&new_stack));
    }

    #[test]
    fn in_pane_existing_falls_back_to_new_split_when_no_sibling() {
        let mut app = build_in_pane_app();
        let (_tab, pane, _stack) = build_single_pane(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::InPlace,
                url: Some("https://x".into()),
            });
        app.update();

        assert!(
            app.world().get::<PaneSplit>(pane).is_some(),
            "should have fallen back to splitting"
        );

        let collected = app.world().resource::<InPaneCollectedSpawns>();
        assert_eq!(collected.0.len(), 1);
        assert_eq!(collected.0[0].url, "https://x");
    }
}
