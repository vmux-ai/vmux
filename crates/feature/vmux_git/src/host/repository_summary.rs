use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::runtime::{GitRuntime, GitTask};
use crate::event::{
    GitRepositorySnapshot, RepositorySummary, RepositorySummaryRequest, RepositorySummaryResult,
    RepositorySummaryUiState,
};

pub(super) struct SummaryPlugin;

impl Plugin for SummaryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .add_plugins(vmux_ecs::UiStatePlugin::<RepositorySummaryUiState>::default())
            .add_plugins(UiEventPlugin::<(RepositorySummaryRequest,)>::default())
            .add_observer(subscribe)
            .add_systems(
                Update,
                (start, bevy::ecs::schedule::ApplyDeferred, finish, publish).chain(),
            );
    }
}

#[derive(Component)]
struct Subscription(Vec<String>);

#[derive(Component, Default)]
struct PublishedSummary(Vec<RepositorySummaryResult>);

#[derive(Component)]
struct SummaryRoot(String);

#[derive(Component)]
struct SummaryState {
    result: RepositorySummaryResult,
    checked: Option<Instant>,
}

#[derive(Component)]
struct SummaryTask(GitTask<Result<RepositorySummary, String>>);

impl SummaryState {
    fn new(root: String, retained: Option<RepositorySummaryResult>) -> Self {
        Self {
            result: retained.unwrap_or(RepositorySummaryResult {
                root,
                loading: true,
                ..Default::default()
            }),
            checked: None,
        }
    }
}

impl RepositorySummary {
    fn read(root: String) -> Result<Self, String> {
        let snapshot = GitRepositorySnapshot::summary(Path::new(&root)).map_err(|error| error.0)?;
        Ok(Self {
            root: snapshot.repo_root,
            name: snapshot.repo_name,
            branch: snapshot.branch,
            changed_files: snapshot.files.len() as u32,
            ahead: snapshot.ahead,
            behind: snapshot.behind,
        })
    }
}

fn subscribe(event: On<UiInput<RepositorySummaryRequest>>, mut commands: Commands) {
    let mut roots = Vec::new();
    for root in &event.payload.roots {
        if root.is_empty() || roots.contains(root) {
            continue;
        }
        roots.push(root.clone());
    }
    commands.entity(event.webview).insert(Subscription(roots));
}

#[allow(clippy::type_complexity)]
fn start(
    subscriptions: Query<
        (Entity, &Subscription, Option<&PublishedSummary>),
        Or<(
            With<vmux_ecs::PagePresented>,
            With<vmux_ecs::PageTransition>,
        )>,
    >,
    entries: Query<(Entity, &SummaryRoot, &SummaryState, Has<SummaryTask>)>,
    runtime: Res<GitRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let now = Instant::now();
    let mut roots = HashSet::new();
    let mut retained = HashMap::new();
    for (_, subscription, state) in &subscriptions {
        roots.extend(subscription.0.iter().cloned());
        if let Some(state) = state {
            for result in &state.0 {
                if subscription.0.contains(&result.root) && !result.loading {
                    retained
                        .entry(result.root.clone())
                        .or_insert_with(|| result.clone());
                }
            }
        }
    }
    let mut existing = HashSet::new();
    for (entity, root, state, running) in &entries {
        if !roots.contains(&root.0) {
            commands.entity(entity).despawn();
            continue;
        }
        existing.insert(root.0.clone());
        if running
            || state
                .checked
                .is_some_and(|checked| now.duration_since(checked) < Duration::from_secs(5))
        {
            continue;
        }
        let path = root.0.clone();
        if let Some(task) = runtime
            .spawn_background(proxy.as_deref().map(|proxy| (**proxy).clone()), move || {
                RepositorySummary::read(path)
            })
        {
            commands.entity(entity).insert(SummaryTask(task));
        }
    }
    for root in roots {
        if existing.contains(&root) {
            continue;
        }
        let state = SummaryState::new(root.clone(), retained.remove(&root));
        let path = root.clone();
        let mut entity = commands.spawn((
            Name::new("git.repository.summary"),
            SummaryRoot(root),
            state,
        ));
        if let Some(task) = runtime
            .spawn_background(proxy.as_deref().map(|proxy| (**proxy).clone()), move || {
                RepositorySummary::read(path)
            })
        {
            entity.insert(SummaryTask(task));
        }
    }
}

