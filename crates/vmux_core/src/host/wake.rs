use bevy::prelude::*;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};

#[derive(Default)]
pub struct Wake(Option<EventLoopProxy<WinitUserEvent>>);

impl Wake {
    pub fn from_resource(proxy: Option<Res<EventLoopProxyWrapper>>) -> Self {
        Self(proxy.map(|proxy| (**proxy).clone()))
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

impl Drop for Wake {
    fn drop(&mut self) {
        self.now();
    }
}
