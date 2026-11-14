use bevy_app::{App, AppExit};

mod command;
#[cfg(feature = "runtime")]
mod plugin;

fn main() -> AppExit {
    #[cfg(feature = "dx")]
    if vmux_dx::Plugin::selected(std::env::args_os().skip(1)) {
        let mut app = App::new();
        app.add_plugins((vmux_dx::Plugin, command::CliRuntimePlugin));
        return app.run();
    }
    run()
}

#[cfg(feature = "runtime")]
#[tokio::main]
async fn run() -> AppExit {
    let mut app = App::new();
    app.add_plugins(plugin::CliPlugin);
    app.run()
}

#[cfg(not(feature = "runtime"))]
fn run() -> AppExit {
    let mut app = App::new();
    app.add_plugins((vmux_dx::Plugin, command::CliRuntimePlugin));
    app.run()
}
