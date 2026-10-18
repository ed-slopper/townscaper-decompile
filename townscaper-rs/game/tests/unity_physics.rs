//! `ts_game::unity::physics` (E12) against the engine. Needs `TS_HARNESS=1`.
//! Every active `MeshCollider` (the voxels' convex collider prisms) and `BoxCollider` (the ground) of the
//! scene's town is read from the game (pose, layer, mesh points, box size and centre); rays through the camera
//! and random rays are cast by the game's `Physics.Raycast(origin, direction, out hit, 1e7, clickMask)` and by
//! ours: hit or miss must agree, and the point, normal and distance match to 1e-4 (the hull planes are our
//! stand-in for PhysX's cooking: in practice all but about 1% of hits are bit-exact; `TS_PHYS_DIFF=1` prints
//! the others). HoverData's enlarged helpers (`LastHitVoxel`, `LastNewVoxel`) are parked on voxels for the
//! scaled-mesh path.

mod graph_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::unity::physics::{raycast, Collider, ConvexHull, Pose, Shape};
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::vector::{v3, Vector3};

const T: &str = "UnityEngine.Transform";
const V3T: &str = "UnityEngine.Vector3, UnityEngine.CoreModule";

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
fn call(class: &str, t: u64, m: &str, args: Vec<Value>, keep: bool) -> Value {
    json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": keep})
}
fn req(h: &mut Harness, r: Value, what: &str) -> Value {
    ok(h.request(r), what)
}
fn hcall(h: &mut Harness, class: &str, t: u64, m: &str, args: Vec<Value>) -> u64 {
    handle(&req(h, call(class, t, m, args, true), m), m)
}
fn fl(h: &mut Harness, class: &str, t: u64, m: &str) -> Vec<f32> {
    f32s(&ret::bytes(&req(h, call(class, t, m, vec![], false), m)))
}
fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&req(h, json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true}), n), n)
}
fn array_items(h: &mut Harness, a: u64) -> Vec<u64> {
    let n = ret::i32(&req(h, call("System.Array", a, "get_Length", vec![], false), "n"));
    (0..n)
        .map(|i| handle(&req(h, json!({"call": "GetValue", "class": "System.Array", "handle": a, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true}), "GetValue"), "item"))
        .collect()
}
fn find_all(h: &mut Harness, type_name: &str) -> Vec<u64> {
    let t = ty(h, type_name);
    let a = handle(&req(h, json!({"call": "UnityEngine.Object.FindObjectsOfType", "args": [{"handle": t}], "param_types": ["System.Type"], "main": true, "keep": true}), "FindObjectsOfType"), "arr");
    array_items(h, a)
}
fn icall_v3(h: &mut Harness, name: &str, obj: u64) -> Vector3 {
    let r = req(h, json!({"icall": name, "args": [{"handle": obj}, {"buf": hexf(&[0.0; 3])}], "main": true}), name);
    let s = r["bufs_after"][0].as_str().unwrap();
    let b: Vec<u8> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
    let f = f32s(&b);
    v3(f[0], f[1], f[2])
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self, r: f32) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0 * 2.0 * r - r
    }
}

