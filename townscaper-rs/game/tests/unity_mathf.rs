//! `ts_game::unity::mathf` against the original game (F9c, F11). Needs `TS_HARNESS=1`.

use ts_check::{arg, harness, ret};
use ts_game::unity::mathf;

/// Inputs with every edge the function has: signs, halves, the i32 range, specials, and a spread of values.
fn round_inputs() -> Vec<f32> {
    let mut v = vec![
        0.0, -0.0, 0.5, 1.5, 2.5, -0.5, -1.5, -2.5, 0.49999997, 0.50000006, 1e-45, -1e-45,
        2147483520.0, 2147483648.0, -2147483648.0, -2147483904.0, 1e10, -1e10,
        f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN, 8388607.5, 8388608.5, -8388607.5,
    ];
    let mut x = 0x2545F491u32; // xorshift, fixed seed: the same inputs every run
    for _ in 0..2000 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let f = f32::from_bits(x);
        if f.is_finite() && f.abs() < 1e12 {
            v.push(f);
        }
        v.push((x % 20001) as f32 / 4.0 - 2500.0); // quarters, many exact halves
    }
    v
}

#[test]
fn round_to_int_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let inputs = round_inputs();
    for &f in &inputs {
        let game = ret::i32(&h.call("UnityEngine.Mathf.RoundToInt", vec![arg::f32(f)], &[], false).unwrap());
        assert_eq!(mathf::round_to_int(f), game, "RoundToInt({f:?}) bits {:08x}", f.to_bits());
    }
    eprintln!("RoundToInt: {} inputs equal to the game", inputs.len());
}
