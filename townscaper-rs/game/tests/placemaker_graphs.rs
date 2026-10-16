//! `ts_game::placemaker::graphs` against the original game (E4a records). Needs `TS_HARNESS=1`.
//! Components are put on fresh `GameObject`s (`AddComponent(Type)`).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::graphs::{
    single_compare_to, Action, Corner, OrientedModule, OrientedModuleSides, Qube, QubeRelation, SideProfile, Square,
    SquareId, SquareRelation, Voxel, VoxelType,
};
use ts_game::unity::mathematics::Int2;
use ts_game::unity::vector::{v3, Vector3};

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}
fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self, s: f32) -> f32 {
        ((self.next() as f32 / u32::MAX as f32) * 2.0 - 1.0) * s
    }
}

/// A new `GameObject` with the component `class` (`Namespace.Class, Assembly-CSharp`); returns (component, transform).
fn component(h: &mut Harness, class: &str) -> (u64, u64) {
    let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check graphs")], "param_types": ["System.String"], "main": true})), "GameObject"), "go");
    let ty = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(class)], "param_types": ["System.String"], "main": true, "keep": true})), class), class);
    let c = handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "component");
    let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "transform"), "transform");
    (c, t)
}

fn set_field(h: &mut Harness, obj: u64, name: &str, raw: &[u8]) {
    ok(h.request(json!({"field": name, "handle": obj, "set_raw": hexs(raw), "main": true})), name);
}

#[test]
fn action_matches_the_game() {
    let Some(mut h) = harness() else { return };
    const A: &str = "Placemaker.Graphs.Action";
    let mut r = Rng(71);
    for _ in 0..200 {
        let a = Action {
            hex_pos: Int2 { x: r.next() as i32 >> 8, y: r.next() as i32 >> 8 },
            height: r.next() as u8,
            in_type: VoxelType::from_i8((r.next() % 19) as i8),
            out_type: VoxelType::from_i8((r.next() % 19) as i8),
            pad: r.next() as u8,
            action_id: r.next() as i32,
        };
        let g = h.call_on_value(&format!("{A}.GetReverse"), A, &bytes(&a), vec![], &[], false).unwrap();
        assert_eq!(ret::bytes(&g), bytes(&a.get_reverse()), "GetReverse({a:?})");
        let n = Action::new(a.hex_pos, a.height, a.in_type, a.out_type, a.action_id);
        let g = h
            .call_on_value(&format!("{A}..ctor"), A, &[0u8; 16], vec![arg::raw(&bytes(&a.hex_pos)), arg::raw(&[a.height]), arg::raw(&[a.in_type as u8]), arg::raw(&[a.out_type as u8]), arg::i32(a.action_id)], &[], false)
            .unwrap();
        assert_eq!(ret::hex_field(&g["this_after"]), bytes(&n), ".ctor");
    }
}

#[test]
fn comparers_match_the_game() {
    let Some(mut h) = harness() else { return };
    const S: &str = "Placemaker.Graphs.OrientedModuleSides";
    let costs = [0.0f32, -0.0, 1.0, -1.0, 0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 3.25];
    for &a in &costs {
        for &b in &costs {
            let x = OrientedModuleSides { sides: SideProfile { v00: 3, ..Default::default() }, oriented_module: OrientedModule { module_index: 7, flags: 2, pad: 0 }, cost: a };
            let y = OrientedModuleSides { cost: b, ..x };
            let g = ok(h.request(json!({"call": "System.IComparable<Placemaker.Graphs.OrientedModuleSides>.CompareTo", "class": S, "this_raw": {"class": S, "raw": hexs(&bytes(&x))}, "args": [arg::raw(&bytes(&y))], "main": false})), "CompareTo");
            assert_eq!(ret::i32(&g), x.compare_to(&y), "OrientedModuleSides({a}).CompareTo({b})");
            assert_eq!(single_compare_to(a, b), x.compare_to(&y));
        }
    }
    let (v0, _) = component(&mut h, "Placemaker.Voxel, Assembly-CSharp");
    let (v1, _) = component(&mut h, "Placemaker.Voxel, Assembly-CSharp");
    for (a, b) in [(0, 0), (1, 2), (5, -3), (i32::MIN, i32::MAX), (7, 7)] {
        set_field(&mut h, v0, "cost", &a.to_le_bytes());
        set_field(&mut h, v1, "cost", &b.to_le_bytes());
        let g = ok(h.request(json!({"call": "System.IComparable<Placemaker.Voxel>.CompareTo", "class": "Placemaker.Voxel", "handle": v0, "args": [{"handle": v1}], "main": true})), "CompareTo");
        let ours = Voxel { cost: a, ..Default::default() }.compare_to(&Voxel { cost: b, ..Default::default() });
        assert_eq!(ret::i32(&g), ours, "Voxel {a} vs {b}");
    }
}

#[test]
fn relation_statics_match_the_game() {
    let Some(mut h) = harness() else { return };
    for (class, ours) in [
        ("Placemaker.Graphs.Square+Relation, Assembly-CSharp", (SquareRelation::EMPTY.square.is_some(), SquareRelation::EMPTY.index_in_other)),
        ("Placemaker.Graphs.Qube+Relation, Assembly-CSharp", (QubeRelation::EMPTY.qube.is_some(), QubeRelation::EMPTY.index_in_other)),
    ] {
        let g = ret::bytes(&ok(h.request(json!({"static_field": "x.empty", "class": class, "main": true})), class));
        let ptr = u32::from_le_bytes(g[..4].try_into().unwrap());
        assert_eq!((ptr != 0, g[4] as i8), ours, "{class}.empty");
    }
}

