//! `ts_game::unity::mathf` against the original game (F9c, F11a). Needs `TS_HARNESS=1`.

use serde_json::Value;
use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::mathf;

/// Fixed-seed xorshift, so every run feeds the same inputs.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// A float from a mix of shapes: any finite bit pattern, small ranges, quarters, near-integers.
    fn f32(&mut self) -> f32 {
        let x = self.next();
        match x % 4 {
            0 => {
                let f = f32::from_bits(self.next());
                if f.is_finite() { f } else { 0.25 }
            }
            1 => (self.next() % 20001) as f32 / 4.0 - 2500.0,
            2 => (self.next() as f32 / u32::MAX as f32) * 2.0 - 0.5,
            _ => (self.next() % 2001) as f32 - 1000.0 + if x & 8 == 0 { 1e-4 } else { -1e-4 },
        }
    }
    fn i32(&mut self) -> i32 {
        match self.next() % 3 {
            0 => self.next() as i32,
            1 => (self.next() % 4001) as i32 - 2000,
            _ => (self.next() % 70000) as i32,
        }
    }
}

const SPECIAL: [f32; 18] = [
    0.0, -0.0, 0.5, -0.5, 1.0, -1.0, 1.5, 2.5, -2.5, 180.0, 360.0, -360.0, 1e-45, 2147483648.0, -2147483904.0,
    f32::INFINITY, f32::NEG_INFINITY, f32::NAN,
];
const SPECIAL_I: [i32; 10] = [0, 1, -1, 2, 3, 1023, 1024, 1025, i32::MAX, i32::MIN];

fn floats(n: usize, seed: u32) -> Vec<f32> {
    let mut r = Rng(seed);
    SPECIAL.iter().copied().chain((0..n).map(|_| r.f32())).collect()
}

fn ints(n: usize, seed: u32) -> Vec<i32> {
    let mut r = Rng(seed);
    SPECIAL_I.iter().copied().chain((0..n).map(|_| r.i32())).collect()
}

/// Tuples of `k` floats: every special combination for k ≤ 2, then a fixed-seed spread.
fn tuples(k: usize, n: usize, seed: u32) -> Vec<Vec<f32>> {
    let mut out = Vec::new();
    if k <= 2 {
        for &a in &SPECIAL {
            if k == 1 {
                out.push(vec![a]);
                continue;
            }
            for &b in &SPECIAL {
                out.push(vec![a, b]);
            }
        }
    }
    let mut r = Rng(seed);
    for _ in 0..n {
        out.push((0..k).map(|_| if r.next() % 8 == 0 { SPECIAL[(r.next() % 18) as usize] } else { r.f32() }).collect());
    }
    out
}

