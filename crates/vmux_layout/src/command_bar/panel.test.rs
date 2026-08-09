use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_observer(on_command_bar_panel_active);
    app
}

/// The panel must release the keyboard as reliably as it takes it: a stuck marker leaves the
/// layout shell owning `CefKeyboardTarget` and no pane can ever get it back.
#[test]
fn active_event_round_trips_the_marker() {
    let mut app = app();
    let webview = app.world_mut().spawn_empty().id();

    app.world_mut().trigger(BinReceive {
        webview,
        payload: CommandBarPanelActiveEvent { active: true },
    });
    app.update();
    assert!(app.world().get::<CommandBarPanelActive>(webview).is_some());

    app.world_mut().trigger(BinReceive {
        webview,
        payload: CommandBarPanelActiveEvent { active: false },
    });
    app.update();
    assert!(app.world().get::<CommandBarPanelActive>(webview).is_none());
}
