//! `ts_game::unity::{vector, quaternion, geometry}` against the original game (F11b). Needs `TS_HARNESS=1`.
//! Values go over as their bytes (Unity's layouts); results are compared as f32 bit patterns (NaN == NaN).

use serde_json::Value;
use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::geometry::*;
use ts_game::unity::quaternion::*;
use ts_game::unity::vector::*;

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}

fn from_bytes<T: Copy>(b: &[u8]) -> T {
    assert!(b.len() >= std::mem::size_of::<T>(), "reply too short: {} bytes", b.len());
    unsafe { std::ptr::read_unaligned(b.as_ptr() as *const T) }
}

/// The game's bytes as the type of `ours`.
fn like<T: Copy>(_ours: &T, g: &[u8]) -> T {
    from_bytes(g)
}

/// Same f32 words (NaN equals NaN; -0 and +0 differ).
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
        const SPECIAL: [f32; 12] = [0.0, -0.0, 1.0, -1.0, 0.5, 1e-6, -1e-6, 1e-20, 1e20, 360.0, 0.70710677, f32::NAN];
        match self.next() % 10 {
            0 => SPECIAL[(self.next() % 12) as usize],
            1 => (self.next() % 2001) as f32 - 1000.0,
            2..=5 => (self.next() as f32 / u32::MAX as f32) * 2.0 - 1.0,
            _ => (self.next() as f32 / u32::MAX as f32) * 200.0 - 100.0,
        }
    }
    fn v2(&mut self) -> Vector2 {
        v2(self.f(), self.f())
    }
    fn v3(&mut self) -> Vector3 {
        match self.next() % 8 {
            0 => [Vector3::UP, Vector3::DOWN, Vector3::FORWARD, Vector3::RIGHT, Vector3::ZERO, v3(0.0, 0.0, -1.0)][(self.next() % 6) as usize],
            1 => Vector3::UP.mul(self.f()),
            _ => v3(self.f(), self.f(), self.f()),
        }
    }
    fn v4(&mut self) -> Vector4 {
        v4(self.f(), self.f(), self.f(), self.f())
    }
    fn q(&mut self) -> Quaternion {
        match self.next() % 4 {
            0 => quat(self.f(), self.f(), self.f(), self.f()),
            _ => {
                // a unit quaternion
                let (a, b, c, d) = (self.f(), self.f(), self.f(), self.f());
                let n = ((a * a + b * b + c * c + d * d) as f64).sqrt() as f32;
                if n > 1e-3 && n.is_finite() { quat(a / n, b / n, c / n, d / n) } else { Quaternion::IDENTITY }
            }
        }
    }
    fn m(&mut self) -> Matrix4x4 {
        match self.next() % 3 {
            0 => Matrix4x4 { m: std::array::from_fn(|_| self.f()) },
            _ => Matrix4x4::trs(self.v3(), self.q(), v3(self.f(), self.f(), self.f())),
        }
    }
}

const N: usize = 1500;
const V2: &str = "UnityEngine.Vector2";
const V3: &str = "UnityEngine.Vector3";
const V4: &str = "UnityEngine.Vector4";
const Q: &str = "UnityEngine.Quaternion";
const M: &str = "UnityEngine.Matrix4x4";
const F: &str = "System.Single";

fn a<T: Copy>(v: &T) -> Value {
    arg::raw(&bytes(v))
}

