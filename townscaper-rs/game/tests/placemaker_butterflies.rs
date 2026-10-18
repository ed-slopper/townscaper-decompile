//! `ts_game::placemaker::life::butterfly_flock` (G5c) against the game (`TS_HARNESS=1`): a fresh `ButterflyFlock`
//! (the scene's master, meshes and `srcButterfly`) and 24 test flowers (`PropCollider` + `ButterflyLanding`, in
//! clusters: three far from the town, one by a perch of the town for the ground push) put into the prop placer's
//! collider grid. 300 rounds of one simulation run on both sides: each round some butterflies are disturbed
//! (clock, state, velocity), the flower count changed (as if the town had others), flowers turned off and on, `UnityEngine.Random` set to a known state, a flower
//! added or removed (or the flock reset), then `OnUpdate`; the generator's state, every butterfly's fields, mesh
//! and pose, the containers' child order and the flowers compared to the bit. The test objects and grid entries are
//! removed at the end.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::life::bird_flock::{ColliderRef, LandingId};
use ts_game::placemaker::life::butterfly_flock::{ButterflyFlock, ButterflyMesh, ButterflyPrefab, ButterflyWorld};
use ts_game::unity::mathematics::Int3;
use ts_game::unity::quaternion::{quat, Quaternion};
use ts_game::unity::random::Rand;
use ts_game::unity::vector::{v3, Vector3};

const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;
const BF: &str = "Placemaker.Life.ButterflyFlock";
const TR: &str = "UnityEngine.Transform";
const FLOWERS: usize = 24;
const ROUNDS: usize = 300;
const MESHES: [ButterflyMesh; 3] = [ButterflyMesh::Flap, ButterflyMesh::Sit, ButterflyMesh::SitFlap];
const MESH_FIELDS: [&str; 3] = ["flapMesh", "sitMesh", "sitFlapMesh"];
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
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}
fn native(va: u32, buf: &[u8]) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": [{"buf": hex(buf)}]})
}
fn new_go(h: &mut Harness, name: &str) -> (u64, u64) {
    let go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true}), "go");
    let t = keep(h, call0(go, "UnityEngine.GameObject", "get_transform"), "transform");
    (go, t)
}
fn type_of(h: &mut Harness, ty: &str) -> u64 {
    keep(h, json!({"call": "System.Type.GetType", "args": [arg::string(ty)], "param_types": ["System.String"], "main": true}), "type")
}
fn add_component(h: &mut Harness, go: u64, ty: &str) -> u64 {
    let t = type_of(h, ty);
    keep(h, call(go, "UnityEngine.GameObject", "AddComponent", vec![json!({"handle": t})], &["System.Type"]), "AddComponent")
}
fn cell_of(p: Vector3) -> Int3 {
    Int3 { x: (p.x / 2.0).floor() as i32, y: (p.y / 2.0).floor() as i32, z: (p.z / 2.0).floor() as i32 }
}
fn cell_bytes(c: Int3) -> Vec<u8> {
    [c.x, c.y, c.z].iter().flat_map(|x| x.to_le_bytes()).collect()
}

struct Flower {
    go: u64,
    t: u64,
    landing: u64,
    node: u64,
    collider: u64,
    pos: Vector3,
    addr: u64,
}

/// A butterfly of the game's, known by its object.
struct GameButterfly {
    bf: u64,
    bf_addr: u64,
    root: u64,
    mf: u64,
    id: usize,
}

struct Setup {
    flock_go: u64,
    flock: u64,
    active: u64,
    disabled: u64,
    dict: u64,
    graph: u64,
    mesh_addrs: Vec<u64>,
    flowers: Vec<Flower>,
    /// Grid cells this test made (removed at the end) and colliders it added to the town's cells.
    new_cells: Vec<Int3>,
    added: Vec<(u64, u64)>,
    cells: Vec<(Int3, Vec<usize>)>,
}

