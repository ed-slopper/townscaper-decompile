//! `ts_game::placemaker::quads` against the original game (E3a). Needs `TS_HARNESS=1`.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::quads::grid_generation::{CustomGridLibrary, CustomGridPatch, MotivationCounter};
use ts_game::placemaker::quads::{Quad, Vert};
use ts_game::unity::mathematics::{f2, Int2, Random};

const Q: &str = "Placemaker.Quads.Quad";
const V: &str = "Placemaker.Quads.Vert";
const M: &str = "Placemaker.Quads.GridGeneration.MotivationCounter";
const LIB: &str = "Placemaker.Quads.GridGeneration.CustomGridLibrary";

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}
fn from<T: Copy + Default>(b: &[u8]) -> T {
    assert_eq!(b.len(), std::mem::size_of::<T>(), "reply size");
    let mut v = T::default();
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), &mut v as *mut T as *mut u8, b.len()) };
    v
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
        ((self.next() as f32 / u32::MAX as f32) * 2.0 - 1.0) * scale
    }
    fn i2(&mut self) -> Int2 {
        Int2 { x: (self.next() % 7) as i32 - 3, y: (self.next() % 7) as i32 - 3 }
    }
    /// A quad around `c`: corners near it, sometimes repeated (so `IsValid` / `ContainsVert` vary).
    fn quad(&mut self, cx: f32, cy: f32) -> Quad {
        let mut q = Quad { quad_index: self.next() as u8, ..Default::default() };
        q.v0 = self.i2();
        q.v1 = if self.next() % 6 == 0 { q.v0 } else { self.i2() };
        q.v2 = self.i2();
        q.v3 = self.i2();
        q.p0 = f2(cx + self.f(1.0), cy + self.f(1.0));
        q.p1 = f2(cx + self.f(1.0), cy + self.f(1.0));
        q.p2 = f2(cx + self.f(1.0), cy + self.f(1.0));
        q.p3 = f2(cx + self.f(1.0), cy + self.f(1.0));
        q
    }
}

/// Call a value-type method; returns (the value after the call, the return value's bytes).
fn on(h: &mut Harness, class: &str, this: &[u8], m: &str, args: Vec<Value>, types: &[&str]) -> (Vec<u8>, Vec<u8>) {
    let r = h.call_on_value(&format!("{class}.{m}"), class, this, args, types, false).unwrap_or_else(|e| panic!("{m}: {e:?}"));
    let rb = if r.get("raw").is_some() { ret::bytes(&r) } else { Vec::new() };
    (ret::hex_field(&r["this_after"]), rb)
}

#[test]
fn quad_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(31);
    for _ in 0..300 {
        let (cx, cy) = (r.f(5.0), r.f(5.0));
        let q = r.quad(cx, cy);
        let b = bytes(&q);
        assert_eq!(from::<Int2>(&on(&mut h, Q, &b, "get_hexPosSum", vec![], &[]).1), q.hex_pos_sum(), "hexPosSum {q:?}");
        assert_eq!(on(&mut h, Q, &b, "get_planePos", vec![], &[]).1, bytes(&q.plane_pos()), "planePos {q:?}");
        assert_eq!(on(&mut h, Q, &b, "IsValid", vec![], &[]).1[0] != 0, q.is_valid(), "IsValid {q:?}");
        for i in 0..4 {
            assert_eq!(from::<Int2>(&on(&mut h, Q, &b, "GetVert", vec![arg::i32(i)], &[]).1), q.get_vert(i), "GetVert({i})");
            assert_eq!(on(&mut h, Q, &b, "GetPlanePos", vec![arg::i32(i)], &[]).1, bytes(&q.get_plane_pos(i)), "GetPlanePos({i})");
        }
        assert!(h.call_on_value(&format!("{Q}.GetVert"), Q, &b, vec![arg::i32(4)], &[], false).is_err(), "GetVert(4) throws");
        let v = if r.next() % 2 == 0 { q.get_vert((r.next() % 4) as i32) } else { r.i2() };
        assert_eq!(on(&mut h, Q, &b, "ContainsVert", vec![arg::raw(&bytes(&v))], &[]).1[0] != 0, q.contains_vert(v), "ContainsVert({v:?})");
        for (m, f) in [("Rotate", Quad::rotate as fn(&mut Quad)), ("Rotate2", Quad::rotate2), ("Rotate3", Quad::rotate3)] {
            let mut ours = q;
            f(&mut ours);
            assert_eq!(on(&mut h, Q, &b, m, vec![], &[]).0, bytes(&ours), "{m} {q:?}");
        }
    }
}

