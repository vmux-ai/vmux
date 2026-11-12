use std::path::PathBuf;
use std::sync::Arc;
use std::task::Wake;

use crate::host::fold_io::{FileWake, FilesystemPlugin};
use crate::host::fold_preferences::{
    Collapsed, FileId, FoldPlugin, FoldSet, Mutation, Preferences, RetryRequest, Root, State,
    Ticket, UpdateRequest,
};
use bevy::app::Plugin as BevyPlugin;
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use vmux_ecs::Revision;
use vmux_ecs::profile::{ActiveProfile, ProfileDirectories};

use super::editor::{Editor, FileView};
use super::file_lifecycle::EditorFileLoadedSet;
use super::viewport::{CursorRenderRequest, FoldsDirty, ViewportRenderRequest};

pub(super) struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FoldPlugin, FilesystemPlugin))
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
        let mut expand = std::collections::BTreeSet::new();
        for line in binding.projected.0.difference(&current) {
            if editor
                .folds
                .regions
                .iter()
                .any(|region| region.start == *line)
            {
                expand.insert(*line);
            }
        }
        updates.write(UpdateRequest {
            ticket: binding.ticket.clone(),
            file: FileId(binding.path.clone()),
            mutation: Mutation { collapse, expand },
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
        if binding.initialized && binding.revision == revision && !editor.is_changed() {
            continue;
        }
        if dirty {
            continue;
        }
        let mut projected = Collapsed::default();
        if let Some((_, collapsed, _)) = record {
            for region in &editor.folds.regions {
                if collapsed.0.contains(&region.start) {
                    projected.0.insert(region.start);
                }
            }
        }
        if binding.initialized
            && binding.revision == revision
            && binding.projected == projected
            && editor
                .folds
                .collapsed
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                == projected.0
        {
            continue;
        }
        editor.folds.collapsed = projected.0.iter().copied().collect();
        editor.folds.reconcile();
        editor.sync_fold_view();
        binding.projected = projected;
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
    use crate::host::fold::{FoldRegion, FoldState};
    use crate::host::fold_io::FileStore;
    use crate::host::fold_preferences::{LoadRequest, Snapshot};

    struct Fixture {
        app: App,
        ticket: Ticket,
        views: Vec<Entity>,
        directory: tempfile::TempDir,
    }

    #[derive(Component)]
    struct PendingEditor(Option<Editor>);

    impl PendingEditor {
        fn finish(mut pending: Query<(Entity, &mut Self)>, mut commands: Commands) {
            for (entity, mut pending) in &mut pending {
                if let Some(editor) = pending.0.take() {
                    commands
                        .entity(entity)
                        .remove::<Editor>()
                        .insert(editor)
                        .remove::<Self>();
                }
            }
        }
    }

    impl Fixture {
        fn new(lines: &[u32]) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().canonicalize().unwrap().join("shared.rs");
            let mut app = App::new();
            let root = directory.path().join("folds.ron");
            FileStore::write(
                &root,
                &Snapshot {
                    files: [(path.to_string_lossy().into_owned(), lines.to_vec())].into(),
                },
            )
            .unwrap();
            app.add_plugins(Plugin).add_systems(
                Update,
                PendingEditor::finish
                    .after(FoldSet::Restore)
                    .in_set(EditorFileLoadedSet),
            );
            app.world_mut().spawn((
                Root(root),
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

        fn restore(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                self.app.update();
                if self
                    .app
                    .world()
                    .get::<crate::host::fold_preferences::Loaded>(self.ticket.owner)
                    .is_some()
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "fold preferences did not restore"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        fn snapshot(&mut self) -> Snapshot {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                self.app.update();
                let state = self.app.world().get::<State>(self.ticket.owner).unwrap();
                assert!(
                    state.error().is_none(),
                    "fold save failed: {:?}",
                    state.error()
                );
                if !state.is_dirty() && state.saving().is_none() {
                    return FileStore::read(&self.ticket.path).unwrap();
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "fold preferences did not save"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        fn regions(&mut self, index: usize, headers: &[u32]) {
            let regions = headers
                .iter()
                .map(|start| FoldRegion {
                    start: *start,
                    end: start + 1,
                })
                .collect();
            self.app
                .world_mut()
                .get_mut::<Editor>(self.views[index])
                .unwrap()
                .folds
                .set_regions(regions);
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
    fn changing_local_regions_does_not_expand_stored_folds_in_other_views() {
        for project_before_toggle in [false, true] {
            let mut fixture = Fixture::new(&[0]);
            fixture.restore();
            fixture.regions(0, &[2]);
            if project_before_toggle {
                fixture.app.update();
            }
            fixture.toggle(0, 2);
            fixture.app.update();
            assert_eq!(fixture.collapsed(0), std::collections::HashSet::from([2]));
            assert_eq!(
                fixture.collapsed(1),
                std::collections::HashSet::from([0, 2])
            );
            let path = fixture
                .app
                .world()
                .get::<FileView>(fixture.views[0])
                .unwrap()
                .path
                .to_string_lossy()
                .into_owned();
            assert_eq!(fixture.snapshot().files[&path], [0, 2]);
        }
    }

    #[test]
    fn stored_folds_apply_when_their_regions_arrive_without_a_preference_revision() {
        let mut fixture = Fixture::new(&[2]);
        fixture.regions(0, &[0]);
        fixture.restore();
        assert!(fixture.collapsed(0).is_empty());
        assert_eq!(fixture.collapsed(1), std::collections::HashSet::from([2]));
        fixture.regions(0, &[0, 2]);
        fixture.app.update();
        assert_eq!(fixture.collapsed(0), std::collections::HashSet::from([2]));
    }

    #[test]
    fn separate_native_views_share_changes_without_copying_local_fold_regions() {
        let mut fixture = Fixture::new(&[]);
        fixture.restore();
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
        let snapshot = fixture.snapshot();
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
        let mut fixture = Fixture::new(&[2]);
        fixture.toggle(0, 0);
        fixture.restore();
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
        let mut fixture = Fixture::new(&[]);
        fixture.restore();
        fixture.toggle(0, 0);
        fixture
            .app
            .world_mut()
            .get_mut::<FileView>(fixture.views[0])
            .unwrap()
            .path = fixture.directory.path().join("other.rs");
        fixture.app.update();
        let snapshot = fixture.snapshot();
        assert!(snapshot.files.values().all(Vec::is_empty));
        assert!(fixture.collapsed(1).is_empty());
    }

    #[test]
    fn reloading_the_same_file_projects_its_durable_folds_into_the_new_editor() {
        let mut fixture = Fixture::new(&[0, 2]);
        fixture.restore();
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
            .insert(PendingEditor(Some(Editor::new(
                core,
                HighlightCache::plain(&path),
                folds,
            ))));
        fixture.app.update();
        assert_eq!(
            fixture.collapsed(0),
            std::collections::HashSet::from([0, 2])
        );
    }
}
