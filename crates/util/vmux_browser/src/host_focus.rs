use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, HostWindow, WebviewWindowed};
use vmux_ecs::KeyboardOwner;
use vmux_ecs::overlay::{Overlay, OverlayState, WindowOverlay};
use vmux_input::{KeyboardContext, KeyboardContextSet};
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

impl bevy::prelude::Plugin for HostFocusPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, sync_keyboard_context.in_set(KeyboardContextSet))
            .add_systems(
                PostUpdate,
                (initialize, compute_intent, apply_windowed)
                    .chain()
                    .in_set(crate::BrowserSystemSet::HostFocusApplied)
                    .after(crate::BrowserSystemSet::SyncWindowedFrames)
                    .after(crate::BrowserSystemSet::SyncWindowedCommandBar)
                    .after(crate::BrowserOverlaySet),
            )
            .add_plugins(platform::HostFocusPlatformPlugin);
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Host focus"),
        HostFocusIntent::default(),
        KeyboardContext::default(),
    ));
}

fn page_owns_escape(terminal_focused: bool, overlay_open: bool) -> bool {
    terminal_focused || overlay_open
}

fn sync_keyboard_context(
    terminal_focus_q: Query<(), (With<Terminal>, With<KeyboardOwner>)>,
    overlay: Overlay,
    mut context: Single<&mut KeyboardContext>,
) {
    let overlay_owns_input = overlay.owns_input();
    **context = KeyboardContext {
        page_owns_escape: page_owns_escape(!terminal_focus_q.is_empty(), overlay_owns_input),
        text_entry_owns_keys: overlay_owns_input,
    };
}

#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFocusIntent {
    Windowed(Entity),
    LayoutView,
    HostedPane(Entity),
    #[default]
    WinitHost,
}

fn host_focus_intent(active_webview: Option<Entity>, is_hosted: bool) -> HostFocusIntent {
    match active_webview {
        Some(webview) if is_hosted => HostFocusIntent::HostedPane(webview),
        Some(webview) => HostFocusIntent::Windowed(webview),
        None => HostFocusIntent::WinitHost,
    }
}

