//! `Placemaker.Sandbanker` (row E9c): the sandbanks and the shore water. Where a building's module meets the
//! water line (y = 0.1 in the qube), the edge becomes a strip on the ground: `Gather` collects those edges, square
//! by square within the frame budget, as quads of a ground vertex and a raised twin, each with a sand "coverage"
//! from the square's corners; `Smooth` spreads the coverage, pushes the strip outwards and shapes its slope;
//! `ApplyWater` builds the shore water mesh with flowing UVs; `ApplySand` the double-sided sand mesh with
//! tangents. `WorldMaster.Iterate` runs it (state `Sandbanker`) until it says it is done.
//!
//! Plain Rust over the town's data; the meshes go to the host ([`SandHost`]). Checked against the running game
//! by `tests/placemaker_sandbanker.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;

use crate::os::extra_math;
use crate::placemaker::graphs::{MaterialType, OrientedModuleSides};
use crate::placemaker::module_builder::{get_ts, multiply_point};
use crate::placemaker::modules::{ModuleLibrary, ModuleMath};
use crate::os::utils::ByteFloat2;
use crate::unity::mathf;
use crate::unity::quaternion::Matrix4x4;
use crate::unity::vector::{v2, v3, v4, Vector2, Vector3, Vector4};

/// `Sandbanker.State` (a `byte`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum State {
    #[default]
    Reset = 0,
    Gather = 1,
    Smooth = 2,
    ApplyWater = 3,
    ApplySand = 4,
    Done = 5,
}

/// The global `UpdateShadeUvs` sets.
pub const SAND_UV: &str = "_SandUv";

/// One child of `graph.squares` as `Gather` reads it: its `Square` (the corner matrices and the four corners'
/// `groundCoverage` and `angle`) and its first child's `Qube`.
#[derive(Clone, Debug, PartialEq)]
pub struct SandSquare {
    pub m: [Matrix4x4; 4],
    /// `c0..c3`: `(groundCoverage, angle)`.
    pub corners: [(u8, f32); 4],
    pub voxel_count: u8,
    pub possible_module_list_index: i32,
    /// `possibleModules[0]`, when the list isn't empty.
    pub first_possible_module: Option<OrientedModuleSides>,
}

/// What the sandbanker reaches outside itself.
pub trait SandHost {
    /// `WorldMaster.KeepGoing`: the frame has time left.
    fn keep_going(&mut self) -> bool;
    /// `graph.squares.childCount`.
    fn square_count(&mut self) -> i32;
    /// `graph.squares.GetChild(index)`, `None` when it has no children (no qube).
    fn square(&mut self, index: i32) -> Option<SandSquare>;
    /// That square transform's `TransformPoint` (the engine's; squares are only moved, so it is `p + position`).
    fn transform_point(&mut self, index: i32, p: Vector3) -> Vector3;
    /// `waterMf.sharedMesh`: cleared, then (at the end of `ApplyWater`) vertices, UV0 and triangles.
    fn set_water_mesh(&mut self, _verts: &[Vector3], _uvs: &[Vector2], _tris: &[i32]) {}
    /// `sandMf.sharedMesh`: cleared, then vertices, triangles, tangents and `RecalculateBounds`.
    fn set_sand_mesh(&mut self, _verts: &[Vector3], _tris: &[i32], _tangents: &[Vector4]) {}
    /// `Clear`: both meshes cleared.
    fn clear_meshes(&mut self) {}
}

/// `Vector3 !=`: not within 1e-5 (squared, `9.99999944e-11`).
fn ne(a: Vector3, b: Vector3) -> bool {
    !(a.sub(b).sqr_magnitude() < 9.99999944e-11)
}

/// `ExtraMath.SetY(float3, float)`.
fn set_y(v: Vector3, y: f32) -> Vector3 {
    v3(v.x, y, v.z)
}

