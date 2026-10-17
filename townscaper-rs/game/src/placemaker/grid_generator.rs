//! `Placemaker.GridGenerator` and its `GridBit`s: the irregular grid, generated around the camera in patches,
//! clusters and bits (rows E3a–E3e). Row E3a: `GridBit`; row E3b: the coordinate maths (free functions here:
//! the instance ones never read the generator's state); row E3c: the patches (`GridGenerator`, `iterate_patch`). Checked against the running game by
//! `tests/placemaker_grid.rs` (`TS_HARNESS=1`).
//!
//! Grid positions are hex positions × `GridConstants.hexScale` (12). Patch centres sit 72 apart
//! (`hexDistance`), on a lattice of multiples of 216 (3 × 72) skewed back by `(p + rotate(p)) / 3`; grid
//! centroids use a 2304 lattice.

use std::collections::HashMap;

use crate::os::hex;
use crate::placemaker::quads::grid_generation::grid_constants::{HEX_TO_PLANE, PLANE_TO_HEX};
use crate::placemaker::quads::grid_generation::{CustomGridLibrary, CustomGridPatch, HexCluster, HexPatch, HexPatchState};
use crate::placemaker::quads::{Quad, Vert};
use crate::unity::mathematics::{f2, f3, hash3, i3, mul_i3x3, normalize2, Float2, Float3, Int2, Int3, Int3x2, Int3x3, Int4, Random};
use crate::unity::mathf;
use crate::unity::vector::{v2, Vector2, Vector3};

/// `GridGenerator.bitRadius`.
pub const BIT_RADIUS: f32 = 10.510949;
/// `GridGenerator.bitExtent`.
pub const BIT_EXTENT: i32 = 84;
/// `GridGenerator.relaxIterations`.
pub const RELAX_ITERATIONS: i32 = 255;
/// `GridGenerator.bitMeshVertCount`.
pub const BIT_MESH_VERT_COUNT: i32 = 300;

/// `GridGenerator.diagonal` as `GridGenerator..cctor` 0xE39320 leaves it (√2 / 2, read from the running game).
pub fn diagonal() -> f32 {
    f32::from_bits(0x3F35_04F3)
}

/// `GridBit.State`: the steps `GridGenerator.IterateBit` takes a bit through (ordered like the game's byte).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum GridBitState {
    #[default]
    Verts = 0,
    Relax = 1,
    Clusters = 2,
    Compose = 3,
    Mesh = 4,
    Done = 5,
}

/// One `Mesh` the game fills for a bit (`IterateBit`'s Mesh step, on a `MeshFilter` from
/// `WorldMeshes.GetMeshFilter`, drawn with `MaterialMaster.gridMaterial` on `LayerMaster`'s grid layer):
/// what `Mesh.SetVertices` / `SetUVs(0)` / `SetUVs(1)` / `SetTriangles` receive.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BitMesh {
    /// Relative to the bit's position (`PlaneToWorld(v - bitPlanePos, 0)`).
    pub vertices: Vec<Vector3>,
    /// The grid hex position (× 12) of each corner, and the mean for the quad's centre.
    pub uv0: Vec<Vector2>,
    /// `(-1,-1) (-1,1) (1,1) (1,-1)` at the corners, `(0,0)` at the centre.
    pub uv1: Vec<Vector2>,
    pub triangles: Vec<i32>,
}

/// `Placemaker.GridBit` (a `MonoBehaviour` under `GridGenerator.bitContainer`): one meshed piece of the grid.
/// `name`, `active`, `position` and `meshes` are its `GameObject`: name, `activeSelf`, `transform.position` and
/// the meshes of its child `MeshFilter`s.
#[derive(Clone, Debug, PartialEq)]
pub struct GridBit {
    pub hex_pos: Int2,
    pub motivations: u8,
    pub relaxation_steps: u8,
    pub cluster_count: u8,
    pub patch_done: bool,
    pub state: GridBitState,
    pub verts: Vec<Float2>,
    pub name: String,
    pub active: bool,
    pub position: Vector3,
    pub meshes: Vec<BitMesh>,
}

impl Default for GridBit {
    /// `GridBit..ctor` 0xE285F0: `verts` with capacity 256 (a new `GameObject` is active).
    fn default() -> Self {
        GridBit {
            hex_pos: Int2::default(),
            motivations: 0,
            relaxation_steps: 0,
            cluster_count: 0,
            patch_done: false,
            state: GridBitState::Verts,
            verts: Vec::with_capacity(256),
            name: String::new(),
            active: true,
            position: Vector3::ZERO,
            meshes: Vec::new(),
        }
    }
}

impl GridBit {
    /// `GridBit.get_done` 0xE28660.
    pub fn done(&self) -> bool {
        self.state == GridBitState::Done
    }
}

fn add3(a: Int3, b: Int3) -> Int3 {
    i3(a.x.wrapping_add(b.x), a.y.wrapping_add(b.y), a.z.wrapping_add(b.z))
}

/// Component-wise C# division (truncating) by a positive constant.
fn div3(a: Int3, d: i32) -> Int3 {
    i3(a.x / d, a.y / d, a.z / d)
}

fn mul3(a: Int3, m: i32) -> Int3 {
    i3(a.x.wrapping_mul(m), a.y.wrapping_mul(m), a.z.wrapping_mul(m))
}

/// `-(a + b)`, the hex coordinate rebuilt from the other two.
fn neg(a: i32, b: i32) -> i32 {
    a.wrapping_add(b).wrapping_neg()
}

/// `((a < 1 ? -half : half) + a) / step`: the nearest multiple's index (halves away from zero, 0 down).
fn round_div(a: i32, half: i32, step: i32) -> i32 {
    (if a < 1 { half.wrapping_neg() } else { half }).wrapping_add(a) / step
}

/// `Placemaker.GridGenerator.HexToPlane(float3)` 0xE2D0C0: `HexMath.HexToPlane(h) * GridConstants.hexToPlane`.
pub fn hex_to_plane(h: Float3) -> Float2 {
    let p = hex::hex_to_plane(h);
    f2(p.x * HEX_TO_PLANE, p.y * HEX_TO_PLANE)
}

/// `Placemaker.GridGenerator.HexToPlane(int3)` 0xE2D150.
pub fn hex_to_plane_i3(h: Int3) -> Float2 {
    hex_to_plane(f3(h.x as f32, h.y as f32, h.z as f32))
}

/// `Placemaker.GridGenerator.HexToPlane(float2)` 0xE2D210.
pub fn hex_to_plane2(h: Float2) -> Float2 {
    let p = hex::hex_to_plane2(h);
    f2(p.x * HEX_TO_PLANE, p.y * HEX_TO_PLANE)
}

/// `Placemaker.GridGenerator.HexToPlane(int2)` 0xE2D290.
pub fn hex_to_plane_i2(h: Int2) -> Float2 {
    hex_to_plane2(f2(h.x as f32, h.y as f32))
}

/// `Placemaker.GridGenerator.PlaneToHex` 0xE37890: `HexMath.PlaneToHex(p * GridConstants.planeToHex)`.
pub fn plane_to_hex(p: Float2) -> Float3 {
    hex::plane_to_hex(f2(p.x * PLANE_TO_HEX, p.y * PLANE_TO_HEX))
}

/// `Placemaker.GridGenerator.GetVertCost` 0xE2BBD0: the plane y of a grid position.
pub fn get_vert_cost(hex_pos: Float2) -> f32 {
    hex::hex_to_plane2(hex_pos).y * HEX_TO_PLANE
}

/// `Placemaker.GridGenerator.GetClusterOffset` 0xE2A590.
pub fn get_cluster_offset() -> Int2 {
    Int2 { x: 72, y: -72 }
}

/// `Placemaker.GridGenerator.GetStartingHex` 0xE2BB20: the dual position of a `HexOnionIterator` after
/// `onion_index` steps, × 72.
pub fn get_starting_hex(onion_index: i32) -> Int2 {
    let mut it = hex::HexOnionIterator::default();
    let mut n = onion_index;
    while 0 < n {
        it.increment();
        n -= 1;
    }
    let d = it.dual_pos2();
    Int2 { x: d.x.wrapping_mul(72), y: d.y.wrapping_mul(72) }
}

/// `Placemaker.GridGenerator.HexToPatchHex(float3)` 0xE2C850: the centre of the patch a position falls in.
pub fn hex_to_patch_hex_f(h: Float3) -> Int3 {
    let r = hex::rotate_cw_f(h);
    let s = f3((h.x + r.x) / 216.0, (h.y + r.y) / 216.0, (h.z + r.z) / 216.0);
    let m = mul3(hex::hex_round(s), 216);
    div3(add3(m, hex::rotate_ccw(m)), 3)
}

/// `Placemaker.GridGenerator.HexToPatchHex(int2)` 0xE2CA10.
pub fn hex_to_patch_hex(h: Int2) -> Int2 {
    let h3 = hex::get_hex3(h);
    hex::get_hex2(hex_to_patch_hex_f(f3(h3.x as f32, h3.y as f32, h3.z as f32)))
}

/// `Placemaker.GridGenerator.GetPatchOffset` 0xE2A5B0: from a cluster position to its patch's centre, as
/// `HexToPatchHex` but rounding with `HexRoundX` and rotating the 2-component form.
pub fn get_patch_offset(cluster_pos: Int2) -> Int2 {
    let h3 = hex::get_hex3(cluster_pos);
    let h = f3(h3.x as f32, h3.y as f32, h3.z as f32);
    let r = hex::rotate_cw_f(h);
    let n = hex::hex_round_x(f3((h.x + r.x) / 216.0, (h.y + r.y) / 216.0, (h.z + r.z) / 216.0));
    let m = Int2 { x: n.x.wrapping_mul(216), y: n.y.wrapping_mul(216) };
    let c = hex::rotate_ccw2i(m);
    Int2 {
        x: (m.x.wrapping_add(c.x) / 3).wrapping_sub(cluster_pos.x),
        y: (m.y.wrapping_add(c.y) / 3).wrapping_sub(cluster_pos.y),
    }
}

