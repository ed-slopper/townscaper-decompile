//! `Placemaker.Quads`: the irregular grid's cells (`Quad`) and corners (`Vert`), row E3a. Both are value types
//! whose byte layout is the game's (padding included, so a copy carries it like the original's). Checked
//! against the running game by `tests/placemaker_quads.rs` (`TS_HARNESS=1`).

pub mod grid_generation;

use crate::os::hex;
use crate::unity::mathematics::{atan2, f2, Float2, Float3, Int2};

/// `Placemaker.Quads.Quad`: four corners as hex positions (`v0..v3`, scaled by `GridConstants.hexScale`) and
/// their plane positions (`p0..p3`), in order around the cell. 68 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Quad {
    pub quad_index: u8,
    pub pad: [u8; 3],
    pub v0: Int2,
    pub v1: Int2,
    pub v2: Int2,
    pub v3: Int2,
    pub p0: Float2,
    pub p1: Float2,
    pub p2: Float2,
    pub p3: Float2,
}

impl Quad {
    /// `Placemaker.Quads.Quad.get_hexPosSum` 0x841160: the corners summed (4 × the centre).
    pub fn hex_pos_sum(&self) -> Int2 {
        let x = self.v3.x.wrapping_add(self.v1.x.wrapping_add(self.v0.x).wrapping_add(self.v2.x));
        let y = self.v3.y.wrapping_add(self.v2.y.wrapping_add(self.v0.y).wrapping_add(self.v1.y));
        Int2 { x, y }
    }

    /// `Placemaker.Quads.Quad.get_planePos` 0x8411A0: the corners' mean.
    pub fn plane_pos(&self) -> Float2 {
        let (p0, p1, p2, p3) = (self.p0, self.p1, self.p2, self.p3);
        f2((p1.x + p0.x + p2.x + p3.x) * 0.25, (p0.y + p1.y + p2.y + p3.y) * 0.25)
    }

    /// `Placemaker.Quads.Quad.GetVert` 0x840BB0 (the game throws `IndexOutOfRangeException` outside 0..=3).
    pub fn get_vert(&self, index: i32) -> Int2 {
        match index {
            0 => self.v0,
            1 => self.v1,
            2 => self.v2,
            3 => self.v3,
            _ => panic!("Quad.GetVert({index}): IndexOutOfRangeException"),
        }
    }

    /// `Placemaker.Quads.Quad.GetPlanePos` 0x840AD0 (throws outside 0..=3, as `GetVert`).
    pub fn get_plane_pos(&self, index: i32) -> Float2 {
        match index {
            0 => self.p0,
            1 => self.p1,
            2 => self.p2,
            3 => self.p3,
            _ => panic!("Quad.GetPlanePos({index}): IndexOutOfRangeException"),
        }
    }

    /// `Placemaker.Quads.Quad.ContainsVert` 0x840A70.
    pub fn contains_vert(&self, v: Int2) -> bool {
        v == self.v0 || v == self.v1 || v == self.v2 || v == self.v3
    }

    /// `Placemaker.Quads.Quad.IsValid` 0x840C40: `v0 != v1` (a default quad has all corners at 0).
    pub fn is_valid(&self) -> bool {
        self.v0 != self.v1
    }

    fn corners(&self) -> [(Int2, Float2); 4] {
        [(self.v0, self.p0), (self.v1, self.p1), (self.v2, self.p2), (self.v3, self.p3)]
    }

    fn set_corners(&mut self, c: [(Int2, Float2); 4]) {
        (self.v0, self.p0) = c[0];
        (self.v1, self.p1) = c[1];
        (self.v2, self.p2) = c[2];
        (self.v3, self.p3) = c[3];
    }

    /// `Placemaker.Quads.Quad.Rotate` 0x840D60: corner `i` becomes corner `i + 1`'s.
    pub fn rotate(&mut self) {
        let mut c = self.corners();
        c.rotate_left(1);
        self.set_corners(c);
    }

