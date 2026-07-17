use bevy::asset::*;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy_cef_core::prelude::*;
use std::hash::{Hash, Hasher};

#[cfg(feature = "pbr")]
const WEBVIEW_UTIL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("6c7cb871-4208-4407-9c25-306c6f069e2b");

pub(super) struct WebviewMaterialPlugin;

impl Plugin for WebviewMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RenderTextureMessage>()
            .add_systems(Update, send_render_textures);

        #[cfg(feature = "pbr")]
        app.add_plugins(MaterialPlugin::<WebviewMaterial>::default());

        #[cfg(feature = "pbr")]
        load_internal_asset!(
            app,
            WEBVIEW_UTIL_SHADER_HANDLE,
            "./webview_util.wgsl",
            Shader::from_wgsl
        );
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct WebviewMaterial {
    /// Holds the texture handle for the webview.
    ///
    /// This texture is automatically updated.
    #[texture(101)]
    #[sampler(102)]
    pub surface: Option<Handle<Image>>,
    /// Rounded-rect clip in **layout pixels**: `x` = corner radius, `y` = width, `z` = height,
    /// `w` = `0` = all corners; `1` = bottom only; `2` = top only.
    #[uniform(103)]
    pub pane_corner_clip: Vec4,
}

impl Hash for WebviewMaterial {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.surface.hash(state);
        self.pane_corner_clip.x.to_bits().hash(state);
        self.pane_corner_clip.y.to_bits().hash(state);
        self.pane_corner_clip.z.to_bits().hash(state);
        self.pane_corner_clip.w.to_bits().hash(state);
    }
}

#[cfg(feature = "pbr")]
impl Material for WebviewMaterial {}

/// Solid dark placeholder until the first CEF frame arrives (`Image::default()` is 1×1 white).
pub(crate) fn webview_placeholder_image() -> Image {
    Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // Bgra8UnormSrgb — matches [`update_webview_image`].
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

fn send_render_textures(mut ew: MessageWriter<RenderTextureMessage>, browsers: NonSend<Browsers>) {
    for texture in browsers.drain_render_textures() {
        ew.write(texture);
    }
}

/// (Re)allocate the surface image to match a CEF paint. Used only on the first frame and on size
/// changes; steady-state same-size paints stream pixels via `write_texture` (see `texture_upload`)
/// without touching the asset system.
///
/// Keeps [`RenderAssetUsages::all`]: the main-world copy must persist so the upload path can read
/// the current surface size and so the prepared `GpuImage` is never unloaded between resizes.
pub(crate) fn update_webview_image(texture: &RenderTextureMessage, image: &mut Image) {
    let stride = texture.width as usize * 4;
    let mut buffer = vec![0_u8; stride * texture.height as usize];
    for patch in texture.patches.iter() {
        let row_bytes = patch.rect.width as usize * 4;
        for row in 0..patch.rect.height as usize {
            let source_start = row * row_bytes;
            let destination_start =
                (patch.rect.y as usize + row) * stride + patch.rect.x as usize * 4;
            buffer[destination_start..destination_start + row_bytes]
                .copy_from_slice(&patch.buffer[source_start..source_start + row_bytes]);
        }
    }
    *image = Image::new(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        buffer,
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    );
}
