use bevy_app::{App, AppExit};

mod command;
#[cfg(feature = "runtime")]
mod plugin;

fn main() -> AppExit {
    #[cfg(feature = "dx")]
    if let Some(code) = vmux_dx::updates::UpdatesPlugin::automatic() {
        return if code == 0 {
            AppExit::Success
        } else {
            AppExit::Error(std::num::NonZero::new(code).unwrap())
        };
    }
    #[cfg(feature = "dx")]
    {
        let mut args = std::env::args_os().skip(1).peekable();
        let tui_runtime = cfg!(feature = "runtime") && args.peek().is_some_and(|arg| arg == "tui");
        if !tui_runtime && vmux_dx::DxPlugin::selected(args) {
            let mut app = App::new();
            app.add_plugins((vmux_dx::DxPlugin, command::CliRuntimePlugin));
            return app.run();
        }
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
    app.add_plugins((vmux_dx::DxPlugin, command::CliRuntimePlugin));
    app.run()
}
