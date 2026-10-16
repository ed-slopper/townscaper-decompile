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
