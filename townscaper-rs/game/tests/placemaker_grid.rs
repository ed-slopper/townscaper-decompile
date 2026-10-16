//! `ts_game::placemaker::grid_generator` against the original game (E3b). Needs `TS_HARNESS=1`.
//! Instance methods run on the scene's `GridGenerator` (they read none of its state).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::grid_generator as gg;
use ts_game::unity::mathematics::{f2, f3, i3, Float2, Float3, Int2, Int3, Int3x3};

const G: &str = "Placemaker.GridGenerator";
const F2: &str = "Unity.Mathematics.float2";
const F3: &str = "Unity.Mathematics.float3";
const I2: &str = "Unity.Mathematics.int2";
const I3: &str = "Unity.Mathematics.int3";

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
    /// Grid coordinates: near the 36 / 72 / 108 / 1152 / 2304 lattices (exact hits included), a wide spread,
    /// and now and then something huge.
    fn i(&mut self) -> i32 {
        match self.next() % 6 {
            0 => ((self.next() % 200) as i32 - 100) * 36,
            1 => ((self.next() % 200) as i32 - 100) * 36 + (self.next() % 5) as i32 - 2,
            2 => ((self.next() % 20) as i32 - 10) * 1152 + (self.next() % 7) as i32 - 3,
            3 => self.next() as i32 >> (self.next() % 8),
            _ => (self.next() % 12001) as i32 - 6000,
        }
    }
    fn i2(&mut self) -> Int2 {
        Int2 { x: self.i(), y: self.i() }
    }
    /// A cube position (`x + y + z = 0`), or now and then not.
    fn i3(&mut self) -> Int3 {
        let (x, y) = (self.i(), self.i());
        if self.next() % 8 == 0 { i3(x, y, self.i()) } else { i3(x, y, x.wrapping_add(y).wrapping_neg()) }
    }
    fn f(&mut self) -> f32 {
        if self.next() % 3 == 0 { self.i() as f32 } else { (self.i() as f32) + (self.next() % 1000) as f32 / 1000.0 - 0.5 }
    }
}

