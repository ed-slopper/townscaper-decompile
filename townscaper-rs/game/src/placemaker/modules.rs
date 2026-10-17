//! The modules (row E5): the building pieces the wave function collapse places in the qubes. `Placemaker`'s
//! `ByteQube`, `SideProfile`, `ModuleMath`, `Orientation` and module classes, and `Placemaker.Modules`
//! (`OrientedModule`, `OutlineUv`, `CornerLookup`, `ModuleLibrary`). Row E5b: the records; row E5c:
//! `Orientation` and `ModuleMath`; row E5d: the module data and `ModuleLibrary`. Checked against the running
//! game by `tests/placemaker_modules.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::os::hex;
use crate::os::utils::{self, ByteFloat, ByteFloat2, ByteFloat3, SbyteFloat3};
use crate::placemaker::graphs::MaterialType;
use crate::placemaker::grid_generator;
use crate::unity::mathematics::{self as math, f4, Float2, Float4, Float4x4, Int3, Int3x2, Int4, Int4x4};
use crate::unity::quaternion::{Matrix4x4, Quaternion};
use crate::unity::vector::{v3, v4, Vector2, Vector4};

/// `Placemaker.ByteQube`: one byte per corner of a cube, the lower four (round the square) then the upper four.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ByteQube {
    pub v: [u8; 8],
}

const fn bq(v: [u8; 8]) -> ByteQube {
    ByteQube { v }
}

impl ByteQube {
    /// `ByteQube..cctor` 0x1FD5D0: the corner permutations of the cube's turns.
    pub const IDENTITY: ByteQube = bq([0, 1, 2, 3, 4, 5, 6, 7]);
    pub const ROTATE_FORWARD: ByteQube = bq([3, 0, 1, 2, 7, 4, 5, 6]);
    pub const ROTATE_BACKWARD: ByteQube = bq([1, 2, 3, 0, 5, 6, 7, 4]);
    pub const MIRROR: ByteQube = bq([3, 2, 1, 0, 7, 6, 5, 4]);
    pub const TIP_FORWARD: ByteQube = bq([2, 1, 5, 6, 3, 0, 4, 7]);
    pub const TIP_BACKWARD: ByteQube = bq([7, 4, 0, 3, 6, 5, 1, 2]);
    pub const UPSIDE_DOWN: ByteQube = bq([4, 5, 6, 7, 0, 1, 2, 3]);

    /// `ByteQube..ctor(byte)` 0x1FD6F0: every corner the same.
    pub const fn splat(value: u8) -> ByteQube {
        bq([value; 8])
    }

    /// `ByteQube..ctor(byte × 8)` 0x1FD720.
    pub const fn new(v0: u8, v1: u8, v2: u8, v3: u8, v4: u8, v5: u8, v6: u8, v7: u8) -> ByteQube {
        bq([v0, v1, v2, v3, v4, v5, v6, v7])
    }

    /// `ByteQube.get_Item` 0x1FD760 (throws outside 0..=7).
    pub fn get(&self, key: i32) -> u8 {
        *usize::try_from(key).ok().and_then(|k| self.v.get(k)).unwrap_or_else(|| panic!("ByteQube[{key}]: IndexOutOfRangeException"))
    }

    /// `ByteQube.set_Item` 0x1FD900 (throws outside 0..=7).
    pub fn set(&mut self, key: i32, value: u8) {
        *usize::try_from(key).ok().and_then(|k| self.v.get_mut(k)).unwrap_or_else(|| panic!("ByteQube[{key}]: IndexOutOfRangeException")) = value;
    }

    /// `ByteQube.get_cost` / `GetHashCode` 0x1FCB60 (one function): a polynomial of the corners (wrapping).
    pub fn cost(&self) -> i32 {
        let v = self.v.map(|b| b as u32);
        let a = (((v[4].wrapping_mul(0x7F).wrapping_add(v[3])).wrapping_mul(0x7F).wrapping_add(v[2])).wrapping_mul(0x7F).wrapping_add(v[1])).wrapping_mul(0x7F);
        v[0].wrapping_add(0x8000_0000)
            .wrapping_add(a)
            .wrapping_add(v[6].wrapping_mul((-0x127C_42FFi32) as u32))
            .wrapping_add(v[5].wrapping_mul((-0x4EC2_7D81i32) as u32))
            .wrapping_add(v[7].wrapping_mul((-0x2BA5_3C81i32) as u32)) as i32
    }

    /// `ByteQube.GetHashCode` 0x1FCB60: `cost`.
    pub fn get_hash_code(&self) -> i32 {
        self.cost()
    }

    /// `ByteQube.GetOriented` 0x1FD180: corner `k` of the result is corner `orientation[k]` of this one.
    pub fn get_oriented(&self, orientation: ByteQube) -> ByteQube {
        bq(orientation.v.map(|k| self.get(k as i32)))
    }

