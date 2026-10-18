//! `BirdFlock.OnUpdate` and `GetLanding` (G5b) against the game (`TS_HARNESS=1`): a fresh `BirdFlock` (the scene's
//! master and meshes) with up to 12 birds cloned from the scene's `srcBird` and 24 test perches (`PropCollider` +
//! `BirdLanding`, and the `PropNode` that comes with them, in clusters) put into the prop placer's collider grid far from the town; a few
//! leaving birds in front of the camera. Each round sets every bird, perch and flock field at random,
//! `UnityEngine.Random` to a known state, runs one `OnUpdate`, and compares the generator's state, every field,
//! the birds' transforms, meshes and active flags, the list and the perches to ours (to the bit). The test
//! objects and grid cells are removed at the end.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::life::bird_flock::{BirdFlock, BirdLanding, BirdMesh, BirdPrefab, BirdWorld, ColliderRef, LandingId};
use ts_game::unity::mathematics::{Int3, Int3x2};
use ts_game::unity::quaternion::{quat, Quaternion};
use ts_game::unity::random::Rand;
use ts_game::unity::vector::{v3, Vector3};

const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;
const BF: &str = "Placemaker.Life.BirdFlock";
const TR: &str = "UnityEngine.Transform";
const BIRDS: usize = 12;
const PERCHES: usize = 24;
const ROUNDS: usize = 300;
const MESHES: [BirdMesh; 7] = [BirdMesh::Sit, BirdMesh::HalfSit, BirdMesh::Stand, BirdMesh::Land, BirdMesh::Flap, BirdMesh::Fly, BirdMesh::Glide];
const MESH_FIELDS: [&str; 7] = ["sitMesh", "halfSitMesh", "standMesh", "landMesh", "flapMesh", "flyMesh", "glideMesh"];
const DICT: &str = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int3, Unity.Mathematics],[System.Collections.Generic.List`1[[Placemaker.Props.PropCollider, Assembly-CSharp]], mscorlib]]";
const COLLIDERS: &str = "System.Collections.Generic.List`1[[Placemaker.Props.PropCollider, Assembly-CSharp]]";
const BIRD_LIST: &str = "System.Collections.Generic.List`1[[Placemaker.Life.Bird, Assembly-CSharp]]";

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
    fn chance(&mut self, p: f32) -> bool {
        self.f(0.0, 1.0) < p
    }
    fn ball(&mut self, r: f32) -> Vector3 {
        loop {
            let v = v3(self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0));
            if v.sqr_magnitude() <= 1.0 {
                return v.mul(r);
            }
        }
    }
    fn quat(&mut self) -> Quaternion {
        let q = quat(self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0), self.f(-1.0, 1.0));
        let n = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
        quat(q.x / n, q.y / n, q.z / n, q.w / n)
    }
}

fn ok(r: &Value, what: &str) {
    assert_eq!(r["ok"], true, "{what}: {r}");
}
fn keep(h: &mut Harness, mut req: Value, what: &str) -> u64 {
    req["keep"] = json!(true);
    let r = h.request(req);
    ok(&r, what);
    r["handle"].as_u64().unwrap()
}
fn addr(h: &mut Harness, o: u64) -> u64 {
    let r = h.request(json!({"addr": o}));
    ok(&r, "addr");
    r["addr"].as_u64().unwrap()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn set(o: u64, f: &str, b: &[u8]) -> Value {
    json!({"field": f, "handle": o, "set_raw": hex(b), "main": true})
}
fn set_obj(o: u64, f: &str, v: Option<u64>) -> Value {
    match v {
        Some(x) => json!({"field": f, "handle": o, "set_handle": x, "main": true}),
        None => json!({"field": f, "handle": o, "set_null": true, "main": true}),
    }
}
fn get(o: u64, f: &str) -> Value {
    json!({"field": f, "handle": o, "main": true})
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
fn v3b(v: Vector3) -> Vec<u8> {
    [v.x, v.y, v.z].iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn qb(q: Quaternion) -> Vec<u8> {
    [q.x, q.y, q.z, q.w].iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn fs(v: &Value) -> Vec<f32> {
    ret::bytes(v).chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn vec3(v: &Value) -> Vector3 {
    let f = fs(v);
    v3(f[0], f[1], f[2])
}
fn bits3(v: Vector3) -> [u32; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}
fn bitsq(q: Quaternion) -> [u32; 4] {
    [q.x.to_bits(), q.y.to_bits(), q.z.to_bits(), q.w.to_bits()]
}
fn native(va: u32, buf: &[u8]) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": [{"buf": hex(buf)}]})
}
fn new_go(h: &mut Harness, name: &str) -> (u64, u64) {
    let go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true}), "go");
    let t = keep(h, call0(go, "UnityEngine.GameObject", "get_transform"), "transform");
    (go, t)
}
fn add_component(h: &mut Harness, go: u64, ty: &str) -> u64 {
    let t = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string(ty)], "param_types": ["System.String"], "main": true}), "type");
    keep(h, call(go, "UnityEngine.GameObject", "AddComponent", vec![json!({"handle": t})], &["System.Type"]), "AddComponent")
}

/// One clone of the scene's bird.
struct GameBird {
    bird: u64,
    root: u64,
    body: u64,
    shade: u64,
    shade_go: u64,
    mf0: u64,
    mf1: u64,
    mf0_go: u64,
    mf0_t: u64,
    mf1_t: u64,
    addr: u64,
}

