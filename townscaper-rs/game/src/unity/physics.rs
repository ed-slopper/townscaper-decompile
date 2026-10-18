//! `UnityEngine.Physics.Raycast` (row E12) against the colliders the game picks through: the voxels' convex
//! `MeshCollider`s (the corner's collider prism) and the graph's ground `BoxCollider`. Unity hands the query to
//! PhysX 4.1 inside UnityPlayer.dll; the PhysX routines it reaches (`raycast_convexMesh` 0x10F97660,
//! `raycast_box` 0x10F96910, `Gu::rayAABBIntersect2` 0x10FBF000, `PxTransform` / `PxQuat` maths) are
//! transcribed here in the engine's operation order. Checked against the running game by
//! `tests/unity_physics.rs` (`TS_HARNESS=1`).

use super::quaternion::Quaternion;
use super::vector::{v3, Vector3};

/// A rigid pose (`physx::PxTransform`): rotation, then translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub q: Quaternion,
    pub p: Vector3,
}

/// `PxQuat::rotate`: `v' = 2v`, `w2 = w² - 0.5`, `dot2 = (x·v'x + y·v'y) + z·v'z`;
/// `x = (((y·v'z - z·v'y)·w) + w2·v'x) + x·dot2`, and so on.
fn rotate(q: Quaternion, v: Vector3) -> Vector3 {
    let (vx, vy, vz) = (v.x * 2.0, v.y * 2.0, v.z * 2.0);
    let w2 = q.w * q.w - 0.5;
    let dot2 = (q.x * vx + q.y * vy) + q.z * vz;
    v3(
        ((q.y * vz - q.z * vy) * q.w + w2 * vx) + q.x * dot2,
        ((q.z * vx - q.x * vz) * q.w + w2 * vy) + q.y * dot2,
        ((q.x * vy - q.y * vx) * q.w + w2 * vz) + q.z * dot2,
    )
}

/// `PxQuat::rotateInv`: `x = (w2·v'x - (y·v'z - z·v'y)·w) + x·dot2`, and so on.
fn rotate_inv(q: Quaternion, v: Vector3) -> Vector3 {
    let (vx, vy, vz) = (v.x * 2.0, v.y * 2.0, v.z * 2.0);
    let w2 = q.w * q.w - 0.5;
    let dot2 = (q.x * vx + q.y * vy) + q.z * vz;
    v3(
        (w2 * vx - (q.y * vz - q.z * vy) * q.w) + q.x * dot2,
        (w2 * vy - (q.z * vx - q.x * vz) * q.w) + q.y * dot2,
        (w2 * vz - (q.x * vy - q.y * vx) * q.w) + q.z * dot2,
    )
}

fn neg(x: f32) -> f32 {
    f32::from_bits(x.to_bits() ^ 0x8000_0000)
}

/// `PxMat33(PxQuat)`: the rotation's columns (`x2 = x + x`, `1 - yy - zz`, ...).
fn rot_mat(q: Quaternion) -> [Vector3; 3] {
    let (x2, y2, z2) = (q.x + q.x, q.y + q.y, q.z + q.z);
    let (xx, yy, zz) = (x2 * q.x, y2 * q.y, z2 * q.z);
    let (xy, xz, xw) = (x2 * q.y, x2 * q.z, x2 * q.w);
    let (yz, yw, zw) = (y2 * q.z, y2 * q.w, z2 * q.w);
    [v3((1.0 - yy) - zz, xy + zw, xz - yw), v3(xy - zw, (1.0 - xx) - zz, yz + xw), v3(xz + yw, yz - xw, (1.0 - xx) - yy)]
}

/// A plane of a convex hull: `n · x + d`, positive outside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullPlane {
    pub n: Vector3,
    pub d: f32,
}

/// A cooked convex mesh's polygons (`Gu::ConvexHullData` `HullPolygonData::mPlane`, in polygon order).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ConvexHull {
    pub planes: Vec<HullPlane>,
}

