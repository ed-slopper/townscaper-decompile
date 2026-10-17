//! `Placemaker.PropPlacer` (rows E10b1–E10b4): props on the built modules. Row E10b1: the prefabs as data
//! ([`PropPrefab`], from the player's files: `ts_data::props`), the prop objects in the scene ([`PropScene`]:
//! the engine's transforms ([`Transforms`]) and each prop object's state), and the pools — `GetProp` takes a
//! pooled instance or instantiates the prefab (`RecursivePropNodeSetup` then links its nodes and gathers their
//! meshes for batching), `OnEnable` maps the pools back. Checked against the running game by
//! `tests/placemaker_prop_placer.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;

use crate::placemaker::props::{NodeId, PropNode};
use crate::placemaker::timing::layer_master;
use crate::unity::quaternion::Quaternion;
use crate::unity::sse::X;
use crate::unity::transform::{Node, TransformId, Transforms};
use crate::unity::vector::Vector3;

/// A material, as the host numbers them (compared by identity only).
pub type MaterialId = u64;

/// A prefab's `PropNode` relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefabRelation {
    /// A node of the same prefab (an instance's points at its own clone).
    Node(usize),
    /// An object outside the prefab.
    Other,
}

/// A prefab's `PropNode` as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabPropNode {
    pub node: PropNode,
    pub relations: Vec<PrefabRelation>,
}

/// One GameObject of a prop prefab.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabNode {
    pub name: String,
    pub active: bool,
    pub layer: u32,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub local_position: Vector3,
    pub local_rotation: Quaternion,
    pub local_scale: Vector3,
    /// The `MeshFilter`'s shared mesh is set.
    pub mesh: bool,
    pub has_mesh_filter: bool,
    /// The `MeshRenderer`: enabled, and its `sharedMaterial` (the first).
    pub renderer: Option<(bool, Option<MaterialId>)>,
    /// `Prop.voxelTypeMask`.
    pub prop: Option<u8>,
    pub node: Option<PrefabPropNode>,
}

/// A prop prefab: its nodes, the root first, depth first in child order.
#[derive(Clone, Debug, PartialEq)]
pub struct PropPrefab {
    pub nodes: Vec<PrefabNode>,
}

/// `Placemaker.Props.PropMeshReference.Item`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshRefItem {
    /// The `MeshFilter`'s object.
    pub mf: TransformId,
    /// A `BigMeshPart` was made for it (it is batched).
    pub big_mesh_part: bool,
    pub should_be_batched: bool,
}

/// `Placemaker.Props.PropMeshReference` (added to a node by `RecursivePropNodeSetup`, disabled).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropMeshReference {
    pub enabled: bool,
    pub items: Vec<MeshRefItem>,
}

/// A prop object's state: what its GameObject is and has.
#[derive(Clone, Debug, PartialEq)]
pub struct PropObject {
    pub name: String,
    pub active: bool,
    pub layer: u32,
    /// The prefab and node it was cloned from.
    pub from: Option<(usize, usize)>,
    /// `Prop.srcProp`, on an instance's root: the prefab.
    pub src_prop: Option<usize>,
    pub node: Option<PropNode>,
    pub mesh_ref: Option<PropMeshReference>,
    pub renderer_enabled: Option<bool>,
}

/// The objects props live among: the engine's transforms, and for each a prop object's state (`None` for the
/// host's own transforms: containers, qubes, ...).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropScene {
    pub transforms: Transforms,
    pub objects: Vec<Option<PropObject>>,
}

impl PropScene {
    /// A host transform (not a prop object).
    pub fn add_transform(&mut self, n: Node) -> TransformId {
        let id = self.transforms.add(n);
        self.objects.push(None);
        id
    }

    fn add_object(&mut self, n: Node, o: PropObject) -> TransformId {
        let id = self.transforms.add(n);
        self.objects.push(Some(o));
        id
    }

    pub fn object(&self, id: TransformId) -> &PropObject {
        self.objects[id].as_ref().expect("a prop object")
    }
    pub fn object_mut(&mut self, id: TransformId) -> &mut PropObject {
        self.objects[id].as_mut().expect("a prop object")
    }
    pub fn node(&self, id: NodeId) -> &PropNode {
        self.object(id).node.as_ref().expect("a PropNode")
    }
    pub fn node_mut(&mut self, id: NodeId) -> &mut PropNode {
        self.object_mut(id).node.as_mut().expect("a PropNode")
    }

