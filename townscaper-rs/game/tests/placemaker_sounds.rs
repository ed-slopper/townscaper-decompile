//! `ts_game::placemaker::audio::many_sounds_manager` and `audio_source_pool` (G6a) against the game
//! (`TS_HARNESS=1`): a fresh `ManySoundsManager` over clones of the scene's 13 `ManySound`s, playing on clones
//! of its 3 pools (the listener muted meanwhile). 300 rounds: each sound's sums, clock and damping, the pools'
//! ring positions and the request counter set at random, `UnityEngine.Random` set to a known state, up to 12
//! `PlaySound` requests (on and off screen, every type and two without a sound), then `OnUpdate`; the generator's
//! state, every sound's fields, the counter, the rings and every source played (volume, pitch, pan, position,
//! clip) compared to the bit.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::audio::many_sounds_manager::{AudioSources, ClipId, ManySound, ManySoundsManager, SoundHost, SourceId};
use ts_game::placemaker::audio_source_pool::AudioSourcePool;
use ts_game::unity::geometry::Ray;
use ts_game::unity::random::Rand;
use ts_game::unity::vector::{v3, Vector3};

const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;
const MSM: &str = "Placemaker.Audio.ManySoundsManager";
const TR: &str = "UnityEngine.Transform";
const AS: &str = "UnityEngine.AudioSource";
const ROUNDS: usize = 300;

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
fn fs(v: &Value) -> Vec<f32> {
    ret::bytes(v).chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn vec3(v: &Value) -> Vector3 {
    let f = fs(v);
    v3(f[0], f[1], f[2])
}
fn native(va: u32, buf: &[u8]) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": [{"buf": hex(buf)}]})
}
fn type_of(h: &mut Harness, ty: &str) -> u64 {
    keep(h, json!({"call": "System.Type.GetType", "args": [arg::string(ty)], "param_types": ["System.String"], "main": true}), "type")
}
fn child(h: &mut Harness, t: u64, i: i32) -> u64 {
    keep(h, call(t, TR, "GetChild", vec![arg::i32(i)], &[]), "child")
}
fn go_of(h: &mut Harness, c: u64) -> u64 {
    keep(h, call0(c, "UnityEngine.Component", "get_gameObject"), "gameObject")
}
fn transform_of(h: &mut Harness, go: u64) -> u64 {
    keep(h, call0(go, "UnityEngine.GameObject", "get_transform"), "transform")
}
fn listener_volume(h: &mut Harness, v: f32) {
    ok(&h.request(json!({"icall": "UnityEngine.AudioListener::set_volume(System.Single)", "args": [{"u32": v.to_bits()}], "main": true})), "AudioListener.volume");
}

struct GameSound {
    name: String,
    ms: u64,
    pool: usize,
    clips: Vec<u64>,
    settings: [f32; 4],
}
struct GamePool {
    pool: u64,
    count: i32,
    sources: Vec<(u64, u64)>, // (AudioSource, transform)
}
struct Setup {
    root_go: u64,
    manager: u64,
    camera: u64,
    cam_pos: Vector3,
    cam_fwd: Vector3,
    graph: u64,
    master: u64,
    click: u64,
    sounds: Vec<GameSound>,
    pools: Vec<GamePool>,
}

