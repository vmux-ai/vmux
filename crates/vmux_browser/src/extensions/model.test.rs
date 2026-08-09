use super::*;
use bevy::ecs::message::Messages;
use bevy::window::{PrimaryWindow, WindowPosition};
use vmux_core::{Order, PageMetadata};
use vmux_history::LastActivatedAt;
use vmux_layout::pane::Pane;
use vmux_layout::space::Space;
use vmux_layout::stack::{FocusedStack, Stack};
use vmux_layout::tab::Tab;

fn spawn_page(world: &mut World, pane: Entity, url: &str, title: &str, activated: i64) -> Entity {
    world
        .spawn((
            Stack::default(),
            PageMetadata {
                title: title.into(),
                url: url.into(),
                ..default()
            },
            LastActivatedAt(activated),
            ChildOf(pane),
        ))
        .id()
}

#[test]
fn projects_ordered_visible_pages_with_stable_ids_and_removals() {
    let mut app = App::new();
    app.init_resource::<ChromeModel>()
        .init_resource::<ChromeStableIds>()
        .insert_resource(FocusedStack::default())
        .add_message::<ChromeModelEvent>()
        .add_systems(Update, crate::extensions::project::rebuild_chrome_model);
    app.world_mut().spawn((
        Window {
            resolution: (1200, 800).into(),
            position: WindowPosition::At(IVec2::new(40, 60)),
            focused: true,
            ..default()
        },
        PrimaryWindow,
    ));
    let space_two = app.world_mut().spawn((Space, Order(2))).id();
    let space_one = app.world_mut().spawn((Space, Order(1))).id();
    let tab_one = app
        .world_mut()
        .spawn((Tab::default(), Order(1), ChildOf(space_one)))
        .id();
    let pane_one = app.world_mut().spawn((Pane, ChildOf(tab_one))).id();
    let first = spawn_page(app.world_mut(), pane_one, "https://one.example/", "One", 1);
    let terminal = spawn_page(app.world_mut(), pane_one, "vmux://terminal/", "Terminal", 2);
    let tab_two = app
        .world_mut()
        .spawn((Tab::default(), Order(1), ChildOf(space_two)))
        .id();
    let pane_two = app.world_mut().spawn((Pane, ChildOf(tab_two))).id();
    let second = spawn_page(app.world_mut(), pane_two, "https://two.example/", "Two", 5);
    spawn_page(
        app.world_mut(),
        pane_two,
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html",
        "Extension",
        4,
    );
    spawn_page(
        app.world_mut(),
        pane_two,
        "cef://localhost/internal",
        "Internal",
        3,
    );
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(second);

    app.update();

    let model = app.world().resource::<ChromeModel>();
    assert_eq!(model.windows.len(), 1);
    assert_eq!(model.tabs.len(), 3);
    assert_eq!(
        model
            .tabs
            .iter()
            .map(|tab| tab.url.as_str())
            .collect::<Vec<_>>(),
        vec![
            "https://one.example/",
            "https://two.example/",
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html",
        ]
    );
    assert_eq!(model.tabs.iter().filter(|tab| tab.active).count(), 1);
    assert!(!model.tabs.iter().any(|tab| tab.url.starts_with("vmux://")));
    assert!(!model.tabs.iter().any(|tab| tab.url.starts_with("cef://")));
    let first_id = model.tabs[0].id;
    let active_id = model.tabs.iter().find(|tab| tab.active).unwrap().id;

    app.world_mut().resource_mut::<FocusedStack>().stack = Some(terminal);
    app.update();
    assert_eq!(
        app.world()
            .resource::<ChromeModel>()
            .tabs
            .iter()
            .find(|tab| tab.active)
            .unwrap()
            .id,
        active_id
    );

    app.world_mut()
        .get_mut::<PageMetadata>(first)
        .unwrap()
        .title = "Updated".into();
    app.update();
    assert_eq!(app.world().resource::<ChromeModel>().tabs[0].id, first_id);

    let mut cursor = app
        .world()
        .resource::<Messages<ChromeModelEvent>>()
        .get_cursor();
    app.world_mut().entity_mut(first).despawn();
    app.update();
    let messages = app.world().resource::<Messages<ChromeModelEvent>>();
    assert!(cursor.read(messages).any(|event| matches!(
        event,
        ChromeModelEvent::TabRemoved { tab_id, .. } if *tab_id == first_id
    )));
}

#[test]
fn filters_extension_visible_urls() {
    assert!(extension_visible_url("https://example.com/"));
    assert!(extension_visible_url("http://example.com/"));
    assert!(extension_visible_url(
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html"
    ));
    assert!(!extension_visible_url("vmux://terminal/"));
    assert!(!extension_visible_url("cef://localhost/internal"));
}
