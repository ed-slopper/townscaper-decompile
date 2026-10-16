//! `Os.Utils.ExtraMath`: the game's small maths helpers (row E1b). Checked against the running game by
//! `tests/os_extra_math.rs` (`TS_HARNESS=1`).

use crate::unity::geometry::{rgba, Color};
use crate::unity::mathematics::{f2, f3, f4, i3, Float2, Float3, Float4, Int3, Int3x2};
use crate::unity::mathf;
use crate::unity::vector::{sqrt_f64, v2, v3, v4, Vector2, Vector3, Vector4};

/// `ExtraMath.third`.
pub const THIRD: f32 = 0.333_333_34;
/// `ExtraMath.invMaxByte` (1 / 255).
pub const INV_MAX_BYTE: f32 = 0.003_921_569;

/// `ExtraMath.toneScale` (`ExtraMath..cctor` 0x6CDE40): `(float)Math.Pow(2, 1 / 12)`, a semitone.
pub fn tone_scale() -> f32 {
    2f64.powf(1.0 / 12.0) as f32
}

/// `Os.Utils.ExtraMath.ToneOffsetToPitch` 0x6CDDD0: `(float)Math.Pow(toneScale, tone)`.
pub fn tone_offset_to_pitch(tone: f32) -> f32 {
    (tone_scale() as f64).powf(tone as f64) as f32
}

/// `ExtraMath.Swap<T>(ref a, ref b)` (generic; the 3- and 4-argument rotations are written out where used).
pub fn swap<T>(a: &mut T, b: &mut T) {
    std::mem::swap(a, b);
}

