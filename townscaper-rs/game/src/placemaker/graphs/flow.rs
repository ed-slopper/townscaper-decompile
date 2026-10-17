//! `Placemaker.Graphs.Flow` (row E4f): a distance field through the open space round the town, kept in each
//! corner's `flowDatas` (one per height): every frame each open cell takes the smallest neighbour distance
//! (+ the step) and a direction down the slope; `AddFlowTarget` seeds it, `SampleFlow` reads it back at a point.

use super::{CornerId, FlowData, Graph};
use crate::os::{extra_math, hex};
use crate::placemaker::grid_generator::{self, GridGenerator};
use crate::unity::mathematics::{f2, f3, Float2, Float3, Int3, Int3x2};
use crate::unity::mathf;
use crate::unity::vector::{Vector2, Vector3};

/// `Flow`: its targets are the positions of its transform's children (`FlowTarget` objects).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Flow {
    pub targets: Vec<Vector3>,
}

/// `Flow.FlowCalcData`: one corner's flow data at one height while `Update` works on a quad.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FlowCalcData {
    flow_data: FlowData,
    corner: Option<CornerId>,
    count: i32,
    index: i32,
    available: bool,
    open: bool,
    pos: Vector3,
}

impl Default for FlowCalcData {
    fn default() -> Self {
        FlowCalcData { flow_data: FlowData::default(), corner: None, count: 0, index: 0, available: false, open: false, pos: Vector3::default() }
    }
}

const UP: Vector3 = Vector3 { x: 0.0, y: 1.0, z: 0.0 };

fn grow(b: Int3x2, by: i32) -> Int3x2 {
    Int3x2 {
        c0: Int3 { x: b.c0.x.wrapping_sub(by), y: b.c0.y.wrapping_sub(by), z: b.c0.z.wrapping_sub(by) },
        c1: Int3 { x: b.c1.x.wrapping_add(by), y: b.c1.y.wrapping_add(by), z: b.c1.z.wrapping_add(by) },
    }
}

/// The bounds test of `AddFlowTarget` / `SampleFlow`: `hex` outside the hex bounds grown by 288 (as floats).
fn outside(b: Int3x2, hex: Float3) -> bool {
    let w = grow(b, 288);
    hex.x < w.c0.x as f32 || hex.y < w.c0.y as f32 || hex.z < w.c0.z as f32 || (w.c1.x as f32) < hex.x || (w.c1.y as f32) < hex.y || (w.c1.z as f32) < hex.z
}

/// What the original does with the vertical `newDist` candidates: `(float)(ushort)(int)value` (asm:
/// `cvttss2si`, `movzx ax`): truncated, and out of `int` range (as `float.MaxValue` is) the x86 indefinite
/// integer `int.MinValue`, whose low 16 bits are 0.
fn ushort_float(v: f32) -> f32 {
    let i = if v >= -2147483648.0 && v < 2147483648.0 { v as i32 } else { i32::MIN };
    (i as u16) as f32
}

fn v2(f: Float2) -> Vector2 {
    Vector2 { x: f.x, y: f.y }
}

impl Flow {
    /// `Placemaker.Graphs.Flow.SetupFlowData` 0x6D2AA0: the corner's flow data at `index` (`FlowData.empty` past
    /// its column), whether there is one, and whether it is open (no voxel).
    fn setup_flow_data(graph: &Graph, f: &mut FlowCalcData) {
        f.available = f.index < f.count;
        f.flow_data = if f.available {
            graph.corner(f.corner.unwrap()).flow_datas[f.index as usize]
        } else {
            FlowData::EMPTY
        };
        f.open = f.flow_data.voxel.is_none();
    }