    /// `ByteQube.GetOrientedForward` 0x1FCF90 / `GetOrientedBackward` 0x1FCDA0: orientation index `o` (mod 48 for
    /// more than 48) as turns: 4 rotations, then a mirror, 3 tips, upside down, in nested order.
    fn get_oriented_by(&self, orientation: u8, rotate: ByteQube, tip: ByteQube) -> ByteQube {
        let mut o = orientation;
        if 0x30 < o {
            let n = (o - 0x31) / 0x30 + 1;
            for _ in 0..n {
                o = o.wrapping_add(0xD0);
            }
        }
        let mut cur = *self;
        for _ in 0..2 {
            for _ in 0..3 {
                for _ in 0..2 {
                    for _ in 0..4 {
                        if o == 0 {
                            return cur;
                        }
                        o -= 1;
                        cur = cur.get_oriented(rotate);
                    }
                    cur = cur.get_oriented(ByteQube::MIRROR);
                }
                cur = cur.get_oriented(tip);
            }
            cur = cur.get_oriented(ByteQube::UPSIDE_DOWN);
        }
        cur
    }

    /// `ByteQube.GetOrientedForward` 0x1FCF90.
    pub fn get_oriented_forward(&self, orientation: u8) -> ByteQube {
        self.get_oriented_by(orientation, ByteQube::ROTATE_FORWARD, ByteQube::TIP_FORWARD)
    }

    /// `ByteQube.GetOrientedBackward` 0x1FCDA0.
    pub fn get_oriented_backward(&self, orientation: u8) -> ByteQube {
        self.get_oriented_by(orientation, ByteQube::ROTATE_BACKWARD, ByteQube::TIP_BACKWARD)
    }

    /// `ByteQube.GetOptimalOrientation` 0x1FCBC0: of the 48 orientations (in `GetOrientedForward` order), the one
    /// with the lowest `cost` (the first on a tie) and its index.
    pub fn get_optimal_orientation(&self) -> (ByteQube, u8) {
        let (mut best, mut best_cost, mut best_o) = (*self, i32::MAX, 0u8);
        let mut cur = *self;
        let mut o = 0u8;
        for _ in 0..2 {
            for _ in 0..3 {
                for _ in 0..2 {
                    for _ in 0..4 {
                        let c = cur.cost();
                        if c < best_cost {
                            best_o = o;
                            best = cur;
                            best_cost = c;
                        }
                        o = o.wrapping_add(1);
                        cur = cur.get_oriented(ByteQube::ROTATE_FORWARD);
                    }
                    cur = cur.get_oriented(ByteQube::MIRROR);
                }
                cur = cur.get_oriented(ByteQube::TIP_FORWARD);
            }
            cur = cur.get_oriented(ByteQube::UPSIDE_DOWN);
        }
        (best, best_o)
    }

    /// `ByteQube.GetWrappedIndex` 0x1FD260: `(corner + 4) % 4 + height * 4` (C# remainder).
    pub fn get_wrapped_index(corner: i32, height: i32) -> i32 {
        corner.wrapping_add(4).wrapping_rem(4).wrapping_add(height.wrapping_mul(4))
    }

    /// `ByteQube.ToString` 0x1FD280.
    pub fn to_string_cs(&self) -> String {
        let v = self.v;
        format!("ByteQube ({} {} {} {} / {} {} {} {})", v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7])
    }
}

/// `Placemaker.SideProfile`: the profile of a module's sides: two values for each of the four sides round the
/// qube (`v00`/`v01` .. `v30`/`v31`), one for the bottom (`v4`) and the top (`v5`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SideProfile {
    pub v00: u16,
    pub v01: u16,
    pub v10: u16,
    pub v11: u16,
    pub v20: u16,
    pub v21: u16,
    pub v30: u16,
    pub v31: u16,
    pub v4: u16,
    pub v5: u16,
}

impl SideProfile {
    /// `SideProfile.Set` 0x310FC0: side `index`'s two values (one for the bottom and top; throws past 5).
    pub fn set(&mut self, index: i32, value0: u16, value1: u16) {
        match index {
            0 => (self.v00, self.v01) = (value0, value1),
            1 => (self.v10, self.v11) = (value0, value1),
            2 => (self.v20, self.v21) = (value0, value1),
            3 => (self.v30, self.v31) = (value0, value1),
            4 => self.v4 = value0,
            5 => self.v5 = value0,
            _ => panic!("SideProfile.Set({index}): IndexOutOfRangeException"),
        }
    }

    /// `SideProfile.Add` 0x310D70: every value added (wrapping).
    pub fn add(&mut self, s: SideProfile) {
        self.v00 = self.v00.wrapping_add(s.v00);
        self.v01 = self.v01.wrapping_add(s.v01);
        self.v10 = self.v10.wrapping_add(s.v10);
        self.v11 = self.v11.wrapping_add(s.v11);
        self.v20 = self.v20.wrapping_add(s.v20);
        self.v21 = self.v21.wrapping_add(s.v21);
        self.v30 = self.v30.wrapping_add(s.v30);
        self.v31 = self.v31.wrapping_add(s.v31);
        self.v4 = self.v4.wrapping_add(s.v4);
        self.v5 = self.v5.wrapping_add(s.v5);
    }

