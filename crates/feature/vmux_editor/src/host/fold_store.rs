use std::path::PathBuf;
use std::sync::Arc;
use std::task::Wake;

use bevy::app::Plugin as BevyPlugin;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use vmux_ecs::Revision;
use vmux_ecs::profile::{ActiveProfile, ProfileDirectories};
use vmux_fold::filesystem::{FileWake, FilesystemPlugin};
use vmux_fold::{
    Collapsed, FileId, FoldPlugin, FoldSet, Mutation, Preferences, RetryRequest, Root, State,
    Ticket, UpdateRequest,
};

use super::editor::{Editor, FileView};
use super::file_lifecycle::EditorFileLoadedSet;
use super::viewport::{CursorRenderRequest, FoldsDirty, ViewportRenderRequest};

pub(super) struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        assert!(
            app.is_plugin_added::<FoldPlugin>(),
            "Editor requires FoldPlugin in the application composition"
        );
        app.add_plugins(FilesystemPlugin)
            .add_systems(Update, (bind, forward).chain().before(FoldSet::Restore))
            .add_systems(
                Update,
                attach
                    .after(EditorFileLoadedSet)
                    .after(FoldSet::Restore)
                    .before(FoldSet::Mutate),
            )
            .add_systems(Update, (project, report).in_set(FoldSet::Project));
    }
}

#[derive(Component)]
struct Binding {
    ticket: Ticket,
    path: PathBuf,
    revision: Option<Revision>,
    projected: Collapsed,
    initialized: bool,
}

struct EventLoopWake(vmux_ecs::wake::Wake);

impl Wake for EventLoopWake {
    fn wake(self: Arc<Self>) {
        self.0.now();
    }
}

fn bind(
    profiles: Query<(Entity, &ProfileDirectories, Option<&Root>), With<ActiveProfile>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    wake: Option<Res<FileWake>>,
    mut commands: Commands,
) {
    for (entity, directories, root) in &profiles {
        let path = directories.0.profile().join("folds.ron");
        if root.is_none_or(|root| root.0 != path) {
            commands.entity(entity).insert(Root(path));
        }
    }
    if wake.is_none()
        && let Some(proxy) = proxy
    {
        let wake = EventLoopWake(vmux_ecs::wake::Wake::beside(Some(&proxy)));
        commands.insert_resource(FileWake(Arc::new(wake).into()));
    }
}

fn attach(
    roots: Query<Entity, (With<Root>, With<ActiveProfile>)>,
    preferences: Preferences,
    views: Query<(Entity, &FileView, Ref<Editor>, Option<&Binding>)>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    let Some(ticket) = preferences.ticket(root) else {
        return;
    };
    for (entity, file, editor, binding) in &views {
        let path = vmux_path::PathIdentity::resolve(&file.path).into_path_buf();
        if !editor.is_added()
            && binding.is_some_and(|binding| binding.ticket == *ticket && binding.path == path)
        {
            continue;
        }
        if binding.is_some() {
            commands.entity(entity).remove::<FoldsDirty>();
        }
        commands.entity(entity).insert(Binding {
            ticket: ticket.clone(),
            path,
            revision: None,
            projected: Collapsed::default(),
            initialized: false,
        });
    }
}

fn forward(
    views: Query<(Entity, &FileView, &Editor, &Binding), With<FoldsDirty>>,
    preferences: Preferences,
    roots: Query<&State, With<Root>>,
    mut updates: MessageWriter<UpdateRequest>,
    mut retries: MessageWriter<RetryRequest>,
    mut commands: Commands,
) {
    for (entity, file, editor, binding) in &views {
        if !preferences.ready(&binding.ticket)
            || vmux_path::PathIdentity::resolve(&file.path).as_path() != binding.path
        {
            continue;
        }
        let current: std::collections::BTreeSet<_> =
            editor.folds.collapsed.iter().copied().collect();
        let collapse = current.difference(&binding.projected.0).copied().collect();
        let expand = binding.projected.0.difference(&current).copied().collect();
        updates.write(UpdateRequest {
            ticket: binding.ticket.clone(),
            file: FileId(binding.path.clone()),
            mutation: Mutation::Change { collapse, expand },
        });
        if roots
            .get(binding.ticket.owner)
            .is_ok_and(|state| state.error().is_some())
        {
            retries.write(RetryRequest(binding.ticket.clone()));
        }
        commands.entity(entity).remove::<FoldsDirty>();
    }
}

fn project(
    preferences: Preferences,
    mut views: Query<(
        Entity,
        &FileView,
        &mut Editor,
        &mut Binding,
        Has<FoldsDirty>,
    )>,
    mut commands: Commands,
) {
    for (entity, file, mut editor, mut binding, dirty) in &mut views {
        if !preferences.ready(&binding.ticket)
            || vmux_path::PathIdentity::resolve(&file.path).as_path() != binding.path
        {
            continue;
        }
        let record = preferences.get(&binding.ticket, &binding.path);
        let revision = record.map(|(_, _, revision)| *revision);
        if binding.initialized && binding.revision == revision {
            continue;
        }
        if dirty {
            continue;
        }
        editor.folds.collapsed.clear();
        if let Some((_, collapsed, _)) = record {
            editor.folds.collapsed.extend(collapsed.0.iter().copied());
        }
        editor.folds.reconcile();
        editor.sync_fold_view();
        binding.projected = Collapsed(editor.folds.collapsed.iter().copied().collect());
        binding.revision = revision;
        binding.initialized = true;
        commands.trigger(ViewportRenderRequest::new(entity));
        commands.trigger(CursorRenderRequest::new(entity));
    }
}

