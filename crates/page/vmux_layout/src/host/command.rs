use bevy::prelude::*;
use vmux_command::{
    AppCommand, BookmarkCommand, BrowserCommand, LayoutCommand, OpenCommand, PaneCommand,
    ReadAppCommands, ServiceCommand, StackCommand, TabCommand, ToggleLayoutCommand, WindowCommand,
};

use crate::{
    archive::ReopenClosedPage,
    bookmark::{CreateFolderRequest, PinActiveRequest, ToggleActiveRequest},
    pane::{
        PaneArrangement, PaneFocus, PaneOpenRequest, PaneRequest, PaneResize, PaneSplitDirection,
    },
    stack::StackRequest,
    tab::{TabFocus, TabRequest},
    target::SiblingDirection,
    toggle::LayoutVisibilityRequest,
    window::MinimizeFocusedWindow,
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
            .add_message::<StackRequest>()
            .add_message::<TabRequest>()
            .add_message::<ReopenClosedPage>()
            .add_message::<ToggleActiveRequest>()
            .add_message::<PinActiveRequest>()
            .add_message::<CreateFolderRequest>()
            .add_message::<LayoutVisibilityRequest>()
            .add_message::<MinimizeFocusedWindow>()
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
            .add_systems(Update, dispatch_commands.in_set(LayoutRequestSet::Dispatch));
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

fn dispatch_commands(
    mut commands: MessageReader<AppCommand>,
    mut pane_requests: MessageWriter<PaneRequest>,
    mut stack_requests: MessageWriter<StackRequest>,
    mut tab_requests: MessageWriter<TabRequest>,
    mut reopen_requests: MessageWriter<ReopenClosedPage>,
    mut bookmark_toggles: MessageWriter<ToggleActiveRequest>,
    mut bookmark_pins: MessageWriter<PinActiveRequest>,
    mut bookmark_folders: MessageWriter<CreateFolderRequest>,
    mut visibility_requests: MessageWriter<LayoutVisibilityRequest>,
    mut minimize_requests: MessageWriter<MinimizeFocusedWindow>,
) {
    for command in commands.read() {
        match command {
            AppCommand::Layout(LayoutCommand::Pane(command)) => {
                pane_requests.write((*command).into());
            }
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPane {
                direction,
                target,
                mode,
                url,
            })) => {
                pane_requests.write(PaneRequest::Open(PaneOpenRequest {
                    direction: *direction,
                    target: *target,
                    mode: *mode,
                    url: url.clone(),
                }));
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close)) => {
                stack_requests.write(StackRequest::Close);
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Next)) => {
                stack_requests.write(StackRequest::Focus(SiblingDirection::Next));
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Previous)) => {
                stack_requests.write(StackRequest::Focus(SiblingDirection::Previous));
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::SwapPrev)) => {
                stack_requests.write(StackRequest::Move(SiblingDirection::Previous));
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::SwapNext)) => {
                stack_requests.write(StackRequest::Move(SiblingDirection::Next));
            }
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Reopen)) => {
                reopen_requests.write(ReopenClosedPage);
            }
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url })) => {
                stack_requests.write(StackRequest::Open { url: url.clone() });
            }
            AppCommand::Service(ServiceCommand::Open) => {
                stack_requests.write(StackRequest::OpenServices);
            }
            AppCommand::Layout(LayoutCommand::Tab(command)) => {
                if let Ok(request) = TabRequest::try_from(*command) {
                    tab_requests.write(request);
                }
            }
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url })) => {
                tab_requests.write(TabRequest::Open { url: url.clone() });
            }
            AppCommand::Bookmark(BookmarkCommand::ToggleActive) => {
                bookmark_toggles.write(ToggleActiveRequest);
            }
            AppCommand::Bookmark(BookmarkCommand::PinActive) => {
                bookmark_pins.write(PinActiveRequest);
            }
            AppCommand::Bookmark(BookmarkCommand::NewFolder) => {
                bookmark_folders.write(CreateFolderRequest);
            }
            AppCommand::Layout(LayoutCommand::ToggleLayout(ToggleLayoutCommand::Toggle)) => {
                visibility_requests.write(LayoutVisibilityRequest::Toggle);
            }
            AppCommand::Layout(LayoutCommand::Window(WindowCommand::Minimize)) => {
                minimize_requests.write(MinimizeFocusedWindow);
            }
            _ => {}
        }
    }
}

impl TryFrom<TabCommand> for TabRequest {
    type Error = ();

