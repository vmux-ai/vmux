use std::future::Future;
use std::sync::Arc;
use std::task::{Wake as TaskWake, Waker};

use bevy::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use vmux_ecs::service::Wake;
use vmux_ecs::{TokioTask, TokioTaskOutcome, Uuid};

pub(super) struct RemoteIoPlugin;

impl Plugin for RemoteIoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, collect.in_set(super::remote::RemoteSet::Poll));
    }
}

#[derive(Component, Clone)]
pub(super) struct Loan {
    pub executor: Handle,
    pub wake: Wake,
}

#[derive(Component, Clone)]
pub(super) struct Accepted {
    pub owner: Entity,
    pub uuid: Uuid,
}

#[derive(Component)]
pub(super) struct Joined(pub Result<(), String>);

#[derive(Component)]
pub(super) struct Output<T: Send + Sync + 'static>(oneshot::Receiver<T>);

impl<T: Send + Sync + 'static> Output<T> {
    pub(super) fn spawn_blocking(
        loan: &Loan,
        accepted: Accepted,
        operation: impl FnOnce() -> T + Send + 'static,
    ) -> impl Bundle {
        let (sender, receiver) = oneshot::channel();
        let task = loan.executor.spawn_blocking(move || {
            let result = operation();
            let _ = sender.send(result);
        });
        let notify = Waker::from(Arc::new(Notify(loan.wake.0.clone())));
        (
            Name::new("Remote blocking IO"),
            Self(receiver),
            TokioTask::new(task),
            accepted,
            loan.clone(),
            CompletionWake(notify),
        )
    }

    pub(super) fn spawn(
        loan: &Loan,
        accepted: Accepted,
        future: impl Future<Output = T> + Send + 'static,
    ) -> impl Bundle {
        let (sender, receiver) = oneshot::channel();
        let task = loan.executor.spawn(async move {
            let result = future.await;
            let _ = sender.send(result);
        });
        let notify = Waker::from(Arc::new(Notify(loan.wake.0.clone())));
        (
            Name::new("Remote native IO"),
            Self(receiver),
            TokioTask::new(task),
            accepted,
            loan.clone(),
            CompletionWake(notify),
        )
    }

    pub(super) fn take(&mut self, joined: &Joined) -> Result<T, String> {
        if let Err(error) = &joined.0 {
            return Err(error.clone());
        }
        self.0.try_recv().map_err(|error| error.to_string())
    }
}

#[derive(Component)]
struct CompletionWake(Waker);

struct Notify(mpsc::UnboundedSender<()>);

impl TaskWake for Notify {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

fn collect(
    mut tasks: Query<(Entity, &mut TokioTask, &CompletionWake), Without<Joined>>,
    mut commands: Commands,
) {
    for (entity, mut task, notify) in &mut tasks {
        let Some(outcome) = task.poll_outcome(&notify.0) else {
            continue;
        };
        let result = match outcome {
            TokioTaskOutcome::Completed => Ok(()),
            TokioTaskOutcome::Canceled | TokioTaskOutcome::Panicked => Err(outcome.to_string()),
        };
        commands.entity(entity).insert(Joined(result));
        notify.0.wake_by_ref();
    }
}
