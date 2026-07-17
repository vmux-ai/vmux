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

fn relaunch_plan(
    exe: &std::path::Path,
    pid: u32,
    dyld_library_path: Option<&str>,
) -> Vec<std::ffi::OsString> {
    let app_bundle = exe
        .ancestors()
        .nth(3)
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("app"));
    match app_bundle {
        Some(app) => vec![
            "-c".into(),
            format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open \"$1\"").into(),
            "vmux-relauncher".into(),
            app.as_os_str().into(),
        ],
        None => match dyld_library_path {
            Some(dyld) if !dyld.is_empty() => vec![
                "-c".into(),
                format!(
                    "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; DYLD_LIBRARY_PATH=\"$2\" \"$1\""
                )
                .into(),
                "vmux-relauncher".into(),
                exe.as_os_str().into(),
                dyld.into(),
            ],
            _ => vec![
                "-c".into(),
                format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; \"$1\"").into(),
                "vmux-relauncher".into(),
                exe.as_os_str().into(),
            ],
        },
    }
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
        assert!(args[1].to_string_lossy().contains("kill -0 4242"));
        assert!(args[1].to_string_lossy().contains("open \"$1\""));
        assert_eq!(args[3], "/Applications/Vmux.app");
    }

    #[test]
    fn relaunch_plan_reexecs_bare_binary_in_dev_with_dyld() {
        let exe = std::path::Path::new("/tmp/target/debug/vmux_desktop");
        let args = relaunch_plan(exe, 7, Some("/rust/lib:/tmp/target/debug/deps"));
        let script = args[1].to_string_lossy();
        assert!(script.contains("kill -0 7"));
        assert!(script.contains("DYLD_LIBRARY_PATH=\"$2\" \"$1\""));
        assert!(!script.contains("open \""));
        assert_eq!(args[3], "/tmp/target/debug/vmux_desktop");
        assert_eq!(args[4], "/rust/lib:/tmp/target/debug/deps");
    }

    #[test]
    fn relaunch_plan_reexecs_bare_binary_without_empty_dyld() {
        let exe = std::path::Path::new("/tmp/target/debug/vmux_desktop");
        let args = relaunch_plan(exe, 8, Some(""));
        let script = args[1].to_string_lossy();
        assert!(!script.contains("DYLD_LIBRARY_PATH"));
        assert!(script.contains("\"$1\""));
        assert_eq!(args.len(), 4);
        assert_eq!(args[3], "/tmp/target/debug/vmux_desktop");
    }

    #[test]
    fn relaunch_plan_keeps_shell_syntax_out_of_script() {
        let exe = std::path::Path::new("/tmp/$(touch vmux-injected)");
        let args = relaunch_plan(exe, 9, Some("`touch vmux-dyld-injected`"));
        let script = args[1].to_string_lossy();
        assert!(!script.contains("vmux-injected"));
        assert!(!script.contains("vmux-dyld-injected"));
        assert_eq!(args[3], "/tmp/$(touch vmux-injected)");
        assert_eq!(args[4], "`touch vmux-dyld-injected`");
    }
}