fn call(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> Value {
    h.call(&format!("UnityEngine.Mathf.{m}"), args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

/// Same bits (so NaN == NaN and -0 != 0 are both caught).
fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

const F: &str = "System.Single";
const I: &str = "System.Int32";

#[test]
fn float_functions_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut checked = 0;

    type F1 = fn(f32) -> f32;
    let f1: [(&str, F1); 2] = [("Sign", mathf::sign), ("Clamp01", mathf::clamp01)];
    for (name, f) in f1 {
        for x in floats(1500, 11) {
            let g = ret::f32(&call(&mut h, name, vec![arg::f32(x)], &[F]));
            assert!(same(f(x), g), "{name}({x:?}): ours {:?} game {g:?}", f(x));
            checked += 1;
        }
    }
    type FI = fn(f32) -> i32;
    let fi: [(&str, FI); 3] =
        [("RoundToInt", mathf::round_to_int), ("FloorToInt", mathf::floor_to_int), ("CeilToInt", mathf::ceil_to_int)];
    for (name, f) in fi {
        for x in floats(1500, 12) {
            let g = ret::i32(&call(&mut h, name, vec![arg::f32(x)], &[]));
            assert_eq!(f(x), g, "{name}({x:?})");
            checked += 1;
        }
    }
    type F2 = fn(f32, f32) -> f32;
    let f2: [(&str, F2); 3] = [("Min", mathf::min), ("Max", mathf::max), ("Repeat", mathf::repeat)];
    for (name, f) in f2 {
        for t in tuples(2, 1500, 13) {
            let g = ret::f32(&call(&mut h, name, vec![arg::f32(t[0]), arg::f32(t[1])], &[F, F]));
            assert!(same(f(t[0], t[1]), g), "{name}({}, {}): ours {:?} game {g:?}", t[0], t[1], f(t[0], t[1]));
            checked += 1;
        }
    }
    for t in tuples(2, 1500, 14) {
        let g = call(&mut h, "Approximately", vec![arg::f32(t[0]), arg::f32(t[1])], &[]);
        assert_eq!(mathf::approximately(t[0], t[1]), ret::bytes(&g)[0] != 0, "Approximately({}, {})", t[0], t[1]);
        checked += 1;
    }
    type F3 = fn(f32, f32, f32) -> f32;
    let f3: [(&str, F3); 6] = [
        ("Clamp", mathf::clamp),
        ("Lerp", mathf::lerp),
        ("InverseLerp", mathf::inverse_lerp),
        ("LerpAngle", mathf::lerp_angle),
        ("MoveTowards", mathf::move_towards),
        ("SmoothStep", mathf::smooth_step),
    ];
    for (name, f) in f3 {
        for t in tuples(3, 2000, 15) {
            let g = ret::f32(&call(&mut h, name, t.iter().map(|&x| arg::f32(x)).collect(), &[F, F, F]));
            let o = f(t[0], t[1], t[2]);
            assert!(same(o, g), "{name}({}, {}, {}): ours {o:?} game {g:?}", t[0], t[1], t[2]);
            checked += 1;
        }
    }
    eprintln!("Mathf float functions: {checked} calls equal to the game");
}

#[test]
fn perlin_noise_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut checked = 0;
    // Perlin noise where the game calls it: small coordinates, many fractions.
    let mut r = Rng(16);
    for _ in 0..3000 {
        let (x, y) = ((r.next() % 100000) as f32 / 977.0 - 30.0, (r.next() % 100000) as f32 / 613.0 - 50.0);
        let g = ret::f32(&call(&mut h, "PerlinNoise", vec![arg::f32(x), arg::f32(y)], &[F, F]));
        assert!(same(mathf::perlin_noise(x, y), g), "PerlinNoise({x}, {y}): ours {:?} game {g:?}", mathf::perlin_noise(x, y));
        checked += 1;
    }
    eprintln!("Mathf.PerlinNoise: {checked} calls equal to the game");
}

#[test]
fn int_functions_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut checked = 0;
    type I1 = fn(i32) -> i32;
    let i1: [(&str, I1); 2] = [("Abs", mathf::abs_i32), ("NextPowerOfTwo", mathf::next_power_of_two)];
    for (name, f) in i1 {
        for x in ints(1500, 17) {
            let g = ret::i32(&call(&mut h, name, vec![arg::i32(x)], &[I]));
            assert_eq!(f(x), g, "{name}({x})");
            checked += 1;
        }
    }
    type I2 = fn(i32, i32) -> i32;
    let i2: [(&str, I2); 2] = [("Min", mathf::min_i32), ("Max", mathf::max_i32)];
    for (name, f) in i2 {
        let (a, b) = (ints(1500, 18), ints(1500, 19));
        for (&x, &y) in a.iter().zip(&b) {
            let g = ret::i32(&call(&mut h, name, vec![arg::i32(x), arg::i32(y)], &[I, I]));
            assert_eq!(f(x, y), g, "{name}({x}, {y})");
            checked += 1;
        }
    }
    let (a, b, c) = (ints(1500, 20), ints(1500, 21), ints(1500, 22));
    for i in 0..a.len() {
        let g = ret::i32(&call(&mut h, "Clamp", vec![arg::i32(a[i]), arg::i32(b[i]), arg::i32(c[i])], &[I, I, I]));
        assert_eq!(mathf::clamp_i32(a[i], b[i], c[i]), g, "Clamp({}, {}, {})", a[i], b[i], c[i]);
        checked += 1;
    }
    // Max over arrays (empty, one, many; NaN inside for floats)
    let mut r = Rng(23);
    for n in 0..300usize {
        let fv: Vec<f32> = (0..n % 9).map(|_| if r.next() % 10 == 0 { f32::NAN } else { r.f32() }).collect();
        let bytes: Vec<u8> = fv.iter().flat_map(|x| x.to_le_bytes()).collect();
        let g = ret::f32(&call(&mut h, "Max", vec![arg::array(F, &bytes)], &["System.Single[]"]));
        assert!(same(mathf::max_slice(&fv), g), "Max({fv:?}): ours {:?} game {g:?}", mathf::max_slice(&fv));
        let iv: Vec<i32> = (0..n % 9).map(|_| r.i32()).collect();
        let bytes: Vec<u8> = iv.iter().flat_map(|x| x.to_le_bytes()).collect();
        let g = ret::i32(&call(&mut h, "Max", vec![arg::array(I, &bytes)], &["System.Int32[]"]));
        assert_eq!(mathf::max_slice_i32(&iv), g, "Max({iv:?})");
        checked += 2;
    }
    eprintln!("Mathf int and array functions: {checked} calls equal to the game");
}
