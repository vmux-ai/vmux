use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenTask};

use crate::cef::Browser;

pub const DEBUG_PAGE_URL: &str = "vmux://debug/";

#[derive(Component)]
pub struct DebugView;

impl DebugView {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(DEBUG_PAGE_URL),
                ResolvedWebviewUri(DEBUG_PAGE_URL.to_string()),
                PageMetadata {
                    title: "Debug".to_string(),
                    url: DEBUG_PAGE_URL.to_string(),
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

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

pub fn handle_debug_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if task.url != DEBUG_PAGE_URL {
            continue;
        }
        if let Ok(children) = children_q.get(task.stack) {
            for child in children.iter() {
                commands.entity(child).try_despawn();
            }
        }
        commands.entity(task.stack).insert(PageMetadata {
            title: "Debug".to_string(),
            url: DEBUG_PAGE_URL.to_string(),
            favicon_url: String::new(),
            bg_color: None,
        });
        commands.spawn((
            DebugView::new(&mut meshes, &mut webview_mt),
            ChildOf(task.stack),
        ));
        commands.entity(entity).insert(PageOpenHandled);
    }
}
