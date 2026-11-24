use bevy::prelude::*;
use std::sync::Arc;

pub(crate) use lifecycle::RecordingPlugin;
pub(crate) mod lifecycle;
#[cfg(target_os = "macos")]
mod macos;
mod native;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_FPS: u32 = 12;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const GIF_MAX_EDGE: u32 = 800;

pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PERMISSION_MSG: &str = "Screen Recording permission required - grant it in System Settings > Privacy & Security > Screen Recording, then call record_start again.";

#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RecordingStatus {
    #[default]
    Idle,
    Recording,
    Paused,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[allow(unknown_lints, vmux_ecs_ownership)]
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
#[allow(unknown_lints, vmux_ecs_ownership)]
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
