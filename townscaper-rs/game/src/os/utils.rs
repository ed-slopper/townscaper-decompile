//! `Os.Utils`: maths and helpers: `Bitwise` and the packed floats `ByteFloat`, `ByteFloat2`, `ByteFloat3`,
//! `SbyteFloat3` (row E1a, checked by `tests/os_utils.rs`), and the Unity glue the game logic uses (E1d, checked
//! by `tests/os_unity_glue.rs`): `UnityFunctions` (`InView`, the hierarchy walk, `GetNewGameObject`) and
//! `Extensions`. `ExtraMath` and `Intersections` have modules of their own.

use crate::unity::mathematics::{i3, Int3, Int3x3};
use crate::unity::mathf;
use crate::unity::transform::{Node, TransformId, Transforms};
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

// ---- UnityFunctions ----

/// What [`HirearchyEnumerator`] reads of a transform tree: children by index, the parent, the sibling index and
/// `gameObject.activeSelf`.
pub trait Hierarchy {
    type Node: Copy + PartialEq;
    fn child_count(&self, t: Self::Node) -> i32;
    fn child(&self, t: Self::Node, i: i32) -> Self::Node;
    fn parent(&self, t: Self::Node) -> Option<Self::Node>;
    fn sibling_index(&self, t: Self::Node) -> i32;
    fn active_self(&self, t: Self::Node) -> bool;
    /// `Object.name`, for [`HirearchyEnumerator::to_string`].
    fn name(&self, t: Self::Node) -> String;
}

/// `Os.Utils.UnityFunctions.HirearchyEnumerator` (`UnityFunctions.EnumerateHirearchy(t, includeInactive)`
/// 0x3CCE10): `t`'s descendants depth first, children in order, not `t` itself. Without `includeInactive` an
/// inactive object (by `activeSelf`) is skipped with everything under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HirearchyEnumerator<N> {
    pub sibling_index: i32,
    pub sibling_count: i32,
    pub depth: i32,
    pub include_inactive: bool,
    pub transform: Option<N>,
    pub parent: Option<N>,
}

impl<N: Copy + PartialEq> HirearchyEnumerator<N> {
    /// `HirearchyEnumerator..ctor(t, includeInactive)` 0x4FB800.
    pub fn new(t: N, include_inactive: bool) -> HirearchyEnumerator<N> {
        HirearchyEnumerator { sibling_index: 0, sibling_count: 0, depth: 0, include_inactive, transform: Some(t), parent: None }
    }

    /// `get_Current` 0x1E7950.
    pub fn current(&self) -> Option<N> {
        self.transform
    }

    /// `MoveNext` 0x4FB2C0: down to the current object's first child if it has any, else on to the next sibling,
    /// climbing while a level is done; false back at `t`. (A `depth` of -2 would yield `t` itself first; nothing
    /// sets it.)
    pub fn move_next<H: Hierarchy<Node = N>>(&mut self, h: &H) -> bool {
        let ok = |s: &Self, h: &H| s.include_inactive || h.active_self(s.transform.expect("NullReferenceException"));
        if self.depth == -2 {
            self.depth = 0;
            if ok(self, h) {
                return true;
            }
        }
        let t = self.transform.expect("NullReferenceException");
        let n = h.child_count(t);
        if 0 < n {
            self.depth += 1;
            self.parent = Some(t);
            self.transform = Some(h.child(t, 0));
            self.sibling_index = 0;
            self.sibling_count = n;
            if ok(self, h) {
                return true;
            }
        }
        loop {
            self.sibling_index += 1;
            while self.sibling_index >= self.sibling_count {
                self.depth -= 1;
                if self.depth < 1 {
                    return false;
                }
                let p = self.parent.expect("NullReferenceException");
                self.transform = Some(p);
                self.sibling_index = h.sibling_index(p);
                let pp = h.parent(p).expect("NullReferenceException");
                self.parent = Some(pp);
                self.sibling_count = h.child_count(pp);
                self.sibling_index += 1;
            }
            let p = self.parent.expect("NullReferenceException");
            self.transform = Some(h.child(p, self.sibling_index));
            if ok(self, h) {
                return true;
            }
        }
    }

    /// `Reset` 0x4FB440: back to the top. (It climbs by setting `transform` to `parent.transform` `depth` times,
    /// which lands on the parent, not on `t`, unless the walk was still at `t`.)
    pub fn reset<H: Hierarchy<Node = N>>(&mut self, h: &H) {
        for _ in 0..self.depth.max(0) {
            self.transform = Some(self.parent.expect("NullReferenceException"));
        }
        self.parent = None;
        self.depth = 0;
        let t = self.transform.expect("NullReferenceException");
        self.sibling_count = h.child_count(t);
        self.sibling_index = 0;
    }

    /// `Dispose` 0x42D9E0 (a body shared with an unrelated method): forgets `transform` and `parent`.
    pub fn dispose(&mut self) {
        self.transform = None;
        self.parent = None;
    }

    /// `ToString` 0x4FB540: `"{depth}, {siblingIndex}/{siblingCount}, {parent} > {transform}"` with the objects'
    /// names (or `null`).
    pub fn to_string<H: Hierarchy<Node = N>>(&self, h: &H) -> String {
        let name = |o: Option<N>| o.map_or_else(|| "null".to_string(), |o| h.name(o));
        format!("{}, {}/{}, {} > {}", self.depth, self.sibling_index, self.sibling_count, name(self.parent), name(self.transform))
    }
}

/// `Os.Utils.UnityFunctions.GetNewGameObject(name, parent)` 0x3CCE80: a new object put under `parent` keeping
/// its (identity) local pose, `SetParent(parent, false)`. (The name is the host's.)
pub fn get_new_game_object(t: &mut Transforms, parent: Option<TransformId>) -> TransformId {
    let id = t.add(Node::default());
    t.set_parent(id, parent, false);
    id
}

/// `Os.Utils.UnityFunctions.InView` 0x3CCEF0 after `camera.WorldToViewportPoint(worldPos)` (`viewport`, the
/// host's): in front of the camera and inside the viewport widened by `margin` on every side.
pub fn in_view(viewport: Vector3, margin: f32) -> bool {
    if !(0.0 < viewport.z) {
        return false;
    }
    let neg = -margin;
    neg < viewport.x && neg < viewport.y && viewport.x < margin + 1.0 && viewport.y < margin + 1.0
}

// ---- Extensions ----

/// `Os.Utils.Extensions.KeepGoing(stopwatch, milliseconds)` 0x6CC2E0: whether a time-budgeted loop may go on,
/// `stopwatch.ElapsedMilliseconds < milliseconds`.
pub fn keep_going(elapsed_milliseconds: i64, milliseconds: i32) -> bool {
    elapsed_milliseconds < milliseconds as i64
}

/// `Os.Utils.Extensions.SetSingleLayer(camera, layerName, enabled)` 0x6CC320: the camera's culling mask with the
/// layer's bits (`LayerMask.GetMask(layerName)`, `layer_mask`, 0 for an unknown name) set or cleared.
pub fn set_single_layer(culling_mask: i32, layer_mask: i32, enabled: bool) -> i32 {
    culling_mask & !layer_mask | if enabled { layer_mask } else { 0 }
}