    fn try_from(command: TabCommand) -> Result<Self, Self::Error> {
        let request = match command {
            TabCommand::Close => Self::Close,
            TabCommand::New => Self::Create,
            TabCommand::Next => Self::Focus(TabFocus::Sibling(SiblingDirection::Next)),
            TabCommand::Previous => Self::Focus(TabFocus::Sibling(SiblingDirection::Previous)),
            TabCommand::SelectIndex1 => Self::Focus(TabFocus::Index(0)),
            TabCommand::SelectIndex2 => Self::Focus(TabFocus::Index(1)),
            TabCommand::SelectIndex3 => Self::Focus(TabFocus::Index(2)),
            TabCommand::SelectIndex4 => Self::Focus(TabFocus::Index(3)),
            TabCommand::SelectIndex5 => Self::Focus(TabFocus::Index(4)),
            TabCommand::SelectIndex6 => Self::Focus(TabFocus::Index(5)),
            TabCommand::SelectIndex7 => Self::Focus(TabFocus::Index(6)),
            TabCommand::SelectIndex8 => Self::Focus(TabFocus::Index(7)),
            TabCommand::SelectLast => Self::Focus(TabFocus::Last),
            TabCommand::SwapPrev => Self::Move(SiblingDirection::Previous),
            TabCommand::SwapNext => Self::Move(SiblingDirection::Next),
            TabCommand::Rename => return Err(()),
        };
        Ok(request)
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
            .add_plugins(LayoutCommandPlugin);
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

    #[test]
    fn stack_commands_route_to_owned_requests() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppCommand>()
            .add_plugins(LayoutCommandPlugin);
        let commands = [
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close)),
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Next)),
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Previous)),
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::SwapPrev)),
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::SwapNext)),
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Reopen)),
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
                url: Some("https://example.com".to_string()),
            })),
            AppCommand::Service(ServiceCommand::Open),
        ];
        for command in commands {
            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(command);
        }

        app.update();

        let stack_requests: Vec<StackRequest> = app
            .world_mut()
            .resource_mut::<Messages<StackRequest>>()
            .drain()
            .collect();
        assert_eq!(
            stack_requests,
            [
                StackRequest::Close,
                StackRequest::Focus(SiblingDirection::Next),
                StackRequest::Focus(SiblingDirection::Previous),
                StackRequest::Move(SiblingDirection::Previous),
                StackRequest::Move(SiblingDirection::Next),
                StackRequest::Open {
                    url: Some("https://example.com".to_string()),
                },
                StackRequest::OpenServices,
            ]
        );
        let reopen_requests: Vec<ReopenClosedPage> = app
            .world_mut()
            .resource_mut::<Messages<ReopenClosedPage>>()
            .drain()
            .collect();
        assert_eq!(reopen_requests, [ReopenClosedPage]);
    }

    #[test]
    fn tab_commands_route_to_owned_requests() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppCommand>()
            .add_plugins(LayoutCommandPlugin);
        let commands = [
            TabCommand::Close,
            TabCommand::New,
            TabCommand::Next,
            TabCommand::Previous,
            TabCommand::Rename,
            TabCommand::SelectIndex1,
            TabCommand::SelectIndex2,
            TabCommand::SelectIndex3,
            TabCommand::SelectIndex4,
            TabCommand::SelectIndex5,
            TabCommand::SelectIndex6,
            TabCommand::SelectIndex7,
            TabCommand::SelectIndex8,
            TabCommand::SelectLast,
            TabCommand::SwapPrev,
            TabCommand::SwapNext,
        ];
        for command in commands {
            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(AppCommand::Layout(LayoutCommand::Tab(command)));
        }
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab {
                    url: Some("https://example.com".to_string()),
                },
            )));

        app.update();

        let requests: Vec<TabRequest> = app
            .world_mut()
            .resource_mut::<Messages<TabRequest>>()
            .drain()
            .collect();
        assert_eq!(
            requests,
            [
                TabRequest::Close,
                TabRequest::Create,
                TabRequest::Focus(TabFocus::Sibling(SiblingDirection::Next)),
                TabRequest::Focus(TabFocus::Sibling(SiblingDirection::Previous)),
                TabRequest::Focus(TabFocus::Index(0)),
                TabRequest::Focus(TabFocus::Index(1)),
                TabRequest::Focus(TabFocus::Index(2)),
                TabRequest::Focus(TabFocus::Index(3)),
                TabRequest::Focus(TabFocus::Index(4)),
                TabRequest::Focus(TabFocus::Index(5)),
                TabRequest::Focus(TabFocus::Index(6)),
                TabRequest::Focus(TabFocus::Index(7)),
                TabRequest::Focus(TabFocus::Last),
                TabRequest::Move(SiblingDirection::Previous),
                TabRequest::Move(SiblingDirection::Next),
                TabRequest::Open {
                    url: Some("https://example.com".to_string()),
                },
            ]
        );
    }

    #[test]
    fn bookmark_visibility_and_window_commands_route_to_owned_requests() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppCommand>()
            .add_plugins(LayoutCommandPlugin);
        let commands = [
            AppCommand::Bookmark(BookmarkCommand::ToggleActive),
            AppCommand::Bookmark(BookmarkCommand::PinActive),
            AppCommand::Bookmark(BookmarkCommand::NewFolder),
            AppCommand::Layout(LayoutCommand::ToggleLayout(ToggleLayoutCommand::Toggle)),
            AppCommand::Layout(LayoutCommand::Window(WindowCommand::Minimize)),
        ];
        for command in commands {
            app.world_mut()
                .resource_mut::<Messages<AppCommand>>()
                .write(command);
        }

        app.update();

        let toggle_requests: Vec<ToggleActiveRequest> = app
            .world_mut()
            .resource_mut::<Messages<ToggleActiveRequest>>()
            .drain()
            .collect();
        assert_eq!(toggle_requests, [ToggleActiveRequest]);
        let pin_requests: Vec<PinActiveRequest> = app
            .world_mut()
            .resource_mut::<Messages<PinActiveRequest>>()
            .drain()
            .collect();
        assert_eq!(pin_requests, [PinActiveRequest]);
        let folder_requests: Vec<CreateFolderRequest> = app
            .world_mut()
            .resource_mut::<Messages<CreateFolderRequest>>()
            .drain()
            .collect();
        assert_eq!(folder_requests, [CreateFolderRequest]);
        let visibility_requests: Vec<LayoutVisibilityRequest> = app
            .world_mut()
            .resource_mut::<Messages<LayoutVisibilityRequest>>()
            .drain()
            .collect();
        assert_eq!(visibility_requests, [LayoutVisibilityRequest::Toggle]);
        let minimize_requests: Vec<MinimizeFocusedWindow> = app
            .world_mut()
            .resource_mut::<Messages<MinimizeFocusedWindow>>()
            .drain()
            .collect();
        assert_eq!(minimize_requests, [MinimizeFocusedWindow]);
    }
}
