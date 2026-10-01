use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
};
#[cfg(feature = "recording")]
use std::path::Path;
use std::path::PathBuf;

use vmux_flex::prelude::ComputedNode;
use vmux_setting::AppSettings;

pub(crate) struct CaptureOutput;

impl CaptureOutput {
    pub(crate) fn directory(settings: &AppSettings) -> PathBuf {
        settings
            .recording
            .output_dir
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| vmux_ecs::profile::ProfilePaths::current().recording())
    }

    #[cfg(feature = "recording")]
    pub(crate) fn paths(
        directory: Option<&str>,
        name: Option<&str>,
        gif: bool,
        timestamp: &str,
        default_directory: &Path,
    ) -> (PathBuf, Option<PathBuf>) {
        let directory = directory
            .map(PathBuf::from)
            .unwrap_or_else(|| default_directory.to_path_buf());
        let name = name
            .map(str::to_string)
            .unwrap_or_else(|| format!("vmux-{timestamp}"));
        let mp4 = directory.join(format!("{name}.mp4"));
        let gif = gif.then(|| directory.join(format!("{name}.gif")));
        (mp4, gif)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CaptureSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl CaptureSize {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub(crate) fn downscaled(self, max_edge: u32) -> Self {
        let long = self.width.max(self.height);
        if long == 0 {
            return Self::new(1, 1);
        }
        if long <= max_edge {
            return Self::new(self.width.max(1), self.height.max(1));
        }
        let scale = max_edge as f64 / long as f64;
        Self::new(
            ((self.width as f64 * scale).round() as u32).max(1),
            ((self.height as f64 * scale).round() as u32).max(1),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CropRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
}

impl CropRect {
    pub(crate) fn from_node(rect: ComputedNode, image: CaptureSize) -> Self {
        let min = rect.min();
        let left = (min.x.round().max(0.0) as u32).min(image.width.saturating_sub(1));
        let top = (min.y.round().max(0.0) as u32).min(image.height.saturating_sub(1));
        let width = (rect.size.x.round().max(1.0) as u32).min(image.width - left);
        let height = (rect.size.y.round().max(1.0) as u32).min(image.height - top);
        Self {
            x: left,
            y: top,
            w: width,
            h: height,
        }
    }
}

pub(crate) struct ResolvedCapture {
    pub(crate) window: Entity,
    pub(crate) size: CaptureSize,
    pub(crate) scale: f64,
    pub(crate) crop: Option<CropRect>,
}

#[derive(SystemParam)]
pub(crate) struct CaptureSource<'w, 's> {
    focused_window: vmux_layout::window::FocusedWindow<'w, 's>,
    hierarchy: vmux_layout::window::WindowHierarchy<'w, 's>,
    windows: Query<'w, 's, (Entity, &'static Window)>,
    nodes: Query<'w, 's, &'static ComputedNode>,
    parents: Query<'w, 's, &'static ChildOf>,
}

impl CaptureSource<'_, '_> {
    pub(crate) fn resolve(&self, pane: Option<&str>) -> Result<ResolvedCapture, String> {
        let pane_window = pane.and_then(|id| {
            let (_, bits) = vmux_layout::protocol::parse_id(id).ok()?;
            self.hierarchy.get(Entity::from_bits(bits))
        });
        let window = pane_window
            .or(self.focused_window.entity())
            .ok_or_else(|| "no focused vmux window".to_string())?;
        let (_, native_window) = self
            .windows
            .get(window)
            .map_err(|_| "focused vmux window not found".to_string())?;
        let size = CaptureSize::new(
            native_window.resolution.physical_width(),
            native_window.resolution.physical_height(),
        );
        let crop = match pane {
            Some(id) => Some(
                self.crop(id, size)
                    .ok_or_else(|| format!("pane not found: {id}"))?,
            ),
            None => None,
        };
        Ok(ResolvedCapture {
            window,
            size,
            scale: native_window.resolution.scale_factor() as f64,
            crop,
        })
    }

    fn crop(&self, id: &str, image: CaptureSize) -> Option<CropRect> {
        let (_, bits) = vmux_layout::protocol::parse_id(id).ok()?;
        let mut entity = Entity::from_bits(bits);
        for _ in 0..8 {
            if let Ok(&computed) = self.nodes.get(entity) {
                return Some(CropRect::from_node(computed, image));
            }
            entity = self.parents.get(entity).ok()?.get();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "recording")]
    #[test]
    fn output_paths_use_defaults_and_custom_values() {
        let default_directory = Path::new("/tmp/def");
        let (mp4, gif) =
            CaptureOutput::paths(None, None, false, "20260623-101010-001", default_directory);
        assert_eq!(mp4, PathBuf::from("/tmp/def/vmux-20260623-101010-001.mp4"));
        assert!(gif.is_none());

        let (mp4, gif) = CaptureOutput::paths(
            Some("/tmp/out"),
            Some("feature-x"),
            true,
            "ts",
            default_directory,
        );
        assert_eq!(mp4, PathBuf::from("/tmp/out/feature-x.mp4"));
        assert_eq!(gif, Some(PathBuf::from("/tmp/out/feature-x.gif")));
    }

    #[test]
    fn downscale_caps_long_edge_without_upscaling() {
        assert_eq!(
            CaptureSize::new(800, 600).downscaled(800),
            CaptureSize::new(800, 600)
        );
        assert_eq!(
            CaptureSize::new(1600, 800).downscaled(800),
            CaptureSize::new(800, 400)
        );
        assert_eq!(
            CaptureSize::new(0, 0).downscaled(800),
            CaptureSize::new(1, 1)
        );
    }

    #[test]
    fn crop_rect_clamps_to_image() {
        let rect = CropRect::from_node(
            ComputedNode {
                size: Vec2::new(80.0, 60.0),
                center: Vec2::new(100.0, 100.0),
                ..default()
            },
            CaptureSize::new(1000, 1000),
        );
        assert_eq!(
            rect,
            CropRect {
                x: 60,
                y: 70,
                w: 80,
                h: 60,
            }
        );

        let rect = CropRect::from_node(
            ComputedNode {
                size: Vec2::splat(40.0),
                center: Vec2::splat(990.0),
                ..default()
            },
            CaptureSize::new(1000, 1000),
        );
        assert_eq!(
            rect,
            CropRect {
                x: 970,
                y: 970,
                w: 30,
                h: 30,
            }
        );
    }
}