/// `Placemaker.GridGenerator.HexToPatchHexes` 0xE2CAF0: the (up to three) patch centres whose patches share
/// the position. `mask`: 1 = `c0` only, +2 when `c1` differs from `c0`, +4 when `c2` differs from both.
pub fn hex_to_patch_hexes(h: Int3) -> (Int3x3, u8) {
    let a = div3(add3(hex::rotate_cw(h), h), 3);
    let (x, y, z) = (round_div(a.x, 36, 72).wrapping_mul(72), round_div(a.y, 36, 72).wrapping_mul(72), round_div(a.z, 36, 72).wrapping_mul(72));
    let dx = a.x.wrapping_sub(x).wrapping_abs();
    let dy = a.y.wrapping_sub(y).wrapping_abs();
    let dz = a.z.wrapping_sub(z).wrapping_abs();
    let step = |v: i32| if v < 1 { -72 } else { 72 };
    let mut c0 = if dy < dx && dz < dx {
        i3(neg(z, y), y, z)
    } else if dz < dy {
        i3(x, neg(z, x), z)
    } else {
        i3(x, y, neg(y, x))
    };
    // The third centre; on the z edge it moves the first.
    let third = |c0: &mut Int3| {
        let c2 = if dx < dz && dy < dz {
            i3(x, y, neg(x, y))
        } else if dy < dx {
            i3(neg(y, z), y, z)
        } else {
            i3(x, neg(x, z), z)
        };
        if dz == 36 && dx == 0 {
            let s = step(a.z);
            *c0 = i3(c2.x, c2.y.wrapping_add(s), c2.z.wrapping_sub(s));
        }
        c2
    };
    let (c1, c2);
    if dx == 36 && dy == 0 {
        let s = step(a.x);
        c1 = i3(c0.x.wrapping_sub(s), c0.y, c0.z.wrapping_add(s));
        c2 = third(&mut c0);
    } else {
        c1 = if dx < dy && dz < dy {
            i3(x, neg(x, z), z)
        } else if dx < dz {
            i3(x, y, neg(x, y))
        } else {
            i3(neg(y, z), y, z)
        };
        if dy == 36 && dz == 0 {
            let s = step(a.y);
            c2 = i3(c1.x.wrapping_add(s), c1.y.wrapping_sub(s), c1.z);
        } else {
            c2 = third(&mut c0);
        }
    }
    let skew = |c: Int3| add3(hex::rotate_ccw(c), c);
    let m = Int3x3 { c: [skew(c0), skew(c1), skew(c2)] };
    let mut mask = if m.c[1] == m.c[0] { 1u8 } else { 3 };
    if m.c[2] != m.c[0] && m.c[2] != m.c[1] {
        mask += 4;
    }
    (m, mask)
}

/// `Placemaker.GridGenerator.HexToGridCentroid` 0xE2C590: the nearest centroid (a 2304 lattice, offset by
/// 2160 in y and z).
pub fn hex_to_grid_centroid(h: Int3) -> Int3 {
    let t = i3(h.x, h.y.wrapping_sub(2160), h.z.wrapping_add(2160));
    let a = div3(add3(t, hex::rotate_cw(t)), 3);
    let q = |v: i32| (if 0 < v { 1152i32 } else { -1152 }).wrapping_add(v) / 2304;
    let (qx, qy, qz) = (q(a.x), q(a.y), q(a.z));
    let (mut x, mut y, mut z) = (qx.wrapping_mul(2304), qy.wrapping_mul(2304), qz.wrapping_mul(2304));
    let ax = a.x.wrapping_sub(x).wrapping_abs();
    let ay = a.y.wrapping_sub(y).wrapping_abs();
    let az = a.z.wrapping_sub(z).wrapping_abs();
    if ay < ax && az < ax {
        x = qy.wrapping_add(qz).wrapping_mul(-2304);
    } else if az < ay {
        y = neg(x, z);
    } else {
        z = neg(x, y);
    }
    let c = i3(x, y, z);
    let r = add3(hex::rotate_ccw(c), c);
    i3(r.x, r.y.wrapping_add(2160), r.z.wrapping_sub(2160))
}

/// `Placemaker.GridGenerator.GetClosestClusterPos` 0xE2A0E0.
pub fn get_closest_cluster_pos(h: Int3) -> Int3 {
    let s = add3(h, hex::rotate_ccw(h));
    let snap = |v: i32| (v / 216).wrapping_mul(216).wrapping_add(if v < 1 { -108 } else { 108 });
    let v = i3(snap(s.x), snap(s.y), snap(s.z));
    div3(add3(hex::rotate_cw(v), v), 3)
}

/// `Placemaker.GridGenerator.GetSubdivisions` 0xE2BB80: 1, 3, 2, 2 subdivisions in diagonal bands 11 wide.
pub fn get_subdivisions(hex_pos: Int2) -> i32 {
    let a = mathf::abs_i32(hex_pos.x.wrapping_add(hex_pos.y).wrapping_add(72) / 11);
    match a.wrapping_add(3) % 4 {
        0 => 1,
        1 | -3 => 3,
        _ => 2,
    }
}

/// `Placemaker.GridGenerator.IsBorder` 0xE2D330: on a patch border (the dominant axis of the doubled, skewed
/// position within its 144 cell, at a multiple of 72).
pub fn is_border(hex_pos: Int2) -> bool {
    let h = hex::get_hex3(hex_pos);
    let d = Int2 { x: hex_pos.x.wrapping_mul(2), y: hex_pos.y.wrapping_mul(2) };
    let t = hex::rotate_ccw2i(d);
    let c = hex::get_hex3(Int2 { x: t.x.wrapping_add(d.x) / 3, y: d.y.wrapping_add(t.y) / 3 });
    let rem = |v: i32| v.wrapping_sub(round_div(v, 72, 144).wrapping_mul(144)).wrapping_abs();
    let (ax, ay, az) = (rem(c.x), rem(c.y), rem(c.z));
    let on_line = |v: i32| {
        let a = v.wrapping_abs();
        a == (a / 72).wrapping_mul(72)
    };
    let mut y_too = true;
    if ay <= ax {
        if az <= ax && 0 < ax && on_line(h.x) {
            return true;
        }
        y_too = ax <= ay;
    }
    if y_too && az <= ay && 0 < ay && on_line(h.y) {
        return true;
    }
    ax <= az && ay <= az && 0 < az && on_line(h.z)
}

/// `Placemaker.GridGenerator.IsNewGrid` 0xE2D570: outside the central ±1152 hexagon, where three Perlin
/// samples sum past 1.55.
pub fn is_new_grid(h: Int3) -> bool {
    let inside = |v: i32| v < 1153 && v.wrapping_neg() <= 1152;
    if inside(h.x) && inside(h.y) && inside(h.z) {
        return false;
    }
    let (x, y, z) = (h.x as f32, h.y as f32, h.z as f32);
    let a = mathf::perlin_noise(x * 0.01123, y * -0.331);
    let b = mathf::perlin_noise(y * 0.01123, z * -0.331);
    let c = mathf::perlin_noise(z * 0.01123, x * -0.331);
    1.55 < c + b + a
}

/// `Placemaker.ModuleMath.baryVectors` (built by `ModuleMath..cctor`, row E5c): per sixth of the hexagon, the
/// gradient of a barycentric weight in the plane.
pub fn bary_vectors() -> [Float2; 6] {
    crate::placemaker::modules::ModuleMath::get().bary_vectors
}

/// `Placemaker.GridGenerator`'s state. Patches, clusters and bits are kept in their lists and found by position
/// through the dictionaries (indexes into the lists: the game's dictionaries hold the same objects as its lists;
/// only `SceneProcessIteration` removes from a list, and it rebuilds the index).
/// `bit_container_position` stands for `bitContainer`'s transform; `master` (`WorldMaster`) is passed in
/// where used: `Graph.genBounds` to `iterate_focus`, `WorldMaster.KeepGoing` as `keep_going`.
#[derive(Clone, Debug)]
pub struct GridGenerator {
    pub custom_grid_library: CustomGridLibrary,
    pub bits: Vec<GridBit>,
    pub bit_dict: HashMap<Int2, usize>,
    pub patches: Vec<HexPatch>,
    pub patch_dict: HashMap<Int2, usize>,
    pub clusters: Vec<HexCluster>,
    pub cluster_dict: HashMap<Int2, usize>,
    pub bit_process_index: u16,
    pub deltas: Vec<Float2>,
    pub focus_updated: bool,
    pub hex_origo: Int2,
}

impl Default for GridGenerator {
    /// `GridGenerator..ctor` 0xE393C0: empty lists and dictionaries, `focusUpdated = true`.
    fn default() -> Self {
        GridGenerator {
            custom_grid_library: CustomGridLibrary::default(),
            bits: Vec::new(),
            bit_dict: HashMap::new(),
            patches: Vec::new(),
            patch_dict: HashMap::new(),
            clusters: Vec::new(),
            cluster_dict: HashMap::new(),
            bit_process_index: 0,
            deltas: Vec::new(),
            focus_updated: true,
            hex_origo: Int2::default(),
        }
    }
}

/// The six neighbours of a bit, clockwise from `(72, -72)` (the clusters around it).
fn cluster_dirs() -> impl Iterator<Item = Int2> {
    std::iter::successors(Some(Int2 { x: 72, y: -72 }), |d| Some(hex::rotate_cw2i(*d))).take(6)
}

fn add2(a: Int2, b: Int2) -> Int2 {
    Int2 { x: a.x.wrapping_add(b.x), y: a.y.wrapping_add(b.y) }
}

fn sub2(a: Int2, b: Int2) -> Int2 {
    Int2 { x: a.x.wrapping_sub(b.x), y: a.y.wrapping_sub(b.y) }
}

/// One relaxation pass over a quad (`r` its corners): the offset that would make it a square of half-diagonal
/// `diagonal` around its centre (`((r1 + r0) + r2) + r3) / 4`), added into `deltas[k]`.
fn square_quad(r: [Float2; 4], k: [usize; 4], centre: Float2, deltas: &mut [Float2]) {
    use crate::os::extra_math::{rotate2d_180, rotate2d_270, rotate2d_90};
    let c = centre;
    let d0 = f2(r[0].x - c.x, r[0].y - c.y);
    let e1 = rotate2d_270(f2(r[1].x - c.x, r[1].y - c.y));
    let e2 = rotate2d_180(f2(r[2].x - c.x, r[2].y - c.y));
    let e3 = rotate2d_90(f2(r[3].x - c.x, r[3].y - c.y));
    let s = f2(d0.x + e1.x + e2.x + e3.x, d0.y + e1.y + e2.y + e3.y);
    let n = normalize2(s);
    let n = f2(diagonal() * n.x, diagonal() * n.y);
    let targets = [n, rotate2d_90(n), rotate2d_180(n), rotate2d_270(n)];
    for i in 0..4 {
        let t = targets[i];
        let d = deltas[k[i]];
        deltas[k[i]] = f2(((c.x + t.x) - r[i].x) + d.x, ((c.y + t.y) - r[i].y) + d.y);
    }
}

impl GridGenerator {
    /// `Placemaker.GridGenerator.MotivatePatch` 0xE34FE0: one more reason for the patch at `patch_pos` to
    /// exist. A new patch reuses the first unwanted one (no motivations left: its lists are cleared, its
    /// `quadTriCutoff` and state reset, `subdivisions` kept), else is added.
    pub fn motivate_patch(&mut self, patch_pos: Int2, source_pos: Int2) -> usize {
        if let Some(&i) = self.patch_dict.get(&patch_pos) {
            self.patches[i].motivations.add_motivation(source_pos);
            return i;
        }
        let mut found = None;
        for (i, q) in self.patches.iter_mut().enumerate() {
            if q.motivations.count == 0 {
                q.quads.clear();
                q.verts.clear();
                q.quad_tri_cutoff = 0;
                q.state = HexPatchState::Verts;
                let old = q.hex_pos;
                self.patch_dict.remove(&old);
                found = Some(i);
                break;
            }
        }
        let i = found.unwrap_or_else(|| {
            self.patches.push(HexPatch::default());
            self.patches.len() - 1
        });
        self.patches[i].hex_pos = patch_pos;
        assert!(self.patch_dict.insert(patch_pos, i).is_none(), "MotivatePatch: Dictionary.Add of an existing key");
        self.patches[i].motivations.add_motivation(source_pos);
        i
    }

