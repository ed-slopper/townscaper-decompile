//! `ts_game::placemaker::big_mesh` against the original game (E7b). Needs `TS_HARNESS=1`.
//! On the scene's `BigMeshMaster`: the big meshes of one material group are read into ours, then random parts
//! are added (`AddMesh`) and returned (`ReturnMesh`) on both sides, each step followed by `OnLateUpdate`
//! (`MaybeApply`), and the group's big meshes and the parts' slots compared.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::big_mesh::{BigMesh, BigMeshGroup, BigMeshLog, BigMeshMaster, BigMeshPart};
use ts_game::placemaker::modules::AssetRef;
use ts_game::unity::vector::{Vector2, Vector3, Vector4};

const BMM: &str = "Placemaker.BigMeshMaster";
const V3: &str = "UnityEngine.Vector3, UnityEngine.CoreModule";
const V4: &str = "UnityEngine.Vector4, UnityEngine.CoreModule";
const V2: &str = "UnityEngine.Vector2, UnityEngine.CoreModule";

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}
fn list(t: &str) -> String {
    format!("System.Collections.Generic.List`1[[{t}]]")
}
fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn i32s(b: &[u8]) -> Vec<i32> {
    b.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits<T: Copy>(v: &[T]) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }.to_vec()
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self) -> f32 {
        (self.next() as f32 / u32::MAX as f32) * 4.0 - 2.0
    }
}

fn field(h: &mut Harness, obj: u64, f: &str) -> Value {
    ok(h.request(json!({"field": f, "handle": obj, "main": true})), f)
}
fn to_array(h: &mut Harness, obj: u64, f: &str, elem: &str) -> Vec<u8> {
    let l = handle(&field(h, obj, f), f);
    ret::bytes(&ok(h.request(json!({"call": "ToArray", "class": list(elem), "handle": l, "args": [], "main": true})), f))
}

/// One game big mesh as ours.
fn read_big_mesh(h: &mut Harness, bm: u64) -> BigMesh {
    let go = handle(&ok(h.request(json!({"call": "get_gameObject", "class": "UnityEngine.Component", "handle": bm, "args": [], "main": true, "keep": true})), "go"), "go");
    let active = ret::bytes(&ok(h.request(json!({"call": "get_activeSelf", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true})), "activeSelf"))[0] != 0;
    let v3 = |b: Vec<u8>| f32s(&b).chunks(3).map(|c| Vector3 { x: c[0], y: c[1], z: c[2] }).collect::<Vec<_>>();
    BigMesh {
        created_count: ret::i32(&field(h, bm, "createdCount")),
        dirty: ret::bytes(&field(h, bm, "dirty"))[0] != 0,
        available_indexes: to_array(h, bm, "availableIndexes", "System.UInt16").chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
        available_tris: i32s(&to_array(h, bm, "availableTris", "System.Int32")),
        verts: v3(to_array(h, bm, "verts", V3)),
        normals: v3(to_array(h, bm, "normals", V3)),
        tangents: f32s(&to_array(h, bm, "tangents", V4)).chunks(4).map(|c| Vector4 { x: c[0], y: c[1], z: c[2], w: c[3] }).collect(),
        uvs: f32s(&to_array(h, bm, "uvs", V2)).chunks(2).map(|c| Vector2 { x: c[0], y: c[1] }).collect(),
        tris: i32s(&to_array(h, bm, "tris", "System.Int32")),
        active,
        applied: 0,
    }
}

fn big_meshes(h: &mut Harness, group: u64) -> Vec<u64> {
    let l = handle(&field(h, group, "bigMeshes"), "bigMeshes");
    let n = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": list("Placemaker.BigMesh, Assembly-CSharp"), "handle": l, "args": [], "main": true})), "Count"));
    (0..n).map(|i| handle(&ok(h.request(json!({"call": "get_Item", "class": list("Placemaker.BigMesh, Assembly-CSharp"), "handle": l, "args": [arg::i32(i)], "main": true, "keep": true})), "item"), "mesh")).collect()
}

