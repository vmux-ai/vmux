use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, WebviewWindowed};
use vmux_core::KeyboardOwner;
use vmux_core::overlay::{OverlayState, OverlayStateQuery, WindowOverlay};
use vmux_layout::Header;
use vmux_layout::side_sheet::SideSheet;
use vmux_layout::stack::FocusedStack;
use vmux_terminal::Terminal;

use crate::Browser;
use vmux_flex::prelude::*;

#[cfg(target_os = "macos")]
#[path = "host_focus/macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "host_focus/other.rs"]
mod platform;

pub(crate) struct HostFocusPlugin;

impl Plugin for HostFocusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HostFocusIntent>()
            .add_systems(Update, publish_native_page_owns_escape)
            .add_systems(
                PostUpdate,
                (compute_host_focus_intent, apply_windowed_host_focus)
                    .chain()
                    .after(crate::present::sync_windowed_frames)
                    .after(crate::present::sync_windowed_command_bar),
            )
            .add_plugins(platform::HostFocusPlatformPlugin);
    }
}

fn page_owns_escape(terminal_focused: bool, overlay_open: bool) -> bool {
    terminal_focused || overlay_open
}

fn publish_native_page_owns_escape(
    terminal_focus_q: Query<(), (With<Terminal>, With<KeyboardOwner>)>,
    overlay_q: OverlayStateQuery,
) {
    let overlay_owns_input = OverlayState::of_any(&overlay_q).owns_input();
    crate::set_native_page_owns_escape(page_owns_escape(
        !terminal_focus_q.is_empty(),
        overlay_owns_input,
    ));
    crate::set_native_text_entry_owns_keys(overlay_owns_input);
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFocusIntent {
    Windowed(Entity),
    LayoutView,
    NativePane(Entity),
    #[default]
    WinitHost,
}

pub(crate) fn host_focus_intent(
    active_webview: Option<Entity>,
    is_native: bool,
) -> HostFocusIntent {
    match active_webview {
        Some(webview) if is_native => HostFocusIntent::NativePane(webview),
        Some(webview) => HostFocusIntent::Windowed(webview),
        None => HostFocusIntent::WinitHost,
    }
}

pub(crate) fn compute_host_focus_intent(
    focus: Res<FocusedStack>,
    child_of_q: Query<&ChildOf>,
    content_q: Query<Entity, (With<Browser>, Without<Header>, Without<SideSheet>)>,
    modal_q: Query<
        (
            Entity,
            &Node,
            Option<&Visibility>,
            Has<KeyboardOwner>,
            Has<WebviewWindowed>,
            Has<vmux_core::overlay::OverlayShownInline>,
        ),
        With<WindowOverlay>,
    >,
    layout_keyboard_q: Query<(), crate::present::LayoutKeyboardHost>,
    native_q: Query<(), With<vmux_core::host::page::HostsPage>>,
    mut intent: ResMut<HostFocusIntent>,
) {
    let next = if let Some((modal, windowed, shown_inline)) = modal_q.iter().find_map(
        |(entity, node, visibility, keyboard_target, windowed, shown_inline)| {
            OverlayState::of(
                node.display,
                visibility.copied().unwrap_or_default(),
                keyboard_target,
                shown_inline,
            )
            .owns_input()
            .then_some((entity, windowed, shown_inline))
        },
    ) {
        if shown_inline {
            HostFocusIntent::LayoutView
        } else if windowed {
            HostFocusIntent::Windowed(modal)
        } else {
            HostFocusIntent::WinitHost
        }
    } else if !layout_keyboard_q.is_empty() {
        HostFocusIntent::LayoutView
    } else {
        let active = focus.stack.and_then(|stack| {
            content_q.iter().find(|&webview| {
                child_of_q
                    .get(webview)
                    .map(|child_of| child_of.get() == stack)
                    .unwrap_or(false)
            })
        });
        let is_native = active.is_some_and(|webview| native_q.contains(webview));
        host_focus_intent(active, is_native)
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
    reclaiming: bool,
) -> Option<Entity> {
    match intent {
        HostFocusIntent::Windowed(webview) if has_browser => {
            let should_focus = reclaiming
                || has_native_focus
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
    mut was_layout_view: Local<bool>,
) {
    let reclaiming = *was_layout_view && *intent != HostFocusIntent::LayoutView;
    *was_layout_view = *intent == HostFocusIntent::LayoutView;
    let (has_browser, has_native_focus) = match *intent {
        HostFocusIntent::Windowed(webview) => (
            browsers.has_browser(webview),
            browsers.windowed_has_native_focus(&webview),
        ),
        _ => (false, None),
    };
    if let Some(webview) = windowed_focus_action(
        *intent,
        has_browser,
        has_native_focus,
        &mut focused,
        reclaiming,
    ) {
        browsers.set_windowed_focus(&webview, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::command_bar::panel::CommandBarPanelActive;
    use vmux_layout::bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive};
    use vmux_layout::cef::LayoutCef;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<HostFocusIntent>()
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
    fn terminal_child_of_active_stack_intends_its_own_view() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let terminal = app
            .world_mut()
            .spawn((
                Browser,
                Terminal,
                vmux_core::host::page::HostsPage,
                ChildOf(stack),
            ))
            .id();
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::NativePane(terminal));
    }

    #[test]
    fn no_active_stack_intends_winit_host() {
        let mut app = app();
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn open_osr_command_bar_reclaims_winit_host_focus() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((
            WindowOverlay,
            Node {
                display: Display::Flex,
                ..default()
            },
            KeyboardOwner,
        ));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn open_windowed_command_bar_takes_native_focus() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        let modal = app
            .world_mut()
            .spawn((
                WindowOverlay,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                KeyboardOwner,
                WebviewWindowed,
            ))
            .id();
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });

        app.update();

        assert_eq!(intent(&app), HostFocusIntent::Windowed(modal));
    }

    #[test]
    fn revealing_windowed_command_bar_keeps_focus_off_the_page() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        let modal = app
            .world_mut()
            .spawn((
                WindowOverlay,
                Node {
                    display: Display::Flex,
                    ..default()
                },
                Visibility::Hidden,
                KeyboardOwner,
                WebviewWindowed,
            ))
            .id();
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });

        app.update();

        assert_eq!(intent(&app), HostFocusIntent::Windowed(modal));
    }

    #[test]
    fn revealing_osr_command_bar_keeps_focus_off_the_page() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((
            WindowOverlay,
            Node {
                display: Display::Flex,
                ..default()
            },
            Visibility::Hidden,
            KeyboardOwner,
        ));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });

        app.update();

        assert_eq!(intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn bookmark_text_input_gives_the_caret_to_the_layout_view() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((LayoutCef, BookmarkTextInputActive));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::LayoutView);
    }

    #[test]
    fn bookmark_context_menu_gives_the_caret_to_the_layout_view() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut()
            .spawn((LayoutCef, BookmarkContextMenuActive));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();
        assert_eq!(intent(&app), HostFocusIntent::LayoutView);
    }

    #[test]
    fn an_overlay_the_layout_page_draws_leaves_the_caret_with_the_layout_view() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((LayoutCef, CommandBarPanelActive));
        app.world_mut().spawn((
            WindowOverlay,
            Node::default(),
            Visibility::Hidden,
            WebviewWindowed,
            vmux_core::overlay::OverlayShownInline,
        ));
        app.insert_resource(FocusedStack {
            stack: Some(stack),
            ..default()
        });
        app.update();

        assert_eq!(
            intent(&app),
            HostFocusIntent::LayoutView,
            "focusing the overlay's own webview takes the responder off the surface showing the field"
        );
    }

    #[test]
    fn windowed_focus_action_focuses_available_target_once() {
        let webview = Entity::from_bits(1);
        let mut focused = None;

        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                None,
                &mut focused,
                false
            ),
            Some(webview)
        );
        assert_eq!(focused, Some(webview));
        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                None,
                &mut focused,
                false
            ),
            None
        );
        assert_eq!(focused, Some(webview));
    }

    #[test]
    fn windowed_focus_action_refocuses_after_browser_reappears() {
        let webview = Entity::from_bits(1);
        let mut focused = None;

        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                None,
                &mut focused,
                false
            ),
            Some(webview)
        );
        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                false,
                None,
                &mut focused,
                false,
            ),
            None
        );
        assert_eq!(focused, None);
        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                None,
                &mut focused,
                false
            ),
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
                false,
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
                false,
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
                false,
            ),
            Some(next)
        );
        assert_eq!(focused, Some(next));
    }

    #[test]
    fn leaving_the_layout_view_refocuses_the_pane_despite_cef_claiming_focus() {
        let webview = Entity::from_bits(1);
        let mut focused = Some(webview);

        assert_eq!(
            windowed_focus_action(
                HostFocusIntent::Windowed(webview),
                true,
                Some(true),
                &mut focused,
                true,
            ),
            Some(webview)
        );
    }

    #[test]
    fn windowed_focus_action_clears_cache_for_winit_host() {
        let mut focused = Some(Entity::from_bits(1));

        assert_eq!(
            windowed_focus_action(HostFocusIntent::WinitHost, false, None, &mut focused, false),
            None
        );
        assert_eq!(focused, None);
    }

    #[test]
    fn windowed_focus_action_clears_cache_when_unmanaged() {
        let mut focused = Some(Entity::from_bits(1));

        assert_eq!(
            windowed_focus_action(HostFocusIntent::WinitHost, false, None, &mut focused, false),
            None
        );
        assert_eq!(focused, None);
    }
}
