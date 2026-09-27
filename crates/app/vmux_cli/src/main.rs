use bevy_app::{App, AppExit};

mod command;

#[tokio::main]
async fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins((vmux_app::VmuxCliPlugin, command::CliRuntimePlugin));
    app.run()
}
