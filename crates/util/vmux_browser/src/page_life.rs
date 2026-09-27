use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_layout::{Browser, Loading};
use vmux_layout::{
    NavigationState,
    pane::{Pane, PaneSplit},
    stack::{Stack, stack_bundle},
};

use crate::WebviewLoadCompleted;
pub(crate) struct PageLifePlugin;

impl Plugin for PageLifePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                apply_fallback_page_icons.after(vmux_layout::apply_cef_state_from_webview),
                drain_loading_state,
                spawn_popup_stacks,
            ),
        );
    }
}

pub(crate) fn drain_loading_state(
    receiver: Res<WebviewLoadingStateReceiver>,
    mut commands: Commands,
    mut completed: MessageWriter<WebviewLoadCompleted>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        let Ok(mut ecmds) = commands.get_entity(ev.webview) else {
            continue;
        };
        if ev.is_loading {
            ecmds.insert(Loading);
        } else {
            ecmds.remove::<Loading>();
            completed.write(WebviewLoadCompleted {
                webview: ev.webview,
            });
        }
        ecmds.insert(NavigationState {
            can_go_back: ev.can_go_back,
            can_go_forward: ev.can_go_forward,
        });
    }
}

pub(crate) fn spawn_popup_stacks(
    popup_rx: Res<WebviewPopupReceiver>,
    extension_popups: Query<(), With<crate::extensions::ExtensionPopup>>,
    child_of_q: Query<&ChildOf>,
    stack_q: Query<(), With<Stack>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
) {
    while let Ok(ev) = popup_rx.0.try_recv() {
        if ev.target_url.is_empty() {
            continue;
        }
        if extension_popups.contains(ev.webview) {
            commands.trigger(RequestNavigate {
                webview: ev.webview,
                url: ev.target_url,
            });
            continue;
        }
        let Ok(stack_co) = child_of_q.get(ev.webview) else {
            continue;
        };
        let stack = stack_co.get();
        if !stack_q.contains(stack) {
            continue;
        }
        let Ok(pane_co) = child_of_q.get(stack) else {
            continue;
        };
        let pane = pane_co.get();
        if !leaf_panes.contains(pane) {
            continue;
        }
        let new_stack = commands
            .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
            .id();
        commands.spawn((Browser::new(&ev.target_url), ChildOf(new_stack)));
    }
}

fn apply_fallback_page_icons(mut metas: Query<&mut PageMetadata, Changed<PageMetadata>>) {
    for mut meta in &mut metas {
        if !meta.icon.is_none() {
            continue;
        }
        if meta.url.starts_with("file:") {
            meta.icon = vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Files);
        } else if meta.url.starts_with("chrome-extension://") {
            meta.icon = vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Puzzle);
        }
    }
}

#[cfg(test)]
mod apply_fallback_page_icons_tests {
    use super::*;
    use vmux_core::{BuiltinIcon, PageIcon, PageMetadata};

    fn resolve(url: &str, seed: PageIcon) -> PageIcon {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, apply_fallback_page_icons);
        let entity = app
            .world_mut()
            .spawn(PageMetadata {
                title: String::new(),
                url: url.to_string(),
                icon: seed,
                bg_color: None,
            })
            .id();
        app.update();
        app.world()
            .get::<PageMetadata>(entity)
            .unwrap()
            .icon
            .clone()
    }

    #[test]
    fn file_url_gets_files_icon() {
        assert_eq!(
            resolve("file:///a/b.rs", PageIcon::None),
            PageIcon::Builtin(BuiltinIcon::Files)
        );
    }

    #[test]
    fn vmux_page_waits_for_its_favicon() {
        assert_eq!(resolve("vmux://team/", PageIcon::None), PageIcon::None);
    }

    #[test]
    fn existing_favicon_is_not_overwritten() {
        assert_eq!(
            resolve("vmux://team/", PageIcon::Favicon("x".into())),
            PageIcon::Favicon("x".into())
        );
    }
}