    /// `SideProfile.Get0` 0x310DC0 (throws past 5).
    pub fn get0(&self, index: i32) -> u16 {
        match index {
            0 => self.v00,
            1 => self.v10,
            2 => self.v20,
            3 => self.v30,
            4 => self.v4,
            5 => self.v5,
            _ => panic!("SideProfile.Get0({index}): IndexOutOfRangeException"),
        }
    }

    /// `SideProfile.Get1` 0x310E60 (throws past 5).
    pub fn get1(&self, index: i32) -> u16 {
        match index {
            0 => self.v01,
            1 => self.v11,
            2 => self.v21,
            3 => self.v31,
            4 => self.v4,
            5 => self.v5,
            _ => panic!("SideProfile.Get1({index}): IndexOutOfRangeException"),
        }
    }

    /// `SideProfile.IsEmtpy` 0x310F80: the first values of the sides, the bottom and the top all 0 (the second
    /// values are not looked at).
    pub fn is_emtpy(&self) -> bool {
        self.v00 == 0 && self.v10 == 0 && self.v20 == 0 && self.v30 == 0 && self.v4 == 0 && self.v5 == 0
    }

    /// `SideProfile.GetHashCode` 0x310F00: products xored, except `v5` and `v4`'s, which are added (C#
    /// precedence: `+` before `^`).
    pub fn get_hash_code(&self) -> i32 {
        let m = |v: u16, k: u32| (v as u32).wrapping_mul(k);
        (m(self.v11, 0x1EC1)
            ^ m(self.v5, 0x1EEF).wrapping_add(m(self.v4, 0x1EE3))
            ^ m(self.v31, 0x1EDD)
            ^ m(self.v30, 0x1ECB)
            ^ m(self.v21, 0x1EC7)
            ^ m(self.v20, 0x1EC5)
            ^ m(self.v10, 0x1EBB)
            ^ m(self.v01, 0x1EAD)
            ^ m(self.v00, 0x1EA1)) as i32
    }

    /// `SideProfile.ToString` 0x311090: the first values only.
    pub fn to_string_cs(&self) -> String {
        format!("Ushort6 ({} {} {} {} {} {})", self.v00, self.v10, self.v20, self.v30, self.v4, self.v5)
    }
}

/// `Placemaker.Modules.OrientedModule`: a module index and flags: the orientation (bits 0..=2), decor (bit 4),
/// empty (bit 5).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OrientedModule {
    pub module_index: u16,
    pub flags: u8,
    pub pad: u8,
}

impl OrientedModule {
    /// `OrientedModule.empty` (`..cctor` 0x6FC700): module 0 with the empty bit.
    pub const EMPTY: OrientedModule = OrientedModule { module_index: 0, flags: 0x20, pad: 0 };

    /// `OrientedModule.get_orientation` 0x6FC780.
    pub fn orientation(&self) -> i32 {
        (self.flags & 7) as i32
    }
    /// `OrientedModule.set_orientation` 0x6FC860: the low bits replaced (the value is not masked).
    pub fn set_orientation(&mut self, value: i32) {
        self.flags = self.flags & 0xF8 | value as u8;
    }
    /// `OrientedModule.get_isDecor` 0x6FC740.
    pub fn is_decor(&self) -> bool {
        utils::is_bit_set(self.flags as i32, 4)
    }
    /// `OrientedModule.set_isDecor` 0x6FC820.
    pub fn set_is_decor(&mut self, value: bool) {
        utils::set_bit(&mut self.flags, 4, value);
    }
    /// `OrientedModule.get_isEmpty` 0x6FC760.
    pub fn is_empty(&self) -> bool {
        utils::is_bit_set(self.flags as i32, 5)
    }
    /// `OrientedModule.set_isEmpty` 0x6FC840.
    pub fn set_is_empty(&mut self, value: bool) {
        utils::set_bit(&mut self.flags, 5, value);
    }
    /// `OrientedModule.op_Equality` 0x6FC790 / `op_Inequality` 0x6FC7B0 / `Equals` 0x6FC580: index and flags
    /// (not the padding byte).
    pub fn eq_cs(&self, other: &OrientedModule) -> bool {
        self.module_index == other.module_index && self.flags == other.flags
    }
    /// `OrientedModule.GetHashCode` 0x6FC630.
    pub fn get_hash_code(&self) -> i32 {
        (self.flags as u32).wrapping_mul(0xD01).wrapping_add((self.module_index as u32).wrapping_mul(0x107)) as i32
    }
    /// `OrientedModule.ToString` 0x6FC650.
    pub fn to_string_cs(&self) -> String {
        format!("({} {} {})", self.module_index, self.orientation(), self.flags)
    }
}

/// `Placemaker.Modules.OutlineUv`: a vertex's outline offset packed in a byte: x in bits 0..=1, y in 2..=3
/// (1 = -1, 2 = +1, else 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OutlineUv {
    pub b: u8,
}

