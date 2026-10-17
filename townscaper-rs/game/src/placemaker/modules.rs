//! The modules (row E5): the building pieces the wave function collapse places in the qubes. `Placemaker`'s
//! `ByteQube`, `SideProfile`, `ModuleMath`, `Orientation` and module classes, and `Placemaker.Modules`
//! (`OrientedModule`, `OutlineUv`, `CornerLookup`, `ModuleLibrary`). Row E5b: the records. Checked against the
//! running game by `tests/placemaker_modules.rs` (`TS_HARNESS=1`).

use crate::os::utils;
use crate::unity::vector::Vector2;

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
