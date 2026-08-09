use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, Receive};
use vmux_layout::event::RestartRequestEvent;

#[derive(serde::Deserialize)]
pub(crate) struct PageRelaunchRequest {
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

pub(crate) fn on_restart_request(
    _trigger: On<BinReceive<RestartRequestEvent>>,
    mut exit: MessageWriter<AppExit>,
) {
    relaunch_now(&mut exit);
}

pub(crate) fn on_page_relaunch(
    trigger: On<Receive<PageRelaunchRequest>>,
    mut exit: MessageWriter<AppExit>,
) {
    if trigger.payload.channel == "vmux-relaunch" {
        relaunch_now(&mut exit);
    }
}

#[cfg(test)]
#[path = "relaunch.test.rs"]
mod tests;
