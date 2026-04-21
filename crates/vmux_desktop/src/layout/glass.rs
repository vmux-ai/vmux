use crate::unit::PIXELS_PER_METER;
use bevy::{
    asset::{Asset, load_internal_asset, uuid_handle},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub struct GlassMaterialPlugin;

impl Plugin for GlassMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            GLASS_SHADER_HANDLE,
            "glass.wgsl",
            Shader::from_wgsl
        );

        app.add_plugins(MaterialPlugin::<GlassMaterial>::default());
    }
}

const GLASS_SHADER_HANDLE: Handle<Shader> = uuid_handle!("a3e43dbf-9f06-4d0b-8a17-ef8d5ad4d1f4");

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq)]
pub struct GlassCorners {
    #[uniform(100)]
    pub clip: Vec4,
    #[uniform(101)]
    pub corner_mode: Vec4,
}

impl Default for GlassCorners {
    fn default() -> Self {
        Self {
            clip: Vec4::new(0.0, 1.0, 1.0, PIXELS_PER_METER),
            corner_mode: Vec4::ZERO,
        }
    }
}

impl MaterialExtension for GlassCorners {
    fn fragment_shader() -> ShaderRef {
        GLASS_SHADER_HANDLE.into()
    }
}

pub type GlassMaterial = ExtendedMaterial<StandardMaterial, GlassCorners>;