fn setup(h: &mut Harness) -> Setup {
    h.wait_for_object("Placemaker.WorldMaster");
    let scene = keep(h, json!({"field": "manySoundsManager", "on": "Placemaker.WorldMaster", "main": true}), "manySoundsManager");
    let master = keep(h, json!({"field": "master", "handle": scene, "main": true}), "master");
    let graph = keep(h, get(master, "graph"), "graph");
    let click = keep(h, get(master, "clickEffect"), "clickEffect");
    let st = keep(h, call0(scene, "UnityEngine.Component", "get_transform"), "manager transform");
    let (scene_sounds, scene_pools) = (child(h, st, 0), child(h, st, 1));
    let root_go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check sounds")], "param_types": ["System.String"], "main": true}), "root");
    let root = transform_of(h, root_go);
    let mut groups = vec![];
    for name in ["ManySounds", "Pools"] {
        let g = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true}), "group");
        let t = transform_of(h, g);
        ok(&h.request(call(t, TR, "SetParent", vec![json!({"handle": root}), arg::bool(false)], &["UnityEngine.Transform", "System.Boolean"])), "SetParent");
        groups.push(t);
    }
    let instantiate = |h: &mut Harness, src: u64, parent: u64| -> u64 {
        let go = go_of(h, src);
        let name = ret::string(&h.request(call0(go, "UnityEngine.Object", "get_name")));
        let clone = keep(h, json!({"call": "UnityEngine.Object.Instantiate", "args": [{"handle": go}], "param_types": ["UnityEngine.Object"], "main": true}), "Instantiate");
        ok(&h.request(call(clone, "UnityEngine.Object", "set_name", vec![arg::string(&name)], &["System.String"])), "set_name");
        let t = transform_of(h, clone);
        ok(&h.request(call(t, TR, "SetParent", vec![json!({"handle": parent}), arg::bool(false)], &["UnityEngine.Transform", "System.Boolean"])), "SetParent");
        clone
    };
    // the pools first (the sounds point at them)
    let pt = type_of(h, "Placemaker.AudioSourcePool, Assembly-CSharp");
    let at = type_of(h, "UnityEngine.AudioSource, UnityEngine.AudioModule");
    let mut pools = vec![];
    let mut pool_of: HashMap<u64, usize> = HashMap::new();
    for i in 0..3 {
        let src_t = child(h, scene_pools, i);
        let src_pool = keep(h, call(src_t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": pt})], &["System.Type"]), "pool");
        let a = addr(h, src_pool);
        let clone = instantiate(h, src_t, groups[1]);
        let pool = keep(h, call(clone, "UnityEngine.GameObject", "GetComponent", vec![json!({"handle": pt})], &["System.Type"]), "pool");
        let count = ret::i32(&h.request(get(pool, "count")));
        let ct = transform_of(h, clone);
        let mut sources = vec![];
        for j in 0..count {
            let c = child(h, ct, j);
            let s = keep(h, call(c, "UnityEngine.Component", "GetComponent", vec![json!({"handle": at})], &["System.Type"]), "AudioSource");
            sources.push((s, c));
        }
        pool_of.insert(a, pools.len());
        pools.push(GamePool { pool, count, sources });
    }
    let mt = type_of(h, "Placemaker.Audio.ManySound, Assembly-CSharp");
    let mut sounds = vec![];
    let n = ret::i32(&h.request(call0(scene_sounds, TR, "get_childCount")));
    for i in 0..n {
        let src_t = child(h, scene_sounds, i);
        let src_go = go_of(h, src_t);
        let name = ret::string(&h.request(call0(src_go, "UnityEngine.Object", "get_name")));
        let src_ms = keep(h, call(src_t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": mt})], &["System.Type"]), "ManySound");
        let src_pool = keep(h, get(src_ms, "audioSourcePool"), "pool");
        let pool = pool_of[&addr(h, src_pool)];
        let clone = instantiate(h, src_t, groups[0]);
        let ms = keep(h, call(clone, "UnityEngine.GameObject", "GetComponent", vec![json!({"handle": mt})], &["System.Type"]), "ManySound");
        ok(&h.request(json!({"field": "audioSourcePool", "handle": ms, "set_handle": pools[pool].pool, "main": true})), "audioSourcePool");
        let arr = keep(h, get(ms, "clips"), "clips");
        let len = ret::i32(&h.request(call0(arr, "System.Array", "get_Length")));
        let mut clips = vec![];
        for k in 0..len {
            let c = keep(h, call(arr, "System.Array", "GetValue", vec![arg::i32(k)], &["System.Int32"]), "clip");
            clips.push(addr(h, c));
        }
        let f = |h: &mut Harness, k: &str| ret::f32(&h.request(get(ms, k)));
        let settings = [f(h, "timeSpacing"), f(h, "clickSoundSpacing"), f(h, "attenuator"), f(h, "usePropPitch")];
        sounds.push(GameSound { name, ms, pool, clips, settings });
    }
    let mgt = type_of(h, &format!("{MSM}, Assembly-CSharp"));
    let manager = keep(h, call(root_go, "UnityEngine.GameObject", "AddComponent", vec![json!({"handle": mgt})], &["System.Type"]), "AddComponent");
    ok(&h.request(json!({"field": "master", "handle": manager, "set_handle": master, "main": true})), "master");
    ok(&h.request(call0(manager, MSM, "OnEnable")), "OnEnable");
    let camera = keep(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true}), "camera");
    let ct = keep(h, call0(camera, "UnityEngine.Component", "get_transform"), "camera transform");
    let cam_pos = vec3(&h.request(call0(ct, TR, "get_position")));
    let cam_fwd = vec3(&h.request(call0(ct, TR, "get_forward")));
    Setup { root_go, manager, camera, cam_pos, cam_fwd, graph, master, click, sounds, pools }
}

/// The game's world, asked lazily (the camera, the graph and the physics scene do not change).
struct GameHost<'a> {
    h: RefCell<&'a mut Harness>,
    s: &'a Setup,
    time: f32,
    dt: f32,
    dimmed: bool,
    last_click: f32,
    slowmo: f32,
    cache: &'a RefCell<HashMap<(u8, Vec<u32>), Vec<u8>>>,
    logs: Vec<String>,
}
impl GameHost<'_> {
    fn ask(&self, kind: u8, key: &[f32], req: Value) -> Vec<u8> {
        let k = (kind, key.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        if let Some(v) = self.cache.borrow().get(&k) {
            return v.clone();
        }
        let r = self.h.borrow_mut().request(req);
        ok(&r, "host query");
        let v = ret::bytes(&r);
        self.cache.borrow_mut().insert(k, v.clone());
        v
    }
}
fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
impl SoundHost for GameHost<'_> {
    fn time(&self) -> f32 {
        self.time
    }
    fn delta_time(&self) -> f32 {
        self.dt
    }
    fn has_main_camera(&self) -> bool {
        true
    }
    fn world_to_viewport_point(&self, p: Vector3) -> Vector3 {
        let v = f32s(&self.ask(1, &[p.x, p.y, p.z], call(self.s.camera, "UnityEngine.Camera", "WorldToViewportPoint", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"])));
        v3(v[0], v[1], v[2])
    }
    fn viewport_point_to_ray(&self, p: Vector3) -> Ray {
        let v = f32s(&self.ask(2, &[p.x, p.y, p.z], call(self.s.camera, "UnityEngine.Camera", "ViewportPointToRay", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"])));
        Ray { origin: v3(v[0], v[1], v[2]), direction: v3(v[3], v[4], v[5]) }
    }
    fn raycast(&self, ray: Ray, max_distance: f32, layer_mask: i32) -> bool {
        let o = ray.origin;
        let d = ray.direction;
        let raw: Vec<u8> = [o.x, o.y, o.z, d.x, d.y, d.z].iter().flat_map(|x| x.to_le_bytes()).collect();
        let b = self.ask(
            3,
            &[o.x, o.y, o.z, d.x, d.y, d.z, max_distance, f32::from_bits(layer_mask as u32)],
            json!({"call": "UnityEngine.Physics.Raycast", "args": [arg::raw(&raw), arg::f32(max_distance), arg::i32(layer_mask)], "param_types": ["UnityEngine.Ray", "System.Single", "System.Int32"], "main": true}),
        );
        b[0] != 0
    }
    fn sample_normal_coverage(&self, p: Vector3) -> (Vector3, f32) {
        let v = f32s(&self.ask(4, &[p.x, p.y, p.z], call(self.s.graph, "Placemaker.Graphs.Graph", "SampleNormalCoverage", vec![arg::raw(&v3b(p))], &["UnityEngine.Vector3"])));
        (v3(v[0], v[1], v[2]), v[3])
    }
    fn is_dimmed(&self) -> bool {
        self.dimmed
    }
    fn last_click(&self) -> f32 {
        self.last_click
    }
    fn slowmo_scale(&self) -> f32 {
        self.slowmo
    }
    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }
}

/// Our audio sources: what was set on each.
#[derive(Default)]
struct Sources {
    set: HashMap<SourceId, (f32, f32, f32, Vector3, Option<ClipId>, bool)>,
}
impl Sources {
    fn e(&mut self, s: SourceId) -> &mut (f32, f32, f32, Vector3, Option<ClipId>, bool) {
        self.set.entry(s).or_insert((f32::NAN, f32::NAN, f32::NAN, Vector3::ZERO, None, false))
    }
}
impl AudioSources for Sources {
    fn set_volume(&mut self, s: SourceId, v: f32) {
        self.e(s).0 = v;
    }
    fn set_pitch(&mut self, s: SourceId, p: f32) {
        // the engine keeps the old pitch when given one that is not finite
        if p.is_finite() {
            self.e(s).1 = p;
        }
    }
    fn set_pan_stereo(&mut self, s: SourceId, p: f32) {
        self.e(s).2 = p;
    }
    fn set_position(&mut self, s: SourceId, p: Vector3) {
        self.e(s).3 = p;
    }
    fn set_clip(&mut self, s: SourceId, c: Option<ClipId>) {
        self.e(s).4 = c;
    }
    fn play(&mut self, s: SourceId) {
        self.e(s).5 = true;
    }
    fn stop(&mut self, s: SourceId) {
        self.e(s).5 = false;
    }
}

fn run(h: &mut Harness, s: &Setup) -> Vec<String> {
    let mut errs = vec![];
    let mut r = Rng(0x50_0d5);
    let cache = RefCell::new(HashMap::new());
    let mut ours = ManySoundsManager::default();
    ours.on_enable(
        s.sounds
            .iter()
            .map(|g| (g.name.clone(), ManySound::new(g.pool, g.clips.iter().map(|&c| c as usize).collect(), g.settings[0], g.settings[1], g.settings[2], g.settings[3])))
            .collect(),
    );
    let mut pools: Vec<AudioSourcePool> = s.pools.iter().enumerate().map(|(p, g)| AudioSourcePool { index: 0, count: g.count, sources: (0..g.count as usize).map(|j| p * 100 + j).collect() }).collect();
    let mut stats = [0usize; 4];
    for round in 0..ROUNDS {
        if errs.len() > 30 {
            break;
        }
        let now = ret::f32(&h.request(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true})));
        let mut reqs = vec![];
        for (k, g) in s.sounds.iter().enumerate() {
            let (_, m) = ours.sounds.iter_mut().find(|(t, _)| ts_game::placemaker::audio::many_sounds_manager::sound_type::NAMES[*t as usize] == g.name).expect("ours");
            let _ = k;
            m.denom = if r.chance(0.6) { 0.0 } else { r.f(0.0, 3.0) };
            m.pos = s.cam_pos.add(s.cam_fwd.mul(r.f(5.0, 40.0))).add(r.ball(5.0)).mul(m.denom);
            m.pan = r.f(0.0, 1.0) * m.denom;
            m.pitch = r.f(-1.0, 1.0);
            m.volume = r.f(0.0, 1.0);
            m.last_time_played = now + r.f(-0.4, 0.2);
            m.attenuation = r.f(0.1, 1.0);
            reqs.push(set(g.ms, "pos", &v3b(m.pos)));
            reqs.push(set(g.ms, "denom", &m.denom.to_le_bytes()));
            reqs.push(set(g.ms, "pan", &m.pan.to_le_bytes()));
            reqs.push(set(g.ms, "pitch", &m.pitch.to_le_bytes()));
            reqs.push(set(g.ms, "volume", &m.volume.to_le_bytes()));
            reqs.push(set(g.ms, "lastTimePlayed", &m.last_time_played.to_le_bytes()));
            reqs.push(set(g.ms, "attenuation", &m.attenuation.to_le_bytes()));
        }
        for (p, g) in s.pools.iter().enumerate() {
            pools[p].index = r.pick(g.count as usize) as i32;
            reqs.push(set(g.pool, "index", &pools[p].index.to_le_bytes()));
        }
        ours.counter = r.pick(56) as i32;
        reqs.push(set(s.manager, "counter", &ours.counter.to_le_bytes()));
        let state: [u32; 4] = [r.next(), r.next(), r.next(), r.next()];
        reqs.push(native(SET_STATE, &state.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>()));
        let mut asks = vec![];
        for _ in 0..r.pick(13) {
            let t = r.pick(15) as i32;
            let pos = if r.chance(0.8) { s.cam_pos.add(s.cam_fwd.mul(r.f(5.0, 40.0))).add(r.ball(8.0)) } else { s.cam_pos.add(r.ball(200.0)) };
            let (pitch, volume) = (r.f(-0.5, 0.5), r.f(0.0, 1.5));
            reqs.push(call(s.manager, MSM, "PlaySound", vec![arg::i32(t), arg::raw(&v3b(pos)), arg::f32(pitch), arg::f32(volume)], &["Placemaker.Audio.ManySoundsManager.SoundType", "UnityEngine.Vector3", "System.Single", "System.Single"]));
            asks.push((t, pos, pitch, volume));
        }
        reqs.push(call0(s.manager, MSM, "OnUpdate"));
        let n = reqs.len();
        reqs.push(native(GET_STATE, &[0u8; 16]));
        for q in ["get_time", "get_deltaTime"] {
            reqs.push(json!({"call": format!("UnityEngine.Time.{q}"), "args": [], "main": true}));
        }
        reqs.push(get(s.master, "isDimmed"));
        reqs.push(get(s.click, "lastClick"));
        reqs.push(json!({"call": "Placemaker.TimeManager.get_slowmoScale", "args": [], "main": true}));
        reqs.push(get(s.manager, "counter"));
        for g in &s.sounds {
            for f in ["pos", "denom", "pan", "pitch", "volume", "lastTimePlayed", "attenuation"] {
                reqs.push(get(g.ms, f));
            }
        }
        for g in &s.pools {
            reqs.push(get(g.pool, "index"));
        }
        let res = h.batch(reqs);
        for (i, x) in res.iter().enumerate() {
            ok(x, &format!("round {round} request {i}"));
        }
        let gstate: Vec<u8> = ret::hex_field(&res[n]["bufs_after"][0]);
        let mut host = GameHost {
            h: RefCell::new(&mut *h),
            s,
            time: ret::f32(&res[n + 1]),
            dt: ret::f32(&res[n + 2]),
            dimmed: ret::bytes(&res[n + 3])[0] != 0,
            last_click: ret::f32(&res[n + 4]),
            slowmo: ret::f32(&res[n + 5]),
            cache: &cache,
            logs: vec![],
        };
        let mut rand = Rand { s: state };
        let mut audio = Sources::default();
        for &(t, pos, pitch, volume) in &asks {
            ours.play_sound(&mut host, t, pos, pitch, volume);
        }
        let played: Vec<bool> = ours.sounds.iter().map(|(_, m)| 0.0 < m.denom).collect();
        ours.on_update(&mut host, &mut rand, &mut pools, &mut audio);
        stats[0] += asks.len();
        stats[1] += played.iter().filter(|&&p| p).count();
        stats[3] += host.logs.len();
        drop(host);
        let what = format!("round {round}");
        if gstate != rand.s.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>() {
            errs.push(format!("{what}: Random state"));
        }
        if ret::i32(&res[n + 6]) != ours.counter {
            errs.push(format!("{what}: counter {} vs {}", ret::i32(&res[n + 6]), ours.counter));
        }
        let mut at = n + 7;
        for g in &s.sounds {
            let (_, m) = ours.sounds.iter().find(|(t, _)| ts_game::placemaker::audio::many_sounds_manager::sound_type::NAMES[*t as usize] == g.name).unwrap();
            let mine = [vec![m.pos.x, m.pos.y, m.pos.z], vec![m.denom], vec![m.pan], vec![m.pitch], vec![m.volume], vec![m.last_time_played], vec![m.attenuation]];
            for (i, f) in ["pos", "denom", "pan", "pitch", "volume", "lastTimePlayed", "attenuation"].iter().enumerate() {
                let gv = fs(&res[at + i]);
                if gv.iter().map(|x| x.to_bits()).collect::<Vec<_>>() != mine[i].iter().map(|x| x.to_bits()).collect::<Vec<_>>() {
                    errs.push(format!("{what}: {} {f}: {gv:?} vs {:?}", g.name, mine[i]));
                }
            }
            at += 7;
        }
        for (p, _) in s.pools.iter().enumerate() {
            if ret::i32(&res[at + p]) != pools[p].index {
                errs.push(format!("{what}: pool {p} index"));
            }
        }
        // the sources played (or stopped) this round
        let mut ids: Vec<SourceId> = audio.set.keys().copied().collect();
        ids.sort();
        let mut reqs = vec![];
        for &id in &ids {
            let (src, t) = s.pools[id / 100].sources[id % 100];
            reqs.push(call0(src, AS, "get_volume"));
            reqs.push(call0(src, AS, "get_pitch"));
            // (the managed getter is stripped)
            reqs.push(json!({"icall": "UnityEngine.AudioSource::get_panStereo()", "args": [{"handle": src}], "ret": "f32", "main": true}));
            reqs.push(call0(t, TR, "get_position"));
            reqs.push(json!({"call": "get_clip", "class": AS, "handle": src, "args": [], "main": true, "keep": true}));
        }
        let res = h.batch(reqs);
        for (k, &id) in ids.iter().enumerate() {
            let x = &res[k * 5..];
            let o = audio.set[&id];
            for (i, y) in x.iter().take(5).enumerate() {
                ok(y, &format!("{what}: source {id} read {i}"));
            }
            let g = [ret::f32(&x[0]), ret::f32(&x[1]), f32::from_bits(x[2]["f32"].as_u64().unwrap() as u32)];
            if [g[0].to_bits(), g[1].to_bits(), g[2].to_bits()] != [o.0.to_bits(), o.1.to_bits(), o.2.to_bits()] {
                errs.push(format!("{what}: source {id}: volume, pitch, pan {g:?} vs {:?}", (o.0, o.1, o.2)));
            }
            if o.5 {
                stats[2] += 1;
                let gp = vec3(&x[3]);
                if [gp.x.to_bits(), gp.y.to_bits(), gp.z.to_bits()] != [o.3.x.to_bits(), o.3.y.to_bits(), o.3.z.to_bits()] {
                    errs.push(format!("{what}: source {id}: position {gp:?} vs {:?}", o.3));
                }
                let gc = x[4].get("handle").and_then(Value::as_u64).map(|hd| addr(h, hd) as usize);
                if gc != o.4 {
                    errs.push(format!("{what}: source {id}: clip"));
                }
            }
        }
    }
    eprintln!("{ROUNDS} updates: {} requests, {} sounds blended, {} sources played, {} unknown types logged", stats[0], stats[1], stats[2], stats[3]);
    errs
}

#[test]
fn sounds_match_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let s = setup(h);
    listener_volume(h, 0.0);
    let result = catch_unwind(AssertUnwindSafe(|| run(h, &s)));
    listener_volume(h, 1.0);
    h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": s.root_go}], "param_types": ["UnityEngine.Object"], "main": true}));
    match result {
        Ok(errs) => assert!(errs.is_empty(), "{} mismatches:\n{}", errs.len(), errs.join("\n")),
        Err(e) => resume_unwind(e),
    }
}

/// `UiAudio.Play(clip, randomizePitch)` on the scene's UI audio (muted): the ring position, the source chosen, its
/// pitch and clip, and the generator.
#[test]
fn ui_audio_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.WorldMaster");
    let ui = keep(h, json!({"call": "UnityEngine.Component.get_transform", "on": "Placemaker.UiAudio", "args": [], "main": true}), "UiAudio transform");
    let ut = type_of(h, "Placemaker.UiAudio, Assembly-CSharp");
    let comp = keep(h, call(ui, "UnityEngine.Component", "GetComponent", vec![json!({"handle": ut})], &["System.Type"]), "UiAudio");
    let at = type_of(h, "UnityEngine.AudioSource, UnityEngine.AudioModule");
    let n = ret::i32(&h.request(call0(ui, TR, "get_childCount")));
    let mut sources = vec![];
    for i in 0..n {
        let c = child(h, ui, i);
        let s = keep(h, call(c, "UnityEngine.Component", "GetComponent", vec![json!({"handle": at})], &["System.Type"]), "AudioSource");
        sources.push((s, addr(h, s)));
    }
    let clips: Vec<Option<(u64, u64)>> = ["buttonDown", "buttonUp"]
        .iter()
        .map(|f| h.request(get(comp, f)).get("handle").and_then(Value::as_u64))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|c| c.map(|c| (c, addr(h, c))))
        .collect();
    let mut ours = ts_game::placemaker::ui_audio::UiAudio { index: 0, sources: (0..n as usize).collect(), button_down: clips[0].map(|c| c.1 as usize), button_up: clips[1].map(|c| c.1 as usize) };
    let original = ret::i32(&h.request(get(comp, "index")));
    listener_volume(h, 0.0);
    let mut r = Rng(0x0a_0d10);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for round in 0..200 {
            ours.index = r.pick(n as usize) as i32;
            let which = r.pick(3);
            let clip = if which < 2 { clips[which] } else { None };
            let randomize = r.pick(2) == 0;
            let state: [u32; 4] = [r.next(), r.next(), r.next(), r.next()];
            let res = h.batch(vec![
                set(comp, "index", &ours.index.to_le_bytes()),
                native(SET_STATE, &state.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>()),
                json!({"call": "Play", "class": "Placemaker.UiAudio", "handle": comp, "args": [clip.map_or(json!({"null": true}), |c| json!({"handle": c.0})), arg::bool(randomize)], "param_types": ["UnityEngine.AudioClip", "System.Boolean"], "main": true, "keep": true}),
                native(GET_STATE, &[0u8; 16]),
                get(comp, "index"),
            ]);
            for x in &res {
                ok(x, "UiAudio batch");
            }
            let mut rand = Rand { s: state };
            let mut audio = Sources::default();
            let src = ours.play(&mut audio, &mut rand, ours_clip(&ours, which), randomize);
            let what = format!("UiAudio round {round}");
            assert_eq!(ret::hex_field(&res[3]["bufs_after"][0]), rand.s.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>(), "{what}: Random state");
            assert_eq!(ret::i32(&res[4]), ours.index, "{what}: index");
            let returned = addr(h, res[2]["handle"].as_u64().unwrap());
            assert_eq!(returned, sources[src].1, "{what}: source");
            let pitch = ret::f32(&h.request(call0(sources[src].0, AS, "get_pitch")));
            assert_eq!(pitch.to_bits(), audio.set[&src].1.to_bits(), "{what}: pitch");
            let gc = h.request(json!({"call": "get_clip", "class": AS, "handle": sources[src].0, "args": [], "main": true, "keep": true}));
            let gc = gc.get("handle").and_then(Value::as_u64).map(|c| addr(h, c) as usize);
            assert_eq!(gc, audio.set[&src].4, "{what}: clip");
        }
    }));
    listener_volume(h, 1.0);
    h.request(set(comp, "index", &original.to_le_bytes()));
    if let Err(e) = result {
        resume_unwind(e);
    }
    eprintln!("200 UiAudio.Play calls over {n} sources");
}

fn ours_clip(u: &ts_game::placemaker::ui_audio::UiAudio, which: usize) -> Option<ClipId> {
    match which {
        0 => u.button_down,
        1 => u.button_up,
        _ => None,
    }
}
