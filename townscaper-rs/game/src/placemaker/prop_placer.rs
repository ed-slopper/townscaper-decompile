//! `Placemaker.PropPlacer` (rows E10b1–E10b4): props on the built modules. Row E10b1: the prefabs as data
//! ([`PropPrefab`], from the player's files: `ts_data::props`), the prop objects in the scene ([`PropScene`]:
//! the engine's transforms ([`Transforms`]) and each prop object's state), and the pools — `GetProp` takes a
//! pooled instance or instantiates the prefab (`RecursivePropNodeSetup` then links its nodes and gathers their
//! meshes for batching), `OnEnable` maps the pools back. Row E10b2: `PlacePropsInQube` — each module
//! container's placements rolled, checked against the neighbours and placed, bent into the square, their
//! modifiers run, their nodes linked to wires and given priorities. Row E10b3: the collider dictionary
//! (`AddColliderToDict`, `RemoveColliderFromDict`) and `RecursiveCheck`, which turns nodes on and off. Row
//! E10b4: taking props down (`ReturnProp`, `RemovePropsFromContainer`, `IterateDeconstructProps`) and the reset
//! hooks. The modifiers `PlacePropsInQube` runs (row E10c) are here too, as plain Rust; what only draws (the
//! wire meshes, the mesh animator, the flocks' landings, sounds) goes to a [`PropHost`]. Checked against the
//! running game by `tests/placemaker_prop_placer.rs` and `tests/placemaker_prop_placing.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;

use crate::os::intersections::{self, CORNERS};
use crate::placemaker::graphs::{CornerId, Graph, OrientedModuleSides, QubeId, SquareId};
use crate::placemaker::module_builder::{self as mb, ContainerId, ModuleBuilder};
use crate::placemaker::modules::{AssetRef, BaseModule, ModuleLibrary, ModuleMath};
use crate::placemaker::props::{NodeId, PropNode};
use crate::placemaker::timing::layer_master;
use crate::unity::collections;
use crate::unity::geometry::Bounds;
use crate::unity::mathematics::{Int3, Random};
use crate::unity::mathf;
use crate::unity::quaternion::{Matrix4x4, Quaternion};
use crate::unity::sse::X;
use crate::unity::transform::{Node, TransformId, Transforms};
use crate::unity::vector::{v3, Vector3, Vector4};

/// A material, as the host numbers them (compared by identity only).
pub type MaterialId = u64;
/// A mesh asset, as the host numbers them.
pub type MeshAsset = u64;

/// A mesh a `MeshFilter` holds: an asset, or a copy made at run time ([`PropScene::mesh_clones`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeshId {
    Asset(MeshAsset),
    Clone(u32),
}

/// How a mesh copy's uvs differ from its source's (`Object.Instantiate(mesh)` then `SetUVs(0, ..)`), each
/// vertex's `(u, v)` computed as the original does (data for the host's meshes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeshClone {
    /// `PropPlacer.GetMeshWithVoxelType`'s copy `k` (1..=15): `(u + (k - 4·(k / 4))·0.25, v + (k / 4)·0.25)`.
    Material { src: MeshId, k: i32 },
    /// `Window.GetMeshWithVoxelType`'s copy for voxel type `group` and variation `row`:
    /// `(u + (group % 4)·0.25, ((7·group + row) + v) + (group / 4)·0.25)`.
    Window { src: MeshId, group: i32, row: i32 },
}

impl MeshClone {
    /// One vertex's uv in the copy.
    pub fn uv(&self, u: f32, v: f32) -> (f32, f32) {
        match *self {
            MeshClone::Material { k, .. } => {
                let q = k / 4;
                (u + (k - q * 4) as f32 * 0.25, v + q as f32 * 0.25)
            }
            MeshClone::Window { group, row, .. } => {
                let q = group / 4;
                (u + (group - q * 4) as f32 * 0.25, ((7 * group + row) as f32 + v) + q as f32 * 0.25)
            }
        }
    }
}

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

/// `Placemaker.Props.PropCollider` (its `layer` is a `PropCollider.Layer`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropCollider {
    pub layer: u8,
    pub local_bounds: Bounds,
    pub world_bounds: Bounds,
}

/// `Placemaker.Props.PropTransformer`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropTransformer {
    pub fit_in_square: bool,
    pub scale_variance: f32,
    pub rotation_variance: f32,
    pub pos_variance: Vector3,
}

/// The `IPropModifier` components of a prefab object, in component order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Modifier {
    Transformer(PropTransformer),
    NgonScaler,
    /// `PropPropeller`: `waterSplashEffect` (a node of the prefab).
    Propeller { forward: bool, splash: Option<usize> },
    /// `WindVane`: `compass` and `rotator` (nodes of the prefab).
    WindVane { compass: Option<usize>, rotator: Option<usize> },
    Window,
    WireFlagMesh { uv: Vector4 },
    WireItalianMesh,
}

/// The `IPropEnable` components (the `PropMeshReference` that `RecursivePropNodeSetup` adds comes last).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnableKind {
    MeshReference,
    BirdLanding,
    ButterflyLanding,
}

/// The `IPropEnable` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnableCall {
    OnFirstEnable,
    OnEnable,
    OnDisable,
}

/// The wire meshes `PropWireAnchor`s make (`Setup(master, wire, length)`, row E10d).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireMesh {
    Flag,
    Italian,
}

/// `Placemaker.Props.PropWireAnchor` as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrefabWireAnchor {
    /// `srcWire`: the wire prefab.
    pub src_wire: Option<usize>,
    /// `wireConnectorNode`: a node of the same prefab.
    pub wire_connector_node: Option<usize>,
    pub connect_pos: Vector3,
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
    /// The `MeshFilter`'s shared mesh.
    pub mesh: Option<MeshAsset>,
    pub has_mesh_filter: bool,
    /// The `MeshRenderer`: enabled, and its `sharedMaterial` (the first).
    pub renderer: Option<(bool, Option<MaterialId>)>,
    /// `Prop.voxelTypeMask`.
    pub prop: Option<u8>,
    pub node: Option<PrefabPropNode>,
    pub colliders: Vec<PropCollider>,
    pub modifiers: Vec<Modifier>,
    pub enables: Vec<EnableKind>,
    pub colored_mesh: bool,
    pub wire_anchor: Option<PrefabWireAnchor>,
    /// A `PropWire`.
    pub wire: bool,
}

impl PrefabNode {
    /// `TryGetComponent<PropTransformer>`: the first.
    pub fn transformer(&self) -> Option<PropTransformer> {
        self.modifiers.iter().find_map(|m| match m {
            Modifier::Transformer(t) => Some(*t),
            _ => None,
        })
    }
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

/// A `PropWireAnchor`'s run-time state.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AnchorState {
    /// `propWires`: the wires' objects.
    pub prop_wires: Vec<TransformId>,
    pub voxel_type: u8,
}

/// A prop object's state: what its GameObject is and has.
#[derive(Clone, Debug, PartialEq)]
pub struct PropObject {
    pub name: String,
    pub layer: u32,
    /// The prefab and node it was cloned from.
    pub from: Option<(usize, usize)>,
    /// The instance's root (the prefab's node `j` is object `base + j`).
    pub base: TransformId,
    /// `Prop.srcProp`, on an instance's root: the prefab.
    pub src_prop: Option<usize>,
    pub node: Option<PropNode>,
    pub mesh_ref: Option<PropMeshReference>,
    pub renderer_enabled: Option<bool>,
    /// `MeshFilter.sharedMesh`.
    pub mesh: Option<MeshId>,
    pub colliders: Vec<PropCollider>,
    /// `Window.mf0` and `mf1`.
    pub window: (Option<TransformId>, Option<TransformId>),
    pub anchor: Option<AnchorState>,
    /// `PropWire.anchor0` and `anchor1` (the anchors' objects).
    pub wire: (Option<TransformId>, Option<TransformId>),
}

/// The objects props live among: the engine's transforms, each one's `activeSelf`, and for each a prop object's
/// state (`None` for the host's own transforms: containers, qubes, ...); and the meshes made at run time.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropScene {
    pub transforms: Transforms,
    pub active: Vec<bool>,
    pub objects: Vec<Option<PropObject>>,
    pub mesh_clones: Vec<MeshClone>,
    /// `Window.materialMeshes` (static: shared by every window).
    pub window_meshes: Vec<MeshId>,
}

