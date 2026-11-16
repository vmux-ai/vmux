use bevy_app::{App, Plugin};

#[vmux_page::page]
pub struct FeaturePagePlugin;

impl Plugin for FeaturePagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::FeaturePlugin>() {
            app.add_plugins(crate::FeaturePlugin);
        }
        app.add_plugins(vmux_ecs::manifest::FeaturePlugin::<Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::FeaturesPage::plugin());
        #[cfg(all(host, ui))]
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::subtree(
                    Self::URL,
                    crate::ui::FeaturesPage::PAGE.title,
                )),
        )
        .add_plugins(crate::host::FeatureHostPlugin);
    }
}

struct Feature;

impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(all(host, ui))]
pub use crate::native::Endpoint;
