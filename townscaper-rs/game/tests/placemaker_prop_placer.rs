//! `ts_game::placemaker::prop_placer` (E10b1) against the original game. Needs `TS_HARNESS=1` and the install.
//! A fresh `PropPlacer` on an inactive object (the scene master's materials, its own pool and prop containers)
//! makes prop instances with the game's `GetProp` under an inactive parent: random prefabs, instantiated or
//! (after an instance is moved into its pool) taken from the pool. Ours does the same; every instance must
//! match object by object: names, active flags, layers, local poses (bit for bit), the prop node states and
//! their relations, the mesh references and their batching, the renderers, `srcProp`, and the pools.

mod graph_common;
mod prop_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::prop_placer::{PropPlacer, PropScene};
use ts_game::unity::transform::{Node, TransformId};

const T: &str = "UnityEngine.Transform";
const PP: &str = "Placemaker.PropPlacer";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
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
fn field_h(h: &mut Harness, t: u64, f: &str) -> Option<u64> {
    opt_handle(&ok(h.request(json!({"field": f, "handle": t, "main": true})), f))
}
fn set_h(h: &mut Harness, t: u64, f: &str, v: u64) {
    ok(h.request(json!({"field": f, "handle": t, "set_handle": v, "main": true})), f);
}
fn addr(h: &mut Harness, t: u64) -> u64 {
    ok(h.request(json!({"addr": t})), "addr")["addr"].as_u64().unwrap()
}
fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n)
}
fn name(h: &mut Harness, o: u64) -> String {
    ret::string(&call(h, o, "UnityEngine.Object", "get_name", vec![]))
}
fn get_component(h: &mut Harness, t: u64, ty: u64) -> Option<u64> {
    opt_handle(&ok(h.request(json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "GetComponent"))
}
fn new_go(h: &mut Harness, n: &str) -> (u64, u64) {
    let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n);
    call(h, go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)]);
    let t = call_keep(h, go, "UnityEngine.GameObject", "get_transform", vec![]);
    (go, t)
}

struct Types {
    prop_node: u64,
    mesh_ref: u64,
    renderer: u64,
    prop: u64,
}

/// An instance in the game and in ours, object by object (child order), with the game's PropNode addresses.
fn pair_up(h: &mut Harness, scene: &PropScene, ours: TransformId, game: u64, out: &mut Vec<(TransformId, u64)>) {
    out.push((ours, game));
    let n = ret::i32(&call(h, game, T, "get_childCount", vec![]));
    assert_eq!(n as usize, scene.transforms.child_count(ours), "{}: child count", scene.object(ours).name);
    for i in 0..n {
        let c = call_keep(h, game, T, "GetChild", vec![arg::i32(i)]);
        pair_up(h, scene, scene.transforms.child(ours, i as usize), c, out);
    }
}

