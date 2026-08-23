use super::device::SimulatorDevice;
use super::geometry::Mapping;
use super::stream::source::LatestFrame;
use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

/// Draws the device stream as a single fitted sprite and publishes the mapping input reads.
pub struct MirrorPlugin;

impl Plugin for MirrorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DevicePoints>()
            .add_systems(Startup, Self::spawn)
            .add_systems(PreUpdate, (Self::consume_frame, Self::fit).chain());
    }
}

/// Point size of the device, probed lazily because the accessibility tree is not always up the
/// instant the app starts.
#[derive(Resource, Default)]
struct DevicePoints(Option<Vec2>);

#[derive(Component)]
struct MirrorSurface;

impl MirrorPlugin {
    fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
        commands.spawn(Camera2d);
        commands.spawn((
            MirrorSurface,
            Sprite {
                image: images.add(Self::blank()),
                custom_size: Some(Vec2::ZERO),
                ..default()
            },
        ));
    }

    fn blank() -> Image {
        Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::all(),
        )
    }

    fn consume_frame(
        slot: Res<LatestFrame>,
        surface: Single<&Sprite, With<MirrorSurface>>,
        mut images: ResMut<Assets<Image>>,
    ) {
        let Some(frame) = slot.take() else {
            return;
        };
        let Some(mut image) = images.get_mut(&surface.image) else {
            return;
        };
        let extent = Extent3d {
            width: frame.width,
            height: frame.height,
            depth_or_array_layers: 1,
        };
        if image.texture_descriptor.size != extent {
            *image = Image::new(
                extent,
                TextureDimension::D2,
                frame.rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::all(),
            );
            return;
        }
        let Some(data) = image.data.as_mut() else {
            return;
        };
        data.copy_from_slice(&frame.rgba);
    }

    fn fit(
        mut commands: Commands,
        window: Single<&Window, With<PrimaryWindow>>,
        mut surface: Single<&mut Sprite, With<MirrorSurface>>,
        images: Res<Assets<Image>>,
        device: Option<Res<SimulatorDevice>>,
        mut points: ResMut<DevicePoints>,
    ) {
        let Some(image) = images.get(&surface.image) else {
            return;
        };
        let frame = Vec2::new(
            image.texture_descriptor.size.width as f32,
            image.texture_descriptor.size.height as f32,
        );
        if frame.x <= 1.0 {
            return;
        }
        if points.0.is_none()
            && let Some(device) = device.as_deref()
            && let Some((w, h)) = device.point_size()
        {
            points.0 = Some(Vec2::new(w, h));
            info!("device points {w}x{h}, stream {}x{}", frame.x, frame.y);
        }
        let Some(device_points) = points.0 else {
            return;
        };
        let Some(mapping) = Mapping::new(window.size(), frame, device_points) else {
            return;
        };
        surface.custom_size = Some(mapping.drawn_size());
        commands.insert_resource(mapping);
    }
}
