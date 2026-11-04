use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_api::command_bar::{StartBranchesRequest, StartProjectBranches, UiCommandBar};
use vmux_api::space::ProjectBranch;
use vmux_git::{GitRuntime, GitTask};

pub(crate) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .add_plugins(UiEventPlugin::<(StartBranchesRequest,)>::default())
            .add_observer(request)
            .add_systems(Update, (read, finish).chain());
    }
}

#[derive(Component)]
struct Requested {
    webview: Entity,
    project: String,
}

#[derive(Component)]
struct Read {
    subscribers: Vec<Entity>,
    project: String,
    task: GitTask<Vec<ProjectBranch>>,
}

impl Read {
    fn subscribe(&mut self, webview: Entity) {
        if !self.subscribers.contains(&webview) {
            self.subscribers.push(webview);
        }
    }
}

fn request(trigger: On<UiInput<StartBranchesRequest>>, mut commands: Commands) {
    commands.spawn(Requested {
        webview: trigger.event().webview,
        project: trigger.event().payload.project.clone(),
    });
}

fn read(
    requests: Query<(Entity, &Requested), Without<Read>>,
    mut reads: Query<&mut Read>,
    runtime: Res<GitRuntime>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut grouped: HashMap<String, Vec<(Entity, Entity)>> = HashMap::new();
    for (entity, request) in &requests {
        let project = request.project.trim().to_string();
        if project.is_empty() {
            commands.entity(entity).despawn();
            continue;
        }
        grouped
            .entry(project)
            .or_default()
            .push((entity, request.webview));
    }
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    for (project, requests) in grouped {
        let mut joined = false;
        for mut pending in &mut reads {
            if pending.project != project {
                continue;
            }
            for (entity, webview) in &requests {
                pending.subscribe(*webview);
                commands.entity(*entity).despawn();
            }
            joined = true;
            break;
        }
        if joined {
            continue;
        }
        let root = PathBuf::from(&project);
        let Some(task) =
            runtime.spawn_background(wake.clone(), move || Catalog { root: &root }.read())
        else {
            continue;
        };
        let entity = requests[0].0;
        let mut pending = Read {
            subscribers: Vec::new(),
            project,
            task,
        };
        for (request, webview) in requests {
            pending.subscribe(webview);
            if request != entity {
                commands.entity(request).despawn();
            }
        }
        commands
            .entity(entity)
            .remove::<Requested>()
            .insert(pending);
    }
}

fn finish(
    mut reads: Query<(Entity, &mut Read)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut read) in &mut reads {
        let Some(result) = read.task.poll() else {
            continue;
        };
        commands.entity(entity).despawn();
        let branches = match result {
            Ok(branches) => branches,
            Err(_) => {
                warn!("Start branch lookup failed for {}", read.project);
                Vec::new()
            }
        };
        for webview in &read.subscribers {
            if !browsers.can_emit_to(webview) {
                continue;
            }
            commands.trigger(vmux_ecs::UiStateWrite::<UiCommandBar>::from_event(
                *webview,
                &StartProjectBranches {
                    project: read.project.clone(),
                    branches: branches.clone(),
                },
            ));
        }
    }
}

struct Catalog<'a> {
    root: &'a Path,
}

impl Catalog<'_> {
    fn read(&self) -> Vec<ProjectBranch> {
        let mut branches = Vec::new();
        let Ok(checkout) = vmux_git::worktree::CheckoutInfo::try_from(self.root) else {
            return branches;
        };
        let Ok(holders) = checkout.branch_holders() else {
            return branches;
        };
        branches.reserve(holders.len());
        for holder in holders {
            let checkout = holder.checkout_path();
            let label = holder.checkout_label();
            branches.push(ProjectBranch {
                branch: holder.branch,
                checkout,
                label,
                insertions: holder.change.insertions,
                deletions: holder.change.deletions,
            });
        }
        branches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct BlockedWorkers {
        releases: Vec<mpsc::Sender<()>>,
        _tasks: Vec<GitTask<()>>,
    }

    impl BlockedWorkers {
        fn new(runtime: &GitRuntime) -> Self {
            let mut releases = Vec::new();
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let (release, resume) = mpsc::channel();
                let (started, ready) = mpsc::channel();
                tasks.push(
                    runtime
                        .spawn_background(None, move || {
                            started.send(()).unwrap();
                            resume.recv_timeout(Duration::from_secs(10)).unwrap();
                        })
                        .unwrap(),
                );
                ready.recv_timeout(Duration::from_secs(2)).unwrap();
                releases.push(release);
            }
            Self {
                releases,
                _tasks: tasks,
            }
        }
    }

    impl Drop for BlockedWorkers {
        fn drop(&mut self) {
            for release in &self.releases {
                let _ = release.send(());
            }
        }
    }

    #[test]
    fn repeated_requests_share_one_pending_read_per_project_and_keep_all_subscribers() {
        let runtime = GitRuntime::default();
        let blocked = BlockedWorkers::new(&runtime);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(runtime)
            .insert_non_send(Browsers::default())
            .add_plugins(Plugin);
        let first = app.world_mut().spawn_empty().id();
        let second = app.world_mut().spawn_empty().id();
        let third = app.world_mut().spawn_empty().id();
        for (webview, project) in [
            (first, "/project-a"),
            (first, "/project-a"),
            (second, " /project-a "),
            (third, "/project-b"),
        ] {
            app.world_mut().trigger(UiInput {
                webview,
                payload: StartBranchesRequest {
                    project: project.to_string(),
                },
            });
        }
        app.update();

        let mut reads = app.world_mut().query::<&Read>();
        assert_eq!(reads.iter(app.world()).count(), 2);
        let pending = reads
            .iter(app.world())
            .find(|read| read.project == "/project-a")
            .unwrap();
        assert_eq!(pending.subscribers, [first, second]);
        assert_eq!(
            app.world_mut()
                .query::<&Requested>()
                .iter(app.world())
                .count(),
            0
        );

        app.world_mut().trigger(UiInput {
            webview: third,
            payload: StartBranchesRequest {
                project: "/project-a".to_string(),
            },
        });
        app.update();

        let mut reads = app.world_mut().query::<&Read>();
        assert_eq!(reads.iter(app.world()).count(), 2);
        let pending = reads
            .iter(app.world())
            .find(|read| read.project == "/project-a")
            .unwrap();
        assert_eq!(pending.subscribers, [first, second, third]);
        drop(app);
        drop(blocked);
    }
}