impl PropScene {
    /// A host transform (not a prop object), active.
    pub fn add_transform(&mut self, n: Node) -> TransformId {
        self.add_host(n, true)
    }
    /// A host transform with its `activeSelf`.
    pub fn add_host(&mut self, n: Node, active: bool) -> TransformId {
        let id = self.transforms.add(n);
        self.active.push(active);
        self.objects.push(None);
        id
    }

    fn add_object(&mut self, n: Node, active: bool, o: PropObject) -> TransformId {
        let id = self.transforms.add(n);
        self.active.push(active);
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
    fn has_node(&self, id: TransformId) -> bool {
        self.objects[id].as_ref().is_some_and(|o| o.node.is_some())
    }

    /// `GameObject.activeInHierarchy`.
    pub fn active_in_hierarchy(&self, id: TransformId) -> bool {
        let mut p = Some(id);
        while let Some(i) = p {
            if !self.active[i] {
                return false;
            }
            p = self.transforms.nodes[i].parent;
        }
        true
    }

    /// The prefab node an object was cloned from.
    fn prefab_node<'a>(&self, prefabs: &'a [PropPrefab], id: TransformId) -> &'a PrefabNode {
        let (p, n) = self.object(id).from.expect("a cloned object");
        &prefabs[p].nodes[n]
    }

    /// `Object.Instantiate(prefab, parent, false)`: the prefab's objects cloned under `parent` with their local
    /// poses as stored (the root named `<name>(Clone)`), components copied, relations inside the prefab pointing
    /// at the clones.
    pub fn instantiate(&mut self, prefabs: &[PropPrefab], prefab: usize, parent: Option<TransformId>) -> TransformId {
        let pf = &prefabs[prefab];
        let base = self.transforms.nodes.len();
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
                layer: n.layer,
                from: Some((prefab, i)),
                base,
                src_prop: None,
                node: n.node.as_ref().map(|d| d.node.clone()),
                mesh_ref: None,
                renderer_enabled: n.renderer.map(|r| r.0),
                mesh: n.mesh.map(MeshId::Asset),
                colliders: n.colliders.clone(),
                window: (None, None),
                anchor: n.wire_anchor.map(|_| AnchorState::default()),
                wire: (None, None),
            };
            ids.push(self.add_object(tn, n.active, o));
        }
        assert_eq!(ids[ids.len() - 1], base + ids.len() - 1, "an instance's objects are consecutive");
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

    /// The object of the prefab node `j` of `id`'s instance.
    fn of_instance(&self, id: TransformId, j: usize) -> TransformId {
        self.object(id).base + j
    }

    /// A mesh copy.
    fn clone_mesh(&mut self, c: MeshClone) -> MeshId {
        self.mesh_clones.push(c);
        MeshId::Clone(self.mesh_clones.len() as u32 - 1)
    }

    /// `GetComponent<IPropEnable>()`: the first of the prefab's, else the added `PropMeshReference`.
    fn first_enable(&self, prefabs: &[PropPrefab], id: TransformId) -> Option<EnableKind> {
        self.prefab_node(prefabs, id).enables.first().copied().or(self.object(id).mesh_ref.as_ref().map(|_| EnableKind::MeshReference))
    }
}

/// What the prop placer calls outside the props: sounds, achievements, the mesh animator and the flocks
/// (`IPropEnable`), the wire meshes, the world meshes and the time budget.
pub trait PropHost {
    /// `master.manySoundsManager.PlaySound(sound, position, 0.4f, 1f)`.
    fn play_sound(&mut self, _sound: i32, _position: Vector3) {}
    /// `master.propAchievements.propCounts[achievement] += delta`, when the master has a `PropAchievements`.
    fn achievement(&mut self, _achievement: u8, _delta: i32) {}
    /// An `IPropEnable` call on a node's first `IPropEnable` component (`PropMeshReference` →
    /// `PropMeshAnimator.OnTurnedOn` / `OnTurnedOff`, row E10d; the landings → the flocks).
    fn prop_enable(&mut self, _scene: &mut PropScene, _obj: TransformId, _kind: EnableKind, _call: EnableCall) {}
    /// `PropWireFlagMesh.Setup` / `PropWireItalianMesh.Setup(master, wire, length)` (row E10d).
    fn wire_setup(&mut self, _scene: &mut PropScene, _wire: TransformId, _mesh: WireMesh, _length: f32) {}
    /// `master.worldMeshes.ReturnMesh(mesh)` (`PropWireFlagMesh.Reset`).
    fn return_world_mesh(&mut self, _mesh: Option<MeshId>) {}
    /// `WorldMaster.KeepGoing`.
    fn keep_going(&mut self) -> bool {
        true
    }
    /// `Debug.LogError`.
    fn log_error(&mut self, _message: &str) {}
}

/// A host that does nothing.
pub struct NoProps;
impl PropHost for NoProps {}

/// `Placemaker.PropPlacer`'s state.
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
    /// `master.materialMaster.hole0`: a window's hole mesh.
    pub hole_material: Option<MaterialId>,
    /// `colliderDict`: each chunk's `PropCollider`s (object, index among its colliders).
    pub collider_dict: HashMap<Int3, Vec<(TransformId, usize)>>,
    /// `materialMeshes`: each coloured mesh's sixteen voxel-type copies (the source first).
    pub material_meshes: Vec<MeshId>,
    /// The library placements' `Prop`s as prefabs.
    pub prefab_of: HashMap<AssetRef, usize>,
}

/// `PropNode.InserOverlap` on `a` (the comparer reads every node's priority).
fn inser_overlap(scene: &mut PropScene, a: NodeId, b: NodeId) {
    let mut n = scene.object_mut(a).node.take().expect("a PropNode");
    let pa = n.priority;
    n.inser_overlap(b, &|x| if x == a { pa } else { scene.node(x).priority });
    scene.object_mut(a).node = Some(n);
}

/// `Unity.Mathematics.Random.NextFloat(min, max)` (returned through x87, stored as a float).
fn next_float(r: &mut Random, min: f32, max: f32) -> f32 {
    (max - min) * r.next_float() + min
}

/// `UnityEngine.Mathf.Abs(int)` → `Math.Abs`: `int.MinValue` throws `OverflowException`.
fn abs_i32(v: i32) -> i32 {
    v.checked_abs().expect("OverflowException: Negating the minimum value of a twos complement number is invalid.")
}

/// `Vector4` to `Vector3`.
fn xyz(v: Vector4) -> Vector3 {
    v3(v.x, v.y, v.z)
}

fn neg(v: Vector3) -> Vector3 {
    v3(-v.x, -v.y, -v.z)
}

/// `a * s + b`, per component.
fn mul_add(a: Vector3, s: f32, b: Vector3) -> Vector3 {
    v3(a.x * s + b.x, a.y * s + b.y, a.z * s + b.z)
}

/// `(a - b) · Vector3.right`, as the original computes it.
fn dot_right(a: Vector3, b: Vector3) -> f32 {
    let r = Vector3::RIGHT;
    ((a.x - b.x) * r.x + r.y * (a.y - b.y)) + r.z * (a.z - b.z)
}

/// `Placemaker.Props.PropCollider.Collides` 0x705270: which collider layers block each other (the pair sorted,
/// then 0 with 0, 2 and 5; 1, 3, 4 and 5 with themselves).
pub fn collides(layer0: u8, layer1: u8) -> bool {
    let (lo, hi) = if layer1 < layer0 { (layer1, layer0) } else { (layer0, layer1) };
    match lo {
        0 => hi == 0 || hi == 2 || hi == 5,
        1 | 3 | 4 | 5 => hi == lo,
        _ => false,
    }
}

