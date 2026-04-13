use crate::{
    browser::{browser_bundle, Browser},
    layout::pane::{Active, Pane, PaneSplit, leaf_pane_bundle},
    layout::rounded::{RoundedCorners, RoundedMaterial},
    layout::side_sheet::SideSheet,
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use vmux_webview_app::WebviewAppEmbedSet;
use bevy::{
    prelude::*,
    render::alpha::AlphaMode,
    ui::{FlexDirection, UiTargetCamera},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use vmux_header::{HEADER_HEIGHT_PX, HEADER_WEBVIEW_URL, Header, HeaderBundle};

pub(crate) const WEBVIEW_Z_MAIN: f32 = 0.12;
pub(crate) const WEBVIEW_Z_OUTLINE: f32 = 0.13;
pub(crate) const WEBVIEW_Z_HEADER: f32 = 0.125;
pub(crate) const WEBVIEW_Z_SIDE_SHEET: f32 = 0.125;
pub(crate) const WEBVIEW_MESH_DEPTH_BIAS: f32 = -4.0;

pub(crate) struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (setup, fit_display_glass_to_window)
                .chain()
                .after(load_settings)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
        )
        .add_systems(PostUpdate, fit_display_glass_to_window);
    }
}

#[derive(Bundle)]
struct DisplayGlassBundle<M>
where
    M: Material,
{
    marker: DisplayGlass,
    mesh: Mesh3d,
    material: MeshMaterial3d<M>,
    transform: Transform,
    node: Node,
    ui_target: UiTargetCamera,
}

#[derive(Component)]
pub(crate) struct DisplayGlass;

fn setup(
    window: Single<&Window, With<PrimaryWindow>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    main_camera: Single<Entity, With<MainCamera>>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let m = window.meters();
    let pw = *primary_window;
    let startup_url = settings.browser.startup_url.as_str();

    commands.spawn((
        DisplayGlassBundle {
            marker: DisplayGlass,
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(materials.add(RoundedMaterial {
                base: StandardMaterial {
                    base_color: Color::srgba(0.08, 0.08, 0.08, 0.4),
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    perceptual_roughness: 0.12,
                    metallic: 0.0,
                    specular_transmission: 0.9,
                    diffuse_transmission: 1.0,
                    thickness: 0.1,
                    ior: 1.5,
                    ..default()
                },
                extension: RoundedCorners {
                    clip: Vec4::new(settings.layout.pane.radius, m.x, m.y, PIXELS_PER_METER),
                    ..default()
                },
            })),
            transform: Transform {
                translation: Vec3::new(0.0, m.y * 0.5, 0.0),
                scale: Vec3::new(m.x, m.y, 1.0),
                ..default()
            },
            node: Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Relative,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(settings.layout.window.padding)),
                row_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        },
        children![
            (
                SideSheet,
                HostWindow(pw),
                Browser,
                Node {
                    width: Val::Px(settings.layout.side_sheet.width),
                    flex_shrink: 0.0,
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    left: Val::Px(settings.layout.window.padding),
                    top: Val::Px(settings.layout.window.padding),
                    bottom: Val::Px(settings.layout.window.padding),
                    ..default()
                },
                ZIndex(2),
                WebviewSource::new("vmux://side-sheet/"),
                Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
                WebviewSize(Vec2::new(settings.layout.side_sheet.width, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Hidden,
            ),
            (
                ZIndex(1),
                HostWindow(pw),
                Browser,
                Node {
                    height: Val::Px(HEADER_HEIGHT_PX),
                    flex_shrink: 0.0,
                    ..default()
                },
                HeaderBundle {
                    marker: Header,
                    source: WebviewSource::new(HEADER_WEBVIEW_URL),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                    material: MeshMaterial3d(webview_mt.add(
                        WebviewExtendStandardMaterial {
                            base: StandardMaterial {
                                unlit: true,
                                alpha_mode: AlphaMode::Blend,
                                depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                                ..default()
                            },
                            ..default()
                        },
                    )),
                    webview_size: WebviewSize(Vec2::new(1280.0, HEADER_HEIGHT_PX)),
                },
            ),
            (
                Pane,
                PaneSplit,
                HostWindow(pw),
                ZIndex(0),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    column_gap: Val::Px(settings.layout.pane.gap),
                    row_gap: Val::Px(settings.layout.pane.gap),
                    ..default()
                },
                children![(
                    leaf_pane_bundle(),
                    Active,
                    children![browser_bundle(
                        &mut meshes,
                        &mut webview_mt,
                        startup_url
                    )],
                )],
            ),
        ],
    ));
}

pub(crate) fn fit_display_glass_to_window(
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<RoundedMaterial>), With<DisplayGlass>>,
) {
    let m = window.meters();
    if (m.x - last_size.x).abs() < 0.001 && (m.y - last_size.y).abs() < 0.001 {
        return;
    }
    *last_size = m;

    let r = settings.layout.pane.radius;

    for (mut tf, handle) in &mut q {
        tf.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        tf.scale = Vec3::new(m.x, m.y, 1.0);

        if let Some(mat) = materials.get_mut(handle) {
            mat.extension.clip = Vec4::new(r, m.x, m.y, PIXELS_PER_METER);
        }
    }
}