impl OutlineUv {
    fn unpack(bits: u8, minus: u8, plus: u8) -> i32 {
        if bits == minus {
            -1
        } else {
            (bits == plus) as i32
        }
    }
    /// `OutlineUv.get_x` 0x6FC8F0.
    pub fn x(&self) -> i32 {
        OutlineUv::unpack(self.b & 3, 1, 2)
    }
    /// `OutlineUv.get_y` 0x6FC920.
    pub fn y(&self) -> i32 {
        OutlineUv::unpack(self.b & 0xC, 4, 8)
    }
    /// `OutlineUv.set_x` 0x6FC950 (throws outside -1..=1).
    pub fn set_x(&mut self, value: i32) {
        self.b = match value {
            -1 => self.b & 0xFD | 1,
            0 => self.b & 0xFC,
            1 => self.b & 0xFE | 2,
            _ => panic!("OutlineUv.set_x({value}): IndexOutOfRangeException"),
        };
    }
    /// `OutlineUv.set_y` 0x6FC9C0 (throws outside -1..=1).
    pub fn set_y(&mut self, value: i32) {
        self.b = match value {
            -1 => self.b & 0xF7 | 4,
            0 => self.b & 0xF3,
            1 => self.b & 0xFB | 8,
            _ => panic!("OutlineUv.set_y({value}): IndexOutOfRangeException"),
        };
    }
    /// `OutlineUv.get_uv` 0x6FC890: `(x, y)`.
    pub fn uv(&self) -> Vector2 {
        Vector2 { x: self.x() as f32, y: self.y() as f32 }
    }
    /// `OutlineUv.get_border` 0x6FC880: any offset bit set.
    pub fn border(&self) -> bool {
        self.b & 0xF != 0
    }
}

// ---- Orientation and ModuleMath (row E5c) ----

/// `Placemaker.Orientation` (0x5C bytes): one of the eight turns of a qube (four about y, each also mirrored in
/// z), as an integer matrix, the corner permutations both ways, and the side permutations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Orientation {
    pub int4x4: Int4x4,
    /// `cornerTransform0[i]`: the corner that lands on corner i.
    pub corner_transform0: ByteQube,
    /// `cornerTransform1[j]`: where corner j lands.
    pub corner_transform1: ByteQube,
    /// Sides 0..=3 round y: `[0..4]` like `cornerTransform0`, `[4..8]` like `cornerTransform1`.
    pub side_transform: ByteQube,
    pub inverted: bool,
}

impl Orientation {
    /// `Placemaker.Orientation.get_matrix` 0x6FC310: the columns as floats.
    pub fn matrix(&self) -> Matrix4x4 {
        let c = |v: Int4| v4(v.x as f32, v.y as f32, v.z as f32, v.w as f32);
        let m = &self.int4x4.c;
        Matrix4x4::from_columns(c(m[0]), c(m[1]), c(m[2]), c(m[3]))
    }

    /// `Placemaker.Orientation.GetSideTransform0` 0x6FBFA0: the bottom (4) and top (5) stay.
    pub fn get_side_transform0(&self, index: i32) -> u8 {
        match index {
            0..=3 => self.side_transform.v[index as usize],
            4 | 5 => index as u8,
            _ => panic!("IndexOutOfRangeException: GetSideTransform0({index})"),
        }
    }

    /// `Placemaker.Orientation.GetSideTransform1` 0x6FC030.
    pub fn get_side_transform1(&self, index: i32) -> u8 {
        match index {
            0..=3 => self.side_transform.v[index as usize + 4],
            4 | 5 => index as u8,
            _ => panic!("IndexOutOfRangeException: GetSideTransform1({index})"),
        }
    }

    /// `Placemaker.Orientation.MultiplyBounds` 0x6FC0C0.
    pub fn multiply_bounds(&self, bounds: Int3x2) -> Int3x2 {
        ModuleMath::multiply_bounds(&self.int4x4, bounds)
    }

    /// `Placemaker.Orientation.MultiplyPoint` 0x6FC190.
    pub fn multiply_point(&self, point: SbyteFloat3) -> SbyteFloat3 {
        ModuleMath::multiply_point_s(&self.int4x4, point)
    }

    /// `Placemaker.Orientation.MultiplyVector` 0x6FC250.
    pub fn multiply_vector(&self, vector: SbyteFloat3) -> SbyteFloat3 {
        ModuleMath::multiply_vector_s(&self.int4x4, vector)
    }
}

