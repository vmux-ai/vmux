use std::io;
use std::path::{Path, PathBuf};
use std::result::Result;

use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use vmux_ecs::{OperationState, Order};
use vmux_path::PathIdentity;

use crate::graph::KnowledgeNote;
use crate::{KnowledgeGraph, WikiLink};

pub struct KnowledgeRenamePlugin;

impl Plugin for KnowledgeRenamePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::KnowledgeGraphPlugin>() {
            app.add_plugins(crate::KnowledgeGraphPlugin);
        }
        app.add_observer(prepare)
            .add_observer(apply)
            .add_systems(Update, (poll, start, finish).chain());
    }
}

#[derive(EntityEvent)]
pub struct PrepareKnowledgeRename {
    pub entity: Entity,
    pub old: PathBuf,
    pub new: PathBuf,
}

#[derive(EntityEvent)]
pub struct KnowledgeRenamePrepared {
    pub entity: Entity,
}

#[derive(EntityEvent)]
pub struct ApplyKnowledgeRename {
    pub entity: Entity,
    pub old: PathBuf,
    pub new: PathBuf,
}

#[derive(EntityEvent)]
pub struct KnowledgeRenameComplete {
    pub entity: Entity,
    pub result: Result<(), String>,
}

#[derive(Clone)]
struct Replacement {
    start: usize,
    end: usize,
    text: String,
    expected: String,
}

impl Replacement {
    fn for_link(link: &WikiLink, note: &str, source: &str) -> Self {
        let mut text = String::new();
        if link.embed {
            text.push('!');
        }
        text.push_str("[[");
        text.push_str(note);
        if let Some(anchor) = &link.anchor {
            text.push('#');
            text.push_str(anchor);
        }
        if let Some(label) = &link.label {
            text.push('|');
            text.push_str(label);
        }
        text.push_str("]]");
        Self {
            start: link.start,
            end: link.end,
            text,
            expected: source[link.start..link.end].to_string(),
        }
    }
}

#[derive(Component, Clone)]
struct NotePatch {
    source: PathBuf,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    replacements: Vec<Replacement>,
}

impl NotePatch {
    fn apply(&self, source: &Path) -> io::Result<()> {
        let mut text = std::fs::read_to_string(source)?;
        let mut replacements = self.replacements.clone();
        replacements.sort_by_key(|replacement| std::cmp::Reverse(replacement.start));
        for replacement in replacements {
            if text.get(replacement.start..replacement.end) != Some(replacement.expected.as_str()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Knowledge note changed before its link rewrite",
                ));
            }
            text.replace_range(replacement.start..replacement.end, &replacement.text);
        }
        std::fs::write(source, text)
    }
}

#[derive(Component)]
struct RewriteSource(PathBuf);

#[derive(Component)]
struct ApplyingRename;

#[derive(Component)]
struct RewriteFailed(String);

#[derive(Component)]
struct RewriteDone;

#[derive(Component)]
struct RewriteTask(Task<Result<(), String>>);

fn prepare(trigger: On<PrepareKnowledgeRename>, graph: KnowledgeGraph, mut commands: Commands) {
    let request = trigger.event();
    let Some(root) = graph.root(request.entity) else {
        return;
    };
    let old = PathIdentity::resolve(&request.old).into_path_buf();
    let new = PathIdentity::resolve(&request.new).into_path_buf();
    for (entity, source, _, _, parent) in &graph.resolver.notes {
        if parent.parent() != request.entity {
            continue;
        }
        let mut replacements = Vec::new();
        let Ok(children) = graph.children.get(entity) else {
            continue;
        };
        for link_entity in children.iter() {
            let Ok((link, _, order, target)) = graph.links.get(link_entity) else {
                continue;
            };
            let Some(target) = target else { continue };
            let target = target.0;
            let Ok((_, target, _, _, _)) = graph.resolver.notes.get(target) else {
                continue;
            };
            if !target.path.starts_with(&old) {
                continue;
            }
            if link.link.note.is_empty() && source.path.starts_with(&old) {
                continue;
            }
            let suffix = target.path.strip_prefix(&old).unwrap_or(Path::new(""));
            let destination = new.join(suffix);
            let note = KnowledgeNote::relative(root, &destination);
            replacements.push((
                order.0,
                Replacement::for_link(&link.link, &note, &source.text),
            ));
        }
        replacements.sort_by_key(|(order, _)| *order);
        if !replacements.is_empty() {
            let replacements = replacements
                .into_iter()
                .map(|(_, replacement)| replacement)
                .collect();
            commands.entity(entity).insert(NotePatch {
                source: source.path.clone(),
                replacements,
            });
        }
    }
    commands.trigger(KnowledgeRenamePrepared {
        entity: request.entity,
    });
}