fn st(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> Value {
    h.call(m, args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

fn on(h: &mut Harness, m: &str, class: &str, this: &[u8], args: Vec<Value>, types: &[&str]) -> Value {
    h.call_on_value(m, class, this, args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

/// Assert the reply's raw value equals ours (as f32 words).
macro_rules! check {
    ($reply:expr, $ours:expr, $what:expr) => {{
        let g = ret::bytes(&$reply);
        let o = bytes(&$ours);
        let ours = $ours;
        assert!(same_words(&g, &o), "{}: ours {:?} game {:?}", $what, ours, like(&ours, &g));
    }};
}

fn check_f32(reply: &Value, ours: f32, what: String) {
    let g = ret::f32(reply);
    assert!(g.to_bits() == ours.to_bits() || (g.is_nan() && ours.is_nan()), "{what}: ours {ours:?} game {g:?}");
}

#[test]
fn vector2_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(31);
    for _ in 0..N {
        let (x, y, t) = (r.v2(), r.v2(), r.f());
        check!(st(&mut h, "UnityEngine.Vector2.Lerp", vec![a(&x), a(&y), arg::f32(t)], &[]), Vector2::lerp(x, y, t), format!("Lerp({x:?}, {y:?}, {t})"));
        check!(st(&mut h, "UnityEngine.Vector2.Max", vec![a(&x), a(&y)], &[]), Vector2::max(x, y), format!("Max({x:?}, {y:?})"));
        check_f32(&st(&mut h, "UnityEngine.Vector2.SignedAngle", vec![a(&x), a(&y)], &[]), Vector2::signed_angle(x, y), format!("SignedAngle({x:?}, {y:?})"));
        check_f32(&on(&mut h, "UnityEngine.Vector2.get_magnitude", V2, &bytes(&x), vec![], &[]), x.magnitude(), format!("{x:?}.magnitude"));
        check_f32(&on(&mut h, "UnityEngine.Vector2.get_sqrMagnitude", V2, &bytes(&x), vec![], &[]), x.sqr_magnitude(), format!("{x:?}.sqrMagnitude"));
        let g: Vector2 = from_bytes(&ret::bytes(&on(&mut h, "UnityEngine.Vector2.get_normalized", V2, &bytes(&x), vec![], &[])));
        assert!(same_words(&bytes(&g), &bytes(&x.normalized())), "{x:?}.normalized: ours {:?} game {g:?}", x.normalized());
        assert_eq!(ret::i32(&on(&mut h, "UnityEngine.Vector2.GetHashCode", V2, &bytes(&x), vec![], &[])), x.hash_code(), "{x:?}.GetHashCode");
    }
    let mut r = Rng(32);
    for _ in 0..200 {
        let v = Vector2Int { x: r.next() as i32 >> (r.next() % 31), y: r.next() as i32 >> (r.next() % 31) };
        check!(st(&mut h, "UnityEngine.Vector2Int.op_Implicit", vec![a(&v)], &["UnityEngine.Vector2Int"]), v.to_vector2(), format!("{v:?} to Vector2"));
    }
}

#[test]
fn vector3_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(33);
    for _ in 0..N {
        let (x, y, t) = (r.v3(), r.v3(), r.f());
        check!(st(&mut h, "UnityEngine.Vector3.Lerp", vec![a(&x), a(&y), arg::f32(t)], &[]), Vector3::lerp(x, y, t), format!("Lerp({x:?}, {y:?}, {t})"));
        check!(st(&mut h, "UnityEngine.Vector3.Min", vec![a(&x), a(&y)], &[]), Vector3::min(x, y), format!("Min({x:?}, {y:?})"));
        check!(st(&mut h, "UnityEngine.Vector3.Max", vec![a(&x), a(&y)], &[]), Vector3::max(x, y), format!("Max({x:?}, {y:?})"));
        check!(st(&mut h, "UnityEngine.Vector3.Cross", vec![a(&x), a(&y)], &[]), Vector3::cross(x, y), format!("Cross({x:?}, {y:?})"));
        check!(st(&mut h, "UnityEngine.Vector3.MoveTowards", vec![a(&x), a(&y), arg::f32(t)], &[]), Vector3::move_towards(x, y, t), format!("MoveTowards({x:?}, {y:?}, {t})"));
        check!(st(&mut h, "UnityEngine.Vector3.Normalize", vec![a(&x)], &[V3]), Vector3::normalize_of(x), format!("Normalize({x:?})"));
        check_f32(&st(&mut h, "UnityEngine.Vector3.Angle", vec![a(&x), a(&y)], &[]), Vector3::angle(x, y), format!("Angle({x:?}, {y:?})"));
        check_f32(&st(&mut h, "UnityEngine.Vector3.Distance", vec![a(&x), a(&y)], &[]), Vector3::distance(x, y), format!("Distance({x:?}, {y:?})"));
        let ne = ret::bytes(&st(&mut h, "UnityEngine.Vector3.op_Inequality", vec![a(&x), a(&y)], &[]))[0] != 0;
        assert_eq!(ne, !Vector3::approx_eq(x, y), "{x:?} != {y:?}");
        check_f32(&on(&mut h, "UnityEngine.Vector3.get_magnitude", V3, &bytes(&x), vec![], &[]), x.magnitude(), format!("{x:?}.magnitude"));
        check_f32(&on(&mut h, "UnityEngine.Vector3.get_sqrMagnitude", V3, &bytes(&x), vec![], &[]), x.sqr_magnitude(), format!("{x:?}.sqrMagnitude"));
        check!(on(&mut h, "UnityEngine.Vector3.get_normalized", V3, &bytes(&x), vec![], &[]), x.normalized(), format!("{x:?}.normalized"));
        assert_eq!(ret::i32(&on(&mut h, "UnityEngine.Vector3.GetHashCode", V3, &bytes(&x), vec![], &[])), x.hash_code(), "{x:?}.GetHashCode");
        let after = on(&mut h, "UnityEngine.Vector3.Normalize", V3, &bytes(&x), vec![], &[]);
        let mut ours = x;
        ours.normalize();
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&ours)), "{x:?}.Normalize()");
    }
}