    /// `Object.Instantiate(prefab, parent, false)`: the prefab's objects cloned under `parent` with their local
    /// poses as stored (the root named `<name>(Clone)`), components copied, relations inside the prefab pointing
    /// at the clones.
    pub fn instantiate(&mut self, prefabs: &[PropPrefab], prefab: usize, parent: Option<TransformId>) -> TransformId {
        let pf = &prefabs[prefab];
        let mut ids: Vec<TransformId> = Vec::with_capacity(pf.nodes.len());
        for (i, n) in pf.nodes.iter().enumerate() {
            let p = match n.parent {
                Some(pi) => Some(ids[pi]),
                None => parent,
            };
            let q = n.local_rotation;
            let tn = Node {
                t: X::from_f([n.local_position.x, n.local_position.y, n.local_position.z, 0.0]),
                r: X::from_f([q.x, q.y, q.z, q.w]),
                s: X::from_f([n.local_scale.x, n.local_scale.y, n.local_scale.z, 0.0]),
                parent: p,
            };
            let name = if i == 0 { format!("{}(Clone)", n.name) } else { n.name.clone() };
            let o = PropObject {
                name,
                active: n.active,
                layer: n.layer,
                from: Some((prefab, i)),
                src_prop: None,
                node: n.node.as_ref().map(|d| d.node.clone()),
                mesh_ref: None,
                renderer_enabled: n.renderer.map(|r| r.0),
            };
            ids.push(self.add_object(tn, o));
        }
        // the relations, now that every clone exists (children are added in order: the depth-first ids)
        for (i, n) in pf.nodes.iter().enumerate() {
            if let Some(d) = &n.node {
                let rel: Vec<NodeId> = d
                    .relations
                    .iter()
                    .map(|r| match r {
                        PrefabRelation::Node(j) => ids[*j],
                        PrefabRelation::Other => panic!("a prefab relation outside the prefab"),
                    })
                    .collect();
                self.node_mut(ids[i]).relations = rel;
            }
        }
        ids[0]
    }
}

/// `Placemaker.PropPlacer`'s pools.
#[derive(Clone, Debug, PartialEq)]
pub struct PropPlacer {
    /// `srcProps`: the prefabs with a pool, in the pools' order.
    pub src_props: Vec<usize>,
    /// `propPools`.
    pub prop_pools: HashMap<usize, TransformId>,
    pub prop_pool_container: TransformId,
    pub secondary_props_container: TransformId,
    pub deconstruction_container: TransformId,
    /// `master.materialMaster.doorMaterial`: door meshes are never batched.
    pub door_material: Option<MaterialId>,
}

impl PropPlacer {
    /// `Placemaker.PropPlacer.OnEnable` 0x70A5C0, the pools: `srcProps[i]`'s pool is the container's child `i`
    /// (the colliders of nodes already in the dictionary are re-added: row E10b4).
    pub fn on_enable(&mut self, scene: &PropScene) {
        self.prop_pools = self.src_props.iter().enumerate().map(|(i, &p)| (p, scene.transforms.child(self.prop_pool_container, i))).collect();
    }

