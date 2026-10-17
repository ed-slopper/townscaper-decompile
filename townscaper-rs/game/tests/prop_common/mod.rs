//! The prop prefabs for the prop tests: `ts_data::props` (the player's files) as `ts_game` prefabs, with the
//! install's materials and meshes numbered.
#![allow(dead_code)]

use std::collections::HashMap;

use ts_data::props::{AssetId, Relation};
use ts_game::placemaker::modules::{AssetRef, ModuleLibrary};
use ts_game::placemaker::prop_placer::{
    EnableKind, MaterialId, MeshAsset, Modifier, PrefabNode, PrefabPropNode, PrefabRelation, PrefabWireAnchor, PropCollider, PropPrefab, PropTransformer,
};
use ts_game::placemaker::props::PropNode;
use ts_game::unity::geometry::Bounds;
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::vector::{v3, Vector4};

pub struct Props {
    pub prefabs: Vec<PropPrefab>,
    pub data: Vec<ts_data::props::PropPrefab>,
    /// Each placement's prefab (module library order).
    pub placement_prefab: Vec<usize>,
    /// The install's materials: id to name.
    pub material_names: HashMap<MaterialId, String>,
    /// The install's meshes: id to name.
    pub mesh_names: HashMap<MeshAsset, String>,
}

fn bounds(b: [f32; 6]) -> Bounds {
    Bounds { center: v3(b[0], b[1], b[2]), extents: v3(b[3], b[4], b[5]) }
}

fn node_of(r: &Relation) -> Option<usize> {
    match r {
        Relation::Node(i) => Some(*i),
        Relation::Other(_) => None,
    }
}

