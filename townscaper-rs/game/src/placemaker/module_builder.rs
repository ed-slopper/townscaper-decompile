//! `Placemaker.ModuleBuilder` (row E7): turns the collapsed qubes' modules into mesh data. Row E7a: the geometry
//! that bends a module's vertices into a square (its four corner matrices, blended bilinearly). Checked against
//! the running game by `tests/placemaker_module_builder.rs` (`TS_HARNESS=1`).

use crate::placemaker::graphs::Square;
use crate::unity::vector::{v3, v4, Vector3, Vector4};

/// `Placemaker.ModuleBuilder.GetTs` 0x32FE30: the bilinear weights of the square's four corners for a vertex
/// at `(x, z)` in the unit square: `((1 - z)(1 - x), (1 - z)x, zx, (1 - x)z)`.
pub fn get_ts(src_vert: Vector3) -> Vector4 {
    let (x, z) = (src_vert.x, src_vert.z);
    v4((1.0 - z) * (1.0 - x), (1.0 - z) * x, z * x, (1.0 - x) * z)
}

/// The weights `ts` applied to the four corners' results: `((a·x + b·y) + c·z) + d·w` per component.
fn blend(ts: Vector4, r: [Vector3; 4]) -> Vector3 {
    let c = |f: fn(&Vector3) -> f32| ((f(&r[0]) * ts.x + f(&r[1]) * ts.y) + f(&r[2]) * ts.z) + f(&r[3]) * ts.w;
    v3(c(|v| v.x), c(|v| v.y), c(|v| v.z))
}

/// `Placemaker.ModuleBuilder.MultiplyPoint` 0x3302D0: a module vertex into the square: `(0, y, 0)` through each
/// corner matrix (`Matrix4x4.MultiplyPoint`), blended by `ts` (the vertex's x and z only enter through `ts`).
pub fn multiply_point(square: &Square, ts: Vector4, vector: Vector3) -> Vector3 {
    let p = v3(0.0, vector.y, 0.0);
    blend(ts, square.m.map(|m| m.multiply_point(p)))
}

/// `Placemaker.ModuleBuilder.MultiplyVector` 0x3304E0: a direction (normal, tangent) through each corner matrix
/// (`Matrix4x4.MultiplyVector`), blended by `ts`.
pub fn multiply_vector(square: &Square, ts: Vector4, vector: Vector3) -> Vector3 {
    blend(ts, square.m.map(|m| m.multiply_vector(vector)))
}
