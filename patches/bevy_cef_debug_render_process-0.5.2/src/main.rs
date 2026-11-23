//! CEF render-process binary. Keep in lockstep with the workspace `bevy_cef_core` patch.

#![allow(clippy::all)]
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use bevy_cef_core::prelude::*;
use cef::{args::Args, *};

fn main() {
    let args = Args::new();

    #[cfg(all(target_os = "macos", feature = "debug"))]
    let _loader = {
        let loader = DebugLibraryLoader::new();
        assert!(loader.load());
        loader
    };
    #[cfg(all(target_os = "macos", not(feature = "debug")))]
    let _loader = {
        let loader =
            cef::library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    try_set_cef_embedded_page_config(CefEmbeddedPageConfig::default());
    let mut app = RenderProcessAppBuilder::build();
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    std::process::exit(code);
}
