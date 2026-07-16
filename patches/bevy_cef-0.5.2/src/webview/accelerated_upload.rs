use super::TextureWakeCallback;
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
use bevy_cef_core::prelude::{AcceleratedFrame, AcceleratedPixelFormat, Browsers};
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
    texture_wake: Option<Res<TextureWakeCallback>>,
) {
    let Ok(mut pending) = queue.0.lock() else {
        return;
    };
    // Coalesce to the newest frame per webview. Each accelerated frame carries the full current
    // surface, so older queued frames are redundant. When more than one arrived for a webview since
    // the last upload, blit the whole surface (drop dirty rects) so no superseded region is missed.
    let mut latest: HashMap<Entity, AcceleratedFrame> = HashMap::new();
    let mut coalesced: HashSet<Entity> = HashSet::new();
    let mut resized = false;
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
        if let Some(mut image) = images.get_mut(id) {
            resized |= resize_surface_image_if_needed(
                &mut image,
                frame.width,
                frame.height,
                accelerated_surface_format(frame.format),
            );
        }
        pending.push(PendingAcceleratedUpload { image: id, frame });
    }
    if resized {
        request_followup_frame(texture_wake.as_deref());
    }
}

fn resize_surface_image_if_needed(
    image: &mut Image,
    width: u32,
    height: u32,
    format: TextureFormat,
) -> bool {
    if image.width() == width
        && image.height() == height
        && image.texture_descriptor.format == format
    {
        return false;
    }
    *image = resized_surface_image(width, height, format);
    true
}

fn request_followup_frame(texture_wake: Option<&TextureWakeCallback>) {
    if let Some(wake) = texture_wake.and_then(|wake| wake.0.as_ref()) {
        wake();
    }
}

fn accelerated_surface_format(format: AcceleratedPixelFormat) -> TextureFormat {
    match format {
        AcceleratedPixelFormat::Rgba8 => TextureFormat::Rgba8UnormSrgb,
        AcceleratedPixelFormat::Bgra8 => TextureFormat::Bgra8UnormSrgb,
    }
}

fn resized_surface_image(width: u32, height: u32, format: TextureFormat) -> Image {
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
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
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[test]
    fn surface_resize_requests_followup_frame() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_for_callback = Arc::clone(&wakes);
        let wake = TextureWakeCallback(Some(Arc::new(move || {
            wakes_for_callback.fetch_add(1, Ordering::Relaxed);
        })));
        let mut image = resized_surface_image(1, 1, TextureFormat::Bgra8UnormSrgb);

        assert!(resize_surface_image_if_needed(
            &mut image,
            100,
            50,
            TextureFormat::Bgra8UnormSrgb,
        ));
        request_followup_frame(Some(&wake));

        assert_eq!(image.width(), 100);
        assert_eq!(image.height(), 50);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert!(!resize_surface_image_if_needed(
            &mut image,
            100,
            50,
            TextureFormat::Bgra8UnormSrgb,
        ));
    }

    #[test]
    fn accelerated_surface_matches_cef_pixel_format() {
        assert_eq!(
            accelerated_surface_format(AcceleratedPixelFormat::Rgba8),
            TextureFormat::Rgba8UnormSrgb
        );
        assert_eq!(
            accelerated_surface_format(AcceleratedPixelFormat::Bgra8),
            TextureFormat::Bgra8UnormSrgb
        );

        let mut image = resized_surface_image(100, 50, TextureFormat::Bgra8UnormSrgb);
        assert!(resize_surface_image_if_needed(
            &mut image,
            100,
            50,
            TextureFormat::Rgba8UnormSrgb,
        ));
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
    }
}
