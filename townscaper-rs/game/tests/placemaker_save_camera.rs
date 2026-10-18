//! `ts_game::placemaker::save_camera` (G2) against the original game. Needs `TS_HARNESS=1`.
//! The scene's `SaveCamera` takes the thumbnail of the scene's town (`GetImage` on a fresh `SaveData` with a
//! random camera turn); the camera's pose afterwards must match ours to the bit, with its transform chain read
//! from the game into our transform model and the corner columns' voxel positions read from the game.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::graphs::Graph;
use ts_game::placemaker::save_camera::SaveCamera;
use ts_game::unity::mathematics::{Int3, Int3x2};
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::transform::{Node, Transforms};
use ts_game::unity::vector::{v3, Vector3};

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], true, "{what}: {r}");
    r
}
fn handle(r: &Value) -> u64 {
    r["handle"].as_u64().unwrap()
}
fn fs(v: &Value) -> Vec<f32> {
    ret::bytes(v).chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn keep(h: &mut Harness, mut req: Value, what: &str) -> u64 {
    req["keep"] = json!(true);
    handle(&ok(h.request(req), what))
}
fn field(o: u64, name: &str) -> Value {
    json!({"field": name, "handle": o, "main": true})
}
fn tcall(o: u64, m: &str, args: Vec<Value>) -> Value {
    json!({"call": m, "class": "UnityEngine.Transform", "handle": o, "args": args, "main": true})
}
fn v3_of(v: &Value) -> Vector3 {
    let x = fs(v);
    v3(x[0], x[1], x[2])
}

#[test]
fn thumbnail_camera_matches_the_game() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    let ty = keep(h, json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.SaveCamera, Assembly-CSharp")], "param_types": ["System.String"], "main": true}), "type");
    let sc = keep(h, json!({"call": "UnityEngine.Object.FindObjectOfType", "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true}), "SaveCamera");
    let cam = keep(h, field(sc, "cam"), "cam");
    let cam_t = keep(h, json!({"call": "get_transform", "class": "UnityEngine.Component", "handle": cam, "args": [], "main": true}), "cam transform");
    let master = keep(h, field(sc, "master"), "master");
    let graph = keep(h, field(master, "graph"), "graph");
    let corners = keep(h, field(graph, "corners"), "corners");
    // the camera's ancestors, root first
    let mut chain = vec![cam_t];
    loop {
        let r = ok(h.request(json!({"call": "get_parent", "class": "UnityEngine.Transform", "handle": *chain.last().unwrap(), "args": [], "main": true, "keep": true})), "parent");
        match r["handle"].as_u64() {
            Some(p) if r.get("null").is_none() => chain.push(p),
            _ => break,
        }
    }
    chain.reverse();
    eprintln!("camera depth {}", chain.len());
    let mut r = 0x9e37u32;
    let mut next = || {
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        r
    };
    for round in 0..12 {
        let cam_y = (next() % 256) as u8;
        // the town's columns as the game sees them now (the scene town does not change between requests)
        let n = ret::i32(&ok(h.request(tcall(corners, "get_childCount", vec![])), "childCount"));
        let kids: Vec<u64> = h.batch((0..n).map(|i| {
            let mut q = tcall(corners, "GetChild", vec![arg::i32(i)]);
            q["keep"] = json!(true);
            q
        }).collect()).iter().map(handle).collect();
        let counts: Vec<i32> = h.batch(kids.iter().map(|&k| tcall(k, "get_childCount", vec![])).collect()).iter().map(ret::i32).collect();
        let mut columns: Vec<Vec<Vector3>> = vec![];
        for (k, &c) in kids.iter().zip(&counts) {
            let mut reqs = vec![];
            for i in [0, c - 1] {
                if c > 0 {
                    let mut q = tcall(*k, "GetChild", vec![arg::i32(i)]);
                    q["keep"] = json!(true);
                    reqs.push(q);
                }
            }
            let ts: Vec<u64> = h.batch(reqs).iter().map(handle).collect();
            let ps: Vec<Vector3> = h.batch(ts.iter().map(|&t| tcall(t, "get_position", vec![])).collect()).iter().map(v3_of).collect();
            // a column of c voxels: its first and last (one when c is 1)
            let col = match c {
                0 => vec![],
                1 => vec![ps[0]],
                _ => vec![ps[0]; c as usize - 1].into_iter().chain([ps[1]]).collect(),
            };
            columns.push(col);
        }
        let b: Vec<i32> = ret::bytes(&ok(h.request(field(graph, "bounds")), "bounds")).chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
        let bh = ret::i32(&ok(h.request(field(graph, "boundsHeight")), "boundsHeight"));
        // the ancestors' local poses
        let mut tr = Transforms::default();
        let mut parent = None;
        let mut cam_id = 0;
        for &t in &chain {
            let res = h.batch(vec![tcall(t, "get_localPosition", vec![]), tcall(t, "get_localRotation", vec![]), tcall(t, "get_localScale", vec![])]);
            let id = tr.add(Node { parent, ..Default::default() });
            tr.set_local_position(id, v3_of(&res[0]));
            let q = fs(&res[1]);
            tr.set_local_rotation(id, Quaternion { x: q[0], y: q[1], z: q[2], w: q[3] });
            tr.set_local_scale(id, v3_of(&res[2]));
            parent = Some(id);
            cam_id = id;
        }
        let sd = keep(h, json!({"new": "Placemaker.SaveData", "args": [], "main": true}), "SaveData");
        let res = h.batch(vec![
            json!({"field": "cam", "handle": sd, "set_raw": format!("00{cam_y:02x}00"), "main": true}),
            json!({"call": "GetImage", "class": "Placemaker.SaveCamera", "handle": sc, "args": [{"handle": sd}], "param_types": ["Placemaker.SaveData"], "main": true}),
            tcall(cam_t, "get_position", vec![]),
            tcall(cam_t, "get_rotation", vec![]),
            json!({"call": "get_fieldOfView", "class": "UnityEngine.Camera", "handle": cam, "args": [], "main": true}),
        ]);
        for x in &res {
            ok(x.clone(), "GetImage batch");
        }
        let mut g = Graph::default();
        g.bounds = Int3x2 { c0: Int3 { x: b[0], y: b[1], z: b[2] }, c1: Int3 { x: b[3], y: b[4], z: b[5] } };
        g.bounds_height = bh;
        let points = SaveCamera::column_points(&columns);
        let plan = SaveCamera.get_image(&g, cam_y, &mut tr, cam_id, &points);
        let what = format!("round {round}: turn {cam_y}, {} columns, bounds {b:?} height {bh}", columns.len());
        let p = tr.position(cam_id);
        let q = tr.rotation(cam_id);
        let gp = fs(&res[2]);
        let gq = fs(&res[3]);
        assert_eq!([gp[0].to_bits(), gp[1].to_bits(), gp[2].to_bits()], [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()], "{what}: position (game {gp:?}, ours {p:?})");
        assert_eq!([gq[0].to_bits(), gq[1].to_bits(), gq[2].to_bits(), gq[3].to_bits()], [q.x.to_bits(), q.y.to_bits(), q.z.to_bits(), q.w.to_bits()], "{what}: rotation");
        assert_eq!(fs(&res[4])[0].to_bits(), plan.field_of_view.to_bits(), "{what}: fieldOfView");
        eprintln!("{what}: ok");
    }
}