impl ConvexHull {
    /// STANDIN: PhysX cooking (`PxCooking::createConvexMesh`: quickhull, polygon merging, plane fitting) is not
    /// ported; the hull of the points is found by brute force (every plane through three points with all points
    /// behind it, coplanar ones merged), each polygon's vertices ordered round its normal from the lowest index,
    /// and its plane fitted by Newell's method (`n` the normalised Newell sums, `d = -(centroid · n)`).
    pub fn from_points(input: &[Vector3]) -> ConvexHull {
        // `ConvexHullLib::cleanupVertices` 0x10E44500: normalised by the bounds' size and scaled back
        let (mut lo, mut hi) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(-f32::MAX, -f32::MAX, -f32::MAX));
        for p in input {
            lo = v3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = v3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        let dim = v3(hi.x - lo.x, hi.y - lo.y, hi.z - lo.z);
        let recip = v3(1.0 / dim.x, 1.0 / dim.y, 1.0 / dim.z);
        let mut points: Vec<Vector3> = vec![];
        for p in input {
            let q = v3(p.x * recip.x, p.y * recip.y, p.z * recip.z);
            if !points.iter().any(|v| (q.x - v.x).abs() < 1e-4 && (q.y - v.y).abs() < 1e-4 && (q.z - v.z).abs() < 1e-4) {
                points.push(q);
            }
        }
        for q in &mut points {
            *q = v3(dim.x * q.x, q.y * dim.y, q.z * dim.z);
        }
        let points = &points[..];
        let eps = 1e-5f32;
        let n = points.len();
        let mut polys: Vec<(Vector3, Vec<usize>)> = vec![];
        for a in 0..n {
            for b in a + 1..n {
                for c in b + 1..n {
                    let (pa, pb, pc) = (points[a], points[b], points[c]);
                    let nrm = Vector3::cross(pb.sub(pa), pc.sub(pa));
                    let len = nrm.magnitude();
                    if len < 1e-9 {
                        continue;
                    }
                    let mut nrm = nrm.div(len);
                    let mut above = 0;
                    let mut below = 0;
                    for p in points {
                        let s = Vector3::dot(nrm, p.sub(pa));
                        if s > eps {
                            above += 1;
                        } else if s < -eps {
                            below += 1;
                        }
                    }
                    if above > 0 && below > 0 {
                        continue;
                    }
                    if above > 0 {
                        nrm = v3(-nrm.x, -nrm.y, -nrm.z);
                    }
                    if polys.iter().any(|(m, _)| Vector3::dot(*m, nrm) > 1.0 - 1e-6) {
                        continue;
                    }
                    let on: Vec<usize> = (0..n).filter(|&i| Vector3::dot(nrm, points[i].sub(pa)).abs() <= eps).collect();
                    polys.push((nrm, on));
                }
            }
        }
        let planes = polys
            .into_iter()
            .map(|(nrm, mut idx)| {
                // round the normal (counter-clockwise seen from outside), from the lowest index
                let mut c = Vector3::ZERO;
                for &i in &idx {
                    c = c.add(points[i]);
                }
                let c = c.div(idx.len() as f32);
                let u = points[idx[0]].sub(c).normalized();
                let w = Vector3::cross(nrm, u);
                let ang = |i: usize| {
                    let d = points[i].sub(c);
                    let a = Vector3::dot(d, w).atan2(Vector3::dot(d, u));
                    if a < 0.0 {
                        a + std::f32::consts::TAU
                    } else {
                        a
                    }
                };
                idx.sort_by(|&a, &b| ang(a).partial_cmp(&ang(b)).unwrap());
                // only the face's corners: points inside it or on a straight edge are not hull vertices
                loop {
                    let n = idx.len();
                    let bad = (0..n).find(|&k| {
                        let (a, b, c) = (points[idx[(k + n - 1) % n]], points[idx[k]], points[idx[(k + 1) % n]]);
                        Vector3::dot(Vector3::cross(b.sub(a), c.sub(b)), nrm) <= 1e-7
                    });
                    match bad {
                        Some(k) if n > 3 => {
                            idx.remove(k);
                        }
                        _ => break,
                    }
                }
                let k = idx.iter().enumerate().min_by_key(|(_, &v)| v).map(|(k, _)| k).unwrap();
                idx.rotate_left(k);
                fan_plane(points, &idx)
            })
            .collect();
        ConvexHull { planes }
    }
}

