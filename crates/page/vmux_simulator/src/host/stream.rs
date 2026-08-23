pub mod mjpeg;
pub mod source;

use super::device::{Axe, SimulatorDevice};
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use source::{AxeStream, LatestFrame, WakeFn};
use std::sync::Arc;

/// Owns the `axe stream-video` child and hands decoded frames to the mirror.
pub struct StreamPlugin;

impl Plugin for StreamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LatestFrame>()
            .add_systems(Startup, Self::start);
    }
}

impl StreamPlugin {
    fn start(
        mut commands: Commands,
        slot: Res<LatestFrame>,
        proxy: Option<Res<EventLoopProxyWrapper>>,
    ) {
        let Some(version) = Axe::version() else {
            error!(
                "`{}` not found on PATH — install it with `brew install cameroncooke/axe/axe`",
                Axe::BIN
            );
            return;
        };
        let Some(device) = SimulatorDevice::booted() else {
            error!("no booted simulator — boot one, then restart");
            return;
        };
        info!("axe {version}, mirroring {} ({})", device.name, device.udid);

        // Frames arrive off a reader thread while the loop sits in `Reactive`; without a wake
        // the mirror would only advance on cursor movement.
        let wake: Option<WakeFn> = proxy.map(|p| {
            let proxy = (**p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as WakeFn
        });

        let Some(stream) = AxeStream::start(&device, slot.clone(), wake) else {
            error!("failed to start `{} stream-video`", Axe::BIN);
            return;
        };
        commands.insert_resource(stream);
        commands.insert_resource(device);
    }
}
