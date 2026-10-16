//! `UnityEngine.Mathf`: the members the game calls (row F11a), as the reference build compiles them.
//! Every function is checked against the running game by `tests/unity_mathf.rs` (`TS_HARNESS=1`).

use super::conv::f64_to_i32;

/// `UnityEngine.Mathf.RoundToInt` 0xA3C0A0: `(int)Math.Round((double)f)`: round half to even in double,
/// then the truncating x86 convert (NaN / out of range → `i32::MIN`).
pub fn round_to_int(f: f32) -> i32 {
    f64_to_i32((f as f64).round_ties_even())
}

/// `UnityEngine.Mathf.FloorToInt` 0xA3B930: `(int)Math.Floor((double)f)`.
pub fn floor_to_int(f: f32) -> i32 {
    f64_to_i32((f as f64).floor())
}

/// `UnityEngine.Mathf.CeilToInt` 0xA3B700: `(int)Math.Ceiling((double)f)`.
pub fn ceil_to_int(f: f32) -> i32 {
    f64_to_i32((f as f64).ceil())
}

/// `UnityEngine.Mathf.Floor`: `(float)Math.Floor((double)f)`.
pub fn floor(f: f32) -> f32 {
    (f as f64).floor() as f32
}

/// `UnityEngine.Mathf.Abs(int)` 0xA3B510: branchless, so `int.MinValue` stays `int.MinValue` (no exception).
pub fn abs_i32(v: i32) -> i32 {
    v.wrapping_abs()
}

/// `UnityEngine.Mathf.Min(float, float)` 0xA3BEA0: `a < b ? a : b`.
pub fn min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

/// `UnityEngine.Mathf.Min(int, int)` 0xA3BE00.
pub fn min_i32(a: i32, b: i32) -> i32 {
    if b <= a {
        b
    } else {
        a
    }
}

/// `UnityEngine.Mathf.Max(float, float)` 0xA3BCE0: `a > b ? a : b`.
pub fn max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// `UnityEngine.Mathf.Max(int, int)` 0xA3BD00.
pub fn max_i32(a: i32, b: i32) -> i32 {
    if a <= b {
        b
    } else {
        a
    }
}

/// `UnityEngine.Mathf.Max(float[])` 0xA3BD70: 0 for an empty array, else the first largest by `>`.
pub fn max_slice(values: &[f32]) -> f32 {
    let Some((&first, rest)) = values.split_first() else { return 0.0 };
    rest.iter().fold(first, |m, &v| if m < v { v } else { m })
}

/// `UnityEngine.Mathf.Max(int[])` 0xA3BD10.
pub fn max_slice_i32(values: &[i32]) -> i32 {
    let Some((&first, rest)) = values.split_first() else { return 0 };
    rest.iter().fold(first, |m, &v| if m < v { v } else { m })
}

/// `UnityEngine.Mathf.Sign` 0xA3C170: `f >= 0 ? 1 : -1` (0 gives 1, NaN gives -1: proven by the harness).
pub fn sign(f: f32) -> f32 {
    if f >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `UnityEngine.Mathf.Clamp(float, float, float)` 0xA3B820.
pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if max < value {
        max
    } else {
        value
    }
}

/// `UnityEngine.Mathf.Clamp(int, int, int)` 0xA3B850.
pub fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        min
    } else if max < value {
        max
    } else {
        value
    }
}

/// `UnityEngine.Mathf.Clamp01` 0xA3B7E0: below 0 → 0, above 1 → 1, else the value (NaN stays NaN).
pub fn clamp01(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

/// `UnityEngine.Mathf.Lerp` 0xA3BBC0: `(b - a) * Clamp01(t) + a`.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a) * clamp01(t) + a
}

/// `UnityEngine.Mathf.InverseLerp` 0xA3BA50: 0 when `a == b`, else `Clamp01((value - a) / (b - a))`.
pub fn inverse_lerp(a: f32, b: f32, value: f32) -> f32 {
    if a == b {
        return 0.0;
    }
    clamp01((value - a) / (b - a))
}

/// `UnityEngine.Mathf.Repeat` 0xA3C020: `Clamp(t - Floor(t / length) * length, 0, length)`.
pub fn repeat(t: f32, length: f32) -> f32 {
    clamp(t - floor(t / length) * length, 0.0, length)
}

/// `UnityEngine.Mathf.LerpAngle` 0xA3BAC0: `a + d * Clamp01(t)` with `d = Repeat(b - a, 360)`, less 360 above 180.
/// The build does the last multiply-add on the x87 stack (the function calls the CRT's x87 `floor` and returns
/// in `ST0`), so it is rounded to `float` once, from extended precision: [`x87_mul_add`].
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = repeat(b - a, 360.0);
    if d > 180.0 {
        d -= 360.0;
    }
    super::conv::x87_mul_add(clamp01(t), d, a)
}