#[test]
fn raycasts_match_the_engine() {
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    let mask = ret::i32(&req(h, json!({"static_field": "Placemaker.LayerMaster.clickMask", "main": true}), "clickMask"));
    eprintln!("clickMask {mask:#x}");
    let mut colliders: Vec<Collider> = vec![];
    let mut scales = 0;
    let mut scaled: Vec<(u64, u64, i32, Vector3)> = vec![];
    let mut voxel_meshes: Vec<(u64, Vec<Vector3>, Vector3)> = vec![];
    for c in find_all(h, "UnityEngine.MeshCollider, UnityEngine.PhysicsModule") {
        let t = hcall(h, "UnityEngine.Component", c, "get_transform", vec![]);
        let go = hcall(h, "UnityEngine.Component", c, "get_gameObject", vec![]);
        let layer = ret::i32(&req(h, call("UnityEngine.GameObject", go, "get_layer", vec![], false), "layer"));
        if mask & (1 << layer) == 0 {
            continue;
        }
        let p = fl(h, T, t, "get_position");
        let q = fl(h, T, t, "get_rotation");
        let s = fl(h, T, t, "get_lossyScale");
        if s != [1.0, 1.0, 1.0] {
            let n = ret::string(&req(h, call("UnityEngine.Object", go, "get_name", vec![], false), "name"));
            let pt = opt_handle(&req(h, call(T, t, "get_parent", vec![], true), "parent"));
            let pn = pt.map(|pt| ret::string(&req(h, call("UnityEngine.Object", pt, "get_name", vec![], false), "name"))).unwrap_or_default();
            eprintln!("scaled mesh collider {n} (layer {layer}, under {pn}) at {p:?} rotation {q:?} scale {s:?}");
            scaled.push((c, t, layer, v3(s[0], s[1], s[2])));
            scales += 1;
            continue;
        }
        // (the collider's mesh getter is stripped: the corner's `colliderMesh`, which the graph gives its voxels)
        let parent = hcall(h, T, t, "get_parent", vec![]);
        let corner_ty = ty(h, "Placemaker.Graphs.Corner, Assembly-CSharp");
        // (a pooled voxel, under `voxelPool`, keeps its collider with the mesh cleared: nothing to hit)
        let Some(corner) = opt_handle(&req(h, json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": parent, "args": [{"handle": corner_ty}], "param_types": ["System.Type"], "main": true, "keep": true}), "GetComponent")) else {
            continue;
        };
        let Some(mesh) = opt_handle(&req(h, json!({"field": "colliderMesh", "handle": corner, "main": true}), "colliderMesh")) else {
            continue;
        };
        let l = handle(&req(h, json!({"new": list(V3T), "args": [], "main": true}), "list"), "list");
        req(h, json!({"call": "GetVertices", "class": "UnityEngine.Mesh", "handle": mesh, "args": [{"handle": l}], "param_types": ["System.Collections.Generic.List<UnityEngine.Vector3>"], "main": true}), "GetVertices");
        let vs = f32s(&ret::bytes(&req(h, call(&list(V3T), l, "ToArray", vec![], false), "ToArray")));
        let pts: Vec<Vector3> = vs.chunks(3).map(|c| v3(c[0], c[1], c[2])).collect();
        voxel_meshes.push((mesh, pts.clone(), v3(p[0], p[1], p[2])));
        colliders.push(Collider {
            pose: Pose { q: Quaternion { x: q[0], y: q[1], z: q[2], w: q[3] }, p: v3(p[0], p[1], p[2]) },
            shape: Shape::Convex { hull: ConvexHull::from_points(&pts), scale: v3(s[0], s[1], s[2]) },
            layer,
        });
    }
    // HoverData's enlarged helpers (`LastHitVoxel` 1.2, `LastNewVoxel` 1.5) parked on random voxels with
    // their corner's mesh, as hovering does: the scaled-mesh path
    let mut rr = Rng(77);
    for &(c, t, layer, sc) in &scaled {
        let (mesh, pts, pos) = voxel_meshes[(rr.next() as usize) % voxel_meshes.len()].clone();
        let at = pos.add(v3(rr.f(0.3), rr.f(0.3), rr.f(0.3)));
        req(h, call(T, t, "set_position", vec![raw(&[at.x, at.y, at.z])], false), "set_position");
        req(h, json!({"call": "set_sharedMesh", "class": "UnityEngine.MeshCollider", "handle": c, "args": [{"handle": mesh}], "main": true}), "set_sharedMesh");
        let p = fl(h, T, t, "get_position");
        let q = fl(h, T, t, "get_rotation");
        colliders.push(Collider {
            pose: Pose { q: Quaternion { x: q[0], y: q[1], z: q[2], w: q[3] }, p: v3(p[0], p[1], p[2]) },
            shape: Shape::Convex { hull: ConvexHull::from_points(&pts), scale: sc },
            layer,
        });
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    for c in find_all(h, "UnityEngine.BoxCollider, UnityEngine.PhysicsModule") {
        let t = hcall(h, "UnityEngine.Component", c, "get_transform", vec![]);
        let go = hcall(h, "UnityEngine.Component", c, "get_gameObject", vec![]);
        let layer = ret::i32(&req(h, call("UnityEngine.GameObject", go, "get_layer", vec![], false), "layer"));
        let p = fl(h, T, t, "get_position");
        let q = fl(h, T, t, "get_rotation");
        let s = fl(h, T, t, "get_lossyScale");
        let size = icall_v3(h, "UnityEngine.BoxCollider::get_size_Injected(UnityEngine.Vector3&)", c);
        let center = icall_v3(h, "UnityEngine.BoxCollider::get_center_Injected(UnityEngine.Vector3&)", c);
        eprintln!("box: layer {layer} position {p:?} rotation {q:?} scale {s:?} size {size:?} center {center:?}");
        if s != [1.0, 1.0, 1.0] || q != [0.0, 0.0, 0.0, 1.0] {
            scales += 1;
            continue;
        }
        colliders.push(Collider {
            pose: Pose { q: Quaternion { x: q[0], y: q[1], z: q[2], w: q[3] }, p: v3(p[0] + center.x, p[1] + center.y, p[2] + center.z) },
            shape: Shape::Box { half_extents: size.mul(0.5) },
            layer,
        });
    }
    eprintln!("{} colliders ({} scaled or turned)", colliders.len(), scales);
    let cam = handle(&req(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true, "keep": true}), "main"), "cam");
    let w = ret::i32(&req(h, json!({"call": "UnityEngine.Screen.get_width", "args": [], "main": true}), "w")) as f32;
    let hh = ret::i32(&req(h, json!({"call": "UnityEngine.Screen.get_height", "args": [], "main": true}), "h")) as f32;
    let mut r = Rng(5);
    let (mut hits, mut exact, mut misses) = (0, 0, 0);
    let mut worst = 0f32;
    for k in 0..400 {
        let (o, d) = if k % 2 == 0 {
            let ray = f32s(&ret::bytes(&req(h, call("UnityEngine.Camera", cam, "ScreenPointToRay", vec![raw(&[r.f(1.0).abs() * w, r.f(1.0).abs() * hh, 0.0])], false), "ray")));
            (v3(ray[0], ray[1], ray[2]), v3(ray[3], ray[4], ray[5]))
        } else {
            // at a random voxel, from a random side (mostly above)
            let target = colliders[(r.next() as usize) % colliders.len()].pose.p.add(v3(r.f(1.0), r.f(1.0), r.f(1.0)));
            let from = v3(r.f(1.0), r.f(1.0) + 0.6, r.f(1.0)).normalized().mul(5.0 + r.f(15.0).abs());
            let o = target.add(from);
            (o, target.sub(o))
        };
        let hit_raw = vec![0f32; 11];
        let res = req(h, json!({"call": "UnityEngine.Physics.Raycast", "args": [raw(&[o.x, o.y, o.z]), raw(&[d.x, d.y, d.z]), raw(&hit_raw), arg::f32(1e7), arg::i32(mask)], "param_types": ["UnityEngine.Vector3", "UnityEngine.Vector3", "UnityEngine.RaycastHit&", "System.Single", "System.Int32"], "main": true}), "Raycast");
        let got = ret::bytes(&res)[0] != 0;
        let ours = raycast(&colliders, o, d, 1e7, mask);
        let what = format!("ray {k} from {o:?} along {d:?}");
        assert_eq!(got, ours.is_some(), "{what}: hit");
        if !got {
            misses += 1;
            continue;
        }
        hits += 1;
        let s = res["args_after"][2].as_str().unwrap();
        let b: Vec<u8> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
        let g = f32s(&b[..0x28]);
        let ours = ours.unwrap();
        let gp = v3(g[0], g[1], g[2]);
        let gn = v3(g[3], g[4], g[5]);
        let gd = g[7];
        let err = gp.sub(ours.point).magnitude().max(gn.sub(ours.normal).magnitude()).max((gd - ours.distance).abs());
        worst = worst.max(err);
        let same = bits(&[gp.x, gp.y, gp.z, gn.x, gn.y, gn.z, gd]) == bits(&[ours.point.x, ours.point.y, ours.point.z, ours.normal.x, ours.normal.y, ours.normal.z, ours.distance]);
        if !same && std::env::var("TS_PHYS_DIFF").is_ok() {
            let nb = |a: Vector3, b: Vector3| [a.x.to_bits() as i64 - b.x.to_bits() as i64, a.y.to_bits() as i64 - b.y.to_bits() as i64, a.z.to_bits() as i64 - b.z.to_bits() as i64];
            eprintln!("diff: point {:?} normal {:?} distance {} | normal {:?} face {} (game {}) collider {:?}", nb(gp, ours.point), nb(gn, ours.normal), gd.to_bits() as i64 - ours.distance.to_bits() as i64, ours.normal, ours.face_id, g[6].to_bits(), colliders[ours.collider]);
        }
        if same {
            exact += 1;
        } else if err > 1e-4 {
            panic!("{what}: game point {gp:?} normal {gn:?} distance {gd} vs ours {:?} (collider {})", ours, ours.collider);
        }
    }
    eprintln!("{hits} hits ({exact} bit-exact), {misses} misses; largest difference {worst}");
}