fn finish(
    mut entries: Query<(Entity, &mut SummaryState, &mut SummaryTask)>,
    mut commands: Commands,
) {
    for (entity, mut state, mut task) in &mut entries {
        let Some(result) = task.0.poll() else {
            continue;
        };
        commands.entity(entity).remove::<SummaryTask>();
        state.checked = Some(Instant::now());
        state.result.loading = false;
        match result {
            Ok(Ok(summary)) => {
                state.result.summary = Some(summary);
                state.result.error.clear();
            }
            Ok(Err(error)) => {
                state.result.error = error;
            }
            Err(_) => {
                state.result.error = "repository summary worker failed".to_string();
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn publish(
    subscriptions: Query<
        (Entity, &Subscription, Option<&PublishedSummary>),
        Or<(
            With<vmux_ecs::PagePresented>,
            With<vmux_ecs::PageTransition>,
        )>,
    >,
    entries: Query<(&SummaryRoot, &SummaryState)>,
    mut commands: Commands,
) {
    let results = entries
        .iter()
        .map(|(root, state)| (root.0.as_str(), &state.result))
        .collect::<HashMap<_, _>>();
    for (owner, subscription, current) in &subscriptions {
        let mut repositories = Vec::new();
        for root in &subscription.0 {
            if let Some(result) = results.get(root.as_str()) {
                repositories.push((*result).clone());
            }
        }
        if current.is_some_and(|published| published.0 == repositories) {
            continue;
        }
        let state = RepositorySummaryUiState {
            repositories: repositories.clone(),
        };
        commands
            .trigger(vmux_ecs::UiStateWrite::<RepositorySummaryUiState>::from_event(owner, &state));
        commands
            .entity(owner)
            .insert(PublishedSummary(repositories));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::repository::test_repo;

    struct SummaryPages(App);

    impl SummaryPages {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, SummaryPlugin))
                .insert_non_send(bevy_cef::prelude::Browsers::default())
                .add_observer(Self::deliver);
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: "vmux://projects/",
                owns_subtree: true,
                permissions: &["RepositorySummaryUiState"],
            });
            Self(app)
        }

        fn deliver(event: On<bevy_cef::prelude::BinHostEmitEvent>) {
            event.mark_delivered();
        }

        fn request(&mut self, root: &Path) -> Entity {
            let owner = self
                .0
                .world_mut()
                .spawn((
                    vmux_ecs::PageMetadata {
                        url: "vmux://projects/example".into(),
                        ..Default::default()
                    },
                    vmux_ecs::PagePresented,
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                ))
                .id();
            self.0
                .world_mut()
                .non_send_mut::<bevy_cef::prelude::Browsers>()
                .set_externally_hosted(owner);
            self.0.world_mut().trigger(UiInput {
                webview: owner,
                payload: RepositorySummaryRequest {
                    roots: vec![root.to_string_lossy().into_owned()],
                },
            });
            self.0.world_mut().flush();
            owner
        }

        fn finish(&mut self, owner: Entity) -> RepositorySummaryResult {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.0.update();
                if let Some(state) = self
                    .0
                    .world()
                    .get::<vmux_ecs::UiState<RepositorySummaryUiState>>(owner)
                    .and_then(vmux_ecs::UiState::current)
                    && let Some(result) = state.repositories.first()
                    && !result.loading
                {
                    return result.clone();
                }
                assert!(
                    Instant::now() < deadline,
                    "repository summary never settled"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn project_pages_share_requested_repository_summaries_and_non_repositories_settle() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "tracked.txt", "first\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "initial"]);
        test_repo::write(repo.path(), "tracked.txt", "changed\n");
        let mut pages = SummaryPages::new();
        let first = pages.request(repo.path());
        let second = pages.request(repo.path());
        let result = pages.finish(first);
        let shared = pages.finish(second);
        assert_eq!(result, shared);
        let summary = result.summary.unwrap();
        assert_eq!(summary.branch, "main");
        assert_eq!(summary.changed_files, 1);
        assert_eq!(
            pages
                .0
                .world_mut()
                .query::<&SummaryRoot>()
                .iter(pages.0.world())
                .count(),
            1
        );
        let empty = tempfile::tempdir().unwrap();
        let non_repository = pages.request(empty.path());
        let result = pages.finish(non_repository);
        assert!(result.summary.is_none());
        assert!(!result.error.is_empty());
        pages
            .0
            .world_mut()
            .entity_mut(first)
            .remove::<vmux_ecs::PagePresented>();
        pages
            .0
            .world_mut()
            .entity_mut(second)
            .remove::<vmux_ecs::PagePresented>();
        pages.0.update();
        let expected_root = repo.path().to_string_lossy().into_owned();
        assert!(
            !pages
                .0
                .world_mut()
                .query::<&SummaryRoot>()
                .iter(pages.0.world())
                .any(|root| root.0 == expected_root)
        );
        pages
            .0
            .world_mut()
            .entity_mut(first)
            .insert(vmux_ecs::PagePresented);
        pages.0.update();
        let retained = pages
            .0
            .world()
            .get::<vmux_ecs::UiState<RepositorySummaryUiState>>(first)
            .and_then(vmux_ecs::UiState::current)
            .unwrap();
        assert!(!retained.repositories[0].loading);
        assert_eq!(
            retained.repositories[0]
                .summary
                .as_ref()
                .unwrap()
                .changed_files,
            1
        );
        let result = pages.finish(first);
        assert_eq!(result.summary.unwrap().changed_files, 1);
    }
}
