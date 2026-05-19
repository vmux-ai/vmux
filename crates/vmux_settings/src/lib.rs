#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
pub mod schema;
pub mod themes;
#[cfg(not(target_arch = "wasm32"))]
pub mod view;

#[cfg(not(target_arch = "wasm32"))]
pub use runtime::{
    AgentSettings, AppProviderSettings, AppSettings, BrowserSettings, KeyComboDef,
    LastSelfWriteHash, SettingsCorePlugin, SettingsLoadSet, SettingsWriteRequest, ShortcutDef,
    ShortcutEntry, ShortcutSettings, TerminalSettings, TerminalTheme, apply_settings_update,
    load_settings, resolve_startup_url, serialize_settings_to_json, set_at_path,
};

#[cfg(not(target_arch = "wasm32"))]
pub use view::{SettingsPlugin, SettingsView};
