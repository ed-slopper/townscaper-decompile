//! `UnityEngine.Bounds`, `Rect`, `Plane`, `Ray`, `Color`, `Color32`: the members the game calls (row F11b).
//! Unity's memory layouts. Checked against the running game by `tests/unity_vectors.rs` (`TS_HARNESS=1`).

use super::mathf;
use super::vector::{v2, v3, Vector2, Vector3};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub center: Vector3,
    pub extents: Vector3,
}

impl Bounds {
    /// `UnityEngine.Bounds..ctor(Vector3 center, Vector3 size)`: extents = size * 0.5.
    pub fn new(center: Vector3, size: Vector3) -> Bounds {
        Bounds { center, extents: size.mul(0.5) }
    }
    /// `UnityEngine.Bounds.get_size`.
    pub fn size(&self) -> Vector3 {
        self.extents.mul(2.0)
    }
    /// `UnityEngine.Bounds.get_min`.
    pub fn min(&self) -> Vector3 {
        self.center.sub(self.extents)
    }
    /// `UnityEngine.Bounds.get_max`.
    pub fn max(&self) -> Vector3 {
        self.center.add(self.extents)
    }
    /// `UnityEngine.Bounds.SetMinMax`.
    pub fn set_min_max(&mut self, min: Vector3, max: Vector3) {
        self.extents = max.sub(min).mul(0.5);
        self.center = min.add(self.extents);
    }
    /// `UnityEngine.Bounds.Encapsulate(Vector3)`.
    pub fn encapsulate(&mut self, p: Vector3) {
        let (min, max) = (Vector3::min(self.min(), p), Vector3::max(self.max(), p));
        self.set_min_max(min, max);
    }
    /// `UnityEngine.Bounds.Intersects`.
    pub fn intersects(&self, b: &Bounds) -> bool {
        let (amin, amax, bmin, bmax) = (self.min(), self.max(), b.min(), b.max());
        amin.x <= bmax.x && amax.x >= bmin.x && amin.y <= bmax.y && amax.y >= bmin.y && amin.z <= bmax.z && amax.z >= bmin.z
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// `UnityEngine.Rect.get_center`: `(x + width / 2, y + height / 2)`.
    pub fn center(&self) -> Vector2 {
        v2(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
    /// `UnityEngine.Rect.get_max`: `(width + x, height + y)`.
    pub fn max(&self) -> Vector2 {
        v2(self.width + self.x, self.height + self.y)
    }
    /// `UnityEngine.Rect.get_size`.
    pub fn size(&self) -> Vector2 {
        v2(self.width, self.height)
    }
    /// `UnityEngine.Rect.set_center`.
    pub fn set_center(&mut self, c: Vector2) {
        self.x = c.x - self.width / 2.0;
        self.y = c.y - self.height / 2.0;
    }
    /// `UnityEngine.Rect.set_max`: width/height from the new max.
    pub fn set_max(&mut self, m: Vector2) {
        self.width = m.x - self.x;
        self.height = m.y - self.y;
    }
    /// `UnityEngine.Rect.set_min`: keeps the max.
    pub fn set_min(&mut self, m: Vector2) {
        let (xmax, ymax) = (self.width + self.x, self.height + self.y);
        self.x = m.x;
        self.width = xmax - self.x;
        self.y = m.y;
        self.height = ymax - self.y;
    }
    /// `UnityEngine.Rect.op_Equality` (exact fields; `op_Inequality` is its negation).
    pub fn eq(&self, o: &Rect) -> bool {
        self.x == o.x && self.y == o.y && self.width == o.width && self.height == o.height
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vector3,
    pub direction: Vector3,
}

impl Ray {
    /// `UnityEngine.Ray..ctor`: the direction is normalized.
    pub fn new(origin: Vector3, direction: Vector3) -> Ray {
        Ray { origin, direction: direction.normalized() }
    }
    /// `UnityEngine.Ray.GetPoint`: `origin + direction * d`.
    pub fn point(&self, d: f32) -> Vector3 {
        self.origin.add(self.direction.mul(d))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: Vector3,
    pub distance: f32,
}

impl Plane {
    /// `UnityEngine.Plane..ctor(Vector3 inNormal, Vector3 inPoint)`.
    pub fn from_point(normal: Vector3, point: Vector3) -> Plane {
        let n = normal.normalized();
        Plane { normal: n, distance: -Vector3::dot(n, point) }
    }
    /// `UnityEngine.Plane..ctor(Vector3 inNormal, float d)`.
    pub fn new(normal: Vector3, d: f32) -> Plane {
        Plane { normal: normal.normalized(), distance: d }
    }
    /// `UnityEngine.Plane.GetDistanceToPoint`: `Dot(normal, p) + distance`.
    pub fn distance_to_point(&self, p: Vector3) -> f32 {
        Vector3::dot(self.normal, p) + self.distance
    }
    /// `UnityEngine.Plane.Raycast`: `(hit, enter)`; parallel rays (`Approximately(dot, 0)`) give `(false, 0)`.
    pub fn raycast(&self, ray: &Ray) -> (bool, f32) {
        let vdot = Vector3::dot(ray.direction, self.normal);
        let ndot = -Vector3::dot(ray.origin, self.normal) - self.distance;
        if mathf::approximately(vdot, 0.0) {
            return (false, 0.0);
        }
        let enter = ndot / vdot;
        (enter > 0.0, enter)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

impl Color {
    pub const RED: Color = rgba(1.0, 0.0, 0.0, 1.0);
    pub const GREEN: Color = rgba(0.0, 1.0, 0.0, 1.0);
    pub const BLUE: Color = rgba(0.0, 0.0, 1.0, 1.0);
    pub const BLACK: Color = rgba(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Color = rgba(1.0, 1.0, 1.0, 1.0);
    pub const GRAY: Color = rgba(0.5, 0.5, 0.5, 1.0);
    /// `Color.yellow` is not (1, 1, 0): (1, 0.92156863, 0.015686275, 1).
    pub const YELLOW: Color = rgba(1.0, 0.921_568_63, 0.015_686_275, 1.0);

    /// `UnityEngine.Color.Lerp`: `a + (b - a) * Clamp01(t)` per channel.
    pub fn lerp(a: Color, b: Color, t: f32) -> Color {
        let t = mathf::clamp01(t);
        rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
    }

    /// `UnityEngine.Color.HSVToRGB(float, float, float)` (hdr = true).
    pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
        let mut c = Color::WHITE;
        if s == 0.0 {
            c.r = v;
            c.g = v;
            c.b = v;
        } else if v == 0.0 {
            c.r = 0.0;
            c.g = 0.0;
            c.b = 0.0;
        } else {
            c.r = 0.0;
            c.g = 0.0;
            c.b = 0.0;
            let hf = h * 6.0;
            let i = super::conv::f32_to_i32(mathf::floor(hf));
            let t = hf - i as f32;
            let v1 = v * (1.0 - s);
            let v2 = v * (1.0 - s * t);
            let v3 = v * (1.0 - s * (1.0 - t));
            let (r, g, b) = match i {
                0 => (v, v3, v1),
                1 => (v2, v, v1),
                2 => (v1, v, v3),
                3 => (v1, v2, v),
                4 => (v3, v1, v),
                5 => (v, v1, v2),
                6 => (v, v3, v1),
                -1 => (v, v1, v2),
                _ => (0.0, 0.0, 0.0),
            };
            c.r = r;
            c.g = g;
            c.b = b;
        }
        c
    }
}

impl Color32 {
    /// `UnityEngine.Color32.op_Implicit(Color)`: `(byte)Mathf.Round(Clamp01(c) * 255)` per channel.
    pub fn from_color(c: Color) -> Color32 {
        let f = |x: f32| super::conv::f32_to_i32((mathf::clamp01(x) * 255.0).round_ties_even()) as u8;
        Color32 { r: f(c.r), g: f(c.g), b: f(c.b), a: f(c.a) }
    }
    /// `UnityEngine.Color32.op_Implicit(Color32)`: `c / 255f` per channel.
    pub fn to_color(self) -> Color {
        rgba(self.r as f32 / 255.0, self.g as f32 / 255.0, self.b as f32 / 255.0, self.a as f32 / 255.0)
    }
}

pub fn v3_from(c: Color) -> Vector3 {
    v3(c.r, c.g, c.b)
}
