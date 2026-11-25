use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Waker;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::menu::{
    About, Entry, EventSender, MenuAction, MenuActionPlugin, MenuSet, MenuWake, Selected,
    Transient,
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

#[vmux_app::menu::menu(standard_edit = "Edit")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StudioMenu {
    #[submenu("Studio")]
    #[predefined(about)]
    #[separator]
    #[item("Quit Studio", id = "studio_quit", accelerator = "super+q")]
    Quit,
    #[submenu("File")]
    #[item("Open...", id = "studio_open", accelerator = "super+o")]
    Open,
}

#[derive(Resource, Default)]
struct ObservedActions(Vec<StudioMenu>);

struct CustomAppMenuPlugin;

impl Plugin for CustomAppMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxPlugin::builder().menu(true).build(),
            MenuActionPlugin::<StudioMenu>::default().with_about(About {
                name: Some("Studio".into()),
                version: Some("1.0.0".into()),
                ..Default::default()
            }),
        ))
        .init_resource::<ObservedActions>()
        .add_systems(Update, record_menu_actions.in_set(MenuSet::Dispatch));
    }
}

fn record_menu_actions(
    mut actions: MessageReader<Selected<StudioMenu>>,
    mut observed: ResMut<ObservedActions>,
) {
    for Selected(action) in actions.read() {
        observed.0.push(*action);
    }
}

#[test]
fn custom_app_declares_menu_via_macro_receives_typed_actions_and_cleans_up_transient_entries() {
    let mut app = App::new();
    let wake = WakeCounter::new();
    app.add_plugins(CustomAppMenuPlugin)
        .insert_resource(MenuWake(Waker::from(Arc::clone(&wake))));

    let transient = app
        .world_mut()
        .spawn((
            Entry::new("Quick Action", true).with_id("studio_quick"),
            Transient,
        ))
        .id();

    app.update();

    let sender = app.world().resource::<EventSender>().clone();
    sender.send(StudioMenu::Open.id());
    sender.send("studio_quick");
    sender.send(StudioMenu::Quit.id());
    assert_eq!(wake.count(), 3);

    app.update();

    assert_eq!(
        app.world().resource::<ObservedActions>().0,
        vec![StudioMenu::Open, StudioMenu::Quit]
    );
    assert!(app.world().get_entity(transient).is_err());
}
