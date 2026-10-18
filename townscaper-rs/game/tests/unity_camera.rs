//! `ts_game::unity::camera` (E12) against the engine. Needs `TS_HARNESS=1`.
//! A fresh, disabled camera under random hierarchies of fresh objects (mirrored scales included), with random
//! field of view, aspect, clip planes and viewport rect; its matrices and every screen / viewport / world
//! conversion the game uses must match to the bit, for points on and off the screen.

mod graph_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::camera::Camera;
use ts_game::unity::geometry::Rect;
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::transform::{Node, Transforms};
use ts_game::unity::vector::{v3, Vector2, Vector3};

const T: &str = "UnityEngine.Transform";
const C: &str = "UnityEngine.Camera";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn raw(v: &[f32]) -> Value {
    arg::raw(&v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>())
}
fn hexf(v: &[f32]) -> String {
    v.iter().flat_map(|f| f.to_le_bytes()).map(|b| format!("{b:02x}")).collect()
}
fn call(class: &str, t: u64, m: &str, args: Vec<Value>) -> Value {
    json!({"call": m, "class": class, "handle": t, "args": args, "main": true})
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// In `-r..r`.
    fn f(&mut self, r: f32) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0 * 2.0 * r - r
    }
    fn v3(&mut self, r: f32) -> Vector3 {
        v3(self.f(r), self.f(r), self.f(r))
    }
    fn q(&mut self) -> Quaternion {
        Quaternion { x: self.f(1.0), y: self.f(1.0), z: self.f(1.0), w: self.f(1.0) }
    }
    fn scale(&mut self) -> Vector3 {
        let mut s = v3(0.2 + self.f(1.0).abs() * 2.0, 0.2 + self.f(1.0).abs() * 2.0, 0.2 + self.f(1.0).abs() * 2.0);
        match self.next() % 6 {
            0 => s.x = -s.x,
            1 => s.y = -s.y,
            2 => s = v3(1.0, 1.0, 1.0),
            _ => {}
        }
        s
    }
}

fn new_go(h: &mut Harness, name: &str) -> (u64, u64) {
    let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
    let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
    (go, t)
}

fn get(h: &mut Harness, class: &str, t: u64, m: &str, args: Vec<Value>) -> Vec<u32> {
    bits(&f32s(&ret::bytes(&ok(h.request(call(class, t, m, args)), m))))
}

