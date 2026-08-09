use bevy::{picking::Pickable, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_layout::cef::Browser;
use vmux_layout::warm_page::WarmPage;

use crate::event::SPACES_PAGE_URL;
use crate::model::SpaceRecord;

#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveSpace {
    pub record: SpaceRecord,
}

pub fn space_profile_bundle(record: &SpaceRecord) -> impl Bundle {
    (
        vmux_layout::space::Space,
        vmux_layout::space::SpaceId(record.id.clone()),
        vmux_layout::profile::Profile {
            name: record.profile.clone(),
        },
        Name::new(record.name.clone()),
    )
}

#[derive(Component)]
pub struct Spaces;

impl Spaces {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SPACES_PAGE_URL),
                ResolvedWebviewUri(SPACES_PAGE_URL.to_string()),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_PAGE_URL.to_string(),
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Inherited,
                Pickable::default(),
            ),
        )
    }
}

impl WarmPage for Spaces {
    const HOST: &'static str = "spaces";
    const URL: &'static str = SPACES_PAGE_URL;
    const TITLE: &'static str = "Spaces";

    fn spawn(
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> Entity {
        commands.spawn(Spaces::new(meshes, webview_mt)).id()
    }
}

#[cfg(test)]
#[path = "spaces.test.rs"]
mod tests;
