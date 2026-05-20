use bevy::prelude::*;
use vmux_command::snapshot::CommandBarSettingsSnapshot;

use crate::AppSettings;
use crate::event::SETTINGS_PAGE_URL;

pub fn update_settings_snapshot(
    settings: Option<Res<AppSettings>>,
    mut snapshot: ResMut<CommandBarSettingsSnapshot>,
) {
    let changed = settings
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !changed && !snapshot.settings_page_url.is_empty() {
        return;
    }
    snapshot.settings_page_url = SETTINGS_PAGE_URL.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_url_on_run() {
        let mut app = App::new();
        app.init_resource::<CommandBarSettingsSnapshot>();
        app.add_systems(Update, update_settings_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarSettingsSnapshot>();
        assert_eq!(snap.settings_page_url, SETTINGS_PAGE_URL);
    }
}
