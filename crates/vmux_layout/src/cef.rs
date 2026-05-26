use std::path::PathBuf;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_server::{PageConfig, Server};

use crate::event::LAYOUT_PAGE_URL;

#[derive(Component)]
pub struct Browser;

#[derive(Component)]
pub struct LayoutCef;

#[derive(Component)]
pub struct Loading;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct NavigationState {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub struct LayoutCefPlugin;

impl Plugin for LayoutCefPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().resource_mut::<Server>().register(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            &PageConfig::with_custom_host("layout").with_bundle_dir("dist-layout"),
        );
    }
}

pub fn mirror_metadata_to_url(
    chrome_q: Query<
        &vmux_core::PageMetadata,
        (Without<vmux_core::Url>, Changed<vmux_core::PageMetadata>),
    >,
    mut urls: Query<&mut vmux_core::PageMetadata, With<vmux_core::Url>>,
) {
    for tab_meta in chrome_q.iter() {
        if tab_meta.url.is_empty() {
            continue;
        }
        for mut url_meta in urls.iter_mut() {
            if url_meta.url == tab_meta.url {
                if !tab_meta.title.is_empty() {
                    url_meta.title.clone_from(&tab_meta.title);
                }
                if !tab_meta.favicon_url.is_empty() {
                    url_meta.favicon_url.clone_from(&tab_meta.favicon_url);
                }
                if tab_meta.bg_color.is_some() {
                    url_meta.bg_color.clone_from(&tab_meta.bg_color);
                }
                break;
            }
        }
    }
}

pub fn apply_chrome_state_from_cef(
    chrome_rx: Res<WebviewChromeStateReceiver>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata>,
) {
    while let Ok(ev) = chrome_rx.0.try_recv() {
        let Ok(mut meta) = browser_meta.get_mut(ev.webview) else {
            continue;
        };
        apply_chrome_state_to_meta(&mut meta, ev);
    }
}

pub(crate) fn apply_chrome_state_to_meta(
    meta: &mut vmux_core::PageMetadata,
    ev: bevy_cef_core::prelude::WebviewChromeStateEvent,
) {
    let on_native_view = meta.url.starts_with("vmux://");
    let navigating_away = ev.url.as_deref().is_some_and(|u| !u.starts_with("vmux://"));
    if on_native_view && !navigating_away {
        return;
    }
    if let Some(url) = ev.url {
        meta.url = url;
        meta.favicon_url.clear();
    }
    if let Some(title) = ev.title {
        meta.title = title;
    }
    if let Some(favicon) = ev.favicon_url {
        meta.favicon_url = favicon;
    }
}

