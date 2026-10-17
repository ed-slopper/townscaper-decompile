//! `placemaker::wfc` against the original game (E6a). Needs `TS_HARNESS=1` and the player's install.
//! On the fresh game graph and ours (`graph_common`), with the stand-in's fresh `WaveFunctionCollapse` on the game
//! side and ours behind the graph's hooks: random actions, each followed by the graph's update; the queues the
//! hooks fill (`updates`, `removeNeighbors`) and the qubes' steps are compared. Then every qube is filled
//! (`FillQubePossibilities`, which also prunes its neighbours through `RemovePossibleModule`) and offered to
//! `MaybeQueueQubeForRebuilding`, in the same order on both sides, and all qubes' modules, costs and masks, the
//! `lastRemovals` and `qubesToRebuild` lists compared after each. Also `Propagation.CompareTo` on pairs of
//! qubes and `ResetPropagationCounter`.

mod graph_common;
mod module_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, OrientedModuleSides, QubeId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::maker::Maker;
use ts_game::placemaker::big_mesh::{BigMeshLog, BigMeshMaster};
use ts_game::placemaker::module_builder::{ModuleBuilder, ModuleBuilderHost};
use ts_game::placemaker::modules::{ModuleLibrary, OrientedModule};
use ts_game::placemaker::wfc::{Propagation, WaveFunctionCollapse, WfcHost};
use ts_game::unity::mathematics::{f2, Int2};
use ts_game::unity::vector::Vector3;

const WFC: &str = "Placemaker.WaveFunctionCollapse";
const PROP: &str = "Placemaker.WaveFunctionCollapse+Propagation, Assembly-CSharp";
const LAST: &str = "Placemaker.WaveFunctionCollapse+LastRemovedStruct, Assembly-CSharp";
const MC: &str = "Placemaker.Graphs.ModuleContainer";
const OMS: &str = "Placemaker.Graphs.OrientedModuleSides, Assembly-CSharp";

