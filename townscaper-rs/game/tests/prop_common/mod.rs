//! The prop prefabs for the prop tests: `ts_data::props` (the player's files) as `ts_game` prefabs, with the
//! install's materials numbered.
#![allow(dead_code)]

use std::collections::HashMap;

use ts_data::props::{AssetId, Relation};
use ts_game::placemaker::prop_placer::{MaterialId, PrefabNode, PrefabPropNode, PrefabRelation, PropPrefab};
use ts_game::placemaker::props::PropNode;
use ts_game::unity::quaternion::Quaternion;
use ts_game::unity::vector::v3;

pub struct Props {
    pub prefabs: Vec<PropPrefab>,
    pub data: Vec<ts_data::props::PropPrefab>,
    /// Each placement's prefab (module library order).
    pub placement_prefab: Vec<usize>,
    /// The install's materials: id to name.
    pub material_names: HashMap<MaterialId, String>,
}

pub fn load() -> Props {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let (data, placement_prefab) = ts_data::props::library_props(&a).expect("the library's props");
    let mut ids: HashMap<AssetId, MaterialId> = HashMap::new();
    let mut material_names = HashMap::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) == 21 {
                let mut r = ts_data::objects::Reader::new(f.bytes_of(o));
                if let Ok(n) = r.string() {
                    let id = ids.len() as MaterialId + 1;
                    ids.insert(AssetId { file: f.name.clone(), path_id: o.path_id }, id);
                    material_names.insert(id, n);
                }
            }
        }
    }
    let prefabs = data
        .iter()
        .map(|pf| PropPrefab {
            nodes: pf
                .nodes
                .iter()
                .map(|n| PrefabNode {
                    name: n.name.clone(),
                    active: n.active,
                    layer: n.layer,
                    parent: n.parent,
                    children: n.children.clone(),
                    local_position: v3(n.local_position[0], n.local_position[1], n.local_position[2]),
                    local_rotation: Quaternion { x: n.local_rotation[0], y: n.local_rotation[1], z: n.local_rotation[2], w: n.local_rotation[3] },
                    local_scale: v3(n.local_scale[0], n.local_scale[1], n.local_scale[2]),
                    mesh: n.mesh.is_some(),
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
                })
                .collect(),
        })
        .collect();
    Props { prefabs, data, placement_prefab, material_names }
}
