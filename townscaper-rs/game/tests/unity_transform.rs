//! `ts_game::unity::transform` (E11) against the engine. Needs `TS_HARNESS=1`.
//! Random hierarchies of fresh GameObjects (random local positions, rotations and scales, mirrored ones
//! included) are built in the game and in ours; every derived value the game reads back must match to the bit.

mod graph_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::transform::{Node, TransformId, Transforms};
use ts_game::unity::vector::{v3, Vector3};

const T: &str = "UnityEngine.Transform";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn raw3(v: Vector3) -> Value {
    arg::raw(&[v.x, v.y, v.z].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>())
}
fn raw4(q: Quaternion) -> Value {
    arg::raw(&[q.x, q.y, q.z, q.w].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>())
}
fn call(t: u64, m: &str, args: Vec<Value>) -> Value {
    json!({"call": m, "class": T, "handle": t, "args": args, "main": true})
}

pub struct Rng(pub u32);
impl Rng {
    pub fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// In `-r..r`.
    pub fn f(&mut self, r: f32) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0 * 2.0 * r - r
    }
    pub fn v3(&mut self, r: f32) -> Vector3 {
        v3(self.f(r), self.f(r), self.f(r))
    }
    /// A random rotation-ish quaternion (not normalised: the setter normalises).
    pub fn q(&mut self) -> Quaternion {
        Quaternion { x: self.f(1.0), y: self.f(1.0), z: self.f(1.0), w: self.f(1.0) }
    }
    /// A scale: mostly positive, sometimes mirrored or tiny.
    pub fn scale(&mut self) -> Vector3 {
        let mut s = v3(0.2 + self.f(1.0).abs() * 2.0, 0.2 + self.f(1.0).abs() * 2.0, 0.2 + self.f(1.0).abs() * 2.0);
        match self.next() % 8 {
            0 => s.x = -s.x,
            1 => s.y = -s.y,
            2 => {
                s.x = -s.x;
                s.z = -s.z
            }
            3 => s = v3(1.0, 1.0, 1.0),
            _ => {}
        }
        s
    }
}

/// A hierarchy in both: `n` fresh objects, each parented to a random earlier one (or none).
pub struct Pair {
    pub ours: Transforms,
    pub game: Vec<u64>,
    pub gos: Vec<u64>,
}