/// A quickhull face's plane (`QuickHullFace::computeNormalAndCentroid`): from the longest of the first three
/// edges, `d = v1 - v0` crossed with each later vertex's offset from `v0`, summed and normalised (`1 / sqrtf`);
/// the centroid the vertices' sum times `1 / n`; the plane's offset `-(n · centroid)`.
fn fan_plane(points: &[Vector3], idx: &[usize]) -> HullPlane {
    let n = idx.len();
    let at = |k: usize| points[idx[k % n]];
    let mut start = 0;
    let mut best = 0.0f32;
    for k in 0..3 {
        let e = at(k).sub(at(k + 1));
        let d = (e.x * e.x + e.y * e.y) + e.z * e.z;
        if d > best {
            best = d;
            start = k;
        }
    }
    let p0 = at(start);
    let d = at(start + 1).sub(p0);
    let mut c = p0;
    let mut nrm = Vector3::ZERO;
    for k in 1..n {
        c = c.add(at(start + k));
        let e = at(start + k + 1).sub(p0);
        let x = v3(d.y * e.z - d.z * e.y, d.z * e.x - d.x * e.z, d.x * e.y - d.y * e.x);
        nrm = nrm.add(x);
    }
    let m = ((nrm.x * nrm.x + nrm.y * nrm.y) + nrm.z * nrm.z).sqrt();
    if m > 0.0 {
        let f = 1.0 / m;
        nrm = v3(nrm.x * f, nrm.y * f, nrm.z * f);
    }
    let f = 1.0 / n as f32;
    let c = v3(c.x * f, c.y * f, c.z * f);
    HullPlane { n: nrm, d: neg((nrm.x * c.x + nrm.y * c.y) + nrm.z * c.z) }
}

/// Newell's plane through a polygon (`computeNewellPlane` in PhysX cooking).
#[allow(dead_code)]
fn newell_plane(points: &[Vector3], idx: &[usize]) -> HullPlane {
    let mut n = Vector3::ZERO;
    let mut c = Vector3::ZERO;
    let mut i = idx.len() - 1;
    for j in 0..idx.len() {
        let (a, b) = (points[idx[i]], points[idx[j]]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
        c = c.add(b);
        i = j;
    }
    let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    let n = if len > 0.0 { n.mul(1.0 / len) } else { n };
    HullPlane { n, d: -(Vector3::dot(c, n)) / idx.len() as f32 }
}

/// A collider's shape in PhysX terms.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A convex `MeshCollider` and its `PxMeshScale` (the transform's lossy scale, no scale rotation).
    Convex { hull: ConvexHull, scale: Vector3 },
    /// A `BoxCollider`: its half extents (the shape's pose carries the centre).
    Box { half_extents: Vector3 },
}

/// One collider in the scene: its shape's world pose, the shape, its GameObject's layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    pub pose: Pose,
    pub shape: Shape,
    pub layer: i32,
}

/// `UnityEngine.RaycastHit` (the PhysX hit as Unity copies it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaycastHit {
    pub point: Vector3,
    pub normal: Vector3,
    pub face_id: u32,
    pub distance: f32,
    /// The collider's index in the list queried.
    pub collider: usize,
}

