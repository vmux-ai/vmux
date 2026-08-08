//! Application settings: a typed schema (appearance, editor, browser, agent, lsp,
//! shortcuts, spaces), RON load/save with debounce, theme definitions, and the settings webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(not(web))]
pub mod appearance;
pub mod event;
#[cfg(web)]
pub mod page;
#[cfg(not(web))]
pub mod plugin;
pub mod schema;
pub mod themes;

#[cfg(not(web))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "settings",
    title: "Settings",
    keywords: &["preferences", "config"],
    icon: Some(vmux_core::BuiltinIcon::Settings),
    command_bar: true,
};

#[cfg(not(web))]
pub use appearance::{ColorSchemeChanged, ResolvedColorScheme, ResolvedScheme, SystemAppearance};
#[cfg(not(web))]
pub use plugin::SettingsPlugin;
#[cfg(not(web))]
pub use plugin::runtime::{
    AcpAgentConfig, AgentSettings, AppProviderSettings, AppSettings, AppearanceSettings,
    BrowserSettings, ColorScheme, DirSource, EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH,
    EXPLORER_MIN_WIDTH, EditorSettings, ExplorerSettings, KeyComboDef, LastSelfWriteHash,
    LspServerOverride, LspSettings, SettingsLoadSet, SettingsSaveRequest, SettingsWriteRequest,
    ShortcutDef, ShortcutEntry, ShortcutSettings, SpaceOverrides, TerminalSettings, TerminalTheme,
    apply_settings_update, load_settings, read_settings_from_disk, resolve_startup_dir,
    resolve_startup_dir_for_tab, resolve_startup_dir_for_tab_with_source, resolve_startup_url,
    resolve_tab_workspace_dir, serialize_settings_to_json, set_at_path, validate_tab_workspace_dir,
};
#[cfg(not(web))]
pub use plugin::view::Settings;
#[cfg(not(web))]
pub use vmux_command::event::SearchEngine;
