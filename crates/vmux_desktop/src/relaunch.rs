use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive, JsEmitEventPlugin, Receive};
use vmux_layout::event::RestartRequestEvent;

pub(crate) struct RelaunchPlugin;

impl Plugin for RelaunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(RestartRequestEvent,)>::for_hosts(
            &["debug", "extensions", "layout"],
        ))
        .add_plugins(JsEmitEventPlugin::<PageRelaunchRequest>::default())
        .add_observer(on_restart_request)
        .add_observer(on_page_relaunch);
    }
}

#[derive(serde::Deserialize)]
struct PageRelaunchRequest {
    channel: String,
}

fn relaunch_plan(exe: &std::path::Path, pid: u32, dyld_library_path: Option<&str>) -> Vec<String> {
    let app_bundle = exe
        .ancestors()
        .nth(3)
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("app"))
        .and_then(|path| path.to_str());
    let launch = match app_bundle {
        Some(app) => format!("open \"{app}\""),
        None => match dyld_library_path {
            Some(dyld) if !dyld.is_empty() => {
                format!("DYLD_LIBRARY_PATH=\"{dyld}\" \"{}\"", exe.display())
            }
            _ => format!("\"{}\"", exe.display()),
        },
    };
    vec![
        "-c".to_string(),
        format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; {launch}"),
    ]
}

fn relaunch_now(exit: &mut MessageWriter<AppExit>) {
    let Ok(exe) = std::env::current_exe() else {
        bevy::log::error!("restart requested but current_exe() is unavailable");
        return;
    };
    let dyld = std::env::var("DYLD_LIBRARY_PATH").ok();
    let args = relaunch_plan(&exe, std::process::id(), dyld.as_deref());
    if let Err(error) = std::process::Command::new("sh").args(&args).spawn() {
        bevy::log::error!("failed to spawn relauncher: {error}");
        return;
    }
    bevy::log::info!("relaunching");
    exit.write(AppExit::Success);
}

fn on_restart_request(
    _trigger: On<BinReceive<RestartRequestEvent>>,
    mut exit: MessageWriter<AppExit>,
) {
    relaunch_now(&mut exit);
}

fn on_page_relaunch(trigger: On<Receive<PageRelaunchRequest>>, mut exit: MessageWriter<AppExit>) {
    if trigger.payload.channel == "vmux-relaunch" {
        relaunch_now(&mut exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relaunch_plan_opens_app_bundle() {
        let exe = std::path::Path::new("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
        let args = relaunch_plan(exe, 4242, None);
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("kill -0 4242"));
        assert!(args[1].contains("open \"/Applications/Vmux.app\""));
    }

    #[test]
    fn relaunch_plan_reexecs_bare_binary_in_dev_with_dyld() {
        let exe = std::path::Path::new("/tmp/target/debug/vmux_desktop");
        let args = relaunch_plan(exe, 7, Some("/rust/lib:/tmp/target/debug/deps"));
        assert!(args[1].contains("kill -0 7"));
        assert!(
            args[1].contains("DYLD_LIBRARY_PATH=\"/rust/lib:/tmp/target/debug/deps\" \"/tmp/target/debug/vmux_desktop\"")
        );
        assert!(!args[1].contains("open \""));
    }
}
