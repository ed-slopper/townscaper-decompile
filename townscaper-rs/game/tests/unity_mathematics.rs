//! `ts_game::unity::mathematics` against the original game (F11d). Needs `TS_HARNESS=1`.

use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::mathematics::*;

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}

fn same_words(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.chunks(4).zip(b.chunks(4)).all(|(x, y)| {
            let (x, y) = (f32::from_le_bytes(x.try_into().unwrap()), f32::from_le_bytes(y.try_into().unwrap()));
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
        })
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self) -> f32 {
        match self.next() % 8 {
            0 => [0.0, -0.0, 1.0, -1.0, 1e-20, 1e20, f32::NAN, 0.5][(self.next() % 8) as usize],
            1 => (self.next() % 21) as f32 - 10.0,
            _ => (self.next() as f32 / u32::MAX as f32) * 20.0 - 10.0,
        }
    }
    fn i(&mut self) -> i32 {
        match self.next() % 3 {
            0 => self.next() as i32,
            _ => (self.next() % 201) as i32 - 100,
        }
    }
    fn i3(&mut self) -> Int3 {
        i3(self.i(), self.i(), self.i())
    }
    fn i4(&mut self) -> Int4 {
        Int4 { x: self.i(), y: self.i(), z: self.i(), w: self.i() }
    }
}

const M: &str = "Unity.Mathematics.math";

fn call(h: &mut Harness, m: &str, args: Vec<serde_json::Value>, types: &[&str]) -> Vec<u8> {
    ret::bytes(&h.call(&format!("{M}.{m}"), args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}")))
}

#[test]
fn mathematics_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(41);
    for _ in 0..1500 {
        let v = f2(r.f(), r.f());
        assert!(same_words(&call(&mut h, "length", vec![arg::raw(&bytes(&v))], &["Unity.Mathematics.float2"]), &length2(v).to_le_bytes()), "length({v:?})");
        assert!(same_words(&call(&mut h, "normalize", vec![arg::raw(&bytes(&v))], &["Unity.Mathematics.float2"]), &bytes(&normalize2(v))), "normalize({v:?})");
        let (a, b) = (r.i3(), r.i3());
        assert_eq!(call(&mut h, "abs", vec![arg::raw(&bytes(&a))], &["Unity.Mathematics.int3"]), bytes(&abs3(a)), "abs({a:?})");
        assert_eq!(call(&mut h, "max", vec![arg::raw(&bytes(&a)), arg::raw(&bytes(&b))], &["Unity.Mathematics.int3", "Unity.Mathematics.int3"]), bytes(&max3(a, b)), "max");
        let m3 = Int3x3 { c: [r.i3(), r.i3(), r.i3()] };
        assert_eq!(call(&mut h, "mul", vec![arg::raw(&bytes(&m3)), arg::raw(&bytes(&a))], &["Unity.Mathematics.int3x3", "Unity.Mathematics.int3"]), bytes(&mul_i3x3(&m3, a)), "mul(int3x3, int3)");
        let (m4, n4, v4) = (Int4x4 { c: [r.i4(), r.i4(), r.i4(), r.i4()] }, Int4x4 { c: [r.i4(), r.i4(), r.i4(), r.i4()] }, r.i4());
        assert_eq!(call(&mut h, "mul", vec![arg::raw(&bytes(&m4)), arg::raw(&bytes(&v4))], &["Unity.Mathematics.int4x4", "Unity.Mathematics.int4"]), bytes(&mul_i4x4_v(&m4, v4)), "mul(int4x4, int4)");
        assert_eq!(call(&mut h, "mul", vec![arg::raw(&bytes(&m4)), arg::raw(&bytes(&n4))], &["Unity.Mathematics.int4x4", "Unity.Mathematics.int4x4"]), bytes(&mul_i4x4(&m4, &n4)), "mul(int4x4, int4x4)");
        let f = float4x4_from_int(&m4);
        let after = h.call_on_value("Unity.Mathematics.float4x4..ctor", "Unity.Mathematics.float4x4", &[0u8; 64], vec![arg::raw(&bytes(&m4))], &["Unity.Mathematics.int4x4"], false).unwrap();
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&f)), "float4x4(int4x4)");
        let fm = Float4x4 { c: std::array::from_fn(|_| f4(r.f(), r.f(), r.f(), r.f())) };
        assert!(same_words(&call(&mut h, "inverse", vec![arg::raw(&bytes(&fm))], &["Unity.Mathematics.float4x4"]), &bytes(&inverse(&fm))), "inverse({fm:?})");
        let g = h.call_on_value("Unity.Mathematics.int3.GetHashCode", "Unity.Mathematics.int3", &bytes(&a), vec![], &[], false).unwrap();
        assert_eq!(ret::i32(&g), hash3(a), "{a:?}.GetHashCode");
    }
}

#[test]
fn mathematics_random_matches_the_game() {
    let Some(mut h) = harness() else { return };
    const R: &str = "Unity.Mathematics.Random";
    let mut r = Rng(42);
    for _ in 0..1000 {
        let seed = r.next() | 1;
        let after = h.call_on_value(&format!("{R}..ctor"), R, &[0u8; 4], vec![arg::raw(&seed.to_le_bytes())], &["System.UInt32"], false).unwrap();
        let mut ours = Random::new(seed);
        assert_eq!(ret::hex_field(&after["this_after"]), ours.state.to_le_bytes(), "Random({seed})");
        for k in 0..6 {
            let before = ours.state.to_le_bytes();
            let (g, o): (Vec<u8>, Vec<u8>) = match k % 3 {
                0 => {
                    let rr = h.call_on_value(&format!("{R}.NextFloat"), R, &before, vec![], &[], false).unwrap();
                    assert_eq!(ret::hex_field(&rr["this_after"]), { let mut c = ours; c.next_float(); c.state.to_le_bytes() });
                    (ret::bytes(&rr), ours.next_float().to_le_bytes().to_vec())
                }
                1 => {
                    let (a, b) = (r.f(), r.f());
                    let rr = h.call_on_value(&format!("{R}.NextFloat"), R, &before, vec![arg::f32(a), arg::f32(b)], &["System.Single", "System.Single"], false).unwrap();
                    (ret::bytes(&rr), ours.next_float_range(a, b).to_le_bytes().to_vec())
                }
                _ => {
                    let (a, b) = (r.i(), r.i());
                    let rr = h.call_on_value(&format!("{R}.NextInt"), R, &before, vec![arg::i32(a), arg::i32(b)], &["System.Int32", "System.Int32"], false);
                    match rr {
                        Ok(rr) => (ret::bytes(&rr), ours.next_int_range(a, b).to_le_bytes().to_vec()),
                        Err(_) => continue, // the original's min > max check (only in debug builds) is not ours to port
                    }
                }
            };
            assert!(same_words(&g, &o), "Random step {k}: ours {o:?} game {g:?}");
        }
    }
}
