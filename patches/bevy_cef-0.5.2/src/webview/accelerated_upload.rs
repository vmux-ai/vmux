use crate::common::WebviewNativeOverlay;
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
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

struct PendingAcceleratedUpload {
    image: AssetId<Image>,
    frame: AcceleratedFrame,
}

#[derive(Resource, Default)]
pub struct WebviewAcceleratedQueue(Mutex<Vec<PendingAcceleratedUpload>>);

/// Latest accelerated frame per [`WebviewNativeOverlay`] webview, for a native overlay layer to
/// display (instead of a Bevy texture). Take the frame and keep it alive while its IOSurface is set
/// as a `CALayer`'s contents.
#[derive(Resource, Default)]
pub struct NativeOverlayFrames(pub Mutex<HashMap<Entity, AcceleratedFrame>>);

#[derive(Resource, Default)]
struct ExtractedAcceleratedUploads(Vec<PendingAcceleratedUpload>);

pub(crate) struct WebviewAcceleratedUploadPlugin;

impl Plugin for WebviewAcceleratedUploadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WebviewAcceleratedQueue>()
            .init_resource::<NativeOverlayFrames>()
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
    overlay_webviews: Query<Entity, With<WebviewNativeOverlay>>,
    overlay_frames: Res<NativeOverlayFrames>,
    mut images: ResMut<Assets<Image>>,
    queue: Res<WebviewAcceleratedQueue>,
) {
    let Ok(mut pending) = queue.0.lock() else {
        return;
    };
    // Coalesce to the newest frame per webview. Each accelerated frame carries the full current
    // surface, so older queued frames are redundant. When more than one arrived for a webview since
    // the last upload, blit the whole surface (drop dirty rects) so no superseded region is missed.
    let mut latest: HashMap<Entity, AcceleratedFrame> = HashMap::new();
    let mut coalesced: HashSet<Entity> = HashSet::new();
    while let Ok(frame) = browsers.try_receive_accelerated() {
        let webview = frame.webview;
        if latest.insert(webview, frame).is_some() {
            coalesced.insert(webview);
        }
    }
    for (webview, mut frame) in latest {
        let overlay = overlay_webviews.contains(webview);
        if overlay {
            if let Ok(mut overlay) = overlay_frames.0.lock() {
                overlay.insert(webview, frame);
            }
            continue;
        }
        let Ok(surface) = surfaces.get(webview) else {
            continue;
        };
        if coalesced.contains(&webview) {
            frame.dirty.clear();
        }
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
    if let Ok(mut pending) = main.0.lock() {
        extracted.0.append(&mut pending);
    }
    coalesce_pending_accelerated_uploads(&mut extracted.0);
}

fn coalesce_pending_accelerated_uploads(uploads: &mut Vec<PendingAcceleratedUpload>) {
    if uploads.len() < 2 {
        return;
    }
    let mut latest = HashMap::new();
    for upload in uploads.drain(..) {
        latest.insert(upload.frame.webview, upload);
    }
    uploads.extend(latest.into_values());
}

fn upload_accelerated_textures(
    mut extracted: ResMut<ExtractedAcceleratedUploads>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    let uploads = std::mem::take(&mut extracted.0);
    let mut retry = Vec::new();
    for upload in uploads {
        let Some(gpu) = gpu_images.get(upload.image) else {
            retry.push(upload);
            continue;
        };
        if gpu.texture_descriptor.size.width != upload.frame.width
            || gpu.texture_descriptor.size.height != upload.frame.height
        {
            retry.push(upload);
            continue;
        }
        let PendingAcceleratedUpload { frame, .. } = upload;
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
    if !retry.is_empty() {
        extracted.0.extend(retry);
        coalesce_pending_accelerated_uploads(&mut extracted.0);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn accelerated_uploads_survive_gpu_asset_resize_lag() {
        let source = include_str!("accelerated_upload.rs");
        let extract = source
            .split("fn extract_accelerated_uploads")
            .nth(1)
            .and_then(|tail| tail.split("fn upload_accelerated_textures").next())
            .expect("extract system source");
        let upload = source
            .split("fn upload_accelerated_textures")
            .nth(1)
            .expect("upload system source");

        assert!(
            !extract.contains("extracted.0.clear();"),
            "deferred accelerated uploads must survive extract frames"
        );
        assert!(
            upload.contains("retry.push(upload);"),
            "GPU-size misses must retry instead of dropping one-shot frames"
        );
    }
}
