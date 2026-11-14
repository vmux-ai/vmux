use bevy_app::{App, AppExit};

#[cfg(feature = "runtime")]
mod browser;
mod command;
#[cfg(feature = "runtime")]
mod plugin;

fn main() -> AppExit {
    #[cfg(feature = "dx")]
    if let Some(code) = vmux_dx::updates::Plugin::automatic() {
        return if code == 0 {
            AppExit::Success
        } else {
            AppExit::Error(std::num::NonZero::new(code).unwrap())
        };
    }
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