    /// `Placemaker.GridGenerator.UnmotivatePatch` 0xE392C0 (`patchDict[patchPos]`: a missing key throws).
    pub fn unmotivate_patch(&mut self, patch_pos: Int2, source_pos: Int2) {
        let i = *self.patch_dict.get(&patch_pos).expect("UnmotivatePatch: KeyNotFoundException");
        self.patches[i].motivations.remove_motivation(source_pos);
    }

    /// `Placemaker.GridGenerator.IteratePatch` 0xE30F50 on `patches[index]`.
    pub fn iterate_patch(&mut self, index: usize) {
        iterate_patch(&self.custom_grid_library, &mut self.patches[index]);
    }

    fn patch_at(&self, pos: Int2) -> usize {
        *self.patch_dict.get(&pos).expect("patchDict[pos]: KeyNotFoundException")
    }

    fn cluster_at(&self, pos: Int2) -> usize {
        *self.cluster_dict.get(&pos).expect("clusterDict[pos]: KeyNotFoundException")
    }

    /// `Placemaker.GridGenerator.MotivateCluster` 0xE34CD0: as `MotivatePatch`, for clusters (a reused one
    /// loses its relaxed verts); a cluster's first motivation, while not done, motivates its three patches.
    pub fn motivate_cluster(&mut self, cluster_pos: Int2, source_pos: Int2) -> usize {
        let i = match self.cluster_dict.get(&cluster_pos) {
            Some(&i) => i,
            None => {
                let mut found = None;
                for (k, q) in self.clusters.iter_mut().enumerate() {
                    if q.motivations.count == 0 {
                        q.patch_count = 0;
                        q.relaxation_steps = 0;
                        q.relaxed_verts.clear();
                        q.vert_indexes.clear();
                        q.done = false;
                        let old = q.hex_pos;
                        self.cluster_dict.remove(&old);
                        found = Some(k);
                        break;
                    }
                }
                let i = found.unwrap_or_else(|| {
                    self.clusters.push(HexCluster::default());
                    self.clusters.len() - 1
                });
                self.clusters[i].hex_pos = cluster_pos;
                assert!(self.cluster_dict.insert(cluster_pos, i).is_none(), "MotivateCluster: Dictionary.Add of an existing key");
                i
            }
        };
        if self.clusters[i].motivations.add_motivation(source_pos) == 1 && !self.clusters[i].done {
            self.clusters[i].patch_count = 0;
            let pos = self.clusters[i].hex_pos;
            let mut off = get_patch_offset(pos);
            for _ in 0..3 {
                self.motivate_patch(add2(pos, off), cluster_pos);
                off = hex::rotate_cw2_2i(off);
            }
        }
        i
    }

    /// `Placemaker.GridGenerator.UnmotivateCluster` 0xE39280.
    pub fn unmotivate_cluster(&mut self, index: usize, source_pos: Int2) {
        if self.clusters[index].motivations.remove_motivation(source_pos) == 0 && !self.clusters[index].done {
            self.unmotivate_cluster_children(index);
        }
    }

    /// `Placemaker.GridGenerator.UnmotivateClusterChildren` 0xE391C0: its three patches.
    pub fn unmotivate_cluster_children(&mut self, index: usize) {
        let pos = self.clusters[index].hex_pos;
        let mut off = get_patch_offset(pos);
        for _ in 0..3 {
            self.unmotivate_patch(add2(pos, off), pos);
            off = hex::rotate_cw2_2i(off);
        }
    }

    /// `Placemaker.GridGenerator.MotivateBit` 0xE34790: one more reason for the bit at `bit_pos` to exist. A new
    /// bit reuses the first unwanted one (its meshes go back to `WorldMeshes` when it was done; verts and steps
    /// reset), else a new `GameObject` under `bitContainer`; it is placed at the position. The first
    /// motivation motivates its patch (hidden until remeshed) and, unless done, its six clusters, stepping its
    /// state back to Clusters when one of the first `clusterCount` is not done.
    pub fn motivate_bit(&mut self, bit_pos: Int2) -> usize {
        let i = match self.bit_dict.get(&bit_pos) {
            Some(&i) => i,
            None => {
                let mut found = None;
                for (k, b) in self.bits.iter_mut().enumerate() {
                    if b.motivations == 0 {
                        let old = b.hex_pos;
                        self.bit_dict.remove(&old);
                        if b.state == GridBitState::Done {
                            // `WorldMaster.worldMeshes.ReturnMeshFilter` on each child, last first
                            b.meshes.clear();
                        }
                        b.verts.clear();
                        b.relaxation_steps = 0;
                        b.cluster_count = 0;
                        b.state = GridBitState::Verts;
                        found = Some(k);
                        break;
                    }
                }
                let i = found.unwrap_or_else(|| {
                    // `UnityFunctions.GetNewObject<GridBit>("Bit " + bits.Count, bitContainer)`: its
                    // `GetNewGameObject` 0x3CCE80 makes a `new GameObject()`, so the name is lost
                    self.bits.push(GridBit { name: "New Game Object".into(), ..Default::default() });
                    self.bits.len() - 1
                });
                let p = hex::plane_to_world(hex_to_plane_i2(bit_pos), 0.0);
                self.bits[i].position = Vector3 { x: p.x, y: p.y, z: p.z };
                self.bits[i].hex_pos = bit_pos;
                assert!(self.bit_dict.insert(bit_pos, i).is_none(), "MotivateBit: Dictionary.Add of an existing key");
                i
            }
        };
        let old = self.bits[i].motivations;
        self.bits[i].motivations = old.wrapping_add(1);
        if old == 0 {
            self.motivate_patch(bit_pos, bit_pos);
            self.bits[i].active = false;
            self.bits[i].patch_done = false;
            if self.bits[i].state != GridBitState::Done {
                for (k, dir) in cluster_dirs().enumerate() {
                    let c = self.motivate_cluster(add2(bit_pos, dir), bit_pos);
                    let k = k as u8;
                    if !self.clusters[c].done && k < self.bits[i].cluster_count {
                        self.bits[i].cluster_count = k;
                        if GridBitState::Clusters < self.bits[i].state {
                            self.bits[i].state = GridBitState::Clusters;
                        }
                    }
                }
            }
        }
        i
    }

    /// `Placemaker.GridGenerator.UnmotivateBit` 0xE39150: the last motivation hides it and lets go of its
    /// clusters (unless meshed) and its patch.
    pub fn unmotivate_bit(&mut self, index: usize) {
        let b = &mut self.bits[index];
        b.motivations = b.motivations.wrapping_sub(1);
        if b.motivations == 0 {
            b.active = false;
            if b.state < GridBitState::Mesh {
                self.unmotivate_bit_children(index);
            }
            let pos = self.bits[index].hex_pos;
            self.unmotivate_patch(pos, pos);
        }
    }

    /// `Placemaker.GridGenerator.UnmotivateBitChildren` 0xE39040: its six clusters.
    pub fn unmotivate_bit_children(&mut self, index: usize) {
        let pos = self.bits[index].hex_pos;
        for dir in cluster_dirs() {
            let c = self.cluster_at(add2(pos, dir));
            if self.clusters[c].motivations.remove_motivation(pos) == 0 && !self.clusters[c].done {
                self.unmotivate_cluster_children(c);
            }
        }
    }

    fn grow_deltas(&mut self, n: usize) {
        while self.deltas.len() < n {
            self.deltas.push(Float2::default());
        }
    }

    /// `Placemaker.GridGenerator.IterateCluster` 0xE2F950: first finishes the cluster's three patches (one
    /// `IteratePatch` per call, "Cluster.Patch"), then gathers their corners once (shared corners on the three
    /// lines through the cluster's centre are merged; "Cluster.Gather"), then relaxes them while `keep_going`
    /// allows, up to 255 steps; the call after that lets go of the patches and marks it done.
    pub fn iterate_cluster(&mut self, index: usize, keep_going: &mut dyn FnMut() -> bool) {
        let pos = self.clusters[index].hex_pos;
        if self.clusters[index].patch_count < 3 {
            let mut off = get_patch_offset(pos);
            for _ in 0..self.clusters[index].patch_count {
                off = hex::rotate_cw2_2i(off);
            }
            let p = self.patch_at(add2(pos, off));
            if !self.patches[p].done() {
                self.iterate_patch(p);
            } else {
                self.clusters[index].patch_count += 1;
            }
            return;
        }
        if self.clusters[index].vert_indexes.is_empty() {
            self.gather_cluster(index);
            return;
        }
        if self.clusters[index].relaxation_steps == 0xFF {
            self.unmotivate_cluster_children(index);
            self.clusters[index].done = true;
            return;
        }
        let mut off = get_patch_offset(pos);
        let mut ps = [0usize; 3];
        for p in ps.iter_mut() {
            *p = self.patch_at(add2(pos, off));
            off = hex::rotate_cw2_2i(off);
        }
        let n = self.clusters[index].relaxed_verts.len();
        self.grow_deltas(n);
        let o = self.clusters[index].patch_vert_offsets;
        let offsets = [o.x, o.y, o.z];
        loop {
            loop {
                if !keep_going() || self.clusters[index].relaxation_steps == 0xFF {
                    return;
                }
                self.clusters[index].relaxation_steps += 1;
                for (pi, &p) in ps.iter().enumerate() {
                    let quads = &self.patches[p].quads;
                    let c = &self.clusters[index];
                    let mut i = 0;
                    while i < quads.len() {
                        let k: [usize; 4] = std::array::from_fn(|j| {
                            let at = offsets[pi].wrapping_add(quads[i + j] as i32);
                            c.vert_indexes[list_index(at)] as i32 as usize
                        });
                        let r: [Float2; 4] = std::array::from_fn(|j| c.relaxed_verts[k[j]]);
                        let centre = f2(
                            (r[1].x + r[0].x + r[2].x + r[3].x) * 0.25,
                            (r[1].y + r[0].y + r[2].y + r[3].y) * 0.25,
                        );
                        square_quad(r, k, centre, &mut self.deltas);
                        i += 4;
                    }
                }
                if n >= 1 {
                    break;
                }
            }
            let c = &mut self.clusters[index];
            for j in 0..n {
                let (v, d) = (c.relaxed_verts[j], self.deltas[j]);
                c.relaxed_verts[j] = f2(v.x + d.x * 0.4, v.y + d.y * 0.4);
                self.deltas[j] = Float2::default();
            }
        }
    }

