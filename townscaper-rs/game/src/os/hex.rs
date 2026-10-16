//! `Os.Hex`: hex coordinates (`HexMath`) and the ring-by-ring walk over them (`HexOnionIterator`), row E2.
//! Cube coordinates: `x + y + z = 0`; the 2-component form keeps `x, y`. Checked against the running game by
//! `tests/os_hex.rs` (`TS_HARNESS=1`).

use crate::unity::mathematics::{f2, f3, i3, Float2, Float3, Int2, Int3};
use crate::unity::mathf;
fn b(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// `HexMath`'s static fields as `HexMath..cctor` 0x3DC860 leaves them (built with `ExtraMath.Rotate2D` and
/// matrix inverses; read from the running game and proven equal bit for bit by `tests/os_hex.rs`).
pub mod statics {
    use super::b;
    use crate::unity::mathematics::{f2, Float2};
    use crate::unity::quaternion::Matrix4x4;

    /// `HexMath.hexAxisLength`: 1/√3.
    pub fn hex_axis_length() -> f32 {
        b(0x3F13_CD3A)
    }
    /// `HexMath.hexAxisX`.
    pub fn hex_axis_x() -> Float2 {
        f2(b(0x3F13_CD3A), 0.0)
    }
    /// `HexMath.hexAxisY` (120°).
    pub fn hex_axis_y() -> Float2 {
        f2(b(0xBE93_CD3B), b(0x3EFF_FFFF))
    }
    /// `HexMath.hexAxisZ` (240°).
    pub fn hex_axis_z() -> Float2 {
        f2(b(0xBE93_CD38), b(0xBF00_0000))
    }
    /// `HexMath.skewAxisX`.
    pub fn skew_axis_x() -> Float2 {
        f2(b(0x3FDD_B3D8), 0.0)
    }
    /// `HexMath.skewAxisY`.
    pub fn skew_axis_y() -> Float2 {
        f2(b(0x3F5D_B3D7), b(0x3FC0_0001))
    }

    fn m(cols: [[f32; 4]; 4]) -> Matrix4x4 {
        let mut out = [0f32; 16];
        for (i, c) in cols.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(c);
        }
        Matrix4x4 { m: out }
    }

    /// `HexMath.hexToPlane`: columns hexAxisX, hexAxisY, hexAxisZ (in x, y), w.
    pub fn hex_to_plane() -> Matrix4x4 {
        let (x, y, z) = (hex_axis_x(), hex_axis_y(), hex_axis_z());
        m([[x.x, x.y, 0.0, 0.0], [y.x, y.y, 0.0, 0.0], [z.x, z.y, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }
    /// `HexMath.hexToWorld`: as hexToPlane with the plane's y on world z.
    pub fn hex_to_world() -> Matrix4x4 {
        let (x, y, z) = (hex_axis_x(), hex_axis_y(), hex_axis_z());
        m([[x.x, 0.0, x.y, 0.0], [y.x, 0.0, y.y, 0.0], [z.x, 0.0, z.y, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }
    /// `HexMath.planeToWorld`: swaps y and z.
    pub fn plane_to_world() -> Matrix4x4 {
        m([[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }
    /// `HexMath.skewToPlane`.
    pub fn skew_to_plane() -> Matrix4x4 {
        let (x, y) = (skew_axis_x(), skew_axis_y());
        m([[x.x, x.y, 0.0, 0.0], [y.x, y.y, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }
    /// `HexMath.planeToSkew`: `skewToPlane.inverse`.
    pub fn plane_to_skew() -> Matrix4x4 {
        skew_to_plane().inverse()
    }
    /// `HexMath.hexToSkew`: `planeToSkew * hexToPlane`.
    pub fn hex_to_skew() -> Matrix4x4 {
        Matrix4x4::mul(&plane_to_skew(), &hex_to_plane())
    }
    /// `HexMath.skewToHex`: `hexToSkew.inverse`, which is all zeros (hexToSkew is singular: the inverse
    /// fails and returns zeros, as in the game).
    pub fn skew_to_hex() -> Matrix4x4 {
        hex_to_skew().inverse()
    }
}

/// `Os.Hex.HexMath.GetHex2(float3)` 0x3DC080.
pub fn get_hex2_f(h: Float3) -> Float2 {
    f2(h.x, h.y)
}
/// `Os.Hex.HexMath.GetHex2(int3)` 0x3DC0B0.
pub fn get_hex2(h: Int3) -> Int2 {
    Int2 { x: h.x, y: h.y }
}
/// `Os.Hex.HexMath.GetHex3(int2)` 0x3DC0C0: `z = -(y + x)`.
pub fn get_hex3(h: Int2) -> Int3 {
    i3(h.x, h.y, h.y.wrapping_add(h.x).wrapping_neg())
}
/// `Os.Hex.HexMath.GetHex3(float2)` 0x3DC0E0: `z = -x - y`.
pub fn get_hex3_f(h: Float2) -> Float3 {
    f3(h.x, h.y, -h.x - h.y)
}

/// `Os.Hex.HexMath.HexToPlane(float3)` 0x3DC4B0.
pub fn hex_to_plane(h: Float3) -> Float2 {
    let (ax, ay, az) = (statics::hex_axis_x(), statics::hex_axis_y(), statics::hex_axis_z());
    f2(h.x * ax.x + h.y * ay.x + h.z * az.x, h.x * ax.y + h.y * ay.y + h.z * az.y)
}
/// `Os.Hex.HexMath.HexToPlane(float2)` 0x3DC550: with `z = -x - y`.
pub fn hex_to_plane2(h: Float2) -> Float2 {
    hex_to_plane(get_hex3_f(h))
}
/// `Os.Hex.HexMath.PlaneToHex` 0x3DC650: `2 * dot(p, axis)` per axis.
pub fn plane_to_hex(p: Float2) -> Float3 {
    let (ax, ay, az) = (statics::hex_axis_x(), statics::hex_axis_y(), statics::hex_axis_z());
    let d = |a: Float2| {
        let v = p.x * a.x + p.y * a.y;
        v + v
    };
    f3(d(ax), d(ay), d(az))
}
/// `Os.Hex.HexMath.PlaneToWorld` 0x3DC700: `(p.x, y, p.y)`.
pub fn plane_to_world(p: Float2, y: f32) -> Float3 {
    f3(p.x, y, p.y)
}
/// `Os.Hex.HexMath.WorldToPlane` 0x3DC830: `(w.x, w.z)`.
pub fn world_to_plane(w: Float3) -> Float2 {
    f2(w.x, w.z)
}

/// `Os.Hex.HexMath.HexRound` 0x3DC3B0: round each coordinate, then rebuild the one that rounded worst.
pub fn hex_round(h: Float3) -> Int3 {
    let (rx, ry, rz) = (mathf::round_to_int(h.x), mathf::round_to_int(h.y), mathf::round_to_int(h.z));
    let (dx, dy, dz) = ((rx as f32 - h.x).abs(), (ry as f32 - h.y).abs(), (rz as f32 - h.z).abs());
    if dy < dx && dz < dx {
        i3(rz.wrapping_add(ry).wrapping_neg(), ry, rz)
    } else if dy <= dz {
        i3(rx, ry, ry.wrapping_add(rx).wrapping_neg())
    } else {
        i3(rx, rz.wrapping_add(rx).wrapping_neg(), rz)
    }
}
/// `Os.Hex.HexMath.HexRoundX` 0x3DC350: y and z rounded, x rebuilt.
pub fn hex_round_x(h: Float3) -> Int3 {
    let (ry, rz) = (mathf::round_to_int(h.y), mathf::round_to_int(h.z));
    i3(rz.wrapping_add(ry).wrapping_neg(), ry, rz)
}

/// `Os.Hex.HexMath.HexClamp` 0x3DC110: clamp a hex position into `min..max`, sliding along the clamped axis.
/// Kept as the game has it, including the `x > max.x, y > max.y` corner, whose z uses `min.x`.
pub fn hex_clamp(p: Float3, min: Float3, max: Float3) -> Float3 {
    let y_in = min.y <= p.y && p.y <= max.y;
    let z_in = min.z <= p.z && p.z <= max.z;
    let yc = if p.y < min.y { min.y } else { max.y };
    let zc = if p.z < min.z { min.z } else { max.z };
    if p.x < min.x {
        if !y_in {
            return f3(min.x, yc, -min.x - yc);
        }
        if z_in {
            let f = (min.x - p.x) * 0.5;
            return f3(p.x + (min.x - p.x), p.y - f, p.z - f);
        }
        return f3(min.x, -min.x - zc, zc);
    }
    if max.x < p.x {
        if p.y < min.y {
            return f3(max.x, min.y, -max.x - min.y);
        }
        if p.y <= max.y {
            if z_in {
                let f = (p.x - max.x) * 0.5;
                return f3(p.x - (p.x - max.x), p.y + f, p.z + f);
            }
            return f3(max.x, -max.x - zc, zc);
        }
        return f3(max.x, max.y, -min.x - max.y);
    }
    if y_in {
        if z_in {
            return p;
        }
        let f = (zc - p.z) * 0.5;
        return f3(p.x - f, p.y - f, p.z + (zc - p.z));
    }
    if z_in {
        let f = (yc - p.y) * 0.5;
        return f3(p.x - f, p.y + (yc - p.y), p.z - f);
    }
    f3(-zc - yc, yc, zc)
}

/// `Os.Hex.HexMath.RotateCCW(int2)` 0x3DC740.
pub fn rotate_ccw2i(h: Int2) -> Int2 {
    Int2 { x: h.y.wrapping_neg(), y: h.y.wrapping_add(h.x) }
}
/// `Os.Hex.HexMath.RotateCCW2(int2)` 0x3DC730 (two steps).
pub fn rotate_ccw2_2i(h: Int2) -> Int2 {
    Int2 { x: h.y.wrapping_add(h.x).wrapping_neg(), y: h.x }
}
/// `Os.Hex.HexMath.RotateCW(int2)` 0x3DC7C0.
pub fn rotate_cw2i(h: Int2) -> Int2 {
    Int2 { x: h.y.wrapping_add(h.x), y: h.x.wrapping_neg() }
}
/// `Os.Hex.HexMath.RotateCW2(int2)` 0x3DC7B0 (two steps).
pub fn rotate_cw2_2i(h: Int2) -> Int2 {
    Int2 { x: h.y, y: h.y.wrapping_add(h.x).wrapping_neg() }
}
/// `Os.Hex.HexMath.RotateCCW(int3)` 0x3DC750: `(-y, -z, -x)`.
pub fn rotate_ccw(h: Int3) -> Int3 {
    i3(h.y.wrapping_neg(), h.z.wrapping_neg(), h.x.wrapping_neg())
}
/// `Os.Hex.HexMath.RotateCW(int3)` 0x3DC7D0: `(-z, -x, -y)`.
pub fn rotate_cw(h: Int3) -> Int3 {
    i3(h.z.wrapping_neg(), h.x.wrapping_neg(), h.y.wrapping_neg())
}
/// `Os.Hex.HexMath.RotateCCW(float3)` 0x3DC770.
pub fn rotate_ccw_f(h: Float3) -> Float3 {
    f3(-h.y, -h.z, -h.x)
}
/// `Os.Hex.HexMath.RotateCW(float3)` 0x3DC7F0.
pub fn rotate_cw_f(h: Float3) -> Float3 {
    f3(-h.z, -h.x, -h.y)
}

/// `Os.Hex.HexOnionIterator`: walks every hex ring by ring outward from the centre (vertex 0 is the centre,
/// then 6 per ring step), keeping the ring (`layer_index`), the side of the ring and the position on it.
/// Field order and sizes are the game's (44 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HexOnionIterator {
    pub vert_index: u32,
    pub side_index: u32,
    pub layer_index: u32,
    pub vert_index_in_side: u32,
    pub vert_count_in_side: u32,
    pub hex_pos: Int3,
    pub delta: Int3,
}

impl HexOnionIterator {
    /// `Os.Hex.HexOnionIterator.GetAt` 0x3DD8E0: a fresh iterator moved to `vert_index`.
    pub fn get_at(vert_index: u32) -> HexOnionIterator {
        let mut it = HexOnionIterator::default();
        it.set_vert_index(vert_index);
        it
    }

    /// `Os.Hex.HexOnionIterator.GetWidthDelta` 0x3DD910: at the centre, stepping by `delta`.
    pub fn with_delta(delta: Int3) -> HexOnionIterator {
        HexOnionIterator { delta, ..Default::default() }
    }

    /// `Os.Hex.HexOnionIterator.get_hexPos2` 0x24D4F0.
    pub fn hex_pos2(&self) -> Int2 {
        Int2 { x: self.hex_pos.x, y: self.hex_pos.y }
    }

    /// `Os.Hex.HexOnionIterator.get_dualPos2` 0x3DDB60: `(x - z, y - x)`.
    pub fn dual_pos2(&self) -> Int2 {
        Int2 { x: self.hex_pos.x.wrapping_sub(self.hex_pos.z), y: self.hex_pos.y.wrapping_sub(self.hex_pos.x) }
    }

    /// `Os.Hex.HexOnionIterator.Increment` 0x3DD960: one step; from the untouched centre it starts ring 1.
    pub fn increment(&mut self) -> Int3 {
        if self.vert_index == 0 && self.delta == Int3::default() {
            self.delta = i3(-1, 0, 1);
            self.hex_pos = i3(1, -1, 0);
            self.layer_index = 1;
            self.vert_count_in_side = 1;
            self.vert_index_in_side = 0;
            self.side_index = 0;
        } else {
            self.vert_index_in_side = self.vert_index_in_side.wrapping_add(1);
            self.hex_pos = i3(
                self.delta.x.wrapping_add(self.hex_pos.x),
                self.delta.y.wrapping_add(self.hex_pos.y),
                self.delta.z.wrapping_add(self.hex_pos.z),
            );
            if self.vert_index_in_side == self.vert_count_in_side {
                self.side_index = self.side_index.wrapping_add(1);
                self.delta = rotate_cw(self.delta);
                self.vert_index_in_side = 0;
                if self.side_index == 6 {
                    self.vert_count_in_side = self.vert_count_in_side.wrapping_add(1);
                    self.layer_index = self.layer_index.wrapping_add(1);
                    self.hex_pos.x = self.hex_pos.x.wrapping_add(1);
                    self.hex_pos.y = self.hex_pos.y.wrapping_sub(1);
                    self.side_index = 0;
                }
            }
        }
        self.vert_index = self.vert_index.wrapping_add(1);
        self.hex_pos
    }

    /// `Os.Hex.HexOnionIterator.GetAndIncrement` 0x3DD8A0: the position before the step.
    pub fn get_and_increment(&mut self) -> Int3 {
        let p = self.hex_pos;
        self.increment();
        p
    }

    /// `Os.Hex.HexOnionIterator.Decrement` 0x3DD6F0: one step back (at the centre the game logs an error and
    /// stays).
    pub fn decrement(&mut self) -> Int3 {
        if self.vert_index == 0 {
            return self.hex_pos;
        }
        if self.vert_index_in_side == 0 {
            self.delta = rotate_ccw(self.delta);
            if self.side_index == 0 {
                self.vert_count_in_side = self.vert_count_in_side.wrapping_sub(1);
                self.layer_index = self.layer_index.wrapping_sub(1);
                self.hex_pos.x = self.hex_pos.x.wrapping_sub(1);
                self.hex_pos.y = self.hex_pos.y.wrapping_add(1);
                self.side_index = 5;
            } else {
                self.side_index -= 1;
            }
            self.vert_index_in_side = self.vert_count_in_side.wrapping_sub(1);
        } else {
            self.vert_index_in_side -= 1;
        }
        self.hex_pos = i3(
            self.hex_pos.x.wrapping_sub(self.delta.x),
            self.hex_pos.y.wrapping_sub(self.delta.y),
            self.hex_pos.z.wrapping_sub(self.delta.z),
        );
        self.vert_index -= 1;
        self.hex_pos
    }

    /// `Os.Hex.HexOnionIterator.set_vertIndex` 0x3DDBA0: steps forward to `target`. Going back, the game calls
    /// `Increment` (not `Decrement`) until the counter wraps to 0, about 2³² steps: never reached in play.
    /// STANDIN: we refuse that path instead of spinning.
    pub fn set_vert_index(&mut self, target: u32) {
        while self.vert_index < target {
            self.increment();
        }
        assert!(
            !(target < self.vert_index && self.vert_index != 0),
            "HexOnionIterator.set_vertIndex backwards: the original spins ~2^32 Increments (bug kept out)"
        );
    }
}