    /// `Placemaker.PropPlacer.GetProp` 0x709EE0: from the prefab's pool (made on first use:
    /// `UnityFunctions.GetNewGameObject` — a nameless object, `SetParent(container, false)` — and the prefab added
    /// to `srcProps`) its last instance, moved under `parent` keeping its local pose; or, when the pool is empty, a
    /// new instance ([`PropScene::instantiate`]), `srcProp` set, its nodes set up ([`recursive_prop_node_setup`]).
    pub fn get_prop(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], src: usize, parent: Option<TransformId>) -> TransformId {
        let pool = match self.prop_pools.get(&src) {
            Some(&p) => p,
            None => {
                let p = scene.add_transform(Node::default());
                scene.transforms.set_parent(p, Some(self.prop_pool_container), false);
                self.prop_pools.insert(src, p);
                self.src_props.push(src);
                p
            }
        };
        let n = scene.transforms.child_count(pool);
        if n < 1 {
            let inst = scene.instantiate(prefabs, src, parent);
            scene.object_mut(inst).src_prop = Some(src);
            self.recursive_prop_node_setup(scene, prefabs, None, (src, 0), inst);
            inst
        } else {
            let inst = scene.transforms.child(pool, n - 1);
            scene.transforms.set_parent(inst, parent, false);
            inst
        }
    }

    /// `Placemaker.PropPlacer.RecursivePropNodeSetup` 0x70E500 on a fresh instance: a node's runtime state is
    /// reset (in the dictionary, turned on before, hide count, and the parent, child, overlap and motevee counts;
    /// not the anchors, nor the relations the prefab carries) and it becomes the child of the nearest node
    /// above; every mesh filter with a mesh gives that node's `PropMeshReference` (added, disabled, when missing)
    /// an item, batched unless it is on the no-shadows layer or uses the door material
    /// ([`should_be_batched`](Self::should_be_batched): its renderer is then turned off). Children the last first.
    pub fn recursive_prop_node_setup(&self, scene: &mut PropScene, prefabs: &[PropPrefab], parent: Option<NodeId>, src: (usize, usize), dst: TransformId) {
        let mut cur = parent;
        if scene.object(dst).node.is_some() {
            {
                let n = scene.node_mut(dst);
                n.in_dict = false;
                n.has_been_turned_on = false;
                n.hide_count = 0;
                n.child_count = 0;
                n.parent_count = 0;
                n.overlap_count = 0;
                n.motevee_count = 0;
            }
            if let Some(p) = parent {
                scene.node_mut(p).add_child(dst);
                scene.node_mut(dst).add_parent(p);
            }
            cur = Some(dst);
        }
        let pn = &prefabs[src.0].nodes[src.1];
        if pn.has_mesh_filter && pn.mesh {
            let c = cur.expect("a node above a mesh (the original throws)");
            if scene.object(c).mesh_ref.is_none() {
                scene.object_mut(c).mesh_ref = Some(PropMeshReference { enabled: false, items: Vec::new() });
            }
            let batched = self.should_be_batched(scene, prefabs, dst);
            let item = MeshRefItem { mf: dst, big_mesh_part: batched, should_be_batched: batched };
            scene.object_mut(c).mesh_ref.as_mut().unwrap().items.push(item);
        }
        let n = scene.transforms.child_count(dst);
        for i in (0..n).rev() {
            let sc = pn.children[i];
            let dc = scene.transforms.child(dst, i);
            self.recursive_prop_node_setup(scene, prefabs, cur, (src.0, sc), dc);
        }
    }

    /// `PropPlacer.<RecursivePropNodeSetup>g__ShouldBeBatched|24_0` 0x70F910 (inlined there): not on the
    /// no-shadows layer, and its renderer's `sharedMaterial` is not the door material; then the renderer is turned
    /// off (the big mesh draws it).
    pub fn should_be_batched(&self, scene: &mut PropScene, prefabs: &[PropPrefab], mf: TransformId) -> bool {
        let o = scene.object(mf);
        if o.layer as i32 == layer_master::NO_SHADOWS_LAYER {
            return false;
        }
        let (prefab, node) = o.from.expect("a cloned object");
        let mat = prefabs[prefab].nodes[node].renderer.expect("a renderer (the original throws)").1;
        if mat.is_some() && mat == self.door_material {
            return false;
        }
        scene.object_mut(mf).renderer_enabled = Some(false);
        true
    }

    /// `Placemaker.PropPlacer.ChangeHideCount` 0x7099B0 (and `HideCountPlus` 0x70A0E0 / `HideCountMinus` 0x70A0C0):
    /// the node's hide count up or down (a byte); 255 logs an error ("…" + its name).
    pub fn change_hide_count(&self, scene: &mut PropScene, node: Option<NodeId>, add: bool) -> bool {
        let Some(n) = node else { return false };
        let h = &mut scene.node_mut(n).hide_count;
        *h = if add { h.wrapping_add(1) } else { h.wrapping_sub(1) };
        *h == 0xFF
    }

    /// `Placemaker.PropPlacer.FirstTurnOn` 0x709A50.
    pub fn first_turn_on(&self, scene: &mut PropScene, node: NodeId) {
        scene.node_mut(node).has_been_turned_on = true;
    }
}

/// `Placemaker.PropPlacer.GetChunkMatrix` 0x709A70: a collider chunk (`chunkSize` 2) as a matrix: identity
/// rotation, its centre `2c + 1` as translation.
pub fn get_chunk_matrix(c: crate::unity::mathematics::Int3) -> crate::unity::quaternion::Matrix4x4 {
    let f = |v: i32| v as f32 + v as f32 + 1.0;
    crate::unity::quaternion::Matrix4x4 { m: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, f(c.x), f(c.y), f(c.z), 1.0] }
}