/// `Placemaker.ModuleMath`'s statics, as `ModuleMath..cctor` 0x331880 builds them. Get them with
/// [`ModuleMath::get`].
#[derive(Clone, Debug)]
pub struct ModuleMath {
    /// `corners`: the qube's corners at ±0.5 (±127 as sbytes), the lower four round the square, then the upper.
    pub corners: [SbyteFloat3; 8],
    pub qube_forward: ByteQube,
    pub qube_backward: ByteQube,
    pub qube_upside_down: ByteQube,
    pub orientations: Vec<Orientation>,
    /// `orientationMulLookup[i, j]`: the orientation of `mul(o_i, o_j)`.
    pub orientation_mul_lookup: [[u8; 8]; 8],
    pub orientation_inverse_lookup: [u8; 8],
    /// `sideMatrices`: from qube space into each side's frame (sides 0..=3 round y, then 4 and 5).
    pub side_matrices: [Int4x4; 6],
    /// `invSideMatrices`: allocated, never filled.
    pub inv_side_matrices: [Int4x4; 6],
    /// `cornerLinkLookup[a, b]`: the index of the corner pair (0..28 for a ≠ b, symmetric; 0 on the diagonal).
    pub corner_link_lookup: [[i32; 8]; 8],
    /// `baryVectors`: per sixth of the hexagon, the gradient of a barycentric weight in the plane.
    pub bary_vectors: [Float2; 6],
    pub ground_byte_qube: ByteQube,
    pub any_byte_qube: ByteQube,
}

static MODULE_MATH: OnceLock<ModuleMath> = OnceLock::new();

/// `(int)f` per component (`int4(Vector4)` in the cctor).
fn int4_of(v: Vector4) -> Int4 {
    Int4 { x: v.x as i32, y: v.y as i32, z: v.z as i32, w: v.w as i32 }
}

fn int4x4_of(m: &Matrix4x4) -> Int4x4 {
    Int4x4 { c: std::array::from_fn(|i| int4_of(m.column(i))) }
}

/// Each column dotted with `v` (`mul(transpose(m), v)`, as the cctor's inlined code reads its matrix; every
/// value here is a small integer, so the grouping can't matter).
fn mul_transposed(m: &Float4x4, v: Float4) -> Float4 {
    let d = |c: &Float4| c.x * v.x + c.y * v.y + c.z * v.z + c.w * v.w;
    f4(d(&m.c[0]), d(&m.c[1]), d(&m.c[2]), d(&m.c[3]))
}

