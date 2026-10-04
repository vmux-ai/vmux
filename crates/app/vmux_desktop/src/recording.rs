use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use vmux_input::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
};
use vmux_setting::AppSettings;

use crate::capture_output::{CaptureOutput, CaptureSource};

pub(crate) struct RecordingPlugin;

impl Plugin for RecordingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RecordingControl>()
            .add_systems(Startup, spawn_runtime)
            .add_systems(
                Update,
                (start, control, auto_stop_recordings, drain_recordings)
                    .chain()
                    .after(vmux_command::WriteCommandRequests),
            );
    }
}

fn spawn_runtime(mut commands: Commands) {
    commands.spawn((
        Name::new("Recording capture"),
        RecordingBridge::default(),
        RecordingStatus::default(),
    ));
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_FPS: u32 = 12;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_MAX_EDGE: u32 = 800;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PERMISSION_MSG: &str = "Screen Recording permission required - grant it in System Settings > \
Privacy & Security > Screen Recording, then call record_start again.";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct RecordOutcome {
    pub request_id: Option<[u8; 16]>,
    pub result: Result<RecordingInfo, String>,
}

#[derive(Component)]
struct RecordingBridge {
    pub(crate) tx: Sender<RecordOutcome>,
    rx: Receiver<RecordOutcome>,
    capture: capture::CaptureRuntime,
}

impl Default for RecordingBridge {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx,
            rx,
            capture: capture::CaptureRuntime::default(),
        }
    }
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RecordingStatus {
    #[default]
    Idle,
    Recording,
    Paused,
}

#[derive(Message, Clone, Copy, Debug)]
pub(crate) enum RecordingControl {
    Pause,
    Resume,
    Done,
}

