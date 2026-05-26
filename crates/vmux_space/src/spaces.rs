use std::path::{Path, PathBuf};

use bevy::{picking::Pickable, prelude::*};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::profile;
use vmux_layout::cef::Browser;

use crate::event::{SPACES_PAGE_URL, SpaceRow};
use crate::model::{
    SpaceRecord, SpaceRegistry, bootstrap_space_record, registry_path, space_layout_path_for,
};

#[derive(Resource, Clone, Debug)]
pub struct ActiveSpace {
    pub record: SpaceRecord,
}

impl Default for ActiveSpace {
    fn default() -> Self {
        let registry = read_space_registry_from(&profile::shared_data_dir());
        let record = registry
            .spaces
            .first()
            .cloned()
            .unwrap_or_else(bootstrap_space_record);
        Self { record }
    }
}

impl ActiveSpace {
    pub fn layout_path(&self) -> PathBuf {
        space_layout_path_for(
            &profile::shared_data_dir(),
            &self.record.id,
            &self.record.profile,
        )
    }
}

pub fn read_space_registry_from(root: &Path) -> SpaceRegistry {
    let mut registry = std::fs::read_to_string(registry_path(root))
        .ok()
        .and_then(|body| ron::de::from_str::<SpaceRegistry>(&body).ok())
        .unwrap_or_default();
    if registry.spaces.is_empty() {
        registry.spaces.push(bootstrap_space_record());
    }
    registry
}

pub fn space_profile_bundle(record: &SpaceRecord) -> impl Bundle {
    (
        vmux_layout::space::Space,
        vmux_layout::profile::Profile {
            name: record.profile.clone(),
        },
        Name::new(record.name.clone()),
    )
}

pub fn registry_space_summaries() -> Vec<(String, String, String)> {
    let registry = read_space_registry_from(&profile::shared_data_dir());
    registry
        .spaces
        .into_iter()
        .map(|space| (space.id, space.name, space.profile))
        .collect()
}

pub fn active_space_rows(active: &ActiveSpace, active_stack_count: usize) -> Vec<SpaceRow> {
    let registry = read_space_registry_from(&profile::shared_data_dir());
    registry
        .spaces
        .into_iter()
        .map(|space| {
            let is_active = space.id == active.record.id;
            SpaceRow {
                id: space.id.clone(),
                name: space.name.clone(),
                profile: space.profile.clone(),
                is_active,
                tab_count: if is_active {
                    active_stack_count as u32
                } else {
                    0
                },
            }
        })
        .collect()
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
                    favicon_url: String::new(),
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
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
    use crate::model::{BOOTSTRAP_PROFILE_NAME, BOOTSTRAP_SPACE_NAME, bootstrap_space_record};

    #[test]
    fn space_profile_bundle_spawns_space_name_and_profile_name() {
        let mut app = App::new();
        app.world_mut()
            .spawn(space_profile_bundle(&bootstrap_space_record()));

        let mut query = app
            .world_mut()
            .query_filtered::<(&Name, &vmux_layout::profile::Profile), With<vmux_layout::space::Space>>();
        let (name, profile) = query.single(app.world()).unwrap();

        assert_eq!(name.as_str(), BOOTSTRAP_SPACE_NAME);
        assert_eq!(profile.name, BOOTSTRAP_PROFILE_NAME);
    }
}