impl PropCollider {
    /// `Placemaker.Props.PropCollider.GetMatrix` 0x7052E0: `Translate(center) * Scale(extents)` of the local
    /// bounds (anchors, layer 3: the box turned 45° about y, its x and z columns `(c2 + c0) / 2` and
    /// `(c2 - c0) / 2`), then the object's `localToWorldMatrix` before it.
    pub fn get_matrix(&self, transforms: &Transforms, obj: TransformId) -> Matrix4x4 {
        let mut m = Matrix4x4::mul(&Matrix4x4::translate(self.local_bounds.center), &Matrix4x4::scale(self.local_bounds.extents));
        if self.layer == 3 {
            let h = |c: Vector4| Vector4 { x: c.x * 0.5, y: c.y * 0.5, z: c.z * 0.5, w: c.w * 0.5 };
            let (c0, c2) = (h(m.column(0)), h(m.column(2)));
            m.set_column(0, Vector4 { x: c2.x + c0.x, y: c2.y + c0.y, z: c2.z + c0.z, w: c2.w + c0.w });
            m.set_column(2, Vector4 { x: c2.x - c0.x, y: c2.y - c0.y, z: c2.z - c0.z, w: c2.w - c0.w });
        }
        Matrix4x4::mul(&transforms.local_to_world_matrix(obj), &m)
    }

    /// `Placemaker.Props.PropCollider.CalculateWorldBounds` 0x7050D0: `new Bounds(zero, -float.MaxValue · one)`
    /// grown over the box's eight corners (`Intersections`' table) through [`get_matrix`](Self::get_matrix).
    pub fn calculate_world_bounds(&mut self, transforms: &Transforms, obj: TransformId) {
        let m = self.get_matrix(transforms, obj);
        let f = -f32::MAX;
        self.world_bounds = Bounds::new(Vector3::ZERO, v3(f, f, f));
        for c in CORNERS {
            self.world_bounds.encapsulate(m.multiply_point(c));
        }
    }
}

/// The chunks (`chunkSize` 2) a bounds touches: `FloorToInt(min / 2)` to `CeilToInt(max / 2)`, exclusive.
fn chunk_range(b: &Bounds) -> ([i32; 3], [i32; 3]) {
    let (mn, mx) = (b.min(), b.max());
    let lo = [mathf::floor_to_int(mn.x * 0.5), mathf::floor_to_int(mn.y * 0.5), mathf::floor_to_int(mn.z * 0.5)];
    let hi = [mathf::ceil_to_int(mx.x * 0.5), mathf::ceil_to_int(mx.y * 0.5), mathf::ceil_to_int(mx.z * 0.5)];
    (lo, hi)
}

/// The placement context `PlacePropsInQube` reads: the graph, the library and the qube's module containers.
pub struct PlaceContext<'a> {
    pub graph: &'a Graph,
    pub lib: &'a ModuleLibrary,
    pub mb: &'a mut ModuleBuilder,
    /// Each module container's transform.
    pub container_transform: &'a dyn Fn(ContainerId) -> TransformId,
    /// A corner's `transform.position`.
    pub corner_position: &'a dyn Fn(CornerId) -> Vector3,
}

