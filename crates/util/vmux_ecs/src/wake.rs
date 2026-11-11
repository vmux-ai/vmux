use std::sync::Arc;
use std::task::Waker;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};

use crate::JobWake;

pub struct WakePlugin;

impl Plugin for WakePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(First, bind);
    }
}

fn bind(
    proxy: Option<Res<EventLoopProxyWrapper>>,
    current: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    if current.is_some() {
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    if let Some(waker) = Wake::optional_waker(Some((**proxy).clone())) {
        commands.insert_resource(JobWake(waker));
    }
}

#[derive(Default)]
pub struct Wake(Option<EventLoopProxy<WinitUserEvent>>);

impl Wake {
    pub fn waker(proxy: Option<&EventLoopProxyWrapper>) -> Waker {
        Waker::from(Arc::new(Self::beside(proxy)))
    }

    pub fn optional_waker(proxy: Option<EventLoopProxy<WinitUserEvent>>) -> Option<Waker> {
        proxy.map(|proxy| Arc::new(EventLoopWake(proxy)).into())
    }

    pub fn beside(proxy: Option<&EventLoopProxyWrapper>) -> Self {
        Self(proxy.map(|proxy| (**proxy).clone()))
    }

    pub fn now(&self) {
        let Some(proxy) = &self.0 else {
            return;
        };
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

impl std::task::Wake for Wake {
    fn wake(self: Arc<Self>) {
        self.now();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.now();
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        self.now();
    }
}

struct EventLoopWake(EventLoopProxy<WinitUserEvent>);

impl std::task::Wake for EventLoopWake {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send_event(WinitUserEvent::WakeUp);
    }
}