impl Browser {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        url: &str,
    ) -> impl Bundle {
        (
            Self,
            vmux_core::PageMetadata {
                title: url.to_string(),
                url: url.to_string(),
                favicon_url: String::new(),
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        )
    }

    pub fn new_with_title(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        url: &str,
        title: &str,
    ) -> impl Bundle {
        (
            Self,
            vmux_core::PageMetadata {
                title: title.to_string(),
                url: url.to_string(),
                favicon_url: String::new(),
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        )
    }
}

pub fn layout_cef_bundle(
    host_window: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> impl Bundle {
    (
        LayoutCef,
        Browser,
        HostWindow(host_window),
        WebviewTransparent,
        bevy_cef::prelude::CefIgnorePinchZoom,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        ZIndex(2),
        WebviewSource::new(LAYOUT_PAGE_URL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
        WebviewSize(Vec2::new(1280.0, 720.0)),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Inherited,
        Pickable {
            should_block_lower: false,
            is_hoverable: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_cef(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) {
        let host = commands.spawn_empty().id();
        commands.spawn(layout_cef_bundle(host, &mut meshes, &mut webview_mt));
    }

    #[test]
    fn layout_cef_does_not_block_pointer_events_below_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Startup, build_test_cef);
        app.update();

        let pickable = app
            .world_mut()
            .query_filtered::<&Pickable, With<LayoutCef>>()
            .single(app.world())
            .expect("layout chrome pickable");

        assert_eq!(
            pickable,
            &Pickable {
                should_block_lower: false,
                is_hoverable: true,
            }
        );
    }
}

#[cfg(test)]
mod apply_chrome_state_tests {
    use super::*;
    use bevy_cef_core::prelude::WebviewChromeStateEvent;
    use vmux_core::PageMetadata;

    fn vmux_meta() -> PageMetadata {
        PageMetadata {
            url: "vmux://history/".into(),
            title: "History".into(),
            favicon_url: String::new(),
            bg_color: None,
        }
    }

    fn external_meta() -> PageMetadata {
        PageMetadata {
            url: "https://example.com".into(),
            title: "old".into(),
            favicon_url: String::new(),
            bg_color: None,
        }
    }

    fn ev(
        title: Option<&str>,
        favicon: Option<&str>,
        url: Option<&str>,
    ) -> WebviewChromeStateEvent {
        WebviewChromeStateEvent {
            webview: Entity::PLACEHOLDER,
            url: url.map(str::to_string),
            title: title.map(str::to_string),
            favicon_url: favicon.map(str::to_string),
        }
    }

    #[test]
    fn vmux_url_preserves_title_against_cef_update() {
        let mut meta = vmux_meta();
        apply_chrome_state_to_meta(&mut meta, ev(Some("vmux history POC"), None, None));
        assert_eq!(meta.title, "History");
    }

    #[test]
    fn vmux_url_preserves_favicon_against_cef_update() {
        let mut meta = vmux_meta();
        apply_chrome_state_to_meta(&mut meta, ev(None, Some("https://x/fav.ico"), None));
        assert_eq!(meta.favicon_url, "");
    }

    #[test]
    fn vmux_url_preserves_url_when_cef_reports_same_vmux_url() {
        let mut meta = vmux_meta();
        apply_chrome_state_to_meta(&mut meta, ev(None, None, Some("vmux://history/")));
        assert_eq!(meta.url, "vmux://history/");
        assert_eq!(meta.title, "History");
    }

    #[test]
    fn vmux_url_updates_when_cef_navigates_to_external_url() {
        let mut meta = vmux_meta();
        apply_chrome_state_to_meta(&mut meta, ev(None, None, Some("https://anthropic.com")));
        assert_eq!(meta.url, "https://anthropic.com");
    }

    #[test]
    fn after_navigation_away_subsequent_title_updates_apply() {
        let mut meta = vmux_meta();
        apply_chrome_state_to_meta(&mut meta, ev(None, None, Some("https://anthropic.com")));
        apply_chrome_state_to_meta(&mut meta, ev(Some("Frontier AI"), None, None));
        assert_eq!(meta.title, "Frontier AI");
    }

    #[test]
    fn external_url_accepts_title_update() {
        let mut meta = external_meta();
        apply_chrome_state_to_meta(&mut meta, ev(Some("New Title"), None, None));
        assert_eq!(meta.title, "New Title");
    }

    #[test]
    fn external_url_accepts_favicon_update() {
        let mut meta = external_meta();
        apply_chrome_state_to_meta(&mut meta, ev(None, Some("https://x/fav.ico"), None));
        assert_eq!(meta.favicon_url, "https://x/fav.ico");
    }

    #[test]
    fn external_url_url_change_clears_favicon() {
        let mut meta = PageMetadata {
            url: "https://example.com".into(),
            title: "Old".into(),
            favicon_url: "https://example.com/fav.ico".into(),
            bg_color: None,
        };
        apply_chrome_state_to_meta(&mut meta, ev(None, None, Some("https://other.com")));
        assert_eq!(meta.url, "https://other.com");
        assert_eq!(meta.favicon_url, "");
    }
}

#[cfg(test)]
mod url_mirror_tests {
    use super::*;
    use vmux_core::{CorePlugin, CreatedAt, LastVisitedAt, PageMetadata, Url, VisitCount};

    #[test]
    fn updates_matching_url_meta() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CorePlugin)
            .add_systems(Update, mirror_metadata_to_url);

        app.world_mut().spawn((
            Url,
            PageMetadata {
                url: "https://example.com".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(0),
            CreatedAt(0),
        ));

        app.world_mut().spawn(PageMetadata {
            url: "https://example.com".into(),
            title: "Example".into(),
            favicon_url: "https://example.com/fav.ico".into(),
            bg_color: None,
        });

        app.update();

        let url_meta = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Url>>()
            .iter(app.world())
            .next()
            .unwrap();
        assert_eq!(url_meta.title, "Example");
        assert_eq!(url_meta.favicon_url, "https://example.com/fav.ico");
    }

    #[test]
    fn skips_empty_tab_url() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CorePlugin)
            .add_systems(Update, mirror_metadata_to_url);

        app.world_mut().spawn((
            Url,
            PageMetadata {
                url: "https://example.com".into(),
                title: "old".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(0),
            CreatedAt(0),
        ));

        app.world_mut().spawn(PageMetadata {
            url: "".into(),
            title: "new".into(),
            ..default()
        });

        app.update();

        let url_meta = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Url>>()
            .iter(app.world())
            .next()
            .unwrap();
        assert_eq!(url_meta.title, "old");
    }
}