impl PropPlacer {
    /// `Placemaker.PropPlacer.OnEnable` 0x70A5C0: the pools (`srcProps[i]`'s pool is the container's child `i`),
    /// then `master.GetComponentsInChildren<PropNode>(true)` (`nodes_below_master`, depth first) from the last:
    /// those in the dictionary are added again.
    pub fn on_enable(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], nodes_below_master: &[NodeId]) {
        self.prop_pools = self.src_props.iter().enumerate().map(|(i, &p)| (p, scene.transforms.child(self.prop_pool_container, i))).collect();
        for &n in nodes_below_master.iter().rev() {
            if scene.node(n).in_dict {
                self.add_collider_to_dict(scene, prefabs, n);
            }
        }
    }

    /// `Placemaker.PropPlacer.OnReset` 0x70A7D0: the secondary props (the wires) hidden.
    pub fn on_reset(&mut self, scene: &mut PropScene) {
        scene.active[self.secondary_props_container] = false;
    }

    /// `Placemaker.PropPlacer.OnDoneResetting` 0x70A1A0: the secondary props shown again.
    pub fn on_done_resetting(&mut self, scene: &mut PropScene) {
        scene.active[self.secondary_props_container] = true;
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
        if pn.has_mesh_filter && pn.mesh.is_some() {
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

    // ---- meshes ----

    /// `Placemaker.PropPlacer.GetMeshWithVoxelType` 0x709BE0: a mesh not yet in `materialMeshes` goes in with
    /// fifteen copies, copy `k`'s uvs shifted to the `k`-th cell of a 4×4 grid of quarters; the result is the
    /// copy `voxel_type` of the mesh's group (a copy maps back to its own group).
    pub fn get_mesh_with_voxel_type(&mut self, scene: &mut PropScene, mesh: Option<MeshId>, voxel_type: u8) -> Option<MeshId> {
        let at = match self.material_meshes.iter().position(|&m| Some(m) == mesh) {
            Some(i) => i as i32,
            None => {
                let i = self.material_meshes.len() as i32;
                let src = mesh.expect("a mesh (the original throws)");
                self.material_meshes.push(src);
                for k in 1..16 {
                    let c = scene.clone_mesh(MeshClone::Material { src, k });
                    self.material_meshes.push(c);
                }
                i
            }
        };
        let i = (at / 16) * 16 + voxel_type as i32;
        Some(*self.material_meshes.get(i as usize).expect("ArgumentOutOfRangeException"))
    }

    // ---- the collider dictionary ----

    /// `Placemaker.PropPlacer.AddColliderToDict` 0x707C40: the node joins the dictionary (`inDict`); each of its
    /// colliders (from the last) gets its world bounds and goes into every chunk its box meets
    /// (`Intersections.OrientedBoxOrientedBox` with the chunk), sorted by priority among the chunk's colliders;
    /// on the way it meets the chunk's other colliders whose bounds it touches, of nodes not yet related to it,
    /// on layers that collide: anchors (layer 3) facing each other (`right`s ≥ 0.3 along the line between them,
    /// ≥ 0.5 against each other) of the same wire become each other's anchors; others (layer 7: only of the
    /// same prefab) whose boxes intersect and which aren't parent and child become overlaps — and a turned-on
    /// node of at least its priority hides it.
    pub fn add_collider_to_dict(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], node: NodeId) {
        scene.node_mut(node).in_dict = true;
        let count = scene.object(node).colliders.len();
        let mut src_prop: Option<TransformId> = None;
        let mut self_anchor: Option<TransformId> = None;
        for ci in (0..count).rev() {
            if scene.object(node).colliders[ci].layer == 7 {
                src_prop = Some(Self::prop_above(scene, prefabs, node));
            }
            let mut c = scene.object(node).colliders[ci];
            c.calculate_world_bounds(&scene.transforms, node);
            scene.object_mut(node).colliders[ci] = c;
            let (lo, hi) = chunk_range(&c.world_bounds);
            let m = c.get_matrix(&scene.transforms, node);
            for z in lo[2]..hi[2] {
                for y in lo[1]..hi[1] {
                    for x in lo[0]..hi[0] {
                        let key = Int3 { x, y, z };
                        if !intersections::oriented_box_oriented_box(&m, &get_chunk_matrix(key)) {
                            continue;
                        }
                        let Some(list) = self.collider_dict.get(&key).cloned() else {
                            self.collider_dict.insert(key, vec![(node, ci)]);
                            continue;
                        };
                        // where it goes: before the first collider of a higher priority (or its own)
                        let prio = scene.node(node).priority;
                        let mut at = 0;
                        while at < list.len() {
                            let on = list[at].0;
                            if on == node || scene.node(on).priority > prio {
                                break;
                            }
                            at += 1;
                        }
                        for &(oo, oi) in &list {
                            let on = oo;
                            if on == node {
                                continue;
                            }
                            let oc = scene.object(oo).colliders[oi];
                            if !c.world_bounds.intersects(&oc.world_bounds) {
                                continue;
                            }
                            if scene.node(node).relations.contains(&on) {
                                continue;
                            }
                            if !collides(c.layer, oc.layer) {
                                continue;
                            }
                            if c.layer == 7 {
                                let other = Self::prop_above(scene, prefabs, on);
                                if scene.object(src_prop.expect("set for layer 7")).src_prop != scene.object(other).src_prop {
                                    continue;
                                }
                            } else if c.layer == 3 {
                                let (pc, po) = (scene.transforms.position(node), scene.transforms.position(oo));
                                let d = po.sub(pc);
                                let mag = d.magnitude();
                                let dir = v3(d.x / mag, d.y / mag, d.z / mag);
                                let r1 = scene.transforms.right(node);
                                if 0.3 > ((-dir.x * r1.x) + (r1.y * -dir.y)) + (r1.z * -dir.z) {
                                    continue;
                                }
                                let r2 = scene.transforms.right(oo);
                                if 0.3 > ((r2.y * dir.y) + (r2.x * dir.x)) + (r2.z * dir.z) {
                                    continue;
                                }
                                if 0.5 > ((-r2.x * r1.x) + (r1.y * -r2.y)) + (r1.z * -r2.z) {
                                    continue;
                                }
                                let sa = *self_anchor.get_or_insert(node);
                                let src_a = Self::anchor_src_wire(scene, prefabs, sa);
                                let src_b = Self::anchor_src_wire(scene, prefabs, oo);
                                if src_a != src_b {
                                    continue;
                                }
                            }
                            if !intersections::oriented_box_oriented_box(&m, &oc.get_matrix(&scene.transforms, oo)) {
                                continue;
                            }
                            if scene.transforms.is_child_of(node, oo) || scene.transforms.is_child_of(oo, node) {
                                continue;
                            }
                            if c.layer == 3 {
                                scene.node_mut(node).add_anchor(on);
                                scene.node_mut(on).add_anchor(node);
                                continue;
                            }
                            inser_overlap(scene, node, on);
                            inser_overlap(scene, on, node);
                            let (po, pn) = (scene.node(on).priority, scene.node(node).priority);
                            // not below it (a NaN doesn't hide), and turned on
                            if po >= pn && scene.node(on).turned_on {
                                self.change_hide_count(scene, Some(node), true);
                            }
                        }
                        self.collider_dict.get_mut(&key).unwrap().insert(at, (node, ci));
                    }
                }
            }
        }
    }

    /// The `Prop` at or above an object (`GetComponent<Prop>()` up the parents; the original throws when none).
    fn prop_above(scene: &PropScene, prefabs: &[PropPrefab], obj: TransformId) -> TransformId {
        let mut t = Some(obj);
        while let Some(i) = t {
            if scene.objects[i].as_ref().is_some_and(|o| o.from.is_some()) && scene.prefab_node(prefabs, i).prop.is_some() {
                return i;
            }
            t = scene.transforms.nodes[i].parent;
        }
        panic!("NullReferenceException: no Prop above")
    }

    /// An anchor's `srcWire` (the original reads it through `GetComponent<PropWireAnchor>()`, which throws when
    /// there is none).
    fn anchor_src_wire(scene: &PropScene, prefabs: &[PropPrefab], obj: TransformId) -> Option<usize> {
        scene.prefab_node(prefabs, obj).wire_anchor.expect("a PropWireAnchor (the original throws)").src_wire
    }

    /// `Placemaker.PropPlacer.RemoveColliderFromDict` 0x70E920: when in the dictionary, its colliders (from the
    /// last) leave every chunk their box meets (a missing one logs "Prop Collider not int dict"; an emptied chunk
    /// is removed); then `RecursiveCheck`, and its anchors and overlaps drop it (from the last) and are cleared.
    pub fn remove_collider_from_dict(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], host: &mut dyn PropHost, node: NodeId) {
        if !scene.node(node).in_dict {
            return;
        }
        scene.node_mut(node).in_dict = false;
        let count = scene.object(node).colliders.len();
        for ci in (0..count).rev() {
            let c = scene.object(node).colliders[ci];
            let (lo, hi) = chunk_range(&c.world_bounds);
            let m = c.get_matrix(&scene.transforms, node);
            for z in lo[2]..hi[2] {
                for y in lo[1]..hi[1] {
                    for x in lo[0]..hi[0] {
                        let key = Int3 { x, y, z };
                        if !intersections::oriented_box_oriented_box(&m, &get_chunk_matrix(key)) {
                            continue;
                        }
                        let list = self.collider_dict.get_mut(&key).expect("KeyNotFoundException");
                        match list.iter().position(|&e| e == (node, ci)) {
                            Some(i) => {
                                list.remove(i);
                            }
                            None => host.log_error(&format!("Prop Collider not int dict ({}, {}, {}) {}", x, y, z, scene.object(node).name)),
                        }
                        if list.is_empty() {
                            self.collider_dict.remove(&key);
                        }
                    }
                }
            }
        }
        self.recursive_check(scene, prefabs, host, node, None);
        let n = scene.node(node);
        let anchor_at = n.anchor_index() as usize;
        let mut i = anchor_at as i32 + n.anchor_count as i32 - 1;
        while i >= scene.node(node).anchor_index() {
            let o = scene.node(node).relations[i as usize];
            let on = scene.node_mut(o);
            if let Some(k) = on.relations.iter().position(|&r| r == node) {
                on.relations.remove(k);
                on.anchor_count = on.anchor_count.wrapping_sub(1);
            }
            i -= 1;
        }
        let n = scene.node_mut(node);
        let (at, cnt) = (n.anchor_index() as usize, n.anchor_count as usize);
        n.relations.drain(at..at + cnt);
        n.anchor_count = 0;
        let mut i = n.overlap_index() + n.overlap_count as i32 - 1;
        while i >= scene.node(node).overlap_index() {
            let o = scene.node(node).relations[i as usize];
            let on = scene.node_mut(o);
            if let Some(k) = on.relations.iter().position(|&r| r == node) {
                on.relations.remove(k);
                on.overlap_count = on.overlap_count.wrapping_sub(1);
            }
            i -= 1;
        }
        let n = scene.node_mut(node);
        let (at, cnt) = (n.overlap_index() as usize, n.overlap_count as usize);
        n.relations.drain(at..at + cnt);
        n.overlap_count = 0;
    }

    // ---- turning nodes on and off ----

    /// `Placemaker.PropPlacer.RecursiveCheck` 0x70DC20. A node that isn't hidden and has motivations: added to
    /// the dictionary if it isn't (which may hide it: then nothing more); if not yet on, it turns on (shown);
    /// the first time, its children are bubble-sorted by priority and its `IPropEnable` gets `OnFirstEnable`; then
    /// `OnEnable`, its sound (when active in the hierarchy) and achievement; it hides the overlaps of lower
    /// priority (in order, up to the first not lower), shows its children, motivates its motevees, and checks
    /// those overlaps, its children (from the last) and its motevees (from the last). Otherwise a node that is on
    /// turns off: sound, achievement down, `OnDisable`, the reverse (overlaps shown, children hidden, motevees
    /// demotivated), checks from the last, and hidden unless something turned it back on. True when it is (or
    /// stays) on.
    pub fn recursive_check(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], host: &mut dyn PropHost, node: NodeId, _in_node: Option<NodeId>) -> bool {
        let n = scene.node(node);
        if n.hide_count == 0 && n.motivations != 0 {
            if !n.in_dict {
                self.add_collider_to_dict(scene, prefabs, node);
                if scene.node(node).hide_count != 0 {
                    return false;
                }
            }
            if scene.node(node).turned_on {
                return true;
            }
            scene.node_mut(node).turned_on = true;
            scene.active[node] = true;
            let n = scene.node(node);
            let (p, c, o, m) = (n.parent_count as usize, n.child_count as usize, n.overlap_count as usize, n.motevee_count as usize);
            let (child_at, overlap_at, motevee_at) = (p, p + c, p + c + o);
            if !n.has_been_turned_on {
                // bubble sort of the children by priority
                let mut sorted = c < 2;
                while !sorted {
                    sorted = true;
                    let rel = &scene.node(node).relations;
                    let mut cur = rel[child_at];
                    let mut i = child_at + 1;
                    while i < p + c {
                        let next = scene.node(node).relations[i];
                        if scene.node(cur).priority > scene.node(next).priority {
                            let r = &mut scene.node_mut(node).relations;
                            r[i - 1] = next;
                            r[i] = cur;
                            sorted = false;
                        } else {
                            cur = next;
                        }
                        i += 1;
                    }
                }
                scene.node_mut(node).has_been_turned_on = true;
                if let Some(k) = scene.first_enable(prefabs, node) {
                    host.prop_enable(scene, node, k, EnableCall::OnFirstEnable);
                }
            }
            if let Some(k) = scene.first_enable(prefabs, node) {
                host.prop_enable(scene, node, k, EnableCall::OnEnable);
            }
            let sound = scene.node(node).appearance_sound;
            if scene.active_in_hierarchy(node) && sound != 0 {
                host.play_sound(sound, scene.transforms.position(node));
            }
            let ach = scene.node(node).achievement;
            if ach != 0 {
                host.achievement(ach, 1);
            }
            let prio = scene.node(node).priority;
            let mut hidden = o;
            for i in 0..o {
                let x = scene.node(node).relations[overlap_at + i];
                if scene.node(x).priority >= prio {
                    hidden = i;
                    break;
                }
                self.change_hide_count(scene, Some(x), true);
            }
            for i in (0..c).rev() {
                let x = scene.node(node).relations[child_at + i];
                self.change_hide_count(scene, Some(x), false);
            }
            for i in (0..m).rev() {
                let x = scene.node(node).relations[motevee_at + i];
                let mo = &mut scene.node_mut(x).motivations;
                *mo = mo.wrapping_add(1);
            }
            for i in 0..hidden {
                let x = scene.node(node).relations[overlap_at + i];
                self.recursive_check(scene, prefabs, host, x, Some(node));
            }
            for i in (0..c).rev() {
                let x = scene.node(node).relations[child_at + i];
                self.recursive_check(scene, prefabs, host, x, None);
            }
            for i in (0..m).rev() {
                let x = scene.node(node).relations[motevee_at + i];
                self.recursive_check(scene, prefabs, host, x, Some(node));
            }
            return true;
        }
        if !n.turned_on {
            return false;
        }
        scene.node_mut(node).turned_on = false;
        let sound = scene.node(node).appearance_sound;
        if sound != 0 {
            host.play_sound(sound, scene.transforms.position(node));
        }
        let ach = scene.node(node).achievement;
        if ach != 0 {
            host.achievement(ach, -1);
        }
        if let Some(k) = scene.first_enable(prefabs, node) {
            host.prop_enable(scene, node, k, EnableCall::OnDisable);
        }
        let n = scene.node(node);
        let (p, c, o, m) = (n.parent_count as usize, n.child_count as usize, n.overlap_count as usize, n.motevee_count as usize);
        let (overlap_at, motevee_at) = (p + c, p + c + o);
        let prio = n.priority;
        let mut shown = o;
        for i in 0..o {
            let x = scene.node(node).relations[overlap_at + i];
            if scene.node(x).priority >= prio {
                shown = i;
                break;
            }
            self.change_hide_count(scene, Some(x), false);
        }
        for i in (p..p + c).rev() {
            let x = scene.node(node).relations[i];
            self.change_hide_count(scene, Some(x), true);
        }
        for i in (motevee_at..motevee_at + m).rev() {
            let x = scene.node(node).relations[i];
            let mo = &mut scene.node_mut(x).motivations;
            *mo = mo.wrapping_sub(1);
        }
        for i in (overlap_at..overlap_at + shown).rev() {
            let x = scene.node(node).relations[i];
            self.recursive_check(scene, prefabs, host, x, Some(node));
        }
        for i in (p..p + c).rev() {
            let x = scene.node(node).relations[i];
            self.recursive_check(scene, prefabs, host, x, Some(node));
        }
        for i in (motevee_at..motevee_at + m).rev() {
            let x = scene.node(node).relations[i];
            self.recursive_check(scene, prefabs, host, x, Some(node));
        }
        if !scene.node(node).turned_on {
            scene.active[node] = false;
        }
        false
    }

    // ---- placing ----

    /// `Placemaker.PropPlacer.PlacePropsInQube` 0x70A800. Nothing without possible modules. The random state is
    /// seeded with `|hash of the qube's position|`. Each of the qube's module containers (from the last) that
    /// has no props yet: its module (the first container) or decor's placements (from the last) — each rolls
    /// its priority (`NextFloat(prioAdd + priority, priority + 1 + prioAdd)`, below 0: skipped) and must stand
    /// on a voxel type its prop allows; its matrix (the orientation's times the placement's) is bent into the
    /// square at its position: up and forward through `ModuleBuilder.MultiplyVector`, the position through
    /// `MultiplyPoint`. A placement at a side (|x| or |z| past 0.4, facing a side the container's profile has)
    /// leans towards the neighbouring qube's square (its forward blended by `2|x|` with the neighbour's); a
    /// decor's at a side only shows on the corner with the smaller x, one at both sides only at the right corner
    /// pair. Then the prop (`GetProp`) under the container: position, rotation `LookRotation(-forward, up)`
    /// (mirrored placements flip x), scale; its objects breadth first: coloured meshes take the voxel type,
    /// transformers that fit in the square are bent in as well, every `IPropModifier` (from the last) runs; a
    /// node is hidden, given a priority (a root: the roll, joining the placed list; others: below their parents')
    /// and, with anchors, wired to its anchored partners; a node that rolled below 0 stops its branch. Last, the
    /// placed nodes are sorted by priority and checked from the last.
    pub fn place_props_in_qube(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], ctx: &mut PlaceContext, host: &mut dyn PropHost, qube: QubeId) {
        let graph = ctx.graph;
        let q = graph.qube(qube);
        if q.possible_modules.is_empty() {
            return;
        }
        let qpos = graph.qube_position(qube);
        let mut random = Random::new(abs_i32(qpos.hash_code()) as u32);
        let sq_id = q.square.expect("a qube in a square");
        let square = graph.square(sq_id);
        let sides0 = q.possible_modules[0].sides;
        let mut placed: Vec<NodeId> = Vec::new();
        let mut list: Vec<TransformId> = Vec::new();
        let children: Vec<ContainerId> = ctx.mb.children(qube).to_vec();
        let level = graph.qube_sibling_index(qube);
        let math = ModuleMath::get();
        for i in (0..children.len()).rev() {
            let cont = children[i];
            if ctx.mb.containers[cont.0 as usize].has_props {
                continue;
            }
            ctx.mb.containers[cont.0 as usize].has_props = true;
            let oms = *q.possible_modules.get(i).expect("ArgumentOutOfRangeException");
            let om = oms.oriented_module;
            let module: &BaseModule = if i == 0 {
                &ctx.lib.modules[om.module_index as usize].base
            } else {
                if ctx.lib.decor_modules.len() <= om.module_index as usize {
                    host.log_error(&format!("{i} {} {}", om.module_index, q.possible_modules.len()));
                }
                &ctx.lib.decor_modules.get(om.module_index as usize).expect("ArgumentOutOfRangeException").base
            };
            let orientation = &math.orientations[(om.flags & 7) as usize];
            let o_matrix = orientation.matrix();
            let container_t = (ctx.container_transform)(cont);
            for pi in (0..module.prop_placements.len()).rev() {
                let pl = module.prop_placements[pi];
                let prefab = *self.prefab_of.get(&pl.prop).expect("a known prop");
                let root = &prefabs[prefab].nodes[0];
                let prio = root.node.as_ref().expect("a PropNode on the prop (the original throws)").node.priority;
                let pa = pl.prio_add as f32;
                let roll = next_float(&mut random, pa + prio, (prio + 1.0) + pa);
                if 0.0 > roll {
                    continue;
                }
                let corner_index = orientation.corner_transform1.get(pl.corner as i32);
                let corner_value = if pl.corner < 0 { 0 } else { q.corner_values[corner_index as usize] };
                if root.prop.expect("a Prop") & (1u8 << (corner_value & 3)) == 0 {
                    continue;
                }
                let m = Matrix4x4::mul(&o_matrix, &pl.get_matrix());
                let (c1, c2, c3) = (xyz(m.column(1)), xyz(m.column(2)), xyz(m.column(3)));
                let one = Vector3::ONE;
                let p = v3(one.x * 0.5 + c3.x, c3.y + one.y * 0.5, c3.z + one.z * 0.5);
                let mut ts = mb::get_ts(p);
                let up = mb::multiply_vector(square, ts, c1);
                let mut fwd = mb::multiply_vector(square, ts, c2);
                let pos = mb::multiply_point(square, ts, p);
                let x_side: i32 = if c3.x <= 0.0 { 3 } else { 1 };
                let z_side: i32 = if 0.0 < c3.z { 2 } else { 0 };
                let ax = c3.x.abs();
                let az = c3.z.abs();
                let sides = oms.sides;
                let side_x = 0.4 < ax && sides.get0(x_side) != 0;
                let side_z = 0.4 < az && sides.get0(z_side) != 0;
                if !(side_x && side_z) {
                    // the lean towards each neighbour, and the decor's corner check
                    let mut skip = false;
                    for (side, a, flag) in [(x_side, ax, side_x), (z_side, az, side_z)] {
                        if !(0.0 < a) {
                            continue;
                        }
                        let s0 = sides0.get0(side);
                        if s0 != 0 && !(level == 0 && ctx.lib.water_sides.contains(&(s0 as u32))) {
                            let rel = graph.get_qube_relation(qube, side as i8);
                            if rel.index_in_other != -1 {
                                let nq = rel.qube.expect("a qube");
                                let nsq = graph.square(graph.qube(nq).square.expect("a square"));
                                let k = (side + rel.index_in_other as i32).rem_euclid(4);
                                let nm = math.orientations[k as usize].matrix();
                                let n = xyz(nm.column(0));
                                let zero = Vector3::ZERO;
                                let v = v3((n.x + c3.x) + one.x * 0.5, one.y * 0.5 + (n.y + c3.y), one.z * 0.5 + (c3.z + n.z));
                                let ts2 = mb::get_ts(Vector3::min(one, Vector3::max(zero, v)));
                                // (the right vectors, leaned the same way, only feed a rotation the original drops)
                                let fc2 = if side == x_side { c2 } else { neg(c2) };
                                let f2 = mb::multiply_vector(nsq, ts2, nm.multiply_vector(fc2));
                                let s = a + a;
                                fwd = mul_add(f2, s, fwd);
                            }
                        }
                        if i > 0 && flag {
                            let ca = square.get_corner(side as i8).expect("a corner");
                            let cb = square.get_corner(if side == x_side { ((side + 1) & 3) as i8 } else { (side + 1) as i8 }).expect("a corner");
                            if (ctx.corner_position)(ca).x > (ctx.corner_position)(cb).x {
                                skip = true;
                                break;
                            }
                        }
                    }
                    if skip {
                        continue;
                    }
                } else {
                    let mut c = (x_side + 1) & 3;
                    let (a, b) = if c == z_side {
                        c = z_side + 1;
                        (x_side, z_side)
                    } else {
                        (z_side, x_side)
                    };
                    let pos_of = |k: i32| (ctx.corner_position)(square.get_corner(k as i8).expect("a corner"));
                    let (pa_, pb, pc) = (pos_of(a), pos_of(b), pos_of(c));
                    if 0.0 > dot_right(pa_, pb) {
                        continue;
                    }
                    if dot_right(pc, pb) > 0.0 {
                        continue;
                    }
                }
                let inst = self.get_prop(scene, prefabs, prefab, Some(container_t));
                let lossy = m.lossy_scale();
                let mirror = (lossy.z < 0.0) != (lossy.x < 0.0);
                let mirror_y = lossy.y < 0.0;
                let mut scale = lossy;
                if mirror != mirror_y {
                    scale.x = -scale.x;
                    fwd = neg(fwd);
                }
                let rot = Quaternion::look_rotation_up(neg(fwd), up);
                scene.transforms.set_local_position(inst, pos);
                scene.transforms.set_rotation(inst, rot);
                scene.transforms.set_local_scale(inst, scale);
                let use_corner = pl.corner >= 0 && corner_value < 0xF;
                list.clear();
                list.push(inst);
                let mut k = 0;
                while k < list.len() {
                    let t = list[k];
                    k += 1;
                    let pn = scene.prefab_node(prefabs, t);
                    if pn.colored_mesh {
                        let vt = if use_corner { corner_value } else { 0xF };
                        let mesh = scene.object(t).mesh;
                        scene.object_mut(t).mesh = self.get_mesh_with_voxel_type(scene, mesh, vt);
                    }
                    if let Some(tr) = pn.transformer() {
                        if tr.fit_in_square {
                            let m2 = Matrix4x4::mul(&m, &scene.transforms.world_to_local_matrix(inst));
                            let m3 = Matrix4x4::mul(&m2, &scene.transforms.local_to_world_matrix(t));
                            let (d1, d2, d3) = (xyz(m3.column(1)), xyz(m3.column(2)), xyz(m3.column(3)));
                            let p3 = v3(one.x * 0.5 + d3.x, d3.y + one.y * 0.5, d3.z + one.z * 0.5);
                            ts = mb::get_ts(p3);
                            let t_up = mb::multiply_vector(square, ts, d1);
                            let mut t_fwd = mb::multiply_vector(square, ts, d2);
                            let t_pos = mb::multiply_point(square, ts, p3);
                            scene.transforms.set_position(t, qpos.add(t_pos));
                            if mirror != mirror_y {
                                t_fwd = neg(t_fwd);
                            }
                            scene.transforms.set_rotation(t, Quaternion::look_rotation_up(neg(t_fwd), t_up));
                        }
                    }
                    for md in pn.modifiers.iter().rev() {
                        let s = ModifierStruct { square: sq_id, qube, ts, corner_index, oriented_module: oms, corner_value };
                        self.apply_modifier(scene, prefabs, &*ctx, &mut random, md, t, &s);
                    }
                    let mut stop = false;
                    if scene.has_node(t) {
                        scene.node_mut(t).turned_on = false;
                        scene.active[t] = false;
                        let n = scene.node_mut(t);
                        let mut lo = n.priority;
                        n.hide_count = 0;
                        let mut hi = lo + 1.0;
                        if n.parent_count == 0 {
                            n.priority = roll;
                            placed.push(t);
                        } else {
                            let pc = n.parent_count as usize;
                            for j in 0..pc {
                                let par = scene.node(t).relations[j];
                                let pp = scene.node(par).priority;
                                lo = mathf::min(mathf::max(pp - 0.1, pp * 0.9), lo);
                                hi = mathf::min(pp, hi);
                                if !scene.node(par).turned_on {
                                    self.change_hide_count(scene, Some(t), true);
                                }
                            }
                            let r = next_float(&mut random, lo, hi);
                            scene.node_mut(t).priority = r;
                            if 0.0 > r {
                                self.change_hide_count(scene, Some(t), true);
                                stop = true;
                            }
                        }
                        if !stop && pn.wire_anchor.is_some() {
                            self.wire_anchors(scene, prefabs, host, t, corner_value, &mut list);
                        }
                    }
                    if !stop {
                        for c in 0..scene.transforms.child_count(t) {
                            list.push(scene.transforms.child(t, c));
                        }
                    }
                }
            }
        }
        // STANDIN: the pooled list's capacity (the sort's depth limit) as a fresh list's
        let cap = collections::list_grow(0, placed.len()).max(placed.len());
        {
            let pr = |n: &NodeId| scene.node(*n).priority;
            let len = placed.len();
            collections::sort_by(&mut placed, 0, len, cap, &mut |a, b| crate::placemaker::props::single_compare_to(pr(a), pr(b)));
        }
        for &n in placed.iter().rev() {
            self.recursive_check(scene, prefabs, host, n, None);
        }
    }

    /// The `PropWireAnchor` part of `PlacePropsInQube`'s node setup: the anchor takes the corner value, its node
    /// joins the dictionary (which finds its anchored partners), and to each partner with an anchor a wire is
    /// strung — a `srcWire` prop under the secondary props, between the two `connectPos`es (the one with the
    /// smaller x first), at their middle, looking along it; the wire's node is the child of both anchors' nodes,
    /// motivated by both connector nodes; its mesh is made (`Setup`) and it joins the breadth-first list.
    fn wire_anchors(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], host: &mut dyn PropHost, node: NodeId, corner_value: u8, list: &mut Vec<TransformId>) {
        scene.object_mut(node).anchor.as_mut().expect("an anchor").voxel_type = corner_value;
        self.add_collider_to_dict(scene, prefabs, node);
        let mut k = 0;
        while k < scene.node(node).anchor_count as usize {
            let n = scene.node(node);
            let other = n.relations[n.anchor_index() as usize + k];
            k += 1;
            if scene.object(other).anchor.is_none() {
                continue;
            }
            let wa = scene.prefab_node(prefabs, node).wire_anchor.unwrap();
            let wb = scene.prefab_node(prefabs, other).wire_anchor.unwrap();
            let mut p0 = scene.transforms.transform_point(node, wa.connect_pos);
            let mut p1 = scene.transforms.transform_point(other, wb.connect_pos);
            let mid = v3((p0.x + p1.x) * 0.5, (p1.y + p0.y) * 0.5, (p1.z + p0.z) * 0.5);
            let d = v3(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
            let len = d.magnitude();
            let mut dir = v3(d.x / len, d.y / len, d.z / len);
            let wire_prefab = wa.src_wire.expect("a srcWire (the original throws)");
            let wire = self.get_prop(scene, prefabs, wire_prefab, Some(self.secondary_props_container));
            let (mut a0, mut a1) = (node, other);
            if p0.x > p1.x {
                std::mem::swap(&mut a0, &mut a1);
                std::mem::swap(&mut p0, &mut p1);
                dir = neg(dir);
            }
            scene.object_mut(wire).wire = (Some(a0), Some(a1));
            scene.object_mut(a0).anchor.as_mut().unwrap().prop_wires.push(wire);
            scene.object_mut(a1).anchor.as_mut().unwrap().prop_wires.push(wire);
            scene.transforms.set_position(wire, mid);
            scene.transforms.set_local_rotation(wire, Quaternion::look_rotation(dir));
            scene.node_mut(node).add_child(wire);
            scene.node_mut(other).add_child(wire);
            scene.node_mut(wire).add_parent(node);
            scene.node_mut(wire).add_parent(other);
            let conn = |s: &PropScene, a: TransformId| s.prefab_node(prefabs, a).wire_anchor.unwrap().wire_connector_node.map(|j| s.of_instance(a, j));
            let c0 = conn(scene, a0).expect("a wireConnectorNode (the original throws)");
            let c1 = conn(scene, a1).expect("a wireConnectorNode (the original throws)");
            scene.node_mut(wire).add_motevee(c0);
            scene.node_mut(wire).add_motevee(c1);
            let wpn = scene.prefab_node(prefabs, wire);
            if wpn.modifiers.iter().any(|m| matches!(m, Modifier::WireFlagMesh { .. })) {
                host.wire_setup(scene, wire, WireMesh::Flag, len);
            } else if wpn.modifiers.iter().any(|m| matches!(m, Modifier::WireItalianMesh)) {
                host.wire_setup(scene, wire, WireMesh::Italian, len);
            }
            list.push(wire);
        }
    }

    // ---- the modifiers (row E10c) ----

    /// `IPropModifier.Apply(master, ref random, PropModifierStruct)` of one modifier on object `t`.
    fn apply_modifier(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], ctx: &PlaceContext, random: &mut Random, md: &Modifier, t: TransformId, s: &ModifierStruct) {
        match *md {
            Modifier::Transformer(tr) => apply_transformer(scene, random, &tr, t),
            Modifier::NgonScaler => {
                let sq = ctx.graph.square(s.square);
                let (a, b) = if (0.5 < s.ts.x) == (s.ts.y < 0.5) { (sq.m[0], sq.m[2]) } else { (sq.m[1], sq.m[3]) };
                let (pa, pb) = (xyz(a.column(3)), xyz(b.column(3)));
                let d = pa.sub(pb);
                let f = d.magnitude() / std::f32::consts::SQRT_2;
                let ls = scene.transforms.local_scale(t);
                scene.transforms.set_local_scale(t, v3(ls.x * f, ls.y * f, ls.z * f));
            }
            Modifier::Propeller { splash, .. } => {
                let pos = scene.transforms.position(t);
                let fx = splash.map(|j| scene.of_instance(t, j)).expect("a waterSplashEffect (the original throws)");
                if pos.y < 1.5 {
                    scene.transforms.set_position(fx, v3(pos.x, 0.0, pos.z));
                    scene.active[fx] = true;
                } else {
                    scene.active[fx] = false;
                }
            }
            Modifier::WindVane { compass, rotator } => {
                let c = compass.map(|j| scene.of_instance(t, j)).expect("a compass (the original throws)");
                scene.transforms.set_rotation(c, Quaternion::IDENTITY);
                let r = rotator.map(|j| scene.of_instance(t, j)).expect("a rotator (the original throws)");
                scene.transforms.set_rotation(r, Quaternion::euler(0.0, 23.5, 0.0));
            }
            Modifier::Window => self.apply_window(scene, prefabs, ctx, t, s),
            Modifier::WireFlagMesh { .. } | Modifier::WireItalianMesh => {}
        }
    }

    /// `Placemaker.Props.Window`'s `IPropModifier.Apply` 0x1F8110: a variation hashed from the square's corner
    /// (`cornerIndex & 3`) and the qube's height; the window's two mesh filters (`mf0`: its `PropMeshReference`'s
    /// first item; `mf1`: its last other item drawn with the hole material — found once) both take
    /// `Window.GetMeshWithVoxelType(mf0's mesh, cornerValue, variation)`.
    fn apply_window(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], ctx: &PlaceContext, t: TransformId, s: &ModifierStruct) {
        let sq = ctx.graph.square(s.square);
        let corner = ctx.graph.corner(sq.get_corner((s.corner_index & 3) as i8).expect("a corner (the original throws)"));
        let y = mathf::round_to_int(ctx.graph.qube_position(s.qube).y);
        let var = abs_i32(
            corner.hex_pos.x.wrapping_mul(0x4c7f6dd1).wrapping_add(corner.hex_pos.y.wrapping_mul(0x4822a3e9)).wrapping_add(y.wrapping_mul(-0x553c3da3)).wrapping_add(-0x2de2f6bb),
        );
        if scene.object(t).window.0.is_none() {
            let items = scene.object(t).mesh_ref.as_ref().expect("a PropMeshReference (the original throws)").items.clone();
            let first = items.first().expect("ArgumentOutOfRangeException");
            scene.object_mut(t).window.0 = Some(first.mf);
            for it in items.iter().skip(1).rev() {
                let mat = scene.prefab_node(prefabs, it.mf).renderer.expect("a renderer (the original throws)").1;
                if mat.is_some() && mat == self.hole_material {
                    scene.object_mut(t).window.1 = Some(it.mf);
                }
            }
        }
        let (mf0, mf1) = scene.object(t).window;
        let mf0 = mf0.expect("mf0");
        let mesh = scene.object(mf0).mesh;
        let m = window_mesh_with_voxel_type(scene, mesh, s.corner_value, var);
        scene.object_mut(mf0).mesh = m;
        let mf1 = mf1.expect("NullReferenceException: Window.mf1");
        scene.object_mut(mf1).mesh = m;
    }

    // ---- taking props down ----

    /// `Placemaker.PropPlacer.ReturnProp` 0x70F2B0: the instance and its prefab walked side by side, breadth
    /// first; each node is hidden, checked (turning off) and taken out of the dictionary, takes back the
    /// prefab's priority and motivations and is no longer turned on before; an anchor's wires (from the last)
    /// are unhooked from both anchors and returned. Then every pair's `IPropModifier`s (from the last) reset
    /// from the prefab object, and the instance goes back into its pool, keeping its world pose.
    pub fn return_prop(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], host: &mut dyn PropHost, prop: TransformId) {
        let src = scene.object(prop).src_prop.expect("a srcProp (the original throws)");
        let mut pairs: Vec<(usize, TransformId)> = vec![(0, prop)];
        let mut i = 0;
        while i < pairs.len() {
            let (sn, dst) = pairs[i];
            i += 1;
            let kids = &prefabs[src].nodes[sn].children;
            for (c, &sc) in kids.iter().enumerate() {
                pairs.push((sc, scene.transforms.child(dst, c)));
            }
            if !scene.has_node(dst) {
                continue;
            }
            self.change_hide_count(scene, Some(dst), true);
            self.recursive_check(scene, prefabs, host, dst, None);
            self.remove_collider_from_dict(scene, prefabs, host, dst);
            let sn_node = prefabs[src].nodes[sn].node.as_ref();
            if sn_node.is_none() {
                host.log_error(&scene.object(prop).name);
            }
            let sn_node = sn_node.expect("the prefab's PropNode (the original throws)");
            let n = scene.node_mut(dst);
            n.priority = sn_node.node.priority;
            n.motivations = sn_node.node.motivations;
            n.has_been_turned_on = false;
            if scene.object(dst).anchor.is_some() {
                let mut w = scene.object(dst).anchor.as_ref().unwrap().prop_wires.len() as i32 - 1;
                while w >= 0 {
                    let wire = scene.object(dst).anchor.as_ref().unwrap().prop_wires[w as usize];
                    let (a0, a1) = scene.object(wire).wire;
                    let (a0, a1) = (a0.expect("anchor0"), a1.expect("anchor1"));
                    scene.node_mut(a0).remove_child(wire);
                    scene.node_mut(a1).remove_child(wire);
                    scene.node_mut(wire).remove_parent(a0);
                    scene.node_mut(wire).remove_parent(a1);
                    for a in [a0, a1] {
                        let pw = &mut scene.object_mut(a).anchor.as_mut().unwrap().prop_wires;
                        if let Some(k) = pw.iter().position(|&x| x == wire) {
                            pw.remove(k);
                        }
                    }
                    self.return_prop(scene, prefabs, host, wire);
                    let conn = |s: &PropScene, a: TransformId| s.prefab_node(prefabs, a).wire_anchor.unwrap().wire_connector_node.map(|j| s.of_instance(a, j));
                    let c0 = conn(scene, a0).expect("a wireConnectorNode");
                    let c1 = conn(scene, a1).expect("a wireConnectorNode");
                    scene.node_mut(wire).remove_motevee(c0);
                    scene.node_mut(wire).remove_motevee(c1);
                    scene.object_mut(wire).wire = (None, None);
                    w -= 1;
                }
            }
        }
        for &(sn, dst) in &pairs {
            let pn = &prefabs[src].nodes[sn];
            for md in pn.modifiers.iter().rev() {
                match md {
                    Modifier::Transformer(_) | Modifier::NgonScaler => {
                        scene.transforms.set_local_scale(dst, pn.local_scale);
                        scene.transforms.set_local_position(dst, pn.local_position);
                        scene.transforms.set_local_rotation(dst, pn.local_rotation);
                    }
                    Modifier::WireFlagMesh { .. } => {
                        host.return_world_mesh(scene.object(dst).mesh);
                        scene.object_mut(dst).mesh = None;
                    }
                    _ => {}
                }
            }
        }
        let pool = *self.prop_pools.get(&src).expect("KeyNotFoundException");
        scene.transforms.set_parent(prop, Some(pool), true);
    }

    /// `Placemaker.PropPlacer.RemovePropsFromContainer` 0x70F1C0: a container with props loses them: each child
    /// (from the last; one without a `PropNode` ends it) is hidden, checked and moved to the deconstruction
    /// container, keeping its world pose.
    pub fn remove_props_from_container(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], host: &mut dyn PropHost, has_props: &mut bool, container: TransformId) {
        if !*has_props {
            return;
        }
        *has_props = false;
        let n = scene.transforms.child_count(container);
        for i in (0..n).rev() {
            let c = scene.transforms.child(container, i);
            if !scene.has_node(c) {
                return;
            }
            self.change_hide_count(scene, Some(c), true);
            self.recursive_check(scene, prefabs, host, c, None);
            scene.transforms.set_parent(c, Some(self.deconstruction_container), true);
        }
    }

    /// `Placemaker.PropPlacer.IterateDeconstructProps` 0x70A100: the deconstruction container's props (from the
    /// last) returned while `KeepGoing`; false when there were none.
    pub fn iterate_deconstruct_props(&mut self, scene: &mut PropScene, prefabs: &[PropPrefab], host: &mut dyn PropHost) -> bool {
        let mut i = scene.transforms.child_count(self.deconstruction_container) as i32;
        if i < 1 {
            return false;
        }
        loop {
            i -= 1;
            if !host.keep_going() {
                return true;
            }
            let c = scene.transforms.child(self.deconstruction_container, i as usize);
            self.return_prop(scene, prefabs, host, c);
            if i < 1 {
                return true;
            }
        }
    }
}