#[derive(Default)]
struct Host {
    ao: AoBaker,
    maker: Maker,
    wfc: WaveFunctionCollapse,
    /// Qubes whose module containers the graph took down (`ModuleBuilder`'s, row E7).
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

/// The module builder and the log, recorded.
#[derive(Default)]
struct Rec {
    applied: Vec<(QubeId, OrientedModuleSides)>,
    errors: Vec<(String, QubeId)>,
}
impl WfcHost for Rec {
    fn apply_module(&mut self, _graph: &mut Graph, qube: QubeId, module: OrientedModuleSides) {
        self.applied.push((qube, module));
    }
    fn log_error(&mut self, message: &str, qube: QubeId) {
        self.errors.push((message.into(), qube));
    }
}

fn settle(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut Host) {
    while ret::bytes(&g.g("Iterate", vec![]))[0] != 0 {}
    while ours.iterate(grid, host) {}
    while ret::bytes(&g.g("IterateFocus", vec![]))[0] != 0 {}
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

/// The game's qubes (in squares, then the pool) as kept handles, with our ids and their addresses.
struct Qubes {
    handles: Vec<u64>,
    ids: Vec<QubeId>,
    by_addr: HashMap<u64, QubeId>,
    addr_of: HashMap<QubeId, u64>,
}

fn qubes(g: &mut Game, ours: &Graph) -> Qubes {
    let ts = [g.containers["squares"], g.containers["qubePool"]];
    let kids = g.children(&ts);
    let squares = g.components(&kids[0], "Placemaker.Graphs.Square");
    let _ = squares;
    let qube_kids = g.children(&kids[0]);
    let mut qube_ts = vec![];
    let mut ids = vec![];
    assert_eq!(kids[0].len(), ours.squares.len(), "squares");
    for (k, &s) in ours.squares.iter().enumerate() {
        assert_eq!(qube_kids[k].len(), ours.square(s).qubes.len(), "qubes of square {s:?}");
        qube_ts.extend(qube_kids[k].iter().copied());
        ids.extend(ours.square(s).qubes.iter().copied());
    }
    assert_eq!(kids[1].len(), ours.qube_pool.len(), "qube pool");
    qube_ts.extend(kids[1].iter().copied());
    ids.extend(ours.qube_pool.iter().copied());
    let handles = g.components(&qube_ts, "Placemaker.Graphs.Qube");
    let addrs = g.addrs(&handles);
    Qubes {
        by_addr: addrs.iter().zip(&ids).map(|(&a, &q)| (a, q)).collect(),
        addr_of: addrs.iter().zip(&ids).map(|(&a, &q)| (q, a)).collect(),
        handles,
        ids,
    }
}

fn ptr(b: &[u8]) -> u64 {
    u32::from_le_bytes(b[..4].try_into().unwrap()) as u64
}

fn qube_of(qs: &Qubes, a: u64) -> Option<QubeId> {
    if a == 0 {
        None
    } else {
        Some(*qs.by_addr.get(&a).unwrap_or_else(|| panic!("address {a:#x} is no qube of the graph")))
    }
}

fn oms_bytes(v: &[OrientedModuleSides]) -> Vec<u8> {
    v.iter().flat_map(|m| bytes(m)).collect()
}

fn wfc_list(g: &mut Game, wfc: u64, f: &str, elem: &str) -> Vec<u8> {
    let l = handle(&ok(g.h.request(Game::field(wfc, f)), f), f);
    ret::bytes(&ok(g.h.request(Game::call(l, &list(elem), "ToArray", vec![], false)), f))
}

fn game_props(g: &mut Game, qs: &Qubes, wfc: u64, f: &str) -> Vec<Propagation> {
    wfc_list(g, wfc, f, PROP)
        .chunks(16)
        .map(|c| Propagation {
            qube: qube_of(qs, ptr(c)),
            propagation_index: u16::from_le_bytes([c[4], c[5]]),
            existance_index: u32::from_le_bytes(c[8..12].try_into().unwrap()),
            side_variation_mask: c[12],
        })
        .collect()
}

/// The WFC's state and every qube's WFC fields, both sides.
fn check_state(g: &mut Game, qs: &Qubes, wfc: u64, ours: &Graph, w: &WaveFunctionCollapse, what: &str) {
    check_state_with(g, qs, wfc, ours, w, what, true)
}

/// `grass`: compare `awaitingGrass` too (the module builder sets it, row E7; only the WFC's `QubeRemoved` clears it).
fn check_state_with(g: &mut Game, qs: &Qubes, wfc: u64, ours: &Graph, w: &WaveFunctionCollapse, what: &str, grass: bool) {
    for (f, ours_list) in [
        ("updates", &w.updates),
        ("removeNeighbors", &w.remove_neighbors),
        ("propagations", &w.propagations),
        ("toCollapse", &w.to_collapse),
        ("decorUpdates", &w.decor_updates),
        ("decorPropagations", &w.decor_propagations),
        ("decorToCollapse", &w.decor_to_collapse),
        ("decorToBuild", &w.decor_to_build),
    ] {
        assert_eq!(&game_props(g, qs, wfc, f), ours_list, "{what}: {f}");
    }
    let ls = ret::bytes(&ok(g.h.request(Game::field(wfc, "lastStartedPropagation")), "lastStarted"));
    assert_eq!(u16::from_le_bytes([ls[0], ls[1]]), w.last_started_propagation, "{what}: lastStartedPropagation");
    for (f, ours_list) in [("lastRemovals", &w.last_removals), ("lastDecorRemovals", &w.last_decor_removals)] {
        let lr: Vec<(Option<QubeId>, u16, i8, u16)> = wfc_list(g, wfc, f, LAST)
            .chunks(12)
            .map(|c| (qube_of(qs, ptr(c)), u16::from_le_bytes([c[4], c[5]]), c[6] as i8, u16::from_le_bytes([c[8], c[9]])))
            .collect();
        let lo: Vec<(Option<QubeId>, u16, i8, u16)> = ours_list.iter().map(|l| (l.qube, l.side, l.side_index, l.propagation_index)).collect();
        assert_eq!(lr, lo, "{what}: {f}");
    }
    let rb: Vec<Option<QubeId>> = {
        let qc = list("Placemaker.Graphs.Qube, Assembly-CSharp");
        let l = handle(&ok(g.h.request(Game::field(wfc, "qubesToRebuild")), "qubesToRebuild"), "qubesToRebuild");
        let n = ret::i32(&ok(g.h.request(Game::call(l, &qc, "get_Count", vec![], false)), "Count"));
        let items: Vec<u64> = g.batch((0..n).map(|i| Game::call(l, &qc, "get_Item", vec![arg::i32(i)], true)).collect(), "get_Item").iter().map(|r| handle(r, "qube")).collect();
        g.addrs(&items).into_iter().map(|a| qube_of(qs, a)).collect()
    };
    assert_eq!(rb, w.qubes_to_rebuild.iter().map(|&q| Some(q)).collect::<Vec<_>>(), "{what}: qubesToRebuild");
    let counter = ret::bytes(&ok(g.h.request(Game::field(wfc, "propagationCounter")), "counter"));
    assert_eq!(u16::from_le_bytes([counter[0], counter[1]]), w.propagation_counter, "{what}: propagationCounter");
    let names = ["lastPropagation", "cost", "sideVariationMask", "awaitingRebuilding", "awaitingGrass", "possibleModules", "decorSideVariationMask"];
    let f = g.fields(&qs.handles, &names);
    let lists: Vec<u64> = f.iter().map(|v| handle(&v[5], "possibleModules")).collect();
    let pm = g.batch(lists.iter().map(|&l| Game::call(l, &list(OMS), "ToArray", vec![], false)).collect(), "ToArray");
    for (k, &q) in qs.ids.iter().enumerate() {
        let o = ours.qube(q);
        let v = &f[k];
        assert_eq!(u16::from_le_bytes(ret::bytes(&v[0])[..2].try_into().unwrap()), o.last_propagation, "{what}: qube {q:?} lastPropagation");
        assert_eq!(u32::from_le_bytes(ret::bytes(&v[1])[..4].try_into().unwrap()), o.cost.to_bits(), "{what}: qube {q:?} cost {}", o.cost);
        assert_eq!(ret::bytes(&v[2])[0], o.side_variation_mask, "{what}: qube {q:?} sideVariationMask");
        assert_eq!(ret::bytes(&v[3])[0] != 0, o.awaiting_rebuilding, "{what}: qube {q:?} awaitingRebuilding");
        if grass {
            assert_eq!(ret::bytes(&v[4])[0] != 0, o.awaiting_grass, "{what}: qube {q:?} awaitingGrass");
        }
        assert_eq!(ret::bytes(&v[6])[0], o.decor_side_variation_mask, "{what}: qube {q:?} decorSideVariationMask");
        let gm = ret::bytes(&pm[k]);
        if gm != oms_bytes(&o.possible_modules) {
            let n = gm.len() / 0x1C;
            panic!("{what}: qube {q:?} possibleModules: game {n}, ours {}\ngame {:02x?}\nours {:?}", o.possible_modules.len(), &gm[..gm.len().min(0x1C * 4)], &o.possible_modules[..o.possible_modules.len().min(4)]);
        }
    }
}

#[test]
fn wave_function_collapse_per_qube_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut lib = module_common::our_module_library();
    lib.setup();
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "master"), "master");
    let wfc = handle(&ok(g.h.request(Game::field(master, "waveFunctionCollapse")), "wfc"), "wfc");
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = Host { ao, ..Default::default() };
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    settle(&mut g, &mut grid, &mut ours, &mut host);
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(61);
    let mut verts: Vec<Int2> = Vec::new();
    for _ in 0..150 {
        let q = grid.get_quad(f2(r.f(cx - 2.5, cx + 2.5), r.f(cz - 2.5, cz + 2.5)));
        if q.is_valid() {
            for v in [q.v0, q.v1, q.v2, q.v3] {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
    }
    let mut id = 0;
    for round in 0..3 {
        for _ in 0..12 {
            let a = random_action(&mut r, &ours, &verts, id);
            id += 1;
            ours.apply_action(&mut grid, &mut host, a, false);
            g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
            settle(&mut g, &mut grid, &mut ours, &mut host);
            // the world master's step counter between frames
            ok(g.h.request(Game::call(wfc, WFC, "IteratePropagationCount", vec![], false)), "IteratePropagationCount");
            host.wfc.iterate_propagation_count();
        }
        let qs = qubes(&mut g, &ours);
        check_state(&mut g, &qs, wfc, &ours, &host.wfc, &format!("round {round} built"));

        // fill every qube in squares, in a shuffled order, on both sides
        let mut order: Vec<usize> = (0..qs.ids.len()).filter(|&k| ours.qube(qs.ids[k]).square.is_some()).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, (r.next() as usize) % (i + 1));
        }
        let mut rec = Rec::default();
        let mut game_errors = 0;
        for (n, &k) in order.iter().enumerate() {
            let q = qs.ids[k];
            let gr = g.h.request(json!({"call": format!("{WFC}.FillQubePossibilities"), "handle": wfc, "args": [{"handle": qs.handles[k]}], "main": true}));
            let gr = ok(gr, "FillQubePossibilities");
            let om = host.wfc.fill_qube_possibilities(&mut ours, &lib, &mut rec, q);
            assert_eq!(ret::bytes(&gr)[0], om, "round {round}: FillQubePossibilities({q:?}) mask");
            let gq = ret::bytes(&ok(g.h.request(json!({"call": format!("{WFC}.MaybeQueueQubeForRebuilding"), "handle": wfc, "args": [{"handle": qs.handles[k]}], "main": true})), "MaybeQueue"))[0] != 0;
            let oq = host.wfc.maybe_queue_qube_for_rebuilding(&mut ours, &mut rec, q);
            assert_eq!(gq, oq, "round {round}: MaybeQueueQubeForRebuilding({q:?})");
            if n % 6 == 5 || n + 1 == order.len() {
                check_state(&mut g, &qs, wfc, &ours, &host.wfc, &format!("round {round}: {} qubes filled", n + 1));
            }
            game_errors += 0;
        }
        let _ = game_errors;
        eprintln!(
            "round {round}: {} qubes filled, {} built, {} errors, {} removals, {} to rebuild",
            order.len(),
            rec.applied.len(),
            rec.errors.len(),
            host.wfc.last_removals.len(),
            host.wfc.qubes_to_rebuild.len()
        );

        // CompareTo on pairs of the queued qubes
        let mut pairs = vec![];
        for _ in 0..200 {
            let mut p = || {
                let k = (r.next() as usize) % qs.ids.len();
                Propagation { qube: Some(qs.ids[k]), propagation_index: (r.next() % 3) as u16 * 4, existance_index: r.next() % 5, side_variation_mask: r.next() as u8 }
            };
            pairs.push((p(), p()));
        }
        let raw = |p: &Propagation| {
            let mut b = vec![0u8; 16];
            b[..4].copy_from_slice(&(qs.addr_of[&p.qube.unwrap()] as u32).to_le_bytes());
            b[4..6].copy_from_slice(&p.propagation_index.to_le_bytes());
            b[8..12].copy_from_slice(&p.existance_index.to_le_bytes());
            b[12] = p.side_variation_mask;
            b
        };
        let reqs: Vec<Value> = pairs
            .iter()
            .map(|(a, b)| {
                json!({"call": "System.IComparable<Placemaker.WaveFunctionCollapse.Propagation>.CompareTo", "class": PROP,
                       "this_raw": {"class": PROP, "raw": hex(&raw(a))}, "args": [arg::raw(&raw(b))], "main": true})
            })
            .collect();
        let got = g.batch(reqs, "CompareTo");
        for ((a, b), gr) in pairs.iter().zip(&got) {
            assert_eq!(ret::i32(gr).signum(), a.compare_to(b, &ours).signum(), "CompareTo({a:?}, {b:?})");
        }

        // the queues start over for the next round on both sides
        for f in ["updates", "removeNeighbors", "lastRemovals", "qubesToRebuild"] {
            let elem = match f {
                "lastRemovals" => LAST,
                "qubesToRebuild" => "Placemaker.Graphs.Qube, Assembly-CSharp",
                _ => PROP,
            };
            let l = handle(&ok(g.h.request(Game::field(wfc, f)), f), f);
            ok(g.h.request(Game::call(l, &list(elem), "Clear", vec![], false)), "Clear");
        }
        host.wfc.updates.clear();
        host.wfc.remove_neighbors.clear();
        host.wfc.last_removals.clear();
        host.wfc.qubes_to_rebuild.clear();
    }

    // ResetPropagationCounter past 0x7FFF
    let qs = qubes(&mut g, &ours);
    for v in [0x7FFFu16, 0x8004] {
        ok(g.h.request(json!({"field": "propagationCounter", "handle": wfc, "set_raw": hex(&v.to_le_bytes()), "main": true})), "set counter");
        host.wfc.propagation_counter = v;
        ok(g.h.request(Game::call(wfc, WFC, "ResetPropagationCounter", vec![], false)), "ResetPropagationCounter");
        host.wfc.reset_propagation_counter(&mut ours);
        check_state(&mut g, &qs, wfc, &ours, &host.wfc, &format!("ResetPropagationCounter at {v:#x}"));
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// No sound, sandbanker, camera or props on our side (the game's stand-in has them, they change nothing compared).
struct NoSides;
impl ModuleBuilderHost for NoSides {}

/// Our module builder and big meshes behind the collapse.
struct Builder<'a> {
    lib: &'a ModuleLibrary,
    mb: ModuleBuilder,
    bmm: BigMeshMaster,
    log: Vec<BigMeshLog>,
    applied: usize,
    decor: usize,
    errors: usize,
}
impl WfcHost for Builder<'_> {
    fn apply_module(&mut self, graph: &mut Graph, qube: QubeId, module: OrientedModuleSides) {
        self.applied += 1;
        self.mb.apply_module(graph, self.lib, &mut self.bmm, &mut NoSides, &mut self.log, qube, module);
    }
    fn apply_decor(&mut self, graph: &mut Graph, qube: QubeId) {
        self.decor += 1;
        self.mb.apply_decor(graph, self.lib, &mut self.bmm, &mut NoSides, &mut self.log, qube);
    }
    fn mark_module_containers_for_deconstruction(&mut self, _graph: &mut Graph, qube: QubeId) {
        self.mb.mark_qube_containers_for_deconstruction(&mut self.bmm, &mut NoSides, qube);
    }
    fn current_module(&mut self, _graph: &Graph, qube: QubeId) -> Option<OrientedModule> {
        self.mb.children(qube).first().map(|&c| self.mb.container(c).oriented_module)
    }
    fn log_error(&mut self, _message: &str, _qube: QubeId) {
        self.errors += 1;
    }
}