/// `Placemaker.Sandbanker` (`.ctor` 0x8562E0: empty lists and dictionary).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Sandbanker {
    /// Per vertex pair: how sandy the bank is (0..1-ish).
    pub coverage: Vec<f32>,
    /// Pairs: the ground vertex, then its raised twin.
    pub verts: Vec<Vector3>,
    /// Quads of six: `(i0, i1, i1 + 1, i1 + 1, i0 + 1, i0)`, later triangles.
    pub tris: Vec<i32>,
    pub state: State,
    pub uv_pos: ByteFloat2,
    pub uv_size: ByteFloat2,
    /// The next square `Gather` reads.
    pub index: i32,
    /// Shared ground points: `RoundToInt(x * 100) * 6101 + RoundToInt(z * 100) * 6113` to the vertex pair, while
    /// only one edge has used it.
    pub dict: HashMap<i32, i32>,
}

impl Sandbanker {
    /// `Placemaker.Sandbanker.Reset` 0x856210 (the editor's `Reset`) and the state part of `OnEnable` 0x856120.
    pub fn reset(&mut self) {
        self.state = State::Reset;
    }

    /// `Placemaker.Sandbanker.Clear` 0x84FB40: both meshes cleared, back to `Reset`.
    pub fn clear(&mut self, host: &mut dyn SandHost) {
        host.clear_meshes();
        self.state = State::Reset;
    }

    /// `Placemaker.Sandbanker.UpdateShadeUvs` 0x856220 (also `OnEnable`, `OnValidate` 0x856200): `_SandUv =
    /// (uvPos, uvSize) * 0.25`.
    pub fn shade_uvs(&self) -> Vector4 {
        v4(self.uv_pos.fx() * 0.25, self.uv_pos.fy() * 0.25, self.uv_size.fx() * 0.25, self.uv_size.fy() * 0.25)
    }

    /// `Placemaker.Sandbanker.Iterate` 0x84FBA0: one step of the state machine. `Reset` clears; `Gather` reads
    /// squares while the frame has time (true, done when all are read); `Smooth`, `ApplyWater`, `ApplySand` run
    /// whole (true); `Done` (and past it) is false.
    pub fn iterate(&mut self, library: &ModuleLibrary, host: &mut dyn SandHost) -> bool {
        match self.state {
            State::Reset => {
                self.verts.clear();
                self.tris.clear();
                self.coverage.clear();
                self.index = 0;
                self.dict.clear();
                self.state = State::Gather;
                true
            }
            State::Gather => {
                self.gather(library, host);
                true
            }
            State::Smooth => {
                self.smooth();
                self.state = State::ApplyWater;
                true
            }
            State::ApplyWater => {
                self.apply_water(host);
                self.state = State::ApplySand;
                true
            }
            State::ApplySand => {
                self.apply_sand(host);
                self.state = State::Done;
                true
            }
            State::Done => false,
        }
    }

    /// The vertex pair for a ground point: the one an earlier edge left (taken out of the dictionary), or a new
    /// pair with the coverage `ts · (c0, c1, c2, c3)`.
    fn pair(&mut self, w: Vector3, ts: Vector4, cov: [f32; 4]) -> i32 {
        let key = mathf::round_to_int(w.z * 100.0)
            .wrapping_mul(0x17E1)
            .wrapping_add(mathf::round_to_int(w.x * 100.0).wrapping_mul(0x17D5));
        if let Some(i) = self.dict.remove(&key) {
            return i;
        }
        let i = self.verts.len() as i32;
        self.verts.push(w);
        self.verts.push(w);
        self.dict.insert(key, i);
        self.coverage.push(ts.x * cov[0] + ts.y * cov[1] + ts.z * cov[2] + ts.w * cov[3]);
        i
    }

