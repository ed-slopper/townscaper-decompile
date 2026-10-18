//! `ts_game::placemaker::life::bird_flock` (G5a) against the game (`TS_HARNESS=1`), on a fresh `BirdFlock` (the
//! scene's master, so its prop grid and sounds) with fresh `Bird` / `BirdLanding` objects far from the town:
//! `CalculateOptimalLandings` on every perch count up to 5000, `GetSearchCoordinates`, `SetState` and
//! `UprootBird` (sitting and flying birds, `UnityEngine.Random` set to a known state, the generator's state, the
//! bird's fields and the flock's take-off sums compared to the bit).

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::life::bird_flock::{BirdFlock, BirdPrefab, BirdWorld, ColliderRef, LandingId};
use ts_game::unity::mathematics::Int3;
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::random::Rand;
use ts_game::unity::vector::{v3, Vector3};

const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;
const BF: &str = "Placemaker.Life.BirdFlock";

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
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn set(o: u64, f: &str, b: &[u8]) -> Value {
    json!({"field": f, "handle": o, "set_raw": hex(b), "main": true})
}
fn get(o: u64, f: &str) -> Value {
    json!({"field": f, "handle": o, "main": true})
}
fn v3b(v: Vector3) -> Vec<u8> {
    [v.x, v.y, v.z].iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn fs(v: &Value) -> Vec<f32> {
    ret::bytes(v).chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn native(va: u32, buf: &[u8]) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": [{"buf": hex(buf)}]})
}
fn component(h: &mut Harness, ty: &str, name: &str) -> (u64, u64) {
    let go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true}), "go");
    let t = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string(&format!("{ty}, Assembly-CSharp"))], "param_types": ["System.String"], "main": true}), "type");
    let c = keep(h, json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": t}], "param_types": ["System.Type"], "main": true}), "AddComponent");
    let tr = keep(h, json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true}), "transform");
    (c, tr)
}

/// A world with no props near (the test objects are far from the town).
struct Far {
    time: f32,
    landing_pos: Vector3,
}
impl BirdWorld for Far {
    fn time(&self) -> f32 {
        self.time
    }
    fn colliders(&self, _c: Int3) -> Option<Vec<ColliderRef>> {
        None
    }
    fn landing_position(&self, _l: LandingId) -> Vector3 {
        self.landing_pos
    }
    fn landing_transform_point(&self, _l: LandingId, p: Vector3) -> Vector3 {
        p
    }
    fn landing_rotation(&self, _l: LandingId) -> Quaternion {
        Quaternion::IDENTITY
    }
    fn landing_up(&self, _l: LandingId) -> Vector3 {
        Vector3::UP
    }
    fn square_count(&self) -> i32 {
        0
    }
    fn landings_in_square(&self, _i: i32) -> Vec<LandingId> {
        vec![]
    }
    fn play_sound(&mut self, _s: i32, _p: Vector3) {}
    fn feathers(&mut self, _p: Vector3, _r: Quaternion) {}
}

