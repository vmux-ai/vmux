use bevy::prelude::*;

pub struct ExtensionPlugin;

impl Plugin for ExtensionPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::ExtensionPage::plugin())
            .add_systems(Startup, spawn_extension_page);

        app.add_plugins((
            crate::catalog::ExtensionCatalogPlugin,
            vmux_core::host::UiStatePlugin::<vmux_api::extension::ExtensionsEvent>::default(),
        ));
    }
}

#[cfg(ui)]
fn spawn_extension_page(mut commands: Commands) {
    commands.spawn((
        crate::ui::ExtensionPage::MANIFEST,
        vmux_core::host::page::NativelyHosted::page(
            crate::ui::ExtensionPage::URL,
            crate::ui::ExtensionPage::NATIVE.title,
        ),
    ));
}