    /// `IterateCluster`'s "Cluster.Gather": the three patches' corners into `vertIndexes` (where each patch's
    /// start is `patchVertOffsets`) and the distinct ones, relative to the cluster, into `relaxedVerts`.
    fn gather_cluster(&mut self, index: usize) {
        let pos = self.clusters[index].hex_pos;
        let c3 = hex::get_hex3(pos);
        let mut distinct: Vec<Int2> = Vec::with_capacity(768);
        let mut off = get_patch_offset(pos);
        let mut starts = [0i32; 3];
        let mut total = 0i32;
        for (p, start) in starts.iter_mut().enumerate() {
            let pi = self.patch_at(add2(pos, off));
            off = hex::rotate_cw2_2i(off);
            *start = total;
            let verts = &self.patches[pi].verts;
            for &v in verts {
                let on_line = !(v.x != c3.x && v.y != c3.y && c3.z.wrapping_neg() != v.x.wrapping_add(v.y));
                let found = if p == 0 || !on_line { None } else { distinct.iter().position(|&u| u == v) };
                let k = found.unwrap_or_else(|| {
                    distinct.push(v);
                    distinct.len() - 1
                });
                self.clusters[index].vert_indexes.push(k as i16);
            }
            total = total.wrapping_add(verts.len() as i32);
        }
        let c = &mut self.clusters[index];
        c.patch_vert_offsets = i3(starts[0], starts[1], starts[2]);
        for u in distinct {
            c.relaxed_verts.push(hex_to_plane_i2(sub2(u, pos)));
        }
    }

    /// `Placemaker.GridGenerator.IterateBit` 0xE2D6A0: first finishes the bit's patch (one step per call), then
    /// one `GridBit.State` step: Verts (the patch's corners in the plane, relative to it), Relax (while
    /// `keep_going` allows, up to 255 steps), Clusters (one `IterateCluster` per call on each of the six until
    /// done), Compose (each corner blended with the two neighbouring clusters' relaxed corners by barycentric
    /// weights; then lets go of the clusters), Mesh (the bit's `Mesh`es; shown).
    pub fn iterate_bit(&mut self, index: usize, keep_going: &mut dyn FnMut() -> bool) {
        let pos = self.bits[index].hex_pos;
        if !self.bits[index].patch_done {
            let p = self.patch_at(pos);
            if !self.patches[p].done() {
                self.iterate_patch(p);
                if !self.patches[p].done() {
                    return;
                }
            }
            self.bits[index].patch_done = true;
            if self.bits[index].state == GridBitState::Done {
                self.bits[index].active = true;
            }
        }
        match self.bits[index].state {
            GridBitState::Verts => {
                let p = self.patch_at(pos);
                let patch = &self.patches[p];
                for &v in &patch.verts {
                    self.bits[index].verts.push(hex_to_plane_i2(sub2(v, patch.hex_pos)));
                }
                self.bits[index].state = GridBitState::Relax;
            }
            GridBitState::Relax => {
                if self.bits[index].relaxation_steps == 0xFF {
                    self.bits[index].state = GridBitState::Clusters;
                    return;
                }
                let p = self.patch_at(pos);
                let n = self.bits[index].verts.len();
                self.grow_deltas(n);
                loop {
                    if !keep_going() || self.bits[index].relaxation_steps == 0xFF {
                        return;
                    }
                    self.bits[index].relaxation_steps += 1;
                    let quads = &self.patches[p].quads;
                    let b = &self.bits[index];
                    let mut i = 0;
                    while i < quads.len() {
                        let k: [usize; 4] = std::array::from_fn(|j| quads[i + j] as usize);
                        let r: [Float2; 4] = std::array::from_fn(|j| b.verts[k[j]]);
                        let centre = f2(
                            (r[1].x + r[0].x + r[2].x + r[3].x) * 0.25,
                            (r[1].y + r[0].y + r[2].y + r[3].y) * 0.25,
                        );
                        square_quad(r, k, centre, &mut self.deltas);
                        i += 4;
                    }
                    let b = &mut self.bits[index];
                    for j in 0..n {
                        let (v, d) = (b.verts[j], self.deltas[j]);
                        b.verts[j] = f2(d.x * 0.4 + v.x, d.y * 0.4 + v.y);
                        self.deltas[j] = Float2::default();
                    }
                }
            }
            GridBitState::Clusters => {
                if 5 < self.bits[index].cluster_count {
                    self.bits[index].state = GridBitState::Compose;
                    return;
                }
                let dir = cluster_dirs().nth(self.bits[index].cluster_count as usize).unwrap();
                let c = self.cluster_at(add2(pos, dir));
                if !self.clusters[c].done {
                    self.iterate_cluster(c, keep_going);
                } else {
                    self.bits[index].cluster_count += 1;
                }
            }
            GridBitState::Compose => self.compose_bit(index),
            GridBitState::Mesh => self.mesh_bit(index),
            GridBitState::Done => {}
        }
    }

    /// `IterateBit`'s Compose step: each corner, by the sixth of the hexagon it lies in (from its angle to
    /// `HexToPlane((1, -1))`), is blended with the matching relaxed corners of the two clusters on that sixth's
    /// edges, by the barycentric weights of `ModuleMath.baryVectors`; the result is absolute in the plane.
    fn compose_bit(&mut self, index: usize) {
        let pos = self.bits[index].hex_pos;
        let p = self.patch_at(pos);
        let to = hex_to_plane_i2(Int2 { x: 1, y: -1 });
        let bit_plane = hex_to_plane_i2(pos);
        let bary = bary_vectors();
        let n = self.bits[index].verts.len();
        for i in 0..n {
            let patch = &self.patches[p];
            let from = hex_to_plane_i2(sub2(patch.verts[i], patch.hex_pos));
            let angle = Vector2::signed_angle(v2(from.x, from.y), v2(to.x, to.y));
            let angle = (((angle + 360.0) as f64) % 360.0) as f32;
            let s = mathf::floor_to_int(angle / 60.0);
            let next = (s + 1) % 6;
            let mut dir_a = Int2 { x: 72, y: -72 };
            for _ in 0..s.max(0) {
                dir_a = hex::rotate_cw2i(dir_a);
            }
            let dir_b = hex::rotate_cw2i(dir_a);
            let ca = &self.clusters[self.cluster_at(add2(pos, dir_a))];
            let cb = &self.clusters[self.cluster_at(add2(pos, dir_b))];
            let pick = |o: Int3, r: i32| match r {
                0 => o.z,
                1 => o.y,
                2 => o.x,
                _ => 0,
            };
            let off_a = pick(ca.patch_vert_offsets, s % 3);
            let off_b = pick(cb.patch_vert_offsets, next % 3);
            let pa = hex_to_plane_i2(dir_a);
            let pb = hex_to_plane_i2(dir_b);
            let w = self.bits[index].verts[i];
            let ra = ca.relaxed_verts[ca.vert_indexes[list_index(off_a.wrapping_add(i as i32))] as i32 as usize];
            let ra = f2(pa.x + ra.x, pa.y + ra.y);
            let rb = cb.relaxed_verts[cb.vert_indexes[list_index(off_b.wrapping_add(i as i32))] as i32 as usize];
            let rb = f2(pb.x + rb.x, pb.y + rb.y);
            let (b0, b1, b2) = (bary[list_index(s % 6)], bary[list_index((s + 2) % 6)], bary[list_index((s + 4) % 6)]);
            let f0 = b0.x * (from.x - pa.x) + b0.y * (from.y - pa.y);
            let f1 = b1.x * (from.x - pb.x) + b1.y * (from.y - pb.y);
            let f2_ = b2.y * from.y + b2.x * from.x;
            self.bits[index].verts[i] = f2(
                bit_plane.x + ((f0 * w.x + f1 * ra.x) + f2_ * rb.x),
                bit_plane.y + ((f0 * w.y + f1 * ra.y) + f2_ * rb.y),
            );
        }
        self.unmotivate_bit_children(index);
        self.bits[index].state = GridBitState::Mesh;
    }

    /// `IterateBit`'s Mesh step: per quad of the patch, its four corners and its centre (5 vertices, 4
    /// triangles round the centre), a new `Mesh` every 35 quads.
    fn mesh_bit(&mut self, index: usize) {
        let pos = self.bits[index].hex_pos;
        let p = self.patch_at(pos);
        let bit_plane = hex_to_plane_i2(pos);
        let patch = &self.patches[p];
        let b = &self.bits[index];
        let mut meshes = Vec::new();
        let mut m = BitMesh::default();
        let world = |v: Float2| {
            let w = hex::plane_to_world(f2(v.x - bit_plane.x, v.y - bit_plane.y), 0.0);
            Vector3 { x: w.x, y: w.y, z: w.z }
        };
        let mut base = 0i32;
        let mut i = 0;
        while i < patch.quads.len() {
            let q: [usize; 4] = std::array::from_fn(|j| patch.quads[i + j] as usize);
            let r: [Float2; 4] = std::array::from_fn(|j| b.verts[q[j]]);
            let h: [Float2; 4] = std::array::from_fn(|j| f2(patch.verts[q[j]].x as f32, patch.verts[q[j]].y as f32));
            let centre = f2((r[0].x + r[1].x + r[2].x + r[3].x) * 0.25, (r[0].y + r[1].y + r[2].y + r[3].y) * 0.25);
            let hc = f2((h[0].x + h[1].x + h[2].x + h[3].x) * 0.25, (h[0].y + h[1].y + h[2].y + h[3].y) * 0.25);
            for v in [r[0], r[1], r[2], r[3], centre] {
                m.vertices.push(world(v));
            }
            for v in [h[0], h[1], h[2], h[3], hc] {
                m.uv0.push(v2(v.x, v.y));
            }
            for v in [v2(-1.0, -1.0), v2(-1.0, 1.0), v2(1.0, 1.0), v2(1.0, -1.0), v2(0.0, 0.0)] {
                m.uv1.push(v);
            }
            for t in [0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4] {
                m.triangles.push(base + t);
            }
            base += 5;
            if base >= 171 {
                meshes.push(std::mem::take(&mut m));
                base = 0;
            }
            i += 4;
        }
        if base > 0 {
            meshes.push(m);
        }
        let b = &mut self.bits[index];
        b.meshes.extend(meshes);
        b.state = GridBitState::Done;
        b.active = true;
    }

    /// `Placemaker.GridGenerator.IterateBits` 0xE2F8B0: one step of the bit at `bitProcessIndex` while it
    /// needs one, else on to the next; false once past the last.
    pub fn iterate_bits(&mut self, keep_going: &mut dyn FnMut() -> bool) -> bool {
        let i = self.bit_process_index as usize;
        if self.bits.len() <= i {
            return false;
        }
        let b = &self.bits[i];
        if (b.state != GridBitState::Done || !b.patch_done) && b.motivations != 0 {
            self.iterate_bit(i, keep_going);
        } else {
            self.bit_process_index = self.bit_process_index.wrapping_add(1);
        }
        true
    }