fn report(roots: Query<(&Root, &State), Changed<State>>) {
    for (root, state) in &roots {
        if let Some(error) = state.error() {
            tracing::warn!(path = %root.0.display(), "fold store failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::edit::{EditCore, EditMode, highlight_cache::HighlightCache};
    use crate::host::fold::FoldState;
    use vmux_fold::{LoadCompleted, LoadRequest, SaveRequest, Snapshot};

    struct Fixture {
        app: App,
        ticket: Ticket,
        views: Vec<Entity>,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().canonicalize().unwrap().join("shared.rs");
            let mut app = App::new();
            app.add_plugins(FoldPlugin)
                .add_systems(Update, forward.before(FoldSet::Restore))
                .add_systems(
                    Update,
                    attach.after(FoldSet::Restore).before(FoldSet::Mutate),
                )
                .add_systems(Update, project.in_set(FoldSet::Project));
            app.world_mut().spawn((
                Root(directory.path().join("folds.ron")),
                ActiveProfile(vmux_ecs::profile::Profile::current()),
            ));
            let mut views = Vec::new();
            for _ in 0..2 {
                let core = EditCore::new(
                    path.clone(),
                    "Rust".into(),
                    "first:\n  one\nsecond:\n  two\n",
                    EditMode::Normal,
                );
                let mut folds = FoldState::default();
                folds.set_indented(&core.buffer.rope);
                views.push(
                    app.world_mut()
                        .spawn((
                            FileView { path: path.clone() },
                            Editor::new(core, HighlightCache::plain(&path), folds),
                        ))
                        .id(),
                );
            }
            app.update();
            let ticket = app
                .world_mut()
                .resource_mut::<Messages<LoadRequest>>()
                .drain()
                .next()
                .unwrap()
                .0;
            Self {
                app,
                ticket,
                views,
                directory,
            }
        }

        fn restore(&mut self, lines: &[u32]) {
            let path = self
                .app
                .world()
                .get::<FileView>(self.views[0])
                .unwrap()
                .path
                .to_string_lossy()
                .into_owned();
            self.app.world_mut().write_message(LoadCompleted {
                ticket: self.ticket.clone(),
                result: Ok(Snapshot {
                    files: [(path, lines.to_vec())].into(),
                }),
            });
            self.app.update();
        }

        fn toggle(&mut self, index: usize, line: u32) {
            self.app
                .world_mut()
                .get_mut::<Editor>(self.views[index])
                .unwrap()
                .folds
                .toggle_header(line);
            self.app
                .world_mut()
                .entity_mut(self.views[index])
                .insert(FoldsDirty);
        }

        fn collapsed(&self, index: usize) -> std::collections::HashSet<u32> {
            self.app
                .world()
                .get::<Editor>(self.views[index])
                .unwrap()
                .folds
                .collapsed
                .clone()
        }
    }

    #[test]
    fn separate_native_views_share_changes_without_copying_local_fold_regions() {
        let mut fixture = Fixture::new();
        fixture.restore(&[]);
        fixture.toggle(0, 0);
        fixture.toggle(1, 2);
        fixture.app.update();
        assert_eq!(
            fixture.collapsed(0),
            std::collections::HashSet::from([0, 2])
        );
        assert_eq!(
            fixture.collapsed(1),
            std::collections::HashSet::from([0, 2])
        );
        let snapshot = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<SaveRequest>>()
            .drain()
            .next()
            .unwrap()
            .snapshot;
        let path = fixture
            .app
            .world()
            .get::<FileView>(fixture.views[0])
            .unwrap()
            .path
            .to_string_lossy();
        assert_eq!(snapshot.files[path.as_ref()], [0, 2]);
    }

    #[test]
    fn a_local_fold_intent_survives_pending_restore_and_preserves_other_stored_lines() {
        let mut fixture = Fixture::new();
        fixture.toggle(0, 0);
        fixture.restore(&[2]);
        assert_eq!(fixture.collapsed(0), std::collections::HashSet::from([0]));
        fixture.app.update();
        assert_eq!(
            fixture.collapsed(0),
            std::collections::HashSet::from([0, 2])
        );
        assert_eq!(
            fixture.collapsed(1),
            std::collections::HashSet::from([0, 2])
        );
    }

    #[test]
    fn navigation_refuses_pending_fold_changes_for_the_previous_file() {
        let mut fixture = Fixture::new();
        fixture.restore(&[]);
        fixture.toggle(0, 0);
        fixture
            .app
            .world_mut()
            .get_mut::<FileView>(fixture.views[0])
            .unwrap()
            .path = fixture.directory.path().join("other.rs");
        fixture.app.update();
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .is_none()
        );
        assert!(fixture.collapsed(1).is_empty());
    }

    #[test]
    fn reloading_the_same_file_projects_its_durable_folds_into_the_new_editor() {
        let mut fixture = Fixture::new();
        fixture.restore(&[0, 2]);
        let entity = fixture.views[0];
        let path = fixture
            .app
            .world()
            .get::<FileView>(entity)
            .unwrap()
            .path
            .clone();
        let core = EditCore::new(
            path.clone(),
            "Rust".into(),
            "first:\n  one\nsecond:\n  two\n",
            EditMode::Normal,
        );
        let mut folds = FoldState::default();
        folds.set_indented(&core.buffer.rope);
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .remove::<Editor>()
            .insert(Editor::new(core, HighlightCache::plain(&path), folds));
        fixture.app.update();
        assert_eq!(
            fixture.collapsed(0),
            std::collections::HashSet::from([0, 2])
        );
    }
}