#[test]
fn vector4_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(34);
    for _ in 0..N {
        let (x, y, t) = (r.v4(), r.v4(), r.f());
        check!(st(&mut h, "UnityEngine.Vector4.Lerp", vec![a(&x), a(&y), arg::f32(t)], &[]), Vector4::lerp(x, y, t), format!("Lerp({x:?}, {y:?}, {t})"));
        check!(on(&mut h, "UnityEngine.Vector4.get_normalized", V4, &bytes(&x), vec![], &[]), x.normalized(), format!("{x:?}.normalized"));
        check_f32(&on(&mut h, "UnityEngine.Vector4.get_sqrMagnitude", V4, &bytes(&x), vec![], &[]), x.sqr_magnitude(), format!("{x:?}.sqrMagnitude"));
        assert_eq!(ret::i32(&on(&mut h, "UnityEngine.Vector4.GetHashCode", V4, &bytes(&x), vec![], &[])), x.hash_code(), "{x:?}.GetHashCode");
        let (p, q) = (r.v2(), r.v3());
        check!(st(&mut h, "UnityEngine.Vector4.op_Implicit", vec![a(&p)], &[V2]), Vector4::from_v2(p), "Vector4(Vector2)");
        check!(st(&mut h, "UnityEngine.Vector4.op_Implicit", vec![a(&q)], &[V3]), Vector4::from_v3(q), "Vector4(Vector3)");
        check!(st(&mut h, "UnityEngine.Vector4.op_Implicit", vec![a(&x)], &[V4]), x.xyz(), "Vector3(Vector4)");
    }
}

#[test]
fn quaternion_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(35);
    for _ in 0..N {
        let (p, q, v, t) = (r.q(), r.q(), r.v3(), r.f());
        check!(st(&mut h, "UnityEngine.Quaternion.op_Multiply", vec![a(&p), a(&q)], &[Q, Q]), Quaternion::mul(p, q), format!("{p:?} * {q:?}"));
        check!(st(&mut h, "UnityEngine.Quaternion.op_Multiply", vec![a(&p), a(&v)], &[Q, V3]), Quaternion::rotate(p, v), format!("{p:?} * {v:?}"));
        check!(st(&mut h, "UnityEngine.Quaternion.Inverse", vec![a(&p)], &[]), Quaternion::inverse(p), format!("Inverse({p:?})"));
        check!(st(&mut h, "UnityEngine.Quaternion.Lerp", vec![a(&p), a(&q), arg::f32(t)], &[]), Quaternion::lerp(p, q, t), format!("Lerp({p:?}, {q:?}, {t})"));
        let (f, u) = (r.v3(), r.v3());
        check!(st(&mut h, "UnityEngine.Quaternion.LookRotation", vec![a(&f)], &[V3]), Quaternion::look_rotation(f), format!("LookRotation({f:?})"));
        check!(st(&mut h, "UnityEngine.Quaternion.LookRotation", vec![a(&f), a(&u)], &[V3, V3]), Quaternion::look_rotation_up(f, u), format!("LookRotation({f:?}, {u:?})"));
    }
}

