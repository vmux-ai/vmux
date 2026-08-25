use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_flex::prelude::*;

pub struct LayoutCefPlugin;

impl Plugin for LayoutCefPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::LAYOUT_PAGE_MANIFEST);

        app.world_mut().spawn(crate::ERROR_PAGE_MANIFEST);
    }
}

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

pub fn mirror_metadata_to_url(
    cef_q: Query<
        &vmux_core::PageMetadata,
        (Without<vmux_core::Url>, Changed<vmux_core::PageMetadata>),
    >,
    mut urls: Query<&mut vmux_core::PageMetadata, With<vmux_core::Url>>,
) {
    for tab_meta in cef_q.iter() {
        if tab_meta.url.is_empty() {
            continue;
        }
        for mut url_meta in urls.iter_mut() {
            if url_meta.url == tab_meta.url {
                if !tab_meta.title.is_empty() {
                    url_meta.title.clone_from(&tab_meta.title);
                }
                if !tab_meta.icon.is_none() {
                    url_meta.icon.clone_from(&tab_meta.icon);
                }
                if tab_meta.bg_color.is_some() {
                    url_meta.bg_color.clone_from(&tab_meta.bg_color);
                }
                break;
            }
        }
    }
}

pub fn apply_cef_state_from_webview(
    cef_rx: Res<WebviewCefStateReceiver>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata>,
) {
    while let Ok(ev) = cef_rx.0.try_recv() {
        let Ok(mut meta) = browser_meta.get_mut(ev.webview) else {
            continue;
        };
        apply_cef_state_to_meta(&mut meta, ev);
    }
}

pub(crate) fn apply_cef_state_to_meta(
    meta: &mut vmux_core::PageMetadata,
    ev: bevy_cef_core::prelude::WebviewCefStateEvent,
) {
    let on_native_view = meta.url.starts_with("vmux://");
    let accepts_dynamic_title = meta.url.starts_with("vmux://agent/");
    let navigating_away = ev.url.as_deref().is_some_and(|u| !u.starts_with("vmux://"));
    if on_native_view && !navigating_away {
        if accepts_dynamic_title && let Some(title) = ev.title {
            meta.title = title;
        }
        return;
    }
    if let Some(url) = ev.url {
        meta.url = url;
        meta.icon = vmux_core::PageIcon::None;
    }
    if let Some(title) = ev.title {
        meta.title = title;
    }
    if let Some(favicon) = ev.favicon_url {
        meta.icon = vmux_core::PageIcon::favicon(favicon);
    }
}

