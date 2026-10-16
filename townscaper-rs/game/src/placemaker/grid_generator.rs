//! `Placemaker.GridGenerator` and its `GridBit`s: the irregular grid, generated around the camera in patches,
//! clusters and bits (rows E3a–E3e). Row E3a: `GridBit`; row E3b: the coordinate maths (free functions here:
//! the instance ones never read the generator's state). Checked against the running game by
//! `tests/placemaker_grid.rs` (`TS_HARNESS=1`).
//!
//! Grid positions are hex positions × `GridConstants.hexScale` (12). Patch centres sit 72 apart
//! (`hexDistance`), on a lattice of multiples of 216 (3 × 72) skewed back by `(p + rotate(p)) / 3`; grid
//! centroids use a 2304 lattice.

use crate::os::hex;
use crate::placemaker::quads::grid_generation::grid_constants::{HEX_TO_PLANE, PLANE_TO_HEX};
use crate::unity::mathematics::{f2, f3, i3, Float2, Float3, Int2, Int3, Int3x3};
use crate::unity::mathf;

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

/// `GridBit.State`: the steps `GridGenerator.IterateBit` takes a bit through.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridBitState {
    #[default]
    Verts = 0,
    Relax = 1,
    Clusters = 2,
    Compose = 3,
    Mesh = 4,
    Done = 5,
}

/// `Placemaker.GridBit` (a `MonoBehaviour` under `GridGenerator.bitContainer`): one meshed piece of the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct GridBit {
    pub hex_pos: Int2,
    pub motivations: u8,
    pub relaxation_steps: u8,
    pub cluster_count: u8,
    pub patch_done: bool,
    pub state: GridBitState,
    pub verts: Vec<Float2>,
}

impl Default for GridBit {
    /// `GridBit..ctor` 0xE285F0: `verts` with capacity 256.
    fn default() -> Self {
        GridBit {
            hex_pos: Int2::default(),
            motivations: 0,
            relaxation_steps: 0,
            cluster_count: 0,
            patch_done: false,
            state: GridBitState::Verts,
            verts: Vec::with_capacity(256),
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
