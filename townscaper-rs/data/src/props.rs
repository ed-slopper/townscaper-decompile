//! The prop prefabs (row E10b1): every prop the module library places, as a flat tree of nodes with their
//! local poses, meshes and the fields of the game's prop components (`Prop`, `PropNode` with its relations
//! resolved to nodes, `PropCollider`, `PropTransformer`, `PropWireAnchor`), for
//! `ts_game::placemaker::props`. Read with the layouts of row F8g.

use std::collections::HashMap;

use crate::layout::Value;
use crate::objects::{AssetSet, PPtr};
use crate::scene::{self, Node};

/// An object of the install by file and path id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId {
    pub file: String,
    pub path_id: i64,
}

/// `Placemaker.Prop`.
#[derive(Clone, Debug, PartialEq)]
pub struct PropData {
    pub voxel_type_mask: u8,
}

/// A `PropNode` relation: a node of the same prefab, or one elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub enum Relation {
    Node(usize),
    Other(Option<AssetId>),
}

/// `Placemaker.Props.PropNode` as stored in the prefab.
#[derive(Clone, Debug, PartialEq)]
pub struct PropNodeData {
    pub priority: f32,
    pub appearance_sound: i32,
    pub achievement: u8,
    pub turned_on: bool,
    pub in_dict: bool,
    pub has_been_turned_on: bool,
    pub hide_count: u8,
    pub motivations: u8,
    pub relations: Vec<Relation>,
    pub parent_count: u8,
    pub child_count: u8,
    pub overlap_count: u8,
    pub motevee_count: u8,
    pub anchor_count: u8,
}

/// `Placemaker.Props.PropCollider`.
#[derive(Clone, Debug, PartialEq)]
pub struct PropColliderData {
    pub layer: i32,
    /// Center, then extents.
    pub local_bounds: [f32; 6],
    pub world_bounds: [f32; 6],
}

/// `Placemaker.Props.PropTransformer`.
#[derive(Clone, Debug, PartialEq)]
pub struct PropTransformerData {
    pub fit_in_square: bool,
    pub scale_variance: f32,
    pub rotation_variance: f32,
    pub pos_variance: [f32; 3],
}

/// `Placemaker.Props.PropWireAnchor`.
#[derive(Clone, Debug, PartialEq)]
pub struct PropWireAnchorData {
    pub src_wire: Option<AssetId>,
    pub prop_wires: Vec<Option<AssetId>>,
    pub voxel_type: i64,
    pub wire_connector_node: Relation,
    pub connect_pos: [f32; 3],
}

/// One GameObject of a prefab.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabNode {
    pub name: String,
    pub active: bool,
    pub layer: u32,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub local_position: [f32; 3],
    pub local_rotation: [f32; 4],
    pub local_scale: [f32; 3],
    /// Every component's kind in order (`MeshFilter`, `Placemaker.Props.Window`, ...), the Transform left out.
    pub kinds: Vec<String>,
    pub mesh: Option<AssetId>,
    pub prop: Option<PropData>,
    pub node: Option<PropNodeData>,
    /// In component order (a node can have several).
    pub colliders: Vec<PropColliderData>,
    pub transformers: Vec<PropTransformerData>,
    pub wire_anchor: Option<PropWireAnchorData>,
}

/// A prop prefab: its nodes, the root first, depth first in child order.
#[derive(Clone, Debug, PartialEq)]
pub struct PropPrefab {
    /// The prefab's `Prop` component.
    pub id: AssetId,
    pub nodes: Vec<PrefabNode>,
}

fn resolve(a: &AssetSet, file: &str, p: PPtr) -> Option<AssetId> {
    if p.is_null() {
        return None;
    }
    let f = a.file(file)?;
    let (of, o) = a.resolve(f, p)?;
    Some(AssetId { file: of.name.clone(), path_id: o.path_id })
}

fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Int(i) => Some(*i),
        Value::UInt(u) => Some(*u as i64),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}
