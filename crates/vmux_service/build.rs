use std::path::PathBuf;

fn main() {
    let profile = std::env::var("VMUX_BUILD_PROFILE").unwrap_or_else(|_| {
        match std::env::var("PROFILE").as_deref() {
            Ok("release") => "release".to_string(),
            _ => "dev".to_string(),
        }
    });
    println!("cargo::rustc-env=VMUX_BUILD_PROFILE={profile}");
    println!("cargo::rerun-if-env-changed=VMUX_BUILD_PROFILE");

    use vmux_webview_app::build::{CefEmbeddedWebviewFinalize, WebviewAppBuilder};
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_service", "vmux_service_app")
        .track_manifest_rel_paths(&["tailwind.config.js", "../vmux_ui/assets/theme.css"])
        .dx_extra_args(&["--bin", "vmux_service_app", "--features", "web"])
        .cef_finalize(CefEmbeddedWebviewFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxs", "services-dxs"])
        .run("vmux_service");
}