/// Calls a `bool` method of the game's WFC until it returns false (in batches: once false, the loops stay false).
fn run_game(g: &mut Game, wfc: u64, m: &str) -> usize {
    let mut n = 0;
    loop {
        let reqs = (0..64).map(|_| Game::call(wfc, WFC, m, vec![], false)).collect();
        let r = g.batch(reqs, m);
        for v in &r {
            if ret::bytes(v)[0] == 0 {
                return n;
            }
            n += 1;
        }
        assert!(n < 1_000_000, "{m} never ends");
    }
}

fn u16s(b: &[u8]) -> Vec<u16> {
    b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}
fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits_of_f32<T: Copy>(v: &[T]) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }.to_vec()
}

/// The game's `List<T>` field of an object, as element bytes.
fn list_bytes(g: &mut Game, obj: u64, f: &str, elem: &str) -> Vec<u8> {
    let l = handle(&ok(g.h.request(Game::field(obj, f)), f), f);
    ret::bytes(&ok(g.h.request(Game::call(l, &list(elem), "ToArray", vec![], false)), f))
}

/// The objects of a `List<class>` field.
fn list_items(g: &mut Game, obj: u64, f: &str, elem: &str) -> Vec<u64> {
    let l = handle(&ok(g.h.request(Game::field(obj, f)), f), f);
    let lc = list(elem);
    let n = ret::i32(&ok(g.h.request(Game::call(l, &lc, "get_Count", vec![], false)), "Count"));
    g.batch((0..n).map(|i| Game::call(l, &lc, "get_Item", vec![arg::i32(i)], true)).collect(), "get_Item").iter().map(|r| handle(r, "item")).collect()
}

