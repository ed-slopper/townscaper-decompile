//! G6b against the game (`TS_HARNESS=1`): `Beatbox.GetBeat` / `IsBeat`; `WaterSound.Update` over random
//! shorelines (a stand-in master's sandbanker) and camera positions; fresh `AmbientSound`, `AudioSnapshotManager`
//! and `MenuMusic` components (with their own sources, the listener muted) through random sequences of their
//! calls, every field, the source's volume, clip and playing flag, and `UnityEngine.Random` compared.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::beatbox;
use ts_game::placemaker::ambient_sound::AmbientSound;
use ts_game::placemaker::audio::many_sounds_manager::ClipId;
use ts_game::placemaker::audio_snapshot_manager::AudioSnapshotManager;
use ts_game::placemaker::ui::menu_music::{MenuMusic, MusicSource};
use ts_game::placemaker::water_sound;
use ts_game::unity::random::Rand;
use ts_game::unity::vector::{v3, Vector3};

const GET_STATE: u32 = 0x107A_74D0;
const SET_STATE: u32 = 0x107A_74F0;
const TR: &str = "UnityEngine.Transform";
const AS: &str = "UnityEngine.AudioSource";

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
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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
fn vec3(v: &Value) -> Vector3 {
    let b = ret::bytes(v);
    let f = |i: usize| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    v3(f(0), f(1), f(2))
}
fn native(va: u32, buf: &[u8]) -> Value {
    json!({"native": "UnityPlayer.dll", "va": format!("0x{va:X}"), "args": [{"buf": hex(buf)}]})
}
fn type_of(h: &mut Harness, ty: &str) -> u64 {
    keep(h, json!({"call": "System.Type.GetType", "args": [arg::string(ty)], "param_types": ["System.String"], "main": true}), "type")
}
fn add(h: &mut Harness, go: u64, ty: &str) -> u64 {
    let t = type_of(h, ty);
    keep(h, call(go, "UnityEngine.GameObject", "AddComponent", vec![json!({"handle": t})], &["System.Type"]), "AddComponent")
}
fn listener_volume(h: &mut Harness, v: f32) {
    ok(&h.request(json!({"icall": "UnityEngine.AudioListener::set_volume(System.Single)", "args": [{"u32": v.to_bits()}], "main": true})), "AudioListener.volume");
}
fn enabled(c: u64) -> Value {
    call0(c, "UnityEngine.Behaviour", "get_enabled")
}

#[test]
fn beatbox_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let mut r = Rng(0xbea7);
    let mut reqs = vec![];
    let mut cases = vec![];
    for _ in 0..2000 {
        let (t, s, o) = (r.f(0.0, 5000.0), r.f(-3.0, 3.0), r.f(-5.0, 5.0));
        reqs.push(json!({"call": "Beatbox.GetBeat", "args": [arg::f32(t), arg::f32(s), arg::f32(o)], "param_types": ["System.Single", "System.Single", "System.Single"], "main": true}));
        cases.push((t, s, o));
    }
    for (res, &(t, s, o)) in h.batch(reqs).iter().zip(&cases) {
        ok(res, "GetBeat");
        assert_eq!(ret::i32(res), beatbox::get_beat(t, s, o), "GetBeat({t}, {s}, {o})");
    }
    let mut beats = 0;
    for _ in 0..300 {
        let (s, o) = (r.f(-60.0, 60.0), r.f(-5.0, 5.0));
        let res = h.batch(vec![
            json!({"call": "Beatbox.IsBeat", "args": [arg::f32(s), arg::f32(o)], "param_types": ["System.Single", "System.Single"], "main": true}),
            json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true}),
            json!({"call": "UnityEngine.Time.get_deltaTime", "args": [], "main": true}),
        ]);
        let g = ret::bytes(&res[0])[0] != 0;
        assert_eq!(g, beatbox::is_beat(ret::f32(&res[1]), ret::f32(&res[2]), s, o), "IsBeat({s}, {o})");
        beats += g as usize;
    }
    eprintln!("2000 beats, 300 IsBeat ({beats} on a beat)");
}

