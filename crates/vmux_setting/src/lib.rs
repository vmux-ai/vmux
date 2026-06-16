#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
#[cfg(target_arch = "wasm32")]
pub mod page;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
pub mod schema;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot_updater;
pub mod themes;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest { host: "settings" };

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::SettingsPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::runtime::{
    AgentSettings, AppProviderSettings, AppSettings, BrowserSettings, KeyComboDef,
    LastSelfWriteHash, SettingsLoadSet, SettingsWriteRequest, ShortcutDef, ShortcutEntry,
    ShortcutSettings, SpaceOverrides, TerminalSettings, TerminalTheme, apply_settings_update,
    load_settings, resolve_startup_dir, resolve_startup_url, serialize_settings_to_json,
    set_at_path,
};
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::view::Settings;
