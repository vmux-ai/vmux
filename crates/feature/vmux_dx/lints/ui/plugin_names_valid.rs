use bevy_app::{App, Plugin};

pub struct HttpPlugin;

impl Plugin for HttpPlugin {
    fn build(&self, _: &mut App) {}
}

type AliasPlugin = HttpPlugin;

pub use HttpPlugin as RootPlugin;

pub use bevy_app::MainSchedulePlugin as SchedulePlugin;

mod foreign {
    pub trait Plugin {}

    pub struct Value;

    impl Plugin for Value {}
}

fn main() {
    let alias: AliasPlugin = HttpPlugin;
    App::new().add_plugins((alias, RootPlugin, SchedulePlugin));
    let _: &dyn foreign::Plugin = &foreign::Value;
}