const V3T: &str = "UnityEngine.Vector3, UnityEngine.CoreModule";
const V4T: &str = "UnityEngine.Vector4, UnityEngine.CoreModule";
const V2T: &str = "UnityEngine.Vector2, UnityEngine.CoreModule";

/// The game's module builder and big meshes against ours: every big mesh of every group, each qube's
/// containers (module, then decor) with their parts' slots, the pool, the deconstruction list, the grass queue.
fn check_builder(g: &mut Game, qs: &Qubes, mb: u64, groups: &[u64], b: &Builder, what: &str) {
    // big meshes, and their addresses
    let mut mesh_of: HashMap<u64, (usize, usize)> = HashMap::new();
    for (gi, &grp) in groups.iter().enumerate() {
        let meshes = list_items(g, grp, "bigMeshes", "Placemaker.BigMesh, Assembly-CSharp");
        let ours_meshes = &b.bmm.big_mesh_groups[gi].big_meshes;
        assert_eq!(meshes.len(), ours_meshes.len(), "{what}: group {gi}: big mesh count");
        let addrs = g.addrs(&meshes);
        for (k, (&m, &a)) in meshes.iter().zip(&addrs).enumerate() {
            mesh_of.insert(a, (gi, k));
            let o = &ours_meshes[k];
            let w = format!("{what}: group {gi} big mesh {k}");
            assert_eq!(ret::i32(&ok(g.h.request(Game::field(m, "createdCount")), "createdCount")), o.created_count, "{w}: createdCount");
            assert_eq!(u16s(&list_bytes(g, m, "availableIndexes", "System.UInt16")), o.available_indexes, "{w}: availableIndexes");
            assert_eq!(i32s(&list_bytes(g, m, "availableTris", "System.Int32")), o.available_tris, "{w}: availableTris");
            assert_eq!(list_bytes(g, m, "verts", V3T), bits_of_f32(&o.verts), "{w}: verts");
            assert_eq!(list_bytes(g, m, "normals", V3T), bits_of_f32(&o.normals), "{w}: normals");
            assert_eq!(list_bytes(g, m, "tangents", V4T), bits_of_f32(&o.tangents), "{w}: tangents");
            let uv = list_bytes(g, m, "uvs", V2T);
            if uv != bits_of_f32(&o.uvs) {
                let gu = f32s(&uv);
                let k0 = gu.iter().zip(o.uvs.iter().flat_map(|v| [v.x, v.y])).position(|(a, b)| a.to_bits() != b.to_bits());
                panic!("{w}: uvs differ at {k0:?} ({} vs {})", gu.len(), o.uvs.len() * 2);
            }
            assert_eq!(i32s(&list_bytes(g, m, "tris", "System.Int32")), o.tris, "{w}: tris");
        }
    }
    // each qube's containers
    let qube_ts: Vec<u64> = g.batch(qs.handles.iter().map(|&q| Game::call(q, "UnityEngine.Component", "get_transform", vec![], true)).collect(), "transform").iter().map(|r| handle(r, "t")).collect();
    let kids = g.children(&qube_ts);
    if !g.types.contains_key(MC) {
        let t = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string(&format!("{MC}, Assembly-CSharp"))], "param_types": ["System.String"], "main": true, "keep": true})), MC), MC);
        g.types.insert(MC, t);
    }
    for (k, &q) in qs.ids.iter().enumerate() {
        let ours_kids = b.mb.children(q);
        assert_eq!(kids[k].len(), ours_kids.len(), "{what}: qube {q:?} container count");
        if kids[k].is_empty() {
            continue;
        }
        let cs = g.components(&kids[k], MC);
        for (ci, (&c, &oc)) in cs.iter().zip(ours_kids).enumerate() {
            let w = format!("{what}: qube {q:?} container {ci}");
            let om = ret::bytes(&ok(g.h.request(Game::field(c, "orientedModule")), "orientedModule"));
            let oo = b.mb.container(oc);
            assert_eq!((u16::from_le_bytes([om[0], om[1]]), om[2]), (oo.oriented_module.module_index, oo.oriented_module.flags), "{w}: orientedModule");
            let parts = list_items(g, c, "bigMeshParts", "Placemaker.BigMeshPart, Assembly-CSharp");
            assert_eq!(parts.len(), oo.big_mesh_parts.len(), "{w}: part count");
            for (pi, (&p, op)) in parts.iter().zip(&oo.big_mesh_parts).enumerate() {
                let bm = ok(g.h.request(Game::field(p, "bigMesh")), "bigMesh");
                let gm = if bm["null"] == json!(true) { None } else { Some(mesh_of[&g.addrs(&[handle(&bm, "bigMesh")])[0]]) };
                assert_eq!(gm, op.big_mesh.map(|m| (m.group, m.mesh)), "{w} part {pi}: big mesh");
                assert_eq!(u16s(&list_bytes(g, p, "vertIndexes", "System.UInt16")), op.vert_indexes, "{w} part {pi}: vertIndexes");
                assert_eq!(i32s(&list_bytes(g, p, "triIndexes", "System.Int32")), op.tri_indexes, "{w} part {pi}: triIndexes");
            }
        }
    }
    // the pool, the deconstruction list, the grass queue
    for (f, n) in [("moduleContainerPool", b.mb.container_pool.len()), ("moduleContainersToDeconstruct", b.mb.containers_to_deconstruct.len())] {
        let t = handle(&ok(g.h.request(Game::field(mb, f)), f), f);
        let c = ret::i32(&ok(g.h.request(Game::call(t, "UnityEngine.Transform", "get_childCount", vec![], false)), "childCount"));
        assert_eq!(c as usize, n, "{what}: {f}");
    }
    let grass = list_items(g, mb, "qubesToGrass", "Placemaker.Graphs.Qube, Assembly-CSharp");
    let grass: Vec<QubeId> = g.addrs(&grass).into_iter().map(|a| qube_of(qs, a).unwrap()).collect();
    assert_eq!(grass, b.mb.qubes_to_grass, "{what}: qubesToGrass");
}