/// `raycast_convexMesh` 0x10F97660: the ray into hull space (the inverse pose: `rotateInv(-p)` and the
/// conjugate's matrix), then each plane: `dist = ((o·n)) + d`, `dn = d·n`, `t = -(dist / dn)`; leaving planes
/// (`dn > 1e-7`) lower the exit, entering ones (`dn < -1e-7`) raise the entry (and pick the face), parallel ones
/// with the origin outside miss. Inside every plane: distance 0 at the origin, normal `-dir`. Else a hit when
/// `exit > entry > 0` and `entry < max - 1e-5`: the point `pose(o + t·d)`, the normal the face plane through the
/// transposed matrix, normalised by `1 / sqrtf`.
pub fn raycast_convex(hull: &ConvexHull, scale: Vector3, pose: &Pose, origin: Vector3, dir: Vector3, max: f32) -> Option<(Vector3, Vector3, u32, f32)> {
    let qi = Quaternion { x: neg(pose.q.x), y: neg(pose.q.y), z: neg(pose.q.z), w: pose.q.w };
    let pi = rotate_inv(pose.q, v3(neg(pose.p.x), neg(pose.p.y), neg(pose.p.z)));
    // `scale.getInverse() * pose.getInverse()` (`physx::operator*(PxMeshScale, PxTransform)` 0x10FA2BA0): with no
    // scale rotation the scale matrix is `diag(1 / s)`, its rows scaling the rotation's and the translation
    let inv = v3(1.0 / scale.x, 1.0 / scale.y, 1.0 / scale.z);
    let row = |c: Vector3| v3(inv.x * c.x, inv.y * c.y, inv.z * c.z);
    let [r0, r1, r2] = rot_mat(qi);
    let (c0, c1, c2) = (row(r0), row(r1), row(r2));
    let pi = row(pi);
    let o = v3(
        ((c0.x * origin.x + c1.x * origin.y) + c2.x * origin.z) + pi.x,
        ((c0.y * origin.x + c1.y * origin.y) + c2.y * origin.z) + pi.y,
        ((c0.z * origin.x + c1.z * origin.y) + c2.z * origin.z) + pi.z,
    );
    let d = v3((c0.x * dir.x + c1.x * dir.y) + c2.x * dir.z, (c0.y * dir.x + c1.y * dir.y) + c2.y * dir.z, (c0.z * dir.x + c1.z * dir.y) + c2.z * dir.z);
    let mut inside = true;
    let mut entry = -f32::MAX;
    let mut exit = f32::MAX;
    let mut face = u32::MAX;
    for (i, pl) in hull.planes.iter().enumerate() {
        let dist = ((o.x * pl.n.x + o.y * pl.n.y) + o.z * pl.n.z) + pl.d;
        let dn = (d.x * pl.n.x + d.y * pl.n.y) + d.z * pl.n.z;
        if dist > 0.0 {
            inside = false;
        }
        let t = neg(dist / dn);
        if dn > 1e-7 {
            if !(t > exit) {
                exit = t;
            }
        } else if -1e-7 > dn {
            if t > entry {
                entry = t;
                face = i as u32;
            }
        } else if dist > 0.0 {
            return None;
        }
    }
    if inside {
        return Some((origin, v3(neg(dir.x), neg(dir.y), neg(dir.z)), u32::MAX, 0.0));
    }
    if !(exit > entry && entry > 0.0 && max - 1e-5 > entry) {
        return None;
    }
    let local = v3(o.x + d.x * entry, o.y + d.y * entry, o.z + d.z * entry);
    let local = v3(scale.x * local.x, scale.y * local.y, scale.z * local.z);
    let r = rotate(pose.q, local);
    let point = v3(r.x + pose.p.x, r.y + pose.p.y, r.z + pose.p.z);
    let n = hull.planes[face as usize].n;
    let nx = (c0.x * n.x + c0.y * n.y) + c0.z * n.z;
    let ny = (c1.x * n.x + c1.y * n.y) + c1.z * n.z;
    let nz = (c2.x * n.x + c2.y * n.y) + c2.z * n.z;
    let len = ((nx * nx + ny * ny) + nz * nz).sqrt();
    let normal = if len > 0.0 {
        let inv = 1.0 / len;
        v3(inv * nx, inv * ny, inv * nz)
    } else {
        v3(nx, ny, nz)
    };
    Some((point, normal, face, entry))
}