fn setup(h: &mut Harness) -> Setup {
    h.wait_for_object("Placemaker.WorldMaster");
    let scene = keep(h, json!({"field": "butterflyFlock", "on": "Placemaker.WorldMaster", "main": true}), "butterflyFlock");
    let master = keep(h, get(scene, "master"), "master");
    let (flock_go, _) = new_go(h, "ts_check butterflies");
    let flock = add_component(h, flock_go, &format!("{BF}, Assembly-CSharp"));
    ok(&h.request(json!({"field": "master", "handle": flock, "set_handle": master, "main": true})), "master");
    let mut mesh_addrs = vec![];
    for f in MESH_FIELDS {
        let m = keep(h, get(scene, f), f);
        ok(&h.request(json!({"field": f, "handle": flock, "set_handle": m, "main": true})), f);
        mesh_addrs.push(addr(h, m));
    }
    let src = keep(h, get(scene, "srcButterfly"), "srcButterfly");
    ok(&h.request(json!({"field": "srcButterfly", "handle": flock, "set_handle": src, "main": true})), "srcButterfly");
    let (_, active) = new_go(h, "ts_check active butterflies");
    let (_, disabled) = new_go(h, "ts_check disabled butterflies");
    ok(&h.request(json!({"field": "activeButterflies", "handle": flock, "set_handle": active, "main": true})), "active");
    ok(&h.request(json!({"field": "disabledButterflies", "handle": flock, "set_handle": disabled, "main": true})), "disabled");
    let graph = keep(h, get(master, "graph"), "graph");
    let placer = keep(h, get(master, "propPlacer"), "propPlacer");
    let dict = keep(h, get(placer, "colliderDict"), "colliderDict");
    // a place by the town: one of its birds' perches, else in front of the camera
    let near = {
        let bird_flock = keep(h, json!({"field": "flock", "on": "Placemaker.WorldMaster", "main": true}), "flock");
        let birds = keep(h, get(bird_flock, "birds"), "birds");
        let n = ret::i32(&h.request(call0(birds, BIRD_LIST, "get_Count")));
        let mut at = None;
        for i in 0..n {
            let b = keep(h, call(birds, BIRD_LIST, "get_Item", vec![arg::i32(i)], &[]), "bird");
            let l = h.request(get(b, "landing"));
            if let Some(l) = l.get("handle").and_then(Value::as_u64) {
                let t = keep(h, call0(l, "UnityEngine.Component", "get_transform"), "perch");
                at = Some(vec3(&h.request(call0(t, TR, "get_position"))));
                break;
            }
        }
        at.unwrap_or_else(|| {
            let cam = keep(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true}), "camera");
            let ct = keep(h, call0(cam, "UnityEngine.Component", "get_transform"), "camera transform");
            let p = vec3(&h.request(call0(ct, TR, "get_position")));
            let f = vec3(&h.request(call0(ct, TR, "get_forward")));
            p.add(f.mul(20.0))
        })
    };
    eprintln!("flowers by the town at {near:?}");
    let nt = type_of(h, "Placemaker.Props.PropNode, Assembly-CSharp");
    let mut r = Rng(0xf10e_5eed);
    let mut flowers: Vec<Flower> = vec![];
    for k in 0..FLOWERS {
        let want = if k % 6 == 0 {
            if k / 6 == 3 {
                near.add(r.ball(1.5))
            } else {
                v3(r.f(4994.0, 5006.0), r.f(2.0, 6.0), r.f(4994.0, 5006.0))
            }
        } else {
            flowers[k - k % 6].pos.add(r.ball(1.0))
        };
        let (go, t) = new_go(h, &format!("ts_check flower {k}"));
        let collider = add_component(h, go, "Placemaker.Props.PropCollider, Assembly-CSharp");
        let landing = add_component(h, go, "Placemaker.Life.ButterflyLanding, Assembly-CSharp");
        // (a PropCollider brings its PropNode along)
        let node = keep(h, call(go, "UnityEngine.GameObject", "GetComponent", vec![json!({"handle": nt})], &["System.Type"]), "PropNode");
        let rot = r.quat();
        ok(&h.request(call(t, TR, "set_position", vec![arg::raw(&v3b(want))], &["UnityEngine.Vector3"])), "flower position");
        ok(&h.request(call(t, TR, "set_rotation", vec![arg::raw(&qb(rot))], &["UnityEngine.Quaternion"])), "flower rotation");
        let pos = vec3(&h.request(call0(t, TR, "get_position")));
        let a = addr(h, landing);
        flowers.push(Flower { go, t, landing, node, collider, pos, addr: a });
    }
    // into the grid: the town's own cells get them appended, new cells are made
    let mut cells: Vec<(Int3, Vec<usize>)> = vec![];
    for (k, f) in flowers.iter().enumerate() {
        let c = cell_of(f.pos);
        match cells.iter_mut().find(|x| x.0 == c) {
            Some(x) => x.1.push(k),
            None => cells.push((c, vec![k])),
        }
    }
    let mut new_cells = vec![];
    let mut added = vec![];
    for (c, ks) in &cells {
        let has = ret::bytes(&h.request(call(dict, DICT, "ContainsKey", vec![arg::raw(&cell_bytes(*c))], &[])))[0] != 0;
        let l = if has {
            keep(h, call(dict, DICT, "get_Item", vec![arg::raw(&cell_bytes(*c))], &[]), "town cell")
        } else {
            let l = keep(h, json!({"new": COLLIDERS, "args": [], "main": true}), "collider list");
            ok(&h.request(call(dict, DICT, "Add", vec![arg::raw(&cell_bytes(*c)), json!({"handle": l})], &[])), "dict Add");
            new_cells.push(*c);
            l
        };
        for &k in ks {
            ok(&h.request(call(l, COLLIDERS, "Add", vec![json!({"handle": flowers[k].collider})], &[])), "list Add");
            if has {
                added.push((l, flowers[k].collider));
            }
        }
    }
    eprintln!("{} cells ({} of the town's)", cells.len(), cells.len() - new_cells.len());
    Setup { flock_go, flock, active, disabled, dict, graph, mesh_addrs, flowers, new_cells, added, cells }
}