/// `Placemaker.Props.PropModifierStruct`.
#[derive(Clone, Copy, Debug)]
pub struct ModifierStruct {
    pub square: SquareId,
    pub qube: QubeId,
    pub ts: Vector4,
    pub corner_index: u8,
    pub oriented_module: OrientedModuleSides,
    pub corner_value: u8,
}

/// `Placemaker.Props.PropTransformer`'s `IPropModifier.Apply` 0x83F260: the local scale times
/// `(float)pow(2, NextFloat(-v, v))`, the local rotation turned by `NextFloat(-v, v)`° about y, the position
/// moved by `localToWorldMatrix.MultiplyVector` of a random offset (three rolls, x first), each when its
/// variance isn't zero.
fn apply_transformer(scene: &mut PropScene, random: &mut Random, tr: &PropTransformer, t: TransformId) {
    if tr.scale_variance != 0.0 {
        let ls = scene.transforms.local_scale(t);
        let v = tr.scale_variance;
        let e = next_float(random, -v, v);
        let f = 2f64.powf(e as f64) as f32;
        scene.transforms.set_local_scale(t, v3(ls.x * f, ls.y * f, ls.z * f));
    }
    if tr.rotation_variance != 0.0 {
        let lr = scene.transforms.local_rotation(t);
        let v = tr.rotation_variance;
        let a = next_float(random, -v, v);
        scene.transforms.set_local_rotation(t, Quaternion::mul(lr, Quaternion::euler(0.0, a, 0.0)));
    }
    let pv = tr.pos_variance;
    if pv.x == 0.0 && pv.y == 0.0 && pv.z == 0.0 {
        return;
    }
    let x = next_float(random, -pv.x, pv.x);
    let y = next_float(random, -pv.y, pv.y);
    let z = next_float(random, -pv.z, pv.z);
    let pos = scene.transforms.position(t);
    let d = scene.transforms.local_to_world_matrix(t).multiply_vector(v3(x, y, z));
    scene.transforms.set_position(t, v3(d.x + pos.x, d.y + pos.y, pos.z + d.z));
}