#[test]
fn matrix4x4_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(36);
    for _ in 0..N {
        let (m, n, p, q, s) = (r.m(), r.m(), r.v3(), r.q(), r.v3());
        check!(st(&mut h, "UnityEngine.Matrix4x4.op_Multiply", vec![a(&m), a(&n)], &[M, M]), Matrix4x4::mul(&m, &n), format!("{m:?} * {n:?}"));
        check!(on(&mut h, "UnityEngine.Matrix4x4.MultiplyPoint", M, &bytes(&m), vec![a(&p)], &[]), m.multiply_point(p), format!("{m:?}.MultiplyPoint({p:?})"));
        check!(on(&mut h, "UnityEngine.Matrix4x4.MultiplyVector", M, &bytes(&m), vec![a(&p)], &[]), m.multiply_vector(p), format!("{m:?}.MultiplyVector({p:?})"));
        check!(st(&mut h, "UnityEngine.Matrix4x4.Scale", vec![a(&p)], &[]), Matrix4x4::scale(p), "Scale");
        check!(st(&mut h, "UnityEngine.Matrix4x4.Translate", vec![a(&p)], &[]), Matrix4x4::translate(p), "Translate");
        check!(st(&mut h, "UnityEngine.Matrix4x4.Rotate", vec![a(&q)], &[]), Matrix4x4::rotate(q), format!("Rotate({q:?})"));
        check!(st(&mut h, "UnityEngine.Matrix4x4.TRS", vec![a(&p), a(&q), a(&s)], &[]), Matrix4x4::trs(p, q, s), format!("TRS({p:?}, {q:?}, {s:?})"));
        check!(on(&mut h, "UnityEngine.Matrix4x4.get_inverse", M, &bytes(&m), vec![], &[]), m.inverse(), format!("{m:?}.inverse"));
        check!(on(&mut h, "UnityEngine.Matrix4x4.get_lossyScale", M, &bytes(&m), vec![], &[]), m.lossy_scale(), format!("{m:?}.lossyScale"));
        let (from, to, up) = (r.v3(), r.v3(), r.v3());
        // skip the inputs where the engine returns uninitialised memory
        if look_rotation_matrix(to.sub(from), up) != LookMatrix::Unwritten {
            check!(st(&mut h, "UnityEngine.Matrix4x4.LookAt", vec![a(&from), a(&to), a(&up)], &[]), Matrix4x4::look_at(from, to, up), format!("LookAt({from:?}, {to:?}, {up:?})"));
        }
        let i = (r.next() % 4) as i32;
        check!(on(&mut h, "UnityEngine.Matrix4x4.GetColumn", M, &bytes(&m), vec![arg::i32(i)], &[]), m.column(i as usize), "GetColumn");
        assert_eq!(ret::i32(&on(&mut h, "UnityEngine.Matrix4x4.GetHashCode", M, &bytes(&m), vec![], &[])), m.hash_code(), "{m:?}.GetHashCode");
        let c = [r.v4(), r.v4(), r.v4(), r.v4()];
        let after = on(&mut h, "UnityEngine.Matrix4x4..ctor", M, &[0u8; 64], c.iter().map(a).collect(), &[V4, V4, V4, V4]);
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&Matrix4x4::from_columns(c[0], c[1], c[2], c[3]))), "Matrix4x4(columns)");
        let mut ours = m;
        ours.set_column(i as usize, c[0]);
        let after = on(&mut h, "UnityEngine.Matrix4x4.SetColumn", M, &bytes(&m), vec![arg::i32(i), a(&c[0])], &[]);
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&ours)), "SetColumn");
    }
}

