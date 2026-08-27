//! Tabs are entities. Something reports what is open, one is selected, and the phone
//! can add its own that no report will ever mention.

use bevy_app::App;
use vmux_mobile::nav::{Nav, NavPlugin, OpenBlank, Report, Screen, Select};

#[derive(Clone, PartialEq)]
enum Page {
    Inbox,
    Note(&'static str),
    Untitled,
}

impl Screen for Page {
    fn title(&self) -> String {
        match self {
            Self::Inbox => "Inbox".to_string(),
            Self::Note(name) => (*name).to_string(),
            Self::Untitled => "Untitled".to_string(),
        }
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(NavPlugin::<Page>::default());

    app.world_mut().write_message(Report {
        tabs: vec![
            ("inbox".to_string(), Page::Inbox),
            ("groceries".to_string(), Page::Note("Groceries")),
        ],
        focused: Some("groceries".to_string()),
    });
    app.update();
    show(&mut app, "reported, focused on groceries");

    app.world_mut().write_message(Select("inbox".to_string()));
    app.update();
    show(&mut app, "selected the inbox");

    app.world_mut().write_message(OpenBlank(Page::Untitled));
    app.update();
    show(&mut app, "opened one of the phone's own");
}

fn show(app: &mut App, note: &str) {
    let view = app.world_mut().run_system_cached(read).unwrap_or_default();
    println!("\n{note}");
    for tab in &view.tabs {
        let mark = if Some(&tab.id) == view.selected.as_ref() {
            ">"
        } else {
            " "
        };
        let origin = if tab.local { "local" } else { "reported" };
        println!("  {mark} {:<12} {:<10} {}", tab.id, origin, tab.name);
    }
}

fn read(world: &mut bevy_ecs::world::World) -> vmux_mobile::nav::View<Page> {
    Nav::view::<Page>(world)
}
