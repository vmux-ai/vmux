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
    keywords: &[],
    icon: Some(BuiltinIcon::Users),
    command_bar: true,
};
const AGENT: PageManifest = PageManifest {
    host: "agent",
    title: "Agent",
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
