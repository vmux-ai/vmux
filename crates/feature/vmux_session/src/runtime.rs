use bevy_app::{App, Plugin};

pub struct SessionHostPlugin;

impl Plugin for SessionHostPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::SessionDomainPlugin>() {
            app.add_plugins(crate::SessionDomainPlugin);
        }
        if !app.is_plugin_added::<crate::provider::ProviderPlugin>() {
            app.add_plugins(crate::provider::ProviderPlugin);
        }
        if !app.is_plugin_added::<crate::QueuePlugin>() {
            app.add_plugins(crate::QueuePlugin);
        }
        if !app.is_plugin_added::<crate::history::HistoryPlugin>() {
            app.add_plugins(crate::history::HistoryPlugin);
        }
        app.add_plugins(crate::catalog::CatalogPlugin);
    }
}
