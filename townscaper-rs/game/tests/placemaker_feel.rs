//! `ts_game::placemaker::{click_effect, voxel_bob_effect}` (G1d) against the original game. Needs `TS_HARNESS=1`.
//! The scene's own `ClickEffect` clicks far from the town (no bird in reach) with `UnityEngine.Random` set to a
//! known state, in one batch with its pitch walk set up: the generator's state afterwards (so every draw, in
//! order), the walked pitch, the click time and the last colour must match ours. The scene's `VoxelBobEffect` is
//! set to random springs behind random amounts of time and stepped by its own `Update`: the spring and the
//! enabled flag must match ours to the bit.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::click_effect::{ClickEffect, FeelContext};
use ts_game::placemaker::graphs::VoxelType;
use ts_game::placemaker::voxel_bob_effect::VoxelBobEffect;
use ts_game::unity::mathematics::f2;
use ts_game::unity::random::Rand;
use ts_game::unity::vector::Vector3;

const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn native(va: u32, args: Vec<Value>) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": args})
}
fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], true, "{what}: {r}");
    r
}
fn find(h: &mut Harness, t: &str) -> u64 {
    let ty = ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(t)], "param_types": ["System.String"], "main": true, "keep": true})), t)["handle"].as_u64().unwrap();
    ok(h.request(json!({"call": "UnityEngine.Object.FindObjectOfType", "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true})), t)["handle"].as_u64().unwrap()
}
fn f(v: &Value) -> f32 {
    f32::from_le_bytes(ret::bytes(v)[..4].try_into().unwrap())
}
fn fbits(x: f32) -> String {
    hex(&x.to_le_bytes())
}
fn field(o: u64, name: &str) -> Value {
    json!({"field": name, "handle": o, "main": true})
}
fn set(o: u64, name: &str, raw: String) -> Value {
    json!({"field": name, "handle": o, "set_raw": raw, "main": true})
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

#[test]
fn click_effect_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let ce = find(h, "Placemaker.ClickEffect, Assembly-CSharp");
    let count = |h: &mut Harness, name: &str| {
        let l = ok(h.request(json!({"field": name, "handle": ce, "main": true})), name)["handle"].as_u64().unwrap();
        ret::i32(&ok(h.request(json!({"call": "get_Count", "class": "System.Collections.Generic.List`1[[UnityEngine.AudioClip, UnityEngine.AudioModule]]", "handle": l, "args": [], "main": true})), "count"))
    };
    let clip_counts = ["addClips", "removeClips", "addClips1", "removeClips1", "splashClips", "colorPickClips"].map(|n| count(h, n));
    eprintln!("clip lists {clip_counts:?}");
    let mut r = Rng(9);
    let mut walked = [0usize; 3];
    for round in 0..200 {
        let start = [r.next(), r.next(), r.next(), r.next()];
        let add = r.next() % 2 == 0;
        let height = if r.next() % 4 == 0 { 0 } else { (r.next() % 30) as i32 };
        let t = (r.next() % 15) as u8;
        let last_pitch = 90.0 + (r.next() % 400) as f32 * 0.07;
        let last_type = (r.next() % 15) as u8;
        // the click time: now (a walk) or long ago (a reset)
        let now = f(&ok(h.request(json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true})), "time"));
        let last_click = if r.next() % 3 == 0 { now - 10.0 } else { now - 0.3 };
        let plane = f2(4000.0 + (r.next() % 100) as f32, 4000.0 + (r.next() % 100) as f32);
        let reqs = vec![
            native(SET_STATE, vec![json!({"buf": hex(&start.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>())})]),
            set(ce, "lastPitch", fbits(last_pitch)),
            set(ce, "lastClick", fbits(last_click)),
            set(ce, "lastVoxelType", format!("{last_type:02x}")),
            json!({"call": "Click", "class": "Placemaker.ClickEffect", "handle": ce, "args": [arg::bool(add), arg::raw(&[plane.x.to_le_bytes(), plane.y.to_le_bytes()].concat()), arg::i32(height), arg::raw(&[t])], "param_types": ["System.Boolean", "UnityEngine.Vector2", "System.Int32", "Placemaker.VoxelType"], "main": true}),
            native(GET_STATE, vec![json!({"buf": "00".repeat(16)})]),
            field(ce, "lastPitch"),
            field(ce, "lastClick"),
            field(ce, "lastVoxelType"),
            json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true}),
            json!({"call": "Placemaker.TimeManager.get_slowmoScale", "args": [], "main": true}),
        ];
        let res = h.batch(reqs);
        for x in &res {
            ok(x.clone(), "click batch");
        }
        let time = f(&res[9]);
        let slowmo = f(&res[10]);
        let mut rand = Rand { s: start };
        let mut eff = ClickEffect { last_voxel_type: VoxelType::from_i8(last_type as i8), last_click, last_pitch, ..Default::default() };
        let mut uproot = |_: &mut Rand, _: Vector3, _: f32, _: f32| {};
        let mut cx = FeelContext { random: &mut rand, uproot: &mut uproot, time, slowmo_scale: slowmo, palette_width: 16, clip_counts };
        eff.click(&mut cx, add, plane, height, VoxelType::from_i8(t as i8));
        let what = format!("round {round}: add {add}, height {height}, type {t}, last pitch {last_pitch}, last click {last_click} at {time}");
        let state: Vec<u8> = rand.s.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(ret::hex_field(&res[5]["bufs_after"][0]), state, "{what}: Random state after");
        assert_eq!(f(&res[6]).to_bits(), eff.last_pitch.to_bits(), "{what}: lastPitch");
        assert_eq!(f(&res[7]).to_bits(), eff.last_click.to_bits(), "{what}: lastClick");
        assert_eq!(ret::bytes(&res[8])[0], eff.last_voxel_type as u8, "{what}: lastVoxelType");
        walked[if eff.last_pitch == 100.0 { 0 } else if add { 1 } else { 2 }] += 1;
    }
    eprintln!("200 clicks: {} pitch resets, {} up, {} down", walked[0], walked[1], walked[2]);
}

