//! `ts_game::os::utils`'s Unity glue (E1d) against the game (`TS_HARNESS=1`): `UnityFunctions.EnumerateHirearchy`
//! walked step by step (`MoveNext`, `Current`, `ToString`, `Reset`, `Dispose`) over random object trees with
//! inactive branches, with and without `includeInactive`; `GetNewGameObject` under random parents (pose and name);
//! `Extensions.KeepGoing` and `SetSingleLayer` (on a disabled test camera, every layer name and an unknown one).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::timing::layer_master;
use ts_game::os::utils::{get_new_game_object, keep_going, set_single_layer, Hierarchy, HirearchyEnumerator};
use ts_game::unity::quaternion::{quat, Quaternion};
use ts_game::unity::transform::{Node, TransformId, Transforms};
use ts_game::unity::vector::{v3, Vector3};

const TR: &str = "UnityEngine.Transform";
const GO: &str = "UnityEngine.GameObject";
const HE: &str = "Os.Utils.UnityFunctions+HirearchyEnumerator, Assembly-CSharp";

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() % 100_000) as f32 / 100_000.0 * (hi - lo)
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u32) as usize
    }
}

fn ok(r: &Value, what: &str) {
    assert_eq!(r["ok"], true, "{what}: {r}");
}
fn keep(h: &mut Harness, mut req: Value, what: &str) -> u64 {
    req["keep"] = json!(true);
    let r = h.request(req);
    ok(&r, what);
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle: {r}"))
}
fn addr(h: &mut Harness, o: u64) -> u64 {
    let r = h.request(json!({"addr": o}));
    ok(&r, "addr");
    r["addr"].as_u64().unwrap()
}
fn call(o: u64, class: &str, m: &str, args: Vec<Value>, types: &[&str]) -> Value {
    let mut r = json!({"call": m, "class": class, "handle": o, "args": args, "main": true});
    if !types.is_empty() {
        r["param_types"] = json!(types);
    }
    r
}
fn call0(o: u64, class: &str, m: &str) -> Value {
    json!({"call": m, "class": class, "handle": o, "args": [], "main": true})
}
fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn v3b(v: Vector3) -> Vec<u8> {
    [v.x, v.y, v.z].iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn qb(q: Quaternion) -> Vec<u8> {
    [q.x, q.y, q.z, q.w].iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn fs(v: &Value) -> Vec<u32> {
    ret::bytes(v).chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn new_go(h: &mut Harness, name: &str) -> (u64, u64) {
    let go = keep(h, json!({"new": GO, "args": [arg::string(name)], "param_types": ["System.String"], "main": true}), "go");
    let t = keep(h, call0(go, GO, "get_transform"), "transform");
    (go, t)
}
fn destroy(h: &mut Harness, go: u64) {
    h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": go}], "param_types": ["UnityEngine.Object"], "main": true}));
}

/// Our side of a test tree: the transforms, each node's `activeSelf`, name and game address.
struct Tree {
    t: Transforms,
    active: Vec<bool>,
    names: Vec<String>,
}
impl Hierarchy for Tree {
    type Node = TransformId;
    fn child_count(&self, t: TransformId) -> i32 {
        self.t.child_count(t) as i32
    }
    fn child(&self, t: TransformId, i: i32) -> TransformId {
        self.t.child(t, i as usize)
    }
    fn parent(&self, t: TransformId) -> Option<TransformId> {
        self.t.nodes[t].parent
    }
    fn sibling_index(&self, t: TransformId) -> i32 {
        let p = self.t.nodes[t].parent.expect("a parent");
        (0..self.t.child_count(p)).position(|i| self.t.child(p, i) == t).unwrap() as i32
    }
    fn active_self(&self, t: TransformId) -> bool {
        self.active[t]
    }
    fn name(&self, t: TransformId) -> String {
        self.names[t].clone()
    }
}

#[test]
fn unity_glue_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.WorldMaster");
    let mut r = Rng(0x91e0_0e1d);
    let mut gos = vec![];
    // the hierarchy walk
    let mut walked = [0usize; 2];
    for tree in 0..40 {
        let n = 2 + r.pick(30);
        let mut ours = Tree { t: Transforms::default(), active: vec![], names: vec![] };
        let mut game: Vec<(u64, u64, u64)> = vec![]; // (go, transform, address)
        for k in 0..n {
            let name = format!("ts_check node {tree}.{k}");
            let (go, t) = new_go(h, &name);
            let parent = if k == 0 { None } else { Some(r.pick(k)) };
            let id = ours.t.add(Node { parent, ..Default::default() });
            assert_eq!(id, k);
            if let Some(p) = parent {
                ok(&h.request(call(t, TR, "SetParent", vec![json!({"handle": game[p].1})], &["UnityEngine.Transform"])), "SetParent");
            }
            let active = k == 0 || r.pick(4) != 0;
            if !active {
                ok(&h.request(call(go, GO, "SetActive", vec![arg::bool(false)], &["System.Boolean"])), "SetActive");
            }
            ours.active.push(active);
            ours.names.push(name);
            let a = addr(h, t);
            game.push((go, t, a));
        }
        gos.push(game[0].0);
        for include_inactive in [false, true] {
            // EnumerateHirearchy boxes `new HirearchyEnumerator(t, includeInactive)`; the harness keeps no boxed
            // structs, so the walk runs on the struct's bytes (`this_raw`), as the constructor lays them out
            let e = h.request(json!({"call": "Os.Utils.UnityFunctions.EnumerateHirearchy", "args": [{"handle": game[0].1}, arg::bool(include_inactive)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true}));
            ok(&e, "EnumerateHirearchy");
            assert_eq!(e["object"], ".HirearchyEnumerator", "EnumerateHirearchy's object");
            let mut raw: Vec<u8> = [0u32, 0, 0, include_inactive as u32, game[0].2 as u32, 0].iter().flat_map(|x| x.to_le_bytes()).collect();
            let on = |h: &mut Harness, m: &str, raw: &mut Vec<u8>| -> Value {
                let r = h.request(json!({"call": m, "class": HE, "this_raw": {"class": HE, "raw": hexs(raw)}, "args": [], "main": true}));
                ok(&r, m);
                *raw = ret::hex_field(&r["this_after"]);
                r
            };
            let mut mine = HirearchyEnumerator::new(0usize, include_inactive);
            let reset_at = r.pick(n + 2);
            let mut step = 0;
            loop {
                let what = format!("tree {tree} ({n} nodes), includeInactive {include_inactive}, step {step}");
                if step == reset_at {
                    on(h, "Reset", &mut raw);
                    mine.reset(&ours);
                }
                let g = ret::bytes(&on(h, "MoveNext", &mut raw))[0] != 0;
                let o = mine.move_next(&ours);
                assert_eq!(g, o, "{what}: MoveNext");
                let s = on(h, "ToString", &mut raw);
                assert_eq!(ret::string(&s), mine.to_string(&ours), "{what}: ToString");
                let w = |i: usize| u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap());
                assert_eq!([w(0) as i32, w(1) as i32, w(2) as i32], [mine.sibling_index, mine.sibling_count, mine.depth], "{what}: state");
                let at = |p: u32| if p == 0 { None } else { Some(game.iter().position(|x| x.2 as u32 == p).expect("a node of the tree")) };
                assert_eq!((at(w(4)), at(w(5))), (mine.current(), mine.parent), "{what}: transform, parent");
                if !g {
                    break;
                }
                walked[include_inactive as usize] += 1;
                step += 1;
                assert!(step < 500, "{what}: endless");
            }
            on(h, "Dispose", &mut raw);
            mine.dispose();
            let s = on(h, "ToString", &mut raw);
            assert_eq!(ret::string(&s), mine.to_string(&ours), "tree {tree}: ToString after Dispose");
            assert_eq!(raw[12] != 0, mine.include_inactive, "tree {tree}: includeInactive after Dispose");
        }
    }
    // GetNewGameObject
    for k in 0..40 {
        let mut ours = Transforms::default();
        let parent = if k % 5 == 0 {
            None
        } else {
            let (go, t) = new_go(h, "ts_check parent");
            gos.push(go);
            let p = v3(r.f(-50.0, 50.0), r.f(-50.0, 50.0), r.f(-50.0, 50.0));
            let q = {
                let q = quat(r.f(-1.0, 1.0), r.f(-1.0, 1.0), r.f(-1.0, 1.0), r.f(-1.0, 1.0));
                let m = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
                quat(q.x / m, q.y / m, q.z / m, q.w / m)
            };
            let s = v3(r.f(0.2, 3.0), r.f(0.2, 3.0), r.f(-3.0, 3.0));
            ok(&h.request(call(t, TR, "set_localPosition", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"])), "parent position");
            ok(&h.request(call(t, TR, "set_localRotation", vec![arg::raw(&qb(q))], &["UnityEngine.Quaternion"])), "parent rotation");
            ok(&h.request(call(t, TR, "set_localScale", vec![arg::raw(&v3b(s))], &["UnityEngine.Vector3"])), "parent scale");
            let id = ours.add(Node::default());
            ours.set_local_position(id, p);
            ours.set_local_rotation(id, q);
            ours.set_local_scale(id, s);
            Some((t, id))
        };
        let pa = match parent {
            Some((t, _)) => json!({"handle": t}),
            None => json!({"null": true}),
        };
        let go = keep(h, json!({"call": "Os.Utils.UnityFunctions.GetNewGameObject", "args": [arg::string("ts_check new"), pa], "param_types": ["System.String", "UnityEngine.Transform"], "main": true}), "GetNewGameObject");
        if parent.is_none() {
            gos.push(go);
        }
        let t = keep(h, call0(go, GO, "get_transform"), "transform");
        let id = get_new_game_object(&mut ours, parent.map(|x| x.1));
        let x = h.request(call0(go, "UnityEngine.Object", "get_name"));
        let name = x["string"].as_str().unwrap_or_else(|| panic!("name: {x}")).to_string();
        assert_eq!(name, "New Game Object", "GetNewGameObject ignores the name");
        let g: Vec<Vec<u32>> = ["get_position", "get_rotation", "get_localPosition", "get_localRotation", "get_localScale"].iter().map(|m| fs(&h.request(call0(t, TR, m)))).collect();
        let b3 = |v: Vector3| vec![v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
        let bq = |q: Quaternion| vec![q.x.to_bits(), q.y.to_bits(), q.z.to_bits(), q.w.to_bits()];
        let o = vec![b3(ours.position(id)), bq(ours.rotation(id)), b3(ours.local_position(id)), bq(ours.local_rotation(id)), b3(ours.local_scale(id))];
        assert_eq!(g, o, "GetNewGameObject {k}: pose");
    }
    // KeepGoing on a stopped stopwatch (0 ms)
    let sw = keep(h, json!({"new": "System.Diagnostics.Stopwatch", "args": [], "main": true}), "Stopwatch");
    for ms in [-3, -1, 0, 1, 2, 1000, i32::MIN, i32::MAX] {
        let g = ret::bytes(&h.request(json!({"call": "Os.Utils.Extensions.KeepGoing", "args": [{"handle": sw}, arg::i32(ms)], "param_types": ["System.Diagnostics.Stopwatch", "System.Int32"], "main": true})))[0] != 0;
        assert_eq!(g, keep_going(0, ms), "KeepGoing(0 ms, {ms})");
    }
    // SetSingleLayer on a disabled test camera
    let (cam_go, _) = new_go(h, "ts_check camera");
    gos.push(cam_go);
    let ct = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("UnityEngine.Camera, UnityEngine.CoreModule")], "param_types": ["System.String"], "main": true}), "type");
    let cam = keep(h, call(cam_go, GO, "AddComponent", vec![json!({"handle": ct})], &["System.Type"]), "Camera");
    ok(&h.request(call(cam, "UnityEngine.Behaviour", "set_enabled", vec![arg::bool(false)], &["System.Boolean"])), "disable camera");
    // the project's layers (LayerMaster's table, read from the running game) and an unknown name
    let names: Vec<(&str, i32)> = vec![
        ("ts_check no such layer", -1),
        ("Default", layer_master::DEFAULT_LAYER),
        ("Voxels", layer_master::VOXEL_LAYER),
        ("Ground", layer_master::GROUND_LAYER),
        ("Props", layer_master::PROPS_LAYER),
        ("Grid", layer_master::GRID_LAYER),
        ("LastClick", layer_master::LAST_CLICK_LAYER),
        ("Ao", layer_master::AO_LAYER),
        ("AoStack", layer_master::AO_STACK_LAYER),
        ("OnlyShadows", layer_master::ONLY_SHADOWS_LAYER),
        ("NoShadows", layer_master::NO_SHADOWS_LAYER),
    ];
    let mut checked = 0;
    for &(name, layer) in &names {
        let layer_mask = if layer < 0 { 0 } else { 1i32 << layer };
        for _ in 0..6 {
            let mask = r.next() as i32;
            let enabled = r.pick(2) == 0;
            let res = h.batch(vec![
                call(cam, "UnityEngine.Camera", "set_cullingMask", vec![arg::i32(mask)], &["System.Int32"]),
                json!({"call": "Os.Utils.Extensions.SetSingleLayer", "args": [{"handle": cam}, arg::string(name), arg::bool(enabled)], "param_types": ["UnityEngine.Camera", "System.String", "System.Boolean"], "main": true}),
                call0(cam, "UnityEngine.Camera", "get_cullingMask"),
            ]);
            for x in &res {
                ok(x, "SetSingleLayer");
            }
            assert_eq!(ret::i32(&res[2]), set_single_layer(mask, layer_mask, enabled), "SetSingleLayer({name}, {enabled}) on {mask:#x}");
            checked += 1;
        }
    }
    for g in gos {
        destroy(h, g);
    }
    eprintln!("hierarchy steps {} (active only) + {} (all); {} layer masks", walked[0], walked[1], checked);
}
