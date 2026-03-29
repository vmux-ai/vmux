use crate::common::WebviewSource;
use crate::prelude::{WebviewMaterial, WebviewSurface, update_webview_image, webview_placeholder_image};
use bevy::app::Plugin;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy_cef_core::prelude::*;
use std::hash::Hash;
use std::marker::PhantomData;

pub type WebviewExtendedMaterial<E> = ExtendedMaterial<WebviewMaterial, E>;

fn ensure_extended_webview_placeholder<E: MaterialExtension>(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WebviewExtendedMaterial<E>>>,
    webviews: Query<
        (Entity, &MeshMaterial3d<WebviewExtendedMaterial<E>>),
        With<WebviewSource>,
    >,
) {
    for (entity, mesh_mat) in &webviews {
        let Some(mat) = materials.get_mut(mesh_mat.id()) else {
            continue;
        };
        if mat.base.surface.is_some() {
            continue;
        }
        let handle = images.add(webview_placeholder_image());
        mat.base.surface = Some(handle.clone());
        commands.entity(entity).insert(WebviewSurface(handle));
    }
}

/// A plugin that extends the [`WebviewMaterial`] with a custom material extension.
///
/// This plugin allows you to create custom materials that can be used with webviews.
pub struct WebviewExtendMaterialPlugin<E>(PhantomData<E>);

impl<E> Default for WebviewExtendMaterialPlugin<E>
where
    E: MaterialExtension + Default,
    <E as AsBindGroup>::Data: PartialEq + Eq + Hash + Clone + Copy,
{
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<E> Plugin for WebviewExtendMaterialPlugin<E>
where
    E: MaterialExtension + AsBindGroup<Data: PartialEq + Eq + Hash + Clone + Copy> + Default,
{
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WebviewExtendedMaterial<E>>::default())
            .add_systems(PreUpdate, ensure_extended_webview_placeholder::<E>)
            .add_systems(PostUpdate, render::<E>);
    }
}

fn render<E: MaterialExtension>(
    mut commands: Commands,
    mut er: MessageReader<RenderTextureMessage>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WebviewExtendedMaterial<E>>>,
    webviews: Query<&MeshMaterial3d<WebviewExtendedMaterial<E>>>,
) {
    for texture in er.read() {
        if let Ok(handle) = webviews.get(texture.webview)
            && let Some(material) = materials.get_mut(handle.id())
            && let Some(image) = {
                let handle = material
                    .base
                    .surface
                    .get_or_insert_with(|| images.add(webview_placeholder_image()));
                commands
                    .entity(texture.webview)
                    .insert(WebviewSurface(handle.clone()));
                images.get_mut(handle.id())
            }
        {
            //OPTIMIZE: Avoid cloning the texture.
            update_webview_image(texture.clone(), image);
        }
    }
}
