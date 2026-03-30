use bevy::app::AppExit;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::component::{AppAction, AppInputRoot, VmuxPrefixState};

pub(crate) fn spawn_app_input(mut commands: Commands) {
    let mut input_map = InputMap::<AppAction>::default();
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Super, KeyCode::KeyQ),
    );
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Control, KeyCode::KeyQ),
    );
    commands.spawn((
        AppInputRoot,
        VmuxPrefixState::default(),
        input_map,
        ActionState::<AppAction>::default(),
    ));
}

pub(crate) fn exit_on_quit_action(
    query: Query<&ActionState<AppAction>, With<AppInputRoot>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Ok(state) = query.single() else {
        return;
    };
    if state.just_pressed(&AppAction::Quit) {
        app_exit.write(AppExit::Success);
    }
}