impl Browser {
    pub fn new(url: &str) -> impl Bundle {
        (
            Self,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            vmux_core::PageMetadata {
                title: url.to_string(),
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        )
    }

    pub fn new_with_title(url: &str, title: &str) -> impl Bundle {
        (
            Self,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            vmux_core::PageMetadata {
                title: title.to_string(),
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        )
    }

    pub fn native_page(url: &str, title: &str) -> impl Bundle {
        (
            Self,
            WebviewWindowed,
            vmux_core::host::page::HostsPage,
            vmux_core::PageMetadata {
                title: title.to_string(),
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        )
    }
}

pub fn layout_cef_bundle(host_window: Entity) -> impl Bundle {
    (
        LayoutCef,
        vmux_core::host::page::HostsPage,
        vmux_core::launcher::RendersLauncherPanel,
        HostWindow(host_window),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        Transform::default(),
        Visibility::Visible,
    )
}

#[cfg(test)]
mod apply_cef_state_tests {
    use super::*;
    use bevy_cef_core::prelude::WebviewCefStateEvent;
    use vmux_core::PageMetadata;

    fn vmux_meta() -> PageMetadata {
        PageMetadata {
            url: "vmux://history/".into(),
            title: "History".into(),
            icon: vmux_core::PageIcon::None,
            bg_color: None,
        }
    }

    fn external_meta() -> PageMetadata {
        PageMetadata {
            url: "https://example.com".into(),
            title: "old".into(),
            icon: vmux_core::PageIcon::None,
            bg_color: None,
        }
    }

    fn ev(title: Option<&str>, favicon: Option<&str>, url: Option<&str>) -> WebviewCefStateEvent {
        WebviewCefStateEvent {
            webview: Entity::PLACEHOLDER,
            url: url.map(str::to_string),
            title: title.map(str::to_string),
            favicon_url: favicon.map(str::to_string),
        }
    }

    #[test]
    fn vmux_url_preserves_title_against_cef_update() {
        let mut meta = vmux_meta();
        apply_cef_state_to_meta(&mut meta, ev(Some("vmux history POC"), None, None));
        assert_eq!(meta.title, "History");
    }

    #[test]
    fn vmux_agent_url_accepts_dynamic_title_only() {
        let mut meta = PageMetadata {
            url: "vmux://agent/codex".into(),
            title: "Codex".into(),
            icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Sparkles),
            bg_color: None,
        };
        apply_cef_state_to_meta(
            &mut meta,
            ev(
                Some("● Codex"),
                Some("https://example.com/favicon.ico"),
                None,
            ),
        );
        assert_eq!(meta.title, "● Codex");
        assert_eq!(meta.url, "vmux://agent/codex");
        assert_eq!(
            meta.icon,
            vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Sparkles)
        );
    }

    #[test]
    fn vmux_url_preserves_favicon_against_cef_update() {
        let mut meta = vmux_meta();
        apply_cef_state_to_meta(&mut meta, ev(None, Some("https://x/fav.ico"), None));
        assert_eq!(meta.icon, vmux_core::PageIcon::None);
    }

    #[test]
    fn vmux_url_preserves_url_when_cef_reports_same_vmux_url() {
        let mut meta = vmux_meta();
        apply_cef_state_to_meta(&mut meta, ev(None, None, Some("vmux://history/")));
        assert_eq!(meta.url, "vmux://history/");
        assert_eq!(meta.title, "History");
    }

    #[test]
    fn vmux_url_updates_when_cef_navigates_to_external_url() {
        let mut meta = vmux_meta();
        apply_cef_state_to_meta(&mut meta, ev(None, None, Some("https://anthropic.com")));
        assert_eq!(meta.url, "https://anthropic.com");
    }

    #[test]
    fn after_navigation_away_subsequent_title_updates_apply() {
        let mut meta = vmux_meta();
        apply_cef_state_to_meta(&mut meta, ev(None, None, Some("https://anthropic.com")));
        apply_cef_state_to_meta(&mut meta, ev(Some("Frontier AI"), None, None));
        assert_eq!(meta.title, "Frontier AI");
    }

    #[test]
    fn external_url_accepts_title_update() {
        let mut meta = external_meta();
        apply_cef_state_to_meta(&mut meta, ev(Some("New Title"), None, None));
        assert_eq!(meta.title, "New Title");
    }

    #[test]
    fn external_url_accepts_favicon_update() {
        let mut meta = external_meta();
        apply_cef_state_to_meta(&mut meta, ev(None, Some("https://x/fav.ico"), None));
        assert_eq!(
            meta.icon,
            vmux_core::PageIcon::Favicon("https://x/fav.ico".into())
        );
    }

    #[test]
    fn external_url_url_change_clears_favicon() {
        let mut meta = PageMetadata {
            url: "https://example.com".into(),
            title: "Old".into(),
            icon: vmux_core::PageIcon::Favicon("https://example.com/fav.ico".into()),
            bg_color: None,
        };
        apply_cef_state_to_meta(&mut meta, ev(None, None, Some("https://other.com")));
        assert_eq!(meta.url, "https://other.com");
        assert_eq!(meta.icon, vmux_core::PageIcon::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_page(mut commands: Commands) {
        commands.spawn(Browser::new("https://example.com"));
    }

    #[test]
    fn page_cef_uses_opaque_dark_initial_background() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Startup, build_test_page);
        app.update();

        let page = app
            .world_mut()
            .query_filtered::<Entity, (With<Browser>, Without<LayoutCef>)>()
            .single(app.world())
            .expect("page CEF");

        assert!(
            app.world()
                .get::<WebviewOpaqueWindowedBackground>(page)
                .is_some()
        );
    }

    #[test]
    fn page_cef_allows_native_first_responder() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Startup, build_test_page);
        app.update();

        let page = app
            .world_mut()
            .query_filtered::<Entity, (With<Browser>, Without<LayoutCef>)>()
            .single(app.world())
            .expect("page CEF");

        assert!(
            app.world()
                .get::<WebviewWindowedNativeFocus>(page)
                .is_some(),
            "windowed web pages must allow native first-responder so they are typeable without a click"
        );
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
            icon: vmux_core::PageIcon::Favicon("https://example.com/fav.ico".into()),
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
        assert_eq!(
            url_meta.icon,
            vmux_core::PageIcon::Favicon("https://example.com/fav.ico".into())
        );
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