pub fn load() -> Props {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let (data, placement_prefab) = ts_data::props::library_props(&a).expect("the library's props");
    let mut ids: HashMap<AssetId, MaterialId> = HashMap::new();
    let mut material_names = HashMap::new();
    let mut mesh_ids: HashMap<AssetId, MeshAsset> = HashMap::new();
    let mut mesh_names = HashMap::new();
    for f in &a.files {
        for o in &f.meta.objects {
            let cid = f.class_id(o);
            if cid == 21 || cid == 43 {
                let mut r = ts_data::objects::Reader::new(f.bytes_of(o));
                if let Ok(n) = r.string() {
                    let key = AssetId { file: f.name.clone(), path_id: o.path_id };
                    if cid == 21 {
                        let id = ids.len() as MaterialId + 1;
                        ids.insert(key, id);
                        material_names.insert(id, n);
                    } else {
                        let id = mesh_ids.len() as MeshAsset + 1;
                        mesh_ids.insert(key, id);
                        mesh_names.insert(id, n);
                    }
                }
            }
        }
    }
    let prefab_of_id: HashMap<AssetId, usize> = data.iter().enumerate().map(|(i, pf)| (pf.id.clone(), i)).collect();
    let prefabs = data
        .iter()
        .map(|pf| PropPrefab {
            nodes: pf
                .nodes
                .iter()
                .map(|n| {
                    let mut transformers = n.transformers.iter();
                    let mut modifiers = vec![];
                    let mut enables = vec![];
                    for k in &n.kinds {
                        match k.as_str() {
                            "Placemaker.Props.PropTransformer" => {
                                let t = transformers.next().unwrap();
                                modifiers.push(Modifier::Transformer(PropTransformer {
                                    fit_in_square: t.fit_in_square,
                                    scale_variance: t.scale_variance,
                                    rotation_variance: t.rotation_variance,
                                    pos_variance: v3(t.pos_variance[0], t.pos_variance[1], t.pos_variance[2]),
                                }))
                            }
                            "Placemaker.Props.PropNgonScaler" => modifiers.push(Modifier::NgonScaler),
                            "Placemaker.Props.PropPropeller" => {
                                let p = n.propeller.as_ref().unwrap();
                                modifiers.push(Modifier::Propeller { forward: p.forward, splash: node_of(&p.water_splash_effect) })
                            }
                            "Placemaker.Props.WindVane" => {
                                let w = n.wind_vane.as_ref().unwrap();
                                modifiers.push(Modifier::WindVane { compass: node_of(&w.compass), rotator: node_of(&w.rotator) })
                            }
                            "Placemaker.Props.Window" => modifiers.push(Modifier::Window),
                            "Placemaker.Props.PropWireFlagMesh" => {
                                let u = n.flag_mesh_uv.unwrap();
                                modifiers.push(Modifier::WireFlagMesh { uv: Vector4 { x: u[0], y: u[1], z: u[2], w: u[3] } })
                            }
                            "Placemaker.Props.PropWireItalianMesh" => modifiers.push(Modifier::WireItalianMesh),
                            "Placemaker.Life.BirdLanding" => enables.push(EnableKind::BirdLanding),
                            "Placemaker.Life.ButterflyLanding" => enables.push(EnableKind::ButterflyLanding),
                            "Placemaker.Props.PropMeshReference" => enables.push(EnableKind::MeshReference),
                            _ => {}
                        }
                    }
                    PrefabNode {
                        name: n.name.clone(),
                        active: n.active,
                        layer: n.layer,
                        parent: n.parent,
                        children: n.children.clone(),
                        local_position: v3(n.local_position[0], n.local_position[1], n.local_position[2]),
                        local_rotation: Quaternion { x: n.local_rotation[0], y: n.local_rotation[1], z: n.local_rotation[2], w: n.local_rotation[3] },
                        local_scale: v3(n.local_scale[0], n.local_scale[1], n.local_scale[2]),
                        mesh: n.mesh.as_ref().map(|m| *mesh_ids.get(m).expect("a known mesh")),
                        has_mesh_filter: n.kinds.iter().any(|k| k == "MeshFilter"),
                        renderer: n.renderer.as_ref().map(|(en, mats)| (*en, mats.first().cloned().flatten().and_then(|m| ids.get(&m).copied()))),
                        prop: n.prop.as_ref().map(|p| p.voxel_type_mask),
                        node: n.node.as_ref().map(|d| PrefabPropNode {
                            node: PropNode {
                                priority: d.priority,
                                appearance_sound: d.appearance_sound,
                                achievement: d.achievement,
                                turned_on: d.turned_on,
                                in_dict: d.in_dict,
                                has_been_turned_on: d.has_been_turned_on,
                                hide_count: d.hide_count,
                                motivations: d.motivations,
                                relations: Vec::new(),
                                parent_count: d.parent_count,
                                child_count: d.child_count,
                                overlap_count: d.overlap_count,
                                motevee_count: d.motevee_count,
                                anchor_count: d.anchor_count,
                            },
                            relations: d
                                .relations
                                .iter()
                                .map(|r| match r {
                                    Relation::Node(i) => PrefabRelation::Node(*i),
                                    Relation::Other(_) => PrefabRelation::Other,
                                })
                                .collect(),
                        }),
                        colliders: n
                            .colliders
                            .iter()
                            .map(|c| PropCollider { layer: c.layer as u8, local_bounds: bounds(c.local_bounds), world_bounds: bounds(c.world_bounds) })
                            .collect(),
                        modifiers,
                        enables,
                        colored_mesh: n.kinds.iter().any(|k| k == "Placemaker.Props.PropColoredMesh"),
                        wire_anchor: n.wire_anchor.as_ref().map(|w| PrefabWireAnchor {
                            src_wire: w.src_wire.as_ref().map(|s| *prefab_of_id.get(s).expect("a loaded wire prefab")),
                            wire_connector_node: node_of(&w.wire_connector_node),
                            connect_pos: v3(w.connect_pos[0], w.connect_pos[1], w.connect_pos[2]),
                        }),
                        wire: n.kinds.iter().any(|k| k == "Placemaker.Props.PropWire"),
                    }
                })
                .collect(),
        })
        .collect();
    Props { prefabs, data, placement_prefab, material_names, mesh_names }
}

/// The library placements' props (as `PropPlacement.prop` holds them) to prefabs, for `PropPlacer.prefab_of`.
pub fn prefab_of(lib: &ModuleLibrary, props: &Props) -> HashMap<AssetRef, usize> {
    let placements: Vec<AssetRef> =
        lib.modules.iter().map(|m| &m.base).chain(lib.decor_modules.iter().map(|m| &m.base)).flat_map(|b| b.prop_placements.iter().map(|p| p.prop)).collect();
    assert_eq!(placements.len(), props.placement_prefab.len(), "placements");
    let mut out = HashMap::new();
    for (p, &i) in placements.iter().zip(&props.placement_prefab) {
        let prev = out.insert(*p, i);
        assert!(prev.is_none() || prev == Some(i), "one prefab per prop reference");
    }
    out
}

// ---- the game's side ----

use crate::graph_common::{handle, list, ok, opt_handle};
use serde_json::{json, Value};
use ts_check::{arg, ret, Harness};