fn check_instance(h: &mut Harness, scene: &PropScene, ours: TransformId, game: u64, types: &Types, prefab_addr: u64, what: &str) {
    let mut pairs = vec![];
    pair_up(h, scene, ours, game, &mut pairs);
    // the game's PropNode component addresses by our object
    let mut node_addr: HashMap<TransformId, u64> = HashMap::new();
    let mut obj_addr: HashMap<TransformId, u64> = HashMap::new();
    for &(o, g) in &pairs {
        obj_addr.insert(o, addr(h, g));
        if let Some(n) = get_component(h, g, types.prop_node) {
            let a = addr(h, n);
            node_addr.insert(o, a);
        }
    }
    for &(o, g) in &pairs {
        let ob = scene.object(o);
        let what = format!("{what}: {}", ob.name);
        let go = call_keep(h, g, "UnityEngine.Component", "get_gameObject", vec![]);
        assert_eq!(name(h, go), ob.name, "{what}: name");
        assert_eq!(ret::bytes(&call(h, go, "UnityEngine.GameObject", "get_activeSelf", vec![]))[0] != 0, scene.active[o], "{what}: active");
        assert_eq!(ret::i32(&call(h, go, "UnityEngine.GameObject", "get_layer", vec![])) as u32, ob.layer, "{what}: layer");
        let gb = |h: &mut Harness, m: &str| bits(&f32s(&ret::bytes(&call(h, g, T, m, vec![]))));
        let lp = scene.transforms.local_position(o);
        let lq = scene.transforms.local_rotation(o);
        let ls = scene.transforms.local_scale(o);
        assert_eq!(gb(h, "get_localPosition"), bits(&[lp.x, lp.y, lp.z]), "{what}: localPosition");
        assert_eq!(gb(h, "get_localRotation"), bits(&[lq.x, lq.y, lq.z, lq.w]), "{what}: localRotation");
        assert_eq!(gb(h, "get_localScale"), bits(&[ls.x, ls.y, ls.z]), "{what}: localScale");
        if let Some(r) = ob.renderer_enabled {
            let rc = get_component(h, g, types.renderer).expect("renderer");
            assert_eq!(ret::bytes(&call(h, rc, "UnityEngine.Renderer", "get_enabled", vec![]))[0] != 0, r, "{what}: renderer enabled");
        }
        if let Some(src) = ob.src_prop {
            let _ = src;
            let p = get_component(h, g, types.prop).expect("Prop");
            let sp = field_h(h, p, "srcProp").expect("srcProp");
            assert_eq!(addr(h, sp), prefab_addr, "{what}: srcProp");
        }
        if let Some(n) = &ob.node {
            let c = get_component(h, g, types.prop_node).unwrap();
            let b = |h: &mut Harness, f: &str| field(h, c, f)[0];
            assert_eq!(f32s(&field(h, c, "priority"))[0].to_bits(), n.priority.to_bits(), "{what}: priority");
            assert_eq!(
                [b(h, "turnedOn"), b(h, "inDict"), b(h, "hasBeenTurnedOn"), b(h, "hideCount"), b(h, "motivations")],
                [n.turned_on as u8, n.in_dict as u8, n.has_been_turned_on as u8, n.hide_count, n.motivations],
                "{what}: node flags"
            );
            assert_eq!(
                [b(h, "parentCount"), b(h, "childCount"), b(h, "overlapCount"), b(h, "moteveeCount"), b(h, "anchorCount")],
                [n.parent_count, n.child_count, n.overlap_count, n.motevee_count, n.anchor_count],
                "{what}: node counts"
            );
            let l = field_h(h, c, "relations").unwrap();
            let lc = list("Placemaker.Props.PropNode, Assembly-CSharp");
            let cnt = ret::i32(&call(h, l, &lc, "get_Count", vec![]));
            assert_eq!(cnt as usize, n.relations.len(), "{what}: relations");
            for (k, r) in n.relations.iter().enumerate() {
                let it = call_keep(h, l, &lc, "get_Item", vec![arg::i32(k as i32)]);
                assert_eq!(Some(addr(h, it)), node_addr.get(r).copied(), "{what}: relation {k}");
            }
        }
        let gm = get_component(h, g, types.mesh_ref);
        assert_eq!(gm.is_some(), ob.mesh_ref.is_some(), "{what}: PropMeshReference");
        if let (Some(c), Some(m)) = (gm, &ob.mesh_ref) {
            assert_eq!(ret::bytes(&call(h, c, "UnityEngine.Behaviour", "get_enabled", vec![]))[0] != 0, m.enabled, "{what}: mesh reference enabled");
            let l = field_h(h, c, "items").unwrap();
            let lc = list("Placemaker.Props.PropMeshReference+Item, Assembly-CSharp");
            let cnt = ret::i32(&call(h, l, &lc, "get_Count", vec![]));
            assert_eq!(cnt as usize, m.items.len(), "{what}: mesh items");
            for (k, it) in m.items.iter().enumerate() {
                let gi = call_keep(h, l, &lc, "get_Item", vec![arg::i32(k as i32)]);
                let mf = field_h(h, gi, "mf").unwrap();
                let mft = call_keep(h, mf, "UnityEngine.Component", "get_transform", vec![]);
                assert_eq!(addr(h, mft), obj_addr[&it.mf], "{what}: item {k} mesh filter");
                assert_eq!(field_h(h, gi, "bigMeshPart").is_some(), it.big_mesh_part, "{what}: item {k} bigMeshPart");
                assert_eq!(field(h, gi, "shouldBeBatched")[0] != 0, it.should_be_batched, "{what}: item {k} batched");
            }
        }
    }
}

