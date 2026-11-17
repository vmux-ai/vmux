#[cfg(all(host, ui))]
pub use crate::native::Endpoint;
use bevy_app::{App, Plugin};

#[vmux_page::page(owner = Feature)]
pub struct FeaturePagePlugin;

impl Plugin for FeaturePagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::FeaturePlugin>() {
            app.add_plugins(crate::FeaturePlugin);
        }
        app.add_plugins(crate::manifest::FeaturePlugin::<Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::FeaturesPage::plugin());
        #[cfg(all(host, ui))]
        app.add_plugins(
            Self::manifest()
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::subtree(
                    Self::url(),
                    crate::ui::FeaturesPage::page().title,
                )),
        )
        .add_plugins(crate::host::FeatureHostPlugin);
    }
}

pub(crate) struct Feature;

impl crate::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}
