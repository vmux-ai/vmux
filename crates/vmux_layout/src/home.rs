pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::HomePlugin;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "home",
    title: "Home",
    keywords: &["home", "start", "new tab", "launcher"],
    icon: Some(vmux_core::icon::BuiltinIcon::Sparkles),
    command_bar: true,
};