fn float(v: Option<&Value>) -> Option<f32> {
    match v? {
        Value::Float(f) => Some(*f as f32),
        _ => None,
    }
}
fn boolean(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::Int(i) => Some(*i != 0),
        Value::UInt(u) => Some(*u != 0),
        _ => None,
    }
}
fn floats<const N: usize>(v: Option<&Value>) -> Option<[f32; N]> {
    match v? {
        Value::Floats(f) if f.len() == N => f.as_slice().try_into().ok(),
        s @ Value::Struct(_) if N == 3 => {
            let mut o = [0f32; N];
            for (k, n) in ["x", "y", "z"].iter().enumerate() {
                o[k] = float(s.get(n))?;
            }
            Some(o)
        }
        _ => None,
    }
}
fn pptr(v: Option<&Value>) -> Option<PPtr> {
    match v? {
        Value::PPtr(p) => Some(*p),
        _ => None,
    }
}
fn pptrs(v: Option<&Value>) -> Option<Vec<PPtr>> {
    match v? {
        Value::Array(a) => a.iter().map(|x| pptr(Some(x))).collect(),
        Value::Bytes(b) if b.is_empty() => Some(Vec::new()),
        _ => None,
    }
}

/// The prefab whose root holds the `Prop` component `prop` (relative to `file`).
pub fn prop_prefab(a: &AssetSet, file: &str, prop: PPtr) -> Option<PropPrefab> {
    let id = resolve(a, file, prop)?;
    let root = scene::subtree(a, file, prop).ok()?;
    // the nodes depth first; the PropNode components' path ids to their node
    let mut flat: Vec<(&Node, Option<usize>)> = Vec::new();
    fn walk<'a>(n: &'a Node, parent: Option<usize>, out: &mut Vec<(&'a Node, Option<usize>)>) {
        let me = out.len();
        out.push((n, parent));
        for c in &n.children {
            walk(c, Some(me), out);
        }
    }
    walk(&root, None, &mut flat);
    let mut node_of: HashMap<(String, i64), usize> = HashMap::new();
    for (i, (n, _)) in flat.iter().enumerate() {
        for c in &n.components {
            node_of.insert((n.file.clone(), c.path_id), i);
        }
    }
    let relation = |file: &str, p: PPtr| -> Relation {
        match resolve(a, file, p) {
            Some(id) => match node_of.get(&(id.file.clone(), id.path_id)) {
                Some(&i) => Relation::Node(i),
                None => Relation::Other(Some(id)),
            },
            None => Relation::Other(None),
        }
    };
    let mut nodes = Vec::with_capacity(flat.len());
    for (i, (n, parent)) in flat.iter().enumerate() {
        let mut pn = PrefabNode {
            name: n.name.clone(),
            active: n.active,
            layer: n.layer,
            parent: *parent,
            children: (0..flat.len()).filter(|&j| flat[j].1 == Some(i)).collect(),
            local_position: n.transform.local_position,
            local_rotation: n.transform.local_rotation,
            local_scale: n.transform.local_scale,
            kinds: n.components.iter().filter(|c| c.class_id != 4).map(|c| c.kind.clone()).collect(),
            mesh: None,
            prop: None,
            node: None,
            colliders: Vec::new(),
            transformers: Vec::new(),
            wire_anchor: None,
        };
        for c in &n.components {
            if let Some(m) = c.mesh {
                pn.mesh = resolve(a, &n.file, m);
            }
            let Some(v) = &c.fields else { continue };
            match c.kind.as_str() {
                "Placemaker.Prop" => pn.prop = Some(PropData { voxel_type_mask: int(v.get("voxelTypeMask"))? as u8 }),
                "Placemaker.Props.PropNode" => {
                    let rel = pptrs(v.get("relations"))?;
                    pn.node = Some(PropNodeData {
                        priority: float(v.get("priority"))?,
                        appearance_sound: int(v.get("appearanceSound"))? as i32,
                        achievement: int(v.get("achievement"))? as u8,
                        turned_on: boolean(v.get("turnedOn"))?,
                        in_dict: boolean(v.get("inDict"))?,
                        has_been_turned_on: boolean(v.get("hasBeenTurnedOn"))?,
                        hide_count: int(v.get("hideCount"))? as u8,
                        motivations: int(v.get("motivations"))? as u8,
                        relations: rel.into_iter().map(|p| relation(&n.file, p)).collect(),
                        parent_count: int(v.get("parentCount"))? as u8,
                        child_count: int(v.get("childCount"))? as u8,
                        overlap_count: int(v.get("overlapCount"))? as u8,
                        motevee_count: int(v.get("moteveeCount"))? as u8,
                        anchor_count: int(v.get("anchorCount"))? as u8,
                    })
                }
                "Placemaker.Props.PropCollider" => {
                    pn.colliders.push(PropColliderData {
                        layer: int(v.get("layer"))? as i32,
                        local_bounds: floats(v.get("localBounds"))?,
                        world_bounds: floats(v.get("worldBounds"))?,
                    })
                }
                "Placemaker.Props.PropTransformer" => {
                    pn.transformers.push(PropTransformerData {
                        fit_in_square: boolean(v.get("fitInSquare"))?,
                        scale_variance: float(v.get("scaleVariance"))?,
                        rotation_variance: float(v.get("rotationVariance"))?,
                        pos_variance: floats(v.get("posVariance"))?,
                    })
                }
                "Placemaker.Props.PropWireAnchor" => {
                    pn.wire_anchor = Some(PropWireAnchorData {
                        src_wire: pptr(v.get("srcWire")).and_then(|p| resolve(a, &n.file, p)),
                        prop_wires: pptrs(v.get("propWires"))?.into_iter().map(|p| resolve(a, &n.file, p)).collect(),
                        voxel_type: int(v.get("voxelType"))?,
                        wire_connector_node: relation(&n.file, pptr(v.get("wireConnectorNode"))?),
                        connect_pos: floats(v.get("connectPos"))?,
                    })
                }
                _ => {}
            }
        }
        nodes.push(pn);
    }
    Some(PropPrefab { id, nodes })
}

