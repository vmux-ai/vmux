use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_ecs::{
    BlockingCancellation, Job, JobPlugin, JobRequest, JobResult, Operation, OperationRetained,
    OperationState, OperationTarget, PageMetadata, PageOpenRequest, PageOpenTarget,
};

use crate::ToolStore;
use crate::inventory::Registry;
use crate::io::PathChanged;
use crate::presentation::State;
use crate::route::ToolRoute;
use crate::state::ToolOpenRequest;

pub(crate) struct ToolOpenPlugin;

impl Plugin for ToolOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<CreateEmptyFile>::default())
            .add_observer(accept)
            .add_systems(Update, cancel.in_set(crate::ToolOperationCancelSet))
            .add_systems(PostUpdate, finish.after(vmux_ecs::JobSet::Cleanup));
    }
}

#[derive(Component)]
struct Work {
    page: Entity,
    generation: u64,
    registry: Entity,
    store: ToolStore,
    path: PathBuf,
}

fn accept(
    trigger: On<UiInput<ToolOpenRequest>>,
    pages: Query<(&State, &PageMetadata)>,
    stores: Query<(Entity, &ToolStore), With<Registry>>,
    mut requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let page = trigger.event().webview;
    let Ok((state, metadata)) = pages.get(page) else {
        return;
    };
    if ToolRoute::parse(&metadata.url).is_none() {
        return;
    }
    let Ok((registry, store)) = stores.single() else {
        return;
    };
    if !state.bound(registry, store) {
        return;
    }
    let path = PathBuf::from(trigger.event().payload.path.trim());
    let Ok(url) = url::Url::from_file_path(&path) else {
        return;
    };
    if path != store.brewfile_path() {
        requests.write(PageOpenRequest {
            target: PageOpenTarget::NewStack,
            url: url.to_string(),
            request_id: None,
        });
        return;
    }
    commands.spawn((
        Operation::bundle("tool.file.open").unwrap().running(),
        OperationRetained,
        Work {
            page,
            generation: state.generation,
            registry,
            store: store.clone(),
            path: path.clone(),
        },
        JobRequest(CreateEmptyFile { path }),
    ));
}

fn cancel(
    work: Query<(Entity, &Work)>,
    pages: Query<(&State, &PageMetadata)>,
    stores: Query<&ToolStore, With<Registry>>,
    mut commands: Commands,
) {
    for (entity, work) in &work {
        let valid = pages.get(work.page).is_ok_and(|(state, metadata)| {
            state.generation == work.generation
                && state.bound(work.registry, &work.store)
                && ToolRoute::parse(&metadata.url).is_some()
        }) && stores.get(work.registry).ok() == Some(&work.store);
        if !valid {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn finish(
    work: Query<(
        Entity,
        &Work,
        &OperationState,
        Option<&JobResult<CreateEmptyFile>>,
        Has<JobRequest<CreateEmptyFile>>,
    )>,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    pages: Query<(&State, &PageMetadata)>,
    stores: Query<&ToolStore, With<Registry>>,
    mut requests: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for (entity, work, state, result, requested) in &work {
        if requested || captures.iter().any(|target| target.entity() == entity) {
            continue;
        }
        let valid = pages.get(work.page).is_ok_and(|(state, metadata)| {
            state.generation == work.generation
                && state.bound(work.registry, &work.store)
                && ToolRoute::parse(&metadata.url).is_some()
        }) && stores.get(work.registry).ok() == Some(&work.store);
        if *state == OperationState::Running && valid {
            let Some(result) = result else {
                continue;
            };
            if let Ok(Err(error)) | Err(error) = &result.0 {
                bevy::log::warn!("could not create managed Brewfile: {error}");
            }
            if let Ok(url) = url::Url::from_file_path(&work.path) {
                requests.write(PageOpenRequest {
                    target: PageOpenTarget::NewStack,
                    url: url.to_string(),
                    request_id: None,
                });
            }
        }
        commands.entity(entity).despawn();
    }
}

#[derive(Clone)]
struct CreateEmptyFile {
    path: PathBuf,
}

impl Job for CreateEmptyFile {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("file creation canceled".into());
        }
        if !self.path.is_absolute() {
            return Err("file creation requires an absolute path".into());
        }
        let parent = self.path.parent().ok_or("file path has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        if BlockingCancellation::canceled() {
            return Err("file creation canceled".into());
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path)
        {
            Ok(file) => {
                drop(file);
                Ok(PathChanged(true))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(PathChanged(false))
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::tests::Fixture;
    use std::time::{Duration, Instant};

    #[test]
    fn opening_the_managed_brewfile_creates_only_missing_files_and_preserves_foreign_contents() {
        let mut fixture = Fixture::new();
        let page = fixture.page("vmux://tools/homebrew");
        let path = fixture.store.brewfile_path();
        for existing in [false, true] {
            if existing {
                std::fs::write(&path, b"# existing native configuration\n").unwrap();
            }
            fixture.app.world_mut().trigger(UiInput {
                webview: page,
                payload: ToolOpenRequest {
                    path: path.to_string_lossy().into_owned(),
                },
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            let requests = loop {
                fixture.app.update();
                let requests: Vec<_> = fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<PageOpenRequest>>()
                    .drain()
                    .collect();
                if !requests.is_empty() {
                    break requests;
                }
                assert!(
                    Instant::now() < deadline,
                    "native Brewfile open did not complete"
                );
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(requests.len(), 1);
            assert_eq!(
                requests[0].url,
                url::Url::from_file_path(&path).unwrap().to_string()
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                if existing {
                    b"# existing native configuration\n".as_slice()
                } else {
                    b"".as_slice()
                }
            );
            assert!(
                fixture
                    .app
                    .world_mut()
                    .query::<&Work>()
                    .iter(fixture.app.world())
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn retired_pages_and_replaced_stores_refuse_queued_file_creation_and_view_delivery() {
        for interruption in ["page", "store"] {
            let mut fixture = Fixture::new();
            let page = fixture.page("vmux://tools/homebrew");
            let path = fixture.store.brewfile_path();
            fixture.app.world_mut().trigger(UiInput {
                webview: page,
                payload: ToolOpenRequest {
                    path: path.to_string_lossy().into_owned(),
                },
            });
            if interruption == "page" {
                fixture
                    .app
                    .world_mut()
                    .get_mut::<PageMetadata>(page)
                    .unwrap()
                    .url = "https://example.test/".into();
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(fixture.registry)
                    .insert(ToolStore::new(
                        fixture.directory.path().join("replacement"),
                        fixture.store.home(),
                    ));
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                fixture.app.update();
                if fixture
                    .app
                    .world_mut()
                    .query::<&Work>()
                    .iter(fixture.app.world())
                    .next()
                    .is_none()
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "canceled file creation did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!path.exists(), "{interruption}");
            assert!(
                fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<PageOpenRequest>>()
                    .drain()
                    .next()
                    .is_none(),
                "{interruption}"
            );
        }
    }
}
