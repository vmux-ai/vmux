use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::service::RelaunchRequest;

pub(crate) struct RelaunchPlugin;

impl Plugin for RelaunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_cef::prelude::UiEventPlugin::<(RelaunchRequest,)>::default())
            .add_message::<RelaunchApplication>()
            .add_message::<LaunchProfile>()
            .add_observer(restart_request)
            .add_systems(
                Update,
                (request_profile_launch, relaunch, launch_profiles).chain(),
            );
    }
}

fn relaunch_plan(
    exe: &std::path::Path,
    pid: u32,
    dyld_library_path: Option<&str>,
    profile: Option<&str>,
    data_dir: Option<&str>,
) -> Vec<std::ffi::OsString> {
    let app_bundle = exe
        .ancestors()
        .nth(3)
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("app"));
    let mut plan: Vec<std::ffi::OsString> = match (app_bundle, dyld_library_path, profile) {
        (Some(app), _, Some(profile)) => vec![
            "-c".into(),
            format!(
                "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open --env \"$2\" \"$1\""
            )
            .into(),
            "vmux-relauncher".into(),
            app.as_os_str().into(),
            format!("VMUX_PROFILE={profile}").into(),
        ],
        (Some(app), _, None) => vec![
            "-c".into(),
            format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open \"$1\"").into(),
            "vmux-relauncher".into(),
            app.as_os_str().into(),
        ],
        (None, Some(dyld), Some(profile)) if !dyld.is_empty() => vec![
            "-c".into(),
            format!(
                "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; DYLD_LIBRARY_PATH=\"$2\" VMUX_PROFILE=\"$3\" \"$1\""
            )
            .into(),
            "vmux-relauncher".into(),
            exe.as_os_str().into(),
            dyld.into(),
            profile.into(),
        ],
        (None, _, Some(profile)) => vec![
            "-c".into(),
            format!(
                "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; VMUX_PROFILE=\"$2\" \"$1\""
            )
            .into(),
            "vmux-relauncher".into(),
            exe.as_os_str().into(),
            profile.into(),
        ],
        (None, Some(dyld), None) if !dyld.is_empty() => vec![
            "-c".into(),
            format!(
                "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; DYLD_LIBRARY_PATH=\"$2\" \"$1\""
            )
            .into(),
            "vmux-relauncher".into(),
            exe.as_os_str().into(),
            dyld.into(),
        ],
        (None, _, None) => vec![
            "-c".into(),
            format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; \"$1\"").into(),
            "vmux-relauncher".into(),
            exe.as_os_str().into(),
        ],
    };
    if app_bundle.is_some()
        && let Some(data_dir) = data_dir
    {
        let position = plan.len() - 2;
        let script = plan[1]
            .to_string_lossy()
            .replace("done; open ", &format!("done; open --env \"${position}\" "));
        plan[1] = script.into();
        plan.push(format!("VMUX_DATA_DIR={data_dir}").into());
    }
    plan
}

#[derive(Message)]
struct RelaunchApplication;

#[derive(Message)]
struct LaunchProfile(String);

fn relaunch(mut requests: MessageReader<RelaunchApplication>, mut exit: MessageWriter<AppExit>) {
    if requests.read().count() == 0 {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        bevy::log::error!("restart requested but current_exe() is unavailable");
        return;
    };
    let dyld = std::env::var("DYLD_LIBRARY_PATH").ok();
    let data_dir = std::env::var("VMUX_DATA_DIR").ok();
    let profile = vmux_profile::Profile::current();
    let args = relaunch_plan(
        &exe,
        std::process::id(),
        dyld.as_deref(),
        Some(profile.id()),
        data_dir.as_deref(),
    );
    if let Err(error) = std::process::Command::new("sh").args(&args).spawn() {
        bevy::log::error!("failed to spawn relauncher: {error}");
        return;
    }
    bevy::log::info!("relaunching");
    exit.write(AppExit::Success);
}

fn launch_profiles(mut requests: MessageReader<LaunchProfile>, mut exits: MessageWriter<AppExit>) {
    let Some(LaunchProfile(profile)) = requests.read().last() else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        bevy::log::error!("profile launch requested but current_exe() is unavailable");
        return;
    };
    let dyld = std::env::var("DYLD_LIBRARY_PATH").ok();
    let data_dir = std::env::var("VMUX_DATA_DIR").ok();
    let args = relaunch_plan(
        &exe,
        std::process::id(),
        dyld.as_deref(),
        Some(profile),
        data_dir.as_deref(),
    );
    if let Err(error) = std::process::Command::new("sh").args(&args).spawn() {
        bevy::log::error!("failed to launch profile: {error}");
        return;
    }
    bevy::log::info!(profile, "switching profile");
    exits.write(AppExit::Success);
}

