use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::menu::{About, EventSender, MenuAction, MenuActionPlugin, MenuSet, Selected};

#[vmux_app::menu::menu(standard_edit = "Edit")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppMenu {
    #[submenu("Studio")]
    #[predefined(about)]
    #[separator]
    #[item("Quit Studio", id = "studio_quit", accelerator = "super+q")]
    Quit,
    #[submenu("File")]
    #[item("Open...", id = "studio_open", accelerator = "super+o")]
    Open,
}

struct ExampleMenuPlugin;

impl Plugin for ExampleMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxPlugin::builder().menu(true).build(),
            MenuActionPlugin::<AppMenu>::default().with_about(About {
                name: Some("Studio".into()),
                version: Some("1.0.0".into()),
                ..Default::default()
            }),
        ))
        .add_systems(Update, handle_menu.in_set(MenuSet::Dispatch));
    }
}

fn handle_menu(mut actions: MessageReader<Selected<AppMenu>>) {
    for Selected(action) in actions.read() {
        println!("menu selected: {action:?}");
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(ExampleMenuPlugin);
    app.update();

    let sender = app.world().resource::<EventSender>().clone();
    sender.send(AppMenu::Open.id());
    sender.send(AppMenu::Quit.id());
    app.update();
}