fn call(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str], instance: bool) -> Value {
    let mut req = json!({"call": format!("{G}.{m}"), "args": args, "main": true});
    if !types.is_empty() {
        req["param_types"] = json!(types);
    }
    if instance {
        req["on"] = json!(G);
    }
    let r = h.request(req);
    assert_eq!(r["ok"], json!(true), "{m}: {r}");
    r
}
fn st<R: Copy + Default>(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> R {
    from(&ret::bytes(&call(h, m, args, types, false)))
}
fn inst<R: Copy + Default>(h: &mut Harness, m: &str, args: Vec<Value>, types: &[&str]) -> R {
    from(&ret::bytes(&call(h, m, args, types, true)))
}
fn yes(h: &mut Harness, m: &str, args: Vec<Value>) -> bool {
    ret::bytes(&call(h, m, args, &[], true))[0] != 0
}

#[test]
fn grid_statics_match_the_game() {
    let Some(mut h) = harness() else { return };
    let d = ret::bytes(&h.request(json!({"static_field": format!("{G}.diagonal"), "main": true})));
    assert_eq!(d, bytes(&gg::diagonal()), "diagonal");
    assert_eq!(st::<Int2>(&mut h, "GetClusterOffset", vec![], &[]), gg::get_cluster_offset());
    for n in (-2..400).chain([1000, 4000]) {
        assert_eq!(st::<Int2>(&mut h, "GetStartingHex", vec![arg::i32(n)], &[]), gg::get_starting_hex(n), "GetStartingHex({n})");
    }
}

#[test]
fn grid_coordinates_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(41);
    let (mut borders, mut new_grids) = (0, 0);
    for _ in 0..1500 {
        let (p2, p3) = (r.i2(), r.i3());
        let (q2, q3) = (f2(r.f(), r.f()), f3(r.f(), r.f(), r.f()));
        let plane = f2(r.f() / 8.0, r.f() / 8.0);
        let same = |a: &[u8], b: &[u8]| a == b;
        assert!(same(&bytes(&st::<Float2>(&mut h, "HexToPlane", vec![a(&q3)], &[F3])), &bytes(&gg::hex_to_plane(q3))), "HexToPlane({q3:?})");
        assert!(same(&bytes(&st::<Float2>(&mut h, "HexToPlane", vec![a(&p3)], &[I3])), &bytes(&gg::hex_to_plane_i3(p3))), "HexToPlane({p3:?})");
        assert!(same(&bytes(&st::<Float2>(&mut h, "HexToPlane", vec![a(&q2)], &[F2])), &bytes(&gg::hex_to_plane2(q2))), "HexToPlane({q2:?})");
        assert!(same(&bytes(&st::<Float2>(&mut h, "HexToPlane", vec![a(&p2)], &[I2])), &bytes(&gg::hex_to_plane_i2(p2))), "HexToPlane({p2:?})");
        assert!(same(&bytes(&st::<Float3>(&mut h, "PlaneToHex", vec![a(&plane)], &[])), &bytes(&gg::plane_to_hex(plane))), "PlaneToHex({plane:?})");
        assert_eq!(inst::<f32>(&mut h, "GetVertCost", vec![a(&q2)], &[]).to_bits(), gg::get_vert_cost(q2).to_bits(), "GetVertCost({q2:?})");
        let f3p = f3(p3.x as f32, p3.y as f32, p3.z as f32);
        for v in [f3p, q3] {
            assert_eq!(inst::<Int3>(&mut h, "HexToPatchHex", vec![a(&v)], &[F3]), gg::hex_to_patch_hex_f(v), "HexToPatchHex({v:?})");
        }
        assert_eq!(inst::<Int2>(&mut h, "HexToPatchHex", vec![a(&p2)], &[I2]), gg::hex_to_patch_hex(p2), "HexToPatchHex({p2:?})");
        assert_eq!(inst::<Int2>(&mut h, "GetPatchOffset", vec![a(&p2)], &[]), gg::get_patch_offset(p2), "GetPatchOffset({p2:?})");
        let rr = call(&mut h, "HexToPatchHexes", vec![a(&p3), a(&Int3x3::default()), arg::raw(&[0])], &[], true);
        let (m, mask) = gg::hex_to_patch_hexes(p3);
        assert_eq!(from::<Int3x3>(&ret::hex_field(&rr["args_after"][1])), m, "HexToPatchHexes({p3:?})");
        assert_eq!(ret::hex_field(&rr["args_after"][2]), vec![mask], "HexToPatchHexes({p3:?}) mask");
        assert_eq!(inst::<Int3>(&mut h, "HexToGridCentroid", vec![a(&p3)], &[]), gg::hex_to_grid_centroid(p3), "HexToGridCentroid({p3:?})");
        assert_eq!(inst::<Int3>(&mut h, "GetClosestClusterPos", vec![a(&p3)], &[]), gg::get_closest_cluster_pos(p3), "GetClosestClusterPos({p3:?})");
        assert_eq!(inst::<i32>(&mut h, "GetSubdivisions", vec![a(&p2)], &[]), gg::get_subdivisions(p2), "GetSubdivisions({p2:?})");
        let b = gg::is_border(p2);
        assert_eq!(yes(&mut h, "IsBorder", vec![a(&p2)]), b, "IsBorder({p2:?})");
        borders += b as usize;
        let n = gg::is_new_grid(p3);
        assert_eq!(yes(&mut h, "IsNewGrid", vec![a(&p3)]), n, "IsNewGrid({p3:?})");
        new_grids += n as usize;
    }
    eprintln!("of 1500: IsBorder {borders}, IsNewGrid {new_grids}");
    assert!(borders > 20 && new_grids > 20, "both answers should occur");
}