impl ModuleMath {
    /// The statics, built once (`ModuleMath..cctor`).
    pub fn get() -> &'static ModuleMath {
        MODULE_MATH.get_or_init(ModuleMath::cctor)
    }

    /// `Placemaker.ModuleMath..cctor` 0x331880.
    pub fn cctor() -> ModuleMath {
        let h = 0.5f32;
        let corners = [
            SbyteFloat3::new(-h, -h, -h),
            SbyteFloat3::new(h, -h, -h),
            SbyteFloat3::new(h, -h, h),
            SbyteFloat3::new(-h, -h, h),
            SbyteFloat3::new(-h, h, -h),
            SbyteFloat3::new(h, h, -h),
            SbyteFloat3::new(h, h, h),
            SbyteFloat3::new(-h, h, h),
        ];

        // the sides: inverse(Rotate(Euler(..)) * Translate(..)), cut to ints
        let side = |e: (f32, f32, f32), t: f32| {
            let r = Matrix4x4::rotate(Quaternion::euler(e.0, e.1, e.2));
            let m = Matrix4x4::mul(&r, &Matrix4x4::translate(v3(0.0, 0.0, t)));
            int4x4_of(&m.inverse())
        };
        let mut side_matrices = [Int4x4::default(); 6];
        side_matrices[4] = side((-90.0, 0.0, 0.0), -127.0);
        side_matrices[5] = side((-90.0, 0.0, 0.0), 127.0);
        let mut angle = 0i32;
        for s in side_matrices.iter_mut().take(4) {
            *s = side((0.0, angle as f32, 0.0), -127.0);
            angle -= 90;
        }

        // the orientations: four turns about y, then the same mirrored in z
        let rot_y = Matrix4x4::from_columns(
            v4(0.0, 0.0, 1.0, 0.0),
            v4(0.0, 1.0, 0.0, 0.0),
            v4(-1.0, 0.0, 0.0, 0.0),
            v4(0.0, 0.0, 0.0, 1.0),
        );
        let mirror_z = Matrix4x4::from_columns(
            v4(1.0, 0.0, 0.0, 0.0),
            v4(0.0, 1.0, 0.0, 0.0),
            v4(0.0, 0.0, -1.0, 0.0),
            v4(0.0, 0.0, 0.0, 1.0),
        );
        let side_pos: Vec<Float4> =
            side_matrices[..4].iter().map(|s| math::inverse(&math::float4x4_from_int(s)).c[3]).collect();
        let mut orientations = Vec::with_capacity(8);
        let mut m = Matrix4x4::IDENTITY;
        for _mirror in 0..2 {
            for _turn in 0..4 {
                let mut o = Orientation { int4x4: int4x4_of(&m), ..Default::default() };
                for i in 0..8u8 {
                    let ci = corners[i as usize].to_int3();
                    let want = v3(ci.x as f32, ci.y as f32, ci.z as f32);
                    for j in 0..8u8 {
                        let cj = corners[j as usize].to_int3();
                        if want == m.multiply_vector(v3(cj.x as f32, cj.y as f32, cj.z as f32)) {
                            o.corner_transform0.v[i as usize] = j;
                            o.corner_transform1.v[j as usize] = i;
                            break;
                        }
                    }
                }
                let om = math::float4x4_from_int(&o.int4x4);
                for a in 0..4u8 {
                    for b in 0..4u8 {
                        if side_pos[a as usize] == mul_transposed(&om, side_pos[b as usize]) {
                            o.side_transform.v[a as usize] = b;
                            o.side_transform.v[b as usize + 4] = a;
                            break;
                        }
                    }
                }
                let c = &o.int4x4.c;
                let f = |v: i32| v as f32;
                let det = f(c[2].y) * (f(c[1].x) * f(c[0].z) - f(c[0].x) * f(c[1].z))
                    + f(c[2].x) * (f(c[0].y) * f(c[1].z) - f(c[1].y) * f(c[0].z))
                    + f(c[2].z) * (f(c[0].x) * f(c[1].y) - f(c[0].y) * f(c[1].x));
                o.inverted = det < 0.0;
                orientations.push(o);
                m = Matrix4x4::mul(&m, &rot_y);
            }
            m = Matrix4x4::mul(&m, &mirror_z);
        }

        let mut orientation_mul_lookup = [[0u8; 8]; 8];
        for i in 0..8 {
            for j in 0..8 {
                let p = math::mul_i4x4(&orientations[i].int4x4, &orientations[j].int4x4);
                if let Some(k) = orientations.iter().position(|o| o.int4x4 == p) {
                    orientation_mul_lookup[i][j] = k as u8;
                }
            }
        }
        let mut orientation_inverse_lookup = [0u8; 8];
        for i in 0..8 {
            for k in 0..8 {
                if orientations[i].corner_transform0 == orientations[k].corner_transform1 {
                    orientation_inverse_lookup[i] = k as u8;
                }
            }
        }
        let mut corner_link_lookup = [[0i32; 8]; 8];
        let mut n = 0;
        for a in 0..8 {
            for b in a + 1..8 {
                corner_link_lookup[a][b] = n;
                corner_link_lookup[b][a] = n;
                n += 1;
            }
        }

        let mut bary_vectors = [Float2::default(); 6];
        let mut c = grid_generator::get_cluster_offset();
        for v in bary_vectors.iter_mut() {
            let a = grid_generator::hex_to_plane_i2(c);
            c = hex::rotate_cw2i(c);
            let b = grid_generator::hex_to_plane_i2(c);
            let mid = math::f2((b.x + a.x) * 0.5, (b.y + a.y) * 0.5);
            let len = math::length2(mid);
            *v = math::f2(-((mid.x / len) / len), -((mid.y / len) / len));
        }

        ModuleMath {
            corners,
            qube_forward: ByteQube::new(1, 2, 3, 0, 5, 6, 7, 4),
            qube_backward: ByteQube::new(3, 0, 1, 2, 7, 4, 5, 6),
            qube_upside_down: ByteQube::new(4, 5, 6, 7, 0, 1, 2, 3),
            orientations,
            orientation_mul_lookup,
            orientation_inverse_lookup,
            side_matrices,
            inv_side_matrices: [Int4x4::default(); 6],
            corner_link_lookup,
            bary_vectors,
            ground_byte_qube: ByteQube::new(15, 15, 15, 15, 18, 18, 18, 18),
            any_byte_qube: ByteQube::splat(16),
        }
    }

    /// `Placemaker.ModuleMath.AreCornersLinked` 0x330E60: `(1 << (index & 31) & link) == 1`, so only the pair
    /// with index 0 (corners 0 and 1) can ever be linked (kept).
    pub fn are_corners_linked(corner_a: i32, corner_b: i32, link: i32) -> bool {
        let i = ModuleMath::get_corner_link_index(corner_a, corner_b);
        (1i32 << (i & 31)) & link == 1
    }

    /// `Placemaker.ModuleMath.GetCornerLinkIndex` 0x330F50 (throws outside 0..=7).
    pub fn get_corner_link_index(corner_a: i32, corner_b: i32) -> i32 {
        ModuleMath::get().corner_link_lookup[ix8(corner_a)][ix8(corner_b)]
    }

    /// `Placemaker.ModuleMath.GetBidirectionalHash` 0x330F00: the same for either order.
    pub fn get_bidirectional_hash(m0: OrientedModule, m1: OrientedModule) -> i32 {
        let (h1, h2) = (m0.get_hash_code(), m1.get_hash_code());
        if h2 <= h1 {
            h2.wrapping_mul(0x184D).wrapping_add(h1.wrapping_mul(0x1849))
        } else {
            h2.wrapping_mul(0x1849).wrapping_add(h1.wrapping_mul(0x184D))
        }
    }

    /// `Placemaker.ModuleMath.GetDoubleHash` 0x330FE0.
    pub fn get_double_hash(m0: OrientedModule, m1: OrientedModule) -> i32 {
        let (h1, h2) = (m0.get_hash_code(), m1.get_hash_code());
        h2.wrapping_mul(0x1849).wrapping_add(h1.wrapping_mul(0x184D))
    }

    /// `Placemaker.ModuleMath.GetIndexedMaterials` 0x331010: the materials (values below 15) renumbered 0, 1, ...
    /// by first appearance.
    pub fn get_indexed_materials(mut materials: ByteQube) -> ByteQube {
        let mut done = 0u8;
        let mut next = 0u8;
        for i in 0..8 {
            if utils::is_bit_set_u8(done, i) {
                continue;
            }
            let value = materials.get(i);
            if value >= 15 {
                continue;
            }
            materials.set(i, next);
            utils::set_bit(&mut done, i, true);
            for j in i + 1..8 {
                if materials.get(j) == value && !utils::is_bit_set_u8(done, j) {
                    materials.set(j, next);
                    utils::set_bit(&mut done, j, true);
                }
            }
            next = next.wrapping_add(1);
        }
        materials
    }

    /// `Placemaker.ModuleMath.GetMaterialCount` 0x3310E0: distinct values below 15, kept in a byte of bits, so
    /// values 8..=14 count every time they appear (kept).
    pub fn get_material_count(materials: ByteQube) -> i32 {
        let mut seen = 0u8;
        let mut n = 0u8;
        for i in 0..8 {
            let v = materials.get(i);
            if v < 15 && !utils::is_bit_set_u8(seen, v as i32) {
                n = n.wrapping_add(1);
                utils::set_bit(&mut seen, v as i32, true);
            }
        }
        n as i32
    }

    /// `Placemaker.ModuleMath.InvertOrientation` 0x331140.
    pub fn invert_orientation(orientation: i32) -> u8 {
        ModuleMath::get().orientation_inverse_lookup[ix8(orientation)]
    }

    /// `Placemaker.ModuleMath.MarkHiddenCornersAmbigous` 0x3311B0: a corner that isn't 18 (air) in the qube nor
    /// in its forward, backward and upside-down turns becomes 16.
    pub fn mark_hidden_corners_ambigous(mut q: ByteQube) -> ByteQube {
        let mm = ModuleMath::get();
        let f = q.get_oriented(mm.qube_forward);
        let b = q.get_oriented(mm.qube_backward);
        let u = q.get_oriented(mm.qube_upside_down);
        for i in 0..8 {
            if f.get(i) != 18 && b.get(i) != 18 && u.get(i) != 18 && q.get(i) != 18 {
                q.set(i, 16);
            }
        }
        q
    }

    /// `Placemaker.ModuleMath.MulOrientations(int, int)` 0x3312D0 / `(byte, byte)` 0x331360.
    pub fn mul_orientations(o0: i32, o1: i32) -> u8 {
        ModuleMath::get().orientation_mul_lookup[ix8(o0)][ix8(o1)]
    }

    /// `Placemaker.ModuleMath.Mul` 0x3313F0: the child's side transform is kept.
    pub fn mul(parent: &Orientation, child: &Orientation) -> Orientation {
        Orientation {
            int4x4: math::mul_i4x4(&parent.int4x4, &child.int4x4),
            corner_transform0: parent.corner_transform0.get_oriented(child.corner_transform0),
            corner_transform1: parent.corner_transform1.get_oriented(child.corner_transform1),
            side_transform: child.side_transform,
            inverted: parent.inverted != child.inverted,
        }
    }

    /// `Placemaker.ModuleMath.MultiplyBounds` 0x3314F0: both corners turned, then min and max.
    pub fn multiply_bounds(m: &Int4x4, bounds: Int3x2) -> Int3x2 {
        let a = ModuleMath::multiply_point(m, bounds.c0);
        let b = ModuleMath::multiply_point(m, bounds.c1);
        Int3x2 { c0: math::i3(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)), c1: math::max3(a, b) }
    }

    /// `Placemaker.ModuleMath.MultiplyPoint(int4x4, int3)` 0x331640.
    pub fn multiply_point(m: &Int4x4, p: Int3) -> Int3 {
        let r = math::mul_i4x4_v(m, Int4 { x: p.x, y: p.y, z: p.z, w: 1 });
        math::i3(r.x, r.y, r.z)
    }

    /// `Placemaker.ModuleMath.MultiplyVector(int4x4, int3)` 0x3317F0.
    pub fn multiply_vector(m: &Int4x4, p: Int3) -> Int3 {
        let r = math::mul_i4x4_v(m, Int4 { x: p.x, y: p.y, z: p.z, w: 0 });
        math::i3(r.x, r.y, r.z)
    }

    /// `Placemaker.ModuleMath.MultiplyPoint(int4x4, SbyteFloat3)` 0x3316D0: cut to the low bytes.
    pub fn multiply_point_s(m: &Int4x4, p: SbyteFloat3) -> SbyteFloat3 {
        let r = ModuleMath::multiply_point(m, p.to_int3());
        SbyteFloat3::from_ints(r.x, r.y, r.z)
    }

    /// `Placemaker.ModuleMath.MultiplyVector(int4x4, SbyteFloat3)` 0x331760.
    pub fn multiply_vector_s(m: &Int4x4, p: SbyteFloat3) -> SbyteFloat3 {
        let r = ModuleMath::multiply_vector(m, p.to_int3());
        SbyteFloat3::from_ints(r.x, r.y, r.z)
    }
}

