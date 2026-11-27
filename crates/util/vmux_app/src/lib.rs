#[cfg(feature = "daemon")]
pub use host_runtime::SessionHostRuntimePlugin;
#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "ecs")]
pub use plugin::VmuxEcsPlugin;
#[cfg(all(feature = "runtime", not(target_os = "ios")))]
pub use plugin::VmuxRuntimePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
#[cfg(feature = "cli")]
pub use tool::VmuxToolPlugin;
#[cfg(feature = "account")]
pub use vmux_account as account;
#[cfg(feature = "ecs")]
pub use vmux_ecs as ecs;
#[cfg(any(feature = "extension-core", feature = "extension"))]
pub use vmux_extension as extension;
#[cfg(feature = "manifest")]
pub use vmux_feature::manifest;
#[cfg(feature = "flex")]
pub use vmux_flex as flex;
#[cfg(feature = "http-server")]
pub use vmux_http as http;
#[cfg(feature = "jobs")]
pub use vmux_job as job;
#[cfg(feature = "layout")]
pub use vmux_layout as layout;
#[cfg(feature = "lsp")]
pub use vmux_lsp as lsp;
#[cfg(feature = "host")]
pub use vmux_profile as host;
#[cfg(feature = "project")]
pub use vmux_project as project;
#[cfg(feature = "quic-server")]
pub use vmux_quic as quic;
#[cfg(feature = "runner")]
pub use vmux_runner as runner;
#[cfg(feature = "store")]
pub use vmux_store as store;
#[cfg(feature = "test-harness")]
pub use vmux_test as testing;

#[cfg(feature = "release-flags")]
mod feature_flags;
#[cfg(feature = "daemon")]
mod host_runtime;
#[cfg(feature = "menu")]
pub mod menu {
    pub use vmux_macro::app_menu as menu;
    #[cfg(feature = "menu-native")]
    pub use vmux_menu::NativeMenuPlugin;
    pub use vmux_menu::{
        About, ContextMenu, Entry, EventSender, MenuAction, MenuActionPlugin, MenuBar, MenuPlugin,
        MenuSet, MenuWake, Predefined, Selected, Selection, Separator, Submenu, Transient,
        macro_support,
    };
}
#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
pub mod prelude;
#[cfg(all(feature = "profile-account", not(target_os = "ios")))]
mod profile_account;
#[cfg(feature = "cli")]
mod tool;
