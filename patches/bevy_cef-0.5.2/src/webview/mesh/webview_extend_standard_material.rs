#[cfg(feature = "pbr")]
use crate::common::WebviewSource;
use crate::prelude::{
    WebviewMaterial, WebviewMaterialHandle, WebviewSurface, webview_placeholder_image,
};
use crate::webview::texture_upload::{WebviewTextureUploads, apply_webview_texture};
use bevy::asset::*;
#[cfg(feature = "pbr")]
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
#[cfg(feature = "pbr")]
use bevy::shader::ShaderRef;
use bevy_cef_core::prelude::*;

#[cfg(feature = "pbr")]
const FRAGMENT_SHADER_HANDLE: Handle<Shader> = uuid_handle!("b231681f-9c17-4df6-89c9-9dc353e85a08");

#[cfg(feature = "pbr")]
pub(super) struct WebviewExtendStandardMaterialPlugin;

#[cfg(feature = "pbr")]
impl Plugin for WebviewExtendStandardMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WebviewExtendStandardMaterial>::default())
            .add_systems(PreUpdate, ensure_mesh_webview_placeholder)
            .add_systems(PostUpdate, render_standard_materials);
        load_internal_asset!(
            app,
            FRAGMENT_SHADER_HANDLE,
            "./webview_extend_standard_material.wgsl",
            Shader::from_wgsl
        );
    }
}

#[cfg(feature = "pbr")]
impl MaterialExtension for WebviewMaterial {
    fn fragment_shader() -> ShaderRef {
        FRAGMENT_SHADER_HANDLE.into()
    }
}

#[cfg(feature = "pbr")]
pub type WebviewExtendStandardMaterial = ExtendedMaterial<StandardMaterial, WebviewMaterial>;

#[cfg(not(feature = "pbr"))]
#[derive(Debug, Clone, PartialEq)]
pub struct WebviewBaseMaterial {
    pub base_color: Color,
    pub alpha_mode: bevy::material::AlphaMode,
    pub unlit: bool,
    pub depth_bias: f32,
    pub cull_mode: Option<bevy::render::render_resource::Face>,
}

#[cfg(not(feature = "pbr"))]
impl Default for WebviewBaseMaterial {
    fn default() -> Self {
        Self {
            base_color: Color::WHITE,
            alpha_mode: bevy::material::AlphaMode::Opaque,
            unlit: false,
            depth_bias: 0.0,
            cull_mode: Some(bevy::render::render_resource::Face::Back),
        }
    }
}

#[cfg(not(feature = "pbr"))]
#[derive(Asset, TypePath, Debug, Clone, PartialEq, Default)]
pub struct WebviewExtendStandardMaterial {
    pub base: WebviewBaseMaterial,
    pub extension: WebviewMaterial,
}

/// While [`WebviewMaterial::surface`] is [`None`], Bevy binds a default **white** texture — assign
/// our dark placeholder before the first frame is drawn (see [`webview_placeholder_image`]).
#[cfg(feature = "pbr")]
fn ensure_mesh_webview_placeholder(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    webviews: Query<
        (
            Entity,
            &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        ),
        With<WebviewSource>,
    >,
) {
    for (entity, mesh_mat) in &webviews {
        let Some(mut mat) = materials.get_mut(mesh_mat.id()) else {
            continue;
        };
        if mat.extension.surface.is_some() {
            continue;
        }
        let handle = images.add(webview_placeholder_image());
        mat.extension.surface = Some(handle.clone());
        commands.entity(entity).insert(WebviewSurface(handle));
    }
}

/// Applies [`RenderTextureMessage`] updates to [`WebviewExtendStandardMaterial`] meshes.
///
/// Vmux (and similar apps) may schedule this explicitly so pane layout / CEF resize run first in `PostUpdate`.
pub fn render_standard_materials(
    mut commands: Commands,
    mut er: MessageReader<RenderTextureMessage>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut uploads: ResMut<WebviewTextureUploads>,
    webviews: Query<&WebviewMaterialHandle<WebviewExtendStandardMaterial>>,
    mut logged: Local<bevy::platform::collections::HashSet<Entity>>,
) {
    for texture in er.read() {
        let Ok(mat_handle) = webviews.get(texture.webview) else {
            continue;
        };
        let Some(mut material) = materials.get_mut(mat_handle.id()) else {
            continue;
        };
        let handle = material
            .extension
            .surface
            .get_or_insert_with(|| images.add(webview_placeholder_image()))
            .clone();
        commands
            .entity(texture.webview)
            .insert(WebviewSurface(handle.clone()));
        apply_webview_texture(texture, &mut images, &handle, &mut uploads);
        if logged.insert(texture.webview) {
            webview_debug_log(format!(
                "texture applied entity={:?} size={}x{} bytes={}",
                texture.webview,
                texture.width,
                texture.height,
                texture.buffer.len()
            ));
        }
    }
}
