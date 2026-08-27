//! The phone, as one plugin. This is the whole of `main.rs`.

use bevy_app::App;
use vmux_mobile::MobilePlugin;

fn main() {
    App::new().add_plugins(MobilePlugin::default()).run();
}
