use bevy_app::prelude::*;

pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::ServicePage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
    }
}
