//! A pushed level is a child of the tab, so depth is a walk down the tree and closing
//! the tab takes the stack with it. `Dropped` is how a back-swipe reports itself, since
//! UIKit runs the gesture and only says so afterwards.

use bevy_app::App;
use vmux_mobile::nav::{Back, Dropped, Nav, NavPlugin, Open, Report, Screen};

#[derive(Clone, PartialEq)]
struct Page(&'static str);

impl Screen for Page {
    fn title(&self) -> String {
        self.0.to_string()
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(NavPlugin::<Page>::default());

    app.world_mut().write_message(Report {
        tabs: vec![("root".to_string(), Page("Library"))],
        focused: None,
    });
    app.update();
    show(&mut app, "at the root");

    app.world_mut().write_message(Open(Page("Shelf")));
    app.update();
    app.world_mut().write_message(Open(Page("Book")));
    app.update();
    show(&mut app, "pushed twice");

    app.world_mut().write_message(Back);
    app.update();
    show(&mut app, "back once");

    app.world_mut().write_message(Dropped(1));
    app.update();
    show(&mut app, "and a back-swipe UIKit already ran");

    app.world_mut().write_message(Back);
    app.update();
    show(&mut app, "back at the root, which does not pop away");
}

fn show(app: &mut App, note: &str) {
    let view = app.world_mut().run_system_cached(read).unwrap_or_default();
    let showing = match view.current {
        Some(page) => page.title(),
        None => "nothing".to_string(),
    };
    println!("depth {}  showing {showing:<10} — {note}", view.depth);
}

fn read(world: &mut bevy_ecs::world::World) -> vmux_mobile::nav::View<Page> {
    Nav::view::<Page>(world)
}
