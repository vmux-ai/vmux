use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, CefKeyboardTarget};
use vmux_layout::Header;
use vmux_layout::bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive};
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
    bookmark_input_q: Query<
        (),
        (
            With<Browser>,
            Or<(
                With<BookmarkTextInputActive>,
                With<BookmarkContextMenuActive>,
            )>,
        ),
    >,
    mut intent: ResMut<HostFocusIntent>,
) {
    // While the command bar owns native focus, leave content focus alone — otherwise we would
    // re-steal first-responder from the command bar every frame.
    let next = if *mode != InteractionMode::User || is_command_bar_open(&modal_q) {
        HostFocusIntent::Unmanaged
    } else if !bookmark_input_q.is_empty() {
        HostFocusIntent::WinitHost
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

fn windowed_focus_action(
    intent: HostFocusIntent,
    has_browser: bool,
    has_native_focus: Option<bool>,
    focused: &mut Option<Entity>,
) -> Option<Entity> {
    match intent {
        HostFocusIntent::Windowed(webview) if has_browser => {
            let should_focus = has_native_focus
                .map(|has_focus| !has_focus)
                .unwrap_or(*focused != Some(webview));
            *focused = Some(webview);
            should_focus.then_some(webview)
        }
        _ => {
            *focused = None;
            None
        }
    }
}

pub(crate) fn apply_windowed_host_focus(
    intent: Res<HostFocusIntent>,
    browsers: NonSend<Browsers>,
    mut focused: Local<Option<Entity>>,
) {
    let (has_browser, has_native_focus) = match *intent {
        HostFocusIntent::Windowed(webview) => (
            browsers.has_browser(webview),
            browsers.windowed_has_native_focus(&webview),
        ),
        _ => (false, None),
    };
    if let Some(webview) =
        windowed_focus_action(*intent, has_browser, has_native_focus, &mut focused)
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
    fn bookmark_text_input_reclaims_winit_host_focus() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((Browser, BookmarkTextInputActive));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn bookmark_context_menu_reclaims_winit_host_focus() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((Browser, BookmarkContextMenuActive));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn windowed_focus_action_focuses_available_target_once() {
        let webview = Entity::from_bits(1);
        let mut focused = None;

        assert_eq!(
            windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
            Some(webview)
        );
        assert_eq!(focused, Some(webview));
        assert_eq!(
            windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
            None
        );
        assert_eq!(focused, Some(webview));
    }

    #[test]
    fn windowed_focus_action_refocuses_after_browser_reappears() {
        let webview = Entity::from_bits(1);
        let mut focused = None;

        assert_eq!(
            windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
            Some(webview)
        );
        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                false,
                None,
                &mut focused,
            ),
            None
        );
        assert_eq!(focused, None);
        assert_eq!(
            windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
            Some(webview)
        );
    }

    #[test]
    fn windowed_focus_action_recovers_lost_native_focus() {
        let webview = Entity::from_bits(1);
        let mut focused = Some(webview);

        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                Some(false),
                &mut focused,
            ),
            Some(webview)
        );
    }

    #[test]
    fn windowed_focus_action_preserves_held_native_focus() {
        let webview = Entity::from_bits(1);
        let mut focused = Some(webview);

        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                Some(true),
                &mut focused,
            ),
            None
        );
    }

    #[test]
    fn windowed_focus_action_focuses_changed_target() {
        let previous = Entity::from_bits(1);
        let next = Entity::from_bits(2);
        let mut focused = Some(previous);

        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(next),
                true,
                Some(false),
                &mut focused,
            ),
            Some(next)
        );
        assert_eq!(focused, Some(next));
    }

    #[test]
    fn windowed_focus_action_clears_cache_for_winit_host() {
        let mut focused = Some(Entity::from_bits(1));

        assert_eq!(
            windowed_focus_action(HostFocusIntent::WinitHost, false, None, &mut focused),
            None
        );
        assert_eq!(focused, None);
    }

    #[test]
    fn windowed_focus_action_clears_cache_when_unmanaged() {
        let mut focused = Some(Entity::from_bits(1));

        assert_eq!(
            windowed_focus_action(HostFocusIntent::Unmanaged, false, None, &mut focused),
            None
        );
        assert_eq!(focused, None);
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
