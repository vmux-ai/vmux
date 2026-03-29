use crate::prelude::{WebviewSize, WebviewSource};
use crate::system_param::mesh_aabb::MeshAabb;
use bevy::camera::{NormalizedRenderTarget, RenderTarget};
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_picking::mesh_picking::ray_cast::{ray_mesh_intersection, Backfaces};
use std::fmt::Debug;

#[derive(SystemParam)]
pub struct WebviewPointer<'w, 's, C: Component = Camera3d> {
    aabb: MeshAabb<'w, 's>,
    cameras: Query<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            &'static RenderTarget,
        ),
        With<C>,
    >,
    primary_window: Query<'w, 's, Entity, With<PrimaryWindow>>,
    webviews: Query<
        'w,
        's,
        (&'static GlobalTransform, &'static WebviewSize),
        (With<WebviewSource>, Without<Camera>),
    >,
    parents: Query<'w, 's, (Option<&'static ChildOf>, Has<WebviewSource>)>,
    mesh3d: Query<'w, 's, &'static Mesh3d, With<WebviewSource>>,
    mesh_assets: Res<'w, Assets<Mesh>>,
}

impl<C: Component> WebviewPointer<'_, '_, C> {
    pub fn pos_from_trigger<P>(&self, trigger: &On<Pointer<P>>) -> Option<(Entity, Vec2)>
    where
        P: Clone + Reflect + Debug,
    {
        let webview = find_webview_entity(trigger.entity, &self.parents)?;
        let pos = self.pointer_pos(webview, trigger.pointer_location.position)?;
        Some((webview, pos))
    }

    pub fn pointer_pos(&self, webview: Entity, viewport_pos: Vec2) -> Option<Vec2> {
        let (min, max) = self.aabb.calculate_local(webview);
        let aabb_size = Vec2::new(max.x - min.x, max.y - min.y);
        let (webview_gtf, webview_size) = self.webviews.get(webview).ok()?;
        let tex_pixels = cef_pointer_pixel_extents(webview_size.0);

        let primary_entity = self.primary_window.single().ok();

        let mut best: Option<(isize, Vec2)> = None;
        for (camera, cam_gtf, target) in &self.cameras {
            if !camera.is_active {
                continue;
            }
            if let Some(pe) = primary_entity {
                let renders_primary = match target.normalize(Some(pe)) {
                    Some(NormalizedRenderTarget::Window(w)) => w.entity() == pe,
                    _ => false,
                };
                if !renders_primary {
                    continue;
                }
            }
            let Some(rect) = camera.logical_viewport_rect() else {
                continue;
            };
            if !rect.contains(viewport_pos) {
                continue;
            }
            let Some(ray) = camera.viewport_to_world(cam_gtf, viewport_pos).ok() else {
                continue;
            };
            let pos = cef_pixels_from_mesh_ray(
                ray,
                webview,
                webview_gtf,
                &self.mesh3d,
                &self.mesh_assets,
                tex_pixels,
            )
            .or_else(|| {
                pointer_to_webview_uv(
                    viewport_pos,
                    camera,
                    cam_gtf,
                    webview_gtf,
                    aabb_size,
                    tex_pixels,
                )
            });
            let Some(pos) = pos else {
                continue;
            };
            match best {
                None => best = Some((camera.order, pos)),
                Some((ord, _)) if camera.order > ord => best = Some((camera.order, pos)),
                _ => {}
            }
        }

        if let Some((_, pos)) = best {
            return Some(pos);
        }

        // Fallback when no primary window or no camera matched (e.g. headless / tests).
        self.cameras.iter().find_map(|(camera, cam_gtf, _)| {
            let ray = camera.viewport_to_world(cam_gtf, viewport_pos).ok()?;
            cef_pixels_from_mesh_ray(
                ray,
                webview,
                webview_gtf,
                &self.mesh3d,
                &self.mesh_assets,
                tex_pixels,
            )
            .or_else(|| {
                pointer_to_webview_uv(
                    viewport_pos,
                    camera,
                    cam_gtf,
                    webview_gtf,
                    aabb_size,
                    tex_pixels,
                )
            })
        })
    }

}