fn restart_request(
    _trigger: On<UiInput<RelaunchRequest>>,
    mut requests: MessageWriter<RelaunchApplication>,
) {
    requests.write(RelaunchApplication);
}

fn request_profile_launch(
    mut requests: MessageReader<vmux_team::ProfileSwitchRequested>,
    mut launches: MessageWriter<LaunchProfile>,
) {
    let Some(request) = requests.read().last() else {
        return;
    };
    launches.write(LaunchProfile(request.profile_id.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn relaunch_plan_opens_app_bundle() {
        let exe = std::path::Path::new("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
        let args = relaunch_plan(exe, 4242, None, None, None);
        assert_eq!(args[0], "-c");
        assert!(args[1].to_string_lossy().contains("kill -0 4242"));
        assert!(args[1].to_string_lossy().contains("open \"$1\""));
        assert_eq!(args[3], "/Applications/Vmux.app");
    }

    #[test]
    fn relaunch_plan_reexecs_bare_binary_in_dev_with_dyld() {
        let exe = std::path::Path::new("/tmp/target/debug/vmux_desktop");
        let args = relaunch_plan(exe, 7, Some("/rust/lib:/tmp/target/debug/deps"), None, None);
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
        let args = relaunch_plan(exe, 8, Some(""), None, None);
        let script = args[1].to_string_lossy();
        assert!(!script.contains("DYLD_LIBRARY_PATH"));
        assert!(script.contains("\"$1\""));
        assert_eq!(args.len(), 4);
        assert_eq!(args[3], "/tmp/target/debug/vmux_desktop");
    }

    #[test]
    fn relaunch_plan_keeps_shell_syntax_out_of_script() {
        let exe = std::path::Path::new("/tmp/$(touch vmux-injected)");
        let args = relaunch_plan(exe, 9, Some("`touch vmux-dyld-injected`"), None, None);
        let script = args[1].to_string_lossy();
        assert!(!script.contains("vmux-injected"));
        assert!(!script.contains("vmux-dyld-injected"));
        assert_eq!(args[3], "/tmp/$(touch vmux-injected)");
        assert_eq!(args[4], "`touch vmux-dyld-injected`");
    }

    #[cfg(unix)]
    #[test]
    fn bundled_restart_preserves_the_copied_data_root_without_shell_evaluation() {
        let exe = std::path::Path::new("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
        let directory = tempfile::tempdir().unwrap();
        let launcher = directory.path().join("open");
        std::fs::write(
            &launcher,
            "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$VMUX_RELAUNCH_CAPTURE\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        let capture = directory.path().join("arguments");
        for profile in [None, Some("work")] {
            let args = relaunch_plan(
                exe,
                999_999,
                None,
                profile,
                Some("/tmp/copy with spaces/$(injected)"),
            );
            let status = std::process::Command::new("/bin/sh")
                .args(args)
                .env("PATH", directory.path())
                .env("VMUX_RELAUNCH_CAPTURE", &capture)
                .status()
                .unwrap();
            assert!(status.success());
            let mut expected = vec!["--env", "VMUX_DATA_DIR=/tmp/copy with spaces/$(injected)"];
            if profile.is_some() {
                expected.extend(["--env", "VMUX_PROFILE=work"]);
            }
            expected.push("/Applications/Vmux.app");
            let actual = std::fs::read_to_string(&capture).unwrap();
            assert_eq!(actual.split_terminator('\0').collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn relaunch_plan_passes_the_selected_profile() {
        let exe = std::path::Path::new("/Applications/Vmux.app/Contents/MacOS/vmux_desktop");
        let args = relaunch_plan(exe, 10, None, Some("client-work"), None);

        assert!(
            args[1]
                .to_string_lossy()
                .contains("open --env \"$2\" \"$1\"")
        );
        assert_eq!(args[3], "/Applications/Vmux.app");
        assert_eq!(args[4], "VMUX_PROFILE=client-work");
    }
}
