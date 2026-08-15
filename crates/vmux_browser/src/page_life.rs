//! What happens to a page between opening and settling.
//!
//! The icon and loading state arrive from CEF after the navigation is applied, which is why these
//! run after `apply_cef_state_from_webview`; a popup asks for a stack of its own.

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
                apply_page_icons.after(vmux_layout::apply_cef_state_from_webview),
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
    child_of_q: Query<&ChildOf>,
    stack_q: Query<(), With<Stack>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut commands: Commands,
) {
    while let Ok(ev) = popup_rx.0.try_recv() {
        if ev.target_url.is_empty() {
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

fn apply_page_icons(
    manifests: Query<&vmux_core::page::PageManifest>,
    mut metas: Query<&mut PageMetadata, Changed<PageMetadata>>,
) {
    for mut meta in &mut metas {
        if meta.icon.is_none() {
            if meta.url.starts_with("file:") {
                meta.icon = vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Files);
                continue;
            }
            if meta.url.starts_with("chrome-extension://") {
                meta.icon = vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Puzzle);
                continue;
            }
        }
        let Some(host) = meta
            .url
            .strip_prefix("vmux://")
            .and_then(|rest| rest.split('/').next())
            .filter(|host| !host.is_empty() && *host != "agent")
        else {
            continue;
        };
        let Some(manifest) = manifests.iter().find(|manifest| manifest.host == host) else {
            continue;
        };
        if meta.icon.is_none()
            && let Some(builtin) = manifest.icon
        {
            meta.icon = vmux_core::PageIcon::Builtin(builtin);
        }
        if !manifest.title.is_empty() && meta.title == meta.url {
            meta.title = manifest.title.to_string();
        }
    }
}

#[cfg(test)]
mod apply_page_icons_tests {
    use super::*;
    use vmux_core::page::PageManifest;
    use vmux_core::{BuiltinIcon, PageIcon, PageMetadata};

    fn resolve(url: &str, seed: PageIcon, manifests: &[PageManifest]) -> PageIcon {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, apply_page_icons);
        for manifest in manifests {
            app.world_mut().spawn(*manifest);
        }
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

    const TEAM: PageManifest = PageManifest {
        host: "team",
        title: "Team",
        title_message_id: Some("team-title"),
        replaces_command: None,
        keywords: &[],
        icon: Some(BuiltinIcon::Users),
        command_bar: true,
    };
    const AGENT: PageManifest = PageManifest {
        host: "agent",
        title: "Agent",
        title_message_id: None,
        replaces_command: None,
        keywords: &[],
        icon: Some(BuiltinIcon::Sparkles),
        command_bar: false,
    };

    #[test]
    fn vmux_page_gets_manifest_builtin_icon() {
        assert_eq!(
            resolve("vmux://team/", PageIcon::None, &[TEAM]),
            PageIcon::Builtin(BuiltinIcon::Users)
        );
    }

    #[test]
    fn file_url_gets_files_icon() {
        assert_eq!(
            resolve("file:///a/b.rs", PageIcon::None, &[]),
            PageIcon::Builtin(BuiltinIcon::Files)
        );
    }

    #[test]
    fn agent_cli_session_keeps_none_for_provider_favicon() {
        assert_eq!(
            resolve("vmux://agent/vibe/abc", PageIcon::None, &[AGENT]),
            PageIcon::None
        );
    }

    #[test]
    fn existing_favicon_is_not_overwritten() {
        assert_eq!(
            resolve("vmux://team/", PageIcon::Favicon("x".into()), &[TEAM]),
            PageIcon::Favicon("x".into())
        );
    }

    fn resolve_title(url: &str, seed_title: &str, manifests: &[PageManifest]) -> String {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, apply_page_icons);
        for manifest in manifests {
            app.world_mut().spawn(*manifest);
        }
        let entity = app
            .world_mut()
            .spawn(PageMetadata {
                title: seed_title.to_string(),
                url: url.to_string(),
                icon: PageIcon::None,
                bg_color: None,
            })
            .id();
        app.update();
        app.world()
            .get::<PageMetadata>(entity)
            .unwrap()
            .title
            .clone()
    }

    #[test]
    fn raw_url_title_is_replaced_with_manifest_title() {
        assert_eq!(
            resolve_title("vmux://team/", "vmux://team/", &[TEAM]),
            "Team"
        );
    }

    #[test]
    fn handler_set_title_is_preserved() {
        assert_eq!(resolve_title("vmux://team/", "Custom", &[TEAM]), "Custom");
    }
}