fn cleanup(h: &mut Harness, s: &Setup) {
    for (l, c) in &s.added {
        let r = h.request(call(*l, COLLIDERS, "Remove", vec![json!({"handle": c})], &[]));
        assert_eq!(r["ok"], true, "list Remove: {r}");
    }
    for c in &s.new_cells {
        let r = h.request(call(s.dict, DICT, "Remove", vec![arg::raw(&cell_bytes(*c))], &["Unity.Mathematics.int3"]));
        assert_eq!(r["ok"], true, "dict Remove: {r}");
    }
    let mut gos: Vec<u64> = s.flowers.iter().map(|f| f.go).collect();
    gos.push(s.flock_go);
    for t in [s.active, s.disabled] {
        gos.push(keep(h, call0(t, "UnityEngine.Component", "get_gameObject"), "container"));
    }
    for g in gos {
        h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": g}], "param_types": ["UnityEngine.Object"], "main": true}));
    }
}

/// The game's world as the flock sees it.
struct GameWorld<'a> {
    h: RefCell<&'a mut Harness>,
    s: &'a Setup,
    time: f32,
    turned_on: Vec<bool>,
    cache: &'a RefCell<HashMap<(u16, [u32; 3]), Vec<f32>>>,
    pushes: &'a std::cell::Cell<usize>,
}
impl GameWorld<'_> {
    fn ask(&self, kind: u16, p: Vector3, req: Value) -> Vec<f32> {
        let key = (kind, [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]);
        if let Some(v) = self.cache.borrow().get(&key) {
            return v.clone();
        }
        let r = self.h.borrow_mut().request(req);
        ok(&r, "world query");
        let v = fs(&r);
        self.cache.borrow_mut().insert(key, v.clone());
        v
    }
}
impl ButterflyWorld for GameWorld<'_> {
    fn time(&self) -> f32 {
        self.time
    }
    fn colliders(&self, cell: Int3) -> Option<Vec<ColliderRef>> {
        let (_, ks) = self.s.cells.iter().find(|x| x.0 == cell)?;
        Some(ks.iter().map(|&k| ColliderRef { position: self.s.flowers[k].pos, landing: Some(k), turned_on: self.turned_on[k] }).collect())
    }
    fn landing_position(&self, l: LandingId) -> Vector3 {
        self.s.flowers[l].pos
    }
    fn landing_rotation(&self, l: LandingId) -> Quaternion {
        let v = self.ask(100 + l as u16, Vector3::ZERO, call0(self.s.flowers[l].t, TR, "get_rotation"));
        quat(v[0], v[1], v[2], v[3])
    }
    fn landing_up(&self, l: LandingId) -> Vector3 {
        let v = self.ask(200 + l as u16, Vector3::ZERO, call0(self.s.flowers[l].t, TR, "get_up"));
        v3(v[0], v[1], v[2])
    }
    fn landing_transform_point(&self, l: LandingId, p: Vector3) -> Vector3 {
        let v = self.ask(300 + l as u16, p, call(self.s.flowers[l].t, TR, "TransformPoint", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"]));
        v3(v[0], v[1], v[2])
    }
    fn landing_transform_vector(&self, l: LandingId, p: Vector3) -> Vector3 {
        let v = self.ask(400 + l as u16, p, call(self.s.flowers[l].t, TR, "TransformVector", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"]));
        v3(v[0], v[1], v[2])
    }
    fn sample_normal_coverage(&self, p: Vector3) -> (Vector3, f32) {
        let v = self.ask(1, p, call(self.s.graph, "Placemaker.Graphs.Graph", "SampleNormalCoverage", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"]));
        if v[3] > 0.3 {
            self.pushes.set(self.pushes.get() + 1);
        }
        (v3(v[0], v[1], v[2]), v[3])
    }
}

fn run(h: &mut Harness, s: &Setup) -> Vec<String> {
    let mut errs: Vec<String> = vec![];
    let mut r = Rng(0xb077_e7f1);
    let mut ours = ButterflyFlock::new(ButterflyPrefab::default());
    let mut known: Vec<GameButterfly> = vec![];
    let mut by_addr: HashMap<u64, usize> = HashMap::new();
    let mut added = vec![false; FLOWERS];
    let cache = RefCell::new(HashMap::new());
    let pushes = std::cell::Cell::new(0);
    let mut stats = [0usize; 8];
    for round in 0..ROUNDS {
        if errs.len() > 30 {
            break;
        }
        let now = ret::f32(&h.request(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true})));
        let mut reqs = vec![];
        // disturb the active butterflies
        let active_ids: Vec<usize> = (0..ours.transforms.child_count(ours.active)).map(|i| ours.of_root(ours.transforms.child(ours.active, i))).collect();
        for &id in &active_ids {
            let g = known.iter().find(|g| g.id == id).expect("known");
            let bf = &mut ours.all[id];
            bf.time = now - r.f(0.0, 0.6);
            reqs.push(set(g.bf, "time", &bf.time.to_le_bytes()));
            if r.chance(0.25) {
                bf.state = r.pick(3) as u8;
                reqs.push(set(g.bf, "state", &[bf.state]));
            }
            if r.chance(0.25) {
                bf.velocity_next = r.ball(0.5);
                reqs.push(set(g.bf, "velocityNext", &v3b(bf.velocity_next)));
            }
        }
        // as if the town had other flowers: more butterflies wanted, or fewer
        if r.chance(0.15) {
            ours.landing_count = r.pick(500) as i32;
            reqs.push(set(s.flock, "landingCount", &ours.landing_count.to_le_bytes()));
        }
        let turned_on: Vec<bool> = (0..FLOWERS).map(|_| r.chance(0.9)).collect();
        for (k, f) in s.flowers.iter().enumerate() {
            reqs.push(set(f.node, "turnedOn", &[turned_on[k] as u8]));
        }
        let state: [u32; 4] = [r.next(), r.next(), r.next(), r.next()];
        let state_b: Vec<u8> = state.iter().flat_map(|w| w.to_le_bytes()).collect();
        reqs.push(native(SET_STATE, &state_b));
        // one flower added or removed, or the flock reset
        let free: Vec<usize> = (0..FLOWERS).filter(|&k| !added[k]).collect();
        let on: Vec<usize> = (0..FLOWERS).filter(|&k| added[k]).collect();
        let op = if r.chance(0.01) {
            Some((2, 0))
        } else if !free.is_empty() && r.chance(0.35) {
            Some((0, free[r.pick(free.len())]))
        } else if !on.is_empty() && r.chance(0.15) {
            Some((1, on[r.pick(on.len())]))
        } else {
            None
        };
        match op {
            Some((0, k)) => reqs.push(call(s.flock, BF, "AddLanding", vec![json!({"handle": s.flowers[k].landing})], &[])),
            Some((1, k)) => reqs.push(call(s.flock, BF, "RemoveLanding", vec![json!({"handle": s.flowers[k].landing})], &[])),
            Some(_) => reqs.push(call0(s.flock, BF, "OnReset")),
            None => {}
        }
        reqs.push(call0(s.flock, BF, "OnUpdate"));
        let n = reqs.len();
        reqs.push(native(GET_STATE, &[0u8; 16]));
        reqs.push(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true}));
        reqs.push(get(s.flock, "landingCount"));
        let res = h.batch(reqs);
        for (i, x) in res.iter().enumerate() {
            ok(x, &format!("round {round} request {i}"));
        }
        let time = ret::f32(&res[n + 1]);
        let gstate: Vec<u8> = ret::hex_field(&res[n]["bufs_after"][0]);
        // ours
        let mut created = vec![];
        {
            let world = GameWorld { h: RefCell::new(&mut *h), s, time, turned_on: turned_on.clone(), cache: &cache, pushes: &pushes };
            let mut rand = Rand { s: state };
            let mut log = |_: &str| {};
            let before = ours.all.len();
            match op {
                Some((0, k)) => {
                    ours.add_landing(&world, &mut rand, &mut log, k);
                    added[k] = true;
                    stats[0] += 1;
                }
                Some((1, k)) => {
                    ours.remove_landing(&mut log, k);
                    added[k] = false;
                    stats[1] += 1;
                }
                Some(_) => {
                    ours.on_reset();
                    stats[2] += 1;
                }
                None => {}
            }
            created.extend(before..ours.all.len());
            let states: Vec<(u8, Option<usize>)> = ours.all.iter().map(|b| (b.state, b.landing)).collect();
            ours.on_update(&world, &mut rand, &mut log);
            for (b, (s0, l0)) in ours.all.iter().zip(states) {
                if s0 == 1 && b.state == 2 {
                    stats[3] += 1; // settled on its flower
                }
                if s0 == 0 && b.landing != l0 && l0.is_some() {
                    stats[4] += 1; // picked another flower
                }
                if s0 == 2 && b.state == 0 {
                    stats[5] += 1; // took off
                }
            }
            let g: Vec<u8> = rand.s.iter().flat_map(|w| w.to_le_bytes()).collect();
            if g != gstate {
                errs.push(format!("round {round}: Random state (op {op:?})"));
            }
        }
        // the containers' children, new butterflies matched to ours
        let mut lists = vec![];
        for t in [s.active, s.disabled] {
            let c = ret::i32(&h.request(call0(t, TR, "get_childCount"))) as usize;
            let mut ids = vec![];
            for i in 0..c {
                let child = keep(h, call(t, TR, "GetChild", vec![arg::i32(i as i32)], &[]), "child");
                let a = addr(h, child);
                let id = match by_addr.get(&a) {
                    Some(&id) => id,
                    None => {
                        let id = if created.is_empty() { usize::MAX } else { created.remove(0) };
                        let bt = type_of(h, "Placemaker.Life.Butterfly, Assembly-CSharp");
                        let bf = keep(h, call(child, "UnityEngine.Component", "GetComponent", vec![json!({"handle": bt})], &["System.Type"]), "Butterfly");
                        let mt = type_of(h, "UnityEngine.MeshFilter, UnityEngine.CoreModule");
                        // (the body, its only child, has the mesh)
                        let body = keep(h, call(child, TR, "GetChild", vec![arg::i32(0)], &[]), "body");
                        let mf = keep(h, call(body, "UnityEngine.Component", "GetComponent", vec![json!({"handle": mt})], &["System.Type"]), "MeshFilter");
                        by_addr.insert(a, id);
                        let bf_addr = addr(h, bf);
                        known.push(GameButterfly { bf, bf_addr, root: child, mf, id });
                        id
                    }
                };
                ids.push(id);
            }
            lists.push(ids);
        }
        let olists: Vec<Vec<usize>> = [ours.active, ours.disabled]
            .iter()
            .map(|&t| (0..ours.transforms.child_count(t)).map(|i| ours.of_root(ours.transforms.child(t, i))).collect())
            .collect();
        let what = format!("round {round} (op {op:?})");
        if lists != olists {
            errs.push(format!("{what}: containers {lists:?} vs {olists:?}"));
            break;
        }
        stats[6] = stats[6].max(lists[0].len());
        if ret::i32(&res[n + 2]) != ours.landing_count {
            errs.push(format!("{what}: landingCount"));
        }
        // every butterfly and flower
        let fields = ["posMid", "velocityPrev", "velocityNext", "pos", "time", "state", "rotation0", "rotation1"];
        let mut reqs = vec![];
        for g in &known {
            for f in fields {
                reqs.push(get(g.bf, f));
            }
            reqs.push(json!({"field": "landing", "handle": g.bf, "main": true}));
            reqs.push(json!({"call": "get_sharedMesh", "class": "UnityEngine.MeshFilter", "handle": g.mf, "args": [], "main": true, "keep": true}));
            reqs.push(call0(g.root, TR, "get_position"));
            reqs.push(call0(g.root, TR, "get_rotation"));
        }
        for f in &s.flowers {
            reqs.push(json!({"field": "butterfly", "handle": f.landing, "main": true}));
        }
        let res = h.batch(reqs);
        let per = fields.len() + 4;
        for (j, g) in known.iter().enumerate() {
            let o = ours.all[g.id];
            let t = &ours.transforms;
            let x = &res[j * per..];
            let mine: [Vec<f32>; 8] = [
                vec![o.pos_mid.x, o.pos_mid.y, o.pos_mid.z],
                vec![o.velocity_prev.x, o.velocity_prev.y, o.velocity_prev.z],
                vec![o.velocity_next.x, o.velocity_next.y, o.velocity_next.z],
                vec![o.pos.x, o.pos.y, o.pos.z],
                vec![o.time],
                vec![],
                vec![o.rotation0.x, o.rotation0.y, o.rotation0.z, o.rotation0.w],
                vec![o.rotation1.x, o.rotation1.y, o.rotation1.z, o.rotation1.w],
            ];
            for (i, f) in fields.iter().enumerate() {
                if i == 5 {
                    if ret::bytes(&x[i])[0] != o.state {
                        errs.push(format!("{what}: butterfly {} state {} vs {}", g.id, ret::bytes(&x[i])[0], o.state));
                    }
                    continue;
                }
                let gv = fs(&x[i]);
                if bits(&gv) != bits(&mine[i]) {
                    errs.push(format!("{what}: butterfly {} {f}: {gv:?} vs {:?} (state {}, flower {:?})", g.id, mine[i], o.state, o.landing));
                }
            }
            let nf = fields.len();
            let gl = x[nf].get("handle").and_then(Value::as_u64).map(|hd| addr(h, hd)).and_then(|a| s.flowers.iter().position(|f| f.addr == a));
            if gl != o.landing {
                errs.push(format!("{what}: butterfly {} flower {gl:?} vs {:?}", g.id, o.landing));
            }
            let gm = x[nf + 1].get("handle").and_then(Value::as_u64).map(|hd| addr(h, hd)).and_then(|a| s.mesh_addrs.iter().position(|&m| m == a)).map(|i| MESHES[i]);
            if gm != Some(o.mesh) {
                errs.push(format!("{what}: butterfly {} mesh {gm:?} vs {:?}", g.id, o.mesh));
            }
            let gp = fs(&x[nf + 2]);
            let op = t.position(o.root);
            if bits(&gp) != bits(&[op.x, op.y, op.z]) {
                errs.push(format!("{what}: butterfly {} position {gp:?} vs {op:?}", g.id));
            }
            let gq = fs(&x[nf + 3]);
            let oq = t.rotation(o.root);
            if bits(&gq) != bits(&[oq.x, oq.y, oq.z, oq.w]) {
                errs.push(format!("{what}: butterfly {} rotation {gq:?} vs {oq:?}", g.id));
            }
        }
        let base = known.len() * per;
        for k in 0..s.flowers.len() {
            let gb = res[base + k].get("handle").and_then(Value::as_u64).map(|hd| addr(h, hd)).and_then(|a| known.iter().find(|g| g.bf_addr == a).map(|g| g.id));
            let ob = ours.landings.get(&k).and_then(|x| x.butterfly);
            if gb != ob {
                errs.push(format!("{what}: flower {k} butterfly {gb:?} vs {ob:?}"));
            }
        }
        stats[7] += lists[0].len();
    }
    eprintln!(
        "{ROUNDS} updates: {} flowers added, {} removed, {} resets; {} settlings, {} flower changes, {} take-offs, {} ground pushes; up to {} butterflies, {} butterfly-steps",
        stats[0], stats[1], stats[2], stats[3], stats[4], stats[5], pushes.get(), stats[6], stats[7]
    );
    errs
}

#[test]
fn butterflies_match_the_game() {
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
