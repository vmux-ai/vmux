use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vmux_agent::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse, RecordingInfo,
};
use vmux_setting::AppSettings;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_FPS: u32 = 12;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_MAX_EDGE: u32 = 800;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PERMISSION_MSG: &str = "Screen Recording permission required - grant it in System Settings > \
Privacy & Security > Screen Recording, then call vmux_record_start again.";

/// Carries finalize outcomes from off-thread (stop/auto-stop) back to Bevy.
/// `request_id == None` means an auto-stop (no pending query to answer).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct RecordOutcome {
    pub request_id: Option<[u8; 16]>,
    pub result: Result<RecordingInfo, String>,
}

#[derive(Resource)]
pub(crate) struct RecordingBridge {
    pub(crate) tx: Sender<RecordOutcome>,
    rx: Receiver<RecordOutcome>,
}

impl Default for RecordingBridge {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn start_err(request_id: [u8; 16], message: impl Into<String>) -> RecordStartResponse {
    RecordStartResponse {
        request_id,
        result: Err(message.into()),
    }
}

/// Configured output directory for screenshots and recordings: the
/// `recording.output_dir` setting if set, else the default `~/.vmux/recording`.
pub(crate) fn capture_output_dir(settings: &AppSettings) -> PathBuf {
    settings
        .recording
        .output_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(vmux_core::profile::recording_dir)
}

/// Resolve dir/name into final mp4 + optional gif paths. `default_dir` is used
/// when `dir` is not given (the configured/default output directory).
pub(crate) fn resolve_output_paths(
    dir: Option<&str>,
    name: Option<&str>,
    gif: bool,
    timestamp: &str,
    default_dir: &Path,
) -> (PathBuf, Option<PathBuf>) {
    let base_dir = dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_dir.to_path_buf());
    let base_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("vmux-{timestamp}"));
    let mp4 = base_dir.join(format!("{base_name}.mp4"));
    let gif_path = gif.then(|| base_dir.join(format!("{base_name}.gif")));
    (mp4, gif_path)
}

/// Whether a frame at `elapsed_ms` should be sampled into the GIF given the
/// last sampled timestamp and target fps.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn should_sample_gif_frame(
    elapsed_ms: u64,
    last_sampled_ms: Option<u64>,
    fps: u32,
) -> bool {
    let interval = (1000 / fps.max(1)) as u64;
    match last_sampled_ms {
        None => true,
        Some(last) => elapsed_ms.saturating_sub(last) >= interval,
    }
}

/// BGRA (ScreenCaptureKit native) -> RGBA (image/gif crates).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; bgra.len()];
    for (i, px) in bgra.chunks_exact(4).enumerate() {
        let o = i * 4;
        out[o] = px[2];
        out[o + 1] = px[1];
        out[o + 2] = px[0];
        out[o + 3] = px[3];
    }
    out
}

