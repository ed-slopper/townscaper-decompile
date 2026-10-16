//! `Os.Utils`: maths and helpers. So far `Bitwise` and the packed floats `ByteFloat`, `ByteFloat2`,
//! `ByteFloat3`, `SbyteFloat3` (row E1a); `ExtraMath` (E1b), `Intersections` (E1c) and the Unity glue (E1d)
//! follow. Checked against the running game by `tests/os_utils.rs` (`TS_HARNESS=1`).

use crate::unity::mathematics::{i3, Int3, Int3x3};
use crate::unity::mathf;
use crate::unity::vector::{v2, v3, Vector2, Vector3};

// ---- Bitwise ----

/// `Os.Utils.Bitwise.GetByte` 0x7C1B90: bit i set when argument i is true.
pub fn get_byte(bits: [bool; 8]) -> u8 {
    bits.iter().enumerate().fold(0u8, |b, (i, &on)| if on { b | (1 << i) } else { b })
}

/// `Os.Utils.Bitwise.IsBitSet(int, int)` 0x7C1C10: the shift count is taken mod 32 (x86 `shl`).
pub fn is_bit_set(b: i32, index: i32) -> bool {
    b & (1i32 << (index & 31)) != 0
}

/// `Os.Utils.Bitwise.IsBitSet(byte, int)` 0x7C1C30.
pub fn is_bit_set_u8(b: u8, index: i32) -> bool {
    (b as u32) & (1u32 << (index & 31)) != 0
}

/// `Os.Utils.Bitwise.SetBit` 0x7C1C50: the mask is `(byte)(1 << (index & 31))`, so bits 8..31 change nothing.
pub fn set_bit(b: &mut u8, index: i32, value: bool) {
    let mask = (1u32 << (index & 31)) as u8;
    if value {
        *b |= mask;
    } else {
        *b &= !mask;
    }
}

// ---- 0..1 in a byte: ByteFloat, ByteFloat2, ByteFloat3 ----

/// `Os.Utils.ByteFloat.ByteToFloat` 0x1FC9D0 (also every `ByteFloat*` getter): `b * (1 / 256)`.
pub fn byte_to_float(b: u8) -> f32 {
    b as f32 * 0.003_906_25
}