/// One test perch.
struct GamePerch {
    go: u64,
    t: u64,
    landing: u64,
    node: u64,
    collider: u64,
    pos: Vector3,
    addr: u64,
}

struct Setup {
    flock_go: u64,
    flock: u64,
    disabled: u64,
    disabled_addr: u64,
    list: u64,
    dict: u64,
    graph: u64,
    camera: u64,
    cam_pos: Vector3,
    cam_fwd: Vector3,
    bounds: Int3x2,
    bounds_height: i32,
    meshes: Vec<u64>,
    mesh_addrs: Vec<u64>,
    shade_scale: Vector3,
    birds: Vec<GameBird>,
    perches: Vec<GamePerch>,
    cells: Vec<(Int3, Vec<usize>)>,
}

fn setup(h: &mut Harness) -> Setup {
    h.wait_for_object("Placemaker.WorldMaster");
    let scene_flock = keep(h, json!({"field": "flock", "on": "Placemaker.WorldMaster", "main": true}), "flock");
    let master = keep(h, get(scene_flock, "master"), "master");
    let (flock_go, _) = new_go(h, "ts_check flock");
    let flock = add_component(h, flock_go, &format!("{BF}, Assembly-CSharp"));
    ok(&h.request(set_obj(flock, "master", Some(master))), "master");
    let mut meshes = vec![];
    for f in MESH_FIELDS {
        let m = keep(h, get(scene_flock, f), f);
        ok(&h.request(set_obj(flock, f, Some(m))), f);
        meshes.push(m);
    }
    let mesh_addrs = meshes.iter().map(|&m| addr(h, m)).collect();
    let (_, disabled) = new_go(h, "ts_check disabled birds");
    ok(&h.request(set_obj(flock, "disabledBirds", Some(disabled))), "disabledBirds");
    let disabled_addr = addr(h, disabled);
    let list = keep(h, json!({"new": BIRD_LIST, "args": [], "main": true}), "birds");
    ok(&h.request(set_obj(flock, "birds", Some(list))), "birds");
    let graph = keep(h, get(master, "graph"), "graph");
    let b = ret::bytes(&h.request(get(graph, "bounds")));
    let i = |k: usize| i32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap());
    let bounds = Int3x2 { c0: Int3 { x: i(0), y: i(1), z: i(2) }, c1: Int3 { x: i(3), y: i(4), z: i(5) } };
    let bounds_height = ret::i32(&h.request(get(graph, "boundsHeight")));
    let placer = keep(h, get(master, "propPlacer"), "propPlacer");
    let dict = keep(h, get(placer, "colliderDict"), "colliderDict");
    let camera = keep(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true}), "camera");
    let cam_t = keep(h, call0(camera, "UnityEngine.Component", "get_transform"), "camera transform");
    let cam_pos = vec3(&h.request(call0(cam_t, TR, "get_position")));
    let cam_fwd = vec3(&h.request(call0(cam_t, TR, "get_forward")));
    // birds: clones of the scene's
    let src = keep(h, get(scene_flock, "srcBird"), "srcBird");
    let mut birds = vec![];
    for _ in 0..BIRDS {
        let bird = keep(h, json!({"call": "UnityEngine.Object.Instantiate", "args": [{"handle": src}], "param_types": ["UnityEngine.Object"], "main": true}), "Instantiate");
        let root = keep(h, call0(bird, "UnityEngine.Component", "get_transform"), "root");
        let body = keep(h, get(bird, "body"), "body");
        let shade = keep(h, get(bird, "shade"), "shade");
        let shade_go = keep(h, call0(shade, "UnityEngine.Component", "get_gameObject"), "shade go");
        let mf0 = keep(h, get(bird, "mf0"), "mf0");
        let mf1 = keep(h, get(bird, "mf1"), "mf1");
        let mf0_go = keep(h, call0(mf0, "UnityEngine.Component", "get_gameObject"), "mf0 go");
        let mf0_t = keep(h, call0(mf0, "UnityEngine.Component", "get_transform"), "mf0 t");
        let mf1_t = keep(h, call0(mf1, "UnityEngine.Component", "get_transform"), "mf1 t");
        let a = addr(h, bird);
        birds.push(GameBird { bird, root, body, shade, shade_go, mf0, mf1, mf0_go, mf0_t, mf1_t, addr: a });
    }
    let shade_scale = vec3(&h.request(call0(birds[0].shade, TR, "get_localScale")));
    // perches: 8 clusters of 3 round (5000, 4, 5000)
    let mut r = Rng(0x5eed_b12d);
    let mut perches = vec![];
    for k in 0..PERCHES {
        let centre = if k % 3 == 0 { v3(r.f(4994.0, 5006.0), r.f(2.0, 6.0), r.f(4994.0, 5006.0)) } else { perches.last().map(|p: &GamePerch| p.pos).unwrap() };
        let want = if k % 3 == 0 { centre } else { centre.add(r.ball(0.7)) };
        let (go, t) = new_go(h, &format!("ts_check perch {k}"));
        let collider = add_component(h, go, "Placemaker.Props.PropCollider, Assembly-CSharp");
        let landing = add_component(h, go, "Placemaker.Life.BirdLanding, Assembly-CSharp");
        // (a PropCollider brings its PropNode along)
        let nt = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.Props.PropNode, Assembly-CSharp")], "param_types": ["System.String"], "main": true}), "type");
        let node = keep(h, call(go, "UnityEngine.GameObject", "GetComponent", vec![json!({"handle": nt})], &["System.Type"]), "PropNode");
        let rot = r.quat();
        ok(&h.request(call(t, TR, "set_position", vec![arg::raw(&v3b(want))], &["UnityEngine.Vector3"])), "perch position");
        ok(&h.request(call(t, TR, "set_rotation", vec![arg::raw(&qb(rot))], &["UnityEngine.Quaternion"])), "perch rotation");
        let pos = vec3(&h.request(call0(t, TR, "get_position")));
        let a = addr(h, landing);
        perches.push(GamePerch { go, t, landing, node, collider, pos, addr: a });
    }
    // the perches into the prop grid (2-unit cells), in order
    let mut cells: Vec<(Int3, Vec<usize>)> = vec![];
    for (k, p) in perches.iter().enumerate() {
        let c = Int3 { x: (p.pos.x / 2.0).floor() as i32, y: (p.pos.y / 2.0).floor() as i32, z: (p.pos.z / 2.0).floor() as i32 };
        match cells.iter_mut().find(|x| x.0 == c) {
            Some(x) => x.1.push(k),
            None => cells.push((c, vec![k])),
        }
    }
    for (c, ks) in &cells {
        let l = keep(h, json!({"new": COLLIDERS, "args": [], "main": true}), "collider list");
        for &k in ks {
            ok(&h.request(call(l, COLLIDERS, "Add", vec![json!({"handle": perches[k].collider})], &[])), "list Add");
        }
        let cb: Vec<u8> = [c.x, c.y, c.z].iter().flat_map(|x| x.to_le_bytes()).collect();
        ok(&h.request(call(dict, DICT, "Add", vec![arg::raw(&cb), json!({"handle": l})], &[])), "dict Add");
    }
    Setup {
        flock_go,
        flock,
        disabled,
        disabled_addr,
        list,
        dict,
        graph,
        camera,
        cam_pos,
        cam_fwd,
        bounds,
        bounds_height,
        meshes,
        mesh_addrs,
        shade_scale,
        birds,
        perches,
        cells,
    }
}

