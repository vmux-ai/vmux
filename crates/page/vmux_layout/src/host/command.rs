use bevy::prelude::*;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, OpenCommand, PaneCommand, ReadAppCommands,
};

use crate::{
    pane::{
        PaneArrangement, PaneFocus, PaneOpenRequest, PaneRequest, PaneResize, PaneSplitDirection,
    },
    target::SiblingDirection,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LayoutRequestSet {
    Dispatch,
    Prepare,
    Handle,
}

pub(super) struct LayoutCommandPlugin;

impl Plugin for LayoutCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PaneRequest>()
            .configure_sets(
                Update,
                (
                    LayoutRequestSet::Dispatch,
                    LayoutRequestSet::Prepare,
                    LayoutRequestSet::Handle,
                )
                    .chain()
                    .in_set(ReadAppCommands),
            )
            .add_systems(
                Update,
                dispatch_pane_commands.in_set(LayoutRequestSet::Dispatch),
            );
    }
}

impl From<PaneCommand> for PaneRequest {
    fn from(command: PaneCommand) -> Self {
        match command {
            PaneCommand::Toggle => Self::Focus(PaneFocus::Next),
            PaneCommand::Close => Self::Close,
            PaneCommand::Zoom => Self::ToggleZoom,
            PaneCommand::SelectLeft => Self::Focus(PaneFocus::Direction(
                vmux_api::open_target::PaneDirection::Left,
            )),
            PaneCommand::SelectRight => Self::Focus(PaneFocus::Direction(
                vmux_api::open_target::PaneDirection::Right,
            )),
            PaneCommand::SelectUp => Self::Focus(PaneFocus::Direction(
                vmux_api::open_target::PaneDirection::Top,
            )),
            PaneCommand::SelectDown => Self::Focus(PaneFocus::Direction(
                vmux_api::open_target::PaneDirection::Bottom,
            )),
            PaneCommand::SwapPrev => {
                Self::Arrange(PaneArrangement::Swap(SiblingDirection::Previous))
            }
            PaneCommand::SwapNext => Self::Arrange(PaneArrangement::Swap(SiblingDirection::Next)),
            PaneCommand::RotateForward => {
                Self::Arrange(PaneArrangement::Rotate(SiblingDirection::Next))
            }
            PaneCommand::RotateBackward => {
                Self::Arrange(PaneArrangement::Rotate(SiblingDirection::Previous))
            }
            PaneCommand::Mirror => Self::Arrange(PaneArrangement::Mirror(None)),
            PaneCommand::MirrorHorizontal => {
                Self::Arrange(PaneArrangement::Mirror(Some(PaneSplitDirection::Row)))
            }
            PaneCommand::MirrorVertical => {
                Self::Arrange(PaneArrangement::Mirror(Some(PaneSplitDirection::Column)))
            }
            PaneCommand::EqualizeSize => Self::Resize(PaneResize::Equalize),
            PaneCommand::ResizeLeft => Self::Resize(PaneResize::Direction(
                vmux_api::open_target::PaneDirection::Left,
            )),
            PaneCommand::ResizeRight => Self::Resize(PaneResize::Direction(
                vmux_api::open_target::PaneDirection::Right,
            )),
            PaneCommand::ResizeUp => Self::Resize(PaneResize::Direction(
                vmux_api::open_target::PaneDirection::Top,
            )),
            PaneCommand::ResizeDown => Self::Resize(PaneResize::Direction(
                vmux_api::open_target::PaneDirection::Bottom,
            )),
        }
    }
}

fn dispatch_pane_commands(
    mut commands: MessageReader<AppCommand>,
    mut requests: MessageWriter<PaneRequest>,
) {
    for command in commands.read() {
        match command {
            AppCommand::Layout(LayoutCommand::Pane(command)) => {
                requests.write((*command).into());
            }
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPane {
                direction,
                target,
                mode,
                url,
            })) => {
                requests.write(PaneRequest::Open(PaneOpenRequest {
                    direction: *direction,
                    target: *target,
                    mode: *mode,
                    url: url.clone(),
                }));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};

    #[test]
    fn every_pane_command_maps_to_a_request() {
        let cases = [
            (PaneCommand::Toggle, PaneRequest::Focus(PaneFocus::Next)),
            (PaneCommand::Close, PaneRequest::Close),
            (PaneCommand::Zoom, PaneRequest::ToggleZoom),
            (
                PaneCommand::SelectLeft,
                PaneRequest::Focus(PaneFocus::Direction(PaneDirection::Left)),
            ),
            (
                PaneCommand::SelectRight,
                PaneRequest::Focus(PaneFocus::Direction(PaneDirection::Right)),
            ),
            (
                PaneCommand::SelectUp,
                PaneRequest::Focus(PaneFocus::Direction(PaneDirection::Top)),
            ),
            (
                PaneCommand::SelectDown,
                PaneRequest::Focus(PaneFocus::Direction(PaneDirection::Bottom)),
            ),
            (
                PaneCommand::SwapPrev,
                PaneRequest::Arrange(PaneArrangement::Swap(SiblingDirection::Previous)),
            ),
            (
                PaneCommand::SwapNext,
                PaneRequest::Arrange(PaneArrangement::Swap(SiblingDirection::Next)),
            ),
            (
                PaneCommand::RotateForward,
                PaneRequest::Arrange(PaneArrangement::Rotate(SiblingDirection::Next)),
            ),
            (
                PaneCommand::RotateBackward,
                PaneRequest::Arrange(PaneArrangement::Rotate(SiblingDirection::Previous)),
            ),
            (
                PaneCommand::Mirror,
                PaneRequest::Arrange(PaneArrangement::Mirror(None)),
            ),
            (
                PaneCommand::MirrorHorizontal,
                PaneRequest::Arrange(PaneArrangement::Mirror(Some(PaneSplitDirection::Row))),
            ),
            (
                PaneCommand::MirrorVertical,
                PaneRequest::Arrange(PaneArrangement::Mirror(Some(PaneSplitDirection::Column))),
            ),
            (
                PaneCommand::EqualizeSize,
                PaneRequest::Resize(PaneResize::Equalize),
            ),
            (
                PaneCommand::ResizeLeft,
                PaneRequest::Resize(PaneResize::Direction(PaneDirection::Left)),
            ),
            (
                PaneCommand::ResizeRight,
                PaneRequest::Resize(PaneResize::Direction(PaneDirection::Right)),
            ),
            (
                PaneCommand::ResizeUp,
                PaneRequest::Resize(PaneResize::Direction(PaneDirection::Top)),
            ),
            (
                PaneCommand::ResizeDown,
                PaneRequest::Resize(PaneResize::Direction(PaneDirection::Bottom)),
            ),
        ];
        for (command, expected) in cases {
            assert_eq!(PaneRequest::from(command), expected);
        }
    }

    #[test]
    fn in_pane_command_preserves_request_fields() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppCommand>()
            .add_message::<PaneRequest>()
            .add_systems(Update, dispatch_pane_commands);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::Existing,
                    mode: PaneOpenMode::InPlace,
                    url: Some("https://example.com".to_string()),
                },
            )));

        app.update();

        let requests: Vec<PaneRequest> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();
        assert_eq!(
            requests,
            [PaneRequest::Open(PaneOpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::Existing,
                mode: PaneOpenMode::InPlace,
                url: Some("https://example.com".to_string()),
            })]
        );
    }
}