fn call(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> Value {
    ok(h.request(json!({"call": m, "class": class, "handle": t, "args": args, "main": true})), m)
}
fn call_keep(h: &mut Harness, t: u64, class: &str, m: &str, args: Vec<Value>) -> u64 {
    handle(&ok(h.request(json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": true})), m), m)
}
fn field_h(h: &mut Harness, t: u64, f: &str) -> Option<u64> {
    opt_handle(&ok(h.request(json!({"field": f, "handle": t, "main": true})), f))
}
pub fn addr(h: &mut Harness, t: u64) -> u64 {
    ok(h.request(json!({"addr": t})), "addr")["addr"].as_u64().unwrap()
}
pub fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n)
}
fn get_component(h: &mut Harness, t: u64, ty: u64) -> Option<u64> {
    opt_handle(&ok(h.request(json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true})), "GetComponent"))
}

/// The game's prefab `Prop` objects in our prefabs' order: the library's by its placements, the wires through
/// an anchor's `srcWire`.
pub fn game_prefabs(h: &mut Harness, props: &Props) -> Vec<u64> {
    let lib = handle(&ok(h.request(json!({"field": "moduleLibrary", "on": "Placemaker.WorldMaster", "main": true})), "lib"), "lib");
    let mut placement_addrs = vec![];
    let pl = list("Placemaker.PropPlacement, Assembly-CSharp");
    for (f, lc) in [("modules", list("Placemaker.Module, Assembly-CSharp")), ("decorModules", list("Placemaker.DecorModule, Assembly-CSharp"))] {
        let l = field_h(h, lib, f).unwrap();
        let n = ret::i32(&call(h, l, &lc, "get_Count", vec![]));
        let reqs: Vec<Value> = (0..n).map(|i| json!({"call": "get_Item", "class": lc, "handle": l, "args": [arg::i32(i)], "main": true, "keep": true})).collect();
        let mods: Vec<u64> = h.batch(reqs).into_iter().map(|r| handle(&ok(r, "module"), "module")).collect();
        for m in mods {
            let pls = field_h(h, m, "propPlacements").unwrap();
            for c in ret::bytes(&call(h, pls, &pl, "ToArray", vec![])).chunks(20) {
                placement_addrs.push(u32::from_le_bytes(c[12..16].try_into().unwrap()) as u64);
            }
        }
    }
    let prop_type = ty(h, "Placemaker.Prop, Assembly-CSharp");
    let all = handle(&ok(h.request(json!({"call": "UnityEngine.Resources.FindObjectsOfTypeAll", "args": [{"handle": prop_type}], "param_types": ["System.Type"], "main": true, "keep": true})), "all"), "all");
    let n = ret::i32(&call(h, all, "System.Array", "get_Length", vec![]));
    let mut by_addr = HashMap::new();
    for i in 0..n {
        let p = handle(&ok(h.request(json!({"call": "GetValue", "class": "System.Array", "handle": all, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true})), "GetValue"), "item");
        let a = addr(h, p);
        by_addr.insert(a, p);
    }
    let mut out: Vec<Option<u64>> = vec![None; props.prefabs.len()];
    for (k, &pa) in placement_addrs.iter().enumerate() {
        out[props.placement_prefab[k]].get_or_insert(by_addr[&pa]);
    }
    // the wires: an anchor's srcWire's Prop
    let anchor_type = ty(h, "Placemaker.Props.PropWireAnchor, Assembly-CSharp");
    for i in 0..props.prefabs.len() {
        for (j, n) in props.prefabs[i].nodes.iter().enumerate() {
            let Some(w) = n.wire_anchor.and_then(|w| w.src_wire) else { continue };
            if out[w].is_some() {
                continue;
            }
            let Some(root) = out[i] else { continue };
            let mut t = call_keep(h, root, "UnityEngine.Component", "get_transform", vec![]);
            // the node's path from the root
            let mut path = vec![];
            let mut k = j;
            while let Some(p) = props.prefabs[i].nodes[k].parent {
                path.push(props.prefabs[i].nodes[p].children.iter().position(|&c| c == k).unwrap());
                k = p;
            }
            for &c in path.iter().rev() {
                t = call_keep(h, t, "UnityEngine.Transform", "GetChild", vec![arg::i32(c as i32)]);
            }
            let a = get_component(h, t, anchor_type).expect("an anchor");
            let sw = field_h(h, a, "srcWire").expect("srcWire");
            out[w] = Some(get_component(h, sw, prop_type).expect("the wire's Prop"));
        }
    }
    out.into_iter().map(|o| o.expect("every prefab found in the game")).collect()
}