pub fn build(h: &mut Harness, r: &mut Rng, n: usize) -> Pair {
    let mut ours = Transforms::default();
    let mut game = vec![];
    let mut gos = vec![];
    for i in 0..n {
        let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check transform")], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
        let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
        let parent = if i == 0 || r.next() % 4 == 0 { None } else { Some((r.next() as usize) % i) };
        if let Some(p) = parent {
            ok(h.request(json!({"call": "SetParent", "class": T, "handle": t, "args": [{"handle": game[p]}, arg::bool(false)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true})), "SetParent");
        }
        let (lp, lq, ls) = (r.v3(5.0), r.q(), r.scale());
        let rs = h.batch(vec![call(t, "set_localPosition", vec![raw3(lp)]), call(t, "set_localRotation", vec![raw4(lq)]), call(t, "set_localScale", vec![raw3(ls)])]);
        for x in rs {
            ok(x, "set local");
        }
        let id = ours.add(Node { parent, ..Default::default() });
        ours.set_local_position(id, lp);
        ours.set_local_rotation(id, lq);
        ours.set_local_scale(id, ls);
        game.push(t);
        gos.push(go);
    }
    Pair { ours, game, gos }
}

pub fn get(h: &mut Harness, t: u64, m: &str) -> Vec<u32> {
    bits(&f32s(&ret::bytes(&ok(h.request(call(t, m, vec![])), m))))
}

pub fn destroy(h: &mut Harness, gos: &[u64]) {
    for &g in gos {
        ok(h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": g}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
    }
}

fn q_bits(q: Quaternion) -> Vec<u32> {
    bits(&[q.x, q.y, q.z, q.w])
}
fn v_bits(v: Vector3) -> Vec<u32> {
    bits(&[v.x, v.y, v.z])
}

#[test]
fn positions_and_rotations_match_the_engine() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let mut r = Rng(3);
    for round in 0..6 {
        let mut p = build(h, &mut r, 24);
        for i in 0..p.game.len() {
            let t = p.game[i];
            let what = format!("round {round} node {i}");
            assert_eq!(get(h, t, "get_localRotation"), q_bits(p.ours.local_rotation(i)), "{what}: localRotation (normalised)");
            assert_eq!(get(h, t, "get_position"), v_bits(p.ours.position(i)), "{what}: position");
            assert_eq!(get(h, t, "get_rotation"), q_bits(p.ours.rotation(i)), "{what}: rotation");
            assert_eq!(get(h, t, "get_localToWorldMatrix"), bits(&p.ours.local_to_world_matrix(i).m), "{what}: localToWorldMatrix");
            assert_eq!(get(h, t, "get_worldToLocalMatrix"), bits(&p.ours.world_to_local_matrix(i).m), "{what}: worldToLocalMatrix");
            assert_eq!(get(h, t, "get_lossyScale"), v_bits(p.ours.lossy_scale(i)), "{what}: lossyScale");
            let v = r.v3(10.0);
            for (m, want) in [
                ("TransformPoint", p.ours.transform_point(i, v)),
                ("TransformDirection", p.ours.transform_direction(i, v)),
                ("InverseTransformPoint", p.ours.inverse_transform_point(i, v)),
                ("InverseTransformDirection", p.ours.inverse_transform_direction(i, v)),
            ] {
                let g = bits(&f32s(&ret::bytes(&ok(h.request(json!({"call": m, "class": T, "handle": t, "args": [raw3(v)], "param_types": ["UnityEngine.Vector3"], "main": true})), m))));
                assert_eq!(g, v_bits(want), "{what}: {m}({v:?})");
            }
        }
        // the rotation setter: into the parent's space
        for i in 0..p.game.len() {
            let q = r.q();
            ok(h.request(call(p.game[i], "set_rotation", vec![raw4(q)])), "set_rotation");
            p.ours.set_rotation(i as TransformId, q);
            assert_eq!(get(h, p.game[i], "get_localRotation"), q_bits(p.ours.local_rotation(i)), "round {round} node {i}: set_rotation");
        }
        // SetParent, keeping the world pose or not (never under its own subtree)
        for k in 0..p.game.len() {
            let i = (r.next() as usize) % p.game.len();
            let under = |ours: &Transforms, mut n: usize| loop {
                if n == i {
                    return true;
                }
                match ours.nodes[n].parent {
                    Some(q) => n = q,
                    None => return false,
                }
            };
            let cand: Vec<usize> = (0..p.game.len()).filter(|&n| !under(&p.ours, n)).collect();
            let parent = if r.next() % 5 == 0 || cand.is_empty() { None } else { Some(cand[(r.next() as usize) % cand.len()]) };
            let stays = r.next() % 3 != 0;
            let parg = match parent {
                Some(q) => json!({"handle": p.game[q]}),
                None => json!({"null": true}),
            };
            ok(h.request(json!({"call": "SetParent", "class": T, "handle": p.game[i], "args": [parg, arg::bool(stays)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true})), "SetParent");
            p.ours.set_parent(i, parent, stays);
            let what = format!("round {round} reparent {k}: node {i} to {parent:?}, stays {stays}");
            assert_eq!(get(h, p.game[i], "get_localPosition"), v_bits(p.ours.local_position(i)), "{what}: localPosition");
            assert_eq!(get(h, p.game[i], "get_localRotation"), q_bits(p.ours.local_rotation(i)), "{what}: localRotation");
            assert_eq!(get(h, p.game[i], "get_localScale"), v_bits(p.ours.local_scale(i)), "{what}: localScale");
        }
        // the position setter
        for i in 0..p.game.len() {
            let v = r.v3(20.0);
            ok(h.request(call(p.game[i], "set_position", vec![raw3(v)])), "set_position");
            p.ours.set_position(i as TransformId, v);
            assert_eq!(get(h, p.game[i], "get_localPosition"), v_bits(p.ours.local_position(i)), "round {round} node {i}: set_position");
        }
        destroy(h, &p.gos);
    }
}

/// The scene's own transforms: the hierarchy the world master sits in (squares, qubes, module containers,
/// props, ...), breadth first from its root, read as stored and rebuilt in ours; the bird and butterfly flocks
/// are left out (they move every frame).
#[test]
fn scene_transforms_match_the_engine() {
    use ts_game::unity::sse::X;
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    let mt = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.Component", "handle": master, "args": [], "main": true, "keep": true})), "t"), "t");
    let root = handle(&ok(h.request(json!({"call": "get_root", "class": T, "handle": mt, "args": [], "main": true, "keep": true})), "root"), "root");
    let mut ours = Transforms::default();
    let mut queue = vec![(root, None)];
    let mut game = vec![];
    let mut k = 0;
    while k < queue.len() && game.len() < 1500 {
        let (t, parent) = queue[k];
        k += 1;
        let lp = f32s(&ret::bytes(&ok(h.request(call(t, "get_localPosition", vec![])), "lp")));
        let lr = f32s(&ret::bytes(&ok(h.request(call(t, "get_localRotation", vec![])), "lr")));
        let ls = f32s(&ret::bytes(&ok(h.request(call(t, "get_localScale", vec![])), "ls")));
        let id = ours.add(Node {
            t: X::from_f([lp[0], lp[1], lp[2], 0.0]),
            r: X::from_f([lr[0], lr[1], lr[2], lr[3]]),
            s: X::from_f([ls[0], ls[1], ls[2], 0.0]),
            parent,
        });
        game.push(t);
        let n = ret::i32(&ok(h.request(call(t, "get_childCount", vec![])), "childCount"));
        for c in 0..n {
            let ch = handle(&ok(h.request(json!({"call": "GetChild", "class": T, "handle": t, "args": [arg::i32(c)], "main": true, "keep": true})), "child"), "child");
            // (the flocks animate every frame: their transforms move between reads)
            let name = ret::string(&ok(h.request(json!({"call": "get_name", "class": "UnityEngine.Object", "handle": ch, "args": [], "main": true})), "name"));
            if name.contains("Flock") {
                continue;
            }
            queue.push((ch, Some(id)));
        }
    }
    assert!(game.len() > 50, "too few scene transforms: {}", game.len());
    let mut r = Rng(17);
    for (i, &t) in game.iter().enumerate() {
        let what = format!("scene transform {i}");
        assert_eq!(get(h, t, "get_position"), v_bits(ours.position(i)), "{what}: position");
        assert_eq!(get(h, t, "get_rotation"), q_bits(ours.rotation(i)), "{what}: rotation");
        assert_eq!(get(h, t, "get_lossyScale"), v_bits(ours.lossy_scale(i)), "{what}: lossyScale");
        assert_eq!(get(h, t, "get_localToWorldMatrix"), bits(&ours.local_to_world_matrix(i).m), "{what}: localToWorldMatrix");
        assert_eq!(get(h, t, "get_worldToLocalMatrix"), bits(&ours.world_to_local_matrix(i).m), "{what}: worldToLocalMatrix");
        let v = r.v3(10.0);
        let g = bits(&f32s(&ret::bytes(&ok(h.request(json!({"call": "TransformPoint", "class": T, "handle": t, "args": [raw3(v)], "param_types": ["UnityEngine.Vector3"], "main": true})), "TransformPoint"))));
        assert_eq!(g, v_bits(ours.transform_point(i, v)), "{what}: TransformPoint");
    }
}
