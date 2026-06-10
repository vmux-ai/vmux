use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, CefKeyboardTarget};
use vmux_layout::Header;
use vmux_layout::command_bar::handler::is_command_bar_open;
use vmux_layout::scene::InteractionMode;
use vmux_layout::side_sheet::SideSheet;
use vmux_layout::stack::FocusedStack;
use vmux_layout::window::Modal;
use vmux_terminal::Terminal;

use crate::Browser;

/// Which surface should own keyboard first-responder for the active page in User (browse) mode.
///
/// Windowed web pages need their native `NSView` to be first-responder to type. Terminals are OSR
/// and route keys through winit → Bevy → PTY, so the winit host window must hold first-responder
/// instead. Switching between the two requires actively handing first-responder back and forth,
/// because a focused web page's `NSView` otherwise keeps it and blacks out the host keyboard.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFocusIntent {
    /// Not in User mode — leave focus untouched (OSR/Player path owns it).
    #[default]
    Unmanaged,
    /// Active page is a windowed web page; give this webview native first-responder.
    Windowed(Entity),
    /// Active page is a terminal, or there is none — the winit host window must own first-responder.
    WinitHost,
}

pub(crate) fn host_focus_intent(
    active_webview: Option<Entity>,
    is_terminal: bool,
) -> HostFocusIntent {
    match active_webview {
        Some(webview) if !is_terminal => HostFocusIntent::Windowed(webview),
        _ => HostFocusIntent::WinitHost,
    }
}

pub(crate) fn compute_host_focus_intent(
    mode: Res<InteractionMode>,
    focus: Res<FocusedStack>,
    child_of_q: Query<&ChildOf>,
    content_q: Query<Entity, (With<Browser>, Without<Header>, Without<SideSheet>)>,
    terminal_q: Query<(), With<Terminal>>,
    modal_q: Query<(&Node, Has<CefKeyboardTarget>), With<Modal>>,
    mut intent: ResMut<HostFocusIntent>,
) {
    // While the command bar owns native focus, leave content focus alone — otherwise we would
    // re-steal first-responder from the command bar every frame.
    let next = if *mode != InteractionMode::User || is_command_bar_open(&modal_q) {
        HostFocusIntent::Unmanaged
    } else {
        let active = focus.stack.and_then(|stack| {
            content_q.iter().find(|&webview| {
                child_of_q
                    .get(webview)
                    .map(|child_of| child_of.get() == stack)
                    .unwrap_or(false)
            })
        });
        let is_terminal = active.is_some_and(|webview| terminal_q.contains(webview));
        host_focus_intent(active, is_terminal)
    };
    set_intent(&mut intent, next);
}

fn set_intent(intent: &mut ResMut<HostFocusIntent>, next: HostFocusIntent) {
    if **intent != next {
        **intent = next;
    }
}

pub(crate) fn apply_windowed_host_focus(intent: Res<HostFocusIntent>, browsers: NonSend<Browsers>) {
    // Runs every frame so a page that becomes active before its browser exists still gets focused
    // once the browser is created. `set_windowed_focus` is a no-op until then.
    if let HostFocusIntent::Windowed(webview) = *intent
        && browsers.has_browser(webview)
    {
        browsers.set_windowed_focus(&webview, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<HostFocusIntent>()
            .insert_resource(InteractionMode::User)
            .insert_resource(FocusedStack::default())
            .add_systems(Update, compute_host_focus_intent);
        app
    }

    fn intent(app: &App) -> HostFocusIntent {
        *app.world().resource::<HostFocusIntent>()
    }

    #[test]
    fn web_child_of_active_stack_intends_windowed() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let page = app.world_mut().spawn((Browser, ChildOf(stack))).id();
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::Windowed(page));
    }

    #[test]
    fn terminal_child_of_active_stack_intends_winit_host() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, Terminal, ChildOf(stack)));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn no_active_stack_intends_winit_host() {
        let mut app = app();
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn player_mode_is_unmanaged() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.insert_resource(InteractionMode::Player);
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::Unmanaged);
    }
}