#[test]
fn voxel_bob_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let bob = find(h, "Placemaker.VoxelBobEffect, Assembly-CSharp");
    let mut r = Rng(4);
    let mut settled = 0;
    for round in 0..300 {
        let now0 = f(&ok(h.request(json!({"call": "UnityEngine.Time.get_unscaledTime", "args": [], "main": true})), "time"));
        let cur = ((r.next() % 2000) as f32 - 1000.0) * 0.0004;
        let vel = ((r.next() % 2000) as f32 - 1000.0) * 0.0004;
        let behind = (r.next() % 80) as f32 * 0.013;
        let bob_time = now0 - behind;
        let reqs = vec![
            set(bob, "bobCurrent", fbits(cur)),
            set(bob, "bobVelocity", fbits(vel)),
            set(bob, "bobTime", fbits(bob_time)),
            json!({"call": "Update", "class": "Placemaker.VoxelBobEffect", "handle": bob, "args": [], "main": true}),
            field(bob, "bobCurrent"),
            field(bob, "bobVelocity"),
            field(bob, "bobTime"),
            json!({"call": "get_enabled", "class": "UnityEngine.Behaviour", "handle": bob, "args": [], "main": true}),
            json!({"call": "UnityEngine.Time.get_unscaledTime", "args": [], "main": true}),
        ];
        let res = h.batch(reqs);
        for x in &res {
            ok(x.clone(), "bob batch");
        }
        let now = f(&res[8]);
        let mut ours = VoxelBobEffect { enabled: true, bob_current: cur, bob_velocity: vel, bob_time, ..Default::default() };
        ours.update(now);
        let what = format!("round {round}: {cur} {vel} from {bob_time} to {now}");
        assert_eq!([f(&res[4]).to_bits(), f(&res[5]).to_bits(), f(&res[6]).to_bits()], [ours.bob_current.to_bits(), ours.bob_velocity.to_bits(), ours.bob_time.to_bits()], "{what}: bobCurrent, bobVelocity, bobTime");
        let enabled = ret::bytes(&res[7])[0] != 0;
        if !ours.enabled {
            settled += 1;
            assert!(!enabled, "{what}: enabled");
        }
    }
    eprintln!("300 springs, {settled} settled");
}