#[test]
fn geometry_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(37);
    for _ in 0..N {
        let (c, s, p) = (r.v3(), r.v3(), r.v3());
        let b = Bounds::new(c, s);
        let after = on(&mut h, "UnityEngine.Bounds..ctor", "UnityEngine.Bounds", &[0u8; 24], vec![a(&c), a(&s)], &[V3, V3]);
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&b)), "Bounds({c:?}, {s:?})");
        check!(on(&mut h, "UnityEngine.Bounds.get_size", "UnityEngine.Bounds", &bytes(&b), vec![], &[]), b.size(), "Bounds.size");
        check!(on(&mut h, "UnityEngine.Bounds.get_min", "UnityEngine.Bounds", &bytes(&b), vec![], &[]), b.min(), "Bounds.min");
        check!(on(&mut h, "UnityEngine.Bounds.get_max", "UnityEngine.Bounds", &bytes(&b), vec![], &[]), b.max(), "Bounds.max");
        let mut e = b;
        e.encapsulate(p);
        let after = on(&mut h, "UnityEngine.Bounds.Encapsulate", "UnityEngine.Bounds", &bytes(&b), vec![a(&p)], &[V3]);
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&e)), "{b:?}.Encapsulate({p:?})");
        let b2 = Bounds::new(r.v3(), r.v3());
        let hit = ret::bytes(&on(&mut h, "UnityEngine.Bounds.Intersects", "UnityEngine.Bounds", &bytes(&b), vec![a(&b2)], &[]))[0] != 0;
        assert_eq!(hit, b.intersects(&b2), "{b:?}.Intersects({b2:?})");

        let rc = Rect { x: r.f(), y: r.f(), width: r.f(), height: r.f() };
        let v = r.v2();
        const R: &str = "UnityEngine.Rect";
        check!(on(&mut h, "UnityEngine.Rect.get_center", R, &bytes(&rc), vec![], &[]), rc.center(), "Rect.center");
        check!(on(&mut h, "UnityEngine.Rect.get_max", R, &bytes(&rc), vec![], &[]), rc.max(), "Rect.max");
        check!(on(&mut h, "UnityEngine.Rect.get_size", R, &bytes(&rc), vec![], &[]), rc.size(), "Rect.size");
        for (m, f) in [("set_center", Rect::set_center as fn(&mut Rect, Vector2)), ("set_max", Rect::set_max), ("set_min", Rect::set_min)] {
            let mut o = rc;
            f(&mut o, v);
            let after = on(&mut h, &format!("UnityEngine.Rect.{m}"), R, &bytes(&rc), vec![a(&v)], &[]);
            assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&o)), "{rc:?}.{m}({v:?})");
        }
        let rc2 = if r.next() % 3 == 0 { rc } else { Rect { x: r.f(), ..rc } };
        let ne = ret::bytes(&st(&mut h, "UnityEngine.Rect.op_Inequality", vec![a(&rc), a(&rc2)], &[]))[0] != 0;
        assert_eq!(ne, !rc.eq(&rc2), "Rect !=");

        let (n, d) = (r.v3(), r.f());
        const P: &str = "UnityEngine.Plane";
        let after = on(&mut h, "UnityEngine.Plane..ctor", P, &[0u8; 16], vec![a(&n), a(&p)], &[V3, V3]);
        let pl = Plane::from_point(n, p);
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&pl)), "Plane({n:?}, {p:?})");
        let after = on(&mut h, "UnityEngine.Plane..ctor", P, &[0u8; 16], vec![a(&n), arg::f32(d)], &[V3, F]);
        assert!(same_words(&ret::hex_field(&after["this_after"]), &bytes(&Plane::new(n, d))), "Plane({n:?}, {d})");
        check_f32(&on(&mut h, "UnityEngine.Plane.GetDistanceToPoint", P, &bytes(&pl), vec![a(&c)], &[]), pl.distance_to_point(c), format!("{pl:?}.GetDistanceToPoint({c:?})"));
        let ray = Ray { origin: c, direction: s };
        let rr = on(&mut h, "UnityEngine.Plane.Raycast", P, &bytes(&pl), vec![a(&ray), arg::f32(0.0)], &[]);
        let (hit, enter) = pl.raycast(&ray);
        assert_eq!(ret::bytes(&rr)[0] != 0, hit, "{pl:?}.Raycast({ray:?}) hit");
        assert!(same_words(&ret::hex_field(&rr["args_after"][1]), &enter.to_le_bytes()), "{pl:?}.Raycast({ray:?}) enter {enter}");
        check!(on(&mut h, "UnityEngine.Ray.GetPoint", "UnityEngine.Ray", &bytes(&ray), vec![arg::f32(d)], &[]), ray.point(d), "Ray.GetPoint");

        let (ca, cb, t) = (rgba(r.f(), r.f(), r.f(), r.f()), rgba(r.f(), r.f(), r.f(), r.f()), r.f());
        check!(st(&mut h, "UnityEngine.Color.Lerp", vec![a(&ca), a(&cb), arg::f32(t)], &[]), Color::lerp(ca, cb, t), format!("Color.Lerp({ca:?}, {cb:?}, {t})"));
        let (hh, ss, vv) = (r.f().abs().fract(), r.f().abs().fract(), r.f().abs());
        check!(st(&mut h, "UnityEngine.Color.HSVToRGB", vec![arg::f32(hh), arg::f32(ss), arg::f32(vv)], &[F, F, F]), Color::hsv_to_rgb(hh, ss, vv), format!("HSVToRGB({hh}, {ss}, {vv})"));
        let g = ret::bytes(&st(&mut h, "UnityEngine.Color32.op_Implicit", vec![a(&ca)], &["UnityEngine.Color"]));
        assert_eq!(&g[..4], &bytes(&Color32::from_color(ca))[..], "Color32({ca:?})");
        let c32 = Color32 { r: r.next() as u8, g: r.next() as u8, b: r.next() as u8, a: r.next() as u8 };
        check!(st(&mut h, "UnityEngine.Color32.op_Implicit", vec![a(&c32)], &["UnityEngine.Color32"]), c32.to_color(), "Color(Color32)");
    }
    for (name, c) in [("red", Color::RED), ("green", Color::GREEN), ("blue", Color::BLUE), ("black", Color::BLACK), ("gray", Color::GRAY), ("yellow", Color::YELLOW)] {
        check!(st(&mut h, &format!("UnityEngine.Color.get_{name}"), vec![], &[]), c, format!("Color.{name}"));
    }
}
