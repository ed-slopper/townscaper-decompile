//! `ts_game::os::hex` against the original game (E2). Needs `TS_HARNESS=1`.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::os::hex::{self, statics, HexOnionIterator};
use ts_game::unity::mathematics::{f2, f3, i3, Float2, Float3, Int2, Int3};
use ts_game::unity::quaternion::Matrix4x4;

const F2: &str = "Unity.Mathematics.float2";
const F3: &str = "Unity.Mathematics.float3";
const I2: &str = "Unity.Mathematics.int2";
const I3: &str = "Unity.Mathematics.int3";
const IT: &str = "Os.Hex.HexOnionIterator";

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}
fn from<T: Copy + Default>(b: &[u8]) -> T {
    assert_eq!(b.len(), std::mem::size_of::<T>(), "reply size");
    let mut v = T::default();
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), &mut v as *mut T as *mut u8, b.len()) };
    v
}
fn a<T: Copy>(v: &T) -> Value {
    arg::raw(&bytes(v))
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
        match self.next() % 8 {
            // halves and integers: rounding ties and exact borders
            0 => ((self.next() % 21) as f32 - 10.0) * 0.5,
            _ => ((self.next() as f32 / u32::MAX as f32) * 2.0 - 1.0) * scale,
        }
    }
    fn f3(&mut self, scale: f32) -> Float3 {
        f3(self.f(scale), self.f(scale), self.f(scale))
    }
    fn i(&mut self) -> i32 {
        if self.next() % 8 == 0 { self.next() as i32 } else { (self.next() % 41) as i32 - 20 }
    }
}

