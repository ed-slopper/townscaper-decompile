//! `ts_game::os::utils` against the original game (E1a). Needs `TS_HARNESS=1`.

use serde_json::Value;
use ts_check::{arg, harness, ret, Harness};
use ts_game::os::utils::*;
use ts_game::unity::mathematics::{Int3, Int3x3};
use ts_game::unity::vector::{v2, v3};

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
            0 => [0.0, -0.0, 1.0, 0.5, -0.5, 0.998, 1.0 / 512.0, 3.0 / 512.0, 1.0 / 508.0, f32::NAN, 1e9, -1e9][(self.next() % 12) as usize],
            1 => (self.next() % 520) as f32 / 512.0 - 0.01,
            _ => (self.next() as f32 / u32::MAX as f32) * 3.0 - 1.5,
        }
    }
}

const U: &str = "Os.Utils";

fn st(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> Vec<u8> {
    ret::bytes(&h.call(&format!("{U}.{m}"), args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}")))
}

fn on(h: &mut Harness, t: &str, m: &str, this: &[u8], args: Vec<Value>, types: &[&str]) -> Value {
    h.call_on_value(&format!("{U}.{t}.{m}"), &format!("{U}.{t}"), this, args, types, false).unwrap_or_else(|e| panic!("{t}.{m}: {e:?}"))
}

#[test]
fn bitwise_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(61);
    for _ in 0..600 {
        let bits: [bool; 8] = std::array::from_fn(|_| r.next() % 2 == 0);
        let g = st(&mut h, "Bitwise.GetByte", bits.iter().map(|&b| arg::bool(b)).collect(), &[]);
        assert_eq!(g[0], get_byte(bits), "GetByte({bits:?})");
        let (b, i) = (r.next() as i32, (r.next() % 70) as i32 - 3);
        assert_eq!(st(&mut h, "Bitwise.IsBitSet", vec![arg::i32(b), arg::i32(i)], &["System.Int32", "System.Int32"])[0] != 0, is_bit_set(b, i), "IsBitSet({b}, {i})");
        let byte = r.next() as u8;
        assert_eq!(st(&mut h, "Bitwise.IsBitSet", vec![arg::raw(&[byte]), arg::i32(i)], &["System.Byte", "System.Int32"])[0] != 0, is_bit_set_u8(byte, i), "IsBitSet({byte}u8, {i})");
        let v = r.next() % 2 == 0;
        let rr = h.call(&format!("{U}.Bitwise.SetBit"), vec![arg::raw(&[byte]), arg::i32(i), arg::bool(v)], &[], false).unwrap();
        let mut ours = byte;
        set_bit(&mut ours, i, v);
        assert_eq!(ret::hex_field(&rr["args_after"][0])[0], ours, "SetBit({byte}, {i}, {v})");
    }
}

#[test]
fn byte_floats_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(62);
    for _ in 0..1500 {
        let (a, b, c) = (r.f(), r.f(), r.f());
        let byte = r.next() as u8;
        // ByteFloat
        assert_eq!(st(&mut h, "ByteFloat.FloatToByte", vec![arg::f32(a)], &[])[0], float_to_byte(a), "FloatToByte({a})");
        assert!(same_words(&st(&mut h, "ByteFloat.ByteToFloat", vec![arg::raw(&[byte])], &[]), &byte_to_float(byte).to_le_bytes()), "ByteToFloat({byte})");
        let bf = ByteFloat { value: byte };
        assert_eq!(ret::i32(&on(&mut h, "ByteFloat", "GetHashCode", &bytes(&bf), vec![], &[])), bf.hash_code(), "ByteFloat hash");
        let after = on(&mut h, "ByteFloat", ".ctor", &[0], vec![arg::f32(a)], &["System.Single"]);
        assert_eq!(ret::hex_field(&after["this_after"]), bytes(&ByteFloat::new(a)), "ByteFloat({a})");
        // ByteFloat2
        let b2 = ByteFloat2::new(a, b);
        let after = on(&mut h, "ByteFloat2", ".ctor", &[0, 0], vec![arg::f32(a), arg::f32(b)], &["System.Single", "System.Single"]);
        assert_eq!(ret::hex_field(&after["this_after"]), bytes(&b2), "ByteFloat2({a}, {b})");
        assert_eq!(st(&mut h, "ByteFloat2.op_Explicit", vec![arg::raw(&bytes(&v2(a, b)))], &["UnityEngine.Vector2"]), bytes(&b2), "ByteFloat2(Vector2)");
        // (the op_Explicit overloads from ByteFloat2 differ only by return type: covered by get_fx / get_fy)
        assert_eq!(ret::i32(&on(&mut h, "ByteFloat2", "GetHashCode", &bytes(&b2), vec![], &[])), b2.hash_code(), "ByteFloat2 hash");
        assert!(same_words(&ret::bytes(&on(&mut h, "ByteFloat2", "get_fy", &bytes(&b2), vec![], &[])), &b2.fy().to_le_bytes()), "fy");
        // ByteFloat3
        let b3 = ByteFloat3::new(a, b, c);
        let after = on(&mut h, "ByteFloat3", ".ctor", &[0, 0, 0], vec![arg::f32(a), arg::f32(b), arg::f32(c)], &["System.Single", "System.Single", "System.Single"]);
        assert_eq!(ret::hex_field(&after["this_after"]), bytes(&b3), "ByteFloat3({a}, {b}, {c})");
        assert_eq!(st(&mut h, "ByteFloat3.op_Explicit", vec![arg::raw(&bytes(&v3(a, b, c)))], &["UnityEngine.Vector3"]), bytes(&b3), "ByteFloat3(Vector3)");
        assert!(same_words(&st(&mut h, "ByteFloat3.op_Explicit", vec![arg::raw(&bytes(&b3))], &["Os.Utils.ByteFloat3"]), &bytes(&b3.to_vector3())), "Vector3({b3:?})");
        assert!(same_words(&ret::bytes(&on(&mut h, "ByteFloat3", "get_normal", &bytes(&b3), vec![], &[])), &bytes(&b3.normal())), "{b3:?}.normal");
        assert!(same_words(&ret::bytes(&on(&mut h, "ByteFloat3", "get_fz", &bytes(&b3), vec![], &[])), &b3.fz().to_le_bytes()), "fz");
        assert_eq!(ret::i32(&on(&mut h, "ByteFloat3", "GetHashCode", &bytes(&b3), vec![], &[])), b3.hash_code(), "ByteFloat3 hash");
        let k = (r.next() % 3) as i32;
        assert_eq!(ret::bytes(&on(&mut h, "ByteFloat3", "get_Item", &bytes(&b3), vec![arg::i32(k)], &[]))[0], b3.get(k).unwrap(), "ByteFloat3[{k}]");
        assert!(same_words(&ret::bytes(&on(&mut h, "ByteFloat3", "get_fx", &bytes(&b3), vec![], &[])), &b3.fx().to_le_bytes()), "fx");
        let mut o = b3;
        o.set(k, byte);
        let after = on(&mut h, "ByteFloat3", "set_Item", &bytes(&b3), vec![arg::i32(k), arg::raw(&[byte])], &[]);
        assert_eq!(ret::hex_field(&after["this_after"]), bytes(&o), "ByteFloat3[{k}] = {byte}");
        let eq = st(&mut h, "ByteFloat3.op_Equality", vec![arg::raw(&bytes(&b3)), arg::raw(&bytes(&o))], &[])[0] != 0;
        assert_eq!(eq, b3 == o, "ByteFloat3 ==");
    }
}

