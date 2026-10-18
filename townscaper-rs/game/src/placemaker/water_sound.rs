//! `Placemaker.WaterSound` (row G6b): the water's sound sits on the shore, at a vertex of the sandbank's outline.

use crate::unity::vector::{v3, Vector3};

/// `WaterSound.Update` 0x3D5A60: the point it moves to among `master.sandbanker.verts`, from the camera's
/// position. Meant as the nearest vertex, but each vertex is taken when the camera is nearer the point taken so
/// far than the best distance yet (the distance is measured to the current best, not to the vertex): from the
/// last vertex, the first is taken; then one whenever that distance shrinks. With no vertices it is the origin.
pub fn update(camera: Vector3, verts: &[Vector3]) -> Vector3 {
    let mut best = Vector3::ZERO;
    let mut best_sq = f32::MAX;
    for &v in verts.iter().rev() {
        let sq = v3(camera.x - best.x, camera.y - best.y, camera.z - best.z).sqr_magnitude();
        if sq < best_sq {
            best = v;
            best_sq = sq;
        }
    }
    best
}
