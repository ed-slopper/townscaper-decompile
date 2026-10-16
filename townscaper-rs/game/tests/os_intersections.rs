//! `ts_game::os::intersections` against the original game (E1c). Needs `TS_HARNESS=1`.
//! Shapes are placed close together so both answers occur often; touching and degenerate cases included.

use serde_json::Value;
use ts_check::{arg, harness, ret, Harness};
use ts_game::os::intersections as ix;
use ts_game::unity::geometry::Ray;
use ts_game::unity::quaternion::{quat, Matrix4x4, Quaternion};
use ts_game::unity::vector::{v3, Vector3};

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self, scale: f32) -> f32 {
        if self.next() % 10 == 0 {
            return [0.0, 1.0, -1.0, 0.5][(self.next() % 4) as usize] * scale;
        }
        ((self.next() as f32 / u32::MAX as f32) * 2.0 - 1.0) * scale
    }
    fn v(&mut self, scale: f32) -> Vector3 {
        v3(self.f(scale), self.f(scale), self.f(scale))
    }
    fn q(&mut self) -> Quaternion {
        if self.next() % 4 == 0 {
            return Quaternion::IDENTITY;
        }
        let (a, b, c, d) = (self.f(1.0), self.f(1.0), self.f(1.0), self.f(1.0));
        let n = ((a * a + b * b + c * c + d * d) as f64).sqrt() as f32;
        if n > 1e-3 { quat(a / n, b / n, c / n, d / n) } else { Quaternion::IDENTITY }
    }
    fn bx(&mut self) -> Matrix4x4 {
        let s = v3(self.f(1.0).abs() + 0.05, self.f(1.0).abs() + 0.05, self.f(1.0).abs() + 0.05);
        Matrix4x4::trs(self.v(2.0), self.q(), s)
    }
}

fn a<T: Copy>(v: &T) -> Value {
    arg::raw(&bytes(v))
}

fn yes(h: &mut Harness, m: &str, args: Vec<Value>) -> bool {
    ret::bytes(&h.call(&format!("Os.Utils.Intersections.{m}"), args, &[], false).unwrap_or_else(|e| panic!("{m}: {e:?}")))[0] != 0
}

#[test]
fn intersections_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(81);
    let mut hits = [0usize; 5];
    for _ in 0..1500 {
        // BoxBox
        let (p, q, s, t) = (r.v(2.0), r.v(2.0), r.v(2.0), r.v(2.0));
        let (min0, max0) = (Vector3::min(p, q), Vector3::max(p, q));
        let (min1, max1) = (Vector3::min(s, t), Vector3::max(s, t));
        let o = ix::box_box(min0, max0, min1, max1);
        assert_eq!(yes(&mut h, "BoxBox", vec![a(&min0), a(&max0), a(&min1), a(&max1)]), o, "BoxBox");
        hits[0] += o as usize;
        // OrientedBoxOrientedBox
        let (b0, b1) = (r.bx(), r.bx());
        let o = ix::oriented_box_oriented_box(&b0, &b1);
        assert_eq!(yes(&mut h, "OrientedBoxOrientedBox", vec![a(&b0), a(&b1)]), o, "OrientedBoxOrientedBox({b0:?}, {b1:?})");
        hits[1] += o as usize;
        // OrientedBoxTriangle
        let (t0, t1, t2) = (r.v(2.5), r.v(2.5), r.v(2.5));
        let o = ix::oriented_box_triangle(&b0, t0, t1, t2);
        assert_eq!(yes(&mut h, "OrientedBoxTriangle", vec![a(&b0), a(&t0), a(&t1), a(&t2)]), o, "OrientedBoxTriangle({b0:?}, {t0:?}, {t1:?}, {t2:?})");
        hits[2] += o as usize;
        // TriangleRay (the Ray struct as stored: origin, direction; unit or not)
        let ray = Ray { origin: r.v(2.0), direction: if r.next() % 2 == 0 { r.v(1.0).normalized() } else { r.v(1.0) } };
        let o = ix::triangle_ray(t0, t1, t2, ray.origin, ray.direction);
        assert_eq!(yes(&mut h, "TriangleRay", vec![a(&t0), a(&t1), a(&t2), a(&ray)]), o, "TriangleRay({t0:?}, {t1:?}, {t2:?}, {ray:?})");
        hits[3] += o as usize;
        // TriangleTriangle
        let (u0, u1, u2) = (r.v(2.0), r.v(2.0), r.v(2.0));
        let md = if r.next() % 2 == 0 { 0.0 } else { r.f(0.3).abs() };
        let o = ix::triangle_triangle([t0, t1, t2], [u0, u1, u2], md);
        assert_eq!(yes(&mut h, "TriangleTriangle", vec![a(&t0), a(&t1), a(&t2), a(&u0), a(&u1), a(&u2), arg::f32(md)]), o, "TriangleTriangle");
        hits[4] += o as usize;
    }
    eprintln!("hits (of 1500): BoxBox {}, OBB-OBB {}, OBB-tri {}, tri-ray {}, tri-tri {}", hits[0], hits[1], hits[2], hits[3], hits[4]);
    assert!(hits.iter().all(|&n| n > 50 && n < 1450), "both answers should occur: {hits:?}");
}

#[test]
fn number_strings_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(82);
    for i in 0..300 {
        let n = if i < 40 { i - 20 } else { r.next() as i32 >> (r.next() % 31) };
        let g = h.call("Os.Utils.NumberStrings.Get", vec![arg::i32(n)], &[], false).unwrap();
        assert_eq!(ret::string(&g), ix::number_string(n), "NumberStrings.Get({n})");
    }
}
