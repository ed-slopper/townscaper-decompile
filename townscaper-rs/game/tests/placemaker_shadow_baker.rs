//! `ts_game::placemaker::shadow_baker` against the original game (E9a). Needs `TS_HARNESS=1`.
//! The shader ids, the scene baker's lamp texture and texel-size global; then a fresh `ShadowBaker` and `Camera`
//! on an inactive object (so the engine never runs or renders them) with the scene baker's gradients: random
//! angle edits, saves and loads against a stand-in master, then `LateUpdate` frames (easing, colours, lamps and
//! the framing around the scene's town) with the scene master. The scene baker is rebuilt at the end
//! (`OnBuilt`), which resets every global the test touched.

mod graph_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::shadow_baker::*;
use ts_game::unity::geometry::Color;
use ts_game::unity::mathematics::{i3, Int3x2};
use ts_game::unity::quaternion::{Matrix4x4, Quaternion};
use ts_game::unity::vector::{v2, v3, Vector2, Vector3, Vector4};

const SB: &str = "Placemaker.ShadowBaker";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn v2_raw(v: Vector2) -> Value {
    arg::raw(&[v.x.to_le_bytes(), v.y.to_le_bytes()].concat())
}
fn call(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> Value {
    ok(h.request(json!({"call": m, "class": class, "handle": t, "args": args, "main": true})), m)
}
fn call_keep(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> u64 {
    handle(&ok(h.request(json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": true})), m), m)
}
fn field(h: &mut Harness, t: u64, f: &str) -> Vec<u8> {
    ret::bytes(&ok(h.request(json!({"field": f, "handle": t, "main": true})), f))
}
fn field_h(h: &mut Harness, t: u64, f: &str) -> u64 {
    handle(&ok(h.request(json!({"field": f, "handle": t, "main": true})), f), f)
}
fn set_h(h: &mut Harness, t: u64, f: &str, v: u64) {
    ok(h.request(json!({"field": f, "handle": t, "set_handle": v, "main": true})), f);
}
fn set_raw(h: &mut Harness, t: u64, f: &str, b: &[u8]) {
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    ok(h.request(json!({"field": f, "handle": t, "set_raw": hex, "main": true})), f);
}
fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n)
}
fn prop_id(h: &mut Harness, name: &str) -> i32 {
    ret::i32(&ok(h.request(json!({"call": "UnityEngine.Shader.PropertyToID", "args": [arg::string(name)], "main": true})), name))
}
fn global_vector(h: &mut Harness, name: &str) -> Vec<u32> {
    let id = prop_id(h, name);
    let r = ok(h.request(json!({"call": "UnityEngine.Shader.GetGlobalVector", "args": [arg::i32(id)], "param_types": ["System.Int32"], "main": true})), name);
    bits(&f32s(&ret::bytes(&r)))
}
fn global_bytes(r: &Value) -> Vec<u8> {
    match r["bufs_after"][0].as_str() {
        Some(hex) => (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect(),
        None => ret::bytes(r),
    }
}
fn v3_of(b: &[u8]) -> Vector3 {
    let f = f32s(b);
    v3(f[0], f[1], f[2])
}
fn q_of(b: &[u8]) -> Quaternion {
    let f = f32s(b);
    Quaternion { x: f[0], y: f[1], z: f[2], w: f[3] }
}
fn v4bits(v: Vector4) -> Vec<u32> {
    bits(&[v.x, v.y, v.z, v.w])
}
fn qbits(q: Quaternion) -> Vec<u32> {
    bits(&[q.x, q.y, q.z, q.w])
}

/// The game baker's state as ours.
fn read_state(h: &mut Harness, sb: &u64) -> ShadowBaker {
    let sb = *sb;
    let b = |h: &mut Harness, f: &str| field(h, sb, f)[0] != 0;
    let c = f32s(&field(h, sb, "current"));
    let t = f32s(&field(h, sb, "target"));
    let cam = field_h(h, sb, "cam");
    ShadowBaker {
        count: ret::i32(&ok(h.request(json!({"field": "count", "handle": sb, "main": true})), "count")),
        current: v2(c[0], c[1]),
        target: v2(t[0], t[1]),
        framing_dirty: b(h, "framingDirty"),
        angle_dirty: b(h, "angleDirty"),
        snap: b(h, "snap"),
        should_bake: b(h, "shouldBake"),
        lamps: f32s(&field(h, sb, "lamps"))[0],
        enabled: ret::bytes(&call(h, sb, "UnityEngine.Behaviour", "get_enabled", vec![]))[0] != 0,
        camera_enabled: ret::bytes(&call(h, cam, "UnityEngine.Behaviour", "get_enabled", vec![]))[0] != 0,
    }
}

fn same(game: &ShadowBaker, ours: &ShadowBaker, what: &str) {
    let key = |s: &ShadowBaker| {
        (s.count, bits(&[s.current.x, s.current.y, s.target.x, s.target.y, s.lamps]), [s.framing_dirty, s.angle_dirty, s.snap, s.should_bake, s.enabled, s.camera_enabled])
    };
    assert_eq!(key(game), key(ours), "{what}:\n game {game:?}\n ours {ours:?}");
}

/// Our host: colours from the game's gradients, globals and the camera rotation written down.
struct Host<'a> {
    h: &'a mut Harness,
    gradients: [u64; 3],
    vectors: Vec<(String, Vector4)>,
    matrices: Vec<(String, Matrix4x4)>,
    rotation: Option<Quaternion>,
    /// the camera transform's rotation as read after the game's frame
    transform_rotation: Quaternion,
    /// the game camera's transform: a rotation set is read back through it (the engine's transform hands back
    /// a slightly different quaternion from the one set; row E11)
    cam_t: Option<u64>,
}
impl Host<'_> {
    fn eval(&mut self, k: usize, t: f32) -> Color {
        let f = f32s(&ret::bytes(&call(self.h, self.gradients[k], "UnityEngine.Gradient", "Evaluate", vec![arg::f32(t)])));
        Color { r: f[0], g: f[1], b: f[2], a: f[3] }
    }
}
impl SunHost for Host<'_> {
    fn sun_color(&mut self, t: f32) -> Color {
        self.eval(0, t)
    }
    fn shadow_color(&mut self, t: f32) -> Color {
        self.eval(1, t)
    }
    fn bg_color(&mut self, t: f32) -> Color {
        self.eval(2, t)
    }
    fn set_camera_rotation(&mut self, q: Quaternion) {
        self.rotation = Some(match self.cam_t {
            Some(t) => {
                let raw = arg::raw(&[q.x, q.y, q.z, q.w].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>());
                call(self.h, t, "UnityEngine.Transform", "set_rotation", vec![raw]);
                q_of(&ret::bytes(&call(self.h, t, "UnityEngine.Transform", "get_rotation", vec![])))
            }
            None => q,
        });
    }
    fn camera_rotation(&mut self) -> Quaternion {
        self.rotation.unwrap_or(self.transform_rotation)
    }
    fn set_global_vector(&mut self, name: &str, v: Vector4) {
        self.vectors.push((name.into(), v));
    }
    fn set_global_matrix(&mut self, name: &str, m: Matrix4x4) {
        self.matrices.push((name.into(), m));
    }
}

#[test]
fn shadow_baker_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    // ids
    for (f, n) in [
        ("worldToShadowId", ids::WORLD_TO_SHADOW),
        ("shadowToWorldId", ids::SHADOW_TO_WORLD),
        ("shadowRangeId", ids::SHADOW_RANGE),
        ("sunDirId", ids::SUN_DIR),
        ("sunColorId", ids::SUN_COLOR),
        ("shadowColorId", ids::SHADOW_COLOR),
        ("bgColorId", ids::BG_COLOR),
        ("lampId", ids::LAMP),
    ] {
        let s = ret::i32(&ok(h.request(json!({"static_field": format!("{SB}.{f}"), "main": true})), f));
        assert_eq!(s, prop_id(h, n), "{f} = {n}");
    }
    h.wait_for_object("Placemaker.GridGenerator");
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    let live = field_h(h, master, "shadowBaker");
    // the lamp texture and the shadow texture's texel size
    let lamp = field_h(h, live, "lampTex");
    let px: Vec<u8> = f32s(&ret::bytes(&call(h, lamp, "UnityEngine.Texture2D", "GetPixels", vec![]))).iter().map(|f| (f * 255.0).round() as u8).collect();
    let ours: Vec<u8> = lamp_texture().iter().flat_map(|c| [c.r, c.g, c.b, c.a]).collect();
    assert_eq!(px, ours, "lampTex");
    assert_eq!(global_vector(h, ids::SHADOW_TEX_TEXEL_SIZE), v4bits(shadow_tex_texel_size(RESOLUTION)), "_ShadowTex_TexelSize");
    assert_eq!(ret::i32(&ok(h.request(json!({"field": "resolution", "handle": live, "main": true})), "resolution")), RESOLUTION);

    // a fresh baker on an inactive object
    let new_go = |h: &mut Harness, n: &str| handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n);
    let go = new_go(h, "ts_check shadow baker");
    call(h, go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)]);
    let add = |h: &mut Harness, t: &str| {
        let tt = ty(h, t);
        handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": tt}], "param_types": ["System.Type"], "main": true, "keep": true})), t), t)
    };
    let sb = add(h, "Placemaker.ShadowBaker, Assembly-CSharp");
    let cam = add(h, "UnityEngine.Camera, UnityEngine.CoreModule");
    let plane_go = new_go(h, "ts_check shadow plane");
    let plane = call_keep(h, plane_go, "UnityEngine.GameObject", "get_transform", vec![]);
    set_h(h, sb, "cam", cam);
    set_h(h, sb, "shadowPlane", plane);
    let gradients = ["sunColor", "shadowColor", "bgColor"].map(|f| field_h(h, live, f));
    for (f, g) in ["sunColor", "shadowColor", "bgColor"].iter().zip(gradients) {
        set_h(h, sb, f, g);
    }
    let stand_in = handle(&ok(h.request(json!({"new": "Placemaker.WorldMaster", "args": [], "main": true, "keep": true})), "stand-in"), "stand-in");
    set_h(h, sb, "master", stand_in);
    let save = handle(&ok(h.request(json!({"new": "Placemaker.SaveData", "args": [], "main": true, "keep": true})), "save"), "save");

    let mut ours = read_state(h, &sb);
    let mut host = Host { h, gradients, vectors: vec![], matrices: vec![], rotation: None, transform_rotation: Quaternion { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }, cam_t: None };
    let mut s: u32 = 9;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    for k in 0..300 {
        let r = rnd();
        let f = |r: u32| (r >> 8) as f32 / 16_777_216.0 * 2.4 - 0.7;
        let what = match r % 9 {
            0 => {
                let v = v2(f(rnd()), f(rnd()));
                call(host.h, sb, SB, "SetAngle", vec![v2_raw(v)]);
                ours.set_angle(v, &mut host);
                "SetAngle"
            }
            1 => {
                let x = f(rnd());
                call(host.h, sb, SB, "SetX", vec![arg::f32(x)]);
                ours.set_x(x, &mut host);
                "SetX"
            }
            2 => {
                let y = f(rnd());
                call(host.h, sb, SB, "SetY", vec![arg::f32(y)]);
                ours.set_y(y, &mut host);
                "SetY"
            }
            3 | 4 => {
                let v = v2(f(rnd()) * 3.0, f(rnd()) * 0.5);
                call(host.h, sb, SB, "RotateAngle", vec![v2_raw(v)]);
                ours.rotate_angle(v, &mut host);
                "RotateAngle"
            }
            5 => {
                call(host.h, sb, SB, "OnBuilt", vec![]);
                ours.on_built();
                "OnBuilt"
            }
            6 => {
                let b = rnd();
                let sun = SaveSun { h: b as u8, r: (b >> 8) as u8 };
                set_raw(host.h, save, "sun", &[sun.h, sun.r]);
                call(host.h, sb, SB, "Load", vec![json!({"handle": save})]);
                ours.load(sun, &mut host);
                "Load"
            }
            7 => {
                call(host.h, sb, SB, "Save", vec![json!({"handle": save})]);
                let got = field(host.h, save, "sun");
                let want = ours.save();
                assert_eq!((got[0], got[1]), (want.h, want.r), "step {k}: Save of {:?}", ours.target);
                "Save"
            }
            _ => {
                // turn the baker and camera off (as a finished bake leaves them), then BakeShadow
                call(host.h, sb, "UnityEngine.Behaviour", "set_enabled", vec![arg::bool(false)]);
                call(host.h, cam, "UnityEngine.Behaviour", "set_enabled", vec![arg::bool(false)]);
                ours.enabled = false;
                shadow_baker_camera_on_post_render(&mut ours);
                call(host.h, sb, SB, "BakeShadow", vec![]);
                ours.bake_shadow();
                "BakeShadow"
            }
        };
        let game = read_state(host.h, &sb);
        same(&game, &ours, &format!("step {k}: {what}"));
    }

    // frames with the scene's town, once it has finished building (WorldMaster state Done)
    for _ in 0..600 {
        if field(host.h, master, "state")[0] == 43 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(field(host.h, master, "state")[0], 43, "the scene's world never settled");
    set_h(host.h, sb, "master", master);
    let graph = field_h(host.h, master, "graph");
    let b = i32s(&field(host.h, graph, "bounds"));
    let bounds = Int3x2 { c0: i3(b[0], b[1], b[2]), c1: i3(b[3], b[4], b[5]) };
    let corners_t = field_h(host.h, graph, "corners");
    let n = ret::i32(&call(host.h, corners_t, "UnityEngine.Transform", "get_childCount", vec![]));
    let mut corners = vec![];
    for i in 0..n {
        let c = call_keep(host.h, corners_t, "UnityEngine.Transform", "GetChild", vec![arg::i32(i)]);
        let position = v3_of(&ret::bytes(&call(host.h, c, "UnityEngine.Transform", "get_position", vec![])));
        let child_count = ret::i32(&call(host.h, c, "UnityEngine.Transform", "get_childCount", vec![]));
        let last_child_position = if child_count > 0 {
            let l = call_keep(host.h, c, "UnityEngine.Transform", "GetChild", vec![arg::i32(child_count - 1)]);
            v3_of(&ret::bytes(&call(host.h, l, "UnityEngine.Transform", "get_position", vec![])))
        } else {
            Vector3::ZERO
        };
        corners.push(CornerColumn { position, child_count, last_child_position });
    }
    assert!(corners.iter().any(|c| c.child_count > 0), "the scene has no town to frame");
    let cam_t = call_keep(host.h, cam, "UnityEngine.Component", "get_transform", vec![]);
    host.cam_t = Some(cam_t);
    let global_ids: Vec<(&str, i32)> =
        [ids::SUN_COLOR, ids::SHADOW_COLOR, ids::BG_COLOR, ids::LAMP, ids::SUN_DIR, ids::SHADOW_RANGE, ids::WORLD_TO_SHADOW, ids::SHADOW_TO_WORLD]
            .into_iter()
            .map(|n| (n, prop_id(host.h, n)))
            .collect();
    let mut lamp_vectors = 0;
    for k in 0..40 {
        if k % 10 == 0 {
            // a new target (y 0 now and then: night, the lamps)
            let t = v2((rnd() % 1000) as f32 / 1000.0, if k % 20 == 0 { 0.0 } else { (rnd() % 1000) as f32 / 1000.0 });
            set_raw(host.h, sb, "target", &[t.x.to_le_bytes(), t.y.to_le_bytes()].concat());
            ours.target = t;
            if k == 0 {
                call(host.h, sb, SB, "OnBuilt", vec![]);
                ours.on_built();
            }
        }
        // the globals are read in the same batch: the scene's baker may set them again later in the frame
        let mut reqs = vec![
            json!({"static_field": "Placemaker.WorldMaster.expTFast", "main": true}),
            json!({"call": "UnityEngine.Time.get_deltaTime", "args": [], "main": true}),
            json!({"call": "LateUpdate", "class": SB, "handle": sb, "args": [], "main": true}),
        ];
        for (name, id) in &global_ids {
            reqs.push(if name.contains("To") {
                // `GetGlobalMatrix` is stripped: its internal call
                json!({"icall": "UnityEngine.Shader::GetGlobalMatrixImpl_Injected(System.Int32,UnityEngine.Matrix4x4&)", "args": [{"u32": *id as u32}, {"buf": "00".repeat(64)}], "main": true})
            } else {
                json!({"call": "UnityEngine.Shader.GetGlobalVector", "args": [arg::i32(*id)], "param_types": ["System.Int32"], "main": true})
            });
        }
        let r = host.h.batch(reqs);
        let r: Vec<Value> = r.into_iter().map(|v| ok(v, "LateUpdate batch")).collect();
        let globals: std::collections::HashMap<&str, Vec<u32>> =
            global_ids.iter().zip(&r[3..]).map(|((n, _), v)| (*n, bits(&f32s(&global_bytes(v))))).collect();
        let (exp_t, dt) = (ret::f32(&r[0]), ret::f32(&r[1]));
        // the game's camera transform after the frame (rotation from the angle, world-to-local once moved)
        let rot = q_of(&ret::bytes(&call(host.h, cam_t, "UnityEngine.Transform", "get_rotation", vec![])));
        let w2l = Matrix4x4 { m: f32s(&ret::bytes(&call(host.h, cam_t, "UnityEngine.Transform", "get_worldToLocalMatrix", vec![]))).try_into().unwrap() };
        host.vectors.clear();
        host.matrices.clear();
        host.rotation = None;
        host.transform_rotation = rot;
        let mut framing = None;
        let mut frame = |b: &mut ShadowBaker, hst: &mut dyn SunHost| {
            framing = b.maybe_update_framing(bounds, &corners, &mut |_, _| w2l, hst);
        };
        ours.late_update(exp_t, dt, &mut host, &mut frame);
        let game = read_state(host.h, &sb);
        same(&game, &ours, &format!("frame {k}"));
        if host.rotation.is_some() {
            // the angle: against the game's own GetQuaternion (the transform keeps its own copy)
            let r = ok(host.h.request(json!({"call": format!("{SB}.GetQuaternion"), "args": [v2_raw(ours.current)], "main": true})), "GetQuaternion");
            assert_eq!(qbits(ShadowBaker::get_quaternion(ours.current)), bits(&f32s(&ret::bytes(&r))), "frame {k}: GetQuaternion");
        }
        for (name, v) in host.vectors.clone() {
            if name == ids::LAMP {
                lamp_vectors += 1;
            }
            assert_eq!(globals[name.as_str()], v4bits(v), "frame {k}: {name}");
        }
        for (name, m) in host.matrices.clone() {
            assert_eq!(globals[name.as_str()], bits(&m.m), "frame {k}: {name}");
        }
        if let Some(Some(f)) = framing {
            let g = |h: &mut Harness, t: u64, c: &str, m: &str| ret::bytes(&call(h, t, c, m, vec![]));
            assert_eq!(bits(&f32s(&g(host.h, cam_t, "UnityEngine.Transform", "get_position"))), bits(&[f.camera_position.x, f.camera_position.y, f.camera_position.z]), "frame {k}: camera position");
            // (the camera's size and clip getters are stripped; `_WorldToShadow` is built from the size and the
            // depth extent, so its match covers them)
            assert_eq!(bits(&f32s(&g(host.h, plane, "UnityEngine.Transform", "get_position"))), bits(&[f.plane_position.x, f.plane_position.y, f.plane_position.z]), "frame {k}: plane position");
            // the transform's copy of a rotation is not the one set (row E11): ours goes through the same round
            // trip on the plane itself (the game set it to this same value, if ours is right)
            let game_rot = bits(&f32s(&g(host.h, plane, "UnityEngine.Transform", "get_rotation")));
            let q = f.plane_rotation;
            call(host.h, plane, "UnityEngine.Transform", "set_rotation", vec![arg::raw(&[q.x, q.y, q.z, q.w].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>())]);
            assert_eq!(game_rot, bits(&f32s(&g(host.h, plane, "UnityEngine.Transform", "get_rotation"))), "frame {k}: plane rotation");
            assert_eq!(bits(&f32s(&g(host.h, plane, "UnityEngine.Transform", "get_localScale"))), bits(&[f.plane_scale.x, f.plane_scale.y, f.plane_scale.z]), "frame {k}: plane scale");
        } else {
            assert!(k > 0, "the first frame frames the town");
        }
    }
    assert!(lamp_vectors > 0, "the lamps never moved");

    // clean up: our objects gone, the scene baker redoes its globals and bake
    for o in [go, plane_go] {
        ok(host.h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": o}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
    }
    call(host.h, live, SB, "OnBuilt", vec![]);
}