    /// `Placemaker.GridGenerator.IterateGenerateCenter` 0xE30E90: works on the bit at `hexOrigo`; true while
    /// it is not done.
    pub fn iterate_generate_center(&mut self, keep_going: &mut dyn FnMut() -> bool) -> bool {
        let b = self.motivate_bit(self.hex_origo);
        if self.bits[b].state != GridBitState::Done {
            self.iterate_bit(b, keep_going);
            return self.bits[b].state != GridBitState::Done;
        }
        false
    }

    /// `Placemaker.GridGenerator.IterateFocus` 0xE30990: when the focus moved, recentres on the patch at the
    /// centre of `gen_bounds` (`WorldMaster.graph.genBounds`), wants every bit near the bounds (grown by
    /// `bitExtent`; found ring by ring, one ring past the last one used) and lets go of the bits wanted before.
    pub fn iterate_focus(&mut self, gen_bounds: Int3x2) -> bool {
        use crate::os::extra_math::{center, contains};
        if !self.focus_updated {
            return false;
        }
        self.focus_updated = false;
        let c = center(gen_bounds);
        self.hex_origo = hex::get_hex2(hex_to_patch_hex_f(f3(c.x as f32, c.y as f32, c.z as f32)));
        let wanted: Vec<usize> = (0..self.bits.len()).rev().filter(|&i| self.bits[i].motivations != 0).collect();
        let e = BIT_EXTENT;
        let grown = Int3x2 {
            c0: i3(gen_bounds.c0.x.wrapping_sub(e), gen_bounds.c0.y.wrapping_sub(e), gen_bounds.c0.z.wrapping_sub(e)),
            c1: i3(gen_bounds.c1.x.wrapping_add(e), gen_bounds.c1.y.wrapping_add(e), gen_bounds.c1.z.wrapping_add(e)),
        };
        let mut it = hex::HexOnionIterator::default();
        let mut last = 1u32;
        loop {
            let d = it.dual_pos2();
            let p = Int2 {
                x: self.hex_origo.x.wrapping_add(d.x.wrapping_mul(72)),
                y: self.hex_origo.y.wrapping_add(d.y.wrapping_mul(72)),
            };
            if contains(grown, hex::get_hex3(p)) {
                last = it.layer_index.wrapping_add(1);
                self.motivate_bit(p);
            }
            it.increment();
            if !(it.layer_index <= last) {
                break;
            }
        }
        for &i in wanted.iter().rev() {
            let b = &mut self.bits[i];
            b.motivations = b.motivations.wrapping_sub(1);
            if b.motivations == 0 {
                b.active = false;
                if b.state < GridBitState::Mesh {
                    self.unmotivate_bit_children(i);
                }
                let pos = self.bits[i].hex_pos;
                let p = self.patch_at(pos);
                self.patches[p].motivations.remove_motivation(pos);
            }
        }
        self.bit_process_index = 0;
        true
    }

    /// `Placemaker.GridGenerator.OnEnable` 0xE37450: rebuilds the dictionaries from the (serialized) lists; a
    /// bit left part-way between relaxing and done starts over ("Wrongly relaxed grid bit: " warning).
    pub fn on_enable(&mut self) {
        self.patch_dict = self.patches.iter().enumerate().map(|(i, p)| (p.hex_pos, i)).collect();
        self.bit_dict.clear();
        for (i, b) in self.bits.iter_mut().enumerate() {
            if GridBitState::Relax < b.state && b.relaxation_steps != 0xFF {
                // STANDIN: Debug.LogWarning("Wrongly relaxed grid bit: " + relaxationSteps + ... + 255) not shown
                b.verts.clear();
                b.relaxation_steps = 0;
                b.cluster_count = 0;
                b.state = GridBitState::Verts;
            }
            assert!(self.bit_dict.insert(b.hex_pos, i).is_none(), "OnEnable: Dictionary.Add of an existing key");
        }
        self.cluster_dict = self.clusters.iter().enumerate().map(|(i, c)| (c.hex_pos, i)).collect();
    }

    /// `Placemaker.GridGenerator.SceneProcessIteration` 0xE38830 (the scene-baking path): starts over with the
    /// bit at the origin, steps it up to its Mesh state, forgets the clusters and drops unwanted patches.
    pub fn scene_process_iteration(&mut self) {
        self.bits.clear();
        self.bit_dict.clear();
        self.patches.clear();
        self.patch_dict.clear();
        self.clusters.clear();
        self.cluster_dict.clear();
        let b = self.motivate_bit(Int2::default());
        self.bits[b].verts.clear();
        self.bits[b].relaxation_steps = 0;
        self.bits[b].cluster_count = 0;
        self.bits[b].state = GridBitState::Verts;
        // `GridGenerator.<>c.<SceneProcessIteration>b__46_0` 0x1F1440: always true
        while self.bits[b].state < GridBitState::Mesh {
            self.iterate_bit(b, &mut || true);
        }
        self.bits[b].cluster_count = 0;
        self.clusters.clear();
        self.cluster_dict.clear();
        for i in (0..self.patches.len()).rev() {
            if self.patches[i].motivations.count == 0 {
                let pos = self.patches[i].hex_pos;
                self.patch_dict.remove(&pos);
                self.patches.remove(i);
            }
        }
        self.patch_dict = self.patches.iter().enumerate().map(|(i, p)| (p.hex_pos, i)).collect();
    }
}

/// `Unity.Mathematics.Random.NextUInt(uint)` (inlined in the game): the state before the step, times `n`, high
/// word.
fn next_uint(r: &mut Random, n: u32) -> u32 {
    ((n as u64 * r.next_state() as u64) >> 32) as u32
}

fn list_index(i: i32) -> usize {
    usize::try_from(i).expect("List<T> index: ArgumentOutOfRangeException")
}

/// `Placemaker.GridGenerator.IteratePatch` 0xE30F50: one step of the patch's `HexPatch.State` machine.
/// Grid positions are × 12 hex positions; the patch's corners are indexed by bytes.
///
/// - `Verts`: a hand-made patch by where the patch sits relative to its grid centroid (seeded by the patch
///   position's `int3` hash), rotated and maybe mirrored, then straight to `Quadrangulate`; or, without one, a
///   hexagon of corners ring by ring (spacing 9 × 2^subdivisions), on to `Triangulate`.
/// - `Triangulate`: the hexagon's triangles, ring by ring.
/// - `Quadrangulate`: merges one pair of triangles into a quad per call, starting at a hashed offset; when no
///   merge is left, on to `Subdivide`.
/// - `Subdivide`: every quad into four and every triangle into three quads, `subdivisions` times (shared edge
///   midpoints through a dictionary).
/// - `Sort`: quads ordered by their lowest corner (`GetVertCost`), each rotated to start there.
pub fn iterate_patch(library: &CustomGridLibrary, patch: &mut HexPatch) {
    match patch.state {
        HexPatchState::Verts => patch_verts(library, patch),
        HexPatchState::Triangulate => patch_triangulate(patch),
        HexPatchState::Quadrangulate => patch_quadrangulate(patch),
        HexPatchState::Subdivide => patch_subdivide(patch),
        HexPatchState::Sort => patch_sort(patch),
        HexPatchState::Done => {}
    }
}

fn patch_verts(library: &CustomGridLibrary, patch: &mut HexPatch) {
    let h3 = hex::get_hex3(patch.hex_pos);
    let c = hex_to_grid_centroid(h3);
    let (dx, dy, dz) = (h3.x.wrapping_sub(c.x), h3.y.wrapping_sub(c.y), h3.z.wrapping_sub(c.z));
    let mut random = Random::new(hash3(h3) as u32);
    let mut rotation = 0u32;
    let mut custom: Option<&CustomGridPatch> = None;
    // the centroid-curve patches are seeded by the centroid instead
    let along = |d: i32, base: u32, random: &mut Random| -> (Option<&CustomGridPatch>, u32) {
        if mathf::abs_i32(d) == 72 {
            *random = Random::new(hash3(c) as u32);
            let p = library.get_random(&library.centroid_curve_patches, random);
            (p, if 0 < dx { base + 3 } else { base })
        } else {
            let p = library.get_random(&library.straight_patches, random);
            (p, next_uint(random, 4) * 3 + base)
        }
    };
    if dx == dy && dy == dz {
        custom = library.get_random(&library.centroid_patches, &mut random);
        rotation = next_uint(&mut random, 12);
    } else if dx == dy {
        (custom, rotation) = along(dx, 0, &mut random);
    } else if dy == dz {
        (custom, rotation) = along(dy, 1, &mut random);
    } else if dz == dx {
        (custom, rotation) = along(dz, 2, &mut random);
    } else if is_new_grid(h3) {
        custom = library.get_random(&library.generic_patches, &mut random);
        rotation = next_uint(&mut random, 12);
    }
    if let Some(custom) = custom {
        apply_custom_patch(patch, custom, rotation);
        return;
    }
    let subdivisions = get_subdivisions(patch.hex_pos);
    patch.subdivisions = subdivisions as u8;
    let step = 1i32 << subdivisions;
    let layers = 8 / step + 1;
    let mut it = hex::HexOnionIterator::default();
    loop {
        let p = it.hex_pos2();
        patch.verts.push(Int2 {
            x: p.x.wrapping_mul(9).wrapping_mul(step).wrapping_add(patch.hex_pos.x),
            y: p.y.wrapping_mul(9).wrapping_mul(step).wrapping_add(patch.hex_pos.y),
        });
        it.increment();
        if (it.layer_index as i64) >= layers as i64 {
            break;
        }
    }
    patch.state = HexPatchState::Triangulate;
}

/// The hand-made patch, turned `rotation` sixth-turns (past 5: mirrored first, by swapping the x and y axes),
/// placed at the patch; a mirrored patch's corner order is reversed to keep its winding.
fn apply_custom_patch(patch: &mut HexPatch, custom: &CustomGridPatch, rotation: u32) {
    let (mut ax, mut ay, mut az) = (i3(1, 0, 0), i3(0, 1, 0), i3(0, 0, 1));
    let mut mirrored = false;
    let mut rotation = rotation;
    if 5 < rotation {
        for _ in 0..rotation / 6 {
            std::mem::swap(&mut ax, &mut ay);
            mirrored = !mirrored;
        }
        rotation %= 6;
    }
    for _ in 0..rotation {
        ax = hex::rotate_cw(ax);
        ay = hex::rotate_cw(ay);
        az = hex::rotate_cw(az);
    }
    let m = Int3x3 { c: [ax, ay, az] };
    for v in &custom.verts {
        let p = hex::get_hex2(mul_i3x3(&m, hex::get_hex3(*v)));
        patch.verts.push(Int2 { x: p.x.wrapping_add(patch.hex_pos.x), y: patch.hex_pos.y.wrapping_add(p.y) });
    }
    let q = &custom.quads;
    if !mirrored {
        patch.quads.extend_from_slice(q);
    } else {
        let cut = custom.quad_tri_cutoff as usize;
        let mut t = 0;
        while t < cut {
            patch.quads.extend_from_slice(&[q[t + 2], q[t + 1], q[t]]);
            t += 3;
        }
        let mut t = cut;
        while t < q.len() {
            patch.quads.extend_from_slice(&[q[t + 3], q[t + 2], q[t + 1], q[t]]);
            t += 4;
        }
    }
    patch.state = HexPatchState::Quadrangulate;
    patch.subdivisions = custom.subdivisions;
    patch.quad_tri_cutoff = custom.quad_tri_cutoff;
}

