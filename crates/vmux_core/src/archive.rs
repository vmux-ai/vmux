use bevy::prelude::*;
use moonshine_save::prelude::*;

use crate::terminal::TerminalLaunch;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core::archive"]
pub struct ArchivedPage {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub closed_at: i64,
    pub launch: Option<TerminalLaunch>,
}

#[derive(Message, Clone, Debug)]
pub struct PageArchiveRequest {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub launch: Option<TerminalLaunch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_page_defaults_are_empty() {
        let a = ArchivedPage::default();
        assert!(a.url.is_empty());
        assert!(a.launch.is_none());
        assert_eq!(a.closed_at, 0);
    }

    #[test]
    fn archived_page_is_registered_by_core_plugin() {
        let mut app = App::new();
        app.add_plugins(crate::CorePlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry
                .get(std::any::TypeId::of::<ArchivedPage>())
                .is_some()
        );
    }
}