fn st<R: Copy + Default>(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> R {
    from(&ret::bytes(&h.call(&format!("Os.Hex.HexMath.{m}"), args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}"))))
}

fn bits<T: Copy>(v: &T) -> Vec<u8> {
    bytes(v)
}

#[test]
fn hex_math_statics_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut field = |name: &str| ret::bytes(&h.request(json!({"static_field": format!("Os.Hex.HexMath.{name}"), "main": true})));
    assert_eq!(field("hexAxisLength"), bits(&statics::hex_axis_length()), "hexAxisLength");
    let f2s: [(&str, Float2); 5] = [
        ("hexAxisX", statics::hex_axis_x()),
        ("hexAxisY", statics::hex_axis_y()),
        ("hexAxisZ", statics::hex_axis_z()),
        ("skewAxisX", statics::skew_axis_x()),
        ("skewAxisY", statics::skew_axis_y()),
    ];
    for (n, v) in f2s {
        assert_eq!(field(n), bits(&v), "{n}");
    }
    let ms: [(&str, Matrix4x4); 7] = [
        ("hexToPlane", statics::hex_to_plane()),
        ("hexToWorld", statics::hex_to_world()),
        ("planeToWorld", statics::plane_to_world()),
        ("skewToPlane", statics::skew_to_plane()),
        ("planeToSkew", statics::plane_to_skew()),
        ("hexToSkew", statics::hex_to_skew()),
        ("skewToHex", statics::skew_to_hex()),
    ];
    for (n, v) in ms {
        assert_eq!(field(n), bits(&v), "{n}: ours {v:?}");
    }
}

#[test]
fn hex_math_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(91);
    for _ in 0..600 {
        let p = r.f3(6.0);
        let q = f2(r.f(6.0), r.f(6.0));
        let n = i3(r.i(), r.i(), r.i());
        let n2 = Int2 { x: r.i(), y: r.i() };
        let y = r.f(3.0);
        assert_eq!(st::<Float2>(&mut h, "GetHex2", vec![a(&p)], &[F3]), hex::get_hex2_f(p), "GetHex2({p:?})");
        assert_eq!(st::<Int2>(&mut h, "GetHex2", vec![a(&n)], &[I3]), hex::get_hex2(n), "GetHex2({n:?})");
        assert_eq!(st::<Int3>(&mut h, "GetHex3", vec![a(&n2)], &[I2]), hex::get_hex3(n2), "GetHex3({n2:?})");
        assert_eq!(bits(&st::<Float3>(&mut h, "GetHex3", vec![a(&q)], &[F2])), bits(&hex::get_hex3_f(q)), "GetHex3({q:?})");
        assert_eq!(bits(&st::<Float2>(&mut h, "HexToPlane", vec![a(&p)], &[F3])), bits(&hex::hex_to_plane(p)), "HexToPlane({p:?})");
        assert_eq!(bits(&st::<Float2>(&mut h, "HexToPlane", vec![a(&q)], &[F2])), bits(&hex::hex_to_plane2(q)), "HexToPlane({q:?})");
        assert_eq!(bits(&st::<Float3>(&mut h, "PlaneToHex", vec![a(&q)], &[])), bits(&hex::plane_to_hex(q)), "PlaneToHex({q:?})");
        assert_eq!(bits(&st::<Float3>(&mut h, "PlaneToWorld", vec![a(&q), arg::f32(y)], &[])), bits(&hex::plane_to_world(q, y)), "PlaneToWorld");
        assert_eq!(bits(&st::<Float2>(&mut h, "WorldToPlane", vec![a(&p)], &[])), bits(&hex::world_to_plane(p)), "WorldToPlane({p:?})");
        assert_eq!(st::<Int3>(&mut h, "HexRound", vec![a(&p)], &[]), hex::hex_round(p), "HexRound({p:?})");
        assert_eq!(st::<Int3>(&mut h, "HexRoundX", vec![a(&p)], &[]), hex::hex_round_x(p), "HexRoundX({p:?})");
        // HexClamp: a box around the origin, positions inside and out on every side
        let (lo, hi) = (r.f3(3.0), r.f3(3.0));
        let (min, max) = (f3(lo.x.min(hi.x), lo.y.min(hi.y), lo.z.min(hi.z)), f3(lo.x.max(hi.x), lo.y.max(hi.y), lo.z.max(hi.z)));
        let c = hex::hex_clamp(p, min, max);
        assert_eq!(bits(&st::<Float3>(&mut h, "HexClamp", vec![a(&p), a(&min), a(&max)], &[])), bits(&c), "HexClamp({p:?}, {min:?}, {max:?})");
        assert_eq!(st::<Int2>(&mut h, "RotateCCW", vec![a(&n2)], &[I2]), hex::rotate_ccw2i(n2), "RotateCCW({n2:?})");
        assert_eq!(st::<Int2>(&mut h, "RotateCCW2", vec![a(&n2)], &[I2]), hex::rotate_ccw2_2i(n2), "RotateCCW2({n2:?})");
        assert_eq!(st::<Int2>(&mut h, "RotateCW", vec![a(&n2)], &[I2]), hex::rotate_cw2i(n2), "RotateCW({n2:?})");
        assert_eq!(st::<Int2>(&mut h, "RotateCW2", vec![a(&n2)], &[I2]), hex::rotate_cw2_2i(n2), "RotateCW2({n2:?})");
        assert_eq!(st::<Int3>(&mut h, "RotateCCW", vec![a(&n)], &[I3]), hex::rotate_ccw(n), "RotateCCW({n:?})");
        assert_eq!(st::<Int3>(&mut h, "RotateCW", vec![a(&n)], &[I3]), hex::rotate_cw(n), "RotateCW({n:?})");
        assert_eq!(bits(&st::<Float3>(&mut h, "RotateCCW", vec![a(&p)], &[F3])), bits(&hex::rotate_ccw_f(p)), "RotateCCW({p:?})");
        assert_eq!(bits(&st::<Float3>(&mut h, "RotateCW", vec![a(&p)], &[F3])), bits(&hex::rotate_cw_f(p)), "RotateCW({p:?})");
    }
}

/// Call an iterator method on our state; returns the game's state after the call and its return bytes.
fn on(h: &mut Harness, it: &HexOnionIterator, m: &str, args: Vec<Value>) -> (HexOnionIterator, Vec<u8>) {
    let r = h.call_on_value(&format!("{IT}.{m}"), IT, &bytes(it), args, &[], false).unwrap_or_else(|e| panic!("{m}: {e:?}"));
    let ret_bytes = r.get("raw").and_then(Value::as_str).map(|_| ret::bytes(&r)).unwrap_or_default();
    (from(&ret::hex_field(&r["this_after"])), ret_bytes)
}

#[test]
fn hex_onion_iterator_matches_the_game() {
    let Some(mut h) = harness() else { return };
    // GetAt walks forward from the centre
    for v in (0..400).step_by(7).chain([1, 6, 7, 18, 19, 1000]) {
        let g: HexOnionIterator = from(&ret::bytes(&h.call(&format!("{IT}.GetAt"), vec![arg::i32(v)], &[], false).unwrap()));
        assert_eq!(g, HexOnionIterator::get_at(v as u32), "GetAt({v})");
    }
    let d = i3(0, 1, -1);
    let g: HexOnionIterator = from(&ret::bytes(&h.call(&format!("{IT}.GetWidthDelta"), vec![a(&d)], &[], false).unwrap()));
    assert_eq!(g, HexOnionIterator::with_delta(d), "GetWidthDelta");
    // a random walk of steps forward and back, and forward jumps
    let mut r = Rng(92);
    let mut ours = HexOnionIterator::default();
    for step in 0..1500 {
        let before = ours;
        let theirs = match r.next() % 8 {
            0..=3 => {
                let p = ours.increment();
                let t = on(&mut h, &before, "Increment", vec![]);
                assert_eq!(from::<Int3>(&t.1), p, "Increment return, step {step}");
                t.0
            }
            4 => {
                let p = ours.get_and_increment();
                let t = on(&mut h, &before, "GetAndIncrement", vec![]);
                assert_eq!(from::<Int3>(&t.1), p, "GetAndIncrement return, step {step}");
                t.0
            }
            5 if ours.vert_index > 0 => {
                let target = ours.vert_index + r.next() % 20;
                ours.set_vert_index(target);
                on(&mut h, &before, "set_vertIndex", vec![arg::i32(target as i32)]).0
            }
            _ if ours.vert_index > 0 => {
                let p = ours.decrement();
                let t = on(&mut h, &before, "Decrement", vec![]);
                assert_eq!(from::<Int3>(&t.1), p, "Decrement return, step {step}");
                t.0
            }
            _ => continue,
        };
        assert_eq!(theirs, ours, "step {step} from {before:?}");
        let (_, p2) = on(&mut h, &ours, "get_hexPos2", vec![]);
        assert_eq!(from::<Int2>(&p2), ours.hex_pos2(), "hexPos2");
        let (_, d2) = on(&mut h, &ours, "get_dualPos2", vec![]);
        assert_eq!(from::<Int2>(&d2), ours.dual_pos2(), "dualPos2");
    }
    assert!(ours.layer_index > 3, "the walk should reach a few rings: {ours:?}");
}
