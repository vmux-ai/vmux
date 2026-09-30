use bevy::prelude::*;

#[vmux_native::page]
pub struct ExtensionPlugin;

impl Plugin for ExtensionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_core::host::manifest::FeatureManifestPlugin::<
            crate::Feature,
        >::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::ExtensionPage::plugin());

        app.add_plugins((
            crate::catalog::ExtensionCatalogPlugin,
            vmux_core::host::UiStatePlugin::<vmux_api::extension::ExtensionsEvent>::default(),
        ));
    }
}
