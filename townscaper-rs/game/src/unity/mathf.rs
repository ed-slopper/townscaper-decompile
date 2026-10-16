//! `UnityEngine.Mathf`.

use super::conv::f64_to_i32;

/// `UnityEngine.Mathf.RoundToInt` 0xA3C0A0: `(int)Math.Round((double)f)`: round half to even in double,
/// then the truncating x86 convert (NaN / out of range → `i32::MIN`).
pub fn round_to_int(f: f32) -> i32 {
    f64_to_i32((f as f64).round_ties_even())
}