/// An index into the 8-wide tables (`IndexOutOfRangeException` otherwise).
fn ix8(i: i32) -> usize {
    usize::try_from(i).ok().filter(|&i| i < 8).unwrap_or_else(|| panic!("IndexOutOfRangeException: {i}"))
}

// ---- Module data and the library (row E5d) ----

/// STANDIN: a reference to a Unity object the module data points at (a `Material`, a `Prop` component), as the
/// asset stores it: `file_id` 0 = the library's own file, n > 0 = that file's n-th external. The game holds
/// the object itself; the renderer (Bevy) resolves these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AssetRef {
    pub file_id: i32,
    pub path_id: i64,
}

/// `Placemaker.PropPlacement`: a prop on a module (its methods are row E10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PropPlacement {
    pub pos: SbyteFloat3,
    pub scale: ByteFloat3,
    pub rotation_byte: ByteFloat3,
    pub prop: AssetRef,
    pub corner: i8,
    pub prio_add: u8,
}

/// `Placemaker.BaseModule` (`..ctor` 0x7B5A90: empty lists).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BaseModule {
    pub cost: i16,
    /// Into `ModuleLibrary.sideLookups`.
    pub profile_indexes: Vec<u16>,
    /// Into `ModuleLibrary.moduleMeshes`, each with its orientation.
    pub oriented_module_meshes: Vec<OrientedModule>,
    pub prop_placements: Vec<PropPlacement>,
}