/// `Gu::rayAABBIntersect2` 0x10FBF000: the slab test. Per axis outside the box the near face is the candidate
/// (`(face - o) / d` unless `d`'s bits are 0); inside on every axis: the origin, `t = 0`, axis 1. The largest
/// candidate wins (a negative one misses); the other coordinates `d·t + o` must lie within the box ± 1e-5.
/// Returns the axis + 1, the point and `t`.
fn ray_aabb(min: Vector3, max: Vector3, o: Vector3, d: Vector3) -> Option<(usize, Vector3, f32)> {
    let (mn, mx, ro, rd) = ([min.x, min.y, min.z], [max.x, max.y, max.z], [o.x, o.y, o.z], [d.x, d.y, d.z]);
    let mut coord = [0f32; 3];
    let mut inside = true;
    let mut maxt = [-1.0f32; 3];
    for i in 0..3 {
        if !(mn[i] <= ro[i]) {
            coord[i] = mn[i];
            inside = false;
            if rd[i].to_bits() != 0 {
                maxt[i] = (mn[i] - ro[i]) / rd[i];
            }
        } else if mx[i] < ro[i] {
            coord[i] = mx[i];
            inside = false;
            if rd[i].to_bits() != 0 {
                maxt[i] = (mx[i] - ro[i]) / rd[i];
            }
        }
    }
    if inside {
        return Some((1, o, 0.0));
    }
    let mut w = (maxt[0] < maxt[1]) as usize;
    if maxt[w] <= maxt[2] && maxt[2] != maxt[w] {
        w = 2;
    }
    if maxt[w].to_bits() & 0x8000_0000 != 0 {
        return None;
    }
    for i in 0..3 {
        if i != w {
            let c = rd[i] * maxt[w] + ro[i];
            coord[i] = c;
            if c < mn[i] - 1e-5 || mx[i] + 1e-5 < c {
                return None;
            }
        }
    }
    Some((w + 1, v3(coord[0], coord[1], coord[2]), maxt[w]))
}

/// `raycast_box` 0x10F96910: the ray into the box's frame (`rotateInv`), [`ray_aabb`] against `±half_extents`,
/// no hit beyond `max`; the point back through the pose (the origin itself at `t = 0`), the normal the hit
/// axis signed by the point's coordinate (`-dir` from inside).
pub fn raycast_box(half: Vector3, pose: &Pose, origin: Vector3, dir: Vector3, max: f32) -> Option<(Vector3, Vector3, u32, f32)> {
    let lo = rotate_inv(pose.q, v3(origin.x - pose.p.x, origin.y - pose.p.y, origin.z - pose.p.z));
    let ld = rotate_inv(pose.q, dir);
    let (axis, coord, t) = ray_aabb(v3(neg(half.x), neg(half.y), neg(half.z)), half, lo, ld)?;
    if max < t {
        return None;
    }
    let point = if t == 0.0 {
        origin
    } else {
        let r = rotate(pose.q, coord);
        v3(pose.p.x + r.x, pose.p.y + r.y, pose.p.z + r.z)
    };
    let normal = if t != 0.0 {
        let c = [coord.x, coord.y, coord.z][axis - 1];
        let mut n = [0f32; 3];
        n[axis - 1] = if 0.0 < c { 1.0 } else { -1.0 };
        rotate(pose.q, v3(n[0], n[1], n[2]))
    } else {
        v3(neg(dir.x), neg(dir.y), neg(dir.z))
    };
    Some((point, normal, u32::MAX, t))
}

/// `Physics.Raycast(origin, direction, out hit, maxDistance, layerMask)`: the direction normalised
/// (`direction / magnitude` when above `float.Epsilon`, then `Ray`'s `normalized`), every collider on a layer
/// in the mask cast against (one the ray starts inside is skipped: Unity drops PhysX's distance-0 hits), the
/// closest hit kept (a later one must be nearer by PhysX's 1e-5 margin for
/// convex shapes: the query shrinks its distance to each hit; candidates are taken in list order, where PhysX
/// walks its tree).
pub fn raycast(colliders: &[Collider], origin: Vector3, direction: Vector3, max_distance: f32, layer_mask: i32) -> Option<RaycastHit> {
    let len = direction.magnitude();
    if !(len > f32::from_bits(1)) {
        return None;
    }
    let dir = direction.div(len).normalized();
    let mut best: Option<RaycastHit> = None;
    let mut max = max_distance;
    for (i, c) in colliders.iter().enumerate() {
        if layer_mask & (1 << c.layer) == 0 {
            continue;
        }
        let hit = match &c.shape {
            Shape::Convex { hull, scale } => raycast_convex(hull, *scale, &c.pose, origin, dir, max),
            Shape::Box { half_extents } => raycast_box(*half_extents, &c.pose, origin, dir, max),
        };
        if let Some((point, normal, face_id, distance)) = hit {
            // (Unity drops initial overlaps: a ray starting inside a collider does not hit it)
            if distance == 0.0 {
                continue;
            }
            if best.map_or(true, |b| distance < b.distance) {
                max = distance;
                best = Some(RaycastHit { point, normal, face_id, distance, collider: i });
            }
        }
    }
    best
}
