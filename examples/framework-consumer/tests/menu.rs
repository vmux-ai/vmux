use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Waker;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::menu::{
    About, Entry, EventSender, MenuBar, MenuSet, MenuWake, Predefined, Selection, Separator,
    Submenu, Transient,
};

struct WakeCounter(AtomicUsize);

impl WakeCounter {
    fn new() -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(0)))
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

impl std::task::Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Component)]
struct OpenDocumentAction;

#[derive(Resource, Default)]
struct OpenedDocuments(usize);

struct CustomAppMenuPlugin;

impl Plugin for CustomAppMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VmuxPlugin::builder().menu(true).build())
            .init_resource::<OpenedDocuments>()
            .add_systems(Update, handle_open_document.in_set(MenuSet::Dispatch));
    }
}

fn handle_open_document(
    mut selections: MessageReader<Selection>,
    actions: Query<(), With<OpenDocumentAction>>,
    mut opened: ResMut<OpenedDocuments>,
) {
    for selection in selections.read() {
        if actions.contains(selection.target()) {
            opened.0 += 1;
        }
    }
}

#[test]
fn custom_app_declares_menu_bar_receives_selection_and_cleans_up_transient_entries() {
    let mut app = App::new();
    let wake = WakeCounter::new();
    app.add_plugins(CustomAppMenuPlugin)
        .insert_resource(MenuWake(Waker::from(Arc::clone(&wake))));

    let bar = app.world_mut().spawn(MenuBar).id();
    let app_submenu = app
        .world_mut()
        .spawn((Submenu::new("Studio"), ChildOf(bar)))
        .id();
    app.world_mut().spawn((
        Predefined::About,
        About {
            name: Some("Studio".into()),
            version: Some("1.0.0".into()),
            ..Default::default()
        },
        ChildOf(app_submenu),
    ));
    app.world_mut().spawn((Separator, ChildOf(app_submenu)));
    let file_submenu = app
        .world_mut()
        .spawn((Submenu::new("File"), ChildOf(bar)))
        .id();
    let open = app
        .world_mut()
        .spawn((
            Entry::new("Open...", true)
                .with_id("studio_open")
                .with_accelerator("super+o"),
            OpenDocumentAction,
            ChildOf(file_submenu),
        ))
        .id();
    let transient = app
        .world_mut()
        .spawn((
            Entry::new("Quick Action", true).with_id("studio_quick"),
            Transient,
        ))
        .id();

    app.update();

    let sender = app.world().resource::<EventSender>().clone();
    sender.send("studio_open");
    sender.send("studio_quick");
    assert_eq!(wake.count(), 2);

    app.update();

    assert_eq!(app.world().resource::<OpenedDocuments>().0, 1);
    assert!(app.world().get_entity(open).is_ok());
    assert!(app.world().get_entity(transient).is_err());
}
