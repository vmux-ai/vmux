use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake as TaskWake, Waker};

use bevy::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use vmux_ecs::service::{AbortTask, Wake};
use vmux_ecs::{OperationTarget, Uuid};

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
    pub(super) fn new_blocking(
        loan: &Loan,
        target: Option<Entity>,
        accepted: Accepted,
        operation: impl FnOnce() -> T + Send + 'static,
    ) -> (
        (Name, Self, AbortTask, Accepted, Loan, CompletionWake),
        Option<OperationTarget>,
    ) {
        let (sender, receiver) = oneshot::channel();
        let task = loan.executor.spawn_blocking(move || {
            let result = operation();
            let _ = sender.send(result);
        });
        let notify = Waker::from(Arc::new(Notify(loan.wake.0.clone())));
        (
            (
                Name::new("Remote blocking IO"),
                Self(receiver),
                AbortTask(task),
                accepted,
                loan.clone(),
                CompletionWake(notify),
            ),
            target.map(OperationTarget::new),
        )
    }

    pub(super) fn new(
        loan: &Loan,
        target: Option<Entity>,
        accepted: Accepted,
        future: impl Future<Output = T> + Send + 'static,
    ) -> (
        (Name, Self, AbortTask, Accepted, Loan, CompletionWake),
        Option<OperationTarget>,
    ) {
        let (sender, receiver) = oneshot::channel();
        let task = loan.executor.spawn(async move {
            let result = future.await;
            let _ = sender.send(result);
        });
        let notify = Waker::from(Arc::new(Notify(loan.wake.0.clone())));
        (
            (
                Name::new("Remote native IO"),
                Self(receiver),
                AbortTask(task),
                accepted,
                loan.clone(),
                CompletionWake(notify),
            ),
            target.map(OperationTarget::new),
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
pub(super) struct CompletionWake(Waker);

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
    mut tasks: Query<(Entity, &mut AbortTask, &CompletionWake), Without<Joined>>,
    mut commands: Commands,
) {
    for (entity, mut task, notify) in &mut tasks {
        let mut context = Context::from_waker(&notify.0);
        let Poll::Ready(result) = Pin::new(&mut task.0).poll(&mut context) else {
            continue;
        };
        commands
            .entity(entity)
            .insert(Joined(result.map_err(|error| error.to_string())));
        notify.0.wake_by_ref();
    }
}
