//! A sheet is a level like any other, marked so it presents rather than pushes. They
//! stack, so a sheet can raise a sheet, and dismissing takes the top one only.

use bevy_app::App;
use vmux_mobile::nav::{Dismiss, Nav, NavPlugin, Open, Present, Report, Screen};

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
        tabs: vec![("root".to_string(), Page("Settings"))],
        focused: None,
    });
    app.update();

    app.world_mut().write_message(Open(Page("Account")));
    app.update();
    show(&mut app, "pushed, no sheet yet");

    app.world_mut().write_message(Present(Page("Sign out?")));
    app.update();
    show(&mut app, "a sheet over it");

    app.world_mut()
        .write_message(Present(Page("Are you sure?")));
    app.update();
    show(&mut app, "a sheet over the sheet");

    app.world_mut().write_message(Dismiss);
    app.update();
    show(&mut app, "dismissed the top one only");
}

fn show(app: &mut App, note: &str) {
    let view = app.world_mut().run_system_cached(read).unwrap_or_default();
    let showing = match view.current {
        Some(page) => page.title(),
        None => "nothing".to_string(),
    };
    let kind = if view.sheet { "sheet" } else { "pushed" };
    println!("depth {}  {kind:<7} {showing:<14} — {note}", view.depth);
}

fn read(world: &mut bevy_ecs::world::World) -> vmux_mobile::nav::View<Page> {
    Nav::view::<Page>(world)
}
