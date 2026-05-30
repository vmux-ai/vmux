use crate::prelude::WebviewSurface;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    CommandEncoderDescriptor, Extent3d, Origin3d, TexelCopyTextureInfo, TextureAspect,
    TextureDimension, TextureFormat,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy_cef_core::prelude::{AcceleratedFrame, Browsers};
use std::sync::Mutex;

struct PendingAcceleratedUpload {
    image: AssetId<Image>,
    frame: AcceleratedFrame,
}

#[derive(Resource, Default)]
pub struct WebviewAcceleratedQueue(Mutex<Vec<PendingAcceleratedUpload>>);

#[derive(Resource, Default)]
struct ExtractedAcceleratedUploads(Vec<PendingAcceleratedUpload>);

pub(crate) struct WebviewAcceleratedUploadPlugin;

impl Plugin for WebviewAcceleratedUploadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WebviewAcceleratedQueue>()
            .add_systems(Update, queue_accelerated_uploads);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ExtractedAcceleratedUploads>()
                .add_systems(ExtractSchedule, extract_accelerated_uploads)
                .add_systems(
                    Render,
                    upload_accelerated_textures.after(RenderSystems::PrepareAssets),
                );
        }
    }
}

fn queue_accelerated_uploads(
    browsers: NonSend<Browsers>,
    surfaces: Query<&WebviewSurface>,
    mut images: ResMut<Assets<Image>>,
    queue: Res<WebviewAcceleratedQueue>,
) {
    let Ok(mut pending) = queue.0.lock() else {
        return;
    };
    while let Ok(frame) = browsers.try_receive_accelerated() {
        let Ok(surface) = surfaces.get(frame.webview) else {
            continue;
        };
        let id = surface.0.id();
        let mismatched = images
            .get(id)
            .is_none_or(|image| image.width() != frame.width || image.height() != frame.height);
        if mismatched && let Some(mut image) = images.get_mut(id) {
            *image = resized_surface_image(frame.width, frame.height);
        }
        pending.push(PendingAcceleratedUpload { image: id, frame });
    }
}

fn resized_surface_image(width: u32, height: u32) -> Image {
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

fn extract_accelerated_uploads(
    main: Extract<Res<WebviewAcceleratedQueue>>,
    mut extracted: ResMut<ExtractedAcceleratedUploads>,
) {
    extracted.0.clear();
    if let Ok(mut pending) = main.0.lock() {
        extracted.0.append(&mut pending);
    }
}

fn upload_accelerated_textures(
    mut extracted: ResMut<ExtractedAcceleratedUploads>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    for upload in extracted.0.drain(..) {
        let PendingAcceleratedUpload { image, frame } = upload;
        let Some(gpu) = gpu_images.get(image) else {
            continue;
        };
        if gpu.texture_descriptor.size.width != frame.width
            || gpu.texture_descriptor.size.height != frame.height
        {
            continue;
        }
        let AcceleratedFrame {
            handle,
            keepalive,
            dirty,
            width,
            height,
            ..
        } = frame;
        let src = match handle.0.import_texture(render_device.wgpu_device()) {
            Ok(src) => src,
            Err(_) => continue,
        };

        let mut encoder =
            render_device.create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut blit = |x: u32, y: u32, w: u32, h: u32| {
                if w == 0 || h == 0 {
                    return;
                }
                encoder.copy_texture_to_texture(
                    TexelCopyTextureInfo {
                        texture: &src,
                        mip_level: 0,
                        origin: Origin3d { x, y, z: 0 },
                        aspect: TextureAspect::All,
                    },
                    TexelCopyTextureInfo {
                        texture: &gpu.texture,
                        mip_level: 0,
                        origin: Origin3d { x, y, z: 0 },
                        aspect: TextureAspect::All,
                    },
                    Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            };
            if dirty.is_empty() {
                blit(0, 0, width, height);
            } else {
                for rect in &dirty {
                    blit(rect.x, rect.y, rect.width, rect.height);
                }
            }
        }
        render_queue.submit(std::iter::once(encoder.finish()));
        render_queue.on_submitted_work_done(move || {
            drop((keepalive, src));
        });
    }
}
