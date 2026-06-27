//! Application settings: a typed schema (appearance, editor, browser, agent, lsp,
//! shortcuts, spaces), RON load/save with debounce, theme definitions, and the settings webview.
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
pub mod themes;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "settings",
    title: "Settings",
    keywords: &["preferences", "config"],
    icon: Some(vmux_core::BuiltinIcon::Settings),
    command_bar: true,
};

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::SettingsPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::runtime::{
    AgentSettings, AppProviderSettings, AppSettings, AppearanceSettings, BrowserSettings,
    ColorScheme, EditorSettings, KeyComboDef, LastSelfWriteHash, LspServerOverride, LspSettings,
    SettingsLoadSet, SettingsSaveRequest, SettingsWriteRequest, ShortcutDef, ShortcutEntry,
    ShortcutSettings, SpaceOverrides, TerminalSettings, TerminalTheme, apply_settings_update,
    load_settings, resolve_startup_dir, resolve_startup_url, serialize_settings_to_json,
    set_at_path,
};
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::view::Settings;
