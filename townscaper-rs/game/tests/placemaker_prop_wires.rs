//! The wire meshes of `ts_game::placemaker::prop_placer` (E10d) against the original game. Needs `TS_HARNESS=1`
//! and the install. Wire props (the two wire prefabs the anchors use) made by a fresh `PropPlacer` hang between
//! two anchor props whose voxel types are set; the game's `PropWireFlagMesh.Setup` / `PropWireItalianMesh.Setup`
//! (a stand-in master: a fresh `WorldMeshes`, the fresh placer, the scene's `MiscReferences`) and ours run for
//! many lengths, positions and voxel types: the flag mesh's vertices, normals, uvs and triangles bit for bit; the
//! Italian wire's scale, rotation and coloured mesh (its place in `materialMeshes`).

mod graph_common;
mod prop_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::prop_placer::{Modifier, PropHost, PropPlacer, PropScene};
use ts_game::unity::transform::Node;
use ts_game::unity::vector::{v3, Vector3};

const T: &str = "UnityEngine.Transform";
const V3T: &str = "UnityEngine.Vector3, UnityEngine.CoreModule";
const V2T: &str = "UnityEngine.Vector2, UnityEngine.CoreModule";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn req(h: &mut Harness, r: Value, what: &str) -> Value {
    ok(h.request(r), what)
}
fn call(t: u64, class: &str, m: &str, args: Vec<Value>, keep: bool) -> Value {
    json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": keep})
}
fn hcall(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> u64 {
    handle(&req(h, call(t, class, m, args, true), m), m)
}
fn field(t: u64, f: &str) -> Value {
    json!({"field": f, "handle": t, "main": true})
}
fn set_h(h: &mut Harness, t: u64, f: &str, v: u64) {
    req(h, json!({"field": f, "handle": t, "set_handle": v, "main": true}), f);
}
fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&req(h, json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true}), n), n)
}
fn new_go(h: &mut Harness, n: &str) -> (u64, u64) {
    let go = handle(&req(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true}), n), n);
    req(h, call(go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false), "SetActive");
    let t = hcall(h, go, "UnityEngine.GameObject", "get_transform", vec![]);
    (go, t)
}
fn add_component(h: &mut Harness, go: u64, t: u64) -> u64 {
    handle(&req(h, json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": t}], "param_types": ["System.Type"], "main": true, "keep": true}), "AddComponent"), "AddComponent")
}
fn get_component(h: &mut Harness, t: u64, ty: u64) -> u64 {
    handle(&req(h, json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true}), "GetComponent"), "GetComponent")
}
fn addr(h: &mut Harness, t: u64) -> u64 {
    req(h, json!({"addr": t}), "addr")["addr"].as_u64().unwrap()
}
fn list_bytes(h: &mut Harness, l: u64, t: &str) -> Vec<u8> {
    ret::bytes(&req(h, call(l, &list(t), "ToArray", vec![], false), "ToArray"))
}

/// Our world meshes, numbered.
#[derive(Default)]
struct Meshes(u32);
impl PropHost for Meshes {
    fn get_world_mesh(&mut self) -> u32 {
        self.0 += 1;
        self.0 - 1
    }
}

