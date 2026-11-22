#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub use host::{
    AcpAgentConfig, ActivateSpaceProject, AgentSettings, AppSettings, AppearanceSettings,
    AssociateKnownProject, BookmarkDefaults, BookmarkFolderSettings, BrowserSettings, ColorScheme,
    ColorSchemeChanged, DirSource, ForgetSpaceProject, KeyComboDef, KnownProject,
    KnownProjectCheckout, KnownProjectId, KnownProjectLabel, KnownProjectOrder, KnownProjectPath,
    RememberSpaceProject, RenameKnownProject, ResolvedColorScheme, ResolvedScheme,
    SearchEngineSetting, SettingToolPlugin, Settings, SettingsLoadSet, SettingsPlugin,
    SettingsPolicySet, SettingsRuntimePlugin, SettingsSaveRequest, ShortcutDef, ShortcutEntry,
    ShortcutSettings, SpaceActiveProject, SpaceBindingCheckout, SpaceBindingOrder,
    SpaceBindingProject, SpaceBindingTarget, SpaceOverrides, SpaceProject, SpaceProjectBinding,
    SpaceSetting, SpaceSettingKey, SpaceSettings, SpaceStartupUrl, StartupDir, SystemAppearance,
    TerminalSettings, TerminalTheme, UpdateChannel,
};
pub use state::{EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH};

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
pub mod state;
#[cfg(host)]
pub mod themes;

#[cfg(host)]
mod schema;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
