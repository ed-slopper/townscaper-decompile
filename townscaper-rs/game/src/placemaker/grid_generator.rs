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
use crate::placemaker::quads::grid_generation::{CustomGridLibrary, CustomGridPatch, HexPatch, HexPatchState};
use crate::unity::mathematics::{f2, f3, hash3, i3, mul_i3x3, Float2, Float3, Int2, Int3, Int3x3, Random};
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

/// `Placemaker.GridGenerator`'s state, as far as the rows ported so far use it (E3c: the patches). Patches are
/// kept in `patches` and found by position through `patch_dict` (an index into `patches`: the game's dictionary
/// holds the same objects as its list, and patches are never removed from the list, only reused).
#[derive(Clone, Debug, Default)]
pub struct GridGenerator {
    pub custom_grid_library: CustomGridLibrary,
    pub patches: Vec<HexPatch>,
    pub patch_dict: HashMap<Int2, usize>,
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
