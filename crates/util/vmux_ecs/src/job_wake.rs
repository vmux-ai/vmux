use std::task::Waker;

use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use vmux_task::TaskWake;

use crate::channel::ChannelWake;

#[derive(Resource, Clone)]
#[component(on_insert = sync_wake, on_remove = clear_task_wake)]
pub struct JobWake(pub Waker);

fn sync_wake(mut world: DeferredWorld, context: HookContext) {
    let Some(wake) = world.get::<JobWake>(context.entity) else {
        return;
    };
    let waker = wake.0.clone();
    world.commands().insert_resource(ChannelWake(waker.clone()));
    world.commands().insert_resource(TaskWake(waker.clone()));
    world.commands().insert_resource(vmux_time::TimeWake(waker));
}

fn clear_task_wake(mut world: DeferredWorld, _context: HookContext) {
    world.commands().remove_resource::<TaskWake>();
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::sync::{Arc, mpsc};

    use bevy_app::App;

    use super::*;

    struct ChannelWakeProbe {
        output: mpsc::SyncSender<()>,
        source: JobWake,
    }

    impl std::task::Wake for ChannelWakeProbe {
        fn wake(self: Arc<Self>) {
            self.source.0.wake_by_ref();
            let _ = self.output.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.source.0.wake_by_ref();
            let _ = self.output.try_send(());
        }
    }

    #[test]
    fn inserting_job_wake_synchronizes_channel_wake() {
        let mut app = App::new();
        let (wake_tx, wake_rx) = mpsc::sync_channel(4);
        app.insert_resource(JobWake(Waker::from(Arc::new(ChannelWakeProbe {
            output: wake_tx,
            source: JobWake(Waker::noop().clone()),
        }))));
        let channel_wake = app.world().resource::<ChannelWake>().clone();
        channel_wake.0.wake_by_ref();
        assert!(wake_rx.try_recv().is_ok());
    }

    #[test]
    fn job_wake_hooks_sync_and_clear_task_wake_resource() {
        let mut app = App::new();
        let (first_tx, first_rx) = mpsc::sync_channel(4);
        app.insert_resource(JobWake(Waker::from(Arc::new(ChannelWakeProbe {
            output: first_tx,
            source: JobWake(Waker::noop().clone()),
        }))));
        app.world().resource::<TaskWake>().wake();
        assert!(first_rx.try_recv().is_ok());

        let (second_tx, second_rx) = mpsc::sync_channel(4);
        app.insert_resource(JobWake(Waker::from(Arc::new(ChannelWakeProbe {
            output: second_tx,
            source: JobWake(Waker::noop().clone()),
        }))));
        app.world().resource::<TaskWake>().wake();
        assert!(second_rx.try_recv().is_ok());
        assert!(first_rx.try_recv().is_err());

        app.world_mut().remove_resource::<JobWake>();
        assert!(app.world().get_resource::<TaskWake>().is_none());
    }
}
