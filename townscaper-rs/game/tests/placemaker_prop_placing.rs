//! `ts_game::placemaker::prop_placer` (E10b2–E10b4) against the original game. Needs `TS_HARNESS=1` and the
//! install. The wave function collapse loops of `placemaker_wfc.rs` build a town on the fresh game graph and ours
//! (random actions, then each step's module building), with a fresh `PropPlacer` on an inactive object behind the
//! stand-in master on the game side and ours behind our module builder: containers taken down lose their props
//! (`RemovePropsFromContainer`), `PropPlacer.IterateDeconstructProps` returns them to their pools, and the module
//! builder's grass queue (`ModuleBuilder.Iterate`) places the props of the rebuilt qubes (`PlacePropsInQube`,
//! with `AddColliderToDict`, `RecursiveCheck` and the wires). After every step every prop object must match:
//! where it hangs, active flags, local poses (bit for bit), the node states and their relations (by address),
//! the meshes (assets by name, run-time copies by their place in `materialMeshes` / `Window.materialMeshes`),
//! the colliders' world bounds, anchors and wires, windows; and the pools, `srcProps`, the collider dictionary
//! chunk by chunk. The stand-in's mesh animator is a fresh one (dimmed, so nothing animates), the flocks fresh
//! ones; the sounds are the scene's.

mod graph_common;
mod module_common;
mod prop_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::big_mesh::{BigMeshLog, BigMeshMaster};
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, OrientedModuleSides, QubeId, SquareId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::maker::Maker;
use ts_game::placemaker::module_builder::{ContainerId, ModuleBuilder, ModuleBuilderHost, ModuleContainer};
use ts_game::placemaker::modules::{ModuleLibrary, OrientedModule};
use ts_game::placemaker::prop_mesh_animator::{AnimatorHost, MeshFilterPool, PropMeshAnimator};
use ts_game::placemaker::prop_placer::{EnableCall, EnableKind, MeshClone, MeshId, MeshRefItem, PlaceContext, PropHost, PropPlacer, PropPrefab, PropScene};
use ts_game::placemaker::wfc::{WaveFunctionCollapse, WfcHost};
use ts_game::unity::mathematics::{f2, Int2, Int3};
use ts_game::unity::transform::{Node, TransformId};
use ts_game::unity::vector::Vector3;

const WFC: &str = "Placemaker.WaveFunctionCollapse";
const PP: &str = "Placemaker.PropPlacer";
const T: &str = "UnityEngine.Transform";
const V3T: &str = "UnityEngine.Vector3, UnityEngine.CoreModule";
const V2T: &str = "UnityEngine.Vector2, UnityEngine.CoreModule";

#[derive(Default)]
struct Host {
    ao: AoBaker,
    maker: Maker,
    wfc: WaveFunctionCollapse,
    cleared: Vec<QubeId>,
}
impl GraphHost for Host {
    fn ao_baker(&mut self) -> Option<&mut AoBaker> {
        Some(&mut self.ao)
    }
    fn maker(&mut self) -> Option<&mut Maker> {
        Some(&mut self.maker)
    }
    fn wfc_qube_updated(&mut self, graph: &mut Graph, qube: QubeId) {
        self.wfc.qube_updated(graph, qube);
    }
    fn wfc_iterate_propagation_count(&mut self) {
        self.wfc.iterate_propagation_count();
    }
    fn qube_removed(&mut self, graph: &mut Graph, qube: QubeId, side_mask: u8) {
        self.wfc.qube_removed(graph, qube, side_mask);
    }
    fn mark_module_containers_for_deconstruction(&mut self, qube: QubeId) {
        self.cleared.push(qube);
    }
}

/// What our placer calls outside: errors kept, the mesh references to our animator (the stand-in's is dimmed, at
/// state 0), the rest dropped (the game side's go to its stand-ins).
#[derive(Default)]
struct Calls {
    errors: Vec<String>,
    enables: usize,
    wires: usize,
    animator: Option<PropMeshAnimator>,
}
/// The stand-in animator's master: dimmed, state 0; the big meshes not compared here.
struct Dimmed;
impl AnimatorHost for Dimmed {
    fn is_dimmed(&self) -> bool {
        true
    }
    fn state(&self) -> u8 {
        0
    }
    fn viewport_point(&mut self, _p: Vector3) -> Option<Vector3> {
        None
    }
    fn time(&self) -> f32 {
        0.0
    }
    fn exp_t_fast(&self) -> f32 {
        1.0
    }
    fn exp_t_faster(&self) -> f32 {
        1.0
    }
    fn big_mesh_add(&mut self, _scene: &PropScene, _item: &MeshRefItem, _material: Option<ts_game::placemaker::prop_placer::MaterialId>) {}
    fn big_mesh_return(&mut self, _scene: &PropScene, _item: &MeshRefItem) {}
}
impl PropHost for Calls {
    fn prop_enable(&mut self, scene: &mut PropScene, obj: TransformId, kind: EnableKind, call: EnableCall) {
        self.enables += 1;
        if kind == EnableKind::MeshReference {
            let a = self.animator.as_ref().unwrap();
            match call {
                EnableCall::OnEnable => a.on_turned_on(scene, &mut Dimmed, obj),
                EnableCall::OnDisable => a.on_turned_off(scene, &mut Dimmed, obj),
                EnableCall::OnFirstEnable => {}
            }
        }
    }
    fn get_world_mesh(&mut self) -> u32 {
        self.wires += 1;
        self.wires as u32 - 1
    }
    fn log_error(&mut self, message: &str) {
        self.errors.push(message.into());
    }
}

/// Our props behind the module builder: its hooks, and the host transforms the props hang from.
struct PropSide<'a> {
    prefabs: &'a [PropPrefab],
    placer: PropPlacer,
    scene: PropScene,
    calls: Calls,
    squares_root: TransformId,
    square_pool: TransformId,
    qube_pool: TransformId,
    container_pool: TransformId,
    container_decon: TransformId,
    square_t: HashMap<SquareId, TransformId>,
    qube_t: HashMap<QubeId, TransformId>,
    container_t: HashMap<ContainerId, TransformId>,
    to_place: Vec<QubeId>,
}
impl ModuleBuilderHost for PropSide<'_> {
    fn remove_props_from_container(&mut self, id: ContainerId, container: &mut ModuleContainer) {
        if !container.has_props {
            return;
        }
        let t = self.container_t[&id];
        self.placer.remove_props_from_container(&mut self.scene, self.prefabs, &mut self.calls, &mut container.has_props, t);
    }
    fn place_props_in_qube(&mut self, _graph: &mut Graph, qube: QubeId) {
        self.to_place.push(qube);
    }
}