#[test]
fn flock_helpers_match_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.WorldMaster");
    let master = keep(h, json!({"field": "flock", "on": "Placemaker.WorldMaster", "main": true}), "flock");
    let master = keep(h, get(master, "master"), "master");
    let (flock, _) = component(h, BF, "ts_check flock");
    ok(&h.request(json!({"field": "master", "handle": flock, "set_handle": master, "main": true})), "master");
    // CalculateOptimalLandings
    let counts: Vec<i32> = (-2..5000).collect();
    for chunk in counts.chunks(250) {
        let mut reqs = vec![];
        for &n in chunk {
            reqs.push(set(flock, "birdLandingCount", &n.to_le_bytes()));
            reqs.push(set(flock, "optimalBirdCountDirty", &[1]));
            reqs.push(json!({"call": "CalculateOptimalLandings", "class": BF, "handle": flock, "args": [], "main": true}));
            reqs.push(get(flock, "maxBirdCount"));
            reqs.push(get(flock, "minBirdCount"));
            reqs.push(get(flock, "idealBirdCount"));
        }
        let res = h.batch(reqs);
        for (k, &n) in chunk.iter().enumerate() {
            let r = &res[k * 6..];
            for x in r.iter().take(6) {
                ok(x, "optimal");
            }
            let mut ours = BirdFlock::new(BirdPrefab::default());
            ours.bird_landing_count = n;
            ours.optimal_bird_count_dirty = true;
            ours.calculate_optimal_landings();
            let game = [ret::bytes(&r[3])[0], ret::bytes(&r[4])[0], ret::bytes(&r[5])[0]];
            assert_eq!(game, [ours.max_bird_count, ours.min_bird_count, ours.ideal_bird_count], "{n} perches");
        }
    }
    // GetSearchCoordinates
    let mut r = Rng(17);
    let mut reqs = vec![];
    let mut cases = vec![];
    for _ in 0..500 {
        let p = v3(r.f(-300.0, 300.0), r.f(-5.0, 60.0), r.f(-300.0, 300.0));
        let rad = r.f(0.0, 3.0);
        reqs.push(json!({"call": "GetSearchCoordinates", "class": BF, "handle": flock, "args": [arg::raw(&v3b(p)), arg::f32(rad)], "param_types": ["UnityEngine.Vector3", "System.Single"], "main": true}));
        cases.push((p, rad));
    }
    for (res, (p, rad)) in h.batch(reqs).iter().zip(&cases) {
        ok(res, "GetSearchCoordinates");
        let b: Vec<i32> = ret::bytes(res).chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
        let (lo, hi) = BirdFlock::get_search_coordinates(*p, *rad);
        assert_eq!(b, vec![lo.x, lo.y, lo.z, hi.x, hi.y, hi.z], "{p:?} {rad}");
    }
    // SetState and UprootBird on a fresh bird and perch far from the town
    let (bird, bird_t) = component(h, "Placemaker.Life.Bird", "ts_check bird");
    let (landing, landing_t) = component(h, "Placemaker.Life.BirdLanding", "ts_check perch");
    // the bird's mesh filter (UprootBird sets mf1's mesh)
    let go = keep(h, json!({"call": "get_gameObject", "class": "UnityEngine.Component", "handle": bird, "args": [], "main": true}), "bird go");
    let mft = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("UnityEngine.MeshFilter, UnityEngine.CoreModule")], "param_types": ["System.String"], "main": true}), "type");
    let mf = keep(h, json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": mft}], "param_types": ["System.Type"], "main": true}), "MeshFilter");
    ok(&h.request(json!({"field": "mf1", "handle": bird, "set_handle": mf, "main": true})), "mf1");
    let landing_pos = v3(5000.0, 3.0, 5000.0);
    ok(&h.request(json!({"call": "set_position", "class": "UnityEngine.Transform", "handle": landing_t, "args": [arg::raw(&v3b(landing_pos))], "param_types": ["UnityEngine.Vector3"], "main": true})), "perch position");
    let mut sat = 0;
    for round in 0..300 {
        let state: [u32; 4] = [r.next(), r.next(), r.next(), r.next()];
        let state_b: Vec<u8> = state.iter().flat_map(|w| w.to_le_bytes()).collect();
        let pos = v3(r.f(4990.0, 5010.0), r.f(0.0, 9.0), r.f(4990.0, 5010.0));
        let normal = v3(r.f(-1.0, 1.0), r.f(-1.0, 1.0), r.f(-1.0, 1.0)).normalized();
        let sitting = r.next() % 2 == 0;
        let next_state = if sitting { 3u8 } else { [0u8, 1, 2, 4][(r.next() % 4) as usize] };
        let has_landing = sitting || r.next() % 2 == 0;
        let speed = r.f(0.2, 2.0);
        let take_off = (r.next() % 5) as i32;
        let take_off_pos = v3(r.f(-50.0, 50.0), r.f(0.0, 10.0), r.f(-50.0, 50.0));
        let assigned = (r.next() % 10) as u8 + 1;
        let set_state = round % 3 == 0;
        let now = f32::from_le_bytes(ret::bytes(&h.request(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true})))[..4].try_into().unwrap());
        let bird_time = now - r.f(0.0, 0.6);
        let mut reqs = vec![
            set(bird, "pos", &v3b(pos)),
            set(bird, "time", &bird_time.to_le_bytes()),
            set(bird, "stateNext", &[next_state]),
            set(bird, "velocityNext", &[0u8; 12]),
            set(bird, "velocityPrev", &[0u8; 12]),
            set(bird, "posMid", &[0u8; 12]),
            set(landing, "normal", &v3b(normal)),
            set(flock, "takeOffCount", &take_off.to_le_bytes()),
            set(flock, "takeOffPos", &v3b(take_off_pos)),
            set(flock, "assignedBirdsCount", &[assigned]),
            json!({"call": "set_position", "class": "UnityEngine.Transform", "handle": bird_t, "args": [arg::raw(&v3b(pos))], "param_types": ["UnityEngine.Vector3"], "main": true}),
        ];
        if has_landing {
            reqs.push(json!({"field": "landing", "handle": bird, "set_handle": landing, "main": true}));
            reqs.push(json!({"field": "bird", "handle": landing, "set_handle": bird, "main": true}));
        } else {
            reqs.push(json!({"field": "landing", "handle": bird, "set_null": true, "main": true}));
            reqs.push(json!({"field": "bird", "handle": landing, "set_null": true, "main": true}));
        }
        reqs.push(native(SET_STATE, &state_b));
        reqs.push(if set_state {
            json!({"call": "SetState", "class": BF, "handle": flock, "args": [{"handle": bird}, arg::raw(&[next_state.wrapping_add(1) % 5])], "param_types": ["Placemaker.Life.Bird", "Placemaker.Life.BirdFlock.State"], "main": true})
        } else {
            json!({"call": "UprootBird", "class": BF, "handle": flock, "args": [{"handle": bird}, arg::f32(speed)], "param_types": ["Placemaker.Life.Bird", "System.Single"], "main": true})
        });
        let n = reqs.len();
        reqs.push(native(GET_STATE, &[0u8; 16]));
        reqs.push(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true}));
        for f in ["velocityNext", "velocityPrev", "posMid", "stateNext", "lastActionTime", "landing"] {
            reqs.push(get(bird, f));
        }
        reqs.push(get(flock, "takeOffCount"));
        reqs.push(get(flock, "takeOffPos"));
        reqs.push(get(flock, "assignedBirdsCount"));
        let res = h.batch(reqs);
        for x in &res {
            ok(x, "bird batch");
        }
        let time = f32::from_le_bytes(ret::bytes(&res[n + 1])[..4].try_into().unwrap());
        // ours
        let mut world = Far { time, landing_pos };
        let mut ours = BirdFlock::new(BirdPrefab::default());
        ours.take_off_count = take_off;
        ours.take_off_pos = take_off_pos;
        ours.assigned_birds_count = assigned;
        let b = 0usize;
        let root = ours.transforms.add(Default::default());
        ours.transforms.set_position(root, pos);
        let l: LandingId = 999;
        ours.all_birds.push(ts_game::placemaker::life::bird_flock::Bird {
            root,
            body: root,
            shade: root,
            mf0: root,
            mf1: root,
            mesh0: Default::default(),
            mesh1: Default::default(),
            rotation0: Quaternion::IDENTITY,
            rotation1: Quaternion::IDENTITY,
            pos_mid: Vector3::ZERO,
            velocity_prev: Vector3::ZERO,
            velocity_next: Vector3::ZERO,
            pos,
            time: bird_time,
            preferred_neighbour_count: 0,
            state_prev: 0,
            state_next: next_state,
            last_action_time: 0.0,
            landing: has_landing.then_some(l),
        });
        if has_landing {
            ours.landings.insert(l, ts_game::placemaker::life::bird_flock::BirdLanding { bird: Some(b), neighbour_count: 0, normal });
        }
        let mut rand = Rand { s: state };
        let mut log = |_: &str| {};
        if set_state {
            ours.set_state(&world, &mut rand, &mut log, b, next_state.wrapping_add(1) % 5);
        } else {
            ours.uproot_bird(&mut world, &mut rand, &mut log, b, speed);
            if sitting {
                sat += 1;
            }
        }
        let what = format!("round {round}: sitting {sitting}, state {next_state}, landing {has_landing}, set_state {set_state}");
        let gstate: Vec<u8> = ts_check::ret::hex_field(&res[n]["bufs_after"][0]);
        assert_eq!(gstate, rand.s.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>(), "{what}: Random state");
        let o = &ours.all_birds[b];
        let bits = |v: Vector3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
        let g = |i: usize| {
            let f = fs(&res[n + 2 + i]);
            [f[0].to_bits(), f[1].to_bits(), f[2].to_bits()]
        };
        if !set_state {
            assert_eq!(g(0), bits(o.velocity_next), "{what}: velocityNext");
            assert_eq!(g(1), bits(o.velocity_prev), "{what}: velocityPrev");
            assert_eq!(g(2), bits(o.pos_mid), "{what}: posMid");
            assert!(res[n + 7].get("handle").is_none() || res[n + 7]["null"] == true, "{what}: landing left");
            assert_eq!(ret::i32(&res[n + 8]), ours.take_off_count, "{what}: takeOffCount");
            let tp = fs(&res[n + 9]);
            assert_eq!([tp[0].to_bits(), tp[1].to_bits(), tp[2].to_bits()], bits(ours.take_off_pos), "{what}: takeOffPos");
            assert_eq!(ret::bytes(&res[n + 10])[0], ours.assigned_birds_count, "{what}: assignedBirdsCount");
        }
        assert_eq!(ret::bytes(&res[n + 5])[0], o.state_next, "{what}: stateNext");
        assert_eq!(fs(&res[n + 6])[0].to_bits(), o.last_action_time.to_bits(), "{what}: lastActionTime");
    }
    eprintln!("300 birds ({sat} uprooted from a perch)");
}