fn apply(
    trigger: On<ApplyKnowledgeRename>,
    patches: Query<(Entity, &NotePatch, &ChildOf)>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let old = PathIdentity::resolve(&request.old).into_path_buf();
    let new = PathIdentity::resolve(&request.new).into_path_buf();
    for (entity, patch, parent) in &patches {
        if parent.parent() != request.entity {
            continue;
        }
        let source = match patch.source.strip_prefix(&old) {
            Ok(suffix) => new.join(suffix),
            Err(_) => patch.source.clone(),
        };
        commands.entity(entity).insert(RewriteSource(source));
    }
    commands.entity(request.entity).insert(ApplyingRename);
}

fn poll(mut tasks: Query<(Entity, &mut RewriteTask, &ChildOf)>, mut commands: Commands) {
    for (entity, mut task, owner) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<RewriteTask>()
            .insert(RewriteDone);
        if let Err(error) = result {
            commands.entity(owner.parent()).insert(RewriteFailed(error));
        }
    }
}

type WritableScopes<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static OperationState>),
    (With<ApplyingRename>, Without<RewriteFailed>),
>;
type PendingPatches<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static NotePatch,
        &'static RewriteSource,
        &'static Order,
        &'static ChildOf,
    ),
    (Without<RewriteDone>, Without<RewriteTask>),
>;

fn start(
    scopes: WritableScopes,
    tasks: Query<&ChildOf, With<RewriteTask>>,
    patches: PendingPatches,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (scope, state) in &scopes {
        if tasks.iter().any(|parent| parent.parent() == scope) {
            continue;
        }
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            commands
                .entity(scope)
                .insert(RewriteFailed("Knowledge rename canceled".to_string()));
            continue;
        }
        let mut next = None;
        for (entity, patch, source, order, parent) in &patches {
            if parent.parent() == scope && next.is_none_or(|(_, _, _, previous)| order.0 < previous)
            {
                next = Some((entity, patch, source, order.0));
            }
        }
        let Some((entity, patch, source, _)) = next else {
            continue;
        };
        let patch = patch.clone();
        let source = source.0.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = patch.apply(&source).map_err(|error| error.to_string());
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).insert(RewriteTask(task));
    }
}

