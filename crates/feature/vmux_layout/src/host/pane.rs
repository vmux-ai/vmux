#[cfg(test)]
use super::command::LayoutRequestPlugin;
use super::target::SiblingDirection;
use crate::host::zoom::PaneZoomPlugin;
pub use crate::host::zoom::Zoomed;
use crate::stack::Stack;
#[cfg(test)]
use crate::tab::Tab;
use arrangement::ArrangementPlugin;
use bevy::prelude::*;
#[cfg(test)]
use bevy::{
    ecs::{message::Messages, relationship::Relationship},
    window::PrimaryWindow,
};
use close::ClosePlugin;
pub use close::{ForcePaneClose, PendingPaneClose};
use focus::FocusPlugin;
pub use focus::{PaneHoverCooldown, PendingCursorWarp};
use identity::IdentityPlugin;
pub use identity::{PaneId, SpawnCounter, SpawnSeq};
use moonshine_save::prelude::*;
use open::OpenPlugin;
#[cfg(test)]
use open::{BesideOpenPlugin, DirectionalOpenPlugin};
pub use open::{OpenBesideRequest, PanePlacement};
use resize::ResizePlugin;
pub use resize::{PaneDrag, PaneSize, PaneSplitGaps};
pub(crate) use tree::PaneHierarchy;
use tree::TreePlugin;
pub use tree::{Pane, PaneSplit, PaneSplitDirection, PaneTree};
use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
use vmux_command::{BindCommands, CommandInvocation, CommandRegistry, CommandRuntimePlugin};
#[cfg(test)]
use vmux_ecs::host::manifest::FeaturePlugin;
#[cfg(test)]
use vmux_ecs::{Active, PageMetadata, PageOpenId, PageOpenRequest, PageOpenTarget, PageOpenTask};
#[cfg(test)]
use vmux_flex::prelude::*;
#[cfg(test)]
use vmux_history::LastActivatedAt;

mod arrangement;
mod close;
mod focus;
mod identity;
mod open;
mod resize;
mod tree;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ArrangementSet;

pub struct PanePlugin;

impl Plugin for PanePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PaneCommandPlugin)
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

impl TryFrom<&CommandInvocation> for CloseRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "close_pane").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusRequest(pub PaneFocus);

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

impl TryFrom<&CommandInvocation> for ToggleZoomRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "zoom_pane").then_some(Self).ok_or(())
    }
}

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.message::<OpenRequest>(&mut commands);
    registry.message::<CloseRequest>(&mut commands);
    registry.message::<FocusRequest>(&mut commands);
    registry.message::<ArrangeRequest>(&mut commands);
    registry.message::<ResizeRequest>(&mut commands);
    registry.message::<ToggleZoomRequest>(&mut commands);
}

pub struct PaneCommandPlugin;

impl Plugin for PaneCommandPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_message::<OpenRequest>()
            .add_message::<CloseRequest>()
            .add_message::<FocusRequest>()
            .add_message::<ArrangeRequest>()
            .add_message::<ResizeRequest>()
            .add_message::<ToggleZoomRequest>()
            .add_systems(Startup, bind_commands.in_set(BindCommands));
    }
}

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct SideSheetCardCollapsed;

#[derive(bevy::ecs::system::SystemParam)]
pub struct PaneStacks<'w, 's> {
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stacks: Query<'w, 's, Entity, With<Stack>>,
}

impl PaneStacks<'_, '_> {
    pub fn first(&self, pane: Entity) -> Option<Entity> {
        let children = self.pane_children.get(pane).ok()?;
        children.iter().find(|&entity| self.stacks.contains(entity))
    }
}

#[cfg(test)]
mod tests;