fn patch_triangulate(patch: &mut HexPatch) {
    let layers = 8 / (1i32 << (patch.subdivisions & 31)) + 1;
    let quads = &mut patch.quads;
    // inner ring: `inner` corners from `base0`; outer ring: `outer` corners from `base1`
    let (mut inner, mut base0, mut base1, mut ring) = (1i32, 0u8, 1u8, 1i32);
    if 1 < layers {
        let mut outer = 6i32;
        loop {
            let (mut i, mut j) = (0i32, 0i32);
            for _side in 0..6 {
                if 6 < outer {
                    let k0 = 1 - i;
                    loop {
                        let t = ((i % inner) as u8).wrapping_add(base0);
                        quads.push(t);
                        quads.push(((j % outer) as u8).wrapping_add(base1));
                        j += 1;
                        let u = ((j % outer) as u8).wrapping_add(base1);
                        quads.push(u);
                        quads.push(u);
                        i += 1;
                        quads.push(((i % inner) as u8).wrapping_add(base0));
                        quads.push(t);
                        if k0 + i >= ring {
                            break;
                        }
                    }
                }
                quads.push(((i % inner) as u8).wrapping_add(base0));
                quads.push(((j % outer) as u8).wrapping_add(base1));
                j += 1;
                quads.push(((j % outer) as u8).wrapping_add(base1));
            }
            base0 = base1;
            base1 = base1.wrapping_add(outer as u8);
            ring += 1;
            inner = outer;
            outer += 6;
            if ring >= layers {
                break;
            }
        }
    }
    patch.quad_tri_cutoff = patch.quads.len() as u16;
    patch.state = HexPatchState::Quadrangulate;
}

fn patch_quadrangulate(patch: &mut HexPatch) {
    let cutoff = patch.quad_tri_cutoff as i32;
    let mut u = (patch.hex_pos.x as u32)
        .wrapping_mul(0x83B5_8237)
        .wrapping_add(0xA9D9_19BF)
        .wrapping_add((patch.hex_pos.y as u32).wrapping_mul(0x833E_3E29).wrapping_sub(cutoff as u32));
    u ^= u << 13;
    u ^= u >> 17;
    // `((u ^ 0xFC000000) << 5) ^ u`: the constant's low bit survives the shift and flips bit 31
    let offset = mathf::abs_i32((((u ^ 0xFC00_0000) << 5) ^ u) as i32);
    let at = |quads: &Vec<u8>, i: i32| quads[list_index(i)];
    for k in 0..cutoff {
        let e = k.wrapping_add(offset) % cutoff;
        let (t, o) = ((e / 3) * 3, e % 3);
        let a = at(&patch.quads, o % 3 + t);
        let b = at(&patch.quads, (o + 2) % 3 + t);
        if is_border(patch.verts[a as usize]) && is_border(patch.verts[b as usize]) {
            continue;
        }
        let c = at(&patch.quads, (o + 1) % 3 + t);
        let mut s = 3;
        while s < cutoff {
            let t2 = (s + t) % cutoff;
            for m in 1..=3 {
                let p = at(&patch.quads, m + t2 - 1);
                let q = at(&patch.quads, m % 3 + t2);
                let r = at(&patch.quads, (m + 1) % 3 + t2);
                if a == r && b == p {
                    let (hi, lo) = (mathf::max_i32(t, t2), mathf::min_i32(t, t2));
                    patch.quads.drain(list_index(hi)..list_index(hi) + 3);
                    patch.quads.drain(list_index(lo)..list_index(lo) + 3);
                    patch.quads.extend_from_slice(&[a, c, b, q]);
                    patch.quad_tri_cutoff = patch.quad_tri_cutoff.wrapping_sub(6);
                    return;
                }
            }
            s += 3;
        }
    }
    patch.state = HexPatchState::Subdivide;
}

/// The corner at the midpoint of `a`–`b`: the second quad to ask for an edge takes (and forgets) the corner
/// the first one made.
fn midpoint(patch: &mut HexPatch, mids: &mut HashMap<Int2, u8>, next: &mut u8, a: Int2, b: Int2) -> u8 {
    let m = Int2 { x: a.x.wrapping_add(b.x) / 2, y: a.y.wrapping_add(b.y) / 2 };
    if let Some(i) = mids.remove(&m) {
        return i;
    }
    let i = *next;
    mids.insert(m, i);
    patch.verts.push(m);
    *next = next.wrapping_add(1);
    i
}

fn patch_subdivide(patch: &mut HexPatch) {
    let mut mids: HashMap<Int2, u8> = HashMap::with_capacity(patch.verts.len() * 4);
    let mut next = patch.verts.len() as u8;
    let vert = |patch: &HexPatch, i: u8| patch.verts[i as usize];
    for _ in 0..patch.subdivisions as i32 {
        let start = patch.quad_tri_cutoff;
        let end = patch.quads.len() as u16;
        let mut i = start;
        while i < end {
            let k = i as usize;
            let (q0, q1, q2, q3) = (patch.quads[k], patch.quads[k + 1], patch.quads[k + 2], patch.quads[k + 3]);
            let (p0, p1, p2, p3) = (vert(patch, q0), vert(patch, q1), vert(patch, q2), vert(patch, q3));
            let m01 = midpoint(patch, &mut mids, &mut next, p0, p1);
            let m12 = midpoint(patch, &mut mids, &mut next, p1, p2);
            let m23 = midpoint(patch, &mut mids, &mut next, p2, p3);
            let m30 = midpoint(patch, &mut mids, &mut next, p3, p0);
            let centre = next;
            next = next.wrapping_add(1);
            let sx = p0.x.wrapping_add(p1.x).wrapping_add(p2.x).wrapping_add(p3.x);
            let sy = p0.y.wrapping_add(p1.y).wrapping_add(p2.y).wrapping_add(p3.y);
            patch.verts.push(Int2 { x: sx / 4, y: sy / 4 });
            patch.quads[k] = q0;
            patch.quads[k + 1] = m01;
            patch.quads[k + 2] = centre;
            patch.quads[k + 3] = m30;
            patch.quads.extend_from_slice(&[q1, m12, centre, m01, q2, m23, centre, m12, q3, m30, centre, m23]);
            i = i.wrapping_add(4);
        }
        let cutoff = patch.quad_tri_cutoff;
        let mut i = 0u16;
        while i < cutoff {
            let k = i as usize;
            let (t0, t1, t2) = (patch.quads[k], patch.quads[k + 1], patch.quads[k + 2]);
            let (p0, p1, p2) = (vert(patch, t0), vert(patch, t1), vert(patch, t2));
            let m01 = midpoint(patch, &mut mids, &mut next, p0, p1);
            let m12 = midpoint(patch, &mut mids, &mut next, p1, p2);
            let m20 = midpoint(patch, &mut mids, &mut next, p2, p0);
            let centre = next;
            next = next.wrapping_add(1);
            let sx = p0.x.wrapping_add(p1.x).wrapping_add(p2.x);
            let sy = p0.y.wrapping_add(p1.y).wrapping_add(p2.y);
            patch.verts.push(Int2 { x: sx / 3, y: sy / 3 });
            patch.quads.extend_from_slice(&[t0, m01, centre, m20, t1, m12, centre, m01, t2, m20, centre, m12]);
            i = i.wrapping_add(3);
        }
        patch.quads.drain(0..patch.quad_tri_cutoff as usize);
        patch.quad_tri_cutoff = 0;
    }
    patch.state = HexPatchState::Sort;
}

fn patch_sort(patch: &mut HexPatch) {
    let count = patch.quads.len() as i32;
    let mut sorted: Vec<u8> = Vec::with_capacity(count as usize);
    let cost = |patch: &HexPatch, i: u8| {
        let v = patch.verts[i as usize];
        get_vert_cost(f2(v.x as f32, v.y as f32))
    };
    let mut i = 0i32;
    while i < count {
        let k = i as usize;
        let (a, b, c, d) = (patch.quads[k], patch.quads[k + 1], patch.quads[k + 2], patch.quads[k + 3]);
        let (ca, cb, cc, cd) = (cost(patch, a), cost(patch, b), cost(patch, c), cost(patch, d));
        // the game's lowest cost leaves out the fourth corner: Min(Min(a, b), Min(b, c))
        let lowest = mathf::min(mathf::min(ca, cb), mathf::min(cb, cc));
        let (mut lo, mut hi, mut mid, mut last) = (0i32, i / 4 - 1, 0i32, 0f32);
        while lo <= hi {
            mid = (lo + hi) / 2;
            last = cost(patch, sorted[list_index(mid * 4)]);
            if lowest == last {
                break;
            }
            if lowest < last {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }
        if last < lowest && 0 < i {
            mid += 1;
        }
        let at = list_index(mid * 4);
        let order = if !(cb <= ca || cc <= ca || cd <= ca) {
            [a, b, c, d]
        } else if !(cc <= cb || cd <= cb) {
            [b, c, d, a]
        } else if cc < cd {
            [c, d, a, b]
        } else {
            [d, a, b, c]
        };
        sorted.splice(at..at, order);
        i += 4;
    }
    patch.quads.clear();
    patch.quads.extend_from_slice(&sorted);
    patch.state = HexPatchState::Done;
}

/// The mesh `GridGenerator.PopulateBorderMesh` fills (`Mesh.Clear`, then `SetVertices` / `SetUVs(0)` /
/// `SetTriangles`): the quads round a vert that cross `Graph.border`, in world plane coordinates, with UVs into
/// `AoBaker.marchingSquaresTex`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BorderMesh {
    pub vertices: Vec<Vector3>,
    pub uv0: Vec<Vector2>,
    pub triangles: Vec<i32>,
}

/// `(n + (quadIndex - 20) * 4) % n`: where `GetAdjecentQuad` / `AppendQuadList` start looking in a patch's quads.
fn adjacent_start(n: i32, quad_index: u8) -> i32 {
    n.wrapping_add((quad_index as i32 - 20).wrapping_mul(4)) % n
}