impl PropSide<'_> {
    fn host_t(&mut self, parent: TransformId) -> TransformId {
        let t = self.scene.add_transform(Node::default());
        self.scene.transforms.set_parent(t, Some(parent), false);
        t
    }
    /// The graph's and the module builder's objects as host transforms: squares under `squares`, qubes under
    /// their squares, containers under their qubes (identity local poses but the graph's local positions).
    fn sync(&mut self, graph: &Graph, mb: &ModuleBuilder) {
        let move_to = |s: &mut PropScene, t: TransformId, p: TransformId| {
            if s.transforms.nodes[t].parent != Some(p) {
                s.transforms.set_parent(t, Some(p), false);
            }
        };
        for &sq in &graph.squares {
            let t = match self.square_t.get(&sq) {
                Some(&t) => t,
                None => {
                    let t = self.host_t(self.squares_root);
                    self.square_t.insert(sq, t);
                    t
                }
            };
            move_to(&mut self.scene, t, self.squares_root);
            self.scene.transforms.set_local_position(t, graph.square(sq).local_position);
            for &q in &graph.square(sq).qubes {
                let qt = match self.qube_t.get(&q) {
                    Some(&x) => x,
                    None => {
                        let x = self.host_t(t);
                        self.qube_t.insert(q, x);
                        x
                    }
                };
                move_to(&mut self.scene, qt, t);
                self.scene.transforms.set_local_position(qt, graph.qube(q).local_position);
            }
        }
        for &sq in &graph.square_pool {
            if let Some(&t) = self.square_t.get(&sq) {
                move_to(&mut self.scene, t, self.square_pool);
            }
        }
        for &q in &graph.qube_pool {
            if let Some(&t) = self.qube_t.get(&q) {
                move_to(&mut self.scene, t, self.qube_pool);
            }
        }
        let place = |me: &mut Self, c: ContainerId, p: TransformId| {
            let t = match me.container_t.get(&c) {
                Some(&t) => t,
                None => {
                    let t = me.host_t(p);
                    me.container_t.insert(c, t);
                    t
                }
            };
            move_to(&mut me.scene, t, p);
        };
        for (&q, cs) in &mb.qube_containers {
            let Some(&qt) = self.qube_t.get(&q) else { continue };
            for &c in cs {
                place(self, c, qt);
            }
        }
        for &c in &mb.container_pool {
            place(self, c, self.container_pool);
        }
        for &c in &mb.containers_to_deconstruct {
            place(self, c, self.container_decon);
        }
    }
}

struct Builder<'a> {
    lib: &'a ModuleLibrary,
    mb: ModuleBuilder,
    bmm: BigMeshMaster,
    log: Vec<BigMeshLog>,
    props: PropSide<'a>,
}
impl WfcHost for Builder<'_> {
    fn apply_module(&mut self, graph: &mut Graph, qube: QubeId, module: OrientedModuleSides) {
        self.mb.apply_module(graph, self.lib, &mut self.bmm, &mut self.props, &mut self.log, qube, module);
    }
    fn apply_decor(&mut self, graph: &mut Graph, qube: QubeId) {
        self.mb.apply_decor(graph, self.lib, &mut self.bmm, &mut self.props, &mut self.log, qube);
    }
    fn mark_module_containers_for_deconstruction(&mut self, _graph: &mut Graph, qube: QubeId) {
        self.mb.mark_qube_containers_for_deconstruction(&mut self.bmm, &mut self.props, qube);
    }
    fn current_module(&mut self, _graph: &Graph, qube: QubeId) -> Option<OrientedModule> {
        self.mb.children(qube).first().map(|&c| self.mb.container(c).oriented_module)
    }
}

fn settle(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut Host) {
    let gr = g.graph;
    while ret::bytes(&budgeted(g, Game::call(gr, G, "Iterate", vec![], false)))[0] != 0 {}
    while ours.iterate(grid, host) {}
    while ret::bytes(&budgeted(g, Game::call(gr, G, "IterateFocus", vec![], false)))[0] != 0 {}
    while ours.iterate_focus(grid, host) {}
}

fn random_action(r: &mut Rng, ours: &Graph, verts: &[Int2], id: i32) -> Action {
    let existing: Vec<(Int2, u8, VoxelType)> = ours
        .corners
        .iter()
        .flat_map(|&c| ours.corner(c).voxels.iter().map(move |&v| (c, v)))
        .map(|(c, v)| (ours.corner(c).hex_pos, ours.voxel(v).height, ours.voxel(v).voxel_type))
        .collect();
    let kind = r.next() % 8;
    if existing.is_empty() || kind < 6 {
        Action::new(r.pick(verts), (r.next() % 5) as u8, VoxelType::Empty, VoxelType::from_i8((r.next() % 15) as i8), id)
    } else {
        let (hex, h, t) = r.pick(&existing);
        let out = if kind < 7 { VoxelType::from_i8((r.next() % 15) as i8) } else { VoxelType::Empty };
        Action::new(hex, h, t, out, id)
    }
}

/// Calls a `bool` method until it returns false, one call per request (time-budgeted loops).
/// `UniversalStopwatch.stopwatch`: `KeepGoing` is its `ElapsedMilliseconds < 8`, restarted by the scene's frame.
static STOPWATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Calls a time-budgeted `bool` method until it returns false, one call per request, each with the stopwatch
/// restarted first in the same batch (else a busy scene can leave the budget spent for every call).
fn stopwatch(g: &mut Game) -> u64 {
    let sw = STOPWATCH.load(std::sync::atomic::Ordering::Relaxed);
    if sw != 0 {
        return sw;
    }
    let h = handle(&ok(g.h.request(json!({"static_field": "Placemaker.UniversalStopwatch.stopwatch", "main": true, "keep": true})), "stopwatch"), "stopwatch");
    STOPWATCH.store(h, std::sync::atomic::Ordering::Relaxed);
    h
}

/// A time-budgeted call with the stopwatch restarted just before, in the same batch.
fn budgeted(g: &mut Game, call: Value) -> Value {
    let sw = stopwatch(g);
    g.batch(vec![Game::call(sw, "System.Diagnostics.Stopwatch", "Restart", vec![], false), call], "budgeted").remove(1)
}

fn run_one_by_one(g: &mut Game, obj: u64, class: &str, m: &str) -> usize {
    eprintln!("  {m}");
    let mut n = 0;
    while ret::bytes(&budgeted(g, Game::call(obj, class, m, vec![], false)))[0] != 0 {
        n += 1;
        assert!(n < 100_000, "{m} never ends");
    }
    n
}

/// Calls a `bool` method of the game's WFC until it returns false (in batches).
fn run_game(g: &mut Game, wfc: u64, m: &str) -> usize {
    let mut n = 0;
    loop {
        let sw = stopwatch(g);
        let reqs = (0..64).flat_map(|_| [Game::call(sw, "System.Diagnostics.Stopwatch", "Restart", vec![], false), Game::call(wfc, WFC, m, vec![], false)]).collect();
        let r = g.batch(reqs, m);
        for v in r.iter().skip(1).step_by(2) {
            if ret::bytes(v)[0] == 0 {
                return n;
            }
            n += 1;
        }
        assert!(n < 1_000_000, "{m} never ends");
    }
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn ty(g: &mut Game, n: &str) -> u64 {
    handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n)
}
fn list_items(g: &mut Game, l: u64, elem: &str) -> Vec<u64> {
    let lc = list(elem);
    let n = ret::i32(&ok(g.h.request(Game::call(l, &lc, "get_Count", vec![], false)), "Count"));
    g.batch((0..n).map(|i| Game::call(l, &lc, "get_Item", vec![arg::i32(i)], true)).collect(), "get_Item").iter().map(|r| handle(r, "item")).collect()
}
fn array_items(g: &mut Game, a: u64) -> Vec<u64> {
    let n = ret::i32(&ok(g.h.request(Game::call(a, "System.Array", "get_Length", vec![], false)), "Length"));
    g.batch(
        (0..n).map(|i| json!({"call": "GetValue", "class": "System.Array", "handle": a, "args": [arg::i32(i)], "param_types": ["System.Int32"], "main": true, "keep": true})).collect(),
        "GetValue",
    )
    .iter()
    .map(|r| handle(r, "item"))
    .collect()
}

struct Types {
    prop_node: u64,
    mesh_filter: u64,
    collider: u64,
    anchor: u64,
    wire: u64,
    window: u64,
    renderer: u64,
}

