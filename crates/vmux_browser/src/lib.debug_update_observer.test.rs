use super::*;
use bevy_cef::prelude::BinReceive;

#[test]
fn debug_ready_sets_state_then_clear_resets() {
    let mut app = App::new();
    app.init_resource::<UpdateState>()
        .add_observer(on_debug_update_ready)
        .add_observer(on_debug_update_clear);

    app.world_mut().trigger(BinReceive::<DebugUpdateReady> {
        webview: Entity::PLACEHOLDER,
        payload: DebugUpdateReady {
            version: "v9.0.0".into(),
        },
    });
    assert_eq!(
        *app.world().resource::<UpdateState>(),
        UpdateState::Ready {
            version: "v9.0.0".into()
        }
    );

    app.world_mut().trigger(BinReceive::<DebugUpdateClear> {
        webview: Entity::PLACEHOLDER,
        payload: DebugUpdateClear,
    });
    assert_eq!(*app.world().resource::<UpdateState>(), UpdateState::Idle);
}