#[test]
fn get_prop_matches_the_game() {
    let props = prop_common::load();
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    let prefab_handles = prop_common::game_prefabs(h, &props);
    let prefab_addrs: Vec<u64> = prefab_handles.iter().map(|&p| addr(h, p)).collect();
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    // the door material, by name
    let mm = field_h(h, master, "materialMaster").unwrap();
    let door = field_h(h, mm, "doorMaterial").unwrap();
    let door_name = name(h, door);
    let door_ids: Vec<u64> = props.material_names.iter().filter(|(_, n)| **n == door_name).map(|(&i, _)| i).collect();
    assert_eq!(door_ids.len(), 1, "one material named {door_name}");

    // a fresh placer and containers in the game, and ours
    let (go, _) = new_go(h, "ts_check prop placer");
    let pt = ty(h, "Placemaker.PropPlacer, Assembly-CSharp");
    let placer = handle(&ok(h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": pt}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "placer");
    set_h(h, placer, "master", master);
    let mut scene = PropScene::default();
    let mut gos = vec![go];
    let mut cont = vec![];
    for f in ["propPoolContainer", "secondaryPropsContainer", "deconstructionContainer"] {
        let (g, t) = new_go(h, &format!("ts_check {f}"));
        set_h(h, placer, f, t);
        gos.push(g);
        cont.push((t, scene.add_transform(Node::default())));
    }
    let (pgo, parent_t) = new_go(h, "ts_check props parent");
    gos.push(pgo);
    let parent = scene.add_transform(Node::default());
    ok(h.request(json!({"call": "OnEnable", "class": PP, "handle": placer, "args": [], "main": true})), "OnEnable");
    let mut ours = PropPlacer {
        src_props: vec![],
        prop_pools: HashMap::new(),
        prop_pool_container: cont[0].1,
        secondary_props_container: cont[1].1,
        deconstruction_container: cont[2].1,
        door_material: Some(door_ids[0]),
        hole_material: None,
        collider_dict: HashMap::new(),
        material_meshes: vec![],
        prefab_of: HashMap::new(),
        italian_meshes: vec![],
    };
    ours.on_enable(&mut scene, &props.prefabs, &[]);
    let types = Types {
        prop_node: ty(h, "Placemaker.Props.PropNode, Assembly-CSharp"),
        mesh_ref: ty(h, "Placemaker.Props.PropMeshReference, Assembly-CSharp"),
        renderer: ty(h, "UnityEngine.Renderer, UnityEngine.CoreModule"),
        prop: ty(h, "Placemaker.Prop, Assembly-CSharp"),
    };
    // every prefab once, then random ones; now and then an instance goes into its pool and comes back out
    let mut s: u32 = 29;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let mut made: Vec<(usize, TransformId, u64)> = vec![];
    let order: Vec<usize> = (0..props.prefabs.len()).chain((0..60).map(|_| rnd() as usize % props.prefabs.len())).collect();
    for (k, &pf) in order.iter().enumerate() {
        if k > 0 && k % 7 == 0 {
            // move an earlier instance into its pool (as ReturnProp leaves it: SetParent(pool, true))
            let (mpf, mo, mg) = made[rnd() as usize % made.len()];
            if scene.transforms.nodes[mo].parent == Some(parent) {
                let pools = field_h(h, placer, "propPools").unwrap();
                let dict = "System.Collections.Generic.Dictionary`2[[Placemaker.Prop, Assembly-CSharp],[UnityEngine.Transform, UnityEngine.CoreModule]]";
                let gp = call_keep(h, pools, dict, "get_Item", vec![json!({"handle": prefab_handles[mpf]})]);
                ok(h.request(json!({"call": "SetParent", "class": T, "handle": mg, "args": [{"handle": gp}, arg::bool(true)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true})), "to pool");
                let op = ours.prop_pools[&mpf];
                scene.transforms.set_parent(mo, Some(op), true);
            }
        }
        let gp = call_keep(h, placer, PP, "GetProp", vec![json!({"handle": prefab_handles[pf]}), json!({"handle": parent_t})]);
        let gt = call_keep(h, gp, "UnityEngine.Component", "get_transform", vec![]);
        let ot = ours.get_prop(&mut scene, &props.prefabs, pf, Some(parent));
        let what = format!("GetProp {k} (prefab {pf} {})", props.prefabs[pf].nodes[0].name);
        check_instance(h, &scene, ot, gt, &types, prefab_addrs[pf], &what);
        made.push((pf, ot, gt));
    }
    // the pools: srcProps in order
    let sp = field_h(h, placer, "srcProps").unwrap();
    let lc = list("Placemaker.Prop, Assembly-CSharp");
    let n = ret::i32(&call(h, sp, &lc, "get_Count", vec![]));
    assert_eq!(n as usize, ours.src_props.len(), "srcProps");
    for i in 0..n {
        let it = call_keep(h, sp, &lc, "get_Item", vec![arg::i32(i)]);
        assert_eq!(addr(h, it), prefab_addrs[ours.src_props[i as usize]], "srcProps[{i}]");
    }
    for g in gos {
        ok(h.request(json!({"call": "UnityEngine.Object.Destroy", "args": [{"handle": g}], "param_types": ["UnityEngine.Object"], "main": true})), "Destroy");
    }
}