/// Every object below `roots` on both sides, breadth first, paired.
fn pairs(g: &mut Game, scene: &PropScene, roots: &[(TransformId, u64)]) -> Vec<(TransformId, u64)> {
    let mut out = vec![];
    let mut level: Vec<(TransformId, u64)> = roots.to_vec();
    while !level.is_empty() {
        let kids = g.children(&level.iter().map(|p| p.1).collect::<Vec<_>>());
        let mut next = vec![];
        for (k, &(o, _)) in level.iter().enumerate() {
            let n = scene.transforms.child_count(o);
            assert_eq!(kids[k].len(), n, "child count of {}", scene.objects[o].as_ref().map_or("a host object".into(), |p| p.name.clone()));
            for i in 0..n {
                next.push((scene.transforms.child(o, i), kids[k][i]));
            }
        }
        out.extend(next.iter().copied());
        level = next;
    }
    out
}

/// The game's meshes to ours: run-time copies by their place in the two lists, assets by name.
struct Meshes {
    by_addr: HashMap<u64, MeshId>,
    names: HashMap<String, u64>,
}

#[allow(clippy::too_many_arguments)]
fn check(g: &mut Game, side: &PropSide, placer: u64, types: &Types, prefab_addrs: &[u64], meshes: &mut Meshes, roots: &[(TransformId, u64)], what: &str) {
    let scene = &side.scene;
    let all = pairs(g, scene, roots);
    let ts: Vec<u64> = all.iter().map(|p| p.1).collect();
    let addr_of: Vec<u64> = g.addrs(&ts);
    let obj_of: HashMap<u64, TransformId> = addr_of.iter().zip(&all).map(|(&a, p)| (a, p.0)).collect();
    // the PropNode components, by address
    let props: Vec<(usize, TransformId, u64)> = all.iter().enumerate().filter(|(_, p)| scene.objects[p.0].is_some()).map(|(k, p)| (k, p.0, p.1)).collect();
    let nodes: Vec<Option<u64>> =
        g.batch(props.iter().map(|&(_, _, t)| Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": types.prop_node})], true)).collect(), "PropNode").iter().map(opt_handle).collect();
    let node_addrs = g.addrs(&nodes.iter().flatten().copied().collect::<Vec<_>>());
    let mut node_of: HashMap<u64, TransformId> = HashMap::new();
    {
        let mut it = node_addrs.iter();
        for (k, n) in nodes.iter().enumerate() {
            if n.is_some() {
                node_of.insert(*it.next().unwrap(), props[k].1);
            }
        }
    }
    // poses and flags
    let mut reqs = vec![];
    for &(_, _, t) in &props {
        reqs.push(Game::call(t, T, "get_localPosition", vec![], false));
        reqs.push(Game::call(t, T, "get_localRotation", vec![], false));
        reqs.push(Game::call(t, T, "get_localScale", vec![], false));
        reqs.push(Game::call(t, "UnityEngine.Component", "get_gameObject", vec![], true));
    }
    let r = g.batch(reqs, "poses");
    let gos: Vec<u64> = r.chunks(4).map(|c| handle(&c[3], "go")).collect();
    let active: Vec<bool> = g.batch(gos.iter().map(|&o| Game::call(o, "UnityEngine.GameObject", "get_activeSelf", vec![], false)).collect(), "activeSelf").iter().map(|v| ret::bytes(v)[0] != 0).collect();
    for (k, &(_, o, _)) in props.iter().enumerate() {
        let ob = scene.object(o);
        let w = format!("{what}: {} #{o}", ob.name);
        let lp = scene.transforms.local_position(o);
        let lq = scene.transforms.local_rotation(o);
        let ls = scene.transforms.local_scale(o);
        assert_eq!(bits(&f32s(&ret::bytes(&r[k * 4]))), bits(&[lp.x, lp.y, lp.z]), "{w}: localPosition");
        if bits(&f32s(&ret::bytes(&r[k * 4 + 1]))) != bits(&[lq.x, lq.y, lq.z, lq.w]) {
            // the chain up: each level's local and world rotation, both sides
            let (mut go_t, mut oo) = (props[k].2, Some(o));
            while let Some(oi) = oo {
                let gl = bits(&f32s(&ret::bytes(&ok(g.h.request(Game::call(go_t, T, "get_localRotation", vec![], false)), "lr"))));
                let gw = bits(&f32s(&ret::bytes(&ok(g.h.request(Game::call(go_t, T, "get_rotation", vec![], false)), "r"))));
                let (ql, qw) = (scene.transforms.local_rotation(oi), scene.transforms.rotation(oi));
                eprintln!("#{oi}: game local {gl:?} world {gw:?}
      ours  local {:?} world {:?}", bits(&[ql.x, ql.y, ql.z, ql.w]), bits(&[qw.x, qw.y, qw.z, qw.w]));
                match opt_handle(&ok(g.h.request(Game::call(go_t, T, "get_parent", vec![], true)), "parent")) {
                    Some(p) => go_t = p,
                    None => break,
                }
                oo = scene.transforms.nodes[oi].parent;
            }
        }
        assert_eq!(bits(&f32s(&ret::bytes(&r[k * 4 + 1]))), bits(&[lq.x, lq.y, lq.z, lq.w]), "{w}: localRotation");
        assert_eq!(bits(&f32s(&ret::bytes(&r[k * 4 + 2]))), bits(&[ls.x, ls.y, ls.z]), "{w}: localScale");
        assert_eq!(active[k], scene.active[o], "{w}: activeSelf");
    }
    // the nodes
    let names = ["priority", "turnedOn", "inDict", "hasBeenTurnedOn", "hideCount", "motivations", "parentCount", "childCount", "overlapCount", "moteveeCount", "anchorCount", "relations"];
    let node_list: Vec<(TransformId, u64)> = props.iter().zip(&nodes).filter_map(|(p, n)| n.map(|n| (p.1, n))).collect();
    let f = g.fields(&node_list.iter().map(|p| p.1).collect::<Vec<_>>(), &names);
    let rel_lists: Vec<Option<u64>> = f.iter().map(|v| opt_handle(&v[11])).collect();
    let rel_map: HashMap<u64, u32> = node_of.iter().map(|(&a, &o)| (a, o as u32)).collect();
    let rels = g.list_ids(&rel_lists, "Placemaker.Props.PropNode, Assembly-CSharp", &rel_map);
    for (k, &(o, _)) in node_list.iter().enumerate() {
        let n = scene.node(o);
        let w = format!("{what}: {} #{o}", scene.object(o).name);
        let v = &f[k];
        let b = |i: usize| ret::bytes(&v[i])[0];
        assert_eq!(f32s(&ret::bytes(&v[0]))[0].to_bits(), n.priority.to_bits(), "{w}: priority {}", n.priority);
        assert_eq!(
            [b(1), b(2), b(3), b(4), b(5)],
            [n.turned_on as u8, n.in_dict as u8, n.has_been_turned_on as u8, n.hide_count, n.motivations],
            "{w}: turnedOn, inDict, hasBeenTurnedOn, hideCount, motivations"
        );
        assert_eq!([b(6), b(7), b(8), b(9), b(10)], [n.parent_count, n.child_count, n.overlap_count, n.motevee_count, n.anchor_count], "{w}: counts");
        assert_eq!(rels[k], n.relations.iter().map(|&r| r as u32).collect::<Vec<_>>(), "{w}: relations");
    }
    // renderers
    let with_r: Vec<(TransformId, u64)> = props.iter().filter(|p| scene.object(p.1).renderer_enabled.is_some()).map(|p| (p.1, p.2)).collect();
    let rs: Vec<u64> = g
        .batch(with_r.iter().map(|&(_, t)| Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": types.renderer})], true)).collect(), "Renderer")
        .iter()
        .map(|r| handle(r, "renderer"))
        .collect();
    let en = g.batch(rs.iter().map(|&r| Game::call(r, "UnityEngine.Renderer", "get_enabled", vec![], false)).collect(), "enabled");
    for (k, &(o, _)) in with_r.iter().enumerate() {
        assert_eq!(Some(ret::bytes(&en[k])[0] != 0), scene.object(o).renderer_enabled, "{what}: {} #{o}: renderer enabled", scene.object(o).name);
    }
    // meshes
    let with_mf: Vec<(TransformId, u64)> = props.iter().filter(|p| side.scene.object(p.1).mesh.is_some() || prefab_node(side, p.1).has_mesh_filter).map(|p| (p.1, p.2)).collect();
    let mfs: Vec<u64> = g
        .batch(with_mf.iter().map(|&(_, t)| Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": types.mesh_filter})], true)).collect(), "MeshFilter")
        .iter()
        .map(|r| handle(r, "mf"))
        .collect();
    let ms: Vec<Option<u64>> = g.batch(mfs.iter().map(|&m| Game::call(m, "UnityEngine.MeshFilter", "get_sharedMesh", vec![], true)).collect(), "sharedMesh").iter().map(opt_handle).collect();
    let mesh_addrs = g.addrs(&ms.iter().flatten().copied().collect::<Vec<_>>());
    let mut it = mesh_addrs.iter();
    for (k, &(o, _)) in with_mf.iter().enumerate() {
        let w = format!("{what}: {} #{o}", scene.object(o).name);
        let got = match ms[k] {
            None => None,
            Some(mh) => {
                let a = *it.next().unwrap();
                Some(match meshes.by_addr.get(&a) {
                    Some(&m) => m,
                    None if matches!(scene.object(o).mesh, Some(MeshId::World(_))) => {
                        // a mesh built at run time: its contents
                        let Some(MeshId::World(id)) = scene.object(o).mesh else { unreachable!() };
                        let gm = &scene.generated[&id];
                        let lst = |g: &mut Game, t: &str| handle(&ok(g.h.request(json!({"new": list(t), "args": [], "main": true})), "list"), "list");
                        let (lv, ln, lu, lt) = (lst(g, V3T), lst(g, V3T), lst(g, V2T), lst(g, "System.Int32"));
                        let mc = "UnityEngine.Mesh";
                        ok(g.h.request(json!({"call": "GetVertices", "class": mc, "handle": mh, "args": [{"handle": lv}], "param_types": ["System.Collections.Generic.List<UnityEngine.Vector3>"], "main": true})), "GetVertices");
                        ok(g.h.request(json!({"call": "GetNormals", "class": mc, "handle": mh, "args": [{"handle": ln}], "param_types": ["System.Collections.Generic.List<UnityEngine.Vector3>"], "main": true})), "GetNormals");
                        ok(g.h.request(json!({"call": "GetUVs", "class": mc, "handle": mh, "args": [arg::i32(0), {"handle": lu}], "param_types": ["System.Int32", "System.Collections.Generic.List<UnityEngine.Vector2>"], "main": true})), "GetUVs");
                        ok(g.h.request(json!({"call": "GetTriangles", "class": mc, "handle": mh, "args": [{"handle": lt}, arg::i32(0)], "param_types": ["System.Collections.Generic.List<System.Int32>", "System.Int32"], "main": true})), "GetTriangles");
                        let arr = |g: &mut Game, l: u64, t: &str| ret::bytes(&ok(g.h.request(Game::call(l, &list(t), "ToArray", vec![], false)), "ToArray"));
                        let fl = |v: &[Vector3]| v.iter().flat_map(|p| [p.x, p.y, p.z]).collect::<Vec<f32>>();
                        assert_eq!(bits(&f32s(&arr(g, lv, V3T))), bits(&fl(&gm.verts)), "{w}: generated vertices");
                        assert_eq!(bits(&f32s(&arr(g, ln, V3T))), bits(&fl(&gm.normals)), "{w}: generated normals");
                        let uv: Vec<f32> = gm.uvs.iter().flat_map(|p| [p.x, p.y]).collect();
                        assert_eq!(bits(&f32s(&arr(g, lu, V2T))), bits(&uv), "{w}: generated uvs");
                        let tris: Vec<i32> = arr(g, lt, "System.Int32").chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
                        assert_eq!(tris, gm.tris, "{w}: generated triangles");
                        meshes.by_addr.insert(a, MeshId::World(id));
                        MeshId::World(id)
                    }
                    None => {
                        let nm = ret::string(&ok(g.h.request(Game::call(mh, "UnityEngine.Object", "get_name", vec![], false)), "name"));
                        let id = *meshes.names.get(&nm).unwrap_or_else(|| panic!("{w}: mesh {nm} unknown"));
                        meshes.by_addr.insert(a, MeshId::Asset(id));
                        MeshId::Asset(id)
                    }
                })
            }
        };
        assert_eq!(got, scene.object(o).mesh, "{w}: mesh");
    }
    // colliders
    let with_c: Vec<(TransformId, u64)> = props.iter().enumerate().filter(|(_, p)| !scene.object(p.1).colliders.is_empty()).map(|(k, p)| (p.1, gos[k])).collect();
    let arrs: Vec<u64> = g
        .batch(
            with_c
                .iter()
                .map(|&(_, t)| json!({"call": "GetComponents", "class": "UnityEngine.GameObject", "handle": t, "args": [{"handle": types.collider}], "param_types": ["System.Type"], "main": true, "keep": true}))
                .collect(),
            "GetComponents",
        )
        .iter()
        .map(|r| handle(r, "colliders"))
        .collect();
    let mut collider_of: HashMap<u64, (TransformId, usize)> = HashMap::new();
    for (k, &(o, _)) in with_c.iter().enumerate() {
        let cs = array_items(g, arrs[k]);
        let oc = &scene.object(o).colliders;
        assert_eq!(cs.len(), oc.len(), "{what}: #{o} colliders");
        let wb = g.batch(cs.iter().map(|&c| Game::field(c, "worldBounds")).collect(), "worldBounds");
        let ca = g.addrs(&cs);
        for (i, c) in oc.iter().enumerate() {
            let b = c.world_bounds;
            if bits(&f32s(&ret::bytes(&wb[i]))) != bits(&[b.center.x, b.center.y, b.center.z, b.extents.x, b.extents.y, b.extents.z]) {
                let gm = bits(&f32s(&ret::bytes(&ok(g.h.request(Game::call(cs[i], "Placemaker.Props.PropCollider", "GetMatrix", vec![], false)), "GetMatrix"))));
                let om = c.get_matrix(&scene.transforms, o);
                eprintln!("GetMatrix game {gm:?}
          ours {:?}", bits(&om.m));
                let ct = handle(&ok(g.h.request(Game::call(cs[i], "UnityEngine.Component", "get_transform", vec![], true)), "t"), "t");
                let gl = bits(&f32s(&ret::bytes(&ok(g.h.request(Game::call(ct, T, "get_localToWorldMatrix", vec![], false)), "l2w"))));
                eprintln!("l2w game {gl:?}
    ours {:?}", bits(&scene.transforms.local_to_world_matrix(o).m));
                ok(g.h.request(Game::call(cs[i], "Placemaker.Props.PropCollider", "CalculateWorldBounds", vec![], false)), "CalculateWorldBounds");
                let wb2 = bits(&f32s(&ret::bytes(&ok(g.h.request(Game::field(cs[i], "worldBounds")), "wb"))));
                let mut c2 = *c;
                c2.calculate_world_bounds(&scene.transforms, o);
                let b2 = c2.world_bounds;
                eprintln!("recalculated game {wb2:?}
             ours {:?}", bits(&[b2.center.x, b2.center.y, b2.center.z, b2.extents.x, b2.extents.y, b2.extents.z]));
            }
            assert_eq!(
                bits(&f32s(&ret::bytes(&wb[i]))),
                bits(&[b.center.x, b.center.y, b.center.z, b.extents.x, b.extents.y, b.extents.z]),
                "{what}: {} #{o} collider {i} worldBounds",
                scene.object(o).name
            );
            collider_of.insert(ca[i], (o, i));
        }
    }
    // anchors and wires
    for &(_, o, t) in &props {
        let ob = scene.object(o);
        if let Some(a) = &ob.anchor {
            let c = handle(&ok(g.h.request(Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": types.anchor})], true)), "anchor"), "anchor");
            let vt = ret::bytes(&ok(g.h.request(Game::field(c, "voxelType")), "voxelType"))[0];
            assert_eq!(vt, a.voxel_type, "{what}: {} #{o} voxelType", ob.name);
            let l = handle(&ok(g.h.request(Game::field(c, "propWires")), "propWires"), "propWires");
            let ws = list_items(g, l, "Placemaker.Props.PropWire, Assembly-CSharp");
            let wt: Vec<u64> = g.batch(ws.iter().map(|&w| Game::call(w, "UnityEngine.Component", "get_transform", vec![], true)).collect(), "t").iter().map(|r| handle(r, "t")).collect();
            let got: Vec<TransformId> = g.addrs(&wt).iter().map(|a| obj_of[a]).collect();
            assert_eq!(got, a.prop_wires, "{what}: {} #{o} propWires", ob.name);
        }
        if prefab_node(side, o).wire {
            let c = handle(&ok(g.h.request(Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": types.wire})], true)), "wire"), "wire");
            let mut got = vec![];
            for f in ["anchor0", "anchor1"] {
                let a = opt_handle(&ok(g.h.request(Game::field(c, f)), f));
                got.push(a.map(|a| {
                    let at = handle(&ok(g.h.request(Game::call(a, "UnityEngine.Component", "get_transform", vec![], true)), "t"), "t");
                    obj_of[&g.addrs(&[at])[0]]
                }));
            }
            assert_eq!((got[0], got[1]), ob.wire, "{what}: {} #{o} anchors", ob.name);
        }
        if prefab_node(side, o).modifiers.iter().any(|m| matches!(m, ts_game::placemaker::prop_placer::Modifier::Window)) {
            let c = handle(&ok(g.h.request(Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": types.window})], true)), "window"), "window");
            let mut got = vec![];
            for f in ["mf0", "mf1"] {
                let a = opt_handle(&ok(g.h.request(Game::field(c, f)), f));
                got.push(a.map(|a| {
                    let at = handle(&ok(g.h.request(Game::call(a, "UnityEngine.Component", "get_transform", vec![], true)), "t"), "t");
                    obj_of[&g.addrs(&[at])[0]]
                }));
            }
            assert_eq!((got[0], got[1]), ob.window, "{what}: {} #{o} window mf0, mf1", ob.name);
        }
    }
    // srcProps
    let sp = handle(&ok(g.h.request(Game::field(placer, "srcProps")), "srcProps"), "srcProps");
    let items = list_items(g, sp, "Placemaker.Prop, Assembly-CSharp");
    let got: Vec<u64> = g.addrs(&items);
    let want: Vec<u64> = side.placer.src_props.iter().map(|&p| prefab_addrs[p]).collect();
    assert_eq!(got, want, "{what}: srcProps");
    // the collider dictionary
    let dict_class = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int3, Unity.Mathematics],[System.Collections.Generic.List`1[[Placemaker.Props.PropCollider, Assembly-CSharp]], mscorlib]]";
    let d = handle(&ok(g.h.request(Game::field(placer, "colliderDict")), "colliderDict"), "colliderDict");
    let count = ret::i32(&ok(g.h.request(Game::call(d, dict_class, "get_Count", vec![], false)), "dict Count"));
    assert_eq!(count as usize, side.placer.collider_dict.len(), "{what}: colliderDict chunks");
    let mut keys: Vec<&Int3> = side.placer.collider_dict.keys().collect();
    keys.sort_by_key(|k| (k.x, k.y, k.z));
    let lists: Vec<u64> = g
        .batch(keys.iter().map(|k| Game::call(d, dict_class, "get_Item", vec![arg::raw(&bytes(*k))], true)).collect(), "dict get_Item")
        .iter()
        .map(|r| handle(r, "list"))
        .collect();
    for (k, key) in keys.iter().enumerate() {
        let items = list_items(g, lists[k], "Placemaker.Props.PropCollider, Assembly-CSharp");
        let got: Vec<(TransformId, usize)> = g.addrs(&items).iter().map(|a| *collider_of.get(a).unwrap_or_else(|| panic!("{what}: chunk {key:?}: a collider of no object compared"))).collect();
        assert_eq!(got, side.placer.collider_dict[*key], "{what}: chunk {key:?}");
    }
}

fn prefab_node<'a>(side: &'a PropSide, o: TransformId) -> &'a ts_game::placemaker::prop_placer::PrefabNode {
    let (p, n) = side.scene.object(o).from.unwrap();
    &side.prefabs[p].nodes[n]
}

#[test]
fn placing_props_matches_the_game() {
    let props = prop_common::load();
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut lib = module_common::our_module_library();
    lib.setup();
    let prefab_handles = prop_common::game_prefabs(g.h, &props);
    let prefab_addrs: Vec<u64> = g.addrs(&prefab_handles);
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "master"), "master");
    let wfc = handle(&ok(g.h.request(Game::field(master, "waveFunctionCollapse")), "wfc"), "wfc");
    let scene_master = handle(&ok(g.h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "scene master"), "scene master");
    let set = |g: &mut Game, obj: u64, f: &str, v: u64| {
        ok(g.h.request(json!({"field": f, "handle": obj, "set_handle": v, "main": true})), f);
    };
    let new_obj = |g: &mut Game, class: &str| handle(&ok(g.h.request(json!({"new": class, "args": [], "main": true})), class), class);
    let new_go = |g: &mut Game, name: &str, active: bool| -> (u64, u64) {
        let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true, "keep": true})), "go"), "go");
        if !active {
            ok(g.h.request(Game::call(go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false)), "SetActive");
        }
        let t = handle(&ok(g.h.request(Game::call(go, "UnityEngine.GameObject", "get_transform", vec![], true)), "t"), "t");
        (go, t)
    };
    let set_byte = |g: &mut Game, obj: u64, f: &str, v: u8| {
        ok(g.h.request(json!({"field": f, "handle": obj, "set_raw": format!("{v:02x}"), "main": true})), f);
    };
    // the scene's sounds; fresh flocks; a fresh mesh animator whose own master is dimmed
    let sounds = handle(&ok(g.h.request(Game::field(scene_master, "manySoundsManager")), "sounds"), "sounds");
    set(&mut g, master, "manySoundsManager", sounds);
    for (f, class) in [("flock", "Placemaker.Life.BirdFlock"), ("butterflyFlock", "Placemaker.Life.ButterflyFlock")] {
        let o = new_obj(&mut g, class);
        set(&mut g, o, "master", master);
        set(&mut g, master, f, o);
    }
    // a fresh big mesh master (as placemaker_wfc.rs: one group per scene group)
    let bmm = new_obj(&mut g, "Placemaker.BigMeshMaster");
    set(&mut g, bmm, "master", master);
    let scene_bmm = handle(&ok(g.h.request(Game::field(scene_master, "bigMeshMaster")), "bigMeshMaster"), "bigMeshMaster");
    let group_type = ty(&mut g, "Placemaker.BigMeshGroup, Assembly-CSharp");
    let gl = list("Placemaker.BigMeshGroup, Assembly-CSharp");
    let new_groups = handle(&ok(g.h.request(Game::field(bmm, "bigMeshGroups")), "bigMeshGroups"), "bigMeshGroups");
    let mut ours_bmm = BigMeshMaster::default();
    let sgl = handle(&ok(g.h.request(Game::field(scene_bmm, "bigMeshGroups")), "groups"), "groups");
    for sg in list_items(&mut g, sgl, "Placemaker.BigMeshGroup, Assembly-CSharp") {
        let (go, gt) = new_go(&mut g, "ts_check big mesh group", true);
        let grp = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": group_type}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "group");
        let mat = handle(&ok(g.h.request(Game::field(sg, "material")), "material"), "material");
        set(&mut g, grp, "material", mat);
        let st = handle(&ok(g.h.request(Game::call(sg, "UnityEngine.Component", "get_transform", vec![], true)), "t"), "t");
        let template = handle(&ok(g.h.request(Game::call(st, T, "GetChild", vec![arg::i32(0)], true)), "child"), "child");
        let copy = handle(&ok(g.h.request(json!({"call": "UnityEngine.Object.Instantiate", "args": [{"handle": template}], "param_types": ["UnityEngine.Object"], "main": true, "keep": true})), "Instantiate"), "copy");
        ok(g.h.request(json!({"call": "SetParent", "class": T, "handle": copy, "args": [{"handle": gt}, arg::bool(false)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true})), "SetParent");
        ok(g.h.request(Game::call(new_groups, &gl, "Add", vec![json!({"handle": grp})], false)), "Add group");
        let a = g.addrs(&[mat])[0];
        ours_bmm.big_mesh_groups.push(ts_game::placemaker::big_mesh::BigMeshGroup { material: ts_game::placemaker::modules::AssetRef { file_id: -1, path_id: a as i64 }, ..Default::default() });
    }
    set(&mut g, master, "bigMeshMaster", bmm);
    let anim_master = new_obj(&mut g, "Placemaker.WorldMaster");
    set_byte(&mut g, anim_master, "isDimmed", 1);
    set(&mut g, anim_master, "bigMeshMaster", bmm);
    let anim = new_obj(&mut g, "Placemaker.Props.PropMeshAnimator");
    set(&mut g, anim, "master", anim_master);
    set(&mut g, master, "propMeshAnimator", anim);
    let fence = {
        let mm = handle(&ok(g.h.request(Game::field(scene_master, "materialMaster")), "materialMaster"), "materialMaster");
        let f = handle(&ok(g.h.request(Game::field(mm, "fenceMaterial")), "fenceMaterial"), "fenceMaterial");
        let scene_lib = handle(&ok(g.h.request(Game::field(scene_master, "moduleLibrary")), "moduleLibrary"), "moduleLibrary");
        let ml = handle(&ok(g.h.request(Game::field(scene_lib, "moduleMeshes")), "moduleMeshes"), "moduleMeshes");
        let meshes = list_items(&mut g, ml, "Placemaker.ModuleMesh, Assembly-CSharp");
        let mats: Vec<u64> = g.batch(meshes.iter().map(|&m| Game::field(m, "material")).collect(), "material").iter().map(|r| handle(r, "material")).collect();
        let fa = g.addrs(&[f])[0];
        let k = g.addrs(&mats).iter().position(|&a| a == fa).expect("the fence material is a module mesh material");
        lib.module_meshes[k].material
    };
    // our big mesh groups by the module meshes' materials (as placemaker_wfc.rs)
    {
        let scene_lib = handle(&ok(g.h.request(Game::field(scene_master, "moduleLibrary")), "moduleLibrary"), "moduleLibrary");
        let ml = handle(&ok(g.h.request(Game::field(scene_lib, "moduleMeshes")), "moduleMeshes"), "moduleMeshes");
        let meshes = list_items(&mut g, ml, "Placemaker.ModuleMesh, Assembly-CSharp");
        let mats: Vec<u64> = g.batch(meshes.iter().map(|&m| Game::field(m, "material")).collect(), "material").iter().map(|r| handle(r, "material")).collect();
        let mat_of: HashMap<u64, ts_game::placemaker::modules::AssetRef> = g.addrs(&mats).into_iter().zip(&lib.module_meshes).map(|(a, m)| (a, m.material)).collect();
        for grp in &mut ours_bmm.big_mesh_groups {
            if let Some(&m) = mat_of.get(&(grp.material.path_id as u64)) {
                grp.material = m;
            }
        }
    }
    // the module builder's two containers
    let mb = handle(&ok(g.h.request(Game::field(master, "moduleBuilder")), "moduleBuilder"), "moduleBuilder");
    let mut mb_ts = vec![];
    for f in ["moduleContainerPool", "moduleContainersToDeconstruct"] {
        let (_, t) = new_go(&mut g, f, false);
        set(&mut g, mb, f, t);
        mb_ts.push(t);
    }
    // the fresh prop placer on an inactive object, its containers
    let (pgo, _) = new_go(&mut g, "ts_check prop placer", false);
    let pt = ty(&mut g, "Placemaker.PropPlacer, Assembly-CSharp");
    let placer = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": pgo, "args": [{"handle": pt}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "placer");
    set(&mut g, placer, "master", master);
    let mut placer_ts = vec![];
    for (f, active) in [("propPoolContainer", false), ("secondaryPropsContainer", true), ("deconstructionContainer", false)] {
        let (_, t) = new_go(&mut g, &format!("ts_check {f}"), active);
        set(&mut g, placer, f, t);
        placer_ts.push((t, active));
    }
    if opt_handle(&ok(g.h.request(Game::field(placer, "materialMeshes")), "materialMeshes")).is_none() {
        let l = handle(&ok(g.h.request(json!({"new": list("UnityEngine.Mesh, UnityEngine.CoreModule"), "args": [], "main": true})), "list"), "list");
        set(&mut g, placer, "materialMeshes", l);
    }
    // (`OnEnable` would walk the stand-in master's children, which a `new` master has none of: its pools, an empty
    // dictionary, directly; nothing is in the collider dictionary yet)
    let pools = handle(
        &ok(g.h.request(json!({"new": "System.Collections.Generic.Dictionary`2[[Placemaker.Prop, Assembly-CSharp],[UnityEngine.Transform, UnityEngine.CoreModule]]", "args": [], "main": true})), "new pools"),
        "pools",
    );
    set(&mut g, placer, "propPools", pools);
    set(&mut g, master, "propPlacer", placer);
    // the materials by name
    let mm = handle(&ok(g.h.request(Game::field(scene_master, "materialMaster")), "materialMaster"), "materialMaster");
    let mat_id = |g: &mut Game, f: &str| {
        let m = handle(&ok(g.h.request(Game::field(mm, f)), f), f);
        let n = ret::string(&ok(g.h.request(Game::call(m, "UnityEngine.Object", "get_name", vec![], false)), "name"));
        let ids: Vec<u64> = props.material_names.iter().filter(|(_, x)| **x == n).map(|(&i, _)| i).collect();
        assert_eq!(ids.len(), 1, "one material named {n}");
        ids[0]
    };
    let door = mat_id(&mut g, "doorMaterial");
    let hole = mat_id(&mut g, "hole0");
    // the meshes: assets by (unique) name; Window.materialMeshes as the scene left it
    let mut names: HashMap<String, u64> = HashMap::new();
    let mut dup = std::collections::HashSet::new();
    for (&id, n) in &props.mesh_names {
        if names.insert(n.clone(), id).is_some() {
            dup.insert(n.clone());
        }
    }
    for d in &dup {
        names.remove(d);
    }
    let mut meshes = Meshes { by_addr: HashMap::new(), names };
    let mut scene = PropScene::default();
    {
        let wl = handle(&ok(g.h.request(json!({"static_field": "Placemaker.Props.Window.materialMeshes", "main": true, "keep": true})), "Window.materialMeshes"), "list");
        let wm = list_items(&mut g, wl, "UnityEngine.Mesh, UnityEngine.CoreModule");
        let wa = g.addrs(&wm);
        assert_eq!(wm.len() % 60, 0, "Window.materialMeshes in groups of 60");
        for (f, chunk) in wm.chunks(60).enumerate() {
            let nm = ret::string(&ok(g.h.request(Game::call(chunk[0], "UnityEngine.Object", "get_name", vec![], false)), "name"));
            let src = MeshId::Asset(*meshes.names.get(&nm).unwrap_or_else(|| panic!("window mesh {nm}")));
            scene.window_meshes.push(src);
            meshes.by_addr.insert(wa[f * 60], src);
            let mut k = 1;
            for group in 0..15 {
                for row in (if group == 0 { 1 } else { 0 })..4 {
                    scene.mesh_clones.push(MeshClone::Window { src, group, row });
                    let id = MeshId::Clone(scene.mesh_clones.len() as u32 - 1);
                    scene.window_meshes.push(id);
                    meshes.by_addr.insert(wa[f * 60 + k], id);
                    k += 1;
                }
            }
        }
        eprintln!("Window.materialMeshes: {} groups", wm.len() / 60);
    }
    // ours
    let root = |s: &mut PropScene, active: bool| s.add_host(Node::default(), active);
    let squares_root = root(&mut scene, true);
    let square_pool = root(&mut scene, true);
    let qube_pool = root(&mut scene, true);
    let container_pool = root(&mut scene, false);
    let container_decon = root(&mut scene, false);
    let pool_c = root(&mut scene, false);
    let secondary = root(&mut scene, true);
    let decon = root(&mut scene, false);
    let anim_root = root(&mut scene, true);
    let mf_pool = root(&mut scene, false);
    // the Italian wire meshes, by name
    let italian: Vec<u64> = {
        let misc = handle(&ok(g.h.request(Game::field(scene_master, "miscReferences")), "miscReferences"), "misc");
        let arr = handle(&ok(g.h.request(json!({"field": "italianMeshes", "handle": misc, "main": true})), "italianMeshes"), "italian");
        array_items(&mut g, arr)
            .into_iter()
            .map(|m| {
                let n = ret::string(&ok(g.h.request(Game::call(m, "UnityEngine.Object", "get_name", vec![], false)), "name"));
                *meshes.names.get(&n).unwrap_or_else(|| panic!("Italian mesh {n}"))
            })
            .collect()
    };
    let mut placer_ours = PropPlacer {
        src_props: vec![],
        prop_pools: HashMap::new(),
        prop_pool_container: pool_c,
        secondary_props_container: secondary,
        deconstruction_container: decon,
        door_material: Some(door),
        hole_material: Some(hole),
        collider_dict: HashMap::new(),
        material_meshes: vec![],
        prefab_of: prop_common::prefab_of(&lib, &props),
        italian_meshes: italian,
    };
    placer_ours.on_enable(&mut scene, &props.prefabs, &[]);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = Host { ao, ..Default::default() };
    let side = PropSide {
        prefabs: &props.prefabs,
        placer: placer_ours,
        scene,
        calls: Calls { animator: Some(PropMeshAnimator { root: anim_root, pool: MeshFilterPool { root: mf_pool } }), ..Calls::default() },
        squares_root,
        square_pool,
        qube_pool,
        container_pool,
        container_decon,
        square_t: HashMap::new(),
        qube_t: HashMap::new(),
        container_t: HashMap::new(),
        to_place: vec![],
    };
    let mut b = Builder { lib: &lib, mb: ModuleBuilder { fence_material: fence, ..Default::default() }, bmm: ours_bmm, log: vec![], props: side };
    let types = Types {
        prop_node: ty(&mut g, "Placemaker.Props.PropNode, Assembly-CSharp"),
        mesh_filter: ty(&mut g, "UnityEngine.MeshFilter, UnityEngine.CoreModule"),
        collider: ty(&mut g, "Placemaker.Props.PropCollider, Assembly-CSharp"),
        anchor: ty(&mut g, "Placemaker.Props.PropWireAnchor, Assembly-CSharp"),
        wire: ty(&mut g, "Placemaker.Props.PropWire, Assembly-CSharp"),
        window: ty(&mut g, "Placemaker.Props.Window, Assembly-CSharp"),
        renderer: ty(&mut g, "UnityEngine.Renderer, UnityEngine.CoreModule"),
    };
    let gr = g.graph;
    while ret::bytes(&budgeted(&mut g, Game::call(gr, G, "IterateRefillPools", vec![], false)))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    settle(&mut g, &mut grid, &mut ours, &mut host);
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    // TS_PROP_SEED / TS_PROP_SPREAD pick another town (the default one meets no wires)
    let seed: u32 = std::env::var("TS_PROP_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(71);
    let spread: f32 = std::env::var("TS_PROP_SPREAD").ok().and_then(|s| s.parse().ok()).unwrap_or(2.5);
    let mut r = Rng(seed);
    let mut verts: Vec<Int2> = Vec::new();
    for _ in 0..150 {
        let q = grid.get_quad(f2(r.f(cx - spread, cx + spread), r.f(cz - spread, cz + spread)));
        if q.is_valid() {
            for v in [q.v0, q.v1, q.v2, q.v3] {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
    }
    let steps: i32 = std::env::var("TS_PROP_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(12);
    let mut placed_total = 0;
    for step in 0..steps {
        let t0 = std::time::Instant::now();
        let a = random_action(&mut r, &ours, &verts, step);
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
        for q in host.cleared.drain(..) {
            b.mb.mark_qube_containers_for_deconstruction(&mut b.bmm, &mut b.props, q);
        }
        ok(g.h.request(Game::call(wfc, WFC, "IteratePropagationCount", vec![], false)), "IteratePropagationCount");
        host.wfc.iterate_propagation_count();
        eprintln!("step {step}: settled");
        let n0 = run_game(&mut g, wfc, "Iterate0");
        let mut k = 0;
        while host.wfc.iterate0(&mut ours, &lib, &mut b) {
            k += 1;
        }
        assert_eq!(n0, k, "step {step}: Iterate0 rounds");
        run_one_by_one(&mut g, wfc, WFC, "IterateApplyModules");
        while host.wfc.iterate_apply_modules(&mut ours, &mut b) {}
        let n1 = run_game(&mut g, wfc, "Iterate1");
        let mut k = 0;
        while host.wfc.iterate1(&mut ours, &lib, &mut b) {
            k += 1;
        }
        assert_eq!(n1, k, "step {step}: Iterate1 rounds");
        let nd = run_game(&mut g, wfc, "IterateDecor");
        let mut k = 0;
        while host.wfc.iterate_decor(&mut ours, &lib, &mut b) {
            k += 1;
        }
        assert_eq!(nd, k, "step {step}: IterateDecor rounds");
        if step % 3 == 0 {
            ok(g.h.request(Game::call(wfc, WFC, "LaterUpdate", vec![], false)), "LaterUpdate");
            host.wfc.later_update(&mut ours, &lib, &mut b);
        }
        // the world master's order: containers and props taken down, then the grass queue
        // (the game's loops run on `UniversalStopwatch.KeepGoing`, the scene's frame budget, which can stay spent
        // for the stand-in: their bodies are called directly, in the loops' order)
        let mc_type = ty(&mut g, "Placemaker.Graphs.ModuleContainer, Assembly-CSharp");
        let decon_kids = g.children(&[mb_ts[1]]).remove(0);
        for &c in decon_kids.iter().rev() {
            let mc = handle(&ok(g.h.request(Game::call(c, "UnityEngine.Component", "GetComponent", vec![json!({"handle": mc_type})], true)), "ModuleContainer"), "mc");
            ok(g.h.request(Game::call(mb, "Placemaker.ModuleBuilder", "DeconstructModuleContainer", vec![json!({"handle": mc})], false)), "DeconstructModuleContainer");
        }
        while b.mb.iterate_deconstruct_qubes(&mut b.props) {}
        let prop_type = ty(&mut g, "Placemaker.Prop, Assembly-CSharp");
        let decon_props = g.children(&[placer_ts[2].0]).remove(0);
        for &c in decon_props.iter().rev() {
            let p = handle(&ok(g.h.request(Game::call(c, "UnityEngine.Component", "GetComponent", vec![json!({"handle": prop_type})], true)), "Prop"), "prop");
            ok(g.h.request(Game::call(placer, PP, "ReturnProp", vec![json!({"handle": p})], false)), "ReturnProp");
        }
        while b.props.placer.iterate_deconstruct_props(&mut b.props.scene, b.props.prefabs, &mut b.props.calls) {}
        b.props.sync(&ours, &b.mb);
        // the squares' and qubes' local rotations as the game holds them: (-0, -0, -0, 1) or (0, 0, 0, 1) by their
        // history (pooled objects re-parented keeping their pose; the graph's transforms are not ours here)
        {
            let sq_t = g.containers["squares"];
            let sqs = g.children(&[sq_t]).remove(0);
            assert_eq!(sqs.len(), ours.squares.len(), "step {step}: squares");
            let qks = g.children(&sqs);
            let mut pairs: Vec<(TransformId, u64)> = vec![];
            for (k, &s) in ours.squares.iter().enumerate() {
                pairs.push((b.props.square_t[&s], sqs[k]));
                for (j, &q) in ours.square(s).qubes.iter().enumerate() {
                    pairs.push((b.props.qube_t[&q], qks[k][j]));
                }
            }
            let rs = g.batch(pairs.iter().map(|&(_, t)| Game::call(t, T, "get_localRotation", vec![], false)).collect(), "localRotation");
            for (k, &(o, _)) in pairs.iter().enumerate() {
                let q = u32s(&ret::bytes(&rs[k]));
                b.props.scene.transforms.nodes[o].r = ts_game::unity::sse::X([q[0], q[1], q[2], q[3]]);
            }
        }
        {
            let gl = handle(&ok(g.h.request(Game::field(mb, "qubesToGrass")), "qubesToGrass"), "qubesToGrass");
            let gq = list_items(&mut g, gl, "Placemaker.Graphs.Qube, Assembly-CSharp");
            assert_eq!(gq.len(), b.mb.qubes_to_grass.len(), "step {step}: qubesToGrass");
            for k in (0..gq.len()).rev() {
                let aw = ret::bytes(&ok(g.h.request(Game::field(gq[k], "awaitingGrass")), "awaitingGrass"))[0] != 0;
                assert_eq!(aw, ours.qube(b.mb.qubes_to_grass[k]).awaiting_grass, "step {step}: qubesToGrass[{k}] awaitingGrass");
                if aw {
                    ok(g.h.request(Game::call(placer, PP, "PlacePropsInQube", vec![json!({"handle": gq[k]})], false)), "PlacePropsInQube");
                    ok(g.h.request(json!({"field": "awaitingGrass", "handle": gq[k], "set_raw": "00", "main": true})), "awaitingGrass");
                }
            }
            ok(g.h.request(Game::call(gl, &list("Placemaker.Graphs.Qube, Assembly-CSharp"), "Clear", vec![], false)), "Clear");
        }
        while b.mb.iterate(&mut ours, &mut b.props) {}
        let to_place: Vec<QubeId> = b.props.to_place.drain(..).collect();
        placed_total += to_place.len();
        {
            let side = &mut b.props;
            let ct = side.container_t.clone();
            let graph = &ours;
            let corner = |c| graph.corner(c).local_position;
            let cont = |c: ContainerId| ct[&c];
            let mut ctx = PlaceContext { graph, lib: &lib, mb: &mut b.mb, container_transform: &cont, corner_position: &corner };
            for q in to_place {
                side.placer.place_props_in_qube(&mut side.scene, side.prefabs, &mut ctx, &mut side.calls, q);
            }
        }
        ok(g.h.request(Game::call(bmm, "Placemaker.BigMeshMaster", "OnLateUpdate", vec![], false)), "OnLateUpdate");
        b.bmm.on_late_update(&mut b.log);
        // everything that hangs from the containers, the wires, the pools and the deconstruction container
        eprintln!("step {step}: placed, checking");
        let qs = game_qubes(&mut g, &ours);
        let mut roots = vec![];
        let qube_ts: Vec<u64> = g.batch(qs.iter().map(|&(_, q)| Game::call(q, "UnityEngine.Component", "get_transform", vec![], true)).collect(), "t").iter().map(|r| handle(r, "t")).collect();
        let kids = g.children(&qube_ts);
        for (k, &(q, _)) in qs.iter().enumerate() {
            let cs = b.mb.children(q);
            assert_eq!(kids[k].len(), cs.len(), "step {step}: qube {q:?} containers");
            for (i, &c) in cs.iter().enumerate() {
                roots.push((b.props.container_t[&c], kids[k][i]));
            }
        }
        for (k, &(t, _)) in placer_ts.iter().enumerate() {
            roots.push(([pool_c, secondary, decon][k], t));
        }
        let gm = handle(&ok(g.h.request(Game::field(placer, "materialMeshes")), "materialMeshes"), "materialMeshes");
        let gm = list_items(&mut g, gm, "UnityEngine.Mesh, UnityEngine.CoreModule");
        assert_eq!(gm.len(), b.props.placer.material_meshes.len(), "step {step}: materialMeshes");
        for (a, &m) in g.addrs(&gm).into_iter().zip(&b.props.placer.material_meshes) {
            let prev = meshes.by_addr.insert(a, m);
            assert!(prev.is_none() || prev == Some(m), "step {step}: materialMeshes entry {m:?}");
        }
        {
            let wl = handle(&ok(g.h.request(json!({"static_field": "Placemaker.Props.Window.materialMeshes", "main": true, "keep": true})), "Window.materialMeshes"), "list");
            let wm = list_items(&mut g, wl, "UnityEngine.Mesh, UnityEngine.CoreModule");
            assert_eq!(wm.len(), b.props.scene.window_meshes.len(), "step {step}: Window.materialMeshes");
            for (a, &m) in g.addrs(&wm).into_iter().zip(&b.props.scene.window_meshes) {
                let prev = meshes.by_addr.insert(a, m);
                assert!(prev.is_none() || prev == Some(m), "step {step}: Window.materialMeshes entry {m:?}");
            }
        }
        check(&mut g, &b.props, placer, &types, &prefab_addrs, &mut meshes, &roots, &format!("step {step}"));
        eprintln!(
            "step {step}: {:.1} s, same ({} qubes placed so far, {} objects, {} chunks, {} pools, {} enables, {} wires, errors {:?})",
            t0.elapsed().as_secs_f32(),
            placed_total,
            b.props.scene.objects.iter().filter(|o| o.is_some()).count(),
            b.props.placer.collider_dict.len(),
            b.props.placer.src_props.len(),
            b.props.calls.enables,
            b.props.calls.wires,
            b.props.calls.errors
        );
    }
}

/// The game's qubes in squares (our order), with our ids.
fn game_qubes(g: &mut Game, ours: &Graph) -> Vec<(QubeId, u64)> {
    let sq_t = g.containers["squares"];
    let kids = g.children(&[sq_t]);
    assert_eq!(kids[0].len(), ours.squares.len(), "squares");
    let qube_kids = g.children(&kids[0]);
    let mut out = vec![];
    let mut qube_ts = vec![];
    for (k, &s) in ours.squares.iter().enumerate() {
        assert_eq!(qube_kids[k].len(), ours.square(s).qubes.len(), "qubes of square {s:?}");
        qube_ts.extend(qube_kids[k].iter().copied());
        out.extend(ours.square(s).qubes.iter().copied());
    }
    let hs = g.components(&qube_ts, "Placemaker.Graphs.Qube");
    out.into_iter().zip(hs).collect()
}

#[allow(dead_code)]
fn unused(_: Value) {}