/// `Os.Utils.ExtraMath.SetW(float3, float)` 0x2A3FC0.
pub fn set_w_f3(v: Float3, w: f32) -> Float4 {
    f4(v.x, v.y, v.z, w)
}
/// `Os.Utils.ExtraMath.SetW(Vector3, float)` 0x6CDCA0.
pub fn set_w(v: Vector3, w: f32) -> Vector4 {
    v4(v.x, v.y, v.z, w)
}
/// `Os.Utils.ExtraMath.SetA` 0x6CDC50.
pub fn set_a(c: Color, a: f32) -> Color {
    rgba(c.r, c.g, c.b, a)
}
/// `Os.Utils.ExtraMath.SetX(float3, float)` 0x6CDCF0.
pub fn set_x_f3(v: Float3, x: f32) -> Float3 {
    f3(x, v.y, v.z)
}
/// `Os.Utils.ExtraMath.SetY(float3, float)` 0x6CDD50.
pub fn set_y_f3(v: Float3, y: f32) -> Float3 {
    f3(v.x, y, v.z)
}
/// `Os.Utils.ExtraMath.SetZ(float3, float)` 0x6CDDA0.
pub fn set_z_f3(v: Float3, z: f32) -> Float3 {
    f3(v.x, v.y, z)
}
/// `Os.Utils.ExtraMath.SetX(Vector2, float)` 0x6CDD20.
pub fn set_x(v: Vector2, x: f32) -> Vector2 {
    v2(x, v.y)
}
/// `Os.Utils.ExtraMath.SetY(Vector2, float)` 0x6CDD80.
pub fn set_y(v: Vector2, y: f32) -> Vector2 {
    v2(v.x, y)
}
/// `Os.Utils.ExtraMath.XZtoXYZ` / `GetXYZfromXZ` 0x3DC700: `(xz.x, y, xz.y)`.
pub fn xz_to_xyz(xz: Vector2, y: f32) -> Vector3 {
    v3(xz.x, y, xz.y)
}
/// `Os.Utils.ExtraMath.GetXZ` 0x3DC830.
pub fn get_xz(v: Float3) -> Float2 {
    f2(v.x, v.z)
}
/// `Os.Utils.ExtraMath.GetXYZWfromXZ` 0x6CD620.
pub fn get_xyzw_from_xz(xz: Float2, y: f32, w: f32) -> Float4 {
    f4(xz.x, y, xz.y, w)
}
/// `Os.Utils.ExtraMath.Rotate90Y` 0x6CDC20: `(z, y, -x)`.
pub fn rotate90_y(v: Vector3) -> Vector3 {
    v3(v.z, v.y, -v.x)
}
/// `Os.Utils.ExtraMath.Rotate2D90` 0x6CDAB0: `(y, -x)` (sign-bit flips).
pub fn rotate2d_90(v: Float2) -> Float2 {
    f2(v.y, -v.x)
}
/// `Os.Utils.ExtraMath.Rotate2D180` 0x6CDA40: `(-x, -y)`.
pub fn rotate2d_180(v: Float2) -> Float2 {
    f2(-v.x, -v.y)
}
/// `Os.Utils.ExtraMath.Rotate2D270` 0x6CDA80: `(-y, x)`.
pub fn rotate2d_270(v: Float2) -> Float2 {
    f2(-v.y, v.x)
}
/// `Os.Utils.ExtraMath.GetRotate2DFactors` 0x6CD550: `(cos r, sin r)`, each `(float)Math.Cos((double)r)`.
pub fn get_rotate2d_factors(radians: f32) -> Float2 {
    f2((radians as f64).cos() as f32, (radians as f64).sin() as f32)
}
/// `Os.Utils.ExtraMath.ApplyRotate2DFactors` 0x6CCBB0.
pub fn apply_rotate2d_factors(v: Float2, f: Float2) -> Float2 {
    f2(f.x * v.x - v.y * f.y, v.y * f.x + f.y * v.x)
}
/// `Os.Utils.ExtraMath.Rotate2D` 0x6CDAE0: by `degrees * 0.017453292`.
pub fn rotate2d(v: Float2, degrees: f32) -> Float2 {
    let r = degrees * 0.017_453_292;
    let (c, s) = ((r as f64).cos() as f32, (r as f64).sin() as f32);
    f2(v.x * c - s * v.y, s * v.x + v.y * c)
}
/// `Os.Utils.ExtraMath.Clamp01(Vector2)` 0x6CCD20.
pub fn clamp01(v: Vector2) -> Vector2 {
    v2(mathf::clamp01(v.x), mathf::clamp01(v.y))
}
/// `Os.Utils.ExtraMath.Cross2` 0x6CD000: `a.x * b.y - a.y * b.x` computed on the x87 stack (53-bit) and
/// rounded to `float` once (proven by the harness: plain `f32` is 1 ulp off).
pub fn cross2(a: Vector2, b: Vector2) -> f32 {
    (a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64) as f32
}
/// `Os.Utils.ExtraMath.Pow2(float)` 0x6CD9F0.
pub fn pow2(x: f32) -> f32 {
    x * x
}
/// `Os.Utils.ExtraMath.Pow2(int)` 0x6CDA00.
pub fn pow2_i32(x: i32) -> i32 {
    x.wrapping_mul(x)
}
/// `Os.Utils.ExtraMath.Pow(int, uint)` 0x6CDA10: by squaring, wrapping.
pub fn pow_i32(mut x: i32, mut pow: u32) -> i32 {
    let mut r = 1i32;
    while pow != 0 {
        if pow & 1 != 0 {
            r = r.wrapping_mul(x);
        }
        x = x.wrapping_mul(x);
        pow >>= 1;
    }
    r
}
/// `Os.Utils.ExtraMath.InverseLerpUnclamped` 0x6CD930.
pub fn inverse_lerp_unclamped(min: f32, max: f32, value: f32) -> f32 {
    if max == min {
        return 0.0;
    }
    (value - min) / (max - min)
}
/// `Os.Utils.ExtraMath.Encapsulate(Vector2, float)` 0x6CD100: grow the range `(min, max)` to `value`.
pub fn encapsulate_range(min_max: Vector2, value: f32) -> Vector2 {
    v2(mathf::min(min_max.x, value), mathf::max(min_max.y, value))
}
/// `Os.Utils.ExtraMath.GetClampedMagnitudeOne` 0x6CD400.
pub fn clamped_magnitude_one(v: Vector3) -> Vector3 {
    let sq = v.sqr_magnitude();
    if sq <= 1.0 {
        return v;
    }
    let m = sqrt_f64(sq);
    v3(v.x / m, v.y / m, v.z / m)
}
/// `Os.Utils.ExtraMath.GetClampedMagnitude` 0x6CD4A0.
pub fn clamped_magnitude(v: Vector3, max: f32) -> Vector3 {
    let sq = v.sqr_magnitude();
    if sq <= max * max {
        return v;
    }
    let m = sqrt_f64(sq);
    v3(v.x / m * max, v.y / m * max, v.z / m * max)
}
/// `Os.Utils.ExtraMath.GetBarycentric` 0x6CD160: weights of `p0`, `p1`, `p2`, each clamped to 0..1, then
/// normalised to sum to 1.
pub fn barycentric(p: Vector3, p0: Vector3, p1: Vector3, p2: Vector3) -> Vector3 {
    let (a, b, c) = (p1.sub(p0), p2.sub(p0), p.sub(p0));
    let d00 = Vector3::dot(a, a);
    let d01 = Vector3::dot(a, b);
    let d11 = Vector3::dot(b, b);
    let d20 = Vector3::dot(c, a);
    let d21 = Vector3::dot(c, b);
    let inv = 1.0 / (d11 * d00 - d01 * d01);
    let w = (d21 * d00 - d20 * d01) * inv;
    let v = (d20 * d11 - d21 * d01) * inv;
    let u = (1.0 - v) - w;
    let (u, v, w) = (mathf::clamp01(u), mathf::clamp01(v), mathf::clamp01(w));
    let sum = v + u + w;
    v3(u / sum, v / sum, w / sum)
}
/// `Os.Utils.ExtraMath.ClosestPointOnTriangle` 0x6CCD60: the barycentric blend of the corners.
pub fn closest_point_on_triangle(p: Vector3, p0: Vector3, p1: Vector3, p2: Vector3) -> Vector3 {
    let b = barycentric(p, p0, p1, p2);
    v3(
        p1.x * b.y + p0.x * b.x + p2.x * b.z,
        p1.y * b.y + p0.y * b.x + p2.y * b.z,
        p1.z * b.y + p0.z * b.x + p2.z * b.z,
    )
}
/// `Os.Utils.ExtraMath.InvBilinear` 0x6CD690: `(u, v)` of `p` in the quad `p0 p1 p2 p3` (the quadratic
/// inverse of bilinear interpolation), `(-1, -1)` when outside.
pub fn inv_bilinear(p0: Vector2, p1: Vector2, p2: Vector2, p3: Vector2, p: Vector2) -> Vector2 {
    let (ex, ey) = (p1.x - p0.x, p1.y - p0.y);
    let (fx, fy) = (p3.x - p0.x, p3.y - p0.y);
    let gx = (p2.x + (p0.x - p1.x)) - p3.x;
    let gy = (p2.y + (p0.y - p1.y)) - p3.y;
    let (hx, hy) = (p.x - p0.x, p.y - p0.y);
    let k2 = gx * fy - gy * fx;
    let k1 = (ex * fy - ey * fx) + (hx * gy - hy * gx);
    let k0 = hx * ey - hy * ex;
    let w = k1 * k1 - k0 * 4.0 * k2;
    if 0.0 <= w {
        let w = sqrt_f64(w);
        let v = (-k1 - w) / (k2 + k2);
        let u = (hx - fx * v) / (gx * v + ex);
        if 0.0 <= v && v <= 1.0 && 0.0 <= u && u <= 1.0 {
            return v2(u, v);
        }
        let v = (-k1 + w) / (k2 + k2);
        let u = (hx - fx * v) / (gx * v + ex);
        if 0.0 <= v && v <= 1.0 && 0.0 <= u && u <= 1.0 {
            return v2(u, v);
        }
    }
    v2(-1.0, -1.0)
}