#[test]
fn camera_conversions_match_the_engine() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let w = ret::i32(&ok(h.request(json!({"call": "UnityEngine.Screen.get_width", "args": [], "main": true})), "width"));
    let hh = ret::i32(&ok(h.request(json!({"call": "UnityEngine.Screen.get_height", "args": [], "main": true})), "height"));
    let screen = Vector2 { x: w as f32, y: hh as f32 };
    eprintln!("screen {w} x {hh}");
    let cam_ty = handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string("UnityEngine.Camera, UnityEngine.CoreModule")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let mut r = Rng(12);
    let mut gos = vec![];
    for round in 0..40 {
        // a hierarchy, the camera at its end
        let mut ours = Transforms::default();
        let mut game = vec![];
        let depth = (r.next() % 4) as usize;
        for i in 0..=depth {
            let (go, t) = new_go(h, "ts_check camera");
            gos.push(go);
            if i > 0 {
                ok(h.request(json!({"call": "SetParent", "class": T, "handle": t, "args": [{"handle": game[i - 1]}, arg::bool(false)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true})), "SetParent");
            }
            let (lp, lq) = (r.v3(20.0), r.q());
            let ls = if i == depth && r.next() % 2 == 0 { v3(1.0, 1.0, 1.0) } else { r.scale() };
            for x in h.batch(vec![
                call(T, t, "set_localPosition", vec![raw(&[lp.x, lp.y, lp.z])]),
                call(T, t, "set_localRotation", vec![raw(&[lq.x, lq.y, lq.z, lq.w])]),
                call(T, t, "set_localScale", vec![raw(&[ls.x, ls.y, ls.z])]),
            ]) {
                ok(x, "set local");
            }
            let id = ours.add(Node { parent: if i > 0 { Some(i - 1) } else { None }, ..Default::default() });
            ours.set_local_position(id, lp);
            ours.set_local_rotation(id, lq);
            ours.set_local_scale(id, ls);
            game.push(t);
        }
        let go = *gos.last().unwrap();
        let cam = handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": cam_ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "cam");
        ok(h.request(call("UnityEngine.Behaviour", cam, "set_enabled", vec![arg::bool(false)])), "enabled");
        let fov = 5.0 + (r.next() % 1000) as f32 * 0.15;
        let near = 0.01 + (r.next() % 1000) as f32 * 0.002;
        let far = near + 1.0 + (r.next() % 10000) as f32 * 0.37;
        let rect = if round % 3 == 0 {
            Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
        } else {
            Rect { x: r.f(0.3).abs(), y: r.f(0.3).abs(), width: 0.2 + r.f(0.6).abs(), height: 0.2 + r.f(0.6).abs() }
        };
        let mut reqs = vec![
            call(C, cam, "set_fieldOfView", vec![arg::f32(fov)]),
            call(C, cam, "set_nearClipPlane", vec![arg::f32(near)]),
            call(C, cam, "set_farClipPlane", vec![arg::f32(far)]),
            json!({"icall": "UnityEngine.Camera::set_rect_Injected(UnityEngine.Rect&)", "args": [{"handle": cam}, {"buf": hexf(&[rect.x, rect.y, rect.width, rect.height])}], "main": true}),
        ];
        // (the managed aspect accessors are stripped: the icall; without it the aspect is the viewport's)
        let explicit = 0.3 + r.f(1.5).abs();
        if round % 2 == 1 {
            reqs.push(json!({"icall": "UnityEngine.Camera::set_aspect(System.Single)", "args": [{"handle": cam}, {"u32": explicit.to_bits()}], "main": true}));
        }
        for x in h.batch(reqs) {
            ok(x, "camera set");
        }
        let probe = Camera { transform: depth, field_of_view: fov, aspect: 1.0, near, far, rect, projection: None }.screen_viewport_rect(screen);
        let aspect = if round % 2 == 1 { explicit } else { probe.width / probe.height };
        let c = Camera { transform: depth, field_of_view: fov, aspect, near, far, rect, projection: None };
        let what = format!("round {round} (fov {fov}, aspect {aspect}, near {near}, far {far}, rect {rect:?})");
        assert_eq!(get(h, C, cam, "get_projectionMatrix", vec![]), bits(&c.projection_matrix().m), "{what}: projectionMatrix");
        assert_eq!(get(h, C, cam, "get_worldToCameraMatrix", vec![]), bits(&c.world_to_camera_matrix(&ours).m), "{what}: worldToCameraMatrix");
        let pr = get(h, C, cam, "get_pixelRect", vec![]);
        let vr = c.screen_viewport_rect(screen);
        assert_eq!(pr, bits(&[vr.x, vr.y, vr.width, vr.height]), "{what}: pixelRect");
        for k in 0..12 {
            let sp = if k < 9 { v3(r.f(1.0).abs() * screen.x, r.f(1.0).abs() * screen.y, 0.0) } else { v3(r.f(2.0) * screen.x, r.f(2.0) * screen.y, r.f(5.0)) };
            let want = c.screen_point_to_ray(&ours, screen, Vector2 { x: sp.x, y: sp.y });
            let got = get(h, C, cam, "ScreenPointToRay", vec![raw(&[sp.x, sp.y, sp.z])]);
            let o = want.origin;
            let d = want.direction;
            assert_eq!(got, bits(&[o.x, o.y, o.z, d.x, d.y, d.z]), "{what}: ScreenPointToRay {sp:?}");
            let vp = v3(r.f(1.5), r.f(1.5), r.f(10.0));
            let want = c.viewport_point_to_ray(&ours, screen, Vector2 { x: vp.x, y: vp.y });
            let got = get(h, C, cam, "ViewportPointToRay", vec![raw(&[vp.x, vp.y, vp.z])]);
            let (o, d) = (want.origin, want.direction);
            assert_eq!(got, bits(&[o.x, o.y, o.z, d.x, d.y, d.z]), "{what}: ViewportPointToRay {vp:?}");
            let wp = ours.position(depth).add(r.v3(30.0));
            let s = c.world_to_screen_point(&ours, screen, wp);
            assert_eq!(get(h, C, cam, "WorldToScreenPoint", vec![raw(&[wp.x, wp.y, wp.z])]), bits(&[s.x, s.y, s.z]), "{what}: WorldToScreenPoint {wp:?}");
            let v = c.world_to_viewport_point(&ours, screen, wp);
            assert_eq!(get(h, C, cam, "WorldToViewportPoint", vec![raw(&[wp.x, wp.y, wp.z])]), bits(&[v.x, v.y, v.z]), "{what}: WorldToViewportPoint {wp:?}");
        }
        eprintln!("{what}: same");
    }
    for g in gos {
        ok(h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": g}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
    }
}