fn control(
    _non_send: NonSendMarker,
    mut reader: MessageReader<RecordingControl>,
    mut runtime: Query<(&mut RecordingBridge, &mut RecordingStatus)>,
) {
    let Ok((mut bridge, mut status)) = runtime.single_mut() else {
        return;
    };
    for ctrl in reader.read() {
        match ctrl {
            RecordingControl::Pause => {
                bridge.capture.pause();
                *status = RecordingStatus::Paused;
            }
            RecordingControl::Resume => {
                bridge.capture.resume();
                *status = RecordingStatus::Recording;
            }
            RecordingControl::Done => {
                bridge.capture.done();
            }
        }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct GifSampling {
    elapsed_ms: u64,
    last_sampled_ms: Option<u64>,
    fps: u32,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl GifSampling {
    pub(crate) fn new(elapsed_ms: u64, last_sampled_ms: Option<u64>, fps: u32) -> Self {
        Self {
            elapsed_ms,
            last_sampled_ms,
            fps,
        }
    }

    pub(crate) fn should_sample(&self) -> bool {
        let interval = (1000 / self.fps.max(1)) as u64;
        match self.last_sampled_ms {
            None => true,
            Some(last) => self.elapsed_ms.saturating_sub(last) >= interval,
        }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct BgraFrame(Vec<u8>);

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl BgraFrame {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub(crate) fn rgba(&self) -> Vec<u8> {
        let mut output = vec![0u8; self.0.len()];
        for (index, pixel) in self.0.chunks_exact(4).enumerate() {
            let offset = index * 4;
            output[offset] = pixel[2];
            output[offset + 1] = pixel[1];
            output[offset + 2] = pixel[0];
            output[offset + 3] = pixel[3];
        }
        output
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const RECORDING_MAX_EDGE: u32 = 1280;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const RECORDING_BITRATE_BPS: i32 = 800_000;

fn start(
    _non_send: NonSendMarker,
    mut start_reader: MessageReader<RecordStartRequest>,
    mut stop_reader: MessageReader<RecordStopRequest>,
    mut start_responses: MessageWriter<RecordStartResponse>,
    mut runtime: Query<(&mut RecordingBridge, &mut RecordingStatus)>,
    settings: Res<AppSettings>,
    source: CaptureSource,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let Ok((mut bridge, mut status)) = runtime.single_mut() else {
        return;
    };
    let default_dir = CaptureOutput::directory(&settings);
    for req in start_reader.read() {
        let capture = match source.resolve(req.pane.as_deref()) {
            Ok(capture) => capture,
            Err(message) => {
                start_responses.write(RecordStartResponse {
                    request_id: req.request_id,
                    result: Err(message),
                });
                continue;
            }
        };
        let wake: Option<WakeFn> = proxy.as_ref().map(|p| {
            let proxy = (***p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as WakeFn
        });
        let tx = bridge.tx.clone();
        let resp = bridge.capture.start(
            capture.window,
            capture.size.width,
            capture.size.height,
            capture.crop,
            req.request_id,
            req.gif,
            req.max_secs,
            default_dir.clone(),
            capture.scale,
            tx,
            wake,
        );
        if resp.result.is_ok() {
            *status = RecordingStatus::Recording;
        }
        start_responses.write(resp);
    }

    for req in stop_reader.read() {
        bridge
            .capture
            .stop(req.request_id, req.dir.clone(), req.name.clone());
    }
}

fn auto_stop_recordings(_non_send: NonSendMarker, mut runtime: Query<&mut RecordingBridge>) {
    let Ok(mut bridge) = runtime.single_mut() else {
        return;
    };
    bridge.capture.poll_auto_stop();
}

fn drain_recordings(
    mut runtime: Query<(&mut RecordingBridge, &mut RecordingStatus)>,
    mut last_auto: Local<Option<RecordingInfo>>,
    mut stop_responses: MessageWriter<RecordStopResponse>,
) {
    let Ok((mut bridge, mut status)) = runtime.single_mut() else {
        return;
    };
    while let Ok(outcome) = bridge.rx.try_recv() {
        bridge.capture.complete();
        *status = RecordingStatus::Idle;
        match outcome.request_id {
            Some(request_id) => {
                let result = match (&outcome.result, last_auto.take()) {
                    (Err(_), Some(info)) => Ok(info),
                    (r, _) => r.clone(),
                };
                stop_responses.write(RecordStopResponse { request_id, result });
            }
            None => {
                if let Ok(info) = outcome.result {
                    *last_auto = Some(info);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_sampling_respects_fps() {
        assert!(GifSampling::new(0, None, 12).should_sample());
        assert!(!GifSampling::new(40, Some(0), 12).should_sample());
        assert!(GifSampling::new(90, Some(0), 12).should_sample());
    }

    #[test]
    fn bgra_to_rgba_swaps_channels() {
        let bgra = vec![1u8, 2, 3, 4];
        assert_eq!(BgraFrame::new(bgra).rgba(), vec![3, 2, 1, 4]);
    }
}

#[cfg(not(target_os = "macos"))]
mod capture {
    use super::{RecordOutcome, WakeFn};
    use crate::capture_output::CropRect;
    use bevy::prelude::Entity;
    use crossbeam_channel::Sender;
    use std::path::PathBuf;
    use vmux_input::RecordStartResponse;

    #[derive(Default)]
    pub(crate) struct CaptureRuntime {}

    impl CaptureRuntime {
        #[allow(clippy::too_many_arguments)]
        pub(crate) fn start(
            &mut self,
            _window_entity: Entity,
            _img_w: u32,
            _img_h: u32,
            _crop: Option<CropRect>,
            request_id: [u8; 16],
            _gif: bool,
            _max_secs: u32,
            _default_dir: PathBuf,
            _scale: f64,
            _tx: Sender<RecordOutcome>,
            _wake: Option<WakeFn>,
        ) -> RecordStartResponse {
            RecordStartResponse {
                request_id,
                result: Err("recording is only supported on macOS".to_string()),
            }
        }

        pub(crate) fn stop(
            &mut self,
            _request_id: [u8; 16],
            _dir: Option<String>,
            _name: Option<String>,
        ) {
        }

        pub(crate) fn poll_auto_stop(&mut self) {}

        pub(crate) fn pause(&mut self) {}

        pub(crate) fn resume(&mut self) {}

        pub(crate) fn done(&mut self) {}

        pub(crate) fn complete(&mut self) {}
    }
}

#[cfg(target_os = "macos")]
#[path = "recording_capture_macos.rs"]
mod capture;