    /// `Placemaker.Graphs.Flow.FlowTwoFlows` 0x6D0500: between two available cells `space_dist` apart, the
    /// farther one (by `dist`; ties go to the second) gets the clamped difference along the direction from the
    /// first to the second added to `newDir`, the nearer one's distance + the step as a `newDist` candidate, and
    /// one more in `denom`.
    fn flow_two_flows(f0: &mut FlowCalcData, f1: &mut FlowCalcData, space_dist: f32) {
        if !(f0.available && f1.available) {
            return;
        }
        let t = mathf::clamp(f0.flow_data.dist - f1.flow_data.dist, -space_dist, space_dist);
        let dx = (f1.pos.x - f0.pos.x) / space_dist;
        let dy = (f1.pos.y - f0.pos.y) / space_dist;
        let dz = (f1.pos.z - f0.pos.z) / space_dist;
        let (to, from) = if t <= 0.0 { (f1, &*f0) } else { (f0, &*f1) };
        let from_dist = from.flow_data.dist;
        let d = &mut to.flow_data;
        d.new_dir = Vector3 { x: t * dx + d.new_dir.x, y: t * dy + d.new_dir.y, z: t * dz + d.new_dir.z };
        d.new_dist = mathf::min(d.new_dist, from_dist + space_dist);
        d.denom += 1.0;
    }

    /// `FlowTwoFlows` on two of the eight cells of `Update`'s working set.
    fn two(c: &mut [FlowCalcData; 8], a: usize, b: usize, space_dist: f32) {
        let (mut fa, mut fb) = (c[a], c[b]);
        Flow::flow_two_flows(&mut fa, &mut fb, space_dist);
        c[a] = fa;
        c[b] = fb;
    }

    /// `Placemaker.Graphs.Flow.Update` 0x6D2BA0: one step of the field. Each corner's column first takes its new
    /// values (`dist = newDist`, `newDist + 1` as the next candidate, `dir` the averaged `newDir`) and flows up and
    /// down between open neighbours (those candidates cut to a 16-bit integer, see `ushort_float`); then every
    /// target seeds it; then, over every finished grid patch near the town, each quad near the bounds flows
    /// between its four corners at every height: along the edges and across (√2), up to the next height (1, √2
    /// across an edge, √3 across the quad).
    pub fn update(&self, graph: &mut Graph, grid: &GridGenerator) {
        for ci in 0..graph.corners.len() {
            let c = graph.corners[ci];
            let fd = &mut graph.corner_mut(c).flow_datas;
            let n = fd.len();
            for d in fd.iter_mut() {
                if 0.0 < d.denom {
                    d.dir = Vector3 { x: d.new_dir.x / d.denom, y: d.new_dir.y / d.denom, z: d.new_dir.z / d.denom };
                    d.denom = 0.0;
                }
                d.dist = d.new_dist;
                d.new_dist = mathf::min(d.new_dist + 1.0, f32::MAX);
                d.new_dir = Vector3::default();
            }
            if n == 0 {
                continue;
            }
            let mut prev = fd[0];
            for k in 1..n {
                let mut cur = fd[k];
                if prev.voxel.is_none() && cur.voxel.is_none() {
                    let diff = prev.dist - cur.dist;
                    if diff <= 0.0 {
                        if diff < 0.0 {
                            cur.new_dir = Vector3 { x: cur.new_dir.x - UP.x, y: cur.new_dir.y - UP.y, z: cur.new_dir.z - UP.z };
                            cur.new_dist = ushort_float(mathf::min(cur.new_dist, prev.dist + 1.0));
                        }
                    } else {
                        prev.new_dir = Vector3 { x: prev.new_dir.x + UP.x, y: prev.new_dir.y + UP.y, z: prev.new_dir.z + UP.z };
                        prev.new_dist = ushort_float(mathf::min(prev.new_dist, cur.dist + 1.0));
                    }
                }
                fd[k - 1] = prev;
                prev = cur;
            }
            fd[n - 1] = prev;
        }
        for t in self.targets.iter().rev() {
            self.add_flow_target(graph, grid, f3(t.x, t.y, t.z));
        }
        let near = grow(graph.bounds, 72);
        let close = grow(graph.bounds, 18);
        let s2 = 2.0f64.sqrt() as f32;
        let s3 = 3.0f64.sqrt() as f32;
        for patch in &grid.patches {
            if !patch.done() || !extra_math::contains(near, hex::get_hex3(patch.hex_pos)) {
                continue;
            }
            let mut q = 0;
            while q < patch.quads.len() {
                let v0 = patch.verts[patch.quads[q] as usize];
                if extra_math::contains(close, hex::get_hex3(v0)) {
                    let vs = [v0, patch.verts[patch.quads[q + 1] as usize], patch.verts[patch.quads[q + 2] as usize], patch.verts[patch.quads[q + 3] as usize]];
                    Flow::flow_quad(graph, vs.map(|v| graph.corner_dict.get(&v).copied()), s2, s3);
                }
                q += 4;
            }
        }
    }