/// CEF mouse / click coordinates use the same **DIP / layout** space as `GetViewRect` / [`WebviewSize`],
/// not the physical `OnPaint` bitmap size when `device_scale_factor` ≠ 1.
#[inline]
fn cef_pointer_pixel_extents(layout_size: Vec2) -> Vec2 {
    layout_size
}

/// Map pointer ray → CEF pixel coordinates using **mesh UVs** (same as the GPU), then fall back
/// to analytical plane math if ray cast fails.
fn cef_pixels_from_mesh_ray(
    ray: Ray3d,
    webview: Entity,
    webview_gtf: &GlobalTransform,
    mesh3d: &Query<&Mesh3d, With<WebviewSource>>,
    mesh_assets: &Assets<Mesh>,
    tex_size: Vec2,
) -> Option<Vec2> {
    let mesh3d = mesh3d.get(webview).ok()?;
    let mesh = mesh_assets.get(mesh3d.0.id())?;
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let positions = mesh.try_attribute(Mesh::ATTRIBUTE_POSITION).ok()?.as_float3()?;
    let normals = mesh
        .try_attribute(Mesh::ATTRIBUTE_NORMAL)
        .ok()
        .and_then(|n| n.as_float3());
    let uvs = mesh
        .try_attribute(Mesh::ATTRIBUTE_UV_0)
        .ok()
        .and_then(|uvs| match uvs {
            VertexAttributeValues::Float32x2(u) => Some(u.as_slice()),
            _ => None,
        });
    let affine = webview_gtf.affine();
    let hit = match mesh.try_indices().ok()? {
        Indices::U16(i) => ray_mesh_intersection(
            ray,
            &affine,
            positions,
            normals,
            Some(i.as_slice()),
            uvs,
            Backfaces::Include,
        ),
        Indices::U32(i) => ray_mesh_intersection(
            ray,
            &affine,
            positions,
            normals,
            Some(i.as_slice()),
            uvs,
            Backfaces::Include,
        ),
    }?;
    let uv = hit.uv?;
    // `Plane3d` UV is `[tx, tz]` on the XZ patch before rotation; after `from_rotation_arc(Y, Z)`,
    // tz=0 maps to +world Y (top on screen) and tz=1 to −Y (bottom). CEF y is also top→down, so
    // use uv.y as-is (unlike `pointer_to_webview_uv`, where normalized `v` grows bottom→top).
    Some(Vec2::new(uv.x * tex_size.x, uv.y * tex_size.y))
}

fn find_webview_entity(
    entity: Entity,
    parents: &Query<(Option<&ChildOf>, Has<WebviewSource>)>,
) -> Option<Entity> {
    let (child_of, has_webview) = parents.get(entity).ok()?;
    if has_webview {
        return Some(entity);
    }
    if let Some(parent) = child_of {
        return find_webview_entity(parent.0, parents);
    }
    None
}

fn pointer_to_webview_uv(
    cursor_pos: Vec2,
    camera: &Camera,
    cam_tf: &GlobalTransform,
    plane_tf: &GlobalTransform,
    plane_size: Vec2,
    tex_size: Vec2,
) -> Option<Vec2> {
    let ray = camera.viewport_to_world(cam_tf, cursor_pos).ok()?;
    let n = plane_tf.forward().as_vec3();
    let t = ray.intersect_plane(
        plane_tf.translation(),
        InfinitePlane3d::new(plane_tf.forward()),
    )?;
    let hit_world = ray.origin + ray.direction * t;
    let local_hit = plane_tf.affine().inverse().transform_point(hit_world);
    let local_normal = plane_tf.affine().inverse().transform_vector3(n).normalize();
    let abs_normal = local_normal.abs();
    let (u_coord, v_coord) = if abs_normal.z > abs_normal.x && abs_normal.z > abs_normal.y {
        (local_hit.x, local_hit.y)
    } else if abs_normal.y > abs_normal.x {
        (local_hit.x, local_hit.z)
    } else {
        (local_hit.y, local_hit.z)
    };

    let w = plane_size.x;
    let h = plane_size.y;
    let u = (u_coord + w * 0.5) / w;
    let v = (v_coord + h * 0.5) / h;
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        // outside plane bounds
        return None;
    }
    let px = u * tex_size.x;
    let py = (1.0 - v) * tex_size.y;
    Some(Vec2::new(px, py))
}