#[test]
fn water_sound_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.WorldMaster");
    // a stand-in master whose sandbanker holds our shoreline
    let master = keep(h, json!({"new": "Placemaker.WorldMaster", "args": [], "main": true}), "stand-in master");
    let sb = keep(h, json!({"new": "Placemaker.Sandbanker", "args": [], "main": true}), "stand-in sandbanker");
    ok(&h.request(json!({"field": "sandbanker", "handle": master, "set_handle": sb, "main": true})), "sandbanker");
    let list_class = "System.Collections.Generic.List`1[[UnityEngine.Vector3, UnityEngine.CoreModule]]";
    let list = keep(h, json!({"new": list_class, "args": [], "main": true}), "verts");
    ok(&h.request(json!({"field": "verts", "handle": sb, "set_handle": list, "main": true})), "verts field");
    let go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check water")], "param_types": ["System.String"], "main": true}), "go");
    let ws = add(h, go, "Placemaker.WaterSound, Assembly-CSharp");
    ok(&h.request(json!({"field": "master", "handle": ws, "set_handle": master, "main": true})), "master");
    let wt = keep(h, call0(go, "UnityEngine.GameObject", "get_transform"), "transform");
    let cam = keep(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true}), "camera");
    let ct = keep(h, call0(cam, "UnityEngine.Component", "get_transform"), "camera transform");
    let original = vec3(&h.request(call0(ct, TR, "get_position")));
    let mut r = Rng(0x3a7e5);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for round in 0..200 {
            let n = r.pick(40);
            let verts: Vec<Vector3> = (0..n).map(|_| v3(r.f(-60.0, 60.0), r.f(-1.0, 1.0), r.f(-60.0, 60.0))).collect();
            let camera = v3(r.f(-80.0, 80.0), r.f(5.0, 60.0), r.f(-80.0, 80.0));
            let mut reqs = vec![call0(list, list_class, "Clear")];
            for v in &verts {
                reqs.push(call(list, list_class, "Add", vec![arg::raw(&v3b(*v))], &[]));
            }
            // the camera is moved and put back within the one batch
            reqs.push(call(ct, TR, "set_position", vec![arg::raw(&v3b(camera))], &["UnityEngine.Vector3"]));
            reqs.push(call0(ct, TR, "get_position"));
            reqs.push(call0(ws, "Placemaker.WaterSound", "Update"));
            reqs.push(call(ct, TR, "set_position", vec![arg::raw(&v3b(original))], &["UnityEngine.Vector3"]));
            reqs.push(call0(wt, TR, "get_position"));
            let res = h.batch(reqs);
            for x in &res {
                ok(x, "water batch");
            }
            let k = res.len();
            let cam_pos = vec3(&res[k - 4]);
            let g = vec3(&res[k - 1]);
            let o = water_sound::update(cam_pos, &verts);
            assert_eq!([g.x.to_bits(), g.y.to_bits(), g.z.to_bits()], [o.x.to_bits(), o.y.to_bits(), o.z.to_bits()], "round {round}: {n} verts, {g:?} vs {o:?}");
        }
    }));
    h.request(call(ct, TR, "set_position", vec![arg::raw(&v3b(original))], &["UnityEngine.Vector3"]));
    h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": go}], "param_types": ["UnityEngine.Object"], "main": true}));
    if let Err(e) = result {
        resume_unwind(e);
    }
}

/// Our side of a test `AudioSource`.
struct Src {
    volume: f32,
    clip: Option<ClipId>,
    playing: bool,
}
impl MusicSource for Src {
    fn volume(&self) -> f32 {
        self.volume
    }
    fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
    }
    fn is_playing(&self) -> bool {
        self.playing
    }
    fn set_clip(&mut self, clip: Option<ClipId>) {
        // (setting a clip stops the source)
        self.clip = clip;
        self.playing = false;
    }
    fn play(&mut self) {
        self.playing = self.clip.is_some();
    }
    fn stop(&mut self) {
        self.playing = false;
    }
}