/// The prefabs of `props` (Prop component references relative to `file`), each once, in first-seen order.
pub fn prop_prefabs(a: &AssetSet, file: &str, props: impl IntoIterator<Item = PPtr>) -> Vec<PropPrefab> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in props {
        let Some(id) = resolve(a, file, p) else { continue };
        if seen.insert(id) {
            if let Some(pf) = prop_prefab(a, file, p) {
                out.push(pf);
            }
        }
    }
    out
}

/// The props the module library places (its modules', then its decor modules' placements, in stored order), each
/// prefab once in first-seen order; and each placement's prefab index in that order.
pub fn library_props(a: &AssetSet) -> Option<(Vec<PropPrefab>, Vec<usize>)> {
    use crate::objects::{class_id, MonoBehaviourHeader};
    let lib = crate::modules::module_library(a)?;
    let file = a.files.iter().find(|f| {
        f.meta.objects.iter().any(|o| {
            f.class_id(o) == class_id::MONO_BEHAVIOUR
                && MonoBehaviourHeader::read(f.bytes_of(o))
                    .ok()
                    .and_then(|mb| a.script_of(f, &mb))
                    .is_some_and(|s| s.full_name() == "Placemaker.Modules.ModuleLibrary")
        })
    })?;
    let placements: Vec<PPtr> = lib.modules.iter().chain(lib.decor_modules.iter()).flat_map(|m| m.prop_placements.iter().map(|p| p.prop)).collect();
    let prefabs = prop_prefabs(a, &file.name, placements.iter().copied());
    let index: Vec<usize> = placements
        .iter()
        .filter_map(|&p| {
            let id = resolve(a, &file.name, p)?;
            prefabs.iter().position(|pf| pf.id == id)
        })
        .collect();
    Some((prefabs, index))
}
