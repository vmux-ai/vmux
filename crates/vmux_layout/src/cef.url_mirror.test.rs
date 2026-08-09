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
