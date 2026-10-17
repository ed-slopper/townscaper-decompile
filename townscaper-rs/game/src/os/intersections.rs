//! `Os.Utils.Intersections` and `Os.Utils.NumberStrings` (row E1c). Checked against the running game by
//! `tests/os_intersections.rs` (`TS_HARNESS=1`). `PropPlacer` uses the oriented-box tests for props.

use crate::unity::mathf;
use crate::unity::quaternion::Matrix4x4;
use crate::unity::vector::{v3, Vector3};

/// `Intersections.corners` (`..cctor` 0x3EA3B0): the unit cube's corners at ±1, x slowest, each sign + before -
/// (read back from the running game). The boxes are matrices mapping this cube; the order matters for NaN and
/// for `PropCollider.CalculateWorldBounds`, whose `Bounds.Encapsulate` rounds through center ± extents.
pub const CORNERS: [Vector3; 8] = [
    v3(1.0, 1.0, 1.0),
    v3(1.0, 1.0, -1.0),
    v3(1.0, -1.0, 1.0),
    v3(1.0, -1.0, -1.0),
    v3(-1.0, 1.0, 1.0),
    v3(-1.0, 1.0, -1.0),
    v3(-1.0, -1.0, 1.0),
    v3(-1.0, -1.0, -1.0),
];

/// `(float.MaxValue, -float.MaxValue)` grown by `ExtraMath.Encapsulate` over the projections.
fn range(points: &[Vector3], axis: Vector3) -> (f32, f32) {
    points.iter().fold((f32::MAX, -f32::MAX), |(lo, hi), p| {
        let d = Vector3::dot(*p, axis);
        (mathf::min(lo, d), mathf::max(hi, d))
    })
}

/// Separated on this axis: one interval ends before the other begins.
fn apart(a: (f32, f32), b: (f32, f32)) -> bool {
    b.1 < a.0 || a.1 < b.0
}

fn columns(m: &Matrix4x4) -> [Vector3; 3] {
    [m.column(0).xyz(), m.column(1).xyz(), m.column(2).xyz()]
}

/// `Os.Utils.Intersections.BoxBox` 0x3E6E40: axis-aligned boxes overlap (touching counts).
pub fn box_box(min0: Vector3, max0: Vector3, min1: Vector3, max1: Vector3) -> bool {
    min1.x <= max0.x && min0.x <= max1.x && min1.y <= max0.y && min0.y <= max1.y && min1.z <= max0.z && min0.z <= max1.z
}

/// `Os.Utils.Intersections.OrientedBoxOrientedBox` 0x3E80B0 / `OrientedBoxOrientedBoxInternal` 0x3E6E90:
/// separating-axis test of two boxes (each a matrix mapping the ±1 cube) on box0's columns, box1's columns
/// and their nine cross products. The original skips axes it has tested already (a `HashSet<Vector3>`):
/// that never changes the answer.
pub fn oriented_box_oriented_box(box0: &Matrix4x4, box1: &Matrix4x4) -> bool {
    let c0: Vec<Vector3> = CORNERS.iter().map(|c| box0.multiply_point(*c)).collect();
    let c1: Vec<Vector3> = CORNERS.iter().map(|c| box1.multiply_point(*c)).collect();
    let (a, b) = (columns(box0), columns(box1));
    let axes = a.iter().chain(b.iter()).copied().chain(a.iter().flat_map(|x| b.iter().map(move |y| Vector3::cross(*x, *y))));
    for axis in axes {
        if apart(range(&c0, axis), range(&c1, axis)) {
            return false;
        }
    }
    true
}

