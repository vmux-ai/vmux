use bevy_app::{App, AppExit};

mod command;
mod plugin;

#[tokio::main]
async fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins(plugin::CliPlugin);
    app.run()
}
