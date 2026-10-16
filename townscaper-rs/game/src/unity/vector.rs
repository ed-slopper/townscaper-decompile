//! `UnityEngine.Vector2`, `Vector3`, `Vector4`, `Vector2Int`: the members the game calls (row F11b), as the
//! reference build computes them. Same memory layout as Unity's (`#[repr(C)]`, `f32` fields in order).
//! Checked against the running game by `tests/unity_vectors.rs` (`TS_HARNESS=1`).

use super::mathf;

/// `Vector3.kEpsilon` (also Vector2's and Vector4's).
pub const K_EPSILON: f32 = 1e-5;
/// `Vector3.kEpsilonNormalSqrt`.
pub const K_EPSILON_NORMAL_SQRT: f32 = 1e-15;
/// `Mathf.Rad2Deg`.
pub const RAD2DEG: f32 = 57.29578;

/// `(float)Math.Sqrt((double)x)`: the square root in double of a float, rounded back once.
pub fn sqrt_f64(x: f32) -> f32 {
    (x as f64).sqrt() as f32
}

/// `float.GetHashCode()` in the build's BCL: the bits, with +0 and -0 both 0.
pub fn float_hash(f: f32) -> i32 {
    if f == 0.0 {
        0
    } else {
        f.to_bits() as i32
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Vector2Int {
    pub x: i32,
    pub y: i32,
}

pub const fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { x, y }
}
pub const fn v3(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3 { x, y, z }
}
pub const fn v4(x: f32, y: f32, z: f32, w: f32) -> Vector4 {
    Vector4 { x, y, z, w }
}

// ---- Vector2 ----

impl Vector2 {
    pub const ZERO: Vector2 = v2(0.0, 0.0);

    /// `UnityEngine.Vector2.get_sqrMagnitude`: `x*x + y*y`.
    pub fn sqr_magnitude(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// `UnityEngine.Vector2.get_magnitude`: `(float)Math.Sqrt(x*x + y*y)`.
    pub fn magnitude(self) -> f32 {
        sqrt_f64(self.x * self.x + self.y * self.y)
    }

    /// `UnityEngine.Vector2.Normalize` (instance): divide by the magnitude above `kEpsilon`, else zero.
    pub fn normalize(&mut self) {
        let mag = self.magnitude();
        *self = if mag > K_EPSILON { v2(self.x / mag, self.y / mag) } else { Vector2::ZERO };
    }

    /// `UnityEngine.Vector2.get_normalized`.
    pub fn normalized(self) -> Vector2 {
        let mut v = self;
        v.normalize();
        v
    }

    /// `UnityEngine.Vector2.Dot`.
    pub fn dot(a: Vector2, b: Vector2) -> f32 {
        a.x * b.x + a.y * b.y
    }

    /// `UnityEngine.Vector2.Angle`: degrees between, 0 below `kEpsilonNormalSqrt`.
    pub fn angle(from: Vector2, to: Vector2) -> f32 {
        let den = sqrt_f64(from.sqr_magnitude() * to.sqr_magnitude());
        if den < K_EPSILON_NORMAL_SQRT {
            return 0.0;
        }
        let dot = mathf::clamp(Vector2::dot(from, to) / den, -1.0, 1.0);
        (dot as f64).acos() as f32 * RAD2DEG
    }

    /// `UnityEngine.Vector2.SignedAngle`: `Angle(from, to) * Sign(from.x * to.y - from.y * to.x)`.
    pub fn signed_angle(from: Vector2, to: Vector2) -> f32 {
        Vector2::angle(from, to) * mathf::sign(from.x * to.y - from.y * to.x)
    }

    /// `UnityEngine.Vector2.Lerp`: `(b - a) * Clamp01(t) + a` per component.
    pub fn lerp(a: Vector2, b: Vector2, t: f32) -> Vector2 {
        let t = mathf::clamp01(t);
        v2((b.x - a.x) * t + a.x, (b.y - a.y) * t + a.y)
    }

    /// `UnityEngine.Vector2.Max`.
    pub fn max(a: Vector2, b: Vector2) -> Vector2 {
        v2(mathf::max(a.x, b.x), mathf::max(a.y, b.y))
    }

    /// `UnityEngine.Vector2.GetHashCode`: `x.GetHashCode() ^ (y.GetHashCode() << 2)`.
    pub fn hash_code(self) -> i32 {
        float_hash(self.x) ^ (float_hash(self.y) << 2)
    }
}

// ---- Vector3 ----

impl Vector3 {
    pub const ZERO: Vector3 = v3(0.0, 0.0, 0.0);
    pub const ONE: Vector3 = v3(1.0, 1.0, 1.0);
    pub const UP: Vector3 = v3(0.0, 1.0, 0.0);
    pub const DOWN: Vector3 = v3(0.0, -1.0, 0.0);
    pub const FORWARD: Vector3 = v3(0.0, 0.0, 1.0);
    pub const RIGHT: Vector3 = v3(1.0, 0.0, 0.0);

    pub fn add(self, o: Vector3) -> Vector3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn sub(self, o: Vector3) -> Vector3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn mul(self, d: f32) -> Vector3 {
        v3(self.x * d, self.y * d, self.z * d)
    }
    pub fn div(self, d: f32) -> Vector3 {
        v3(self.x / d, self.y / d, self.z / d)
    }

    /// `UnityEngine.Vector3.Dot`.
    pub fn dot(a: Vector3, b: Vector3) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    /// `UnityEngine.Vector3.get_sqrMagnitude`.
    pub fn sqr_magnitude(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// `UnityEngine.Vector3.get_magnitude`: `(float)Math.Sqrt(x*x + y*y + z*z)`.
    pub fn magnitude(self) -> f32 {
        sqrt_f64(self.sqr_magnitude())
    }

    /// `UnityEngine.Vector3.Normalize(Vector3)`: `v / |v|` above `kEpsilon`, else zero.
    pub fn normalize_of(v: Vector3) -> Vector3 {
        let mag = v.magnitude();
        if mag > K_EPSILON {
            v.div(mag)
        } else {
            Vector3::ZERO
        }
    }

    /// `UnityEngine.Vector3.Normalize()` (instance).
    pub fn normalize(&mut self) {
        *self = Vector3::normalize_of(*self);
    }

    /// `UnityEngine.Vector3.get_normalized`.
    pub fn normalized(self) -> Vector3 {
        Vector3::normalize_of(self)
    }

    /// `UnityEngine.Vector3.Cross`.
    pub fn cross(l: Vector3, r: Vector3) -> Vector3 {
        v3(l.y * r.z - l.z * r.y, l.z * r.x - l.x * r.z, l.x * r.y - l.y * r.x)
    }

    /// `UnityEngine.Vector3.Distance`: `(float)Math.Sqrt` of the squared difference.
    pub fn distance(a: Vector3, b: Vector3) -> f32 {
        a.sub(b).magnitude()
    }

    /// `UnityEngine.Vector3.Angle`: degrees between, 0 below `kEpsilonNormalSqrt`.
    pub fn angle(from: Vector3, to: Vector3) -> f32 {
        let den = sqrt_f64(from.sqr_magnitude() * to.sqr_magnitude());
        if den < K_EPSILON_NORMAL_SQRT {
            return 0.0;
        }
        let dot = mathf::clamp(Vector3::dot(from, to) / den, -1.0, 1.0);
        (dot as f64).acos() as f32 * RAD2DEG
    }

    /// `UnityEngine.Vector3.Lerp`: `(b - a) * Clamp01(t) + a` per component.
    pub fn lerp(a: Vector3, b: Vector3, t: f32) -> Vector3 {
        let t = mathf::clamp01(t);
        v3((b.x - a.x) * t + a.x, (b.y - a.y) * t + a.y, (b.z - a.z) * t + a.z)
    }

    /// `UnityEngine.Vector3.Min`.
    pub fn min(a: Vector3, b: Vector3) -> Vector3 {
        v3(mathf::min(a.x, b.x), mathf::min(a.y, b.y), mathf::min(a.z, b.z))
    }

    /// `UnityEngine.Vector3.Max`.
    pub fn max(a: Vector3, b: Vector3) -> Vector3 {
        v3(mathf::max(a.x, b.x), mathf::max(a.y, b.y), mathf::max(a.z, b.z))
    }

    /// `UnityEngine.Vector3.MoveTowards`.
    pub fn move_towards(current: Vector3, target: Vector3, max_delta: f32) -> Vector3 {
        let t = target.sub(current);
        let sq = t.x * t.x + t.y * t.y + t.z * t.z;
        if sq == 0.0 || (max_delta >= 0.0 && sq <= max_delta * max_delta) {
            return target;
        }
        let dist = sqrt_f64(sq);
        v3(current.x + t.x / dist * max_delta, current.y + t.y / dist * max_delta, current.z + t.z / dist * max_delta)
    }

    /// `UnityEngine.Vector3.op_Equality`: squared distance below `kEpsilon²`.
    pub fn approx_eq(a: Vector3, b: Vector3) -> bool {
        let d = a.sub(b);
        d.x * d.x + d.y * d.y + d.z * d.z < K_EPSILON * K_EPSILON
    }

    /// `UnityEngine.Vector3.GetHashCode`: `x ^ (y << 2) ^ (z >> 2)` of the float hashes.
    pub fn hash_code(self) -> i32 {
        float_hash(self.x) ^ (float_hash(self.y) << 2) ^ (float_hash(self.z) >> 2)
    }
}

// ---- Vector4 ----

impl Vector4 {
    pub const ZERO: Vector4 = v4(0.0, 0.0, 0.0, 0.0);

    /// `UnityEngine.Vector4..ctor(float, float, float)`: `w = 0`.
    pub fn new3(x: f32, y: f32, z: f32) -> Vector4 {
        v4(x, y, z, 0.0)
    }

    /// `UnityEngine.Vector4.Dot`.
    pub fn dot(a: Vector4, b: Vector4) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w
    }

    /// `UnityEngine.Vector4.get_sqrMagnitude`: `Dot(this, this)`.
    pub fn sqr_magnitude(self) -> f32 {
        Vector4::dot(self, self)
    }

    /// `UnityEngine.Vector4.Normalize(Vector4)` / `get_normalized`.
    pub fn normalized(self) -> Vector4 {
        let mag = sqrt_f64(Vector4::dot(self, self));
        if mag > K_EPSILON {
            v4(self.x / mag, self.y / mag, self.z / mag, self.w / mag)
        } else {
            Vector4::ZERO
        }
    }

    /// `UnityEngine.Vector4.Lerp`.
    pub fn lerp(a: Vector4, b: Vector4, t: f32) -> Vector4 {
        let t = mathf::clamp01(t);
        v4((b.x - a.x) * t + a.x, (b.y - a.y) * t + a.y, (b.z - a.z) * t + a.z, (b.w - a.w) * t + a.w)
    }

    /// `UnityEngine.Vector4.GetHashCode`: `x ^ (y << 2) ^ (z >> 2) ^ (w >> 1)` of the float hashes.
    pub fn hash_code(self) -> i32 {
        float_hash(self.x) ^ (float_hash(self.y) << 2) ^ (float_hash(self.z) >> 2) ^ (float_hash(self.w) >> 1)
    }

    /// `UnityEngine.Vector4.op_Implicit(Vector2)`.
    pub fn from_v2(v: Vector2) -> Vector4 {
        v4(v.x, v.y, 0.0, 0.0)
    }
    /// `UnityEngine.Vector4.op_Implicit(Vector3)`.
    pub fn from_v3(v: Vector3) -> Vector4 {
        v4(v.x, v.y, v.z, 0.0)
    }
    /// `UnityEngine.Vector4.op_Implicit(Vector4)` (to Vector3).
    pub fn xyz(self) -> Vector3 {
        v3(self.x, self.y, self.z)
    }
}

impl Vector2Int {
    /// `UnityEngine.Vector2Int.op_Implicit` (to Vector2).
    pub fn to_vector2(self) -> Vector2 {
        v2(self.x as f32, self.y as f32)
    }
}