/// `Placemaker.Props.Window.GetMeshWithVoxelType` 0x1F7B20: a mesh not yet in `Window.materialMeshes` goes in
/// with 59 copies (fifteen voxel types × four variations, the source being type 0 variation 0); the result is
/// the copy for `voxel_type` and `variation % 4` of the mesh's group.
pub fn window_mesh_with_voxel_type(scene: &mut PropScene, mesh: Option<MeshId>, voxel_type: u8, variation: i32) -> Option<MeshId> {
    let at = match scene.window_meshes.iter().position(|&m| Some(m) == mesh) {
        Some(i) => i as i32,
        None => {
            let i = scene.window_meshes.len() as i32;
            let src = mesh.expect("a mesh (the original throws)");
            scene.window_meshes.push(src);
            for group in 0..15 {
                for row in (if group == 0 { 1 } else { 0 })..4 {
                    let c = scene.clone_mesh(MeshClone::Window { src, group, row });
                    scene.window_meshes.push(c);
                }
            }
            i
        }
    };
    let i = variation % 4 + ((at / 60) * 15 + voxel_type as i32) * 4;
    Some(*scene.window_meshes.get(i as usize).expect("ArgumentOutOfRangeException"))
}

/// `Placemaker.PropPlacer.GetChunkMatrix` 0x709A70: a collider chunk (`chunkSize` 2) as a matrix: identity
/// rotation, its centre `2c + 1` as translation.
pub fn get_chunk_matrix(c: Int3) -> Matrix4x4 {
    let f = |v: i32| v as f32 + v as f32 + 1.0;
    Matrix4x4 { m: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, f(c.x), f(c.y), f(c.z), 1.0] }
}