/// `Os.Utils.ByteFloat.FloatToByte` 0x1FCA60 (also every `ByteFloat*` setter): `(byte)Min(RoundToInt(f * 256),
/// 255)`. Only the top is clamped: negative values wrap through the byte cast.
pub fn float_to_byte(f: f32) -> u8 {
    mathf::min_i32(mathf::round_to_int(f * 256.0), 255) as u8
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ByteFloat {
    pub value: u8,
}

impl ByteFloat {
    /// `Os.Utils.ByteFloat..ctor(float)` 0x1FC4E0 / `op_Explicit(float)` 0x1FCA60.
    pub fn new(f: f32) -> ByteFloat {
        ByteFloat { value: float_to_byte(f) }
    }
    /// `Os.Utils.ByteFloat.get_f` 0x1FC320.
    pub fn f(self) -> f32 {
        byte_to_float(self.value)
    }
    /// `Os.Utils.ByteFloat.GetHashCode` 0x1FCA90: the byte.
    pub fn hash_code(self) -> i32 {
        self.value as i32
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ByteFloat2 {
    pub x: u8,
    pub y: u8,
}

impl ByteFloat2 {
    /// `Os.Utils.ByteFloat2..ctor(float, float)` 0x1FC2B0 / `op_Explicit(Vector2)` 0x1FC3B0.
    pub fn new(fx: f32, fy: f32) -> ByteFloat2 {
        ByteFloat2 { x: float_to_byte(fx), y: float_to_byte(fy) }
    }
    /// `Os.Utils.ByteFloat2.get_fx` 0x1FC320.
    pub fn fx(self) -> f32 {
        byte_to_float(self.x)
    }
    /// `Os.Utils.ByteFloat2.get_fy` 0x1FC350.
    pub fn fy(self) -> f32 {
        byte_to_float(self.y)
    }
    /// `Os.Utils.ByteFloat2.op_Explicit` 0x1FC460 (to `Vector2`).
    pub fn to_vector2(self) -> Vector2 {
        v2(self.fx(), self.fy())
    }
    /// `Os.Utils.ByteFloat2.GetHashCode` 0x1FC290: `x * 0x101 + 0x11 + y * 0x209`.
    pub fn hash_code(self) -> i32 {
        (self.x as i32 * 0x101 + 0x11) + self.y as i32 * 0x209
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ByteFloat3 {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl ByteFloat3 {
    /// `Os.Utils.ByteFloat3..ctor(float, float, float)` 0x1FC640 / `op_Explicit(Vector3)` 0x1FC800.
    pub fn new(fx: f32, fy: f32, fz: f32) -> ByteFloat3 {
        ByteFloat3 { x: float_to_byte(fx), y: float_to_byte(fy), z: float_to_byte(fz) }
    }
    /// `Os.Utils.ByteFloat3.get_fx` 0x1FC320.
    pub fn fx(self) -> f32 {
        byte_to_float(self.x)
    }
    /// `Os.Utils.ByteFloat3.get_fy` 0x1FC350.
    pub fn fy(self) -> f32 {
        byte_to_float(self.y)
    }
    /// `Os.Utils.ByteFloat3.get_fz` 0x1FC730.
    pub fn fz(self) -> f32 {
        byte_to_float(self.z)
    }
    /// `Os.Utils.ByteFloat3.op_Explicit` 0x1FC8A0 (to `Vector3`).
    pub fn to_vector3(self) -> Vector3 {
        v3(self.fx(), self.fy(), self.fz())
    }
    /// `Os.Utils.ByteFloat3.get_normal` 0x1FC760: `(f + f) - 1` per component.
    pub fn normal(self) -> Vector3 {
        let n = |b: u8| {
            let f = byte_to_float(b);
            (f + f) - 1.0
        };
        v3(n(self.x), n(self.y), n(self.z))
    }
    /// `Os.Utils.ByteFloat3.get_Item` 0x1FC6D0 (0..=2; the original throws `IndexOutOfRangeException`).
    pub fn get(self, key: i32) -> Option<u8> {
        [self.x, self.y, self.z].get(usize::try_from(key).ok()?).copied()
    }
    /// `Os.Utils.ByteFloat3.set_Item` 0x1FC920.
    pub fn set(&mut self, key: i32, value: u8) {
        match key {
            0 => self.x = value,
            1 => self.y = value,
            2 => self.z = value,
            _ => panic!("IndexOutOfRangeException: ByteFloat3[{key}]"),
        }
    }
    /// `Os.Utils.ByteFloat3.GetHashCode` 0x1FC610: `(z * 255 + y) * 255 + x`.
    pub fn hash_code(self) -> i32 {
        (self.z as i32 * 255 + self.y as i32) * 255 + self.x as i32
    }
}

// ---- -0.5..0.5 in signed bytes: SbyteFloat3 (scale 254) ----

/// `(sbyte)RoundToInt(f * 254)` (the int's low byte).
fn float_to_sbyte(f: f32) -> i8 {
    mathf::round_to_int(f * 254.0) as i8
}

/// `(float)s / 254`.
fn sbyte_to_float(s: i8) -> f32 {
    s as f32 / 254.0
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SbyteFloat3 {
    pub x: i8,
    pub y: i8,
    pub z: i8,
}

impl SbyteFloat3 {
    /// `Os.Utils.SbyteFloat3..ctor(float, float, float)` 0x96EAF0 / `op_Explicit(Vector3)` 0x96ED60.
    pub fn new(fx: f32, fy: f32, fz: f32) -> SbyteFloat3 {
        SbyteFloat3 { x: float_to_sbyte(fx), y: float_to_sbyte(fy), z: float_to_sbyte(fz) }
    }
    /// `Os.Utils.SbyteFloat3..ctor(int, int, int)` 0x96EB60 / `op_Explicit(int3)` 0x96ED40: low bytes.
    pub fn from_ints(x: i32, y: i32, z: i32) -> SbyteFloat3 {
        SbyteFloat3 { x: x as i8, y: y as i8, z: z as i8 }
    }
    /// `Os.Utils.SbyteFloat3.get_fx` 0x96EBE0.
    pub fn fx(self) -> f32 {
        sbyte_to_float(self.x)
    }
    /// `Os.Utils.SbyteFloat3.get_fy` 0x96EC10.
    pub fn fy(self) -> f32 {
        sbyte_to_float(self.y)
    }
    /// `Os.Utils.SbyteFloat3.get_fz` 0x96EC40.
    pub fn fz(self) -> f32 {
        sbyte_to_float(self.z)
    }
    /// `Os.Utils.SbyteFloat3.op_Explicit` 0x96ECD0 (to `Vector3`).
    pub fn to_vector3(self) -> Vector3 {
        v3(self.fx(), self.fy(), self.fz())
    }
    /// `Os.Utils.SbyteFloat3.op_Explicit` 0x96ED20 (to `int3`).
    pub fn to_int3(self) -> Int3 {
        i3(self.x as i32, self.y as i32, self.z as i32)
    }
    /// `Os.Utils.SbyteFloat3.get_normal` 0x96EC70: `f + f` per component.
    pub fn normal(self) -> Vector3 {
        let n = |s: i8| {
            let f = sbyte_to_float(s);
            f + f
        };
        v3(n(self.x), n(self.y), n(self.z))
    }
    /// `Os.Utils.SbyteFloat3.get_Item` 0x96EB80.
    pub fn get(self, key: i32) -> Option<i8> {
        [self.x, self.y, self.z].get(usize::try_from(key).ok()?).copied()
    }
    /// `Os.Utils.SbyteFloat3.set_Item` 0x96EE70.
    pub fn set(&mut self, key: i32, value: i8) {
        match key {
            0 => self.x = value,
            1 => self.y = value,
            2 => self.z = value,
            _ => panic!("IndexOutOfRangeException: SbyteFloat3[{key}]"),
        }
    }
    /// `Os.Utils.SbyteFloat3.op_Multiply(int3x3, SbyteFloat3)` 0x96EDE0: component i is `dot(a.c_i, b)` (each
    /// column dotted with the vector: the transpose of `math.mul`; proven by the harness), cut to its low byte.
    pub fn mul(a: &Int3x3, b: SbyteFloat3) -> SbyteFloat3 {
        let v = b.to_int3();
        let dot = |c: &Int3| c.x.wrapping_mul(v.x).wrapping_add(c.y.wrapping_mul(v.y)).wrapping_add(c.z.wrapping_mul(v.z));
        SbyteFloat3::from_ints(dot(&a.c[0]), dot(&a.c[1]), dot(&a.c[2]))
    }
    /// `Os.Utils.SbyteFloat3.ToString` 0x96EA60: `"SbyteFloat3 ({x}, {y}, {z})"`.
    pub fn to_string_cs(self) -> String {
        format!("SbyteFloat3 ({}, {}, {})", self.x, self.y, self.z)
    }

    /// `Os.Utils.SbyteFloat3.GetHashCode` 0x96EA30: `z * 0xC9D + y * 0xC95 + x * 0xC91` (sign-extended).
    pub fn hash_code(self) -> i32 {
        self.z as i32 * 0xC9D + self.y as i32 * 0xC95 + self.x as i32 * 0xC91
    }
}