    /// `Iterate`'s `Gather` ("Sandbanker Gather"): for each square (while `KeepGoing`), its first qube's first
    /// possible module; in each of the module's plain meshes (`materialType` 0), oriented, every triangle's first
    /// edge (trying each rotation of the triangle) lying on the water line (`|mid.y - 0.1| < 0.01`) becomes a quad
    /// on the ground (the edge's ends through the square, `y = 0`). Coverage: 0.8 at each corner when the qube has
    /// no upper voxel, else the corner's `groundCoverage * angle / 2880`.
    fn gather(&mut self, library: &ModuleLibrary, host: &mut dyn SandHost) {
        let n = host.square_count();
        while self.index < n {
            if !host.keep_going() {
                break;
            }
            if let Some(sq) = host.square(self.index) {
                if let (true, Some(oms)) = (sq.possible_module_list_index != -1, sq.first_possible_module) {
                    let om = oms.oriented_module;
                    let module = &library.modules[om.module_index as usize];
                    let cov = if sq.voxel_count & 0xF0 == 0 {
                        [0.8; 4]
                    } else {
                        sq.corners.map(|(gc, angle)| gc as f32 * angle * 0.00034722223)
                    };
                    let square = crate::placemaker::graphs::Square { m: sq.m, ..Default::default() };
                    for mm in &module.base.oriented_module_meshes {
                        let mesh = &library.module_meshes[mm.module_index as usize];
                        if mesh.material_type != MaterialType::None {
                            continue;
                        }
                        let o = ModuleMath::get().orientations[ModuleMath::mul_orientations(om.orientation(), mm.orientation()) as usize];
                        for t in (0..mesh.tris.len()).step_by(3) {
                            let mut p = [0, 1, 2].map(|k| o.multiply_point(mesh.verts[mesh.tris[t + k] as usize]).to_vector3());
                            for _ in 0..3 {
                                if ((p[1].y + p[0].y) * 0.5 - 0.1).abs() < 0.01 {
                                    if !o.inverted {
                                        p.swap(0, 1);
                                    }
                                    let p = p.map(|v| v3(0.5 + v.x, 0.5 + v.y, 0.5 + v.z));
                                    let ts0 = get_ts(p[0]);
                                    let ts1 = get_ts(p[1]);
                                    let w0 = set_y(host.transform_point(self.index, multiply_point(&square, ts0, p[0])), 0.0);
                                    let w1 = set_y(host.transform_point(self.index, multiply_point(&square, ts1, p[1])), 0.0);
                                    let i0 = self.pair(w0, ts0, cov);
                                    let i1 = self.pair(w1, ts1, cov);
                                    self.tris.extend([i0, i1, i1 + 1, i1 + 1, i0 + 1, i0]);
                                    break;
                                }
                                p.rotate_left(1);
                            }
                        }
                    }
                }
            }
            self.index += 1;
        }
        if n <= self.index {
            self.state = State::Smooth;
        }
    }