    /// The quad part of `Update`: cells 0..4 are the corners at the current height, 4..8 the next height.
    fn flow_quad(graph: &mut Graph, corners: [Option<CornerId>; 4], s2: f32, s3: f32) {
        let mut c = [FlowCalcData::default(); 8];
        for k in 0..4 {
            if let Some(id) = corners[k] {
                let p = graph.corner(id).local_position;
                c[k].corner = Some(id);
                c[k].pos = Vector3 { x: UP.x * 0.5 + p.x, y: UP.y * 0.5 + p.y, z: UP.z * 0.5 + p.z };
                c[k].count = graph.corner(id).flow_datas.len() as i32;
            }
        }
        let max = mathf::max_i32(mathf::max_i32(c[0].count, c[1].count), mathf::max_i32(c[2].count, c[3].count));
        for k in 0..4 {
            Flow::setup_flow_data(graph, &mut c[k]);
        }
        let (a, b, cc, d) = (0, 1, 2, 3);
        for i in 0..max {
            let open = |c: &[FlowCalcData; 8], k: usize| c[k].open;
            if open(&c, a) && open(&c, b) && open(&c, cc) && open(&c, d) {
                Flow::two(&mut c, a, cc, s2);
                Flow::two(&mut c, b, d, s2);
            }
            if open(&c, a) && open(&c, b) {
                Flow::two(&mut c, a, b, 1.0);
            }
            if open(&c, b) && open(&c, cc) {
                Flow::two(&mut c, b, cc, 1.0);
            }
            if open(&c, cc) && open(&c, d) {
                Flow::two(&mut c, cc, d, 1.0);
            }
            if open(&c, d) && open(&c, a) {
                Flow::two(&mut c, d, a, 1.0);
            }
            if i < max - 1 {
                for k in 0..4 {
                    let mut up = c[k];
                    up.index += 1;
                    up.pos.y = up.pos.y + 1.0;
                    Flow::setup_flow_data(graph, &mut up);
                    c[k + 4] = up;
                }
                let (a2, b2, c2, d2) = (4, 5, 6, 7);
                if (0..8).all(|k| c[k].open) {
                    Flow::two(&mut c, a, c2, s3);
                    Flow::two(&mut c, b, d2, s3);
                    Flow::two(&mut c, cc, a2, s3);
                    Flow::two(&mut c, d, b2, s3);
                }
                if open(&c, a) && open(&c, b) && open(&c, a2) && open(&c, b2) {
                    Flow::two(&mut c, a, b2, s2);
                    Flow::two(&mut c, b, a2, s2);
                }
                if open(&c, b) && open(&c, cc) && open(&c, c2) && open(&c, b2) {
                    Flow::two(&mut c, b, c2, s2);
                    Flow::two(&mut c, cc, b2, s2);
                }
                if open(&c, cc) && open(&c, d) && open(&c, d2) && open(&c, c2) {
                    Flow::two(&mut c, cc, d2, s2);
                    Flow::two(&mut c, d, c2, s2);
                }
                if open(&c, d) && open(&c, a) && open(&c, a2) && open(&c, d2) {
                    Flow::two(&mut c, d, a2, s2);
                    Flow::two(&mut c, a, d2, s2);
                }
                for k in 0..4 {
                    if open(&c, k) && open(&c, k + 4) {
                        Flow::two(&mut c, k, k + 4, 1.0);
                    }
                }
            }
            for k in 0..4 {
                if c[k].available {
                    let id = c[k].corner.unwrap();
                    graph.corner_mut(id).flow_datas[i as usize] = c[k].flow_data;
                }
            }
            if i < max - 1 {
                for k in 0..4 {
                    c[k] = c[k + 4];
                }
            }
        }
    }

