//! `Placemaker.SaveCamera` (row G2): the camera that takes a save's thumbnail. It turns to the saved camera
//! turn at a 15° tilt, looks at the middle of the town's bounds, backs off until every corner column's lowest
//! and highest voxel fits a 16° view, and renders 512 × 512 with the line, shading and dim globals set for a
//! clean picture. What it renders with (render textures, blit, `ReadPixels`, JPEG at quality 90) is the
//! host's; [`SaveCamera::get_image`] places the camera and says how to render.
//!
//! Also `CustomCameraProjection` (global namespace): keeps a camera's projection matrix as it was at `Start`.

use crate::placemaker::graphs::Graph;
use crate::placemaker::grid_generator;
use crate::placemaker::timing::shader_ids;
use crate::os::hex;
use crate::unity::geometry::Bounds;
use crate::unity::mathematics::f3;
use crate::unity::mathf;
use crate::unity::quaternion::{Matrix4x4, Quaternion};
use crate::unity::transform::{TransformId, Transforms};
use crate::unity::vector::{v3, Vector3};

/// `SaveCamera.saveTexSize`.
pub const SAVE_TEX_SIZE: i32 = 512;
/// `SaveCamera.jpgQuality`.
pub const JPG_QUALITY: i32 = 90;

/// How the host renders the thumbnail (`GetImage` after the camera is placed): `cam` at `field_of_view` into a
/// `render_size` texture (depth 24, 8× antialiasing) with `globals` set, blitted to a `blit_size` one (the same
/// settings), the globals put back as they were, the blit read into the 512 × 512 `bigTex` and encoded as JPEG.
#[derive(Clone, Debug, PartialEq)]
pub struct ThumbnailRender {
    pub field_of_view: f32,
    pub render_size: (i32, i32),
    pub blit_size: (i32, i32),
    pub globals: [(&'static str, [f32; 4]); 3],
    pub jpg_quality: i32,
}

/// The component (its scene references, `master`, `cam`, `bigTex`, are the host's).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SaveCamera;

impl SaveCamera {
    /// `SaveCamera.GetImage(saveData)` 0x965000, the placing: `Euler(15, cam.y / 255 * 360, 0)` at the bounds'
    /// middle (`PlaneToWorld(HexToPlane(centre), boundsHeight / 2)`); then, in the camera's frame
    /// (`worldToLocalMatrix`), a box from the origin round `points`; the camera backs off from the middle by
    /// `Max((extents.x + 1) * h / w, (extents.y + 1) * 1.1) * 2 / tan(16°)`, offset by the box's centre.
    /// `points` are, for each child of the graph's corners container from the last to the first, the
    /// position of its first child and, when it has more than one, of its last (the column's voxels).
    /// `cam` is the camera's transform in `tr`, which this moves.
    pub fn get_image(&self, graph: &Graph, cam_y: u8, tr: &mut Transforms, cam: TransformId, points: &[Vector3]) -> ThumbnailRender {
        let (w, h) = (SAVE_TEX_SIZE, SAVE_TEX_SIZE);
        let rot = Quaternion::euler(15.0, cam_y as i32 as f32 / 255.0 * 360.0, 0.0);
        let b = graph.bounds;
        let centre = f3(
            (b.c0.x as f32 + b.c1.x as f32) * 0.5,
            (b.c0.y as f32 + b.c1.y as f32) * 0.5,
            (b.c0.z as f32 + b.c1.z as f32) * 0.5,
        );
        let p = grid_generator::hex_to_plane(centre);
        let mid = hex::plane_to_world(p, graph.bounds_height as f32 * 0.5);
        let mid = v3(mid.x, mid.y, mid.z);
        tr.set_position(cam, mid);
        tr.set_rotation(cam, rot);
        let to_local: Matrix4x4 = tr.world_to_local_matrix(cam);
        let mut bounds = Bounds::new(Vector3::ZERO, Vector3::ZERO);
        let fov = 16.0;
        let ratio = h as f32 / w as f32;
        let k = 2.0 / ((0.27925267815589905f64).tan() as f32);
        for &q in points {
            bounds.encapsulate(to_local.multiply_point(q));
        }
        let e = bounds.extents;
        let dist = mathf::max((e.x + 1.0) * ratio, (e.y + 1.0) * 1.1);
        let c = bounds.center;
        let neg = f32::from_bits(std::hint::black_box((dist * k).to_bits()) ^ 0x8000_0000);
        let off = Quaternion::rotate(rot, v3(c.x + 0.0, c.y + 0.0, neg + c.z));
        tr.set_position(cam, v3(mid.x + off.x, mid.y + off.y, mid.z + off.z));
        let w2 = mathf::next_power_of_two(w);
        let h2 = mathf::next_power_of_two(h);
        ThumbnailRender {
            field_of_view: fov,
            render_size: (w2 * 2, h2 * 2),
            blit_size: (w2, h2),
            globals: [
                (shader_ids::AA_LINE_BOTH, [2.0, 0.5, 1.0, 4.0]),
                (shader_ids::SHADING_TEX_LIGHT_STACK, [1.0, 1.0, 0.0, 0.0]),
                (shader_ids::DIM, [0.0, 1.0, 0.0, 0.0]),
            ],
            jpg_quality: JPG_QUALITY,
        }
    }

    /// The thumbnail's `points` (see [`Self::get_image`]) from the corner columns' voxel positions, in the
    /// corners container's child order.
    pub fn column_points(columns: &[Vec<Vector3>]) -> Vec<Vector3> {
        let mut out = vec![];
        for col in columns.iter().rev() {
            if let Some(&first) = col.first() {
                out.push(first);
                if col.len() != 1 {
                    out.push(*col.last().unwrap());
                }
            }
        }
        out
    }
}

/// `CustomCameraProjection` (global namespace): `Start` 0x4485C0 keeps the camera's projection matrix,
/// `Update` 0x448640 sets it back every frame.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CustomCameraProjection {
    pub original_projection: Matrix4x4,
}

impl CustomCameraProjection {
    /// `Start`: the camera's current projection.
    pub fn start(projection: Matrix4x4) -> CustomCameraProjection {
        CustomCameraProjection { original_projection: projection }
    }
    /// `Update`: the projection to set on the camera.
    pub fn update(&self) -> Matrix4x4 {
        self.original_projection
    }
}
