use std::sync::Arc;
use std::task::Waker;

use bevy_app::{App, First, Plugin};
use bevy_ecs::prelude::{Commands, Res, Resource};
use bevy_winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use vmux_ecs::JobWake;

pub struct WinitWakePlugin;

impl Plugin for WinitWakePlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<Installed>() {
            return;
        }
        app.init_resource::<Installed>().add_systems(First, bind);
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[derive(Resource, Default)]
struct Installed;

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
    if let Some(waker) = WinitWake::optional_waker(Some((**proxy).clone())) {
        commands.insert_resource(JobWake(waker));
    }
}

#[derive(Default)]
pub struct WinitWake(Option<EventLoopProxy<WinitUserEvent>>);

impl WinitWake {
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

impl std::task::Wake for WinitWake {
    fn wake(self: Arc<Self>) {
        self.now();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.now();
    }
}

impl Drop for WinitWake {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_is_idempotent() {
        let mut app = App::new();
        app.add_plugins((WinitWakePlugin, WinitWakePlugin));
        app.update();
        assert!(app.world().contains_resource::<Installed>());
        assert!(app.world().get_resource::<JobWake>().is_none());
    }

    #[test]
    fn default_wake_without_proxy_is_safe() {
        let wake = WinitWake::default();
        wake.now();
        drop(wake);
        let beside = WinitWake::beside(None);
        beside.now();
        drop(beside);
        let waker = WinitWake::waker(None);
        waker.wake_by_ref();
        waker.wake();
        assert!(WinitWake::optional_waker(None).is_none());
    }
}