/// Cap the long edge at `max_edge`, never upscaling.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn downscale_to(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long == 0 {
        return (1, 1);
    }
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let scale = max_edge as f64 / long as f64;
    (
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub(crate) fn crop_rect_from_node(
    center_x: f32,
    center_y: f32,
    size_x: f32,
    size_y: f32,
    img_w: u32,
    img_h: u32,
) -> CropRect {
    let left = (center_x - size_x * 0.5).round().max(0.0) as u32;
    let top = (center_y - size_y * 0.5).round().max(0.0) as u32;
    let left = left.min(img_w.saturating_sub(1));
    let top = top.min(img_h.saturating_sub(1));
    let w = (size_x.round().max(1.0) as u32).min(img_w - left);
    let h = (size_y.round().max(1.0) as u32).min(img_h - top);
    CropRect {
        x: left,
        y: top,
        w,
        h,
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn resolve_crop(
    id: &str,
    node_q: &Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: &Query<&ChildOf>,
    img_w: u32,
    img_h: u32,
) -> Option<CropRect> {
    use bevy::ecs::relationship::Relationship;
    let (_, bits) = vmux_layout::protocol::parse_id(id).ok()?;
    let mut entity = Entity::from_bits(bits);
    for _ in 0..8 {
        if let Ok((computed, gt)) = node_q.get(entity) {
            let size = computed.size;
            let center = gt.transform_point2(Vec2::ZERO);
            return Some(crop_rect_from_node(
                center.x, center.y, size.x, size.y, img_w, img_h,
            ));
        }
        entity = child_of_q.get(entity).ok()?.get();
    }
    None
}

pub(crate) fn start_recording(
    _non_send: NonSendMarker,
    mut start_reader: MessageReader<RecordStartRequest>,
    mut stop_reader: MessageReader<RecordStopRequest>,
    mut start_responses: MessageWriter<RecordStartResponse>,
    bridge: Res<RecordingBridge>,
    settings: Res<AppSettings>,
    window_q: Query<(Entity, &Window), With<PrimaryWindow>>,
    node_q: Query<(&ComputedNode, &UiGlobalTransform)>,
    child_of_q: Query<&ChildOf>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let default_dir = capture_output_dir(&settings);
    for req in start_reader.read() {
        let Ok((window_entity, window)) = window_q.single() else {
            start_responses.write(start_err(req.request_id, "no primary vmux window"));
            continue;
        };
        let img_w = window.resolution.physical_width();
        let img_h = window.resolution.physical_height();
        let crop = match &req.pane {
            Some(id) => match resolve_crop(id, &node_q, &child_of_q, img_w, img_h) {
                Some(rect) => Some(rect),
                None => {
                    start_responses
                        .write(start_err(req.request_id, format!("pane not found: {id}")));
                    continue;
                }
            },
            None => None,
        };
        let wake: Option<WakeFn> = proxy.as_ref().map(|p| {
            let proxy = (***p).clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }) as WakeFn
        });
        let resp = capture::start(
            window_entity,
            img_w,
            img_h,
            crop,
            req.request_id,
            req.gif,
            req.max_secs,
            default_dir.clone(),
            bridge.tx.clone(),
            wake,
        );
        start_responses.write(resp);
    }

    for req in stop_reader.read() {
        capture::stop(req.request_id, req.dir.clone(), req.name.clone());
    }
}

pub(crate) fn auto_stop_recordings(_non_send: NonSendMarker) {
    capture::poll_auto_stop();
}

pub(crate) fn drain_recordings(
    bridge: Res<RecordingBridge>,
    mut last_auto: Local<Option<RecordingInfo>>,
    mut stop_responses: MessageWriter<RecordStopResponse>,
) {
    while let Ok(outcome) = bridge.rx.try_recv() {
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
    fn output_paths_default_dir_and_name() {
        let default_dir = Path::new("/tmp/def");
        let (mp4, gif) =
            resolve_output_paths(None, None, false, "20260623-101010-001", default_dir);
        assert_eq!(mp4, PathBuf::from("/tmp/def/vmux-20260623-101010-001.mp4"));
        assert!(gif.is_none());
    }

    #[test]
    fn output_paths_custom_dir_name_and_gif() {
        let (mp4, gif) = resolve_output_paths(
            Some("/tmp/out"),
            Some("feature-x"),
            true,
            "ts",
            Path::new("/tmp/def"),
        );
        assert_eq!(mp4, PathBuf::from("/tmp/out/feature-x.mp4"));
        assert_eq!(gif, Some(PathBuf::from("/tmp/out/feature-x.gif")));
    }

    #[test]
    fn gif_sampling_respects_fps() {
        assert!(should_sample_gif_frame(0, None, 12));
        assert!(!should_sample_gif_frame(40, Some(0), 12));
        assert!(should_sample_gif_frame(90, Some(0), 12));
    }

    #[test]
    fn bgra_to_rgba_swaps_channels() {
        let bgra = vec![1u8, 2, 3, 4];
        assert_eq!(bgra_to_rgba(&bgra), vec![3, 2, 1, 4]);
    }

    #[test]
    fn crop_rect_clamps_to_image() {
        let r = crop_rect_from_node(100.0, 100.0, 80.0, 60.0, 1000, 1000);
        assert_eq!(
            r,
            CropRect {
                x: 60,
                y: 70,
                w: 80,
                h: 60
            }
        );
    }

    #[test]
    fn downscale_caps_long_edge_without_upscaling() {
        assert_eq!(downscale_to(800, 600, 800), (800, 600));
        assert_eq!(downscale_to(1600, 800, 800), (800, 400));
        assert_eq!(downscale_to(0, 0, 800), (1, 1));
    }
}

#[cfg(not(target_os = "macos"))]
mod capture {
    use super::{CropRect, RecordOutcome, WakeFn};
    use bevy::prelude::Entity;
    use crossbeam_channel::Sender;
    use std::path::PathBuf;
    use vmux_agent::RecordStartResponse;

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start(
        _window_entity: Entity,
        _img_w: u32,
        _img_h: u32,
        _crop: Option<CropRect>,
        request_id: [u8; 16],
        _gif: bool,
        _max_secs: u32,
        _default_dir: PathBuf,
        _tx: Sender<RecordOutcome>,
        _wake: Option<WakeFn>,
    ) -> RecordStartResponse {
        RecordStartResponse {
            request_id,
            result: Err("recording is only supported on macOS".to_string()),
        }
    }

    pub(crate) fn stop(_request_id: [u8; 16], _dir: Option<String>, _name: Option<String>) {}

    pub(crate) fn poll_auto_stop() {}
}

#[cfg(target_os = "macos")]
#[path = "recording_capture_macos.rs"]
mod capture;