#[test]
fn wave_function_collapse_loops_match_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut lib = module_common::our_module_library();
    lib.setup();
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "master"), "master");
    let wfc = handle(&ok(g.h.request(Game::field(master, "waveFunctionCollapse")), "wfc"), "wfc");
    // the scene's prop placer, so that replaced module containers leave their qube at once, and its sounds
    let scene_master = handle(&ok(g.h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "scene master"), "scene master");
    for f in ["propPlacer", "manySoundsManager"] {
        let v = handle(&ok(g.h.request(Game::field(scene_master, f)), f), f);
        ok(g.h.request(json!({"field": f, "handle": master, "set_handle": v, "main": true})), f);
    }
    // a fresh big mesh master (the scene's own LateUpdate never touches it): one group per scene group, with its
    // material and a copy of its template; materials are matched to ours through the module meshes'
    let mat_of: HashMap<u64, ts_game::placemaker::modules::AssetRef> = {
        let scene_lib = handle(&ok(g.h.request(Game::field(scene_master, "moduleLibrary")), "moduleLibrary"), "moduleLibrary");
        let meshes = list_items(&mut g, scene_lib, "moduleMeshes", "Placemaker.ModuleMesh, Assembly-CSharp");
        let mats: Vec<u64> = g.batch(meshes.iter().map(|&m| Game::field(m, "material")).collect(), "material").iter().map(|r| handle(r, "material")).collect();
        g.addrs(&mats).into_iter().zip(&lib.module_meshes).map(|(a, m)| (a, m.material)).collect()
    };
    let bmm = handle(&ok(g.h.request(json!({"new": "Placemaker.BigMeshMaster", "args": [], "main": true})), "new BigMeshMaster"), "bmm");
    ok(g.h.request(json!({"field": "master", "handle": bmm, "set_handle": master, "main": true})), "bmm.master");
    let scene_bmm = handle(&ok(g.h.request(Game::field(scene_master, "bigMeshMaster")), "bigMeshMaster"), "bigMeshMaster");
    let group_type = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.BigMeshGroup, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let gl = list("Placemaker.BigMeshGroup, Assembly-CSharp");
    let new_groups = handle(&ok(g.h.request(Game::field(bmm, "bigMeshGroups")), "bigMeshGroups"), "bigMeshGroups");
    let mut groups = vec![];
    let mut ours_bmm = ts_game::placemaker::big_mesh::BigMeshMaster::default();
    for sg in list_items(&mut g, scene_bmm, "bigMeshGroups", "Placemaker.BigMeshGroup, Assembly-CSharp") {
        let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check big mesh group")], "param_types": ["System.String"], "main": true})), "go"), "go");
        let grp = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": group_type}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "group");
        let mat = handle(&ok(g.h.request(Game::field(sg, "material")), "material"), "material");
        ok(g.h.request(json!({"field": "material", "handle": grp, "set_handle": mat, "main": true})), "material");
        let st = handle(&ok(g.h.request(Game::call(sg, "UnityEngine.Component", "get_transform", vec![], true)), "t"), "t");
        let template = handle(&ok(g.h.request(Game::call(st, "UnityEngine.Transform", "GetChild", vec![arg::i32(0)], true)), "child"), "child");
        let gt = handle(&ok(g.h.request(Game::call(grp, "UnityEngine.Component", "get_transform", vec![], true)), "t"), "t");
        let copy = handle(&ok(g.h.request(json!({"call": "UnityEngine.Object.Instantiate", "args": [{"handle": template}], "param_types": ["UnityEngine.Object"], "main": true, "keep": true})), "Instantiate"), "copy");
        ok(g.h.request(json!({"call": "SetParent", "class": "UnityEngine.Transform", "handle": copy, "args": [{"handle": gt}, arg::bool(false)], "param_types": ["UnityEngine.Transform", "System.Boolean"], "main": true})), "SetParent");
        ok(g.h.request(Game::call(new_groups, &gl, "Add", vec![json!({"handle": grp})], false)), "Add group");
        let a = g.addrs(&[mat])[0];
        let material = mat_of.get(&a).copied().unwrap_or(ts_game::placemaker::modules::AssetRef { file_id: -1, path_id: a as i64 });
        ours_bmm.big_mesh_groups.push(ts_game::placemaker::big_mesh::BigMeshGroup { material, ..Default::default() });
        groups.push(grp);
    }
    ok(g.h.request(json!({"field": "bigMeshMaster", "handle": master, "set_handle": bmm, "main": true})), "master.bigMeshMaster");
    let fence = {
        let mm = handle(&ok(g.h.request(Game::field(scene_master, "materialMaster")), "materialMaster"), "materialMaster");
        let f = handle(&ok(g.h.request(Game::field(mm, "fenceMaterial")), "fenceMaterial"), "fenceMaterial");
        let a = g.addrs(&[f])[0];
        mat_of.get(&a).copied().expect("the fence material is a module mesh material")
    };
    // the fresh module builder's two containers (its `OnStart` needs a transform of its own)
    let mb = handle(&ok(g.h.request(Game::field(master, "moduleBuilder")), "moduleBuilder"), "moduleBuilder");
    for f in ["moduleContainerPool", "moduleContainersToDeconstruct"] {
        let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(f)], "param_types": ["System.String"], "main": true})), "go"), "go");
        ok(g.h.request(json!({"call": "SetActive", "class": "UnityEngine.GameObject", "handle": go, "args": [arg::bool(false)], "main": true})), "SetActive");
        let t = handle(&ok(g.h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
        ok(g.h.request(json!({"field": f, "handle": mb, "set_handle": t, "main": true})), f);
    }
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = Host { ao, ..Default::default() };
    let mut b = Builder { lib: &lib, mb: ModuleBuilder { fence_material: fence, ..Default::default() }, bmm: ours_bmm, log: vec![], applied: 0, decor: 0, errors: 0 };
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    settle(&mut g, &mut grid, &mut ours, &mut host);
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(71);
    let mut verts: Vec<Int2> = Vec::new();
    for _ in 0..150 {
        let q = grid.get_quad(f2(r.f(cx - 2.5, cx + 2.5), r.f(cz - 2.5, cz + 2.5)));
        if q.is_valid() {
            for v in [q.v0, q.v1, q.v2, q.v3] {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
    }
    let (mut steps0, mut steps1, mut steps_d) = (0, 0, 0);
    let (mut removals, mut decor_removals) = (0, 0);
    for step in 0..12 {
        let t0 = std::time::Instant::now();
        let a = random_action(&mut r, &ours, &verts, step);
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
        for q in host.cleared.drain(..) {
            b.mb.mark_qube_containers_for_deconstruction(&mut b.bmm, &mut NoSides, q);
        }
        // the world master's order: a new step, Iterate0, the queued rebuilds, Iterate1; then LaterUpdate
        ok(g.h.request(Game::call(wfc, WFC, "IteratePropagationCount", vec![], false)), "IteratePropagationCount");
        host.wfc.iterate_propagation_count();
        let ts = t0.elapsed().as_secs_f32();
        let n0 = run_game(&mut g, wfc, "Iterate0");
        let mut k = 0;
        while host.wfc.iterate0(&mut ours, &lib, &mut b) {
            k += 1;
        }
        assert_eq!(n0, k, "step {step}: Iterate0 rounds");
        let t1 = t0.elapsed().as_secs_f32();
        // one call per request: past the frame's time budget the game's loop makes no progress within a batch
        let mut n = 0;
        while ret::bytes(&ok(g.h.request(Game::call(wfc, WFC, "IterateApplyModules", vec![], false)), "IterateApplyModules"))[0] != 0 {
            n += 1;
            assert!(n < 100_000, "IterateApplyModules never ends");
        }
        let t2 = t0.elapsed().as_secs_f32();
        while host.wfc.iterate_apply_modules(&mut ours, &mut b) {}
        let n1 = run_game(&mut g, wfc, "Iterate1");
        let mut k = 0;
        while host.wfc.iterate1(&mut ours, &lib, &mut b) {
            k += 1;
            removals += host.wfc.last_removals.len();
        }
        assert_eq!(n1, k, "step {step}: Iterate1 rounds");
        let t3 = t0.elapsed().as_secs_f32();
        let nd = run_game(&mut g, wfc, "IterateDecor");
        let t4 = t0.elapsed().as_secs_f32();
        let mut k = 0;
        while host.wfc.iterate_decor(&mut ours, &lib, &mut b) {
            k += 1;
            decor_removals += host.wfc.last_decor_removals.len();
        }
        assert_eq!(nd, k, "step {step}: IterateDecor rounds");
        steps_d += nd;
        if step % 3 == 0 {
            ok(g.h.request(Game::call(wfc, WFC, "LaterUpdate", vec![], false)), "LaterUpdate");
            host.wfc.later_update(&mut ours, &lib, &mut b);
        }
        // the module builder's deconstruction (one call per request: time-budgeted), then the big meshes applied
        while ret::bytes(&ok(g.h.request(Game::call(mb, "Placemaker.ModuleBuilder", "IterateDeconstructQubes", vec![], false)), "IterateDeconstructQubes"))[0] != 0 {}
        while b.mb.iterate_deconstruct_qubes(&mut NoSides) {}
        ok(g.h.request(Game::call(bmm, "Placemaker.BigMeshMaster", "OnLateUpdate", vec![], false)), "OnLateUpdate");
        b.bmm.on_late_update(&mut b.log);
        steps0 += n0;
        steps1 += n1;
        eprintln!("step {step}: {:.1} s (settled {ts:.1}, Iterate0 {t1:.1}, apply {t2:.1}, Iterate1 {t3:.1}, decor {t4:.1}; {n0} / {n1} / {nd} rounds)", t0.elapsed().as_secs_f32());
        if step % 3 == 2 {
            let qs = qubes(&mut g, &ours);
            check_state_with(&mut g, &qs, wfc, &ours, &host.wfc, &format!("step {step}"), false);
            check_builder(&mut g, &qs, mb, &groups, &b, &format!("step {step}"));
            eprintln!(
                "step {step}: same (big meshes: {:?} vertices by group; {removals} lastRemovals and {decor_removals} lastDecorRemovals seen; {} qubes, {steps0} Iterate0, {steps1} Iterate1, {steps_d} IterateDecor rounds, {} modules and {} decor applied, {} decor updates, {} errors)",
                b.bmm.big_mesh_groups.iter().map(|g| g.big_meshes.iter().map(|m| m.created_count - m.available_indexes.len() as i32).sum::<i32>()).collect::<Vec<_>>(),
                qs.ids.len(),
                b.applied,
                b.decor,
                host.wfc.decor_updates.len(),
                b.errors
            );
        }
    }
}
