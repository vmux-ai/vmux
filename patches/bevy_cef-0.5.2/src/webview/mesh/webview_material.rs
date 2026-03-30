use bevy::asset::*;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy_cef_core::prelude::*;
use std::hash::{Hash, Hasher};

const WEBVIEW_UTIL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("6c7cb871-4208-4407-9c25-306c6f069e2b");

pub(super) struct WebviewMaterialPlugin;

impl Plugin for WebviewMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WebviewMaterial>::default())
            .add_message::<RenderTextureMessage>()
            .add_systems(Update, send_render_textures);
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
    /// `w` = `0` = all corners; `1` = bottom corners only (status strip).
    #[uniform(103)]
    pub pane_corner_clip: Vec4,
    /// Active-pane frame: `x` = enabled (0/1), `y` = outset px, `z`/`w` = outer layout width/height (expanded mesh).
    #[uniform(104)]
    pub vmux_border_params: Vec4,
    /// Linear RGBA accent for the active-pane ring (vmux passes `vmux_ui` primary token).
    #[uniform(105)]
    pub vmux_border_color: Vec4,
}

impl Hash for WebviewMaterial {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.surface.hash(state);
        self.pane_corner_clip.x.to_bits().hash(state);
        self.pane_corner_clip.y.to_bits().hash(state);
        self.pane_corner_clip.z.to_bits().hash(state);
        self.pane_corner_clip.w.to_bits().hash(state);
        self.vmux_border_params.x.to_bits().hash(state);
        self.vmux_border_params.y.to_bits().hash(state);
        self.vmux_border_params.z.to_bits().hash(state);
        self.vmux_border_params.w.to_bits().hash(state);
        self.vmux_border_color.x.to_bits().hash(state);
        self.vmux_border_color.y.to_bits().hash(state);
        self.vmux_border_color.z.to_bits().hash(state);
        self.vmux_border_color.w.to_bits().hash(state);
    }
}

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
        &[47, 44, 43, 255],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

fn send_render_textures(mut ew: MessageWriter<RenderTextureMessage>, browsers: NonSend<Browsers>) {
    while let Ok(texture) = browsers.try_receive_texture() {
        ew.write(texture);
    }
}

pub(crate) fn update_webview_image(texture: RenderTextureMessage, image: &mut Image) {
    *image = Image::new(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        texture.buffer,
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    );
}