/// `UnityEngine.Mathf.MoveTowards` 0xA3BEC0.
pub fn move_towards(current: f32, target: f32, max_delta: f32) -> f32 {
    if (target - current).abs() <= max_delta {
        return target;
    }
    sign(target - current) * max_delta + current
}

/// `UnityEngine.Mathf.SmoothStep` 0xA3C320: `t = -2t³ + 3t²` (t clamped), `to * t + from * (1 - t)`.
pub fn smooth_step(from: f32, to: f32, t: f32) -> f32 {
    let t = clamp01(t);
    let t = -2.0 * t * t * t + 3.0 * t * t;
    (1.0 - t) * from + to * t
}

/// `UnityEngine.Mathf.Epsilon` on PC (no flush-to-zero): the smallest denormal.
pub const EPSILON: f32 = 1.401_298_5e-45;

/// `UnityEngine.Mathf.Approximately` 0xA3B560: `|b - a| < max(1e-6 * max(|a|, |b|), Epsilon * 8)`.
pub fn approximately(a: f32, b: f32) -> bool {
    (b - a).abs() < max(1e-6 * max(a.abs(), b.abs()), EPSILON * 8.0)
}

/// `UnityEngine.Mathf.NextPowerOfTwo` 0xA3BF30 (native): the smallest power of two ≥ `value` (bit smearing on
/// the value less one, as unsigned).
pub fn next_power_of_two(value: i32) -> i32 {
    let mut v = (value as u32).wrapping_sub(1);
    v |= v >> 16;
    v |= v >> 8;
    v |= v >> 4;
    v |= v >> 2;
    v |= v >> 1;
    v.wrapping_add(1) as i32
}

/// Ken Perlin's reference permutation (improved noise, 2002), repeated.
const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69, 142, 8, 99, 37, 240,
    21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32, 57, 177, 33, 88,
    237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83,
    111, 229, 122, 60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216,
    80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186,
    3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58,
    17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193,
    238, 210, 144, 12, 191, 179, 162, 241, 81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128,
    195, 78, 66, 215, 61, 156, 180,
];

fn p(i: u32) -> u32 {
    PERM[(i & 255) as usize] as u32
}

/// The fade `6t⁵ − 15t⁴ + 10t³` in the engine's order: `((6t − 15)·t + 10) · ((t·t)·t)`.
fn fade(t: f32) -> f32 {
    ((t * 6.0 - 15.0) * t + 10.0) * (t * t * t)
}

/// `minss(1, f)`: 1 when `1 < f`, else `f` (NaN stays NaN).
fn min1(f: f32) -> f32 {
    if 1.0 < f {
        1.0
    } else {
        f
    }
}

/// The gradient of the reference improved noise at z = 0.
fn grad(hash: u32, x: f32, y: f32) -> f32 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        0.0
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// `PerlinNoise::Noise` (UnityPlayer.dll 0x105DE770, native, all SSE single precision): 2D improved Perlin
/// noise of `|x|`, `|y|` (mirrored around 0) with Ken Perlin's permutation (the engine's `int[512]` table is the
/// same 256 twice); integer part by truncating convert; fade input clamped to 1; lerps `(b − a)·t + a`.
fn perlin_raw(x: f32, y: f32) -> f32 {
    let (ax, ay) = (x.abs(), y.abs());
    let (xi, yi) = (super::conv::f32_to_i32(ax), super::conv::f32_to_i32(ay));
    let (fx, fy) = (ax - xi as f32, ay - yi as f32);
    let (xi, yi) = (xi as u32 & 255, yi as u32 & 255);
    let a = p(xi) + yi;
    let b = p(xi + 1) + yi;
    let u = fade(min1(fx));
    let v = fade(min1(fy));
    let lerp = |t: f32, a: f32, b: f32| (b - a) * t + a;
    let lower = lerp(u, grad(p(p(a)), fx, fy), grad(p(p(b)), fx - 1.0, fy));
    let upper = lerp(u, grad(p(p(a + 1)), fx, fy - 1.0), grad(p(p(b + 1)), fx - 1.0, fy - 1.0));
    lerp(v, lower, upper)
}

/// `UnityEngine.Mathf.PerlinNoise` 0xA3BF60 → `Mathf_CUSTOM_PerlinNoise` (UnityPlayer.dll 0x107A1A30, native):
/// `(Noise(x, y) + 0.69f) / 1.483f` on the x87 unit (53-bit precision on Windows), rounded to `float` once.
pub fn perlin_noise(x: f32, y: f32) -> f32 {
    ((perlin_raw(x, y) as f64 + 0.69f32 as f64) / 1.483f32 as f64) as f32
}