#[test]
fn ambience_components_match_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.WorldMaster");
    // (objects a failed run left behind, by this test's own name, would be found before the scene's)
    loop {
        let r = h.request(json!({"call": "UnityEngine.GameObject.Find", "args": [arg::string("ts_check ambience")], "param_types": ["System.String"], "main": true, "keep": true}));
        let Some(old) = r.get("handle").and_then(Value::as_u64) else { break };
        ok(&h.request(json!({"call": "UnityEngine.Object.DestroyImmediate", "args": [{"handle": old}], "param_types": ["UnityEngine.Object"], "main": true})), "DestroyImmediate");
    }
    let scene_snap = keep(h, json!({"call": "UnityEngine.Component.get_transform", "on": "Placemaker.AudioSnapshotManager", "args": [], "main": true}), "scene snapshot manager");
    let st = type_of(h, "Placemaker.AudioSnapshotManager, Assembly-CSharp");
    let scene_snap = keep(h, call(scene_snap, "UnityEngine.Component", "GetComponent", vec![json!({"handle": st})], &["System.Type"]), "scene snapshots");
    let scene_music = keep(h, json!({"call": "UnityEngine.Component.get_transform", "on": "Placemaker.Ui.MenuMusic", "args": [], "main": true}), "scene music");
    let mt = type_of(h, "Placemaker.Ui.MenuMusic, Assembly-CSharp");
    let scene_music = keep(h, call(scene_music, "UnityEngine.Component", "GetComponent", vec![json!({"handle": mt})], &["System.Type"]), "scene MenuMusic");
    let go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check ambience")], "param_types": ["System.String"], "main": true}), "go");
    let src = add(h, go, "UnityEngine.AudioSource, UnityEngine.AudioModule");
    let amb = add(h, go, "Placemaker.AmbientSound, Assembly-CSharp");
    ok(&h.request(json!({"field": "audioSource", "handle": amb, "set_handle": src, "main": true})), "audioSource");
    let snap = add(h, go, "Placemaker.AudioSnapshotManager, Assembly-CSharp");
    for f in ["inFocusSnapshot", "outOfFocusSnapshot"] {
        let s = keep(h, get(scene_snap, f), f);
        ok(&h.request(json!({"field": f, "handle": snap, "set_handle": s, "main": true})), f);
    }
    let music = add(h, go, "Placemaker.Ui.MenuMusic, Assembly-CSharp");
    ok(&h.request(json!({"call": "OnSetup", "class": "Placemaker.Ui.MenuMusic", "handle": music, "args": [{"null": true}], "main": true})), "OnSetup");
    let clips_list = keep(h, get(scene_music, "audioClips"), "audioClips");
    ok(&h.request(json!({"field": "audioClips", "handle": music, "set_handle": clips_list, "main": true})), "audioClips");
    let lc = "System.Collections.Generic.List`1[[UnityEngine.AudioClip, UnityEngine.AudioModule]]";
    let n = ret::i32(&h.request(call0(clips_list, lc, "get_Count")));
    let mut clips = vec![];
    for i in 0..n {
        let c = keep(h, call(clips_list, lc, "get_Item", vec![arg::i32(i)], &[]), "clip");
        clips.push((c, addr(h, c)));
    }
    // Unity calls their Update every frame too: they are kept disabled between batches (each batch sets the
    // enabled flags to ours first and clears them after reading), starting from the game's state now
    let set_enabled = |c: u64, on: bool| call(c, "UnityEngine.Behaviour", "set_enabled", vec![arg::bool(on)], &["System.Boolean"]);
    let res = h.batch(vec![
        set_enabled(amb, false),
        set_enabled(snap, false),
        set_enabled(music, false),
        call0(src, AS, "get_volume"),
        json!({"call": "get_clip", "class": AS, "handle": src, "args": [], "main": true, "keep": true}),
        call0(src, AS, "get_isPlaying"),
        get(snap, "windowFocused"),
        get(snap, "alphaDim"),
        get(music, "targetVolume"),
        get(music, "speed"),
    ]);
    for x in &res {
        ok(x, "sync");
    }
    let mut ours_music = MenuMusic::new(clips.iter().map(|c| c.1 as usize).collect());
    ours_music.target_volume = ret::f32(&res[8]);
    ours_music.speed = ret::f32(&res[9]);
    let mut ours_amb = AmbientSound { enabled: true };
    let mut ours_snap = AudioSnapshotManager::default();
    ours_snap.window_focused = ret::bytes(&res[6])[0] != 0;
    ours_snap.alpha_dim = ret::bytes(&res[7])[0] != 0;
    let clip0 = res[4].get("handle").and_then(Value::as_u64).map(|c| addr(h, c) as usize);
    let mut s = Src { volume: ret::f32(&res[3]), clip: clip0, playing: ret::bytes(&res[5])[0] != 0 };
    listener_volume(h, 0.0);
    let mut r = Rng(0xa3b1e);
    let mut ops = [0usize; 8];
    let result = catch_unwind(AssertUnwindSafe(|| {
        for round in 0..600 {
            let op = r.pick(8);
            ops[op] += 1;
            let state: [u32; 4] = [r.next(), r.next(), r.next(), r.next()];
            let mut reqs = vec![
                set_enabled(amb, ours_amb.enabled),
                set_enabled(snap, ours_snap.enabled),
                set_enabled(music, ours_music.enabled),
                native(SET_STATE, &state.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>()),
            ];
            let v = r.f(0.0, 1.0);
            let focus = r.pick(2) == 0;
            let alpha = if r.pick(3) == 0 { 0.0 } else { r.f(-0.5, 1.0) };
            let timer = r.f(0.1, 4.0);
            let clip = if n > 0 && r.pick(4) != 0 { Some(clips[r.pick(n as usize)]) } else { None };
            let with_clip = r.pick(2) == 1;
            match op {
                0 => {
                    reqs.push(call(src, AS, "set_volume", vec![arg::f32(v)], &["System.Single"]));
                    reqs.push(call0(amb, "Placemaker.AmbientSound", "Update"));
                }
                1 => reqs.push(call(amb, "Placemaker.AmbientSound", "OnApplicationFocus", vec![arg::bool(focus)], &["System.Boolean"])),
                2 => reqs.push(call(snap, "Placemaker.AudioSnapshotManager", "OnApplicationFocus", vec![arg::bool(focus)], &["System.Boolean"])),
                3 => reqs.push(call(snap, "Placemaker.AudioSnapshotManager", "<Placemaker.Ui.UiMaster.IUiSetup.OnSetup>b__4_0", vec![arg::f32(alpha)], &["System.Single"])),
                4 => reqs.push(call0(snap, "Placemaker.AudioSnapshotManager", "Update")),
                5 => {
                    if !with_clip {
                        reqs.push(call0(music, "Placemaker.Ui.MenuMusic", "StartPlaying"));
                    } else {
                        reqs.push(call(music, "Placemaker.Ui.MenuMusic", "StartPlaying", vec![clip.map_or(json!({"null": true}), |c| json!({"handle": c.0}))], &["UnityEngine.AudioClip"]));
                    }
                }
                6 => reqs.push(call(music, "Placemaker.Ui.MenuMusic", "StopPlaying", vec![arg::f32(timer)], &["System.Single"])),
                _ => reqs.push(call0(music, "Placemaker.Ui.MenuMusic", "Update")),
            }
            let k = reqs.len();
            reqs.push(native(GET_STATE, &[0u8; 16]));
            reqs.push(json!({"call": "UnityEngine.Application.get_isFocused", "args": [], "main": true}));
            reqs.push(json!({"call": "UnityEngine.Time.get_unscaledDeltaTime", "args": [], "main": true}));
            reqs.push(json!({"call": "UnityEngine.Time.get_deltaTime", "args": [], "main": true}));
            reqs.push(call0(src, AS, "get_volume"));
            reqs.push(json!({"call": "get_clip", "class": AS, "handle": src, "args": [], "main": true, "keep": true}));
            reqs.push(call0(src, AS, "get_isPlaying"));
            reqs.push(enabled(amb));
            reqs.push(enabled(snap));
            reqs.push(enabled(music));
            reqs.push(get(snap, "windowFocused"));
            reqs.push(get(snap, "alphaDim"));
            reqs.push(get(music, "targetVolume"));
            reqs.push(get(music, "speed"));
            for c in [amb, snap, music] {
                reqs.push(set_enabled(c, false));
            }
            let res = h.batch(reqs);
            for x in &res {
                ok(x, "ambience batch");
            }
            let mut rand = Rand { s: state };
            let is_focused = ret::bytes(&res[k + 1])[0] != 0;
            let (udt, dt) = (ret::f32(&res[k + 2]), ret::f32(&res[k + 3]));
            match op {
                0 => {
                    s.volume = v;
                    ours_amb.update(is_focused, &mut s.volume, udt);
                }
                1 => ours_amb.on_application_focus(focus),
                2 => ours_snap.on_application_focus(focus),
                3 => ours_snap.on_dim_target(alpha),
                4 => {
                    ours_snap.update();
                }
                5 => {
                    if with_clip {
                        ours_music.start_playing_clip(&mut s, &mut rand, clip.map(|c| c.1 as usize));
                    } else {
                        ours_music.start_playing(&mut s, &mut rand);
                    }
                }
                6 => ours_music.stop_playing(timer),
                _ => ours_music.update(&mut s, dt),
            }
            let what = format!("round {round}, op {op}");
            assert_eq!(ret::hex_field(&res[k]["bufs_after"][0]), rand.s.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>(), "{what}: Random state");
            assert_eq!(ret::f32(&res[k + 4]).to_bits(), s.volume.to_bits(), "{what}: volume");
            let gc = res[k + 5].get("handle").and_then(Value::as_u64).map(|c| addr(h, c) as usize);
            assert_eq!(gc, s.clip, "{what}: clip");
            assert_eq!(ret::bytes(&res[k + 6])[0] != 0, s.playing, "{what}: isPlaying");
            assert_eq!(ret::bytes(&res[k + 7])[0] != 0, ours_amb.enabled, "{what}: AmbientSound.enabled");
            assert_eq!(ret::bytes(&res[k + 8])[0] != 0, ours_snap.enabled, "{what}: AudioSnapshotManager.enabled");
            assert_eq!(ret::bytes(&res[k + 9])[0] != 0, ours_music.enabled, "{what}: MenuMusic.enabled");
            assert_eq!(ret::bytes(&res[k + 10])[0] != 0, ours_snap.window_focused, "{what}: windowFocused");
            assert_eq!(ret::bytes(&res[k + 11])[0] != 0, ours_snap.alpha_dim, "{what}: alphaDim");
            assert_eq!(ret::f32(&res[k + 12]).to_bits(), ours_music.target_volume.to_bits(), "{what}: targetVolume");
            assert_eq!(ret::f32(&res[k + 13]).to_bits(), ours_music.speed.to_bits(), "{what}: speed");
        }
    }));
    listener_volume(h, 1.0);
    // the scene's manager re-applies its snapshot
    h.request(call(scene_snap, "UnityEngine.Behaviour", "set_enabled", vec![arg::bool(true)], &["System.Boolean"]));
    h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": go}], "param_types": ["UnityEngine.Object"], "main": true}));
    if let Err(e) = result {
        resume_unwind(e);
    }
    eprintln!("600 calls: {ops:?}");
}