fn cleanup(h: &mut Harness, s: &Setup) {
    for (c, _) in &s.cells {
        let cb: Vec<u8> = [c.x, c.y, c.z].iter().flat_map(|x| x.to_le_bytes()).collect();
        let r = h.request(call(s.dict, DICT, "Remove", vec![arg::raw(&cb)], &["Unity.Mathematics.int3"]));
        assert_eq!(r["ok"], true, "dict Remove: {r}");
    }
    let mut gos: Vec<u64> = s.perches.iter().map(|p| p.go).collect();
    for b in &s.birds {
        gos.push(keep(h, call0(b.bird, "UnityEngine.Component", "get_gameObject"), "bird go"));
    }
    gos.push(s.flock_go);
    gos.push(keep(h, call0(s.disabled, "UnityEngine.Component", "get_gameObject"), "disabled go"));
    for g in gos {
        h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": g}], "param_types": ["UnityEngine.Object"], "main": true}));
    }
}

/// The game's world as the flock sees it: perches, the graph and the camera asked of the running game.
struct GameWorld<'a> {
    h: RefCell<&'a mut Harness>,
    s: &'a Setup,
    time: f32,
    dt: f32,
    turned_on: Vec<bool>,
    cache: RefCell<HashMap<(u8, [u32; 3]), Vec<f32>>>,
}
impl GameWorld<'_> {
    fn ask(&self, kind: u8, p: Vector3, req: Value) -> Vec<f32> {
        if let Some(v) = self.cache.borrow().get(&(kind, bits3(p))) {
            return v.clone();
        }
        let r = self.h.borrow_mut().request(req);
        ok(&r, "world query");
        let v = fs(&r);
        self.cache.borrow_mut().insert((kind, bits3(p)), v.clone());
        v
    }
    fn perch_t(&self, l: LandingId) -> u64 {
        self.s.perches[l].t
    }
}
impl BirdWorld for GameWorld<'_> {
    fn time(&self) -> f32 {
        self.time
    }
    fn colliders(&self, cell: Int3) -> Option<Vec<ColliderRef>> {
        let (_, ks) = self.s.cells.iter().find(|x| x.0 == cell)?;
        Some(ks.iter().map(|&k| ColliderRef { position: self.s.perches[k].pos, landing: Some(k), turned_on: self.turned_on[k] }).collect())
    }
    fn landing_position(&self, l: LandingId) -> Vector3 {
        self.s.perches[l].pos
    }
    fn landing_transform_point(&self, l: LandingId, p: Vector3) -> Vector3 {
        let v = self.ask(10 + l as u8, p, call(self.perch_t(l), TR, "TransformPoint", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"]));
        v3(v[0], v[1], v[2])
    }
    fn landing_rotation(&self, l: LandingId) -> Quaternion {
        let v = self.ask(100 + l as u8, Vector3::ZERO, call0(self.perch_t(l), TR, "get_rotation"));
        quat(v[0], v[1], v[2], v[3])
    }
    fn landing_up(&self, l: LandingId) -> Vector3 {
        let v = self.ask(200 + l as u8, Vector3::ZERO, call0(self.perch_t(l), TR, "get_up"));
        v3(v[0], v[1], v[2])
    }
    fn square_count(&self) -> i32 {
        panic!("TryGetRandomLanding (the round should not retarget through a perch)")
    }
    fn landings_in_square(&self, _i: i32) -> Vec<LandingId> {
        vec![]
    }
    fn play_sound(&mut self, _s: i32, _p: Vector3) {}
    fn feathers(&mut self, _p: Vector3, _r: Quaternion) {}
    fn delta_time(&self) -> f32 {
        self.dt
    }
    fn graph_bounds(&self) -> Int3x2 {
        self.s.bounds
    }
    fn graph_bounds_height(&self) -> i32 {
        self.s.bounds_height
    }
    fn sample_normal_coverage(&self, p: Vector3) -> (Vector3, f32) {
        let v = self.ask(1, p, call(self.s.graph, "Placemaker.Graphs.Graph", "SampleNormalCoverage", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"]));
        (v3(v[0], v[1], v[2]), v[3])
    }
    fn world_to_viewport_point(&self, p: Vector3) -> Vector3 {
        let v = self.ask(2, p, call(self.s.camera, "UnityEngine.Camera", "WorldToViewportPoint", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"]));
        v3(v[0], v[1], v[2])
    }
}

/// What a round sets.
struct RoundBird {
    root_scale: f32,
    pos_mid: Vector3,
    velocity_prev: Vector3,
    velocity_next: Vector3,
    pos: Vector3,
    time: f32,
    state_prev: u8,
    state_next: u8,
    last_action_time: f32,
    rotation0: Quaternion,
    rotation1: Quaternion,
    landing: Option<usize>,
    mesh0: usize,
    mesh1: usize,
    mf0_active: bool,
    shade_active: bool,
}
struct RoundPerch {
    bird: Option<usize>,
    neighbour_count: u8,
    normal: Vector3,
    turned_on: bool,
}

fn run(h: &mut Harness, s: &Setup) -> Vec<String> {
    let mut errs = vec![];
    let mut r = Rng(0xb1d5_f00d);
    let mut stats = [0usize; 6];
    for round in 0..ROUNDS {
        let now = ret::f32(&h.request(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true})));
        let nb = 2 + r.pick(BIRDS - 1);
        // perches: which bird sits where
        let mut perches: Vec<RoundPerch> = (0..PERCHES)
            .map(|_| {
                let normal = if r.chance(0.3) { v3(r.f(-0.2, 0.2), 1.0, r.f(-0.2, 0.2)).normalized() } else { r.ball(1.0).add(v3(0.0, 0.2, 0.0)).normalized() };
                RoundPerch { bird: None, neighbour_count: r.pick(4) as u8, normal, turned_on: r.chance(0.9) }
            })
            .collect();
        let mut free: Vec<usize> = (0..PERCHES).collect();
        let mut birds = vec![];
        for k in 0..nb {
            let state_next = [0u8, 0, 0, 1, 1, 2, 3, 3, 3, 4, 4][r.pick(11)];
            let perched = matches!(state_next, 1..=3) || (state_next == 0 && r.chance(0.3));
            let landing = if perched && !free.is_empty() { Some(free.remove(r.pick(free.len()))) } else { None };
            let state_next = if landing.is_none() && matches!(state_next, 1..=3) { 0 } else { state_next };
            let near_cam = state_next == 4 && r.chance(0.4);
            let pos_mid = match landing {
                Some(l) if state_next != 0 => {
                    let rad = if r.chance(if state_next == 1 { 0.6 } else { 0.3 }) { 0.25 } else { 1.6 };
                    s.perches[l].pos.add(r.ball(rad))
                }
                Some(l) => s.perches[l].pos.add(r.ball(4.0)),
                None if near_cam => s.cam_pos.add(s.cam_fwd.mul(r.f(8.0, 40.0))).add(r.ball(6.0)),
                None => {
                    // some fly at a perch
                    let p = s.perches[r.pick(PERCHES)].pos;
                    p.add(r.ball(2.5))
                }
            };
            let close = landing.is_some_and(|l| state_next == 1 && pos_mid.sub(s.perches[l].pos).magnitude() < 0.3);
            let velocity_next = if close {
                r.ball(0.05)
            } else if state_next == 0 && landing.is_none() && r.chance(0.5) {
                let p = s.perches[r.pick(PERCHES)].pos;
                p.sub(pos_mid).normalized().mul(r.f(0.3, 1.0))
            } else {
                r.ball(1.2)
            };
            if let Some(l) = landing {
                perches[l].bird = Some(k);
            }
            birds.push(RoundBird {
                root_scale: r.f(0.8, 1.2),
                pos_mid,
                velocity_prev: r.ball(1.2),
                velocity_next,
                pos: pos_mid.add(r.ball(0.5)),
                time: now - r.f(0.0, 0.7),
                state_prev: r.pick(5) as u8,
                state_next,
                last_action_time: now - r.f(0.0, 12.0),
                rotation0: r.quat(),
                rotation1: r.quat(),
                landing,
                mesh0: r.pick(7),
                mesh1: r.pick(7),
                mf0_active: r.chance(0.5),
                shade_active: r.chance(0.5),
            });
        }
        let order: Vec<usize> = {
            let mut o: Vec<usize> = (0..nb).collect();
            for i in (1..nb).rev() {
                o.swap(i, r.pick(i + 1));
            }
            o
        };
        let active = r.pick(14) as u8;
        let (min, max, ideal) = (r.pick(10) as u8, r.pick(12) as u8, r.pick(10) as u8);
        let landing_count = if r.chance(0.7) { PERCHES as i32 } else { 0 };
        let take_off = r.pick(3) as i32;
        let orbit_pos = v3(r.f(4990.0, 5010.0), r.f(0.0, 10.0), r.f(4990.0, 5010.0));
        let orbit_target = orbit_pos.add(r.ball(10.0));
        let state: [u32; 4] = [r.next(), r.next(), r.next(), r.next()];
        // a new orbit target only where it doesn't go through a random perch of the town
        let revived = {
            let mut a = active;
            let mut n = 0;
            if a < min {
                for &k in &order {
                    if birds[k].state_next == 4 {
                        n += 1;
                        a = a.wrapping_add(1);
                        if ideal <= a {
                            break;
                        }
                    }
                }
            }
            n
        };
        let retarget = {
            let mut g = Rand { s: state };
            for _ in 0..revived {
                g.value();
            }
            g.range_int(8, 20);
            g.value() >= 0.5
        } && r.chance(0.6);
        let last_orbit = if retarget { now - 1.0 } else { now + 100.0 };
        let assigned = r.pick(10) as u8 + 2;

        // the game
        let mut reqs = vec![call0(s.list, BIRD_LIST, "Clear")];
        for &k in &order {
            reqs.push(call(s.list, BIRD_LIST, "Add", vec![json!({"handle": s.birds[k].bird})], &[]));
        }
        for (k, b) in birds.iter().enumerate() {
            let g = &s.birds[k];
            reqs.push(call(g.root, TR, "SetParent", vec![json!({"null": true})], &["UnityEngine.Transform"]));
            reqs.push(call(g.root, TR, "set_position", vec![arg::raw(&v3b(Vector3::ZERO))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.root, TR, "set_rotation", vec![arg::raw(&qb(Quaternion::IDENTITY))], &["UnityEngine.Quaternion"]));
            reqs.push(call(g.root, TR, "set_localScale", vec![arg::raw(&v3b(v3(b.root_scale, b.root_scale, b.root_scale)))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.body, TR, "set_localPosition", vec![arg::raw(&v3b(Vector3::ZERO))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.body, TR, "set_localRotation", vec![arg::raw(&qb(Quaternion::IDENTITY))], &["UnityEngine.Quaternion"]));
            reqs.push(call(g.shade, TR, "set_position", vec![arg::raw(&v3b(Vector3::ZERO))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.shade, TR, "set_rotation", vec![arg::raw(&qb(Quaternion::IDENTITY))], &["UnityEngine.Quaternion"]));
            reqs.push(call(g.shade, TR, "set_localScale", vec![arg::raw(&v3b(s.shade_scale))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.mf0_t, TR, "set_localScale", vec![arg::raw(&v3b(v3(1.0, 1.0, 1.0)))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.mf1_t, TR, "set_localScale", vec![arg::raw(&v3b(v3(1.0, 1.0, 1.0)))], &["UnityEngine.Vector3"]));
            reqs.push(call(g.mf0, "UnityEngine.MeshFilter", "set_sharedMesh", vec![json!({"handle": s.meshes[b.mesh0]})], &["UnityEngine.Mesh"]));
            reqs.push(call(g.mf1, "UnityEngine.MeshFilter", "set_sharedMesh", vec![json!({"handle": s.meshes[b.mesh1]})], &["UnityEngine.Mesh"]));
            reqs.push(call(g.mf0_go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(b.mf0_active)], &["System.Boolean"]));
            reqs.push(call(g.shade_go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(b.shade_active)], &["System.Boolean"]));
            reqs.push(set(g.bird, "posMid", &v3b(b.pos_mid)));
            reqs.push(set(g.bird, "velocityPrev", &v3b(b.velocity_prev)));
            reqs.push(set(g.bird, "velocityNext", &v3b(b.velocity_next)));
            reqs.push(set(g.bird, "pos", &v3b(b.pos)));
            reqs.push(set(g.bird, "time", &b.time.to_le_bytes()));
            reqs.push(set(g.bird, "statePrev", &[b.state_prev]));
            reqs.push(set(g.bird, "stateNext", &[b.state_next]));
            reqs.push(set(g.bird, "lastActionTime", &b.last_action_time.to_le_bytes()));
            reqs.push(set(g.bird, "rotation0", &qb(b.rotation0)));
            reqs.push(set(g.bird, "rotation1", &qb(b.rotation1)));
            reqs.push(set_obj(g.bird, "landing", b.landing.map(|l| s.perches[l].landing)));
        }
        for (k, p) in perches.iter().enumerate() {
            let g = &s.perches[k];
            reqs.push(set_obj(g.landing, "bird", p.bird.map(|b| s.birds[b].bird)));
            reqs.push(set(g.landing, "neighbourCount", &[p.neighbour_count]));
            reqs.push(set(g.landing, "normal", &v3b(p.normal)));
            reqs.push(set(g.node, "turnedOn", &[p.turned_on as u8]));
        }
        let f = s.flock;
        reqs.push(set(f, "activeBirdsCount", &[active]));
        reqs.push(set(f, "minBirdCount", &[min]));
        reqs.push(set(f, "maxBirdCount", &[max]));
        reqs.push(set(f, "idealBirdCount", &[ideal]));
        reqs.push(set(f, "assignedBirdsCount", &[assigned]));
        reqs.push(set(f, "optimalBirdCountDirty", &[0]));
        reqs.push(set(f, "birdLandingCount", &landing_count.to_le_bytes()));
        reqs.push(set(f, "takeOffCount", &take_off.to_le_bytes()));
        reqs.push(set(f, "orbitPos", &v3b(orbit_pos)));
        reqs.push(set(f, "orbitTarget", &v3b(orbit_target)));
        reqs.push(set(f, "lastOrbitTime", &last_orbit.to_le_bytes()));
        let state_b: Vec<u8> = state.iter().flat_map(|w| w.to_le_bytes()).collect();
        reqs.push(native(SET_STATE, &state_b));
        reqs.push(call0(f, BF, "OnUpdate"));
        let n = reqs.len();
        reqs.push(native(GET_STATE, &[0u8; 16]));
        reqs.push(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true}));
        reqs.push(json!({"call": "UnityEngine.Time.get_deltaTime", "args": [], "main": true}));
        for f2 in ["activeBirdsCount", "assignedBirdsCount", "orbitPos", "orbitTarget", "lastOrbitTime", "takeOffCount"] {
            reqs.push(get(f, f2));
        }
        reqs.push(call0(s.list, BIRD_LIST, "get_Count"));
        let fields = ["posMid", "velocityPrev", "velocityNext", "pos", "time", "statePrev", "stateNext", "lastActionTime", "rotation0", "rotation1"];
        let per_bird = fields.len() + 13;
        let bird_at = reqs.len();
        for g in s.birds.iter().take(nb) {
            for fl in fields {
                reqs.push(get(g.bird, fl));
            }
            reqs.push(json!({"field": "landing", "handle": g.bird, "main": true}));
            reqs.push(json!({"call": "get_sharedMesh", "class": "UnityEngine.MeshFilter", "handle": g.mf0, "args": [], "main": true, "keep": true}));
            reqs.push(json!({"call": "get_sharedMesh", "class": "UnityEngine.MeshFilter", "handle": g.mf1, "args": [], "main": true, "keep": true}));
            reqs.push(call0(g.mf0_go, "UnityEngine.GameObject", "get_activeSelf"));
            reqs.push(call0(g.shade_go, "UnityEngine.GameObject", "get_activeSelf"));
            reqs.push(call0(g.body, TR, "get_position"));
            reqs.push(call0(g.body, TR, "get_rotation"));
            reqs.push(call0(g.shade, TR, "get_localScale"));
            reqs.push(call0(g.shade, TR, "get_position"));
            reqs.push(call0(g.shade, TR, "get_rotation"));
            reqs.push(call0(g.mf0_t, TR, "get_localScale"));
            reqs.push(call0(g.mf1_t, TR, "get_localScale"));
            reqs.push(json!({"call": "get_parent", "class": TR, "handle": g.root, "args": [], "main": true, "keep": true}));
        }
        let perch_at = reqs.len();
        for g in &s.perches {
            reqs.push(json!({"field": "bird", "handle": g.landing, "main": true}));
            reqs.push(get(g.landing, "neighbourCount"));
        }
        let list_at = reqs.len();
        for i in 0..nb {
            reqs.push(json!({"call": "get_Item", "class": BIRD_LIST, "handle": s.list, "args": [arg::i32(i as i32)], "main": true, "keep": true}));
        }
        let res = h.batch(reqs);
        for (i, x) in res.iter().enumerate().take(list_at) {
            ok(x, &format!("round {round} request {i}"));
        }
        let time = ret::f32(&res[n + 1]);
        let dt = ret::f32(&res[n + 2]);
        let gstate: Vec<u8> = ret::hex_field(&res[n]["bufs_after"][0]);
        // references back to our indices
        let mut addr_of = |v: &Value| -> Option<u64> { v.get("handle").and_then(Value::as_u64).map(|x| addr(h, x)) };
        let bird_index = |a: Option<u64>| a.and_then(|a| s.birds.iter().position(|b| b.addr == a));
        let perch_index = |a: Option<u64>| a.and_then(|a| s.perches.iter().position(|p| p.addr == a));
        let mesh_index = |a: Option<u64>| a.and_then(|a| s.mesh_addrs.iter().position(|&m| m == a));
        let count = ret::i32(&res[n + 9]) as usize;
        let mut glist = vec![];
        for i in 0..count {
            glist.push(bird_index(addr_of(&res[list_at + i])));
        }
        let mut gbirds = vec![];
        for k in 0..nb {
            let o = bird_at + k * per_bird;
            let nf = fields.len();
            gbirds.push((
                bits_of(&res[o..o + nf]),
                perch_index(addr_of(&res[o + nf])),
                mesh_index(addr_of(&res[o + nf + 1])),
                mesh_index(addr_of(&res[o + nf + 2])),
                ret::bytes(&res[o + nf + 3])[0] != 0,
                ret::bytes(&res[o + nf + 4])[0] != 0,
                res[o + nf + 5..o + nf + 12].iter().map(fs).collect::<Vec<_>>(),
                addr_of(&res[o + nf + 12]) == Some(s.disabled_addr),
            ));
        }
        let gperches: Vec<(Option<usize>, u8)> = (0..PERCHES).map(|j| (bird_index(addr_of(&res[perch_at + 2 * j])), ret::bytes(&res[perch_at + 2 * j + 1])[0])).collect();

        // ours
        let mut ours = BirdFlock::new(BirdPrefab::default());
        for (k, b) in birds.iter().enumerate() {
            let id = ours.instantiate_bird();
            assert_eq!(id, k);
            let t = &mut ours.transforms;
            let x = ours.all_birds[id];
            t.set_local_position(x.root, Vector3::ZERO);
            t.set_local_rotation(x.root, Quaternion::IDENTITY);
            t.set_local_scale(x.root, v3(b.root_scale, b.root_scale, b.root_scale));
            t.set_position(x.shade, Vector3::ZERO);
            t.set_rotation(x.shade, Quaternion::IDENTITY);
            t.set_local_scale(x.shade, s.shade_scale);
            let o = &mut ours.all_birds[id];
            o.pos_mid = b.pos_mid;
            o.velocity_prev = b.velocity_prev;
            o.velocity_next = b.velocity_next;
            o.pos = b.pos;
            o.time = b.time;
            o.state_prev = b.state_prev;
            o.state_next = b.state_next;
            o.last_action_time = b.last_action_time;
            o.rotation0 = b.rotation0;
            o.rotation1 = b.rotation1;
            o.landing = b.landing;
            o.mesh0 = MESHES[b.mesh0];
            o.mesh1 = MESHES[b.mesh1];
            o.mf0_active = b.mf0_active;
            o.shade_active = b.shade_active;
        }
        ours.birds = order.clone();
        for (k, p) in perches.iter().enumerate() {
            ours.landings.insert(k, BirdLanding { bird: p.bird, neighbour_count: p.neighbour_count, normal: p.normal });
        }
        ours.active_birds_count = active;
        ours.min_bird_count = min;
        ours.max_bird_count = max;
        ours.ideal_bird_count = ideal;
        ours.assigned_birds_count = assigned;
        ours.bird_landing_count = landing_count;
        ours.take_off_count = take_off;
        ours.orbit_pos = orbit_pos;
        ours.orbit_target = orbit_target;
        ours.last_orbit_time = last_orbit;
        let before: Vec<(u8, Option<usize>)> = birds.iter().map(|b| (b.state_next, b.landing)).collect();
        let mut rand = Rand { s: state };
        {
            let mut world = GameWorld { h: RefCell::new(&mut *h), s, time, dt, turned_on: perches.iter().map(|p| p.turned_on).collect(), cache: RefCell::new(HashMap::new()) };
            let mut log = |_: &str| {};
            ours.on_update(&mut world, &mut rand, &mut log);
        }
        let what = format!("round {round} ({nb} birds)");
        let perch_rotations: HashMap<usize, Quaternion> = {
            let w = GameWorld { h: RefCell::new(&mut *h), s, time, dt, turned_on: vec![], cache: RefCell::new(HashMap::new()) };
            birds.iter().filter_map(|b| b.landing).map(|l| (l, w.landing_rotation(l))).collect()
        };
        if std::env::var("BIRD_DEBUG").ok().as_deref() == Some(&round.to_string()) {
            for (k, b) in birds.iter().enumerate() {
                let o = ours.all_birds[k];
                eprintln!("bird {k}: state {} perch {:?} dt {} stepped {} -> state {} perch {:?} | game perch {:?}", b.state_next, b.landing, time - b.last_action_time, b.time + 0.3 < time, o.state_next, o.landing, gbirds[k].1);
                eprintln!("   rot1 ours {:?} game {:?} mesh {:?}", o.rotation1, gbirds[k].0[9].iter().map(|&x| f32::from_bits(x)).collect::<Vec<_>>(), o.mesh1);
                if let Some(l) = b.landing {
                    let w = GameWorld { h: RefCell::new(&mut *h), s, time, dt, turned_on: vec![], cache: RefCell::new(HashMap::new()) };
                    eprintln!("   perch {l}: normal {:?} up {:?}", perches[l].normal, w.landing_up(l));
                }
            }
            for (j, p) in perches.iter().enumerate() {
                eprintln!("perch {j}: pos {:?} on {} nc {} -> game {:?} ours {:?}", s.perches[j].pos, p.turned_on, p.neighbour_count, gperches[j], ours.landings.get(&j).map(|x| (x.bird, x.neighbour_count)));
            }
        }
        let mut check = |ok: bool, msg: String| {
            if !ok && errs.len() < 40 {
                errs.push(format!("{what}: {msg}"));
            }
        };
        check(gstate == rand.s.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>(), "Random state".into());
        check(ret::bytes(&res[n + 3])[0] == ours.active_birds_count, format!("activeBirdsCount {} vs {}", ret::bytes(&res[n + 3])[0], ours.active_birds_count));
        check(ret::bytes(&res[n + 4])[0] == ours.assigned_birds_count, "assignedBirdsCount".into());
        check(bits3(vec3(&res[n + 5])) == bits3(ours.orbit_pos), format!("orbitPos {:?} vs {:?}", vec3(&res[n + 5]), ours.orbit_pos));
        check(bits3(vec3(&res[n + 6])) == bits3(ours.orbit_target), format!("orbitTarget {:?} vs {:?}", vec3(&res[n + 6]), ours.orbit_target));
        check(ret::f32(&res[n + 7]).to_bits() == ours.last_orbit_time.to_bits(), "lastOrbitTime".into());
        check(ret::i32(&res[n + 8]) == ours.take_off_count, "takeOffCount".into());
        let olist: Vec<Option<usize>> = ours.birds.iter().map(|&b| Some(b)).collect();
        check(glist == olist, format!("birds {glist:?} vs {olist:?}"));
        for (k, g) in gbirds.iter().enumerate() {
            let o = ours.all_birds[k];
            let t = &ours.transforms;
            let mine = [
                bits3(o.pos_mid).to_vec(),
                bits3(o.velocity_prev).to_vec(),
                bits3(o.velocity_next).to_vec(),
                bits3(o.pos).to_vec(),
                vec![o.time.to_bits()],
                vec![o.state_prev as u32],
                vec![o.state_next as u32],
                vec![o.last_action_time.to_bits()],
                bitsq(o.rotation0).to_vec(),
                bitsq(o.rotation1).to_vec(),
            ];
            for (i, fl) in fields.iter().enumerate() {
                let extra = match (i, before[k].1) {
                    (9, Some(l)) => format!(
                        " normal {:?} perch rotation {:?} initial rotation1 {:?} time {} bird time {} dt {}",
                        perches[l].normal,
                        perch_rotations.get(&l),
                        birds[k].rotation1,
                        time,
                        birds[k].time,
                        time - birds[k].last_action_time
                    ),
                    _ => String::new(),
                };
                check(g.0[i] == mine[i], format!("bird {k} {fl}: {:?} vs {:?} (was state {}, perch {:?}){extra}", g.0[i], mine[i], before[k].0, before[k].1));
            }
            check(g.1 == o.landing, format!("bird {k} landing {:?} vs {:?}", g.1, o.landing));
            check(g.2.map(|m| MESHES[m]) == Some(o.mesh0), format!("bird {k} mesh0 {:?} vs {:?}", g.2.map(|m| MESHES[m]), o.mesh0));
            check(g.3.map(|m| MESHES[m]) == Some(o.mesh1), format!("bird {k} mesh1 {:?} vs {:?}", g.3.map(|m| MESHES[m]), o.mesh1));
            check(g.4 == o.mf0_active, format!("bird {k} mf0 active"));
            check(g.5 == o.shade_active, format!("bird {k} shade active"));
            let tv = [
                bits3(t.position(o.body)).to_vec(),
                bitsq(t.rotation(o.body)).to_vec(),
                bits3(t.local_scale(o.shade)).to_vec(),
                bits3(t.position(o.shade)).to_vec(),
                bitsq(t.rotation(o.shade)).to_vec(),
                bits3(t.local_scale(o.mf0)).to_vec(),
                bits3(t.local_scale(o.mf1)).to_vec(),
            ];
            let names = ["body position", "body rotation", "shade scale", "shade position", "shade rotation", "mf0 scale", "mf1 scale"];
            let listed = ours.birds.contains(&k);
            for i in 0..7 {
                if i < 2 && !listed {
                    continue; // (a bird that left is not placed)
                }
                let gv: Vec<u32> = g.6[i].iter().map(|x| x.to_bits()).collect();
                check(gv == tv[i], format!("bird {k} {}: {:?} vs {:?}", names[i], g.6[i], tv[i].iter().map(|&x| f32::from_bits(x)).collect::<Vec<_>>()));
            }
            check(g.7 == (t.nodes[o.root].parent == Some(ours.disabled_birds)), format!("bird {k} sent to the disabled birds"));
        }
        for (j, g) in gperches.iter().enumerate() {
            let o = ours.landings.get(&j).copied().unwrap_or_default();
            check(*g == (o.bird, o.neighbour_count), format!("perch {j}: {g:?} vs {:?}", (o.bird, o.neighbour_count)));
        }
        // coverage
        for (k, b) in before.iter().enumerate() {
            let o = ours.all_birds[k];
            if b.0 == 0 && o.state_next == 1 && b.1 != o.landing {
                stats[0] += 1; // took a perch ahead (GetLanding)
            }
            if b.0 == 3 && o.state_next == 2 {
                stats[1] += 1; // hopped
            }
            if b.0 == 1 && o.state_next == 2 {
                stats[2] += 1; // landed
            }
            if !ours.birds.contains(&k) {
                stats[3] += 1; // left
            }
        }
        if retarget {
            stats[4] += 1;
        }
        stats[5] += nb;
    }
    eprintln!(
        "{ROUNDS} updates, {} birds: {} perches taken in flight, {} hops, {} landings, {} birds gone, {} new orbit targets",
        stats[5], stats[0], stats[1], stats[2], stats[3], stats[4]
    );
    errs
}

fn bits_of(v: &[Value]) -> Vec<Vec<u32>> {
    v.iter()
        .map(|x| {
            let b = ret::bytes(x);
            if b.len() == 1 {
                vec![b[0] as u32]
            } else {
                b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
            }
        })
        .collect()
}

#[test]
fn bird_update_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let s = setup(h);
    let result = catch_unwind(AssertUnwindSafe(|| run(h, &s)));
    cleanup(h, &s);
    match result {
        Ok(errs) => assert!(errs.is_empty(), "{} mismatches:\n{}", errs.len(), errs.join("\n")),
        Err(e) => resume_unwind(e),
    }
}