#[test]
fn wire_meshes_match_the_game() {
    let props = prop_common::load();
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    let prefab_handles = prop_common::game_prefabs(h, &props);
    let scene_master = handle(&req(h, json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true}), "master"), "master");
    // the stand-in master, the fresh placer
    let master = handle(&req(h, json!({"new": "Placemaker.WorldMaster", "args": [], "main": true}), "master"), "master");
    let (wgo, _) = new_go(h, "ts_check world meshes");
    let wty = ty(h, "Placemaker.WorldMeshes, Assembly-CSharp");
    let wm = add_component(h, wgo, wty);
    set_h(h, master, "worldMeshes", wm);
    let misc = handle(&req(h, field(scene_master, "miscReferences"), "misc"), "misc");
    set_h(h, master, "miscReferences", misc);
    let (pgo, _) = new_go(h, "ts_check prop placer");
    let pty = ty(h, "Placemaker.PropPlacer, Assembly-CSharp");
    let placer = add_component(h, pgo, pty);
    set_h(h, placer, "master", scene_master);
    set_h(h, master, "propPlacer", placer);
    let mut scene = PropScene::default();
    let mut cont = vec![];
    for f in ["propPoolContainer", "secondaryPropsContainer", "deconstructionContainer"] {
        let (_, t) = new_go(h, &format!("ts_check {f}"));
        set_h(h, placer, f, t);
        cont.push(scene.add_host(Node::default(), false));
    }
    if opt_handle(&req(h, field(placer, "materialMeshes"), "materialMeshes")).is_none() {
        let l = handle(&req(h, json!({"new": list("UnityEngine.Mesh, UnityEngine.CoreModule"), "args": [], "main": true}), "list"), "list");
        set_h(h, placer, "materialMeshes", l);
    }
    req(h, call(placer, "Placemaker.PropPlacer", "OnEnable", vec![], false), "OnEnable");
    // the Italian meshes, by name
    let mut names: HashMap<String, u64> = HashMap::new();
    for (&id, n) in &props.mesh_names {
        names.entry(n.clone()).or_insert(id);
    }
    let italian: Vec<u64> = {
        let arr = handle(&req(h, field(misc, "italianMeshes"), "italianMeshes"), "arr");
        let n = ret::i32(&req(h, call(arr, "System.Array", "get_Length", vec![], false), "n"));
        (0..n)
            .map(|i| {
                let m = handle(&req(h, json!({"call": "GetValue", "class": "System.Array", "handle": arr, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true}), "m"), "m");
                let nm = ret::string(&req(h, call(m, "UnityEngine.Object", "get_name", vec![], false), "name"));
                names[&nm]
            })
            .collect()
    };
    let mut ours = PropPlacer {
        src_props: vec![],
        prop_pools: HashMap::new(),
        prop_pool_container: cont[0],
        secondary_props_container: cont[1],
        deconstruction_container: cont[2],
        door_material: None,
        hole_material: None,
        collider_dict: HashMap::new(),
        material_meshes: vec![],
        prefab_of: HashMap::new(),
        italian_meshes: italian,
    };
    ours.on_enable(&mut scene, &props.prefabs, &[]);
    let (_, parent_t) = new_go(h, "ts_check wires");
    let parent = scene.add_host(Node::default(), false);
    let get_prop = |h: &mut Harness, scene: &mut PropScene, ours: &mut PropPlacer, pf: usize| -> (usize, u64) {
        let gp = hcall(h, placer, "Placemaker.PropPlacer", "GetProp", vec![json!({"handle": prefab_handles[pf]}), json!({"handle": parent_t})]);
        let gt = hcall(h, gp, "UnityEngine.Component", "get_transform", vec![]);
        (ours.get_prop(scene, &props.prefabs, pf, Some(parent)), gt)
    };
    // the wire prefabs and an anchor prefab of each
    let wire_of = |m: fn(&Modifier) -> bool| (0..props.prefabs.len()).find(|&i| props.prefabs[i].nodes[0].modifiers.iter().any(m)).expect("a wire prefab");
    let flag_pf = wire_of(|m| matches!(m, Modifier::WireFlagMesh { .. }));
    let italian_pf = wire_of(|m| matches!(m, Modifier::WireItalianMesh));
    let anchor_pf = (0..props.prefabs.len()).find(|&i| props.prefabs[i].nodes[0].wire_anchor.is_some()).expect("an anchor prefab");
    let anchor_ty = ty(h, "Placemaker.Props.PropWireAnchor, Assembly-CSharp");
    let wire_ty = ty(h, "Placemaker.Props.PropWire, Assembly-CSharp");
    let flag_ty = ty(h, "Placemaker.Props.PropWireFlagMesh, Assembly-CSharp");
    let ital_ty = ty(h, "Placemaker.Props.PropWireItalianMesh, Assembly-CSharp");
    let mf_ty = ty(h, "UnityEngine.MeshFilter, UnityEngine.CoreModule");
    let mut anchors = vec![];
    for _ in 0..2 {
        let (o, g) = get_prop(h, &mut scene, &mut ours, anchor_pf);
        anchors.push((o, g, get_component(h, g, anchor_ty)));
    }
    let mut host = Meshes::default();
    let mut s: u32 = 17;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let lengths = [0.05f32, 0.3, 0.73, 1.0, 1.4999, 2.2, 3.7, 5.03];
    for k in 0..24 {
        let flag = k % 2 == 0;
        let pf = if flag { flag_pf } else { italian_pf };
        let (wo, wg) = get_prop(h, &mut scene, &mut ours, pf);
        let w = get_component(h, wg, wire_ty);
        set_h(h, w, "anchor0", anchors[0].2);
        set_h(h, w, "anchor1", anchors[1].2);
        scene.object_mut(wo).wire = (Some(anchors[0].0), Some(anchors[1].0));
        let (v0, v1) = ((rnd() % 15) as u8, (rnd() % 15) as u8);
        for (a, v) in [(anchors[0], v0), (anchors[1], v1)] {
            req(h, json!({"field": "voxelType", "handle": a.2, "set_raw": format!("{v:02x}"), "main": true}), "voxelType");
            scene.object_mut(a.0).anchor.as_mut().unwrap().voxel_type = v;
        }
        let p = v3((rnd() % 4000) as f32 * 0.013 - 26.0, (rnd() % 400) as f32 * 0.01, (rnd() % 4000) as f32 * 0.0117 - 23.0);
        req(h, json!({"call": "set_position", "class": T, "handle": wg, "args": [arg::raw(&bytes(&p))], "main": true}), "position");
        scene.transforms.set_position(wo, p);
        let len = lengths[k % lengths.len()] + (rnd() % 100) as f32 * 0.001;
        let what = format!("wire {k} ({}, length {len}, voxel types {v0} {v1})", if flag { "flag" } else { "Italian" });
        if flag {
            let c = get_component(h, wg, flag_ty);
            req(h, json!({"call": "Setup", "class": "Placemaker.Props.PropWireFlagMesh", "handle": c, "args": [{"handle": master}, {"handle": w}, arg::f32(len)], "main": true}), "Setup");
            let uv = props.prefabs[pf].nodes[0].modifiers.iter().find_map(|m| match m {
                Modifier::WireFlagMesh { uv } => Some(*uv),
                _ => None,
            });
            ours.flag_mesh_setup(&mut scene, &mut host, wo, uv.unwrap(), len);
            let Some(ts_game::placemaker::prop_placer::MeshId::World(id)) = scene.object(wo).mesh else { panic!("{what}: a world mesh") };
            let gm = &scene.generated[&id];
            let mf = get_component(h, wg, mf_ty);
            let mesh = hcall(h, mf, "UnityEngine.MeshFilter", "get_sharedMesh", vec![]);
            let new_list = |h: &mut Harness, t: &str| handle(&req(h, json!({"new": list(t), "args": [], "main": true}), "list"), "list");
            let (lv, ln, lu, lt) = (new_list(h, V3T), new_list(h, V3T), new_list(h, V2T), new_list(h, "System.Int32"));
            let mc = "UnityEngine.Mesh";
            req(h, json!({"call": "GetVertices", "class": mc, "handle": mesh, "args": [{"handle": lv}], "param_types": ["System.Collections.Generic.List<UnityEngine.Vector3>"], "main": true}), "GetVertices");
            req(h, json!({"call": "GetNormals", "class": mc, "handle": mesh, "args": [{"handle": ln}], "param_types": ["System.Collections.Generic.List<UnityEngine.Vector3>"], "main": true}), "GetNormals");
            req(h, json!({"call": "GetUVs", "class": mc, "handle": mesh, "args": [arg::i32(0), {"handle": lu}], "param_types": ["System.Int32", "System.Collections.Generic.List<UnityEngine.Vector2>"], "main": true}), "GetUVs");
            req(h, json!({"call": "GetTriangles", "class": mc, "handle": mesh, "args": [{"handle": lt}, arg::i32(0)], "param_types": ["System.Collections.Generic.List<System.Int32>", "System.Int32"], "main": true}), "GetTriangles");
            let fl = |v: &[Vector3]| v.iter().flat_map(|p| [p.x, p.y, p.z]).collect::<Vec<f32>>();
            assert_eq!(bits(&f32s(&list_bytes(h, lv, V3T))), bits(&fl(&gm.verts)), "{what}: vertices");
            assert_eq!(bits(&f32s(&list_bytes(h, ln, V3T))), bits(&fl(&gm.normals)), "{what}: normals");
            let uv: Vec<f32> = gm.uvs.iter().flat_map(|p| [p.x, p.y]).collect();
            assert_eq!(bits(&f32s(&list_bytes(h, lu, V2T))), bits(&uv), "{what}: uvs");
            let tris: Vec<i32> = list_bytes(h, lt, "System.Int32").chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
            assert_eq!(tris, gm.tris, "{what}: triangles");
        } else {
            let c = get_component(h, wg, ital_ty);
            req(h, json!({"call": "Setup", "class": "Placemaker.Props.PropWireItalianMesh", "handle": c, "args": [{"handle": master}, {"handle": w}, arg::f32(len)], "main": true}), "Setup");
            ours.italian_mesh_setup(&mut scene, wo, len);
            let gb = |h: &mut Harness, m: &str| bits(&f32s(&ret::bytes(&req(h, call(wg, T, m, vec![], false), m))));
            let (ls, lq) = (scene.transforms.local_scale(wo), scene.transforms.local_rotation(wo));
            assert_eq!(gb(h, "get_localScale"), bits(&[ls.x, ls.y, ls.z]), "{what}: localScale");
            assert_eq!(gb(h, "get_localRotation"), bits(&[lq.x, lq.y, lq.z, lq.w]), "{what}: localRotation");
            // the mesh by its place in materialMeshes
            let mf = get_component(h, wg, mf_ty);
            let mesh = hcall(h, mf, "UnityEngine.MeshFilter", "get_sharedMesh", vec![]);
            let ma = addr(h, mesh);
            let l = handle(&req(h, field(placer, "materialMeshes"), "materialMeshes"), "l");
            let lc = list("UnityEngine.Mesh, UnityEngine.CoreModule");
            let n = ret::i32(&req(h, call(l, &lc, "get_Count", vec![], false), "n"));
            assert_eq!(n as usize, ours.material_meshes.len(), "{what}: materialMeshes");
            let mut at = None;
            for i in 0..n {
                let m = hcall(h, l, &lc, "get_Item", vec![arg::i32(i)]);
                if addr(h, m) == ma {
                    at = Some(i as usize);
                }
            }
            let at = at.expect("the mesh in materialMeshes");
            assert_eq!(scene.object(wo).mesh, Some(ours.material_meshes[at]), "{what}: mesh");
        }
        eprintln!("{what}: same");
    }
}
