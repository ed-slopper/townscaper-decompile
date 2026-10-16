//! `ts_game::os::extra_math` against the original game (E1b). Needs `TS_HARNESS=1`.

use serde_json::Value;
use ts_check::{arg, harness, ret, Harness};
use ts_game::os::extra_math as em;
use ts_game::unity::geometry::rgba;
use ts_game::unity::mathematics::{f2, f3, i3, Int3x2};
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
            0 => [0.0, -0.0, 1.0, -1.0, 0.5, 90.0, 180.0, 1e-7, f32::NAN, 1e20][(self.next() % 10) as usize],
            1 => (self.next() % 9) as f32 - 4.0,
            _ => (self.next() as f32 / u32::MAX as f32) * 8.0 - 4.0,
        }
    }
    fn i(&mut self) -> i32 {
        if self.next() % 6 == 0 { self.next() as i32 } else { (self.next() % 21) as i32 - 10 }
    }
    fn bounds(&mut self) -> Int3x2 {
        let (a, b) = (i3(self.i(), self.i(), self.i()), i3(self.i(), self.i(), self.i()));
        Int3x2 { c0: i3(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)), c1: i3(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)) }
    }
}

fn a<T: Copy>(v: &T) -> Value {
    arg::raw(&bytes(v))
}

fn call(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> Vec<u8> {
    ret::bytes(&h.call(&format!("Os.Utils.ExtraMath.{m}"), args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}")))
}

macro_rules! eq_words {
    ($g:expr, $o:expr, $what:expr) => {{
        let o = $o;
        assert!(same_words(&$g, &bytes(&o)), "{}: ours {:?} game bytes {:?}", $what, o, $g);
    }};
}

const V2: &str = "UnityEngine.Vector2";
const V3: &str = "UnityEngine.Vector3";
const F2: &str = "Unity.Mathematics.float2";
const F3: &str = "Unity.Mathematics.float3";
const I3: &str = "Unity.Mathematics.int3";
const B: &str = "Unity.Mathematics.int3x2";
const F: &str = "System.Single";

#[test]
fn extra_math_matches_the_game() {
    let Some(mut h) = harness() else { return };
    // toneScale, read back through ToneOffsetToPitch(1)
    assert!(same_words(&call(&mut h, "ToneOffsetToPitch", vec![arg::f32(1.0)], &[]), &em::tone_scale().to_le_bytes()), "toneScale");
    let mut r = Rng(71);
    for _ in 0..1200 {
        let (x, y, z, w) = (r.f(), r.f(), r.f(), r.f());
        let (p, q) = (v2(x, y), v2(z, w));
        let (fv, gv) = (f2(x, y), f2(z, w));
        let p3 = v3(x, y, z);
        let ff3 = f3(x, y, z);
        eq_words!(call(&mut h, "ToneOffsetToPitch", vec![arg::f32(x)], &[]), em::tone_offset_to_pitch(x), format!("ToneOffsetToPitch({x})"));
        eq_words!(call(&mut h, "SetW", vec![a(&p3), arg::f32(w)], &[V3, F]), em::set_w(p3, w), "SetW(Vector3)");
        eq_words!(call(&mut h, "SetW", vec![a(&ff3), arg::f32(w)], &[F3, F]), em::set_w_f3(ff3, w), "SetW(float3)");
        eq_words!(call(&mut h, "SetA", vec![a(&rgba(x, y, z, w)), arg::f32(x)], &[]), em::set_a(rgba(x, y, z, w), x), "SetA");
        eq_words!(call(&mut h, "SetX", vec![a(&ff3), arg::f32(w)], &[F3, F]), em::set_x_f3(ff3, w), "SetX(float3)");
        eq_words!(call(&mut h, "SetY", vec![a(&ff3), arg::f32(w)], &[F3, F]), em::set_y_f3(ff3, w), "SetY(float3)");
        eq_words!(call(&mut h, "SetZ", vec![a(&ff3), arg::f32(w)], &[]), em::set_z_f3(ff3, w), "SetZ");
        eq_words!(call(&mut h, "SetX", vec![a(&p), arg::f32(w)], &[V2, F]), em::set_x(p, w), "SetX(Vector2)");
        eq_words!(call(&mut h, "SetY", vec![a(&p), arg::f32(w)], &[V2, F]), em::set_y(p, w), "SetY(Vector2)");
        eq_words!(call(&mut h, "XZtoXYZ", vec![a(&p), arg::f32(w)], &[]), em::xz_to_xyz(p, w), "XZtoXYZ");
        eq_words!(call(&mut h, "GetXZ", vec![a(&ff3)], &[]), em::get_xz(ff3), "GetXZ");
        eq_words!(call(&mut h, "GetXYZWfromXZ", vec![a(&fv), arg::f32(z), arg::f32(w)], &[]), em::get_xyzw_from_xz(fv, z, w), "GetXYZWfromXZ");
        eq_words!(call(&mut h, "Rotate90Y", vec![a(&p3)], &[]), em::rotate90_y(p3), "Rotate90Y");
        eq_words!(call(&mut h, "Rotate2D90", vec![a(&fv)], &[]), em::rotate2d_90(fv), "Rotate2D90");
        eq_words!(call(&mut h, "Rotate2D180", vec![a(&fv)], &[]), em::rotate2d_180(fv), "Rotate2D180");
        eq_words!(call(&mut h, "Rotate2D270", vec![a(&fv)], &[]), em::rotate2d_270(fv), "Rotate2D270");
        // trig: the game's angles are small; huge ones reduce differently in the game's libm (docs/notes/unity.md)
        let ang = if z.is_finite() && z.abs() <= 1e4 { z } else { 0.25 };
        eq_words!(call(&mut h, "GetRotate2DFactors", vec![arg::f32(ang)], &[]), em::get_rotate2d_factors(ang), format!("GetRotate2DFactors({ang})"));
        eq_words!(call(&mut h, "ApplyRotate2DFactors", vec![a(&fv), a(&gv)], &[]), em::apply_rotate2d_factors(fv, gv), "ApplyRotate2DFactors");
        let deg = if w.is_finite() && w.abs() <= 1e3 { w * 50.0 } else { 33.0 };
        eq_words!(call(&mut h, "Rotate2D", vec![a(&fv), arg::f32(deg)], &[]), em::rotate2d(fv, deg), format!("Rotate2D({fv:?}, {deg})"));
        eq_words!(call(&mut h, "Clamp01", vec![a(&p)], &[V2]), em::clamp01(p), "Clamp01(Vector2)");
        eq_words!(call(&mut h, "Cross2", vec![a(&p), a(&q)], &[]), em::cross2(p, q), "Cross2");
        eq_words!(call(&mut h, "Pow2", vec![arg::f32(x)], &[F]), em::pow2(x), "Pow2(float)");
        let (ia, ib) = (r.i(), r.next() % 12);
        assert_eq!(ret::i32(&h.call("Os.Utils.ExtraMath.Pow2", vec![arg::i32(ia)], &["System.Int32"], false).unwrap()), em::pow2_i32(ia), "Pow2({ia})");
        assert_eq!(ret::i32(&h.call("Os.Utils.ExtraMath.Pow", vec![arg::i32(ia), arg::raw(&ib.to_le_bytes())], &[], false).unwrap()), em::pow_i32(ia, ib), "Pow({ia}, {ib})");
        eq_words!(call(&mut h, "InverseLerpUnclamped", vec![arg::f32(x), arg::f32(y), arg::f32(z)], &[]), em::inverse_lerp_unclamped(x, y, z), "InverseLerpUnclamped");
        eq_words!(call(&mut h, "Encapsulate", vec![a(&p), arg::f32(z)], &[V2, F]), em::encapsulate_range(p, z), "Encapsulate(Vector2, float)");
        let big = v3(x * 3.0, y * 3.0, z * 3.0);
        eq_words!(call(&mut h, "GetClampedMagnitudeOne", vec![a(&big)], &[]), em::clamped_magnitude_one(big), "GetClampedMagnitudeOne");
        eq_words!(call(&mut h, "GetClampedMagnitude", vec![a(&big), arg::f32(w.abs())], &[]), em::clamped_magnitude(big, w.abs()), "GetClampedMagnitude");
        let (t0, t1, t2) = (v3(r.f(), r.f(), r.f()), v3(r.f(), r.f(), r.f()), v3(r.f(), r.f(), r.f()));
        eq_words!(call(&mut h, "GetBarycentric", vec![a(&p3), a(&t0), a(&t1), a(&t2)], &[]), em::barycentric(p3, t0, t1, t2), "GetBarycentric");
        eq_words!(call(&mut h, "ClosestPointOnTriangle", vec![a(&p3), a(&t0), a(&t1), a(&t2)], &[]), em::closest_point_on_triangle(p3, t0, t1, t2), "ClosestPointOnTriangle");
        // a quad that is usually convex, and points in and around it
        let quad = [v2(0.0, 0.0), v2(1.0 + x * 0.1, y * 0.1), v2(1.0 + z * 0.1, 1.0 + w * 0.1), v2(x * 0.1, 1.0 + z * 0.1)];
        let pt = v2(x * 0.4 + 0.5, w * 0.4 + 0.5);
        eq_words!(call(&mut h, "InvBilinear", vec![a(&quad[0]), a(&quad[1]), a(&quad[2]), a(&quad[3]), a(&pt)], &[]), em::inv_bilinear(quad[0], quad[1], quad[2], quad[3], pt), format!("InvBilinear({quad:?}, {pt:?})"));
        // integer bounds
        let (b0, b1) = (r.bounds(), r.bounds());
        let pi = i3(r.i(), r.i(), r.i());
        assert_eq!(call(&mut h, "Center", vec![a(&b0)], &[]), bytes(&em::center(b0)), "Center({b0:?})");
        assert_eq!(call(&mut h, "ClosestPoint", vec![a(&b0), a(&pi)], &[]), bytes(&em::closest_point(b0, pi)), "ClosestPoint");
        assert_eq!(call(&mut h, "Contains", vec![a(&b0), a(&b1)], &[B, B])[0] != 0, em::contains_bounds(b0, b1), "Contains(bounds)");
        assert_eq!(call(&mut h, "Contains", vec![a(&b0), a(&pi)], &[B, I3])[0] != 0, em::contains(b0, pi), "Contains(int3)");
        assert_eq!(call(&mut h, "Contains", vec![a(&b0), a(&ff3)], &[B, F3])[0] != 0, em::contains_f(b0, ff3), "Contains(float3)");
        assert_eq!(call(&mut h, "Encapsulate", vec![a(&b0), a(&pi)], &[B, I3]), bytes(&em::encapsulate(b0, pi)), "Encapsulate(int3)");
        assert_eq!(call(&mut h, "Encapsulate", vec![a(&b0), a(&b1)], &[B, B]), bytes(&em::encapsulate_bounds(b0, b1)), "Encapsulate(bounds)");
        assert_eq!(call(&mut h, "Intersects", vec![a(&b0), a(&b1)], &[])[0] != 0, em::intersects(b0, b1), "Intersects");
        // points on and near the faces
        let pe = if r.next() % 2 == 0 { i3(b0.c0.x, pi.y.clamp(b0.c0.y, b0.c1.y), pi.z) } else { pi };
        assert_eq!(call(&mut h, "OnEdge", vec![a(&b0), a(&pe)], &[])[0] != 0, em::on_edge(b0, pe), "OnEdge({b0:?}, {pe:?})");
    }
}