impl GridGenerator {
    /// The quad `quads[i..i + 4]` of a patch, corners in `order`, with the bit's relaxed positions.
    fn quad_of(&self, p: usize, b: usize, i: usize, order: [usize; 4]) -> Quad {
        let patch = &self.patches[p];
        let bit = &self.bits[b];
        let q: [u8; 4] = std::array::from_fn(|k| patch.quads[i + order[k]]);
        Quad {
            quad_index: (i / 4) as u8,
            pad: [0; 3],
            v0: patch.verts[q[0] as usize],
            v1: patch.verts[q[1] as usize],
            v2: patch.verts[q[2] as usize],
            v3: patch.verts[q[3] as usize],
            p0: bit.verts[q[0] as usize],
            p1: bit.verts[q[1] as usize],
            p2: bit.verts[q[2] as usize],
            p3: bit.verts[q[3] as usize],
        }
    }

    /// `Placemaker.GridGenerator.GetQuad(int2, int2, int2)` 0xE2A740: the quad of the finished bit around
    /// `hex_within_patch` that has the edge `corner0 → corner1`, starting at `corner0`; a default (invalid) quad
    /// when there is none. Its `quadIndex` is 0 (the game leaves it unset).
    pub fn get_quad_by_edge(&self, hex_within_patch: Int2, corner0: Int2, corner1: Int2) -> Quad {
        let h3 = hex::get_hex3(hex_within_patch);
        let pp = hex::get_hex2(hex_to_patch_hex_f(f3(h3.x as f32, h3.y as f32, h3.z as f32)));
        let Some(&p) = self.patch_dict.get(&pp) else { return Quad::default() };
        if !self.patches[p].done() {
            return Quad::default();
        }
        let Some(&b) = self.bit_dict.get(&pp) else { return Quad::default() };
        if self.bits[b].state != GridBitState::Done {
            return Quad::default();
        }
        let patch = &self.patches[p];
        let mut i = 0;
        while i < patch.quads.len() {
            for r in 0..4 {
                let order = [r % 4, (r + 1) % 4, (r + 2) % 4, (r + 3) % 4];
                let w0 = patch.verts[patch.quads[i + order[0]] as usize];
                let w1 = patch.verts[patch.quads[i + order[1]] as usize];
                if w0 == corner0 && w1 == corner1 {
                    return Quad { quad_index: 0, ..self.quad_of(p, b, i, order) };
                }
            }
            i += 4;
        }
        Quad::default()
    }

    /// `Placemaker.GridGenerator.GetQuad(float2)` 0xE2AD80: the quad under a plane position. The three bits
    /// round the nearest cluster, nearest first, each when finished and wanted: a binary search (5 halvings) in
    /// its cost-sorted quads for the position's cost - 0.5, then quads alternately after and before that point
    /// (each side stops when its first corner leaves `y - 3.5 .. y + 4.5`), the first within √6 of the point
    /// whose four edges all have it on their inner side.
    pub fn get_quad(&self, plane_pos: Float2) -> Quad {
        use crate::os::extra_math::rotate2d_90;
        let h = plane_to_hex(plane_pos);
        let round = |v: f32| (v as f64).round_ties_even() as f32;
        let r = i3(mathf_cvtt(round(h.x)), mathf_cvtt(round(h.y)), mathf_cvtt(round(h.z)));
        let cc = hex::get_hex2(get_closest_cluster_pos(r));
        let off = get_patch_offset(cc);
        let mut c = [
            hex::get_hex3(add2(cc, off)),
            hex::get_hex3(add2(cc, hex::rotate_cw2_2i(off))),
            hex::get_hex3(add2(cc, hex::rotate_ccw2_2i(off))),
        ];
        let dist = |c: Int3| {
            let (dx, dy, dz) = (h.x - c.x as f32, h.y - c.y as f32, h.z - c.z as f32);
            dx * dx + dy * dy + dz * dz
        };
        let mut d = [dist(c[0]), dist(c[1]), dist(c[2])];
        if d[0] > d[2] {
            c.swap(0, 2);
            d.swap(0, 2);
        }
        if d[0] > d[1] {
            c.swap(0, 1);
            d.swap(0, 1);
        }
        if d[2] > d[1] {
            c.swap(1, 2);
            d.swap(1, 2);
        }
        let (lo, hi) = (plane_pos.y - 3.5, plane_pos.y + 4.5);
        let cost = get_vert_cost(hex::get_hex2_f(h)) - 0.5;
        for _ in 0..3 {
            let cand = c[0];
            c.rotate_left(1);
            let d = i3(cand.x.wrapping_sub(r.x), cand.y.wrapping_sub(r.y), cand.z.wrapping_sub(r.z));
            if !(d.x.wrapping_abs() < 85 && d.y.wrapping_abs() < 85 && d.z.wrapping_abs() < 85) {
                continue;
            }
            let Some(&b) = self.bit_dict.get(&hex::get_hex2(cand)) else { continue };
            let bit = &self.bits[b];
            if bit.state != GridBitState::Done || bit.motivations == 0 {
                continue;
            }
            let Some(&p) = self.patch_dict.get(&bit.hex_pos) else { continue };
            let patch = &self.patches[p];
            let n = patch.quads.len() as i32;
            let vcost = |i: i32| {
                let v = patch.verts[patch.quads[list_index(i)] as usize];
                get_vert_cost(f2(v.x as f32, v.y as f32))
            };
            let (mut lo_i, mut hi_i, mut mid, mut found) = (0i32, n / 4 - 1, 0i32, 0f32);
            for _ in 0..5 {
                mid = (hi_i + lo_i) / 2;
                found = vcost(mid * 4);
                if cost == found {
                    break;
                }
                if cost < found {
                    hi_i = mid - 1;
                } else {
                    lo_i = mid + 1;
                }
            }
            let start = if cost <= found { mid } else { mid + 1 } * 4;
            let (mut up, mut down) = (start, start - 4);
            let (mut up_ok, mut down_ok) = (up < n, down >= 0);
            let mut going_up = true;
            while down_ok || up_ok {
                let this_up = going_up;
                going_up = !going_up;
                let i;
                if this_up {
                    if !up_ok {
                        continue;
                    }
                    i = up;
                    up += 4;
                    up_ok = up < n;
                } else {
                    if !down_ok {
                        continue;
                    }
                    i = down;
                    down -= 4;
                    down_ok = down >= 0;
                }
                let k = list_index(i);
                let q: [u8; 4] = std::array::from_fn(|j| patch.quads[k + j]);
                let pt: [Float2; 4] = std::array::from_fn(|j| bit.verts[q[j] as usize]);
                if this_up {
                    if pt[0].y > hi {
                        up_ok = false;
                        continue;
                    }
                } else if lo > pt[0].y {
                    down_ok = false;
                    continue;
                }
                let (dx, dy) = (pt[0].x - plane_pos.x, pt[0].y - plane_pos.y);
                if dx * dx + dy * dy > 6.0 {
                    continue;
                }
                let e0 = rotate2d_90(f2(pt[1].x - pt[0].x, pt[1].y - pt[0].y));
                if 0.0 > (plane_pos.y - pt[0].y) * e0.y + (plane_pos.x - pt[0].x) * e0.x {
                    continue;
                }
                let inside = |a: Float2, b: Float2| {
                    let e = rotate2d_90(f2(b.x - a.x, b.y - a.y));
                    !(0.0 > e.x * (plane_pos.x - a.x) + e.y * (plane_pos.y - a.y))
                };
                if !inside(pt[1], pt[2]) || !inside(pt[2], pt[3]) || !inside(pt[3], pt[0]) {
                    continue;
                }
                return Quad {
                    quad_index: (i / 4) as u8,
                    pad: [0; 3],
                    v0: patch.verts[q[0] as usize],
                    v1: patch.verts[q[1] as usize],
                    v2: patch.verts[q[2] as usize],
                    v3: patch.verts[q[3] as usize],
                    p0: pt[0],
                    p1: pt[1],
                    p2: pt[2],
                    p3: pt[3],
                };
            }
        }
        Quad::default()
    }

    /// `Placemaker.GridGenerator.GetClostestVertPosition` 0xE2A2A0: the corner of the quad under the position
    /// on the position's side of both diagonals (by the four edges' distances); only `hexPos` and `planePos`
    /// set, `quadCount = -1` when there is no quad.
    pub fn get_clostest_vert_position(&self, plane_pos: Float2) -> Vert {
        use crate::os::extra_math::rotate2d_90;
        let q = self.get_quad(plane_pos);
        let mut v = Vert::default();
        if !q.is_valid() {
            v.quad_count = -1;
            return v;
        }
        let p = plane_pos;
        let n0 = rotate2d_90(f2(q.p1.x - q.p0.x, q.p1.y - q.p0.y));
        let a0 = n0.y * (p.y - q.p0.y) + n0.x * (p.x - q.p0.x);
        let n1 = rotate2d_90(f2(q.p2.x - q.p1.x, q.p2.y - q.p1.y));
        let a1 = n1.y * (p.y - q.p1.y) + n1.x * (p.x - q.p1.x);
        let n2 = rotate2d_90(f2(q.p3.x - q.p2.x, q.p3.y - q.p2.y));
        let a2 = n2.x * (p.x - q.p2.x) + n2.y * (p.y - q.p2.y);
        let n3 = rotate2d_90(f2(q.p0.x - q.p3.x, q.p0.y - q.p3.y));
        let a3 = n3.x * (p.x - q.p3.x) + n3.y * (p.y - q.p3.y);
        let (h, pp) = if !(a2 > a0) {
            if !(a3 > a1) { (q.v3, q.p3) } else { (q.v2, q.p2) }
        } else if !(a3 > a1) {
            (q.v0, q.p0)
        } else {
            (q.v1, q.p1)
        };
        v.hex_pos = h;
        v.plane_pos = pp;
        v
    }

    /// The neighbour of `quad` across its edge `v0–v3` (`e = 0`) or `v1–v2` (`e = 1`), as `GetAdjecentQuad` /
    /// `AppendQuadList` find it; `cache` is the patch / bit last used (the game keeps it between steps).
    fn neighbour(&self, quad: &Quad, e: usize, cache: &mut (Int2, Option<(usize, usize)>)) -> Option<Option<Quad>> {
        let (a, b, c, d) = if e == 0 { (quad.v0, quad.v3, quad.v1, quad.v2) } else { (quad.v1, quad.v2, quad.v0, quad.v3) };
        let pos = Int2 {
            x: (a.x.wrapping_add(b.x).wrapping_mul(5).wrapping_sub(c.x.wrapping_add(d.x))) / 8,
            y: (a.y.wrapping_add(b.y).wrapping_mul(5).wrapping_sub(c.y.wrapping_add(d.y))) / 8,
        };
        let h3 = hex::get_hex3(pos);
        let pp = hex::get_hex2(hex_to_patch_hex_f(f3(h3.x as f32, h3.y as f32, h3.z as f32)));
        if pp != cache.0 {
            cache.0 = pp;
            let p = *self.patch_dict.get(&pp)?;
            let bi = *self.bit_dict.get(&pp)?;
            if self.bits[bi].state != GridBitState::Done {
                return None;
            }
            cache.1 = Some((p, bi));
        }
        let (p, bi) = cache.1?;
        let patch = &self.patches[p];
        let n = patch.quads.len() as i32;
        let start = adjacent_start(n, quad.quad_index);
        let (want0, want1) = if e == 0 { (quad.v0, quad.v3) } else { (quad.v2, quad.v1) };
        let mut k = 0;
        while k < n {
            let i = list_index((k + start) % n);
            for r in 0..4 {
                let w = |j: usize| patch.verts[patch.quads[i + (r + j) % 4] as usize];
                if w(0) == want0 && w(1) == want1 {
                    let order = if e == 0 {
                        [(r + 3) % 4, r, (r + 1) % 4, (r + 2) % 4]
                    } else {
                        [(r + 1) % 4, (r + 2) % 4, (r + 3) % 4, r]
                    };
                    return Some(Some(self.quad_of(p, bi, i, order)));
                }
            }
            k += 4;
        }
        Some(None)
    }

