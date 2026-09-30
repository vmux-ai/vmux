use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::SwitchTabRequest;
use vmux_command::ReadCommandRequests;
use vmux_command::command_bar::CommandBarDismiss;
use vmux_core::launcher::StackInPaneChosen;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LayoutRequestSet {
    Dispatch,
    Prepare,
    Handle,
}

pub(super) struct LayoutRequestPlugin;

impl Plugin for LayoutRequestPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                LayoutRequestSet::Dispatch,
                LayoutRequestSet::Prepare,
                LayoutRequestSet::Handle,
            )
                .chain()
                .in_set(ReadCommandRequests),
        )
        .add_plugins(UiEventPlugin::<(SwitchTabRequest,)>::default())
        .add_observer(switch_tab);
    }
}

fn switch_tab(
    trigger: On<UiInput<SwitchTabRequest>>,
    mut chosen: MessageWriter<StackInPaneChosen>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let request = &trigger.event().payload;
    chosen.write(StackInPaneChosen {
        pane_bits: request.pane,
        index: request.index,
    });
    commands.trigger(CommandBarDismiss::new(webview, true));
}
