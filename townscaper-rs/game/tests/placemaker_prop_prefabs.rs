//! `ts_data::props` (E10b1) against the running game: the prop prefabs the module library places, read from the
//! player's files, must be the game's own prefab objects — the same prefabs in the same placement order, and
//! node by node the same names, active flags, layers, local poses (bit for bit), component kinds, meshes and
//! prop component fields (`PropNode` relations as nodes of the prefab). Needs `TS_HARNESS=1` and the install.

mod graph_common;
mod prop_common;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_data::props::{library_props, PrefabNode, PropPrefab, Relation};

const T: &str = "UnityEngine.Transform";

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
/// A field's integer value whatever its width (enums are bytes or ints).
fn int_of(b: &[u8]) -> i32 {
    match b.len() {
        1 => b[0] as i32,
        2 => i16::from_le_bytes([b[0], b[1]]) as i32,
        _ => i32::from_le_bytes(b[..4].try_into().unwrap()),
    }
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
/// The items of an object array.
fn array_items(h: &mut Harness, a: u64) -> Vec<u64> {
    let n = ret::i32(&call(h, a, "System.Array", "get_Length", vec![]));
    (0..n)
        .map(|i| handle(&ok(h.request(json!({"call": "GetValue", "class": "System.Array", "handle": a, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true})), "GetValue"), "item"))
        .collect()
}

/// The game's prefab nodes depth first in child order (as ours), with each node's components.
fn game_nodes(h: &mut Harness, root_t: u64) -> Vec<u64> {
    let mut out = vec![];
    fn walk(h: &mut Harness, t: u64, out: &mut Vec<u64>) {
        out.push(t);
        let n = ret::i32(&call(h, t, T, "get_childCount", vec![]));
        for i in 0..n {
            let c = call_keep(h, t, T, "GetChild", vec![arg::i32(i)]);
            walk(h, c, out);
        }
    }
    walk(h, root_t, &mut out);
    out
}

fn check_prefab(h: &mut Harness, prop: u64, ours: &PropPrefab, comp_type: u64, types: &Types) {
    let root_t = call_keep(h, prop, "UnityEngine.Component", "get_transform", vec![]);
    let nodes = game_nodes(h, root_t);
    assert_eq!(nodes.len(), ours.nodes.len(), "{}: node count", ours.nodes[0].name);
    // each node's components, and the PropNode components' addresses (for the relations)
    let mut comps: Vec<Vec<u64>> = vec![];
    let mut node_addr: Vec<Option<u64>> = vec![];
    let transform_addr: Vec<u64> = nodes.iter().map(|&t| addr(h, t)).collect();
    for &t in &nodes {
        let go = call_keep(h, t, "UnityEngine.Component", "get_gameObject", vec![]);
        let arr = handle(&ok(h.request(json!({"call": "GetComponents", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": comp_type}], "param_types": ["System.Type"], "main": true, "keep": true})), "GetComponents"), "components");
        let cs = array_items(h, arr);
        let pn = cs.iter().copied().find(|&c| class_name(h, c) == "Placemaker.Props.PropNode");
        node_addr.push(pn.map(|c| addr(h, c)));
        comps.push(cs);
    }
    for (i, (&t, o)) in nodes.iter().zip(&ours.nodes).enumerate() {
        let what = format!("{} node {i} {}", ours.nodes[0].name, o.name);
        check_node(h, t, &comps[i], o, &node_addr, &transform_addr, types, &what);
    }
}

struct Types {
    mesh_filter: u64,
    /// The install's materials by id, with their names.
    material_names: std::collections::HashMap<ts_data::props::AssetId, String>,
}

/// Every material of the install with its name (`m_Name` first).
fn material_names(a: &ts_data::objects::AssetSet) -> std::collections::HashMap<ts_data::props::AssetId, String> {
    let mut out = std::collections::HashMap::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) == 21 {
                let mut r = ts_data::objects::Reader::new(f.bytes_of(o));
                if let Ok(n) = r.string() {
                    out.insert(ts_data::props::AssetId { file: f.name.clone(), path_id: o.path_id }, n);
                }
            }
        }
    }
    out
}

fn class_name(h: &mut Harness, o: u64) -> String {
    let t = call_keep(h, o, "System.Object", "GetType", vec![]);
    ret::string(&call(h, t, "System.RuntimeType", "get_FullName", vec![]))
}

#[allow(clippy::too_many_arguments)]
fn check_node(h: &mut Harness, t: u64, comps: &[u64], o: &PrefabNode, node_addr: &[Option<u64>], transform_addr: &[u64], types: &Types, what: &str) {
    let go = call_keep(h, t, "UnityEngine.Component", "get_gameObject", vec![]);
    assert_eq!(name(h, go), o.name, "{what}: name");
    assert_eq!(ret::bytes(&call(h, go, "UnityEngine.GameObject", "get_activeSelf", vec![]))[0] != 0, o.active, "{what}: active");
    assert_eq!(ret::i32(&call(h, go, "UnityEngine.GameObject", "get_layer", vec![])) as u32, o.layer, "{what}: layer");
    let g = |h: &mut Harness, m: &str| bits(&f32s(&ret::bytes(&call(h, t, T, m, vec![]))));
    assert_eq!(g(h, "get_localPosition"), bits(&o.local_position), "{what}: localPosition");
    assert_eq!(g(h, "get_localRotation"), bits(&o.local_rotation), "{what}: localRotation");
    assert_eq!(g(h, "get_localScale"), bits(&o.local_scale), "{what}: localScale");
    // component kinds in order (the Transform first in the game's list)
    let kinds: Vec<String> = comps.iter().skip(1).map(|&c| class_name(h, c)).map(|k| k.trim_start_matches("UnityEngine.").to_string()).collect();
    assert_eq!(kinds, o.kinds, "{what}: components");
    // the mesh, by name
    if let Some(m) = &o.mesh {
        let mf = handle(&ok(h.request(json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": types.mesh_filter}], "param_types": ["System.Type"], "main": true, "keep": true})), "MeshFilter"), "mf");
        let mesh = call_keep(h, mf, "UnityEngine.MeshFilter", "get_sharedMesh", vec![]);
        let _ = m;
        assert!(!name(h, mesh).is_empty(), "{what}: mesh");
    }
    let (mut nc, mut nt) = (0, 0);
    // the renderer: enabled, and its materials by name
    if let Some((en, mats)) = &o.renderer {
        let r = comps.iter().copied().find(|&c| class_name(h, c) == "UnityEngine.MeshRenderer").expect("a renderer");
        assert_eq!(ret::bytes(&call(h, r, "UnityEngine.Renderer", "get_enabled", vec![]))[0] != 0, *en, "{what}: renderer enabled");
        // (only `sharedMaterial`, the first, is left in the build; it is what `ShouldBeBatched` reads)
        let g = call_keep(h, r, "UnityEngine.Renderer", "get_sharedMaterial", vec![]);
        let want = mats.first().cloned().flatten().and_then(|m| types.material_names.get(&m).cloned());
        assert_eq!(Some(name(h, g)), want, "{what}: sharedMaterial");
    }
    for &c in comps {
        match class_name(h, c).as_str() {
            "Placemaker.Prop" => {
                assert_eq!(field(h, c, "voxelTypeMask")[0], o.prop.as_ref().unwrap().voxel_type_mask, "{what}: voxelTypeMask");
            }
            "Placemaker.Props.PropNode" => {
                let n = o.node.as_ref().unwrap();
                assert_eq!(f32s(&field(h, c, "priority"))[0].to_bits(), n.priority.to_bits(), "{what}: priority");
                assert_eq!(int_of(&field(h, c, "appearanceSound")), n.appearance_sound, "{what}: appearanceSound");
                let b = |h: &mut Harness, f: &str| field(h, c, f)[0];
                assert_eq!(
                    [b(h, "achievement"), b(h, "turnedOn"), b(h, "inDict"), b(h, "hasBeenTurnedOn"), b(h, "hideCount"), b(h, "motivations")],
                    [n.achievement, n.turned_on as u8, n.in_dict as u8, n.has_been_turned_on as u8, n.hide_count, n.motivations],
                    "{what}: PropNode flags"
                );
                assert_eq!(
                    [b(h, "parentCount"), b(h, "childCount"), b(h, "overlapCount"), b(h, "moteveeCount"), b(h, "anchorCount")],
                    [n.parent_count, n.child_count, n.overlap_count, n.motevee_count, n.anchor_count],
                    "{what}: PropNode counts"
                );
                let l = field_h(h, c, "relations").unwrap();
                let lc = list("Placemaker.Props.PropNode, Assembly-CSharp");
                let cnt = ret::i32(&call(h, l, &lc, "get_Count", vec![]));
                assert_eq!(cnt as usize, n.relations.len(), "{what}: relation count");
                for (k, r) in n.relations.iter().enumerate() {
                    let it = call_keep(h, l, &lc, "get_Item", vec![arg::i32(k as i32)]);
                    let a = addr(h, it);
                    match r {
                        Relation::Node(j) => assert_eq!(Some(a), node_addr[*j], "{what}: relation {k}"),
                        Relation::Other(_) => assert!(!node_addr.contains(&Some(a)), "{what}: relation {k} is outside"),
                    }
                }
            }
            "Placemaker.Props.PropCollider" => {
                let cd = &o.colliders[nc];
                nc += 1;
                assert_eq!(int_of(&field(h, c, "layer")), cd.layer, "{what}: collider layer");
                assert_eq!(bits(&f32s(&field(h, c, "localBounds"))), bits(&cd.local_bounds), "{what}: localBounds");
                assert_eq!(bits(&f32s(&field(h, c, "worldBounds"))), bits(&cd.world_bounds), "{what}: worldBounds");
            }
            "Placemaker.Props.PropTransformer" => {
                let td = &o.transformers[nt];
                nt += 1;
                assert_eq!(field(h, c, "fitInSquare")[0] != 0, td.fit_in_square, "{what}: fitInSquare");
                let fs = |h: &mut Harness, f: &str| bits(&f32s(&field(h, c, f)));
                assert_eq!(fs(h, "scaleVariance"), bits(&[td.scale_variance]), "{what}: scaleVariance");
                assert_eq!(fs(h, "rotationVariance"), bits(&[td.rotation_variance]), "{what}: rotationVariance");
                assert_eq!(fs(h, "posVariance"), bits(&td.pos_variance), "{what}: posVariance");
            }
            "Placemaker.Props.PropWireAnchor" => {
                let wd = o.wire_anchor.as_ref().unwrap();
                assert_eq!(bits(&f32s(&field(h, c, "connectPos"))), bits(&wd.connect_pos), "{what}: connectPos");
                let cn = field_h(h, c, "wireConnectorNode");
                let want = match &wd.wire_connector_node {
                    Relation::Node(j) => node_addr[*j],
                    Relation::Other(_) => None,
                };
                assert_eq!(cn.map(|n| addr(h, n)), want, "{what}: wireConnectorNode");
            }
            "Placemaker.Props.PropPropeller" => {
                let pd = o.propeller.as_ref().unwrap();
                assert_eq!(field(h, c, "forward")[0] != 0, pd.forward, "{what}: forward");
                let fx = field_h(h, c, "waterSplashEffect").map(|g| call_keep(h, g, "UnityEngine.GameObject", "get_transform", vec![]));
                let want = match &pd.water_splash_effect {
                    Relation::Node(j) => Some(transform_addr[*j]),
                    Relation::Other(_) => None,
                };
                assert_eq!(fx.map(|t| addr(h, t)), want, "{what}: waterSplashEffect");
            }
            "Placemaker.Props.WindVane" => {
                let wd = o.wind_vane.as_ref().unwrap();
                for (f, r) in [("compass", &wd.compass), ("rotator", &wd.rotator)] {
                    let want = match r {
                        Relation::Node(j) => Some(transform_addr[*j]),
                        Relation::Other(_) => None,
                    };
                    let got = field_h(h, c, f);
                    assert_eq!(got.map(|t| addr(h, t)), want, "{what}: {f}");
                }
            }
            "Placemaker.Props.PropWireFlagMesh" => {
                assert_eq!(bits(&f32s(&field(h, c, "uv"))), bits(&o.flag_mesh_uv.unwrap()), "{what}: uv");
            }
            _ => {}
        }
    }
}

#[test]
fn prop_prefabs_match_the_game() {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let (prefabs, index) = library_props(&a).expect("the library's props");
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    // the game's placements' props, by address, in stored order
    let lib = handle(&ok(h.request(json!({"field": "moduleLibrary", "on": "Placemaker.WorldMaster", "main": true})), "lib"), "lib");
    let mut placement_addrs = vec![];
    let ml = list("Placemaker.Module, Assembly-CSharp");
    let dl = list("Placemaker.DecorModule, Assembly-CSharp");
    let pl = list("Placemaker.PropPlacement, Assembly-CSharp");
    for (f, lc) in [("modules", &ml), ("decorModules", &dl)] {
        let l = field_h(h, lib, f).unwrap();
        let n = ret::i32(&call(h, l, lc, "get_Count", vec![]));
        let reqs: Vec<Value> = (0..n).map(|i| json!({"call": "get_Item", "class": lc, "handle": l, "args": [arg::i32(i)], "main": true, "keep": true})).collect();
        let mods: Vec<u64> = h.batch(reqs).into_iter().map(|r| handle(&ok(r, "module"), "module")).collect();
        for m in mods {
            let pls = field_h(h, m, "propPlacements").unwrap();
            let raw = ret::bytes(&call(h, pls, &pl, "ToArray", vec![]));
            for c in raw.chunks(20) {
                placement_addrs.push(u32::from_le_bytes(c[12..16].try_into().unwrap()) as u64);
            }
        }
    }
    assert_eq!(placement_addrs.len(), index.len(), "placements");
    // every Prop object the engine has, by address
    let prop_type = ty(h, "Placemaker.Prop, Assembly-CSharp");
    let all = handle(&ok(h.request(json!({"call": "UnityEngine.Resources.FindObjectsOfTypeAll", "args": [{"handle": prop_type}], "param_types": ["System.Type"], "main": true, "keep": true})), "FindObjectsOfTypeAll"), "all");
    let mut by_addr = std::collections::HashMap::new();
    for p in array_items(h, all) {
        let a = addr(h, p);
        by_addr.insert(a, p);
    }
    // the same prefabs in the same placement order, then the wires their anchors make
    let mut game_prefab: Vec<u64> = vec![];
    for (k, &pa) in placement_addrs.iter().enumerate() {
        let i = index[k];
        if i == game_prefab.len() {
            game_prefab.push(pa);
        }
        assert_eq!(game_prefab[i], pa, "placement {k}: prefab {i}");
    }
    let wires = prop_common::game_prefabs(h, &prop_common::load());
    assert_eq!(wires.len(), prefabs.len(), "prefab count");
    for &w in &wires[game_prefab.len()..] {
        game_prefab.push(addr(h, w));
    }
    eprintln!("{} prefabs, {} of them wires", prefabs.len(), prefabs.len() - index.iter().max().unwrap() - 1);
    let comp_type = ty(h, "UnityEngine.Component, UnityEngine.CoreModule");
    let types = Types { mesh_filter: ty(h, "UnityEngine.MeshFilter, UnityEngine.CoreModule"), material_names: material_names(&a) };
    for (i, pf) in prefabs.iter().enumerate() {
        let p = *by_addr.get(&game_prefab[i]).expect("the prefab's Prop object");
        check_prefab(h, p, pf, comp_type, &types);
    }
}