    /// `Placemaker.GridGenerator.GetAdjecentQuad` 0xE29AB0: the finished quad across `quad`'s edge `v0–v3`,
    /// starting one corner before that edge; a default quad when there is none. No callers in the game
    /// (`AppendQuadList` does the same inline).
    pub fn get_adjecent_quad(&self, quad: &Quad) -> Quad {
        let mut cache = (Int2 { x: i32::MIN, y: i32::MIN }, None);
        match self.neighbour(quad, 0, &mut cache) {
            Some(Some(q)) => q,
            _ => Quad::default(),
        }
    }

    /// `Placemaker.GridGenerator.AppendQuadList` 0xE289A0 (`DrawPlane.IterateUpdate`): grows the strip of
    /// quads `nodes` up to `steps` quads at each end, across edges `v0–v3` from the last node, then (list
    /// reversed) across `v1–v2` from the first, `hover_index` counting the quads put before it; a step stops
    /// the end when the new quad's first edge faces along `plane_camera_dir`'s perpendicular. True when a quad
    /// was added.
    pub fn append_quad_list(&self, nodes: &mut Vec<Quad>, plane_camera_dir: Vector2, hover_index: &mut i32, steps: i32) -> bool {
        use crate::os::extra_math::rotate2d_90;
        let perp = rotate2d_90(f2(plane_camera_dir.x, plane_camera_dir.y));
        let mut added = false;
        let mut cache = (Int2 { x: -1, y: -1 }, None);
        for e in 0..2 {
            let mut cur = *nodes.last().expect("AppendQuadList: nodes[Count - 1] of an empty list");
            for _ in 0..steps.max(0) {
                if !cur.is_valid() {
                    break;
                }
                let Some(found) = self.neighbour(&cur, e, &mut cache) else { break };
                let Some(q) = found else { continue };
                let d = f2(q.p0.x - q.p1.x, q.p0.y - q.p1.y);
                if perp.y * d.y + perp.x * d.x > 0.0 {
                    cur = Quad::default();
                } else {
                    nodes.push(q);
                    added = true;
                    if e == 1 {
                        *hover_index += 1;
                    }
                    cur = q;
                }
            }
            nodes.reverse();
        }
        added
    }

    /// `Placemaker.GridGenerator.GetVertOrIterate` 0xE2BC70: the `Vert` at a grid position with the quads
    /// round it, from the up to three bits whose patches share it (`HexToPatchHexes`). An unfinished bit is
    /// iterated while `keep_going` allows (no `keep_going`: `quadCount = -2`); a missing bit gives
    /// `quadCount = -1`, one still unfinished `-3`. Stops once the quads' angles close the circle (> 350).
    pub fn get_vert_or_iterate(&mut self, hex_pos: Int2, mut keep_going: Option<&mut dyn FnMut() -> bool>) -> Vert {
        let (m, mask) = hex_to_patch_hexes(hex::get_hex3(hex_pos));
        let mut vert = Vert { hex_pos, ..Default::default() };
        let mut bits = [None; 3];
        for (i, slot) in bits.iter_mut().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            let Some(&b) = self.bit_dict.get(&hex::get_hex2(m.c[i])) else {
                vert.quad_count = -1;
                return vert;
            };
            if self.bits[b].state != GridBitState::Done || !self.bits[b].patch_done {
                let Some(k) = keep_going.as_mut() else {
                    vert.quad_count = -2;
                    return vert;
                };
                while k() {
                    self.iterate_bit(b, &mut **k);
                }
                if self.bits[b].state != GridBitState::Done || !self.bits[b].patch_done {
                    vert.quad_count = -3;
                    return vert;
                }
            }
            *slot = Some(b);
        }
        for i in 0..3 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let b = bits[i].unwrap();
            let p = self.patch_at(self.bits[b].hex_pos);
            let n = self.patches[p].quads.len();
            let mut c = 0;
            while c < n {
                for r in 0..4 {
                    let patch = &self.patches[p];
                    let q = patch.quads[c + r];
                    if patch.verts[q as usize] != hex_pos {
                        continue;
                    }
                    let quad = self.quad_of(p, b, c, [0, 1, 2, 3]);
                    if vert.quad_count == 0 {
                        vert.plane_pos = quad.get_plane_pos(r as i32);
                    }
                    let patch = &self.patches[p];
                    let b1 = patch.quads[c + (r + 1) % 4];
                    let b3 = patch.quads[c + (r + 3) % 4];
                    let bit = &self.bits[b];
                    let (a, n1, n3) = (bit.verts[q as usize], bit.verts[b1 as usize], bit.verts[b3 as usize]);
                    vert.add_quad(quad);
                    let angle = vert.angle;
                    vert.angle = Vector2::angle(v2(n3.x - a.x, n3.y - a.y), v2(n1.x - a.x, n1.y - a.y)) + angle;
                    if 350.0 < vert.angle {
                        return vert;
                    }
                    break;
                }
                c += 4;
            }
        }
        vert
    }

    /// `Placemaker.GridGenerator.PopulateBorderMesh` 0xE37920 (`BorderDrawer.Draw`): for the three bits round
    /// the vert's nearest cluster (within 13.51 of it), each quad near the vert (first corner within 5) that
    /// crosses the edge of `border` (`WorldMaster.graph.border`) and has the vert as a corner: its 4 corners,
    /// 2 triangles, and UVs picking the marching-squares cell of its inside / outside pattern
    /// (`AoBaker.powers`, `marchingSquaresTex.texelSize`), shifted by 1 at the corner(s) on the vert.
    pub fn populate_border_mesh(&self, vert: &Vert, texel_size: Vector2, powers: Int4, border: Int3x2) -> BorderMesh {
        use crate::os::extra_math::{contains, pow2};
        let h3 = hex::get_hex3(vert.hex_pos);
        let cc = hex::get_hex2(get_closest_cluster_pos(h3));
        let mut off = get_patch_offset(cc);
        let vp = hex_to_plane_i3(h3);
        let (far, near) = (pow2(13.510949), pow2(5.0));
        let (tx, ty) = (texel_size.x, texel_size.y);
        let mut out = BorderMesh::default();
        let mut base = 0i32;
        for _ in 0..3 {
            let pos = add2(cc, off);
            off = hex::rotate_cw2_2i(off);
            let bp = hex_to_plane_i2(pos);
            let (dx, dy) = (bp.x - vp.x, bp.y - vp.y);
            if !(dx * dx + dy * dy <= far) {
                continue;
            }
            let Some(&b) = self.bit_dict.get(&pos) else { continue };
            if self.bits[b].state != GridBitState::Done {
                continue;
            }
            let Some(&p) = self.patch_dict.get(&pos) else { continue };
            let patch = &self.patches[p];
            let bit = &self.bits[b];
            let mut k = 0;
            while k < patch.quads.len() {
                let q: [usize; 4] = std::array::from_fn(|j| patch.quads[k + j] as usize);
                k += 4;
                let p0 = bit.verts[q[0]];
                let (dx, dy) = (p0.x - vp.x, p0.y - vp.y);
                if !(dx * dx + dy * dy <= near) {
                    continue;
                }
                let h: [Int3; 4] = std::array::from_fn(|j| hex::get_hex3(patch.verts[q[j]]));
                let inside: [bool; 4] = std::array::from_fn(|j| contains(border, h[j]));
                if inside.iter().all(|&x| x == inside[0]) {
                    continue;
                }
                let on: [bool; 4] = std::array::from_fn(|j| vert.contains_vert(hex::get_hex2(h[j])));
                if !(on[3] || on[2] || on[1] || on[0]) {
                    continue;
                }
                for j in 0..4 {
                    let w = hex::plane_to_world(bit.verts[q[j]], 0.0);
                    out.vertices.push(Vector3 { x: w.x, y: w.y, z: w.z });
                }
                let wgt = |i: bool| -> i32 { if i { 0 } else { 4 } };
                let sum = wgt(inside[0])
                    .wrapping_mul(powers.x)
                    .wrapping_add(powers.y.wrapping_mul(wgt(inside[1])))
                    .wrapping_add(powers.z.wrapping_mul(wgt(inside[2])))
                    .wrapping_add(powers.w.wrapping_mul(wgt(inside[3])));
                let s = sum as f32 * tx;
                let u0 = tx * 0.5 + (s + s);
                let v0 = ty * 0.5 + 0.0;
                let u0b = u0 + tx * 0.0;
                let v0b = v0 + ty * 0.0 + 0.0;
                let u1 = u0 + tx;
                let v1 = v0 + ty + 0.0;
                let f = |b: bool| b as i32 as f32;
                out.uv0.push(v2(u0b + f(on[0]), v0b));
                out.uv0.push(v2(u1 + f(on[1]), v0b));
                out.uv0.push(v2(u1 + f(on[2]), v1));
                out.uv0.push(v2(u0b + f(on[3]), v1));
                for t in [0, 1, 2, 2, 3, 0] {
                    out.triangles.push(base + t);
                }
                base += 4;
            }
        }
        out
    }
}

/// `(int)` of a float (`cvttss2si`; out of range: `int.MinValue`).
fn mathf_cvtt(v: f32) -> i32 {
    crate::unity::conv::f32_to_i32(v)
}

/// `Placemaker.BitMeshQuadChange` (row E7d): a quad of a grid bit's mesh switched on or off. Nothing in the
/// shipped game uses it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BitMeshQuadChange {
    pub quad_index: u8,
    pub hex_pos: Int2,
    pub state: bool,
}

impl BitMeshQuadChange {
    /// `Placemaker.BitMeshQuadChange.System.IComparable<Placemaker.BitMeshQuadChange>.CompareTo` 0x7C1B40: by
    /// `hexPos.x`, then `hexPos.y`; the last step compares `quadIndex` with itself (always 0, kept).
    pub fn compare_to(&self, other: &BitMeshQuadChange) -> i32 {
        let c = self.hex_pos.x.cmp(&other.hex_pos.x) as i32;
        if c != 0 {
            return c;
        }
        let c = self.hex_pos.y.cmp(&other.hex_pos.y) as i32;
        if c != 0 {
            return c;
        }
        self.quad_index.cmp(&self.quad_index) as i32
    }
}
