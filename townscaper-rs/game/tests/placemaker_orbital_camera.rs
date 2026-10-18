//! `ts_game::placemaker::ui::orbital_camera` (G2) against the original game. Needs `TS_HARNESS=1`.
//! The scene's `OrbitalCamera` is given random state and runs its own `LateUpdate` in one batch with the
//! frame's inputs read (delta time, screen, dim, game view rect, any voxels): every field after it must match
//! ours to the bit, and the camera's pose must match ours set on a fresh transform. The input methods (rotate,
//! flick, zoom, focus, remap, save / load) are checked the same way.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::save_data::SaveCam;
use ts_game::placemaker::ui::orbital_camera::{Float3x2, OrbitContext, OrbitalCamera};
use ts_game::unity::geometry::Rect;
use ts_game::unity::mathematics::{f2, f3};
use ts_game::unity::vector::{v3, Vector3};

const OC: &str = "Placemaker.Ui.OrbitalCamera";

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], true, "{what}: {r}");
    r
}
fn handle(r: &Value) -> u64 {
    r["handle"].as_u64().unwrap()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn f(v: &Value) -> f32 {
    f32::from_le_bytes(ret::bytes(v)[..4].try_into().unwrap())
}
fn fs(v: &Value) -> Vec<f32> {
    ret::bytes(v).chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn field(o: u64, name: &str) -> Value {
    json!({"field": name, "handle": o, "main": true})
}
fn set(o: u64, name: &str, b: &[u8]) -> Value {
    json!({"field": name, "handle": o, "set_raw": hex(b), "main": true})
}
fn call(o: u64, class: &str, m: &str, args: Vec<Value>, types: &[&str]) -> Value {
    json!({"call": m, "class": class, "handle": o, "args": args, "param_types": types, "main": true})
}
fn find(h: &mut Harness, t: &str) -> u64 {
    let ty = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(t)], "param_types": ["System.String"], "main": true, "keep": true})), t));
    handle(&ok(h.request(json!({"call": "UnityEngine.Object.FindObjectOfType", "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true})), t))
}
fn keep(h: &mut Harness, mut req: Value, what: &str) -> u64 {
    req["keep"] = json!(true);
    handle(&ok(h.request(req), what))
}

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

fn f3x2_bytes(b: &Float3x2) -> Vec<u8> {
    [b.c0.x, b.c0.y, b.c0.z, b.c1.x, b.c1.y, b.c1.z].iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn f3x2(v: &[f32]) -> Float3x2 {
    Float3x2 { c0: f3(v[0], v[1], v[2]), c1: f3(v[3], v[4], v[5]) }
}

/// The float fields in layout order, with our accessor.
const FLOATS: [&str; 22] = [
    "zoomTarget", "zoomCurrent", "rotationY", "rotationX", "panHexCurrentX", "panHexCurrentY", "panHeightCurrent",
    "panHexTargetX", "panHexTargetY", "panHeightTarget", "rotationSpeedX", "rotationSpeedY", "heightCurrent",
    "heightTarget", "anyVoxelsCurrent", "minRadius", "currentDist", "nonDollyDist", "lastBoundsChangeTime",
    "distMin", "distMax", "snap",
];
fn get(o: &OrbitalCamera, name: &str) -> f32 {
    match name {
        "zoomTarget" => o.zoom_target,
        "zoomCurrent" => o.zoom_current,
        "rotationY" => o.rotation_y,
        "rotationX" => o.rotation_x,
        "panHexCurrentX" => o.pan_hex_current_x,
        "panHexCurrentY" => o.pan_hex_current_y,
        "panHeightCurrent" => o.pan_height_current,
        "panHexTargetX" => o.pan_hex_target_x,
        "panHexTargetY" => o.pan_hex_target_y,
        "panHeightTarget" => o.pan_height_target,
        "rotationSpeedX" => o.rotation_speed_x,
        "rotationSpeedY" => o.rotation_speed_y,
        "heightCurrent" => o.height_current,
        "heightTarget" => o.height_target,
        "anyVoxelsCurrent" => o.any_voxels_current,
        "minRadius" => o.min_radius,
        "currentDist" => o.current_dist,
        "nonDollyDist" => o.non_dolly_dist,
        "lastBoundsChangeTime" => o.last_bounds_change_time,
        "distMin" => o.dist_min,
        "distMax" => o.dist_max,
        _ => unreachable!(),
    }
}
fn set_ours(o: &mut OrbitalCamera, name: &str, v: f32) {
    let r = match name {
        "zoomTarget" => &mut o.zoom_target,
        "zoomCurrent" => &mut o.zoom_current,
        "rotationY" => &mut o.rotation_y,
        "rotationX" => &mut o.rotation_x,
        "panHexCurrentX" => &mut o.pan_hex_current_x,
        "panHexCurrentY" => &mut o.pan_hex_current_y,
        "panHeightCurrent" => &mut o.pan_height_current,
        "panHexTargetX" => &mut o.pan_hex_target_x,
        "panHexTargetY" => &mut o.pan_hex_target_y,
        "panHeightTarget" => &mut o.pan_height_target,
        "rotationSpeedX" => &mut o.rotation_speed_x,
        "rotationSpeedY" => &mut o.rotation_speed_y,
        "heightCurrent" => &mut o.height_current,
        "heightTarget" => &mut o.height_target,
        "anyVoxelsCurrent" => &mut o.any_voxels_current,
        "minRadius" => &mut o.min_radius,
        "currentDist" => &mut o.current_dist,
        "nonDollyDist" => &mut o.non_dolly_dist,
        "lastBoundsChangeTime" => &mut o.last_bounds_change_time,
        "distMin" => &mut o.dist_min,
        "distMax" => &mut o.dist_max,
        _ => unreachable!(),
    };
    *r = v;
}

/// A random camera state.
fn random_state(r: &mut Rng) -> OrbitalCamera {
    let mut o = OrbitalCamera::default();
    for name in &FLOATS[..21] {
        let v = match *name {
            "rotationSpeedX" | "rotationSpeedY" => {
                if r.next() % 3 == 0 {
                    0.0
                } else {
                    r.f(-2.0, 2.0)
                }
            }
            "heightCurrent" | "heightTarget" => r.f(0.0, 40.0),
            "minRadius" => 3.5,
            "currentDist" => r.f(3.0, 300.0),
            "nonDollyDist" => r.f(2.0, 60.0),
            "distMin" => r.f(1.9, 2.1),
            "distMax" => r.f(4.0, 80.0),
            "lastBoundsChangeTime" => r.f(0.0, 100.0),
            "rotationY" => r.f(-1.5, 1.5),
            _ => r.f(-0.2, 1.2),
        };
        set_ours(&mut o, name, v);
    }
    let b = |r: &mut Rng| {
        let lo = f3(r.f(-40.0, 0.0).round(), r.f(-40.0, 0.0).round(), r.f(-40.0, 0.0).round());
        let hi = f3(lo.x + r.f(0.0, 50.0).round(), lo.y + r.f(0.0, 50.0).round(), lo.z + r.f(0.0, 50.0).round());
        Float3x2 { c0: lo, c1: hi }
    };
    o.bounds_current = b(r);
    o.bounds_target = if r.next() % 4 == 0 { o.bounds_current } else { b(r) };
    o.snap = r.next() % 5 == 0;
    o
}

/// The requests that give the game's camera `o`.
fn set_state(oc: u64, o: &OrbitalCamera) -> Vec<Value> {
    let mut reqs: Vec<Value> = FLOATS[..21].iter().map(|n| set(oc, n, &get(o, n).to_le_bytes())).collect();
    reqs.push(set(oc, "boundsCurrent", &f3x2_bytes(&o.bounds_current)));
    reqs.push(set(oc, "boundsTarget", &f3x2_bytes(&o.bounds_target)));
    reqs.push(set(oc, "snap", &[o.snap as u8]));
    reqs
}

/// The requests that read the game's camera state back (in `read_state` order).
fn get_state(oc: u64) -> Vec<Value> {
    let mut reqs: Vec<Value> = FLOATS[..21].iter().map(|n| field(oc, n)).collect();
    reqs.push(field(oc, "boundsCurrent"));
    reqs.push(field(oc, "boundsTarget"));
    reqs.push(field(oc, "snap"));
    reqs
}
fn read_state(res: &[Value]) -> OrbitalCamera {
    let mut o = OrbitalCamera::default();
    for (i, n) in FLOATS[..21].iter().enumerate() {
        set_ours(&mut o, n, f(&res[i]));
    }
    o.bounds_current = f3x2(&fs(&res[21]));
    o.bounds_target = f3x2(&fs(&res[22]));
    o.snap = ret::bytes(&res[23])[0] != 0;
    o
}

fn same(game: &OrbitalCamera, ours: &OrbitalCamera, what: &str) {
    for n in &FLOATS[..21] {
        assert_eq!(get(game, n).to_bits(), get(ours, n).to_bits(), "{what}: {n} (game {}, ours {})", get(game, n), get(ours, n));
    }
    let bits = |b: &Float3x2| f3x2_bytes(b);
    assert_eq!(bits(&game.bounds_current), bits(&ours.bounds_current), "{what}: boundsCurrent");
    assert_eq!(bits(&game.bounds_target), bits(&ours.bounds_target), "{what}: boundsTarget");
    assert_eq!(game.snap, ours.snap, "{what}: snap");
}

struct Scene {
    oc: u64,
    cam_t: u64,
    cam: u64,
    gvr: u64,
    alpha: u64,
    graph: u64,
    probe: u64,
}

fn scene(h: &mut Harness) -> Scene {
    let oc = find(h, "Placemaker.Ui.OrbitalCamera, Assembly-CSharp");
    let master = keep(h, field(oc, "master"), "master");
    let cam = keep(h, field(oc, "cam"), "cam");
    let cam_t = keep(h, call(cam, "UnityEngine.Component", "get_transform", vec![], &[]), "cam transform");
    let gvr = keep(h, field(master, "gameViewRect"), "gameViewRect");
    let dim = keep(h, field(master, "dim"), "dim");
    let alpha = keep(h, field(dim, "alphaState"), "alphaState");
    let graph = keep(h, call(master, "Placemaker.Ui.UiMaster", "get_graph", vec![], &[]), "graph");
    let go = keep(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check orbit probe")], "param_types": ["System.String"], "main": true}), "probe");
    let probe = keep(h, call(go, "UnityEngine.GameObject", "get_transform", vec![], &[]), "probe transform");
    Scene { oc, cam_t, cam, gvr, alpha, graph, probe }
}

fn v3_of(v: &Value) -> Vector3 {
    let x = fs(v);
    v3(x[0], x[1], x[2])
}

#[test]
fn late_update_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let s = scene(h);
    let saved = h.batch(get_state(s.oc));
    let mut r = Rng(31);
    let rounds: usize = std::env::var("TS_ORBIT_ROUNDS").ok().and_then(|x| x.parse().ok()).unwrap_or(300);
    let (mut shifted, mut nan_poses) = (0, 0);
    // the game view rect's margins (the menus' share of the screen), moved in half the rounds
    let sides = ["left", "bottom", "right", "top"];
    let margins: Vec<f32> = h.batch(sides.iter().map(|m| call(s.gvr, "Placemaker.Ui.GameViewRect", &format!("get_{m}"), vec![], &[])).collect()).iter().map(f).collect();
    for round in 0..rounds {
        let o = random_state(&mut r);
        let mut reqs = set_state(s.oc, &o);
        let moved = r.next() % 2 == 0;
        for (k, m) in sides.iter().enumerate() {
            let v = if moved { r.f(-40.0, 200.0) } else { margins[k] };
            reqs.push(call(s.gvr, "Placemaker.Ui.GameViewRect", &format!("set_{m}"), vec![arg::f32(v)], &["System.Single"]));
        }
        let n = reqs.len();
        reqs.push(call(s.oc, OC, "LateUpdate", vec![], &[]));
        reqs.push(json!({"call": "UnityEngine.Time.get_deltaTime", "args": [], "main": true}));
        reqs.push(json!({"call": "UnityEngine.Screen.get_width", "args": [], "main": true}));
        reqs.push(json!({"call": "UnityEngine.Screen.get_height", "args": [], "main": true}));
        reqs.push(field(s.alpha, "_current"));
        reqs.push(call(s.gvr, "Placemaker.Ui.GameViewRect", "GetRelativeRect", vec![], &[]));
        reqs.push(call(s.graph, "Placemaker.Graphs.Graph", "get_anyVoxels", vec![], &[]));
        reqs.push(call(s.cam_t, "UnityEngine.Transform", "get_position", vec![], &[]));
        reqs.push(call(s.cam_t, "UnityEngine.Transform", "get_rotation", vec![], &[]));
        reqs.push(call(s.cam, "UnityEngine.Camera", "get_fieldOfView", vec![], &[]));
        reqs.push(call(s.cam, "UnityEngine.Camera", "get_farClipPlane", vec![], &[]));
        reqs.extend(get_state(s.oc));
        let res = h.batch(reqs);
        for x in &res {
            ok(x.clone(), "late update batch");
        }
        let at = n + 1;
        let rect = fs(&res[at + 4]);
        let cx = OrbitContext {
            dt: f(&res[at]),
            screen_width: ret::i32(&res[at + 1]),
            screen_height: ret::i32(&res[at + 2]),
            dim_alpha: f(&res[at + 3]),
            game_view_rect: Rect { x: rect[0], y: rect[1], width: rect[2], height: rect[3] },
            any_voxels: ret::bytes(&res[at + 5])[0] != 0,
        };
        let c = cx.game_view_rect.center();
        if (c.x - 0.5).abs() > 1e-4 || (c.y - 0.5).abs() > 1e-4 {
            shifted += 1;
        }
        let mut ours = o;
        let pose = ours.late_update(&cx);
        let what = format!("round {round}: {o:?} with {cx:?} -> {ours:?} {pose:?}");
        let game = read_state(&res[at + 10..]);
        same(&game, &ours, &what);
        assert_eq!(f(&res[at + 8]).to_bits(), pose.field_of_view.to_bits(), "{what}: fieldOfView");
        assert_eq!(f(&res[at + 9]).to_bits(), pose.far_clip_plane.to_bits(), "{what}: farClipPlane");
        // our pose through a fresh transform, as the camera's goes through its own (a NaN pose, from a pan
        // height below the ground, is a transform's business: the engine rejects it on either side)
        let p = pose.position;
        if p.x.is_nan() || pose.rotation.w.is_nan() {
            nan_poses += 1;
            continue;
        }
        let q = pose.rotation;
        let probe = h.batch(vec![
            call(s.probe, "UnityEngine.Transform", "set_position", vec![arg::raw(&[p.x, p.y, p.z].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())], &["UnityEngine.Vector3"]),
            call(s.probe, "UnityEngine.Transform", "set_rotation", vec![arg::raw(&[q.x, q.y, q.z, q.w].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())], &["UnityEngine.Quaternion"]),
            call(s.probe, "UnityEngine.Transform", "get_position", vec![], &[]),
            call(s.probe, "UnityEngine.Transform", "get_rotation", vec![], &[]),
        ]);
        assert_eq!(ret::bytes(&probe[2]), ret::bytes(&res[at + 6]), "{what}: position (ours {p:?}, game {:?})", v3_of(&res[at + 6]));
        assert_eq!(ret::bytes(&probe[3]), ret::bytes(&res[at + 7]), "{what}: rotation (ours {q:?}, game {:?})", fs(&res[at + 7]));
    }
    eprintln!("{rounds} late updates, {shifted} with the view rect off centre, {nan_poses} NaN poses");
    let restore = read_state(&saved);
    let mut reqs = set_state(s.oc, &restore);
    for (k, m) in sides.iter().enumerate() {
        reqs.push(call(s.gvr, "Placemaker.Ui.GameViewRect", &format!("set_{m}"), vec![arg::f32(margins[k])], &["System.Single"]));
    }
    for x in h.batch(reqs) {
        ok(x, "restore");
    }
}

#[test]
fn inputs_match_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let s = scene(h);
    let saved = h.batch(get_state(s.oc));
    let mut r = Rng(77);
    let v2 = |x: f32, y: f32| arg::raw(&[x.to_le_bytes(), y.to_le_bytes()].concat());
    let v3a = |p: Vector3| arg::raw(&[p.x, p.y, p.z].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());
    for round in 0..400 {
        let o = random_state(&mut r);
        let kind = round % 8;
        let (a, b) = (r.f(-3.0, 3.0), r.f(-3.0, 3.0));
        let wp = v3(r.f(-30.0, 30.0), r.f(-5.0, 40.0), r.f(-30.0, 30.0));
        let nb = random_state(&mut r).bounds_target;
        let (oh, nh) = (r.f(0.0, 30.0), r.f(0.0, 30.0));
        let saved_cam = SaveCam { x: (r.next() % 256) as u8, y: (r.next() % 256) as u8, z: (r.next() % 256) as u8 };
        let mut reqs = set_state(s.oc, &o);
        let n = reqs.len();
        let f3x2_arg = |b: &Float3x2| arg::raw(&f3x2_bytes(b));
        reqs.push(match kind {
            0 => call(s.oc, OC, "RotateCamera", vec![v2(a, b)], &["Unity.Mathematics.float2"]),
            1 => call(s.oc, OC, "SetRotationSpeed", vec![v2(a, b)], &["Unity.Mathematics.float2"]),
            2 => call(s.oc, OC, "ScrollZoom", vec![arg::f32(a)], &["System.Single"]),
            3 => call(s.oc, OC, "PinchZoom", vec![arg::f32(b.abs())], &["System.Single"]),
            4 => call(s.oc, OC, "GamepadZoom", vec![arg::f32(a)], &["System.Single"]),
            5 => call(s.oc, OC, "Focus", vec![v3a(wp)], &["UnityEngine.Vector3"]),
            6 => call(s.oc, OC, "RemapPans", vec![f3x2_arg(&o.bounds_current), f3x2_arg(&nb), arg::f32(oh), arg::f32(nh)], &["Unity.Mathematics.float3x2", "Unity.Mathematics.float3x2", "System.Single", "System.Single"]),
            _ => call(s.oc, OC, "StopRotation", vec![], &[]),
        });
        reqs.push(json!({"call": "UnityEngine.Time.get_unscaledDeltaTime", "args": [], "main": true}));
        reqs.extend(get_state(s.oc));
        let res = h.batch(reqs);
        for x in &res {
            ok(x.clone(), "input batch");
        }
        let udt = f(&res[n + 1]);
        let mut ours = o;
        match kind {
            0 => ours.rotate_camera(f2(a, b)),
            1 => ours.set_rotation_speed(f2(a, b)),
            2 => ours.scroll_zoom(a),
            3 => ours.pinch_zoom(b.abs()),
            4 => ours.gamepad_zoom(a, udt),
            5 => ours.focus(wp),
            6 => ours.remap_pans(o.bounds_current, nb, oh, nh),
            _ => ours.stop_rotation(),
        }
        let what = format!("round {round} kind {kind} ({a}, {b}, {wp:?}): {o:?}");
        same(&read_state(&res[n + 2..]), &ours, &what);
        // GetWorldPos / GetRelativePos as statics, and save / load through a fresh SaveData
        let wpos = OrbitalCamera::get_world_pos(o.pan_hex_target_x, o.pan_hex_target_y, o.pan_height_target, o.bounds_current, oh);
        let rel = OrbitalCamera::get_relative_pos(wp, nb, nh);
        let res = h.batch(vec![
            json!({"call": "Placemaker.Ui.OrbitalCamera.GetWorldPos", "args": [arg::f32(o.pan_hex_target_x), arg::f32(o.pan_hex_target_y), arg::f32(o.pan_height_target), f3x2_arg(&o.bounds_current), arg::f32(oh)], "param_types": ["System.Single", "System.Single", "System.Single", "Unity.Mathematics.float3x2", "System.Single"], "main": true}),
            json!({"call": "Placemaker.Ui.OrbitalCamera.GetRelativePos", "args": [v3a(wp), f3x2_arg(&nb), arg::f32(nh)], "param_types": ["UnityEngine.Vector3", "Unity.Mathematics.float3x2", "System.Single"], "main": true}),
        ]);
        let g = fs(&ok(res[0].clone(), "GetWorldPos"));
        assert_eq!([g[0].to_bits(), g[1].to_bits(), g[2].to_bits()], [wpos.x.to_bits(), wpos.y.to_bits(), wpos.z.to_bits()], "{what}: GetWorldPos");
        let g = fs(&ok(res[1].clone(), "GetRelativePos"));
        assert_eq!([g[0].to_bits(), g[1].to_bits(), g[2].to_bits()], [rel.0.to_bits(), rel.1.to_bits(), rel.2.to_bits()], "{what}: GetRelativePos");
        if round % 4 == 0 {
            let sd = keep(h, json!({"new": "Placemaker.SaveData", "args": [], "main": true}), "SaveData");
            let mut reqs = set_state(s.oc, &o);
            reqs.push(call(s.oc, OC, "Save", vec![json!({"handle": sd})], &["Placemaker.SaveData"]));
            reqs.push(field(sd, "cam"));
            reqs.push(set(sd, "cam", &[saved_cam.x, saved_cam.y, saved_cam.z]));
            reqs.push(call(s.oc, OC, "Load", vec![json!({"handle": sd})], &["Placemaker.SaveData"]));
            reqs.extend(get_state(s.oc));
            let res = h.batch(reqs);
            for x in &res {
                ok(x.clone(), "save batch");
            }
            let cam = ret::bytes(&res[n + 1]);
            let ours_saved = o.save();
            assert_eq!(&cam[..3], &[ours_saved.x, ours_saved.y, ours_saved.z], "{what}: Save");
            let mut ours = o;
            ours.load(saved_cam);
            same(&read_state(&res[n + 4..]), &ours, &format!("{what}: Load {saved_cam:?}"));
        }
    }
    let restore = read_state(&saved);
    for x in h.batch(set_state(s.oc, &restore)) {
        ok(x, "restore");
    }
}

#[test]
fn pan_and_wasd_match_the_game() {
    use ts_game::placemaker::graphs::{CornerId, Graph};
    use ts_game::unity::geometry::Ray;
    use ts_game::unity::mathematics::{Int2, Int3, Int3x2};
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let s = scene(h);
    let saved = h.batch(get_state(s.oc));
    let master = keep(h, field(s.oc, "master"), "master");
    let scene_wm = keep(h, field(master, "worldMaster"), "worldMaster");
    // an empty town: a fresh WorldMaster and Graph swapped in for the call (one batch, so no frame sees it)
    let wm = keep(h, json!({"new": "Placemaker.WorldMaster", "args": [], "main": true}), "new WorldMaster");
    let g2 = keep(h, json!({"new": "Placemaker.Graphs.Graph", "args": [], "main": true}), "new Graph");
    ok(h.request(json!({"field": "graph", "handle": wm, "set_handle": g2, "main": true})), "graph");
    let main_cam = keep(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true}), "Camera.main");
    let scene_bounds = ret::bytes(&ok(h.request(field(s.graph, "bounds")), "bounds"));
    let i32s = |b: &[u8]| b.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<i32>>();
    let sb = i32s(&scene_bounds);
    let mut r = Rng(5);
    let (mut moved, mut recentred) = (0, 0);
    for round in 0..400 {
        let kind = round % 4;
        let empty = kind >= 2;
        let mut o = random_state(&mut r);
        o.current_dist = r.f(3.0, 120.0);
        o.non_dolly_dist = r.f(2.0, 60.0);
        if r.next() % 8 == 0 {
            o.height_current = 0.0;
        }
        let pos = f2(r.f(0.0, 640.0), r.f(0.0, 360.0));
        let d2 = f2(r.f(-60.0, 60.0), r.f(-60.0, 60.0));
        let key = v3(r.f(-1.0, 1.0), r.f(-1.0, 1.0), r.f(-1.0, 1.0));
        let c = Int3 { x: (r.next() % 40) as i32 - 20, y: (r.next() % 40) as i32 - 20, z: 0 };
        let eb = Int3x2 { c0: Int3 { x: c.x, y: c.y, z: -c.x - c.y }, c1: Int3 { x: c.x, y: c.y, z: -c.x - c.y } };
        let eb_bytes: Vec<u8> = [eb.c0.x, eb.c0.y, eb.c0.z, eb.c1.x, eb.c1.y, eb.c1.z].iter().flat_map(|x| x.to_le_bytes()).collect();
        let v3b = |p: Vector3| arg::raw(&[p.x, p.y, p.z].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());
        let mut reqs = set_state(s.oc, &o);
        reqs.push(call(main_cam, "UnityEngine.Camera", "ScreenPointToRay", vec![v3b(v3(pos.x, pos.y, 0.0))], &["UnityEngine.Vector3"]));
        reqs.push(call(main_cam, "UnityEngine.Camera", "ScreenPointToRay", vec![v3b(v3(d2.x + pos.x, d2.y + pos.y, 0.0))], &["UnityEngine.Vector3"]));
        if empty {
            reqs.push(json!({"field": "worldMaster", "handle": master, "set_handle": wm, "main": true}));
            reqs.push(set(g2, "bounds", &eb_bytes));
            reqs.push(set(wm, "state", &[0]));
        }
        let n = reqs.len();
        reqs.push(if kind % 2 == 0 {
            call(s.oc, OC, "WASDCamera", vec![v3b(key)], &["UnityEngine.Vector3"])
        } else {
            call(s.oc, OC, "PanCamera", vec![arg::raw(&[pos.x, pos.y].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()), arg::raw(&[d2.x, d2.y].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())], &["UnityEngine.Vector2", "UnityEngine.Vector2"])
        });
        reqs.push(json!({"call": "UnityEngine.Time.get_unscaledDeltaTime", "args": [], "main": true}));
        reqs.push(field(g2, "bounds"));
        reqs.push(field(wm, "state"));
        if empty {
            reqs.push(json!({"field": "worldMaster", "handle": master, "set_handle": scene_wm, "main": true}));
        } else {
            reqs.push(call(s.graph, "Placemaker.Graphs.Graph", "get_anyVoxels", vec![], &[]));
        }
        reqs.extend(get_state(s.oc));
        let res = h.batch(reqs);
        for x in &res {
            ok(x.clone(), "pan batch");
        }
        let ray = |v: &Value| {
            let x = fs(v);
            Ray { origin: v3(x[0], x[1], x[2]), direction: v3(x[3], x[4], x[5]) }
        };
        let at = 24;
        let (r1, r2) = (ray(&res[at]), ray(&res[at + 1]));
        let udt = f(&res[n + 1]);
        let mut graph = Graph::default();
        if empty {
            graph.bounds = eb;
        } else {
            assert!(ret::bytes(&res[n + 4])[0] != 0, "the scene town has voxels");
            graph.corners.push(CornerId(0));
            graph.bounds = Int3x2 { c0: Int3 { x: sb[0], y: sb[1], z: sb[2] }, c1: Int3 { x: sb[3], y: sb[4], z: sb[5] } };
        }
        let mut ours = o;
        let centre: Option<Int2> = if kind % 2 == 0 { ours.wasd_camera(key, udt, &graph) } else { ours.pan_camera(&r1, &r2, &graph) };
        let what = format!("round {round} kind {kind} ({pos:?} by {d2:?}, key {key:?}): {o:?}");
        same(&read_state(&res[n + 5..]), &ours, &what);
        if empty {
            let state = ret::bytes(&res[n + 3])[0];
            let gb = i32s(&ret::bytes(&res[n + 2]));
            if let Some(c) = centre {
                graph.set_empty_bounds_center(c);
                recentred += 1;
            }
            let ob = [graph.bounds.c0.x, graph.bounds.c0.y, graph.bounds.c0.z, graph.bounds.c1.x, graph.bounds.c1.y, graph.bounds.c1.z];
            assert_eq!(gb, ob, "{what}: graph bounds");
            assert_eq!(state == 0x13, centre.is_some(), "{what}: ResetState");
        } else if ours.pan_hex_target_x != o.pan_hex_target_x || ours.pan_hex_current_x != o.pan_hex_current_x {
            moved += 1;
        }
    }
    eprintln!("400 pans / key moves: {moved} moved the pan point, {recentred} recentred an empty town");
    let restore = read_state(&saved);
    for x in h.batch(set_state(s.oc, &restore)) {
        ok(x, "restore");
    }
}