    /// `Iterate`'s `Smooth`. Over the quads (each the edge `i0, i1` on the ground with raised twins `+1`):
    /// - the coverage spreads along the edges, three times (each pair moves half the difference);
    /// - each pair's outward normal is the sum of its edges' `(-dz, 0, dx)`; the ground vertex moves out by it
    ///   times `Lerp(0.8, 0.2, coverage)` (the sum, not yet normalised), which is then normalised;
    /// - six rounds of relaxing the ground line (each edge pulls its ends half its length together);
    /// - the twins go up 0.186;
    /// - each quad (the last first) is checked for its twins being outside along the pairs' normals: a quad with
    ///   neither twin outside lies flat on the ground, one with only the second's outside is turned and its first
    ///   twin laid down, one with only the first outside has its second laid down, and one with both is turned
    ///   if that makes the shorter diagonal;
    /// - three rounds over the triangles (the last first): a ground edge under a raised point spreads its ends
    ///   apart (0.3 of the edge); in the first round, a raised edge pushes the ground point opposite to 0.3 off it
    ///   (by 0.6 of the gap);
    /// - the slopes are capped at 0.4 (a raised point at most 0.4 of its distance from the ground point it leans
    ///   on, or of its distance across a ground edge);
    /// - each twin moves in along its pair's normal by 0.4 of its height.
    fn smooth(&mut self) {
        let ntris = self.tris.len();
        let nv = self.verts.len();
        let half = nv / 2;
        let mut a = vec![Vector3::ZERO; half];
        let mut b = vec![Vector3::ZERO; half];
        let tri = |s: &Sandbanker, i: usize| s.tris[i] as usize;
        for _ in 0..3 {
            for t in (0..ntris).step_by(6) {
                let (i, j) = (tri(self, t) / 2, tri(self, t + 1) / 2);
                let d = (self.coverage[j] - self.coverage[i]) * 0.5;
                a[i] = v3(a[i].x + d, a[i].y + 0.0, a[i].z + 0.0);
                a[j] = v3(a[j].x - d, a[j].y - 0.0, a[j].z - 0.0);
            }
            for i in 0..half {
                self.coverage[i] = a[i].x + self.coverage[i];
                a[i] = Vector3::ZERO;
            }
        }
        for t in (0..ntris).step_by(6) {
            let (i, j) = (tri(self, t), tri(self, t + 1));
            let (vi, vj) = (self.verts[i], self.verts[j]);
            let dx = vj.x - vi.x;
            let nz = -(vj.z - vi.z);
            for k in [i / 2, j / 2] {
                b[k] = v3(b[k].x + nz, b[k].y + 0.0, b[k].z + dx);
            }
        }
        for i in 0..half {
            let n = b[i];
            let v = self.verts[2 * i];
            let l = mathf::lerp(0.8, 0.2, self.coverage[i]);
            self.verts[2 * i] = v3(n.x * l + v.x, n.y * l + v.y, n.z * l + v.z);
            b[i] = n.normalized();
        }
        for _ in 0..6 {
            for t in (0..ntris).step_by(6) {
                let (i, j) = (tri(self, t), tri(self, t + 1));
                let (vi, vj) = (self.verts[i], self.verts[j]);
                let d = v3((vj.x - vi.x) * 0.5, (vj.y - vi.y) * 0.5, (vj.z - vi.z) * 0.5);
                a[i / 2] = v3(a[i / 2].x + d.x, a[i / 2].y + d.y, a[i / 2].z + d.z);
                a[j / 2] = v3(a[j / 2].x - d.x, a[j / 2].y - d.y, a[j / 2].z - d.z);
            }
            for i in 0..half {
                let v = self.verts[2 * i];
                self.verts[2 * i] = v3(a[i].x + v.x, v.y + a[i].y, v.z + a[i].z);
                a[i] = Vector3::ZERO;
            }
        }
        for i in (1..nv).step_by(2) {
            let v = self.verts[i];
            self.verts[i] = v3(0.0f32 * 0.186 + v.x, 1.0f32 * 0.186 + v.y, 0.0f32 * 0.186 + v.z);
        }
        // the quads, the last first
        let flat = |v: Vector3| v3(v.x, 0.0, v.z);
        let mut t = ntris as i64 - 6;
        while t >= 0 {
            let t0 = t as usize;
            let (i0, i1, i1p, i0p) = (tri(self, t0), tri(self, t0 + 1), tri(self, t0 + 2), tri(self, t0 + 4));
            let (v0, v1, v1p, v0p) = (self.verts[i0], self.verts[i1], self.verts[i1p], self.verts[i0p]);
            let n0 = b[i0 / 2];
            let s0 = (v0p.y - v0.y) * n0.y + (v0p.x - v0.x) * n0.x + (v0p.z - v0.z) * n0.z < 0.0;
            let n1 = b[i1 / 2];
            let s1 = (v1p.y - v1.y) * n1.y + (v1p.x - v1.x) * n1.x + (v1p.z - v1.z) * n1.z < 0.0;
            let turn = |s: &mut Sandbanker| {
                s.tris[t0..t0 + 6].copy_from_slice(&[i0 as i32, i1 as i32, i0p as i32, i1p as i32, i0p as i32, i1 as i32]);
            };
            if !(s0 && s1) {
                if !s0 {
                    if s1 {
                        turn(self);
                        self.verts[i0p] = flat(v0p);
                        self.verts[i0] = flat(v0p);
                    } else {
                        self.verts[i0] = flat(v0p);
                        self.verts[i1] = flat(v1p);
                        self.verts[i1p] = flat(v1p);
                        self.verts[i0p] = flat(v0p);
                    }
                } else {
                    self.verts[i1p] = flat(v1p);
                    self.verts[i1] = flat(v1p);
                }
            } else {
                let d0 = v0.sub(v1p).sqr_magnitude();
                let d1 = v1.sub(v0p).sqr_magnitude();
                if d1 < d0 {
                    turn(self);
                }
            }
            t -= 6;
        }
        // the triangles, three rounds
        for pass in 0..3 {
            let mut t = self.tris.len() as i64 - 3;
            while t >= 0 {
                let t0 = t as usize;
                let mut k = [self.tris[t0], self.tris[t0 + 1], self.tris[t0 + 2]];
                let mut p = k.map(|i| self.verts[i as usize]);
                for _ in 0..3 {
                    if p[0].y == 0.0 && p[1].y == 0.0 && 0.0 < p[2].y {
                        let e = set_y(p[1].sub(p[0]), 0.0);
                        let e = v3(e.x * 0.3, e.y * 0.3, e.z * 0.3);
                        if k[0] % 2 == 0 {
                            let i = (k[0] / 2) as usize;
                            a[i] = v3(e.x + a[i].x, a[i].y + e.y, a[i].z + e.z);
                        }
                        if k[1] % 2 == 0 {
                            let i = (k[1] / 2) as usize;
                            a[i] = v3(a[i].x - e.x, a[i].y - e.y, a[i].z - e.z);
                        }
                        break;
                    }
                    if pass == 0 && k[0] % 2 == 1 && k[1] % 2 == 1 && k[2] % 2 == 0 && (0.0 < p[0].y || 0.0 < p[1].y) {
                        let d = set_y(p[1].sub(p[0]), 0.0);
                        let perp = v3(d.z, 0.0, -d.x).normalized();
                        let s = 0.3 - ((p[2].x - p[0].x) * perp.x + (p[2].y - p[0].y) * perp.y + (p[2].z - p[0].z) * perp.z);
                        let delta = v3(perp.x * s * 0.6, perp.y * s * 0.6, perp.z * s * 0.6);
                        let i = (k[2] / 2) as usize;
                        a[i] = v3(a[i].x + delta.x, a[i].y + delta.y, a[i].z + delta.z);
                        break;
                    }
                    k.rotate_left(1);
                    p.rotate_left(1);
                }
                t -= 3;
            }
            for i in 0..half {
                if ne(a[i], Vector3::ZERO) {
                    let v = self.verts[2 * i];
                    self.verts[2 * i] = v3(v.x + a[i].x, v.y + a[i].y, v.z + a[i].z);
                    a[i] = Vector3::ZERO;
                }
            }
        }
        // the slopes
        let mut t = self.tris.len() as i64 - 3;
        while t >= 0 {
            let t0 = t as usize;
            let mut k = [self.tris[t0], self.tris[t0 + 1], self.tris[t0 + 2]];
            let mut p = k.map(|i| self.verts[i as usize]);
            let zeros = p.iter().filter(|v| v.y == 0.0).count();
            let mut r = 0;
            'tri: {
                if zeros < 2 {
                    while p[0].y != 0.0 {
                        k.rotate_left(1);
                        p.rotate_left(1);
                        r += 1;
                        if r > 2 {
                            break 'tri;
                        }
                    }
                    let h = Vector3::distance(p[0], p[1]) * 0.4;
                    if h < p[1].y {
                        self.verts[k[1] as usize] = set_y(p[1], h);
                    }
                    let h = Vector3::distance(p[0], p[2]) * 0.4;
                    if h < p[2].y {
                        self.verts[k[2] as usize] = set_y(p[2], h);
                    }
                } else {
                    while p[0].y != 0.0 || p[1].y != 0.0 || p[2].y <= 0.0 || !ne(p[0], p[2]) || !ne(p[1], p[2]) {
                        k.rotate_left(1);
                        p.rotate_left(1);
                        r += 1;
                        if r > 2 {
                            break 'tri;
                        }
                    }
                    let n = Vector3::cross(Vector3::UP, set_y(p[1].sub(p[0]), 0.0)).normalized();
                    let h = ((p[2].x - p[0].x) * n.x + n.y * (p[2].y - p[0].y) + n.z * (p[2].z - p[0].z)) * 0.4;
                    if h < p[2].y {
                        self.verts[k[2] as usize] = set_y(p[2], h);
                    }
                }
            }
            t -= 3;
        }
        for i in 0..half {
            let v = self.verts[2 * i + 1];
            let n = b[i];
            self.verts[2 * i + 1] = v3(v.x - v.y * n.x * 0.4, v.y - v.y * n.y * 0.4, v.z - v.y * n.z * 0.4);
        }
    }

    /// `Iterate`'s `ApplyWater`: the shore water. Each pair gives two vertices at its ground point; the first moves
    /// out along the pair's water normal (the sum of its edges' half-turned directions) by `Lerp(2, 0.3,
    /// coverage)`; each quad gets a middle vertex between its first vertices and three triangles; the middle edges
    /// relax twice (0.3 of each edge). UVs: `x` from Perlin noise at the second vertex (`LerpUnclamped(n, 0.5,
    /// 0.1)`, `+ 1` for the second), `y` along the flow: half and 0.3 of the bank's lengths and the middle's
    /// distance across (`* 0.2`).
    fn apply_water(&mut self, host: &mut dyn SandHost) {
        let nv = self.verts.len();
        let half = nv / 2;
        let ntris = self.tris.len();
        let mut w: Vec<Vector3> = Vec::new();
        let mut n: Vec<Vector3> = Vec::new();
        let mut d: Vec<Vector3> = Vec::new();
        for i in (0..nv).step_by(2) {
            w.push(self.verts[i]);
            w.push(self.verts[i]);
            n.push(Vector3::ZERO);
            n.push(Vector3::ZERO);
            d.push(Vector3::ZERO);
        }
        for t in (0..ntris).step_by(6) {
            let (i, j) = (self.tris[t] as usize, self.tris[t + 1] as usize);
            let dn = self.verts[i].sub(self.verts[j]).normalized();
            let (wx, wz) = (dn.z * 0.5, -(dn.x * 0.5));
            for k in [i, j] {
                n[k] = v3(wx + n[k].x, n[k].y + 0.0, n[k].z + wz);
            }
        }
        for i in 0..half {
            let l = mathf::lerp(2.0, 0.3, self.coverage[i]);
            let (nn, ww) = (n[2 * i], w[2 * i]);
            w[2 * i] = v3(nn.x * l + ww.x, nn.y * l + ww.y, nn.z * l + ww.z);
        }
        let mut wt: Vec<i32> = Vec::new();
        for t in (0..ntris).step_by(6) {
            let (a, b) = (self.tris[t], self.tris[t + 1]);
            let m = w.len() as i32;
            let (wa, wb) = (w[a as usize], w[b as usize]);
            w.push(v3((wb.x + wa.x) * 0.5, (wa.y + wb.y) * 0.5, (wa.z + wb.z) * 0.5));
            wt.extend([m, b + 1, a + 1, a, m, a + 1, m, b, b + 1]);
        }
        while d.len() < w.len() {
            d.push(Vector3::ZERO);
        }
        for _ in 0..2 {
            for k in (0..wt.len()).step_by(3) {
                if k % 9 == 0 {
                    continue;
                }
                let (i, j) = (wt[k] as usize, wt[k + 1] as usize);
                let dd = v3((w[j].x - w[i].x) * 0.3, (w[j].y - w[i].y) * 0.3, (w[j].z - w[i].z) * 0.3);
                d[i] = v3(d[i].x + dd.x, dd.y + d[i].y, dd.z + d[i].z);
                d[j] = v3(d[j].x - dd.x, d[j].y - dd.y, d[j].z - dd.z);
            }
            for i in 0..w.len() {
                if ne(d[i], Vector3::ZERO) {
                    w[i] = v3(d[i].x + w[i].x, d[i].y + w[i].y, d[i].z + w[i].z);
                    d[i] = Vector3::ZERO;
                }
            }
        }
        let mut uv: Vec<Vector2> = Vec::new();
        for i in (0..nv).step_by(2) {
            let b = w[i + 1];
            let noise = mathf::perlin_noise(b.x * 0.1, b.z * 0.1);
            let u = mathf::lerp_unclamped(noise, 0.5, 0.1);
            uv.push(v2(u, 0.0));
            uv.push(v2(u + 1.0, 0.0));
        }
        for t in (0..ntris).step_by(6) {
            let (a, b) = (self.tris[t] as usize, self.tris[t + 1] as usize);
            uv.push(v2((uv[a].x + uv[b].x) * 0.5, (uv[b].y + uv[a].y) * 0.5));
        }
        for k in (0..wt.len()).step_by(9) {
            let (m, b1, a1, a, b) = (wt[k] as usize, wt[k + 1] as usize, wt[k + 2] as usize, wt[k + 3] as usize, wt[k + 7] as usize);
            let la = w[a1].sub(w[a]).magnitude();
            let lb = w[b1].sub(w[b]).magnitude();
            let dm = w[m].sub(w[b1]);
            let r = extra_math::rotate90_y(w[a1].sub(w[b1])).normalized();
            let s = r.x * dm.x + dm.y * r.y + dm.z * r.z;
            let s2 = s * 0.2;
            uv[a].y = la * 0.5 + uv[a].y;
            uv[b].y = lb * 0.5 + uv[b].y;
            uv[b1].y = lb * 0.3 + s2 + uv[b1].y;
            uv[a1].y = la * 0.3 + s2 + uv[a1].y;
            uv[m].y = s + uv[m].y;
        }
        host.set_water_mesh(&w, &uv, &wt);
    }

    /// `Iterate`'s `ApplySand`: triangles lying wholly on the ground are dropped (the last first); every triangle
    /// gets a back face on a second copy of the vertices; the tangents are the summed face normals, normalised, `w`
    /// the Perlin-noise shade (`noise + 0.5`, negated on the back copy).
    fn apply_sand(&mut self, host: &mut dyn SandHost) {
        let nv = self.verts.len();
        let mut tan = vec![Vector4::ZERO; nv];
        let mut t = self.tris.len() as i64 - 3;
        while t >= 0 {
            let t0 = t as usize;
            let k = [self.tris[t0], self.tris[t0 + 1], self.tris[t0 + 2]].map(|i| self.verts[i as usize]);
            if k[0].y == 0.0 && k[1].y == 0.0 && k[2].y == 0.0 {
                self.tris.drain(t0..t0 + 3);
            }
            t -= 3;
        }
        let n = self.tris.len();
        for t in (0..n).step_by(3) {
            let (a, b, c) = (self.tris[t], self.tris[t + 1], self.tris[t + 2]);
            self.tris.push(nv as i32 + a);
            self.tris.push(nv as i32 + c);
            self.tris.push(nv as i32 + b);
            let (va, vb, vc) = (self.verts[a as usize], self.verts[b as usize], self.verts[c as usize]);
            let cr = Vector3::cross(vb.sub(va), vc.sub(va));
            let add = extra_math::set_w(cr, 0.0);
            for i in [a, b, c] {
                let q = tan[i as usize];
                tan[i as usize] = v4(q.x + add.x, q.y + add.y, q.z + add.z, q.w + add.w);
            }
        }
        for i in (0..nv).step_by(2) {
            let v = self.verts[i];
            // (`PerlinNoise` returns on the x87 stack; the `+ 0.5` is done there, rounded to float once)
            let shade = (mathf::perlin_noise_x87(v.x * 0.2, v.z * 0.2) + 0.5) as f32;
            for idx in [i + 1, i] {
                let tn = tan[idx].normalized();
                tan[idx] = v4(tn.x, tn.y, tn.z, shade);
                tan.push(v4(tn.x, tn.y, tn.z, -shade));
            }
        }
        self.verts.extend_from_within(..);
        host.set_sand_mesh(&self.verts, &self.tris, &tan);
    }
}