fn finish(
    scopes: Query<(Entity, Option<&RewriteFailed>), With<ApplyingRename>>,
    tasks: Query<&ChildOf, With<RewriteTask>>,
    patches: Query<&ChildOf, (With<NotePatch>, Without<RewriteDone>)>,
    mut commands: Commands,
) {
    for (entity, failure) in &scopes {
        if tasks.iter().any(|parent| parent.parent() == entity) {
            continue;
        }
        if failure.is_none() && patches.iter().any(|parent| parent.parent() == entity) {
            continue;
        }
        let result = match failure {
            Some(error) => Err(error.0.clone()),
            None => Ok(()),
        };
        commands.entity(entity).remove::<ApplyingRename>();
        commands.trigger(KnowledgeRenameComplete { entity, result });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KnowledgeIndex, MaterializeKnowledge, RawSnapshot};
    use std::time::{Duration, Instant};

    #[derive(Resource, Default)]
    struct Completed(Option<Result<(), String>>);

    impl Completed {
        fn capture(trigger: On<KnowledgeRenameComplete>, mut completed: ResMut<Self>) {
            completed.0 = Some(trigger.result.clone());
        }
    }

    struct RenameFixture {
        directory: tempfile::TempDir,
        app: App,
        owner: Entity,
        old: PathBuf,
        new: PathBuf,
    }

    impl RenameFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(directory.path().join("projects")).unwrap();
            let root = directory.path().canonicalize().unwrap();
            let old = root.join("projects/alpha.md");
            let new = root.join("projects/renamed.md");
            std::fs::write(
                &old,
                "---\ntitle: Alpha\n---\n# Overview\nSelf [[#Overview]]\n",
            )
            .unwrap();
            std::fs::write(
                root.join("projects/source-a.md"),
                "See [[Alpha#Overview|the overview]] and ![[Alpha#Overview|embed]].\n",
            )
            .unwrap();
            std::fs::write(
                root.join("projects/source-b.md"),
                "Other [[Alpha]] and [[Missing Note]].\n",
            )
            .unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(KnowledgeRenamePlugin)
                .init_resource::<Completed>()
                .add_observer(Completed::capture);
            let owner = app.world_mut().spawn(KnowledgeIndex::default()).id();
            app.world_mut().flush();
            app.world_mut().trigger(MaterializeKnowledge {
                entity: owner,
                snapshot: RawSnapshot::capture(&root).unwrap(),
            });
            app.world_mut().flush();
            app.world_mut().trigger(PrepareKnowledgeRename {
                entity: owner,
                old: old.clone(),
                new: new.clone(),
            });
            app.world_mut().flush();
            Self {
                directory,
                app,
                owner,
                old,
                new,
            }
        }

        fn apply(&mut self) {
            std::fs::rename(&self.old, &self.new).unwrap();
            self.app.world_mut().trigger(ApplyKnowledgeRename {
                entity: self.owner,
                old: self.old.clone(),
                new: self.new.clone(),
            });
            self.app.world_mut().flush();
        }

        fn tick(&mut self) {
            self.app.update();
        }

        fn finish(&mut self) -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.app.world().resource::<Completed>().0.is_none() && Instant::now() < deadline
            {
                self.tick();
                std::thread::yield_now();
            }
            assert_eq!(
                self.app
                    .world_mut()
                    .query::<&RewriteTask>()
                    .iter(self.app.world())
                    .count(),
                0
            );
            self.app
                .world_mut()
                .resource_mut::<Completed>()
                .0
                .take()
                .expect("rename IO must drain and report completion")
        }
    }

    #[test]
    fn rename_rewrites_resolved_links_and_preserves_labels_embeds_and_self_anchors() {
        let mut fixture = RenameFixture::new();
        fixture.apply();
        fixture.finish().unwrap();
        let first =
            std::fs::read_to_string(fixture.directory.path().join("projects/source-a.md")).unwrap();
        assert!(first.contains("[[projects/renamed#Overview|the overview]]"));
        assert!(first.contains("![[projects/renamed#Overview|embed]]"));
        let second =
            std::fs::read_to_string(fixture.directory.path().join("projects/source-b.md")).unwrap();
        assert!(second.contains("[[projects/renamed]]"));
        assert!(second.contains("[[Missing Note]]"));
        assert!(
            std::fs::read_to_string(&fixture.new)
                .unwrap()
                .contains("[[#Overview]]")
        );
    }

    #[test]
    fn directory_rename_relocates_each_source_file_before_rewriting_its_links() {
        let mut fixture = RenameFixture::new();
        fixture.old = fixture.directory.path().join("projects");
        fixture.new = fixture.directory.path().join("renamed");
        fixture.app.world_mut().trigger(PrepareKnowledgeRename {
            entity: fixture.owner,
            old: fixture.old.clone(),
            new: fixture.new.clone(),
        });
        fixture.app.world_mut().flush();
        fixture.apply();
        fixture.finish().unwrap();
        let first = std::fs::read_to_string(fixture.new.join("source-a.md")).unwrap();
        assert!(first.contains("[[renamed/alpha#Overview|the overview]]"));
        let second = std::fs::read_to_string(fixture.new.join("source-b.md")).unwrap();
        assert!(second.contains("[[renamed/alpha]]"));
        assert!(
            std::fs::read_to_string(fixture.new.join("alpha.md"))
                .unwrap()
                .contains("[[#Overview]]")
        );
    }

    #[test]
    fn changed_link_bytes_are_refused_without_overwriting_the_newer_source() {
        let mut fixture = RenameFixture::new();
        fixture.apply();
        let source = fixture.directory.path().join("projects/source-a.md");
        let current =
            "Changed See [[Alpha#Overview|the overview]] and ![[Alpha#Overview|embed]].\n";
        std::fs::write(&source, current).unwrap();
        assert!(fixture.finish().is_err());
        assert_eq!(std::fs::read_to_string(source).unwrap(), current);
    }

    #[test]
    fn a_failed_source_stops_later_writes_in_the_existing_source_order() {
        let mut fixture = RenameFixture::new();
        fixture.apply();
        std::fs::remove_file(fixture.directory.path().join("projects/source-a.md")).unwrap();
        assert!(fixture.finish().is_err());
        let second =
            std::fs::read_to_string(fixture.directory.path().join("projects/source-b.md")).unwrap();
        assert_eq!(second, "Other [[Alpha]] and [[Missing Note]].\n");
    }

    #[test]
    fn cancellation_keeps_an_active_write_owned_until_drain_and_refuses_future_writes() {
        let mut fixture = RenameFixture::new();
        fixture.apply();
        let first = fixture
            .app
            .world_mut()
            .query::<(Entity, &KnowledgeNote)>()
            .iter(fixture.app.world())
            .find(|(_, note)| note.path.ends_with("source-a.md"))
            .map(|(entity, _)| entity)
            .unwrap();
        let (release, resume) = std::sync::mpsc::channel();
        let (started, ready) = std::sync::mpsc::channel();
        let task = IoTaskPool::get().spawn(async move {
            started.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(())
        });
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(first)
            .insert(RewriteTask(task));
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(OperationState::Canceled);
        fixture.tick();
        assert!(fixture.app.world().get::<RewriteTask>(first).is_some());
        assert!(fixture.app.world().resource::<Completed>().0.is_none());
        release.send(()).unwrap();
        assert!(fixture.finish().is_err());
        let second =
            std::fs::read_to_string(fixture.directory.path().join("projects/source-b.md")).unwrap();
        assert_eq!(second, "Other [[Alpha]] and [[Missing Note]].\n");
    }
}
