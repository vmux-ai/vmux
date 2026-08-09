use super::*;
use vmux_core::{PageOpenHandled, PageOpenId, PageOpenTask};
use vmux_layout::warm_page::WarmPagePlugin;

#[test]
fn settings_page_open_spawns_marker_and_handles() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_plugins(WarmPagePlugin::<Settings>::default());
    let stack = app.world_mut().spawn_empty().id();
    let claimed = app
        .world_mut()
        .spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: SETTINGS_PAGE_URL.to_string(),
            request_id: None,
        })
        .id();
    let decoy = app
        .world_mut()
        .spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://history/".to_string(),
            request_id: None,
        })
        .id();
    app.update();
    assert!(app.world().get::<PageOpenHandled>(claimed).is_some());
    assert!(app.world().get::<PageOpenHandled>(decoy).is_none());
    let mut q = app.world_mut().query_filtered::<(), With<Settings>>();
    assert_eq!(q.iter(app.world()).count(), 1);
}

#[test]
fn settings_page_open_dedupes_per_stack() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_plugins(WarmPagePlugin::<Settings>::default());
    let stack = app.world_mut().spawn_empty().id();
    for _ in 0..2 {
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: SETTINGS_PAGE_URL.to_string(),
            request_id: None,
        });
    }
    app.update();
    let mut q = app.world_mut().query_filtered::<(), With<Settings>>();
    assert_eq!(q.iter(app.world()).count(), 1);
}