/// `Os.Utils.Intersections.OrientedBoxTriangle` 0x3E8240: separating-axis test of a box and a triangle on the
/// box's columns, the triangle's edges, the triangle's normal (box corners measured from `p0`, against 0)
/// and the columns × edges.
pub fn oriented_box_triangle(bx: &Matrix4x4, p0: Vector3, p1: Vector3, p2: Vector3) -> bool {
    let corners: Vec<Vector3> = CORNERS.iter().map(|c| bx.multiply_point(*c)).collect();
    let tri = [p0, p1, p2];
    let cols = columns(bx);
    for axis in cols {
        if apart(range(&corners, axis), range(&tri, axis)) {
            return false;
        }
    }
    let edges = [p0.sub(p1), p1.sub(p2), p2.sub(p0)];
    for axis in edges {
        if apart(range(&corners, axis), range(&tri, axis)) {
            return false;
        }
    }
    let n = Vector3::cross(edges[1], edges[2]);
    let rel: Vec<Vector3> = corners.iter().map(|c| c.sub(p0)).collect();
    let r = range(&rel, n);
    if 0.0 < r.0 || r.1 < 0.0 {
        return false;
    }
    for col in cols {
        for e in edges {
            let axis = Vector3::cross(col, e);
            if apart(range(&corners, axis), range(&tri, axis)) {
                return false;
            }
        }
    }
    true
}

/// `Os.Utils.Intersections.TriangleRay` 0x3E95C0: Möller–Trumbore with `Mathf.Epsilon`; hits only in front
/// of the origin (`t > Epsilon`).
pub fn triangle_ray(p1: Vector3, p2: Vector3, p3: Vector3, origin: Vector3, direction: Vector3) -> bool {
    let eps = mathf::EPSILON;
    let e1 = p2.sub(p1);
    let e2 = p3.sub(p1);
    let h = Vector3::cross(direction, e2);
    let a = Vector3::dot(e1, h);
    if !(a <= -eps || eps <= a) {
        return false;
    }
    let f = 1.0 / a;
    let s = origin.sub(p1);
    let u = Vector3::dot(s, h) * f;
    if !(0.0 <= u && u <= 1.0) {
        return false;
    }
    let q = Vector3::cross(s, e1);
    let v = Vector3::dot(q, direction) * f;
    if !(0.0 <= v && v + u <= 1.0) {
        return false;
    }
    eps < Vector3::dot(q, e2) * f
}

/// `Os.Utils.Intersections.TriangleTriangleInternal` 0x3E98B0: `v` is beyond the plane of `p`, or beyond
/// one of its edge planes, by more than `min_distance` → false.
fn triangle_triangle_internal(p: [Vector3; 3], v: [Vector3; 3], min_distance: f32) -> bool {
    let a = p[0].sub(p[1]).normalized();
    let b = p[1].sub(p[2]).normalized();
    let n = Vector3::cross(a, b);
    let nn = v3(-n.x, -n.y, -n.z);
    let s = mathf::sign(Vector3::dot(v[0].sub(p[0]), nn));
    let nns = nn.mul(s);
    if v.iter().all(|q| min_distance < Vector3::dot(q.sub(p[0]), nns)) {
        return false;
    }
    let c = p[2].sub(p[0]).normalized();
    for (edge, from) in [(a, p[0]), (b, p[1]), (c, p[2])] {
        let side = Vector3::cross(edge, nn);
        if v.iter().all(|q| min_distance < Vector3::dot(q.sub(from), side)) {
            return false;
        }
    }
    true
}

/// `Os.Utils.Intersections.TriangleTriangle` 0x3E9F60: bounding boxes (grown by `min_distance`) overlap, and
/// neither triangle lies wholly outside the other's planes.
pub fn triangle_triangle(p: [Vector3; 3], v: [Vector3; 3], min_distance: f32) -> bool {
    let pmin = Vector3::min(p[0], Vector3::min(p[1], p[2]));
    let pmax = Vector3::max(p[0], Vector3::max(p[1], p[2]));
    let vmin = Vector3::min(v[0], Vector3::min(v[1], v[2]));
    let vmax = Vector3::max(v[0], Vector3::max(v[1], v[2]));
    let (lo, hi) = (pmin.sub(v3(min_distance, min_distance, min_distance)), pmax.add(v3(min_distance, min_distance, min_distance)));
    let boxes = vmin.x <= hi.x && lo.x <= vmax.x && vmin.y <= hi.y && lo.y <= vmax.y && vmin.z <= hi.z && lo.z <= vmax.z;
    boxes && triangle_triangle_internal(p, v, min_distance) && triangle_triangle_internal(v, p, min_distance)
}

/// `Os.Utils.NumberStrings.Get` 0x334970: `number.ToString()`, cached (invariant digits, `-` sign).
pub fn number_string(n: i32) -> String {
    n.to_string()
}