    /// The grid quad under a point and the point's position in its cell (x / z on the quad, y up from the
    /// height), for `AddFlowTarget` and `SampleFlow`; `None` when the quad is invalid.
    fn cell(grid: &GridGenerator, p: Float2, yy: f32, h: i32) -> Option<(crate::placemaker::quads::Quad, Vector3)> {
        let q = grid.get_quad(p);
        if !q.is_valid() {
            return None;
        }
        let uv = extra_math::inv_bilinear(v2(q.p0), v2(q.p1), v2(q.p2), v2(q.p3), v2(p));
        Some((q, Vector3 { x: uv.x, y: yy - h as f32, z: uv.y }))
    }

    /// `Placemaker.Graphs.Flow.AddFlowTarget` 0x6CF970: a target at a point lowers `newDist` of the eight cells of
    /// its grid cell (the corners of the quad under it, at the two heights round it) to a product of its
    /// coordinates in the cell, where that is lower.
    pub fn add_flow_target(&self, graph: &mut Graph, grid: &GridGenerator, world_pos: Float3) {
        let yy = world_pos.y - 0.5;
        let h = mathf::floor_to_int(yy);
        if h < -1 || graph.bounds_height + 1 < h {
            return;
        }
        let p = f2(world_pos.x, world_pos.z);
        if outside(graph.bounds, grid_generator::plane_to_hex(p)) {
            return;
        }
        let Some((q, t)) = Flow::cell(grid, p, yy, h) else {
            return;
        };
        let valid = h >= 0;
        let (tx, ty, tz) = (t.x, t.y, t.z);
        let (ix, iy, iz) = (1.0 - tx, 1.0 - ty, 1.0 - tz);
        let weights = [
            (q.v0, tx * ty * tz, tx * iy * tz),
            (q.v1, ty * ix * tz, ix * iy * tz),
            (q.v2, ix * ty * iz, ix * iy * iz),
            (q.v3, tx * ty * iz, tx * iy * iz),
        ];
        for (vert, w0, w1) in weights {
            let Some(&c) = graph.corner_dict.get(&vert) else {
                continue;
            };
            let fd = &mut graph.corner_mut(c).flow_datas;
            let n = fd.len() as i32;
            if h < n && valid && w0 < fd[h as usize].new_dist {
                fd[h as usize].new_dist = w0;
            }
            if h + 1 < n && w1 < fd[(h + 1) as usize].new_dist {
                fd[(h + 1) as usize].new_dist = w1;
            }
        }
    }

