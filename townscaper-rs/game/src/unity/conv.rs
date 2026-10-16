//! Numeric conversions as IL2CPP's x86 code does them (MSVC, SSE2).

/// C# `(int)double`: x86 `cvttsd2si`: truncates toward zero; NaN and out-of-range give `i32::MIN`
/// (Rust's `as i32` saturates instead).
pub fn f64_to_i32(x: f64) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x <= -2147483649.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// C# `(int)float`: x86 `cvttss2si`, same rule.
pub fn f32_to_i32(x: f32) -> i32 {
    f64_to_i32(x as f64)
}

/// `x * y + z` computed on the x87 stack and rounded to `float` once at the end (as MSVC's x86 code does where
/// a `float` function keeps its last expression in `ST0`). The product of two `f32` is exact in `f64`; the sum is
/// rounded to 53 bits here instead of x87's 64 before the final rounding to 24, which can differ only when the
/// exact sum lies within 2^-29 ulp of a `float` halfway point (not seen in the harness checks).
pub fn x87_mul_add(x: f32, y: f32, z: f32) -> f32 {
    (x as f64 * y as f64 + z as f64) as f32
}
