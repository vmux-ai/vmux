use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use std::sync::Arc;
use std::task::Waker;

#[derive(Default)]
pub struct Wake(Option<EventLoopProxy<WinitUserEvent>>);

impl Wake {
    pub fn waker(proxy: Option<&EventLoopProxyWrapper>) -> Waker {
        Waker::from(Arc::new(Self::beside(proxy)))
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
