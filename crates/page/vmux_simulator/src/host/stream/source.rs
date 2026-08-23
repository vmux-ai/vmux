use super::super::device::{Axe, SimulatorDevice};
use super::mjpeg::MjpegReader;
use bevy::prelude::*;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};

/// One decoded device frame, in RGBA.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Always-newest slot rather than a queue: a slow consumer should skip stale frames, never
/// accumulate a backlog of them.
#[derive(Resource, Default, Clone)]
pub struct LatestFrame(Arc<Mutex<Option<Frame>>>);

impl LatestFrame {
    pub fn replace(&self, frame: Frame) {
        *self.0.lock().expect("frame slot") = Some(frame);
    }

    pub fn take(&self) -> Option<Frame> {
        self.0.lock().expect("frame slot").take()
    }
}

pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// A running `axe stream-video` child and the thread draining it.
///
/// Dropping this kills the child; AXe streams until killed and has no quit protocol.
#[derive(Resource)]
pub struct AxeStream {
    child: Child,
}

impl AxeStream {
    const FPS: &'static str = "20";
    const SCALE: &'static str = "0.5";

    pub fn start(
        device: &SimulatorDevice,
        slot: LatestFrame,
        wake: Option<WakeFn>,
    ) -> Option<Self> {
        let mut child = Axe::command()
            .args(["stream-video", "--udid", &device.udid])
            .args(["--format", "mjpeg"])
            .args(["--fps", Self::FPS])
            .args(["--scale", Self::SCALE])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        std::thread::Builder::new()
            .name("vmux-simulator-stream".into())
            .spawn(move || Self::pump(MjpegReader::new(stdout), slot, wake))
            .ok()?;
        Some(Self { child })
    }

    fn pump(
        mut reader: MjpegReader<std::process::ChildStdout>,
        slot: LatestFrame,
        wake: Option<WakeFn>,
    ) {
        while let Some(jpeg) = reader.next_frame() {
            let Ok(decoded) = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
            else {
                continue;
            };
            let rgba = decoded.to_rgba8();
            slot.replace(Frame {
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
            });
            if let Some(wake) = &wake {
                wake();
            }
        }
    }
}

impl Drop for AxeStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Frame {
        fn grey(width: u32, height: u32, level: u8) -> Self {
            Self {
                width,
                height,
                rgba: vec![level; (width * height * 4) as usize],
            }
        }
    }

    #[test]
    fn slot_keeps_only_the_newest_frame() {
        let slot = LatestFrame::default();

        slot.replace(Frame::grey(2, 2, 1));
        slot.replace(Frame::grey(2, 2, 2));
        slot.replace(Frame::grey(2, 2, 3));

        let taken = slot.take().expect("a frame");
        assert_eq!(taken.rgba[0], 3);
        assert!(slot.take().is_none(), "slot should be empty after taking");
    }

    #[test]
    fn slot_reports_a_resize_rather_than_mixing_sizes() {
        let slot = LatestFrame::default();

        slot.replace(Frame::grey(4, 4, 1));
        slot.replace(Frame::grey(8, 2, 1));

        let taken = slot.take().expect("a frame");
        assert_eq!((taken.width, taken.height), (8, 2));
        assert_eq!(taken.rgba.len(), 8 * 2 * 4);
    }
}