fn same_mesh(g: &BigMesh, o: &BigMesh, what: &str) {
    assert_eq!(g.created_count, o.created_count, "{what}: createdCount");
    assert_eq!(g.dirty, o.dirty, "{what}: dirty");
    assert_eq!(g.active, o.active, "{what}: active");
    assert_eq!(g.available_indexes, o.available_indexes, "{what}: availableIndexes");
    assert_eq!(g.available_tris, o.available_tris, "{what}: availableTris");
    assert_eq!(bits(&g.verts), bits(&o.verts), "{what}: verts");
    assert_eq!(bits(&g.normals), bits(&o.normals), "{what}: normals");
    assert_eq!(bits(&g.tangents), bits(&o.tangents), "{what}: tangents");
    assert_eq!(bits(&g.uvs), bits(&o.uvs), "{what}: uvs");
    assert_eq!(g.tris, o.tris, "{what}: tris ({} vs {})", g.tris.len(), o.tris.len());
}

#[test]
fn big_meshes_match_the_game() {
    let Some(mut h) = harness() else { return };
    let master = handle(&ok(h.request(json!({"field": "bigMeshMaster", "on": "Placemaker.WorldMaster", "main": true})), "bigMeshMaster"), "bigMeshMaster");
    let groups = handle(&field(&mut h, master, "bigMeshGroups"), "bigMeshGroups");
    let gl = list("Placemaker.BigMeshGroup, Assembly-CSharp");
    let ng = ret::i32(&ok(h.request(json!({"call": "get_Count", "class": gl, "handle": groups, "args": [], "main": true})), "Count"));
    assert!(ng > 0, "no big mesh groups");
    // the last group, and its material (an identity stands for the material: its address)
    let group = handle(&ok(h.request(json!({"call": "get_Item", "class": gl, "handle": groups, "args": [arg::i32(ng - 1)], "main": true, "keep": true})), "group"), "group");
    let mat = handle(&field(&mut h, group, "material"), "material");
    let mat_addr = ok(h.request(json!({"addr": mat})), "addr")["addr"].as_u64().unwrap();
    let material = AssetRef { file_id: 0, path_id: mat_addr as i64 };
    let mut ours = BigMeshMaster::default();
    let meshes: Vec<BigMesh> = big_meshes(&mut h, group).into_iter().map(|m| read_big_mesh(&mut h, m)).collect();
    eprintln!("group {} of {ng}: {} big meshes, {} vertices", ng - 1, meshes.len(), meshes.iter().map(|m| m.created_count).sum::<i32>());
    ours.big_mesh_groups.push(BigMeshGroup { material, big_meshes: meshes, is_obj_exportable: false });
    let g = 0;
    let mut log: Vec<BigMeshLog> = vec![];
    let mut parts: Vec<(u64, BigMeshPart)> = vec![];
    let mut r = Rng(91);
    for step in 0..300 {
        let mut reqs = vec![];
        if parts.is_empty() || r.next() % 3 != 0 {
            // a new part: 0..400 vertices (now and then more than a big mesh has room for), normals / tangents /
            // uvs each present or not
            let nv = if r.next() % 40 == 0 { 20_000 + r.next() % 30_000 } else { r.next() % 400 } as usize;
            let verts: Vec<Vector3> = (0..nv).map(|_| Vector3 { x: r.f(), y: r.f(), z: r.f() }).collect();
            let normals: Vec<Vector3> = if r.next() % 4 != 0 { (0..nv).map(|_| Vector3 { x: r.f(), y: r.f(), z: r.f() }).collect() } else { vec![] };
            let tangents: Vec<Vector4> = if r.next() % 4 != 0 { (0..nv).map(|_| Vector4 { x: r.f(), y: r.f(), z: r.f(), w: r.f() }).collect() } else { vec![] };
            let uvs: Vec<Vector2> = if r.next() % 4 != 0 { (0..nv).map(|_| Vector2 { x: r.f(), y: r.f() }).collect() } else { vec![] };
            let nt = if nv == 0 { 0 } else { (r.next() % (nv as u32 * 2 + 1)) as usize };
            let tris: Vec<i32> = (0..nt * 3).map(|_| (r.next() as usize % nv.max(1)) as i32).collect();
            let part = handle(&ok(h.request(json!({"new": "Placemaker.BigMeshPart", "args": [], "main": true})), "new part"), "part");
            let mut lists = vec![];
            for (elem, raw) in [(V3, bits(&verts)), (V3, bits(&normals)), (V4, bits(&tangents)), (V2, bits(&uvs)), ("System.Int32", bits(&tris))] {
                let l = handle(&ok(h.request(json!({"new": list(elem), "args": [], "main": true})), "new list"), "list");
                let short = elem.split(',').next().unwrap();
                if !raw.is_empty() {
                    ok(h.request(json!({"call": "AddRange", "class": list(elem), "handle": l, "args": [{"array": short, "raw": raw.iter().map(|b| format!("{b:02x}")).collect::<String>()}], "main": true})), "AddRange");
                }
                lists.push(json!({"handle": l}));
            }
            let mut args = vec![json!({"handle": part}), json!({"handle": mat})];
            args.extend(lists);
            reqs.push(json!({"call": format!("{BMM}.AddMesh"), "handle": master, "args": args, "main": true}));
            let mut p = BigMeshPart::default();
            ours.add_mesh(&mut p, material, &verts, &normals, &tangents, &uvs, &tris, &mut log);
            parts.push((part, p));
        } else {
            let k = r.next() as usize % parts.len();
            let (part, mut p) = parts.swap_remove(k);
            reqs.push(json!({"call": format!("{BMM}.ReturnMesh"), "handle": master, "args": [{"handle": part}], "main": true}));
            ours.return_mesh(&mut p);
        }
        reqs.push(json!({"call": format!("{BMM}.OnLateUpdate"), "handle": master, "args": [], "main": true}));
        ours.on_late_update(&mut log);
        for v in h.batch(reqs) {
            ok(v, &format!("step {step}"));
        }
        // the parts' slots, and every few steps the group's big meshes
        if let Some((part, p)) = parts.last() {
            let vi: Vec<u16> = to_array(&mut h, *part, "vertIndexes", "System.UInt16").chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            assert_eq!(vi, p.vert_indexes, "step {step}: vertIndexes");
            assert_eq!(i32s(&to_array(&mut h, *part, "triIndexes", "System.Int32")), p.tri_indexes, "step {step}: triIndexes");
        }
        if step % 25 == 24 {
            let gm = big_meshes(&mut h, group);
            assert_eq!(gm.len(), ours.big_mesh_groups[g].big_meshes.len(), "step {step}: big mesh count");
            for (k, m) in gm.into_iter().enumerate() {
                let game = read_big_mesh(&mut h, m);
                same_mesh(&game, &ours.big_mesh_groups[g].big_meshes[k], &format!("step {step}: big mesh {k}"));
            }
            eprintln!("step {step}: same ({} parts, {} big meshes, {} log)", parts.len(), ours.big_mesh_groups[g].big_meshes.len(), log.len());
        }
    }
    // give everything back
    let mut reqs = vec![];
    for (part, mut p) in parts.drain(..) {
        reqs.push(json!({"call": format!("{BMM}.ReturnMesh"), "handle": master, "args": [{"handle": part}], "main": true}));
        ours.return_mesh(&mut p);
    }
    reqs.push(json!({"call": format!("{BMM}.OnLateUpdate"), "handle": master, "args": [], "main": true}));
    ours.on_late_update(&mut log);
    for v in h.batch(reqs) {
        ok(v, "return all");
    }
    let gm = big_meshes(&mut h, group);
    for (k, m) in gm.into_iter().enumerate() {
        let game = read_big_mesh(&mut h, m);
        same_mesh(&game, &ours.big_mesh_groups[g].big_meshes[k], &format!("all returned: big mesh {k}"));
    }
}