#[test]
fn vert_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(32);
    for round in 0..120 {
        let mut ours = Vert { hex_pos: r.i2(), plane_pos: f2(r.f(4.0), r.f(4.0)), angle: r.f(400.0).abs(), ..Default::default() };
        if round % 10 == 0 {
            ours.quad_count = -1;
        }
        // add quads around the corner (up to 7: the seventh overwrites q5)
        let n = if ours.quad_count < 0 { 1 } else { 1 + r.next() % 7 };
        for _ in 0..n {
            let a = r.f(std::f32::consts::PI);
            let q = r.quad(ours.plane_pos.x + 2.0 * a.cos(), ours.plane_pos.y + 2.0 * a.sin());
            let before = bytes(&ours);
            ours.add_quad(q);
            assert_eq!(on(&mut h, V, &before, "AddQuad", vec![arg::raw(&bytes(&q))], &[]).0, bytes(&ours), "AddQuad round {round}");
        }
        let b = bytes(&ours);
        assert_eq!(on(&mut h, V, &b, "get_worldPos", vec![], &[]).1, bytes(&ours.world_pos()), "worldPos");
        assert_eq!(on(&mut h, V, &b, "get_full", vec![], &[]).1[0] != 0, ours.full(), "full {}", ours.angle);
        assert_eq!(on(&mut h, V, &b, "get_validPosition", vec![], &[]).1[0] != 0, ours.valid_position(), "validPosition");
        for i in -1..8 {
            assert_eq!(on(&mut h, V, &b, "GetQuad", vec![arg::i32(i)], &[]).1, bytes(&ours.get_quad(i)), "GetQuad({i})");
        }
        let (i, q) = ((r.next() % 9) as i32 - 1, r.quad(0.0, 0.0));
        let mut set = ours;
        set.set_quad(i, q);
        assert_eq!(on(&mut h, V, &b, "SetQuad", vec![arg::i32(i), arg::raw(&bytes(&q))], &[]).0, bytes(&set), "SetQuad({i})");
        for _ in 0..4 {
            let v = if r.next() % 2 == 0 { ours.get_quad((r.next() % 6) as i32).get_vert((r.next() % 4) as i32) } else { r.i2() };
            assert_eq!(on(&mut h, V, &b, "ContainsVert", vec![arg::raw(&bytes(&v))], &[]).1[0] != 0, ours.contains_vert(v), "ContainsVert({v:?}) n={}", ours.quad_count);
        }
    }
}

#[test]
fn motivation_counter_matches_the_game() {
    let Some(mut h) = harness() else { return };
    for c in [0u8, 1, 2, 7, 254, 255] {
        let src = Int2 { x: 3, y: -4 };
        for (m, f) in [("AddMotivation", MotivationCounter::add_motivation as fn(&mut MotivationCounter, Int2) -> i32), ("RemoveMotivation", MotivationCounter::remove_motivation)] {
            let mut ours = MotivationCounter { count: c };
            let ret_ours = f(&mut ours, src);
            let (after, rb) = on(&mut h, M, &[c], m, vec![arg::raw(&bytes(&src))], &[]);
            assert_eq!(after, vec![ours.count], "{m}({c})");
            assert_eq!(i32::from_le_bytes(rb[..4].try_into().unwrap()), ret_ours, "{m}({c}) return");
        }
    }
}

/// `GetRandom` on a fresh library (`ScriptableObject.CreateInstance`: its patch list is empty, so the game
/// throws exactly when the drawn slot holds a patch index, and returns null for 255). Lists where only the
/// slot our port picks (or another one) holds a patch show which index the game drew; the state after the
/// draw is compared when it returns.
#[test]
fn custom_grid_library_get_random_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let ty = h.request(json!({"call": "System.Type.GetType", "args": [arg::string(&format!("{LIB}, Assembly-CSharp"))],
        "param_types": ["System.String"], "main": true, "keep": true}));
    let ty = ty["handle"].as_u64().unwrap_or_else(|| panic!("Type.GetType: {ty}"));
    let made = h.request(json!({"call": "UnityEngine.ScriptableObject.CreateInstance", "args": [{"handle": ty}],
        "param_types": ["System.Type"], "main": true, "keep": true}));
    let lib_handle = made["handle"].as_u64().unwrap_or_else(|| panic!("CreateInstance: {made}"));
    let lib = CustomGridLibrary { all_custom_patches: vec![CustomGridPatch::default(); 40], ..Default::default() };
    let mut r = Rng(33);
    let list_class = "System.Collections.Generic.List`1[[System.Byte]]";
    let (mut drew, mut empty) = (0, 0);
    for k in 0..200 {
        let n = 1 + (r.next() % 70) as usize;
        let state = r.next() | 1;
        let mut ours = Random { state };
        let picked = ((n as u64 * state as u64) >> 32) as usize;
        let target = if k % 2 == 0 { picked } else { (r.next() as usize) % n };
        let list: Vec<u8> = (0..n).map(|i| if i == target { (r.next() % 40) as u8 } else { 0xFF }).collect();
        let got_ours = lib.get_random(&list, &mut ours).is_some();
        let made = h.request(json!({"new": list_class, "args": [], "main": true}));
        let handle = made["handle"].as_u64().expect("new List<byte>");
        let adds: Vec<Value> = list.iter().map(|b| json!({"call": "Add", "class": list_class, "handle": handle, "args": [arg::raw(&[*b])]})).collect();
        assert!(h.batch(adds).iter().all(|a| a["ok"] == json!(true)), "List.Add");
        let g = h.request(json!({"call": format!("{LIB}.GetRandom"), "handle": lib_handle, "main": true,
            "args": [{"handle": handle}, arg::raw(&state.to_le_bytes())]}));
        if got_ours {
            drew += 1;
            assert_eq!(g["ok"], json!(false), "GetRandom n={n} state={state:#x}: ours drew a patch, game {g}");
            assert!(g["error"].as_str().unwrap_or("").contains("ArgumentOutOfRange"), "{g}");
        } else {
            empty += 1;
            assert_eq!(g.get("null"), Some(&json!(true)), "GetRandom n={n} state={state:#x}: ours drew 255, game {g}");
            assert_eq!(ret::hex_field(&g["args_after"][1]), ours.state.to_le_bytes(), "state after");
        }
    }
    assert!(drew > 50 && empty > 50, "drew {drew}, empty {empty}");
}