/// `Placemaker.Module` (`..ctor` 0x334420): a building piece the wave function collapse places.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Module {
    pub base: BaseModule,
    /// Into `ModuleLibrary.decorModules`.
    pub decor_modules: Vec<OrientedModule>,
    /// Bits by `ModuleMath.GetCornerLinkIndex`.
    pub corner_links: i32,
}

/// `Placemaker.DecorModule` (`..ctor` 0x44C1F0): nothing beyond `BaseModule`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DecorModule {
    pub base: BaseModule,
}

/// `Placemaker.ModuleMesh` (`..ctor` 0x3342C0): one corner's piece of geometry in packed form.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleMesh {
    pub material: AssetRef,
    pub verts: Vec<SbyteFloat3>,
    pub normals: Vec<SbyteFloat3>,
    pub tangents: Vec<SbyteFloat3>,
    pub concavity: Vec<ByteFloat>,
    pub uvs: Vec<ByteFloat2>,
    pub outline_uvs: Vec<OutlineUv>,
    pub tris: Vec<u8>,
    pub material_type: MaterialType,
    pub corner: u8,
}

/// `Placemaker.Modules.CornerLookup` (`..ctor` 0x443F10): the modules that fit one set of corner values.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CornerLookup {
    pub values: ByteQube,
    pub oriented_modules: Vec<OrientedModule>,
}

/// `Placemaker.Modules.ModuleLibrary` (a `ScriptableObject`; `..ctor` 0x330D00 makes every list but
/// `waterSides` empty).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleLibrary {
    pub water_sides: Vec<u32>,
    pub modules: Vec<Module>,
    pub decor_modules: Vec<DecorModule>,
    pub corner_lookups: Vec<CornerLookup>,
    pub side_lookups: Vec<SideProfile>,
    pub module_meshes: Vec<ModuleMesh>,
    /// Sorted bidirectional hashes of the decor pairs that overlap.
    pub decor_overlaps: Vec<i32>,
    pub ground_uv0: ByteFloat2,
    pub ground_uv1: ByteFloat2,
    /// `cornerDict`: corner values → index into `corner_lookups` (built by `setup`).
    pub corner_dict: HashMap<ByteQube, i32>,
}

impl ModuleLibrary {
    /// `Placemaker.Modules.ModuleLibrary.Setup` / `OnEnable` 0x330C00 (one function): `cornerDict` from
    /// `cornerLookups`, each lookup's values to its index (`Dictionary.Add`: a repeated key throws).
    pub fn setup(&mut self) {
        let mut d = HashMap::with_capacity(self.corner_lookups.len());
        for (i, c) in self.corner_lookups.iter().enumerate() {
            if d.insert(c.values, i as i32).is_some() {
                panic!("ArgumentException: An item with the same key has already been added. Key: {}", c.values.to_string_cs());
            }
        }
        self.corner_dict = d;
    }

    /// `Placemaker.Modules.ModuleLibrary.GetDecorOverlap` 0x330B40: `ModuleMath.GetBidirectionalHash` inlined,
    /// looked up by `List.BinarySearch` in `decorOverlaps`.
    pub fn get_decor_overlap(&self, decor0: OrientedModule, decor1: OrientedModule) -> bool {
        self.decor_overlaps.binary_search(&ModuleMath::get_bidirectional_hash(decor0, decor1)).is_ok()
    }
}