// -- integer bounds: int3x2 (c0 min, c1 max) --

/// `Os.Utils.ExtraMath.Center` 0x6CCCD0: `(min + max) / 2` (C# division: toward zero).
pub fn center(b: Int3x2) -> Int3 {
    let h = |a: i32, c: i32| a.wrapping_add(c) / 2;
    i3(h(b.c0.x, b.c1.x), h(b.c0.y, b.c1.y), h(b.c0.z, b.c1.z))
}
/// `Os.Utils.ExtraMath.ClosestPoint` 0x6CCEC0: `pos` clamped into the bounds (max first, then min).
pub fn closest_point(b: Int3x2, pos: Int3) -> Int3 {
    let c = |lo: i32, hi: i32, p: i32| {
        let v = if lo <= p { p } else { lo };
        if hi < v {
            hi
        } else {
            v
        }
    };
    i3(c(b.c0.x, b.c1.x, pos.x), c(b.c0.y, b.c1.y, pos.y), c(b.c0.z, b.c1.z, pos.z))
}
/// `Os.Utils.ExtraMath.Contains(int3x2, int3x2)` 0x6CCF10.
pub fn contains_bounds(a: Int3x2, b: Int3x2) -> bool {
    a.c0.x <= b.c0.x && a.c0.y <= b.c0.y && a.c0.z <= b.c0.z && b.c1.x <= a.c1.x && b.c1.y <= a.c1.y && b.c1.z <= a.c1.z
}
/// `Os.Utils.ExtraMath.Contains(int3x2, int3)` 0x6CCF50.
pub fn contains(b: Int3x2, p: Int3) -> bool {
    b.c0.x <= p.x && b.c0.y <= p.y && b.c0.z <= p.z && p.x <= b.c1.x && p.y <= b.c1.y && p.z <= b.c1.z
}
/// `Os.Utils.ExtraMath.Contains(int3x2, float3)` 0x6CCF90.
pub fn contains_f(b: Int3x2, p: Float3) -> bool {
    b.c0.x as f32 <= p.x
        && b.c0.y as f32 <= p.y
        && b.c0.z as f32 <= p.z
        && p.x <= b.c1.x as f32
        && p.y <= b.c1.y as f32
        && p.z <= b.c1.z as f32
}
/// `Os.Utils.ExtraMath.Encapsulate(int3x2, int3)` 0x6CD020.
pub fn encapsulate(b: Int3x2, p: Int3) -> Int3x2 {
    Int3x2 {
        c0: i3(b.c0.x.min(p.x), b.c0.y.min(p.y), b.c0.z.min(p.z)),
        c1: i3(b.c1.x.max(p.x), b.c1.y.max(p.y), b.c1.z.max(p.z)),
    }
}
/// `Os.Utils.ExtraMath.Encapsulate(int3x2, int3x2)` 0x6CD090.
pub fn encapsulate_bounds(a: Int3x2, b: Int3x2) -> Int3x2 {
    Int3x2 {
        c0: i3(a.c0.x.min(b.c0.x), a.c0.y.min(b.c0.y), a.c0.z.min(b.c0.z)),
        c1: i3(a.c1.x.max(b.c1.x), a.c1.y.max(b.c1.y), a.c1.z.max(b.c1.z)),
    }
}
/// `Os.Utils.ExtraMath.Intersects(int3x2, int3x2)` 0x6CD650.
pub fn intersects(a: Int3x2, b: Int3x2) -> bool {
    b.c0.x <= a.c1.x && a.c0.x <= b.c1.x && b.c0.y <= a.c1.y && a.c0.y <= b.c1.y && b.c0.z <= a.c1.z && a.c0.z <= b.c1.z
}
/// `Os.Utils.ExtraMath.OnEdge` 0x6CD970: inside the bounds and on one of their faces.
pub fn on_edge(b: Int3x2, p: Int3) -> bool {
    let (lo, hi) = (b.c0, b.c1);
    let inside = |v: i32, l: i32, h: i32| l <= v && v <= h;
    ((p.x == lo.x || p.x == hi.x) && inside(p.y, lo.y, hi.y) && inside(p.z, lo.z, hi.z))
        || (inside(p.x, lo.x, hi.x)
            && (((p.y == lo.y || p.y == hi.y) && inside(p.z, lo.z, hi.z))
                || (inside(p.y, lo.y, hi.y) && (p.z == lo.z || p.z == hi.z))))
}

/// `float.CompareTo`.
fn float_compare(a: f32, b: f32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else if a.is_nan() {
        if b.is_nan() {
            0
        } else {
            -1
        }
    } else {
        1
    }
}

/// `Os.Utils.ExtraMath.BinarySearchTransformX` 0x6CCC00, over the children's local x positions (`xs`):
/// halving from `margin_min` to `count - margin_max - 1` with `x.CompareTo(childX)`; the complement of the
/// insertion point when not found (-1 for an empty range). The `Transform` side is row E11's.
pub fn binary_search_transform_x(xs: &[f32], x: f32, margin_min: i32, margin_max: i32) -> i32 {
    let (mut lo, mut hi) = (margin_min, xs.len() as i32 - margin_max - 1);
    let (mut mid, mut order) = (0i32, 0i32);
    while lo <= hi {
        mid = (hi + lo) / 2;
        order = -float_compare(xs[mid as usize], x);
        if order == 0 {
            return mid;
        }
        if order < 0 {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    !(if order >= 1 { mid + 1 } else { mid })
}
