use bevy_app::{App, Plugin as Contract};

struct Http;

impl Contract for Http {
    fn build(&self, _: &mut App) {}
}

type HttpPlugin = Http;

pub struct GoodPlugin;

impl Contract for GoodPlugin {
    fn build(&self, _: &mut App) {}
}

type Alias = GoodPlugin;

pub use GoodPlugin as Plugin;

pub use bevy_app::MainSchedulePlugin as Engine;

mod exported {
    pub use crate::GoodPlugin as Root;
}

mod consumer {
    use super::exported::*;
    use bevy_app::App;

    pub fn install(app: &mut App) {
        app.add_plugins(Root);
    }
}

fn main() {
    let http: HttpPlugin = Http;
    let alias: Alias = GoodPlugin;
    let mut app = App::new();
    app.add_plugins((http, alias, Plugin, Engine));
    consumer::install(&mut app);
}
