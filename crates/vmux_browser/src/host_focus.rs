use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, CefKeyboardTarget, WebviewWindowed};
use vmux_layout::Header;
use vmux_layout::command_bar::state::CommandBarState;
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
    modal_q: Query<
        (
            Entity,
            &Node,
            Option<&Visibility>,
            Has<CefKeyboardTarget>,
            Has<WebviewWindowed>,
        ),
        With<Modal>,
    >,
    layout_keyboard_q: Query<(), (With<Browser>, crate::LayoutKeyboardCapture)>,
    mut intent: ResMut<HostFocusIntent>,
) {
    let next = if let Some((modal, windowed)) =
        modal_q
            .iter()
            .find_map(|(entity, node, visibility, keyboard_target, windowed)| {
                CommandBarState::from_modal(
                    node.display,
                    visibility.copied().unwrap_or_default(),
                    keyboard_target,
                )
                .owns_input()
                .then_some((entity, windowed))
            }) {
        // A windowed command bar hosts a real DOM text field, so Chromium must receive the
        // keystrokes itself — `send_key_event` forwarding is a windowless API and produces no DOM
        // key events here. Escape and Ctrl-C are intercepted by the `NSEvent` monitor before the
        // event reaches the view, so dismiss still works while the bar holds first responder.
        if windowed {
            HostFocusIntent::Windowed(modal)
        } else {
            HostFocusIntent::WinitHost
        }
    } else if *mode != InteractionMode::User {
        HostFocusIntent::Unmanaged
    } else if !layout_keyboard_q.is_empty() {
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
#[path = "host_focus.test.rs"]
mod tests;
