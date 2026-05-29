use crate::prelude::update_webview_image;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy_cef_core::prelude::{RenderTextureMessage, WebviewDirtyRect};
use std::sync::Arc;

/// A CEF paint streamed into an already-prepared GPU texture via `write_texture`, so the texture
/// and its bind group are reused frame to frame instead of being recreated through the asset system.
#[derive(Clone)]
struct PendingTextureUpload {
    image: AssetId<Image>,
    buffer: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    dirty: Vec<WebviewDirtyRect>,
}

/// Render-world queue of pending webview pixel uploads. Public only because the public
/// `render_standard_materials` system takes it as a parameter; treat it as an internal detail.
#[derive(Resource, Default)]
pub struct WebviewTextureUploads(Vec<PendingTextureUpload>);

#[derive(Resource, Default)]
struct ExtractedTextureUploads(Vec<PendingTextureUpload>);

pub(crate) struct WebviewTextureUploadPlugin;

impl Plugin for WebviewTextureUploadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WebviewTextureUploads>()
            .add_systems(First, clear_main_uploads);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ExtractedTextureUploads>()
                .add_systems(ExtractSchedule, extract_texture_uploads)
                .add_systems(
                    Render,
                    upload_webview_textures.after(RenderSystems::PrepareAssets),
                );
        }
    }
}

/// Stream a CEF paint to the webview's surface texture.
///
/// On first paint or a size change the surface [`Image`] is (re)created so Bevy prepares a GPU
/// texture of the right size; the pixels ride along with that allocation. For same-size paints —
/// the common case — the pixels are queued for an in-place `write_texture` in the render world,
/// leaving the asset (and its bind group) untouched.
pub(crate) fn apply_webview_texture(
    texture: &RenderTextureMessage,
    images: &mut Assets<Image>,
    handle: &Handle<Image>,
    uploads: &mut WebviewTextureUploads,
) {
    let same_size = images
        .get(handle.id())
        .is_some_and(|image| image.width() == texture.width && image.height() == texture.height);

    if same_size {
        uploads.0.push(PendingTextureUpload {
            image: handle.id(),
            buffer: texture.buffer.clone(),
            width: texture.width,
            height: texture.height,
            dirty: texture.dirty.clone(),
        });
    } else if let Some(mut image) = images.get_mut(handle.id()) {
        update_webview_image(texture, &mut image);
    }
}

fn clear_main_uploads(mut uploads: ResMut<WebviewTextureUploads>) {
    uploads.0.clear();
}

fn extract_texture_uploads(
    main: Extract<Res<WebviewTextureUploads>>,
    mut extracted: ResMut<ExtractedTextureUploads>,
) {
    extracted.0.clear();
    extracted.0.extend(main.0.iter().cloned());
}

fn upload_webview_textures(
    extracted: Res<ExtractedTextureUploads>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
) {
    for upload in &extracted.0 {
        let Some(gpu) = gpu_images.get(upload.image) else {
            continue;
        };
        if gpu.texture_descriptor.size.width != upload.width
            || gpu.texture_descriptor.size.height != upload.height
        {
            continue;
        }
        let stride = upload.width * 4;
        if upload.dirty.is_empty() {
            write_region(
                &queue,
                gpu,
                &upload.buffer,
                stride,
                0,
                0,
                upload.width,
                upload.height,
            );
        } else {
            for rect in &upload.dirty {
                write_region(
                    &queue,
                    gpu,
                    &upload.buffer,
                    stride,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_region(
    queue: &RenderQueue,
    gpu: &GpuImage,
    buffer: &[u8],
    stride: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    if width == 0 || height == 0 {
        return;
    }
    let offset = y as u64 * stride as u64 + x as u64 * 4;
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &gpu.texture,
            mip_level: 0,
            // (texture is bevy's `Texture` wrapper; wgpu wants `&wgpu::Texture`)
            origin: Origin3d { x, y, z: 0 },
            aspect: TextureAspect::All,
        },
        buffer,
        TexelCopyBufferLayout {
            offset,
            bytes_per_row: Some(stride),
            rows_per_image: None,
        },
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{TextureDimension, TextureFormat};
    use bevy_cef_core::prelude::RenderPaintElementType;

    fn paint(width: u32, height: u32) -> RenderTextureMessage {
        RenderTextureMessage {
            webview: Entity::from_bits(1),
            ty: RenderPaintElementType::View,
            width,
            height,
            buffer: Arc::new(vec![0u8; (width * height * 4) as usize]),
            dirty: Vec::new(),
        }
    }

    fn surface(width: u32, height: u32) -> Image {
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

    #[test]
    fn same_size_paint_queues_upload_and_leaves_asset_alone() {
        let mut images = Assets::<Image>::default();
        let handle = images.add(surface(4, 4));
        let mut uploads = WebviewTextureUploads::default();

        apply_webview_texture(&paint(4, 4), &mut images, &handle, &mut uploads);

        assert_eq!(uploads.0.len(), 1);
        assert_eq!(
            images.get(handle.id()).map(|i| (i.width(), i.height())),
            Some((4, 4))
        );
    }

    #[test]
    fn size_change_reallocs_asset_and_skips_streaming_upload() {
        let mut images = Assets::<Image>::default();
        let handle = images.add(surface(4, 4));
        let mut uploads = WebviewTextureUploads::default();

        apply_webview_texture(&paint(8, 6), &mut images, &handle, &mut uploads);

        assert!(uploads.0.is_empty());
        assert_eq!(
            images.get(handle.id()).map(|i| (i.width(), i.height())),
            Some((8, 6))
        );
    }
}
