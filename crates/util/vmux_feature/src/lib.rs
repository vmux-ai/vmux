pub use cli::{CliArgumentManifest, CliCommandManifest, CliManifest};
pub use manifest::{
    AuthFlow, AuthProvider, AuthorizationCodePkceFlow, Command, CommandBarEntry, CommandMcp,
    CommandToolbar, DeviceCodeFlow, FeatureManifest, FeatureManifestSource, McpServer,
    OpenIdConnectFlow, Page, PagePlacement, PageReuse, PageSplitAxis, PageSplitPreference,
    SearchEngineManifest, Shortcut, ShortcutKind, Tool, ToolAvailability, ToolProviderManifest,
};
pub use registry::{
    AppFeatureExt, FeatureId, FeatureManifestOwner, FeatureRegistry, FeatureRegistryEntry,
    FeatureRegistryError, FeatureRegistryPlugin,
};

mod cli;
mod manifest;
mod registry;
