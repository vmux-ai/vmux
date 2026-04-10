use crate::{
    rounded::{RoundedCorners, RoundedMaterial},
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{prelude::*, window::PrimaryWindow};

pub struct Layout3Plugin;

impl Plugin for Layout3Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (setup, fit_display_glass_to_window).after(load_settings),
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
}

#[derive(Component)]
pub struct DisplayGlass;

fn setup(
    window: Single<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
) {
    let m = window.meters();

    commands.spawn(DisplayGlassBundle {
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
    });
}

pub fn fit_display_glass_to_window(
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

        // Sync Shader Uniforms
        if let Some(mat) = materials.get_mut(handle) {
            mat.extension.clip = Vec4::new(r, m.x, m.y, PIXELS_PER_METER);
        }
    }
}

// fn on_display_panel_primary_press_clear_capture(
//     trigger: On<Pointer<Press>>,
//     mut focus: ResMut<WebviewKeyboardFocus>,
// ) {
//     if trigger.button != PointerButton::Primary {
//         return;
//     }
//     focus.sticky = false;
// }