    /// `Placemaker.Graphs.Flow.SampleFlow` 0x6D0940: at a point, (the flow direction: the cells' `dir` weighted
    /// trilinearly, the normal of the solid cells round it ×2 as `SampleNormalDistance` builds it on the plane
    /// edges, its coverage, the total weight of the cells found); below the ground or above the town (zero,
    /// zero, 1 - Clamp01(y - 0.5), 0); off the grid (zero, zero, 0, 0).
    pub fn sample_flow(&self, graph: &Graph, grid: &GridGenerator, world_pos: Vector3) -> (Vector3, Vector3, f32, f32) {
        let yy = world_pos.y - 0.5;
        let h = mathf::floor_to_int(yy);
        if h < -1 || graph.bounds_height + 1 < h {
            return (Vector3::default(), Vector3::default(), 1.0 - mathf::clamp01(yy), 0.0);
        }
        let p = f2(world_pos.x, world_pos.z);
        if outside(graph.bounds, grid_generator::plane_to_hex(p)) {
            return (Vector3::default(), Vector3::default(), 0.0, 0.0);
        }
        let Some((q, t)) = Flow::cell(grid, p, yy, h) else {
            return (Vector3::default(), Vector3::default(), 0.0, 0.0);
        };
        let (tx, ty, tz) = (t.x, t.y, t.z);
        let (ix, iy, iz) = (1.0 - tx, 1.0 - ty, 1.0 - tz);
        let valid = h >= 0;
        // solid[corner][0 = h, 1 = h + 1]: below the ground counts as solid
        let mut solid = [[h < 0, false]; 4];
        let mut dir = Vector3::default();
        let mut weight = 0.0f32;
        let wts = [(iy * ix * iz, ty * ix * iz), (iy * tx * iz, ty * tx * iz), (iy * tx * tz, ty * tx * tz), (ix * iy * tz, ty * ix * tz)];
        for (k, vert) in [q.v0, q.v1, q.v2, q.v3].into_iter().enumerate() {
            let Some(&c) = graph.corner_dict.get(&vert) else {
                continue;
            };
            let fd = &graph.corner(c).flow_datas;
            let n = fd.len() as i32;
            if h < n && valid {
                let d = fd[h as usize];
                solid[k][0] = d.voxel.is_some();
                let f = wts[k].0;
                dir = Vector3 { x: d.dir.x * f + dir.x, y: d.dir.y * f + dir.y, z: d.dir.z * f + dir.z };
                weight = f + weight;
            }
            if h + 1 < n {
                let d = fd[(h + 1) as usize];
                solid[k][1] = d.voxel.is_some();
                let f = wts[k].1;
                dir = Vector3 { x: d.dir.x * f + dir.x, y: d.dir.y * f + dir.y, z: d.dir.z * f + dir.z };
                weight = f + weight;
            }
        }
        // the normal of the solid cells, on the plane edges (x on x, plane y on z); grouping as in the asm
        // (the sideways weights multiply a precomputed pair, as in `Qube::sample_normal_coverage`)
        let x2 = tx * tx;
        let ix2 = ix * ix;
        let a_ix2 = 1.0 - ix2;
        let iy2 = iy * iy;
        let iz2 = iz * iz;
        let y2 = ty * ty;
        let a_iy2 = 1.0 - iy2;
        let a_iz2 = 1.0 - iz2;
        let z2 = tz * tz;
        let a_x2 = 1.0 - x2;
        let a_y2 = 1.0 - y2;
        let a_z2 = 1.0 - z2;
        let e01 = (q.p0.x - q.p1.x, q.p0.y - q.p1.y);
        let e32 = (q.p3.x - q.p2.x, q.p3.y - q.p2.y);
        let e03 = (q.p0.x - q.p3.x, q.p0.y - q.p3.y);
        let e12 = (q.p1.x - q.p2.x, q.p1.y - q.p2.y);
        let [s0, s1, s2, s3] = solid;
        let (mut nx, mut ny, mut nz) = (0.0f32, 0.0f32, 0.0f32);
        let mut cov = 0.0f32;
        if s0[0] {
            if !s1[0] {
                let w = ix2 * iy * iz - a_x2 * iy * iz;
                nx = e01.0 * w + 0.0;
                nz = e01.1 * w + 0.0;
                ny = 0.0 + 0.0;
            }
            if !s3[0] {
                let t = ix * iy;
                let w = iz2 * t - a_z2 * t;
                nz = w * e03.1 + nz;
                nx = nx + w * e03.0;
                ny = ny + 0.0;
            }
            if !s0[1] {
                ny = ny - (ix * iy2 * iz - ix * a_y2 * iz);
            }
            cov = ix * iy * iz + 0.0;
        }
        if s1[0] {
            if !s0[0] {
                let w = a_ix2 * iy * iz - x2 * iy * iz;
                nz = e01.1 * w + nz;
                nx = nx + e01.0 * w;
                ny = ny + 0.0;
            }
            if !s2[0] {
                let t = iy * tx;
                let w = iz2 * t - a_z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e12.0;
                nz = nz + w * e12.1;
            }
            if !s1[1] {
                ny = ny - (iy2 * tx * iz - a_y2 * tx * iz);
            }
            cov = iy * tx * iz + cov;
        }
        if s2[0] {
            if !s3[0] {
                let w = a_ix2 * iy * tz - x2 * iy * tz;
                ny = ny + 0.0;
                nx = nx + w * e32.0;
                nz = nz + w * e32.1;
            }
            if !s1[0] {
                let t = iy * tx;
                let w = a_iz2 * t - z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e12.0;
                nz = nz + w * e12.1;
            }
            if !s2[1] {
                ny = ny - (iy2 * tx * tz - a_y2 * tx * tz);
            }
            cov = iy * tx * tz + cov;
        }
        if s3[0] {
            if !s2[0] {
                let w = ix2 * iy * tz - a_x2 * iy * tz;
                nz = w * e32.1 + nz;
                ny = ny + 0.0;
                nx = nx + w * e32.0;
            }
            if !s0[0] {
                let t = ix * iy;
                let w = a_iz2 * t - z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e03.0;
                nz = nz + w * e03.1;
            }
            if !s3[1] {
                ny = ny - (iy2 * ix * tz - a_y2 * ix * tz);
            }
            cov = ix * iy * tz + cov;
        }
        if s0[1] {
            if !s1[1] {
                let w = ty * ix2 * iz - ty * a_x2 * iz;
                ny = ny + 0.0;
                nx = nx + e01.0 * w;
                nz = nz + e01.1 * w;
            }
            if !s3[1] {
                let t = ty * ix;
                let w = iz2 * t - a_z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e03.0;
                nz = nz + w * e03.1;
            }
            if !s0[0] {
                ny = ny - (a_iy2 * ix * iz - y2 * ix * iz);
            }
            cov = ty * ix * iz + cov;
        }
        if s1[1] {
            if !s0[1] {
                let w = ty * a_ix2 * iz - ty * x2 * iz;
                nz = e01.1 * w + nz;
                nx = nx + e01.0 * w;
                ny = ny + 0.0;
            }
            if !s2[1] {
                let t = ty * tx;
                let w = iz2 * t - a_z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e12.0;
                nz = nz + w * e12.1;
            }
            if !s1[0] {
                ny = ny - (a_iy2 * tx * iz - y2 * tx * iz);
            }
            cov = ty * tx * iz + cov;
        }
        if s2[1] {
            if !s3[1] {
                let w = ty * a_ix2 * tz - ty * x2 * tz;
                ny = ny + 0.0;
                nx = nx + w * e32.0;
                nz = nz + w * e32.1;
            }
            if !s1[1] {
                let t = ty * tx;
                let w = a_iz2 * t - z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e12.0;
                nz = nz + w * e12.1;
            }
            if !s2[0] {
                ny = ny - (a_iy2 * tx * tz - y2 * tx * tz);
            }
            cov = ty * tx * tz + cov;
        }
        if s3[1] {
            if !s2[1] {
                let w = ty * ix2 * tz - ty * a_x2 * tz;
                ny = ny + 0.0;
                nx = nx + w * e32.0;
                nz = nz + w * e32.1;
            }
            if !s0[1] {
                let t = ty * ix;
                let w = a_iz2 * t - z2 * t;
                ny = ny + 0.0;
                nx = nx + w * e03.0;
                nz = nz + w * e03.1;
            }
            if !s3[0] {
                ny = ny - (a_iy2 * ix * tz - y2 * ix * tz);
            }
            cov = ty * ix * tz + cov;
        }
        (dir, Vector3 { x: nx * 2.0, y: ny * 2.0, z: nz * 2.0 }, cov, weight)
    }
}