#[test]
fn sbyte_float3_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(63);
    const T: &str = "SbyteFloat3";
    for _ in 0..1500 {
        let (a, b, c) = (r.f() * 0.6, r.f() * 0.6, r.f() * 0.6);
        let s = SbyteFloat3::new(a, b, c);
        let after = on(&mut h, T, ".ctor", &[0, 0, 0], vec![arg::f32(a), arg::f32(b), arg::f32(c)], &["System.Single", "System.Single", "System.Single"]);
        assert_eq!(ret::hex_field(&after["this_after"]), bytes(&s), "SbyteFloat3({a}, {b}, {c})");
        assert_eq!(st(&mut h, "SbyteFloat3.op_Explicit", vec![arg::raw(&bytes(&v3(a, b, c)))], &["UnityEngine.Vector3"]), bytes(&s), "SbyteFloat3(Vector3)");
        let (x, y, z) = (r.next() as i32 >> (r.next() % 24), r.next() as i32 >> (r.next() % 24), r.next() as i32 >> (r.next() % 24));
        let si = SbyteFloat3::from_ints(x, y, z);
        let after = on(&mut h, T, ".ctor", &[0, 0, 0], vec![arg::i32(x), arg::i32(y), arg::i32(z)], &["System.Int32", "System.Int32", "System.Int32"]);
        assert_eq!(ret::hex_field(&after["this_after"]), bytes(&si), "SbyteFloat3({x}, {y}, {z})");
        assert_eq!(st(&mut h, "SbyteFloat3.op_Explicit", vec![arg::raw(&bytes(&Int3 { x, y, z }))], &["Unity.Mathematics.int3"]), bytes(&si), "SbyteFloat3(int3)");
        // (the op_Explicit overloads from SbyteFloat3 differ only by return type: covered by the getters)
        assert!(same_words(&ret::bytes(&on(&mut h, T, "get_normal", &bytes(&si), vec![], &[])), &bytes(&si.normal())), "{si:?}.normal");
        assert!(same_words(&ret::bytes(&on(&mut h, T, "get_fy", &bytes(&si), vec![], &[])), &si.fy().to_le_bytes()), "fy");
        assert_eq!(ret::i32(&on(&mut h, T, "GetHashCode", &bytes(&si), vec![], &[])), si.hash_code(), "{si:?} hash");
        let m = Int3x3 { c: std::array::from_fn(|_| Int3 { x: (r.next() % 7) as i32 - 3, y: (r.next() % 7) as i32 - 3, z: (r.next() % 7) as i32 - 3 }) };
        assert_eq!(st(&mut h, "SbyteFloat3.op_Multiply", vec![arg::raw(&bytes(&m)), arg::raw(&bytes(&si))], &[]), bytes(&SbyteFloat3::mul(&m, si)), "{m:?} * {si:?}");
        let k = (r.next() % 3) as i32;
        assert_eq!(ret::bytes(&on(&mut h, T, "get_Item", &bytes(&si), vec![arg::i32(k)], &[]))[0] as i8, si.get(k).unwrap(), "SbyteFloat3[{k}]");
        assert_eq!(ret::string(&on(&mut h, T, "ToString", &bytes(&si), vec![], &[])), si.to_string_cs(), "{si:?}.ToString");
    }
}