#[test]
fn corner_add_square_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut r = Rng(72);
    for round in 0..20 {
        let (corner, ct) = component(&mut h, "Placemaker.Graphs.Corner, Assembly-CSharp");
        let cp = v3(r.f(10.0), r.f(2.0), r.f(10.0));
        ok(h.request(json!({"call": "set_localPosition", "class": "UnityEngine.Transform", "handle": ct, "args": [arg::raw(&bytes(&cp))], "main": true})), "set_localPosition");
        let mut ours = Corner { local_position: cp, ..Default::default() };
        let n = 1 + (r.next() % 7) as usize;
        let mut positions: Vec<Vector3> = Vec::new();
        for k in 0..n {
            let (sq, st) = component(&mut h, "Placemaker.Graphs.Square, Assembly-CSharp");
            // now and then the same angle as another square (ties keep the earlier one first)
            let p = if k > 0 && r.next() % 5 == 0 { positions[0] } else { v3(cp.x + r.f(3.0), r.f(1.0), cp.z + r.f(3.0)) };
            positions.push(p);
            ok(h.request(json!({"call": "set_localPosition", "class": "UnityEngine.Transform", "handle": st, "args": [arg::raw(&bytes(&p))], "main": true})), "set_localPosition");
            ok(h.request(json!({"call": "AddSquare", "class": "Placemaker.Graphs.Corner", "handle": corner, "args": [{"handle": sq}], "main": true})), "AddSquare");
            ours.add_square(SquareId(k as u32), |s| positions[s.0 as usize]);
        }
        // the game's order, by each square's position
        let list = handle(&ok(h.request(json!({"field": "squares", "handle": corner, "main": true})), "squares"), "list");
        let lc = "System.Collections.Generic.List`1[[Placemaker.Graphs.Square, Assembly-CSharp]]";
        let mut game_positions = Vec::new();
        for i in 0..n {
            let s = handle(&ok(h.request(json!({"call": "get_Item", "class": lc, "handle": list, "args": [arg::i32(i as i32)], "main": true, "keep": true})), "get_Item"), "square");
            let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.Component", "handle": s, "args": [], "main": true, "keep": true})), "transform"), "t");
            let p = ret::bytes(&ok(h.request(json!({"call": "get_localPosition", "class": "UnityEngine.Transform", "handle": t, "args": [], "main": true})), "pos"));
            game_positions.push(p);
        }
        let ours_positions: Vec<Vec<u8>> = ours.squares.iter().map(|s| bytes(&positions[s.0 as usize])).collect();
        assert_eq!(game_positions, ours_positions, "round {round}: square order");
    }
}

#[test]
fn square_and_qube_accessors_match_the_game() {
    let Some(mut h) = harness() else { return };
    let (sq, _) = component(&mut h, "Placemaker.Graphs.Square, Assembly-CSharp");
    let (other, _) = component(&mut h, "Placemaker.Graphs.Square, Assembly-CSharp");
    let mut ours = Square::default();
    for i in 0..4i8 {
        let k = (3 - i) as i8;
        ok(h.request(json!({"call": "SetRelation", "class": "Placemaker.Graphs.Square", "handle": sq, "args": [arg::raw(&[i as u8]), arg::raw(&[k as u8]), {"handle": other}], "main": true})), "SetRelation");
        ours.set_relation(i, k, Some(SquareId(1)));
        let g = ret::bytes(&ok(h.request(json!({"field": format!("q{i}"), "handle": sq, "main": true})), "q"));
        assert_eq!((u32::from_le_bytes(g[..4].try_into().unwrap()) != 0, g[4] as i8), (ours.get_relation(i).square.is_some(), ours.get_relation(i).index_in_other), "q{i}");
    }
    assert!(h.request(json!({"call": "GetRelation", "class": "Placemaker.Graphs.Square", "handle": sq, "args": [arg::raw(&[4])], "main": true}))["ok"] == json!(false), "GetRelation(4) throws");
    let (qube, _) = component(&mut h, "Placemaker.Graphs.Qube, Assembly-CSharp");
    for (count, list_index) in [(0u8, -1i32), (255, -1), (255, 0), (3, 7)] {
        set_field(&mut h, qube, "voxelCount", &[count]);
        set_field(&mut h, qube, "possibleModuleListIndex", &list_index.to_le_bytes());
        let q = Qube { voxel_count: count, possible_module_list_index: list_index, ..Default::default() };
        let e = ret::bytes(&ok(h.request(json!({"call": "get_empty", "class": "Placemaker.Graphs.Qube", "handle": qube, "args": [], "main": true})), "empty"))[0] != 0;
        let f = ret::bytes(&ok(h.request(json!({"call": "get_fullyCovered", "class": "Placemaker.Graphs.Qube", "handle": qube, "args": [], "main": true})), "full"))[0] != 0;
        assert_eq!((e, f), (q.empty(), q.fully_covered()), "Qube({count}, {list_index})");
    }
}
