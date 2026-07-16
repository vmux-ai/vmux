use bevy::{picking::Pickable, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_layout::cef::Browser;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        BOOTSTRAP_SPACE_ID, BOOTSTRAP_SPACE_NAME, bootstrap_profile_name, bootstrap_space_record,
    };

    #[test]
    fn space_profile_bundle_spawns_space_name_profile_and_id() {
        let mut app = App::new();
        app.world_mut()
            .spawn(space_profile_bundle(&bootstrap_space_record()));

        let mut query = app.world_mut().query_filtered::<(
            &Name,
            &vmux_layout::profile::Profile,
            &vmux_layout::space::SpaceId,
        ), With<vmux_layout::space::Space>>();
        let (name, profile, space_id) = query.single(app.world()).unwrap();

        assert_eq!(name.as_str(), BOOTSTRAP_SPACE_NAME);
        assert_eq!(profile.name, bootstrap_profile_name());
        assert_eq!(space_id.0, BOOTSTRAP_SPACE_ID);
    }
}