    /// `Placemaker.Quads.Quad.Rotate2` 0x840C70: by two corners.
    pub fn rotate2(&mut self) {
        let mut c = self.corners();
        c.rotate_left(2);
        self.set_corners(c);
    }

    /// `Placemaker.Quads.Quad.Rotate3` 0x840CD0: by three corners (one back).
    pub fn rotate3(&mut self) {
        let mut c = self.corners();
        c.rotate_left(3);
        self.set_corners(c);
    }
}

/// `Placemaker.Quads.Vert`: a grid corner with the (up to 6) quads around it, kept sorted by angle around the
/// corner. 432 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vert {
    pub hex_pos: Int2,
    pub plane_pos: Float2,
    pub q0: Quad,
    pub q1: Quad,
    pub q2: Quad,
    pub q3: Quad,
    pub q4: Quad,
    pub q5: Quad,
    pub quad_count: i8,
    pub pad: [u8; 3],
    pub angle: f32,
}

impl Vert {
    /// `Placemaker.Quads.Vert.get_worldPos` 0x3D4D50: `HexMath.PlaneToWorld(planePos, 0)`.
    pub fn world_pos(&self) -> Float3 {
        hex::plane_to_world(self.plane_pos, 0.0)
    }

    /// `Placemaker.Quads.Vert.get_full` 0x3D4D10: `angle > 350` (the quads' angles close the circle).
    pub fn full(&self) -> bool {
        350.0 < self.angle
    }

    /// `Placemaker.Quads.Vert.get_validPosition` 0x3D4D30: `quadCount >= 0`.
    pub fn valid_position(&self) -> bool {
        self.quad_count >= 0
    }

    /// `Placemaker.Quads.Vert.GetQuad` 0x3D4A20: slot `index`; any index past 4 (or negative) is `q5`.
    pub fn get_quad(&self, index: i32) -> Quad {
        match index {
            0 => self.q0,
            1 => self.q1,
            2 => self.q2,
            3 => self.q3,
            4 => self.q4,
            _ => self.q5,
        }
    }

    /// `Placemaker.Quads.Vert.SetQuad` 0x3D4BA0: as `GetQuad`, any index past 4 writes `q5`.
    pub fn set_quad(&mut self, index: i32, quad: Quad) {
        *match index {
            0 => &mut self.q0,
            1 => &mut self.q1,
            2 => &mut self.q2,
            3 => &mut self.q3,
            4 => &mut self.q4,
            _ => &mut self.q5,
        } = quad;
    }

    /// The game's `math.atan2(d.x, d.y)` of `d = quad.planePos - planePos` (x first: the angle from +y towards
    /// +x, not the usual one; only the order matters).
    fn angle_to(&self, q: &Quad) -> f32 {
        let c = q.plane_pos();
        atan2(c.x - self.plane_pos.x, c.y - self.plane_pos.y)
    }

    /// `Placemaker.Quads.Vert.AddQuad` 0x3D4500: insert by the angle of the quad's centre around this corner
    /// (ascending), then `quadCount++`. A seventh quad overwrites `q5` (the count still grows).
    pub fn add_quad(&mut self, quad: Quad) {
        let mut carry = quad;
        let mut carry_angle = self.angle_to(&carry);
        let mut i = 0i32;
        while i < self.quad_count as i32 {
            let q = self.get_quad(i);
            let a = self.angle_to(&q);
            if carry_angle < a {
                self.set_quad(i, carry);
                carry = q;
                carry_angle = a;
            }
            i += 1;
        }
        self.set_quad(self.quad_count as i32, carry);
        self.quad_count = self.quad_count.wrapping_add(1);
    }

    /// `Placemaker.Quads.Vert.ContainsVert` 0x3D47C0: one of the first `quadCount` quads (1..=6) has corner `v`.
    pub fn contains_vert(&self, v: Int2) -> bool {
        let n = self.quad_count;
        if !(1..=6).contains(&n) {
            return false;
        }
        (0..n as i32).any(|i| self.get_quad(i).contains_vert(v))
    }
}
