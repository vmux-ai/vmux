use super::*;
use vmux_core::{PageOpenId, PageOpenTask};

use crate::cef::Browser;

#[derive(Component)]
struct TestPage;

impl WarmPage for TestPage {
    const HOST: &'static str = "test";
    const URL: &'static str = "vmux://test/";
    const TITLE: &'static str = "Test";
    const POOL_SIZE: usize = 1;

    fn spawn(
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> Entity {
        commands
            .spawn((
                TestPage,
                Browser::new_with_title(meshes, webview_mt, Self::URL, Self::TITLE),
            ))
            .id()
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_warm_page_open::<TestPage>);
    app
}

fn task(stack: Entity) -> PageOpenTask {
    PageOpenTask {
        id: PageOpenId::new(),
        stack,
        url: TestPage::URL.to_string(),
        request_id: None,
    }
}

#[test]
fn ready_spare_is_reparented() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    let spare = app
        .world_mut()
        .spawn((TestPage, WarmPageSpare { url: TestPage::URL }, PageReady {}))
        .id();
    let task = app.world_mut().spawn(task(stack)).id();

    app.update();

    assert_eq!(
        app.world()
            .get::<ChildOf>(spare)
            .map(|child| child.parent()),
        Some(stack)
    );
    assert!(app.world().get::<WarmPageSpare>(spare).is_none());
    assert!(app.world().get::<CefKeyboardTarget>(spare).is_some());
    assert!(app.world().get::<PageOpenHandled>(task).is_some());
}

#[test]
fn unready_spare_falls_back_to_cold_page() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    let spare = app
        .world_mut()
        .spawn((TestPage, WarmPageSpare { url: TestPage::URL }))
        .id();
    app.world_mut().spawn(task(stack));

    app.update();

    assert!(app.world().get::<WarmPageSpare>(spare).is_some());
    let pages = app
        .world_mut()
        .query_filtered::<Entity, (With<TestPage>, With<ChildOf>)>()
        .iter(app.world())
        .count();
    assert_eq!(pages, 1);
}

#[test]
fn pool_waits_for_layout_then_fills() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .init_resource::<WarmPageSpawnBudget>()
        .add_systems(Update, maintain_warm_page_pool::<TestPage>);
    app.world_mut().spawn(VmuxWindow);

    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<(), With<WarmPageSpare>>()
            .iter(app.world())
            .count(),
        0
    );

    app.world_mut().spawn((LayoutCef, PageReady {}));
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<(), With<WarmPageSpare>>()
            .iter(app.world())
            .count(),
        TestPage::POOL_SIZE
    );
}

#[test]
fn registered_page_claims_ready_spare() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_registered_page_open);
    app.world_mut().spawn(PrewarmPage {
        host: "history",
        url: "vmux://history/",
        title: "History",
        pool_size: 1,
    });
    let stack = app.world_mut().spawn_empty().id();
    let spare = app
        .world_mut()
        .spawn((
            WarmPageSpare {
                url: "vmux://history/",
            },
            PageReady {},
        ))
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://history/".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert_eq!(
        app.world()
            .get::<ChildOf>(spare)
            .map(|child| child.parent()),
        Some(stack)
    );
    assert!(app.world().get::<WarmPageSpare>(spare).is_none());
    assert!(app.world().get::<PageOpenHandled>(task).is_some());
}

#[test]
fn registered_page_without_pool_opens_cold() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_registered_page_open);
    app.world_mut().spawn(PrewarmPage {
        host: "history",
        url: "vmux://history/",
        title: "History",
        pool_size: 0,
    });
    let stack = app.world_mut().spawn_empty().id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://history/".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let pages = app
        .world_mut()
        .query_filtered::<&ChildOf, With<Browser>>()
        .iter(app.world())
        .filter(|child| child.parent() == stack)
        .count();
    assert_eq!(pages, 1);
}

#[test]
fn registered_pools_fill_for_every_page() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .init_resource::<WarmPageSpawnBudget>()
        .add_systems(Update, maintain_registered_page_pools);
    app.world_mut().spawn(VmuxWindow);
    app.world_mut().spawn((LayoutCef, PageReady {}));
    for (host, title) in [("history", "History"), ("lsp", "Language Servers")] {
        app.world_mut().spawn(PrewarmPage {
            host,
            url: if host == "history" {
                "vmux://history/"
            } else {
                "vmux://lsp/"
            },
            title,
            pool_size: 1,
        });
    }

    app.update();
    app.world_mut().resource_mut::<WarmPageSpawnBudget>().0 = 1;
    app.update();

    assert_eq!(
        app.world_mut()
            .query_filtered::<(), With<WarmPageSpare>>()
            .iter(app.world())
            .count(),
        2
    );
}
