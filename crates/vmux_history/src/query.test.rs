use super::*;
use vmux_core::{CorePlugin, CreatedAt, LastVisitedAt, PageMetadata, Url, VisitCount, VisitedUrl};

#[test]
fn build_entries_no_query_orders_by_visit_created_at_desc() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);

    let url_e = app
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "https://example.com".into(),
                ..default()
            },
            VisitCount(2),
            LastVisitedAt(200),
            CreatedAt(0),
        ))
        .id();

    let url_rows = vec![(
        url_e,
        PageMetadata {
            url: "https://example.com".into(),
            ..default()
        },
        VisitCount(2),
        LastVisitedAt(200),
    )];
    let visit_rows = vec![
        (CreatedAt(100), VisitedUrl(url_e)),
        (CreatedAt(200), VisitedUrl(url_e)),
    ];

    let entries = build_entries(&None, &url_rows, &visit_rows, 1000);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].visit_created_at, 200);
    assert_eq!(entries[1].visit_created_at, 100);
}

#[test]
fn build_entries_with_query_filters_and_ranks() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);

    let e1 = app
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "https://github.com".into(),
                title: "GitHub".into(),
                ..default()
            },
            VisitCount(10),
            LastVisitedAt(1000),
            CreatedAt(0),
        ))
        .id();

    let e2 = app
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "https://example.com".into(),
                title: "Example".into(),
                ..default()
            },
            VisitCount(10),
            LastVisitedAt(1000),
            CreatedAt(0),
        ))
        .id();

    let url_rows = vec![
        (
            e1,
            PageMetadata {
                url: "https://github.com".into(),
                title: "GitHub".into(),
                ..default()
            },
            VisitCount(10),
            LastVisitedAt(1000),
        ),
        (
            e2,
            PageMetadata {
                url: "https://example.com".into(),
                title: "Example".into(),
                ..default()
            },
            VisitCount(10),
            LastVisitedAt(1000),
        ),
    ];
    let visit_rows = vec![];

    let entries = build_entries(&Some("git".into()), &url_rows, &visit_rows, 1000);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://github.com");
}

#[test]
fn build_entries_pagination() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);

    let url_e = app
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "u".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(0),
            CreatedAt(0),
        ))
        .id();

    let url_rows = vec![(
        url_e,
        PageMetadata {
            url: "u".into(),
            ..default()
        },
        VisitCount(1),
        LastVisitedAt(0),
    )];
    let visit_rows: Vec<_> = (0..5)
        .map(|i| (CreatedAt(i * 100), VisitedUrl(url_e)))
        .collect();

    let all = build_entries(&None, &url_rows, &visit_rows, 1000);
    assert_eq!(all.len(), 5);

    let page: Vec<_> = all.into_iter().skip(2).take(2).collect();
    assert_eq!(page.len(), 2);
}