fn compute_intent(
    focus: FocusedStack,
    child_of_q: Query<&ChildOf>,
    content_q: Query<
        Entity,
        (
            With<Browser>,
            Without<Header>,
            Without<SideSheet>,
            Without<vmux_layout::hosted_page::SurfaceHandoff>,
            Without<vmux_layout::hosted_page::SurfaceBlocked>,
        ),
    >,
    modal_q: Query<
        (
            Entity,
            &Node,
            Option<&Visibility>,
            Has<KeyboardOwner>,
            Has<WebviewWindowed>,
            Has<vmux_ecs::overlay::OverlayShownInline>,
        ),
        With<WindowOverlay>,
    >,
    layout_keyboard_q: Query<(Entity, Option<&HostWindow>), crate::present::LayoutKeyboardHost>,
    focused_window: vmux_layout::window::FocusedWindow,
    hosted: Query<(), With<vmux_ecs::page::HostsPage>>,
    mut intent: Single<&mut HostFocusIntent>,
) {
    let next = if let Some((modal, windowed, shown_inline)) = modal_q.iter().find_map(
        |(entity, node, visibility, keyboard_target, windowed, shown_inline)| {
            OverlayState::resolve(
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
    } else if layout_keyboard_q.iter().any(|(_, host)| match host {
        Some(host) => focused_window
            .entity()
            .is_none_or(|focused| focused == host.0),
        None => true,
    }) {
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
        let is_hosted = active.is_some_and(|webview| hosted.contains(webview));
        host_focus_intent(active, is_hosted)
    };
    set_intent(&mut intent, next);
}

fn set_intent(intent: &mut HostFocusIntent, next: HostFocusIntent) {
    if *intent != next {
        *intent = next;
    }
}

#[derive(Component, Default)]
struct WindowedFocus {
    surface: Option<Entity>,
    layout_view: bool,
}

fn initialize(
    windows: Query<Entity, (With<bevy::prelude::Window>, Without<WindowedFocus>)>,
    mut commands: bevy::prelude::Commands,
) {
    for window in &windows {
        commands.entity(window).insert(WindowedFocus::default());
    }
}

impl WindowedFocus {
    fn release(&self, previous: Option<Entity>, selected: Option<Entity>) -> Option<Entity> {
        if self.surface == previous || previous == selected {
            return None;
        }
        previous
    }

    fn target(
        &mut self,
        intent: HostFocusIntent,
        has_browser: bool,
        has_native_focus: Option<bool>,
    ) -> Option<Entity> {
        let reclaiming = self.layout_view && intent != HostFocusIntent::LayoutView;
        self.layout_view = intent == HostFocusIntent::LayoutView;
        match intent {
            HostFocusIntent::Windowed(webview) if has_browser => {
                let should_focus = reclaiming
                    || has_native_focus
                        .map(|has_focus| !has_focus)
                        .unwrap_or(self.surface != Some(webview));
                self.surface = Some(webview);
                should_focus.then_some(webview)
            }
            _ => {
                self.surface = None;
                None
            }
        }
    }
}

fn apply_windowed(
    intent: Single<&HostFocusIntent>,
    browsers: NonSend<Browsers>,
    focused_window: vmux_layout::window::FocusedWindow,
    mut windows: Query<(Entity, &mut WindowedFocus), With<bevy::prelude::Window>>,
) {
    let intent = **intent;
    let (has_browser, has_native_focus) = match intent {
        HostFocusIntent::Windowed(webview) => (
            browsers.has_browser(webview),
            browsers.windowed_has_native_focus(&webview),
        ),
        _ => (false, None),
    };
    let active_window = focused_window.entity();
    let selected = match intent {
        HostFocusIntent::Windowed(surface) if active_window.is_some() && has_browser => {
            Some(surface)
        }
        _ => None,
    };
    let mut focus_target = None;
    for (window, mut focus) in &mut windows {
        let previous = focus.surface;
        let target = if Some(window) == active_window {
            focus.target(intent, has_browser, has_native_focus)
        } else {
            focus.target(HostFocusIntent::WinitHost, false, None)
        };
        if let Some(previous) = focus.release(previous, selected) {
            browsers.set_windowed_focus(&previous, false);
        }
        if let Some(webview) = target {
            focus_target = Some(webview);
        }
    }
    if let Some(webview) = focus_target {
        browsers.set_windowed_focus(&webview, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::CommandBarPanelActive;
    use vmux_layout::active_pane::ActiveStack;
    use vmux_layout::bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive};
    use vmux_layout::cef::LayoutCef;

    struct FocusFixture;

    impl FocusFixture {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, HostFocusPlugin))
                .insert_non_send(Browsers::default())
                .init_resource::<ButtonInput<KeyCode>>();
            app.world_mut().spawn(ActiveStack::default().local_bundle());
            app
        }

        fn set_focus(app: &mut App, stack: Entity) {
            let world = app.world_mut();
            let mut query = world.query::<&mut ActiveStack>();
            query.single_mut(world).unwrap().stack = Some(stack);
        }

        fn intent(app: &App) -> HostFocusIntent {
            app.world()
                .iter_entities()
                .find_map(|entity| entity.get::<HostFocusIntent>().copied())
                .unwrap()
        }

        fn keyboard_context(app: &App) -> KeyboardContext {
            app.world()
                .iter_entities()
                .find_map(|entity| entity.get::<KeyboardContext>().copied())
                .unwrap()
        }
    }

    #[test]
    fn transferring_a_surface_between_windows_keeps_its_native_first_responder() {
        let mut app = FocusFixture::app();
        let previous_window = app.world_mut().spawn(Window::default()).id();
        let selected_window = app.world_mut().spawn(Window::default()).id();
        let surface = app.world_mut().spawn_empty().id();
        app.update();
        let mut native_focus = Some(surface);
        let observed_focus = native_focus == Some(surface);
        let mut previous = app
            .world_mut()
            .get_mut::<WindowedFocus>(previous_window)
            .unwrap();
        previous.target(HostFocusIntent::Windowed(surface), true, None);
        let old_surface = previous.surface;
        previous.target(HostFocusIntent::WinitHost, false, None);
        if previous.release(old_surface, Some(surface)) == native_focus {
            native_focus = None;
        }
        let mut selected = app
            .world_mut()
            .get_mut::<WindowedFocus>(selected_window)
            .unwrap();
        if let Some(surface) = selected.target(
            HostFocusIntent::Windowed(surface),
            true,
            Some(observed_focus),
        ) {
            native_focus = Some(surface);
        }
        assert_eq!(native_focus, Some(surface));
    }

    #[test]
    fn existing_windows_keep_independent_native_focus_handoffs() {
        let mut app = App::new();
        let first = app.world_mut().spawn(Window::default()).id();
        let second = app.world_mut().spawn(Window::default()).id();
        let first_page = app.world_mut().spawn_empty().id();
        let second_page = app.world_mut().spawn_empty().id();
        app.add_plugins((MinimalPlugins, HostFocusPlugin))
            .insert_non_send(Browsers::default())
            .init_resource::<ButtonInput<KeyCode>>();
        app.world_mut().spawn(ActiveStack::default().local_bundle());
        app.update();
        app.world_mut()
            .get_mut::<WindowedFocus>(first)
            .unwrap()
            .target(HostFocusIntent::LayoutView, false, None);
        assert_eq!(
            app.world_mut()
                .get_mut::<WindowedFocus>(second)
                .unwrap()
                .target(HostFocusIntent::Windowed(second_page), true, None,),
            Some(second_page)
        );
        assert_eq!(
            app.world_mut()
                .get_mut::<WindowedFocus>(first)
                .unwrap()
                .target(HostFocusIntent::Windowed(first_page), true, Some(true),),
            Some(first_page)
        );
        assert_eq!(
            app.world_mut()
                .get_mut::<WindowedFocus>(second)
                .unwrap()
                .target(HostFocusIntent::Windowed(second_page), true, Some(true),),
            None
        );
    }

    #[test]
    fn web_child_of_active_stack_intends_windowed() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        let page = app.world_mut().spawn((Browser, ChildOf(stack))).id();
        FocusFixture::set_focus(&mut app, stack);
        app.update();
        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::Windowed(page));
    }

    #[test]
    fn blocked_surface_cannot_own_native_focus() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            Browser,
            vmux_layout::hosted_page::SurfaceBlocked,
            ChildOf(stack),
        ));
        FocusFixture::set_focus(&mut app, stack);

        app.update();

        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn incoming_surface_cannot_own_native_focus_before_handoff() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            Browser,
            vmux_layout::hosted_page::SurfaceHandoff::new(
                Vec::new(),
                vmux_ecs::PageOpenId::new(),
                0,
                std::time::Duration::ZERO,
            ),
            ChildOf(stack),
        ));
        FocusFixture::set_focus(&mut app, stack);

        app.update();

        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn terminal_child_of_active_stack_intends_its_own_view() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        let terminal = app
            .world_mut()
            .spawn((Browser, Terminal, vmux_ecs::page::HostsPage, ChildOf(stack)))
            .id();
        FocusFixture::set_focus(&mut app, stack);
        app.update();
        assert_eq!(
            FocusFixture::intent(&app),
            HostFocusIntent::HostedPane(terminal)
        );
    }

    #[test]
    fn no_active_stack_intends_winit_host() {
        let mut app = FocusFixture::app();
        app.update();
        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn overlay_publishes_keyboard_ownership_through_ecs() {
        let mut app = FocusFixture::app();
        app.world_mut().spawn((
            WindowOverlay,
            Node {
                display: Display::Flex,
                ..default()
            },
            KeyboardOwner,
        ));

        app.update();

        assert_eq!(
            FocusFixture::keyboard_context(&app),
            KeyboardContext {
                page_owns_escape: true,
                text_entry_owns_keys: true,
            }
        );
    }

    #[test]
    fn focused_terminal_keeps_escape_without_claiming_text_entry() {
        let mut app = FocusFixture::app();
        app.world_mut().spawn((Terminal, KeyboardOwner));

        app.update();

        assert_eq!(
            FocusFixture::keyboard_context(&app),
            KeyboardContext {
                page_owns_escape: true,
                text_entry_owns_keys: false,
            }
        );
    }

    #[test]
    fn open_osr_command_bar_reclaims_winit_host_focus() {
        let mut app = FocusFixture::app();
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
        FocusFixture::set_focus(&mut app, stack);
        app.update();
        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn open_windowed_command_bar_takes_native_focus() {
        let mut app = FocusFixture::app();
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
        FocusFixture::set_focus(&mut app, stack);

        app.update();

        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::Windowed(modal));
    }

    #[test]
    fn revealing_windowed_command_bar_keeps_focus_off_the_page() {
        let mut app = FocusFixture::app();
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
        FocusFixture::set_focus(&mut app, stack);

        app.update();

        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::Windowed(modal));
    }

    #[test]
    fn revealing_osr_command_bar_keeps_focus_off_the_page() {
        let mut app = FocusFixture::app();
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
        FocusFixture::set_focus(&mut app, stack);

        app.update();

        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::WinitHost);
    }

    #[test]
    fn bookmark_text_input_gives_the_caret_to_the_layout_view() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((LayoutCef, BookmarkTextInputActive));
        FocusFixture::set_focus(&mut app, stack);
        app.update();
        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::LayoutView);
    }

    #[test]
    fn bookmark_context_menu_gives_the_caret_to_the_layout_view() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut()
            .spawn((LayoutCef, BookmarkContextMenuActive));
        FocusFixture::set_focus(&mut app, stack);
        app.update();
        assert_eq!(FocusFixture::intent(&app), HostFocusIntent::LayoutView);
    }

    #[test]
    fn an_overlay_the_layout_page_draws_leaves_the_caret_with_the_layout_view() {
        let mut app = FocusFixture::app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        app.world_mut().spawn((LayoutCef, CommandBarPanelActive));
        app.world_mut().spawn((
            WindowOverlay,
            Node::default(),
            Visibility::Hidden,
            WebviewWindowed,
            vmux_ecs::overlay::OverlayShownInline,
        ));
        FocusFixture::set_focus(&mut app, stack);
        app.update();

        assert_eq!(
            FocusFixture::intent(&app),
            HostFocusIntent::LayoutView,
            "focusing the overlay's own webview takes the responder off the surface showing the field"
        );
    }

    #[test]
    fn windowed_focus_target_focuses_available_target_once() {
        let webview = Entity::from_bits(1);
        let mut focus = WindowedFocus {
            surface: None,
            ..default()
        };

        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, None),
            Some(webview)
        );
        assert_eq!(focus.surface, Some(webview));
        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, None),
            None
        );
        assert_eq!(focus.surface, Some(webview));
    }

    #[test]
    fn windowed_focus_target_refocuses_after_browser_reappears() {
        let webview = Entity::from_bits(1);
        let mut focus = WindowedFocus {
            surface: None,
            ..default()
        };

        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, None),
            Some(webview)
        );
        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), false, None),
            None
        );
        assert_eq!(focus.surface, None);
        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, None),
            Some(webview)
        );
    }

    #[test]
    fn windowed_focus_target_recovers_lost_native_focus() {
        let webview = Entity::from_bits(1);
        let mut focus = WindowedFocus {
            surface: Some(webview),
            ..default()
        };

        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, Some(false)),
            Some(webview)
        );
    }

    #[test]
    fn windowed_focus_target_preserves_held_native_focus() {
        let webview = Entity::from_bits(1);
        let mut focus = WindowedFocus {
            surface: Some(webview),
            ..default()
        };

        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, Some(true)),
            None
        );
    }

    #[test]
    fn windowed_focus_target_focuses_changed_target() {
        let previous = Entity::from_bits(1);
        let next = Entity::from_bits(2);
        let mut focus = WindowedFocus {
            surface: Some(previous),
            ..default()
        };

        assert_eq!(
            focus.target(HostFocusIntent::Windowed(next), true, Some(false)),
            Some(next)
        );
        assert_eq!(focus.surface, Some(next));
    }

    #[test]
    fn leaving_the_layout_view_refocuses_the_pane_despite_cef_claiming_focus() {
        let webview = Entity::from_bits(1);
        let mut focus = WindowedFocus {
            surface: Some(webview),
            ..default()
        };
        focus.layout_view = true;

        assert_eq!(
            focus.target(HostFocusIntent::Windowed(webview), true, Some(true)),
            Some(webview)
        );
    }

    #[test]
    fn windowed_focus_target_clears_cache_for_winit_host() {
        let mut focus = WindowedFocus {
            surface: Some(Entity::from_bits(1)),
            ..default()
        };

        assert_eq!(focus.target(HostFocusIntent::WinitHost, false, None), None);
        assert_eq!(focus.surface, None);
    }

    #[test]
    fn windowed_focus_target_clears_cache_when_unmanaged() {
        let mut focus = WindowedFocus {
            surface: Some(Entity::from_bits(1)),
            ..default()
        };

        assert_eq!(focus.target(HostFocusIntent::WinitHost, false, None), None);
        assert_eq!(focus.surface, None);
    }
}
