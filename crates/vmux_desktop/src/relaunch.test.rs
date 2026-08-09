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
