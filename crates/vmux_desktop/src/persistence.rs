use bevy::prelude::*;
use bevy::scene::SceneFilter;
use moonshine_save::prelude::*;
use std::path::PathBuf;

pub(crate) struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_observer(save_on_default_event)
        .add_observer(load_on_default_event)
        .add_systems(Update, (mark_dirty_on_change, auto_save_system).chain());
    }
}

#[derive(Resource)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    dirty: bool,
}

pub(crate) fn session_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home).join("Library/Application Support/vmux/session.ron")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir().join("vmux_cef/session.ron")
    }
}

fn mark_dirty_on_change(
    mut auto_save: ResMut<AutoSave>,
    added_tabs: Query<(), Added<crate::layout::tab::Tab>>,
    added_panes: Query<(), Added<crate::layout::pane::Pane>>,
    added_spaces: Query<(), Added<crate::layout::space::Space>>,
    removed_tabs: RemovedComponents<crate::layout::tab::Tab>,
    removed_panes: RemovedComponents<crate::layout::pane::Pane>,
    changed_meta: Query<(), (Changed<vmux_header::PageMetadata>, With<crate::layout::tab::Tab>)>,
) {
    if !added_tabs.is_empty()
        || !added_panes.is_empty()
        || !added_spaces.is_empty()
        || removed_tabs.len() > 0
        || removed_panes.len() > 0
        || !changed_meta.is_empty()
    {
        auto_save.dirty = true;
        auto_save.debounce.reset();
    }
}

fn auto_save_system(
    time: Res<Time>,
    mut auto_save: ResMut<AutoSave>,
    mut commands: Commands,
) {
    auto_save.periodic.tick(time.delta());

    if auto_save.dirty {
        auto_save.debounce.tick(time.delta());
        if auto_save.debounce.is_finished() {
            do_save(&mut commands);
            auto_save.dirty = false;
        }
    }

    if auto_save.periodic.just_finished() {
        do_save(&mut commands);
    }
}

fn do_save(commands: &mut Commands) {
    let path = session_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Use an allowlist to only save our model components.
    // This avoids serialization failures from Bevy internals
    // (e.g. VisibilityClass contains TypeId which isn't serializable).
    let mut save = SaveWorld::default_into_file(path);
    save.components = SceneFilter::deny_all()
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<crate::layout::tab::Tab>()
        .allow::<crate::layout::space::Space>()
        .allow::<crate::layout::pane::Pane>()
        .allow::<crate::layout::pane::PaneSplit>()
        .allow::<crate::profile::Profile>()
        .allow::<vmux_header::PageMetadata>()
        .allow::<vmux_history::CreatedAt>()
        .allow::<vmux_history::LastActivatedAt>()
        .allow::<vmux_history::Visit>();
    commands.trigger_save(save);
}

/// Check if a session file exists and trigger load on startup.
pub(crate) fn load_session_on_startup(mut commands: Commands) {
    let path = session_path();
    if path.exists() {
        info!("Loading session from {:?}", path);
        commands.trigger_load(LoadWorld::default_from_file(path));
    }
}

/// Returns true if a saved session file exists on disk.
pub(crate) fn has_saved_session() -> bool {
    session_path().exists()
}
