use bevy_app::App;
use vmux_mobile::MobilePlugin;

fn main() {
    App::new().add_plugins(MobilePlugin::default()).run();
}
