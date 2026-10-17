//! `placemaker::graphs::Graph` editing against the original game (E4c). Needs `TS_HARNESS=1` and the player's
//! install. Both sides grow the same grid (a fresh `GridGenerator`, as in `placemaker_bits.rs`); a fresh game
//! `Graph` gets new containers and a stand-in `WorldMaster` made with `new`: our grid and graph, the scene's
//! `WorldMeshes`, `MaterialMaster`, `Sandbanker`, `ClickEffect` (and a few more), and a fresh
//! `WaveFunctionCollapse` and `ModuleBuilder` whose master is the stand-in (their queues are never run), so
//! nothing of the scene touches the test graph. Random actions (add, paint, remove) run on both, and the whole
//! graph is compared: every object in every container, the references between them (by address),
//! the dictionaries, queues and counters (after each of the first steps, then every 10th).

use std::collections::HashMap;

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::graphs::{Action, CornerId, Graph, GraphHost, QubeId, SquareId, VoxelId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::quads::grid_generation::{CustomGridLibrary, CustomGridPatch};
use ts_game::unity::mathematics::{f2, i3, Int2, Int3x2};
use ts_game::unity::vector::Vector3;

const G: &str = "Placemaker.Graphs.Graph";
const GEN: &str = "Placemaker.GridGenerator";

fn bits_of<T: Copy>(v: &[T]) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }.to_vec()
}
fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    bits_of(std::slice::from_ref(v))
}
fn i32s(b: &[u8]) -> Vec<i32> {
    b.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn u32s(b: &[u8]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}
fn opt_handle(r: &Value) -> Option<u64> {
    r["handle"].as_u64()
}
fn list(t: &str) -> String {
    format!("System.Collections.Generic.List`1[[{t}]]")
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() as f32 / u32::MAX as f32) * (hi - lo)
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.next() as usize % v.len()]
    }
}

fn our_library() -> CustomGridLibrary {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let d = ts_data::grid::custom_grid_library(&a).expect("CustomGridLibrary");
    CustomGridLibrary {
        all_custom_patches: d
            .all_custom_patches
            .into_iter()
            .map(|p| CustomGridPatch { verts: p.verts.into_iter().map(|[x, y]| Int2 { x, y }).collect(), quads: p.quads, quad_tri_cutoff: p.quad_tri_cutoff, subdivisions: p.subdivisions })
            .collect(),
        generic_patches: d.generic_patches,
        straight_patches: d.straight_patches,
        centroid_patches: d.centroid_patches,
        centroid_curve_patches: d.centroid_curve_patches,
    }
}

/// Our side of the systems round the graph: nothing but a record of the calls.
#[derive(Default)]
struct Recorder {
    calls: Vec<String>,
}
impl GraphHost for Recorder {
    fn sandbanker_reset(&mut self) {
        self.calls.push("Sandbanker.Reset".into());
    }
    fn get_mesh(&mut self) {
        self.calls.push("GetMesh".into());
    }
    fn return_mesh(&mut self) {
        self.calls.push("ReturnMesh".into());
    }
    fn qube_removed(&mut self, graph: &mut Graph, qube: QubeId, side_mask: u8) {
        // what the stand-in's fresh `WaveFunctionCollapse.QubeRemoved` does to the fields compared here (it also
        // sets the neighbours' `lastPropagation` to its counter, 0, which they are already)
        let q = graph.qube_mut(qube);
        q.awaiting_rebuilding = false;
        q.awaiting_grass = false;
        q.last_propagation = 0;
        q.possible_modules.clear();
        q.side_variation_mask = 0;
        self.calls.push(format!("QubeRemoved({}, {side_mask})", qube.0));
    }
    fn log(&mut self, m: &str) {
        self.calls.push(format!("Log({m})"));
    }
    fn log_error(&mut self, m: &str) {
        self.calls.push(format!("LogError({m})"));
    }
}

struct Game<'a> {
    h: &'a mut Harness,
    gen: u64,
    graph: u64,
    containers: HashMap<&'static str, u64>,
    types: HashMap<&'static str, u64>,
}

/// One graph object, in our ids (`u32::MAX` for an address that is no object of the graph).
#[derive(Debug, PartialEq, Clone)]
struct CornerS {
    hex: Int2,
    angle: u32,
    ground: u8,
    in_flow: bool,
    pos: Option<[u32; 3]>,
    squares: Vec<u32>,
    blit: Vec<u32>,
    flow: Vec<(u32, u8, Vec<u32>)>,
    mesh: Option<(Vec<u32>, Vec<i32>)>,
    voxels: Vec<u32>,
}
#[derive(Debug, PartialEq, Clone)]
struct SquareS {
    q: [(u32, i8); 4],
    c: [u32; 4],
    m: Vec<u32>,
    motivations: u8,
    quad_index: u8,
    pos: [u32; 3],
    qubes: Vec<u32>,
}
#[derive(Debug, PartialEq, Clone)]
struct QubeS {
    voxel_count: u8,
    corner_values: [u8; 8],
    existance: u32,
    shadow: i32,
    last_propagation: u16,
    awaiting_graph_update: bool,
    pos: [u32; 3],
}
#[derive(Debug, PartialEq, Clone)]
struct VoxelS {
    ty: u8,
    height: u8,
    cost: i32,
    pos: Option<[u32; 3]>,
}
#[derive(Debug, PartialEq, Default)]
struct Snap {
    corners: Vec<u32>,
    corner_pool: Vec<u32>,
    squares: Vec<u32>,
    square_pool: Vec<u32>,
    voxel_pool: Vec<u32>,
    qube_pool: Vec<u32>,
    corner_data: Vec<(u32, CornerS)>,
    square_data: Vec<(u32, SquareS)>,
    qube_data: Vec<(u32, QubeS)>,
    voxel_data: Vec<(u32, VoxelS)>,
    integration_queue: Vec<u32>,
    flow_queue: Vec<u32>,
    qube_update_queue: Vec<u32>,
    shadow_indexes: Vec<i32>,
    dict_counts: [usize; 3],
    corner_dict: Vec<(Int2, u32)>,
    square_dict: Vec<(Int2, u32)>,
    edge_dict: Vec<((Int2, Int2), (u32, i8))>,
    scalars: Vec<i32>,
}

fn v3bits(v: Vector3) -> [u32; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}
fn flow_bits(f: &ts_game::placemaker::graphs::FlowData) -> Vec<u32> {
    let mut v = vec![f.dist.to_bits(), f.new_dist.to_bits(), f.denom.to_bits()];
    v.extend(v3bits(f.dir));
    v.extend(v3bits(f.new_dir));
    v.push(f.target as u32);
    v
}

/// Our graph as a snapshot (pooled voxels without a position: the game's keep their world position).
fn ours_snapshot(g: &Graph) -> Snap {
    let id = |o: Option<u32>| o.unwrap_or(u32::MAX);
    let mut s = Snap {
        corners: g.corners.iter().map(|c| c.0).collect(),
        corner_pool: g.corner_pool.iter().map(|c| c.0).collect(),
        squares: g.squares.iter().map(|c| c.0).collect(),
        square_pool: g.square_pool.iter().map(|c| c.0).collect(),
        voxel_pool: g.voxel_pool.iter().map(|c| c.0).collect(),
        qube_pool: g.qube_pool.iter().map(|c| c.0).collect(),
        integration_queue: g.corner_integration_queue.iter().map(|c| c.0).collect(),
        flow_queue: g.corner_flow_queue.iter().map(|c| c.0).collect(),
        qube_update_queue: g.qube_update_queue.iter().map(|c| c.0).collect(),
        shadow_indexes: g.shadow_mesh_indexes.clone(),
        dict_counts: [g.corner_dict.len(), g.square_dict.len(), g.edge_dict.len()],
        scalars: vec![g.total_voxel_count as i32, g.bounds_height, g.bounds.c0.x, g.bounds.c0.y, g.bounds.c0.z, g.bounds.c1.x, g.bounds.c1.y, g.bounds.c1.z, g.bounds_updated as i32, g.recalculate_hex_bounds as i32, g.recalculate_height_bounds as i32],
        ..Default::default()
    };
    let in_pool: Vec<CornerId> = g.corner_pool.clone();
    for &c in g.corners.iter().chain(g.corner_pool.iter()) {
        let k = g.corner(c);
        s.corner_data.push((
            c.0,
            CornerS {
                hex: k.hex_pos,
                angle: k.angle.to_bits(),
                ground: k.ground_coverage,
                in_flow: k.in_flow_update_queue,
                pos: Some(v3bits(k.local_position)),
                squares: k.squares.iter().map(|s| s.0).collect(),
                blit: k.blit_verts.iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits()]).collect(),
                flow: k.flow_datas.iter().map(|f| (id(f.voxel.map(|v| v.0)), f.neighbour_count, flow_bits(f))).collect(),
                mesh: k.collider_mesh.as_ref().map(|m| (m.vertices.iter().flat_map(|v| v3bits(*v)).collect(), m.triangles.clone())),
                voxels: k.voxels.iter().map(|v| v.0).collect(),
            },
        ));
        let _ = &in_pool;
        for &v in &k.voxels {
            let x = g.voxel(v);
            s.voxel_data.push((v.0, VoxelS { ty: x.voxel_type as u8, height: x.height, cost: x.cost, pos: Some(v3bits(x.local_position)) }));
        }
    }
    for &v in &g.voxel_pool {
        let x = g.voxel(v);
        s.voxel_data.push((v.0, VoxelS { ty: x.voxel_type as u8, height: x.height, cost: x.cost, pos: None }));
    }
    let qube = |s: &mut Snap, q: QubeId| {
        let x = g.qube(q);
        s.qube_data.push((
            q.0,
            QubeS {
                voxel_count: x.voxel_count,
                corner_values: x.corner_values,
                existance: x.existance_index,
                shadow: x.shadow_index,
                last_propagation: x.last_propagation,
                awaiting_graph_update: x.awaiting_graph_update,
                pos: v3bits(x.local_position),
            },
        ));
    };
    for &q in g.squares.iter().chain(g.square_pool.iter()) {
        let x = g.square(q);
        s.square_data.push((
            q.0,
            SquareS {
                q: x.q.map(|r| (id(r.square.map(|s| s.0)), r.index_in_other)),
                c: x.c.map(|c| id(c.map(|c| c.0))),
                m: x.m.iter().flat_map(|m| m.m.map(f32::to_bits)).collect(),
                motivations: x.motivations,
                quad_index: x.quad_index,
                pos: v3bits(x.local_position),
                qubes: x.qubes.iter().map(|q| q.0).collect(),
            },
        ));
        for &k in &x.qubes {
            qube(&mut s, k);
        }
    }
    for &q in &g.qube_pool {
        qube(&mut s, q);
    }
    let mut cd: Vec<(Int2, u32)> = g.corner_dict.iter().map(|(k, v)| (*k, v.0)).collect();
    cd.sort_by_key(|(k, _)| (k.x, k.y));
    s.corner_dict = cd;
    let mut sd: Vec<(Int2, u32)> = g.square_dict.iter().map(|(k, v)| (*k, v.0)).collect();
    sd.sort_by_key(|(k, _)| (k.x, k.y));
    s.square_dict = sd;
    let mut ed: Vec<((Int2, Int2), (u32, i8))> = g.edge_dict.iter().map(|(k, v)| (*k, (id(v.square.map(|s| s.0)), v.index_in_other))).collect();
    ed.sort_by_key(|((a, b), _)| (a.x, a.y, b.x, b.y));
    s.edge_dict = ed;
    s
}

impl Game<'_> {
    fn batch(&mut self, reqs: Vec<Value>, what: &str) -> Vec<Value> {
        if reqs.is_empty() {
            return vec![];
        }
        let n = reqs.len();
        let r = self.h.batch(reqs);
        assert_eq!(r.len(), n, "{what}: batch replies");
        r.into_iter().map(|v| ok(v, what)).collect()
    }
    fn call(t: u64, class: &str, m: &str, args: Vec<Value>, keep: bool) -> Value {
        json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": keep})
    }
    fn field(obj: u64, f: &str) -> Value {
        json!({"field": f, "handle": obj, "main": true})
    }
    fn g(&mut self, m: &str, args: Vec<Value>) -> Value {
        let gr = self.graph;
        ok(self.h.request(Game::call(gr, G, m, args, false)), m)
    }
    fn children(&mut self, ts: &[u64]) -> Vec<Vec<u64>> {
        let counts = self.batch(ts.iter().map(|&t| Game::call(t, "UnityEngine.Transform", "get_childCount", vec![], false)).collect(), "childCount");
        let counts: Vec<usize> = counts.iter().map(|r| ret::i32(r) as usize).collect();
        let mut reqs = vec![];
        for (k, &t) in ts.iter().enumerate() {
            for i in 0..counts[k] {
                reqs.push(Game::call(t, "UnityEngine.Transform", "GetChild", vec![arg::i32(i as i32)], true));
            }
        }
        let flat: Vec<u64> = self.batch(reqs, "GetChild").iter().map(|r| handle(r, "child")).collect();
        let mut out = vec![];
        let mut at = 0;
        for &n in &counts {
            out.push(flat[at..at + n].to_vec());
            at += n;
        }
        out
    }
    fn components(&mut self, ts: &[u64], class: &'static str) -> Vec<u64> {
        let ty = self.types[class];
        self.batch(ts.iter().map(|&t| Game::call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": ty})], true)).collect(), "GetComponent").iter().map(|r| handle(r, class)).collect()
    }
    fn addrs(&mut self, hs: &[u64]) -> Vec<u64> {
        self.batch(hs.iter().map(|&h| json!({"addr": h})).collect(), "addr").iter().map(|r| r["addr"].as_u64().unwrap()).collect()
    }
    fn fields(&mut self, objs: &[u64], names: &[&str]) -> Vec<Vec<Value>> {
        let mut reqs = vec![];
        for &o in objs {
            for n in names {
                reqs.push(Game::field(o, n));
            }
        }
        let flat = self.batch(reqs, "fields");
        flat.chunks(names.len().max(1)).map(|c| c.to_vec()).collect()
    }
    fn local_positions(&mut self, ts: &[u64]) -> Vec<[u32; 3]> {
        self.batch(ts.iter().map(|&t| Game::call(t, "UnityEngine.Transform", "get_localPosition", vec![], false)).collect(), "localPosition")
            .iter()
            .map(|r| {
                let v = u32s(&ret::bytes(r));
                [v[0], v[1], v[2]]
            })
            .collect()
    }
    /// Items of reference lists (handles), mapped to our ids by address.
    fn list_ids(&mut self, lists: &[Option<u64>], class: &str, map: &HashMap<u64, u32>) -> Vec<Vec<u32>> {
        let lc = list(class);
        let real: Vec<u64> = lists.iter().flatten().copied().collect();
        let counts: Vec<usize> = self.batch(real.iter().map(|&l| Game::call(l, &lc, "get_Count", vec![], false)).collect(), "get_Count").iter().map(|r| ret::i32(r) as usize).collect();
        let mut reqs = vec![];
        for (k, &l) in real.iter().enumerate() {
            for i in 0..counts[k] {
                reqs.push(Game::call(l, &lc, "get_Item", vec![arg::i32(i as i32)], true));
            }
        }
        let items: Vec<u64> = self.batch(reqs, "get_Item").iter().map(|r| handle(r, "item")).collect();
        let addrs = self.addrs(&items);
        let mut out = vec![];
        let (mut at, mut k) = (0, 0);
        for l in lists {
            if l.is_some() {
                out.push(addrs[at..at + counts[k]].iter().map(|a| *map.get(a).unwrap_or(&u32::MAX)).collect());
                at += counts[k];
                k += 1;
            } else {
                out.push(vec![]);
            }
        }
        out
    }

    /// The game graph as a snapshot, its objects matched to ours by their place in the containers.
    fn snapshot(&mut self, ours: &Graph) -> Snap {
        let names = ["corners", "cornerPool", "squares", "squarePool", "voxelPool", "qubePool"];
        let ts: Vec<u64> = names.iter().map(|n| self.containers[n]).collect();
        let kids = self.children(&ts);
        let ours_lists: [Vec<u32>; 6] = [
            ours.corners.iter().map(|c| c.0).collect(),
            ours.corner_pool.iter().map(|c| c.0).collect(),
            ours.squares.iter().map(|c| c.0).collect(),
            ours.square_pool.iter().map(|c| c.0).collect(),
            ours.voxel_pool.iter().map(|c| c.0).collect(),
            ours.qube_pool.iter().map(|c| c.0).collect(),
        ];
        for k in 0..6 {
            assert_eq!(kids[k].len(), ours_lists[k].len(), "{} count", names[k]);
        }
        let corner_ts: Vec<u64> = kids[0].iter().chain(kids[1].iter()).copied().collect();
        let corner_ids: Vec<u32> = ours_lists[0].iter().chain(ours_lists[1].iter()).copied().collect();
        let square_ts: Vec<u64> = kids[2].iter().chain(kids[3].iter()).copied().collect();
        let square_ids: Vec<u32> = ours_lists[2].iter().chain(ours_lists[3].iter()).copied().collect();
        let corners = self.components(&corner_ts, "Placemaker.Graphs.Corner");
        let squares = self.components(&square_ts, "Placemaker.Graphs.Square");
        let voxel_pool = self.components(&kids[4], "Placemaker.Voxel");
        let qube_pool = self.components(&kids[5], "Placemaker.Graphs.Qube");
        // voxels under corners, qubes under squares
        let voxel_kids = self.children(&corner_ts);
        let qube_kids = self.children(&square_ts);
        let mut voxel_ts = vec![];
        let mut voxel_ids = vec![];
        for (k, c) in corner_ids.iter().enumerate() {
            let ov = &ours.corner(CornerId(*c)).voxels;
            assert_eq!(voxel_kids[k].len(), ov.len(), "voxels of corner {c}");
            voxel_ts.extend(voxel_kids[k].iter().copied());
            voxel_ids.extend(ov.iter().map(|v| v.0));
        }
        let mut qube_ts = vec![];
        let mut qube_ids = vec![];
        for (k, s) in square_ids.iter().enumerate() {
            let oq = &ours.square(SquareId(*s)).qubes;
            assert_eq!(qube_kids[k].len(), oq.len(), "qubes of square {s}");
            qube_ts.extend(qube_kids[k].iter().copied());
            qube_ids.extend(oq.iter().map(|q| q.0));
        }
        let voxels = self.components(&voxel_ts, "Placemaker.Voxel");
        let qubes = self.components(&qube_ts, "Placemaker.Graphs.Qube");
        // addresses to our ids
        let mut corner_map = HashMap::new();
        for (a, id) in self.addrs(&corners).into_iter().zip(&corner_ids) {
            corner_map.insert(a, *id);
        }
        let mut square_map = HashMap::new();
        for (a, id) in self.addrs(&squares).into_iter().zip(&square_ids) {
            square_map.insert(a, *id);
        }
        let mut voxel_map = HashMap::new();
        let all_voxels: Vec<u64> = voxels.iter().chain(voxel_pool.iter()).copied().collect();
        let all_voxel_ids: Vec<u32> = voxel_ids.iter().chain(ours_lists[4].iter()).copied().collect();
        for (a, id) in self.addrs(&all_voxels).into_iter().zip(&all_voxel_ids) {
            voxel_map.insert(a, *id);
        }
        let mut qube_map = HashMap::new();
        let all_qubes: Vec<u64> = qubes.iter().chain(qube_pool.iter()).copied().collect();
        let all_qube_ids: Vec<u32> = qube_ids.iter().chain(ours_lists[5].iter()).copied().collect();
        for (a, id) in self.addrs(&all_qubes).into_iter().zip(&all_qube_ids) {
            qube_map.insert(a, *id);
        }
        let ptr = |b: &[u8]| u32::from_le_bytes(b[..4].try_into().unwrap()) as u64;
        let mut s = Snap { corners: ours_lists[0].clone(), corner_pool: ours_lists[1].clone(), squares: ours_lists[2].clone(), square_pool: ours_lists[3].clone(), voxel_pool: ours_lists[4].clone(), qube_pool: ours_lists[5].clone(), ..Default::default() };
        // corners
        let cf = self.fields(&corners, &["hexPos", "angle", "groundCoverage", "inFlowUpdateQueue", "squares", "blitVerts", "flowDatas", "colliderMesh"]);
        let cpos = self.local_positions(&corner_ts);
        let sq_lists: Vec<Option<u64>> = cf.iter().map(|f| opt_handle(&f[4])).collect();
        let corner_squares = self.list_ids(&sq_lists, "Placemaker.Graphs.Square, Assembly-CSharp", &square_map);
        let blit = self.batch(cf.iter().map(|f| Game::call(handle(&f[5], "blitVerts"), &list("UnityEngine.Vector2, UnityEngine.CoreModule"), "ToArray", vec![], false)).collect(), "blitVerts");
        let flow = self.batch(cf.iter().map(|f| Game::call(handle(&f[6], "flowDatas"), &list("Placemaker.Graphs.FlowData, Assembly-CSharp"), "ToArray", vec![], false)).collect(), "flowDatas");
        let meshes: Vec<Option<u64>> = cf.iter().map(|f| opt_handle(&f[7])).collect();
        let real_meshes: Vec<u64> = meshes.iter().flatten().copied().collect();
        let mv = self.batch(real_meshes.iter().map(|&m| json!({"call": "UnityEngine.Mesh.get_vertices", "handle": m, "args": [], "main": true})).collect(), "vertices");
        let mt = self.batch(real_meshes.iter().map(|&m| json!({"call": "GetIndicesImpl", "class": "UnityEngine.Mesh", "handle": m, "args": [arg::i32(0), arg::bool(true)], "param_types": ["System.Int32", "System.Boolean"], "main": true})).collect(), "triangles");
        let mut mi = 0;
        for (k, id) in corner_ids.iter().enumerate() {
            let f = &cf[k];
            let hp = i32s(&ret::bytes(&f[0]));
            let fb = ret::bytes(&flow[k]);
            let flow: Vec<(u32, u8, Vec<u32>)> = fb
                .chunks(0x30)
                .map(|c| {
                    let a = ptr(c);
                    let v = if a == 0 { u32::MAX } else { *voxel_map.get(&a).unwrap_or(&(u32::MAX - 1)) };
                    let w = u32s(&c[8..0x2C]);
                    let mut bits = w.clone();
                    bits.push(c[0x2C] as u32);
                    (v, c[4], bits)
                })
                .collect();
            let mesh = meshes[k].map(|_| {
                let m = (u32s(&ret::bytes(&mv[mi])), i32s(&ret::bytes(&mt[mi])));
                mi += 1;
                m
            });
            let vk = &voxel_kids[k];
            let v_ids: Vec<u32> = ours.corner(CornerId(*id)).voxels.iter().map(|v| v.0).collect();
            assert_eq!(vk.len(), v_ids.len());
            s.corner_data.push((
                *id,
                CornerS {
                    hex: Int2 { x: hp[0], y: hp[1] },
                    angle: u32s(&ret::bytes(&f[1]))[0],
                    ground: ret::bytes(&f[2])[0],
                    in_flow: ret::bytes(&f[3])[0] != 0,
                    pos: Some(cpos[k]),
                    squares: corner_squares[k].clone(),
                    blit: u32s(&ret::bytes(&blit[k])),
                    flow,
                    mesh,
                    voxels: v_ids,
                },
            ));
        }
        // voxels (under corners, then the pool)
        let vf = self.fields(&all_voxels, &["type", "height", "cost"]);
        let vpos = self.local_positions(&voxel_ts);
        let mut by_id: Vec<(u32, VoxelS)> = vec![];
        for (k, id) in all_voxel_ids.iter().enumerate() {
            let f = &vf[k];
            by_id.push((*id, VoxelS { ty: ret::bytes(&f[0])[0], height: ret::bytes(&f[1])[0], cost: i32s(&ret::bytes(&f[2]))[0], pos: vpos.get(k).copied() }));
        }
        s.voxel_data = by_id;
        // squares and their qubes
        let sf = self.fields(&squares, &["q0", "q1", "q2", "q3", "m0", "m1", "m2", "m3", "motivations", "quadIndex", "c0", "c1", "c2", "c3"]);
        let spos = self.local_positions(&square_ts);
        let corner_refs: Vec<u64> = sf.iter().flat_map(|f| (10..14).map(move |i| opt_handle(&f[i]).unwrap_or(0))).collect();
        let real_refs: Vec<u64> = corner_refs.iter().copied().filter(|&h| h != 0).collect();
        let ref_addrs = self.addrs(&real_refs);
        let mut ra = ref_addrs.into_iter();
        let corner_of: Vec<u32> = corner_refs.iter().map(|&h| if h == 0 { u32::MAX } else { *corner_map.get(&ra.next().unwrap()).unwrap_or(&(u32::MAX - 1)) }).collect();
        let qf = self.fields(&all_qubes, &["voxelCount", "cornerValues", "existanceIndex", "shadowIndex", "lastPropagation", "awaitingGraphUpdate"]);
        let all_qube_ts: Vec<u64> = qube_ts.iter().chain(kids[5].iter()).copied().collect();
        let qpos = self.local_positions(&all_qube_ts);
        let mut qube_by_id: HashMap<u32, QubeS> = HashMap::new();
        for (k, id) in all_qube_ids.iter().enumerate() {
            let f = &qf[k];
            let cv = ret::bytes(&f[1]);
            qube_by_id.insert(
                *id,
                QubeS {
                    voxel_count: ret::bytes(&f[0])[0],
                    corner_values: cv[..8].try_into().unwrap(),
                    existance: u32s(&ret::bytes(&f[2]))[0],
                    shadow: i32s(&ret::bytes(&f[3]))[0],
                    last_propagation: u16::from_le_bytes(ret::bytes(&f[4])[..2].try_into().unwrap()),
                    awaiting_graph_update: ret::bytes(&f[5])[0] != 0,
                    pos: qpos[k],
                },
            );
        }
        for (k, id) in square_ids.iter().enumerate() {
            let f = &sf[k];
            let rel = |i: usize| {
                let b = ret::bytes(&f[i]);
                let a = ptr(&b);
                (if a == 0 { u32::MAX } else { *square_map.get(&a).unwrap_or(&(u32::MAX - 1)) }, b[4] as i8)
            };
            let m: Vec<u32> = (4..8).flat_map(|i| u32s(&ret::bytes(&f[i]))).collect();
            let q_ids: Vec<u32> = ours.square(SquareId(*id)).qubes.iter().map(|q| q.0).collect();
            s.square_data.push((
                *id,
                SquareS {
                    q: [rel(0), rel(1), rel(2), rel(3)],
                    c: [corner_of[k * 4], corner_of[k * 4 + 1], corner_of[k * 4 + 2], corner_of[k * 4 + 3]],
                    m,
                    motivations: ret::bytes(&f[8])[0],
                    quad_index: ret::bytes(&f[9])[0],
                    pos: spos[k],
                    qubes: q_ids.clone(),
                },
            ));
            for q in q_ids {
                s.qube_data.push((q, qube_by_id[&q].clone()));
            }
        }
        for q in &ours_lists[5] {
            s.qube_data.push((*q, qube_by_id[q].clone()));
        }
        // the graph's own fields
        let gr = self.graph;
        let gf = self.fields(&[gr], &["cornerIntegrationQueue", "cornerFlowQueue", "qubeUpdateQueue", "shadowMeshIndexes", "cornerDict", "squareDict", "edgeDict", "totalVoxelCount", "boundsHeight", "bounds", "boundsUpdated", "recalculateHexBounds", "recalculateHeightBounds"]);
        let gf = &gf[0];
        let ql = self.list_ids(&[opt_handle(&gf[0]), opt_handle(&gf[1])], "Placemaker.Graphs.Corner, Assembly-CSharp", &corner_map);
        s.integration_queue = ql[0].clone();
        s.flow_queue = ql[1].clone();
        s.qube_update_queue = self.list_ids(&[opt_handle(&gf[2])], "Placemaker.Graphs.Qube, Assembly-CSharp", &qube_map)[0].clone();
        s.shadow_indexes = i32s(&ret::bytes(&self.batch(vec![Game::call(handle(&gf[3], "shadow"), &list("System.Int32"), "ToArray", vec![], false)], "shadow")[0]));
        let ci = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2, Unity.Mathematics],[Placemaker.Graphs.Corner, Assembly-CSharp]]";
        let si = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2, Unity.Mathematics],[Placemaker.Graphs.Square, Assembly-CSharp]]";
        let ei = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2x2, Unity.Mathematics],[Placemaker.Graphs.Square+Relation, Assembly-CSharp]]";
        let (cd, sd, ed) = (handle(&gf[4], "cornerDict"), handle(&gf[5], "squareDict"), handle(&gf[6], "edgeDict"));
        let counts = self.batch(vec![Game::call(cd, ci, "get_Count", vec![], false), Game::call(sd, si, "get_Count", vec![], false), Game::call(ed, ei, "get_Count", vec![], false)], "dict counts");
        s.dict_counts = [ret::i32(&counts[0]) as usize, ret::i32(&counts[1]) as usize, ret::i32(&counts[2]) as usize];
        // the dictionaries' values at our keys
        let mut ck: Vec<Int2> = ours.corner_dict.keys().copied().collect();
        ck.sort_by_key(|k| (k.x, k.y));
        let cv: Vec<u64> = self.batch(ck.iter().map(|k| Game::call(cd, ci, "get_Item", vec![arg::raw(&bytes(k))], true)).collect(), "cornerDict[k]").iter().map(|r| handle(r, "corner")).collect();
        let cv = self.addrs(&cv);
        s.corner_dict = ck.iter().zip(cv).map(|(k, a)| (*k, *corner_map.get(&a).unwrap_or(&(u32::MAX - 1)))).collect();
        let mut sk: Vec<Int2> = ours.square_dict.keys().copied().collect();
        sk.sort_by_key(|k| (k.x, k.y));
        let sv: Vec<u64> = self.batch(sk.iter().map(|k| Game::call(sd, si, "get_Item", vec![arg::raw(&bytes(k))], true)).collect(), "squareDict[k]").iter().map(|r| handle(r, "square")).collect();
        let sv = self.addrs(&sv);
        s.square_dict = sk.iter().zip(sv).map(|(k, a)| (*k, *square_map.get(&a).unwrap_or(&(u32::MAX - 1)))).collect();
        let mut ek: Vec<(Int2, Int2)> = ours.edge_dict.keys().copied().collect();
        ek.sort_by_key(|(a, b)| (a.x, a.y, b.x, b.y));
        let ev = self.batch(ek.iter().map(|(a, b)| Game::call(ed, ei, "get_Item", vec![arg::raw(&bits_of(&[a.x, a.y, b.x, b.y]))], false)).collect(), "edgeDict[k]");
        s.edge_dict = ek
            .iter()
            .zip(ev)
            .map(|(k, r)| {
                let b = ret::bytes(&r);
                let a = ptr(&b);
                (*k, (if a == 0 { u32::MAX } else { *square_map.get(&a).unwrap_or(&(u32::MAX - 1)) }, b[4] as i8))
            })
            .collect();
        let b = i32s(&ret::bytes(&gf[9]));
        s.scalars = vec![
            i32s(&ret::bytes(&gf[7]))[0],
            i32s(&ret::bytes(&gf[8]))[0],
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            ret::bytes(&gf[10])[0] as i32,
            ret::bytes(&gf[11])[0] as i32,
            ret::bytes(&gf[12])[0] as i32,
        ];
        s
    }
}

/// Our snapshot with what the game cannot show taken out: pooled voxels have no position here.
fn same(game: &Snap, ours: &Snap, what: &str) {
    let mut ours_v = ours.voxel_data.clone();
    let pool: Vec<u32> = ours.voxel_pool.clone();
    for (id, v) in ours_v.iter_mut() {
        if pool.contains(id) {
            v.pos = None;
        }
    }
    let mut game_v = game.voxel_data.clone();
    for (id, v) in game_v.iter_mut() {
        if pool.contains(id) {
            v.pos = None;
        }
    }
    let sort = |v: &mut Vec<(u32, VoxelS)>| v.sort_by_key(|(k, _)| *k);
    sort(&mut ours_v);
    sort(&mut game_v);
    for (a, b) in game_v.iter().zip(&ours_v) {
        assert_eq!(a, b, "{what}: voxel");
    }
    assert_eq!(game_v.len(), ours_v.len(), "{what}: voxel count");
    let mut gc = game.corner_data.clone();
    let mut oc = ours.corner_data.clone();
    gc.sort_by_key(|(k, _)| *k);
    oc.sort_by_key(|(k, _)| *k);
    for (a, b) in gc.iter().zip(&oc) {
        assert_eq!(a, b, "{what}: corner");
    }
    for (a, b) in game.square_data.iter().zip(&ours.square_data) {
        assert_eq!(a, b, "{what}: square");
    }
    for (a, b) in game.qube_data.iter().zip(&ours.qube_data) {
        assert_eq!(a, b, "{what}: qube");
    }
    assert_eq!(game.integration_queue, ours.integration_queue, "{what}: cornerIntegrationQueue");
    assert_eq!(game.flow_queue, ours.flow_queue, "{what}: cornerFlowQueue");
    assert_eq!(game.qube_update_queue, ours.qube_update_queue, "{what}: qubeUpdateQueue");
    assert_eq!(game.shadow_indexes, ours.shadow_indexes, "{what}: shadowMeshIndexes");
    assert_eq!(game.dict_counts, ours.dict_counts, "{what}: dictionary counts");
    assert_eq!(game.corner_dict, ours.corner_dict, "{what}: cornerDict");
    assert_eq!(game.square_dict, ours.square_dict, "{what}: squareDict");
    assert_eq!(game.edge_dict, ours.edge_dict, "{what}: edgeDict");
    assert_eq!(game.scalars, ours.scalars, "{what}: totalVoxelCount, boundsHeight, bounds, flags");
}

/// The grid both sides build on, and the stand-in master and test graph in the game.
fn setup(h: &mut Harness) -> (Game<'_>, GridGenerator) {
    h.wait_for_object(GEN);
    let scene = |h: &mut Harness, on: &str, f: &str| handle(&ok(h.request(json!({"field": f, "on": on, "main": true})), f), f);
    let scene_master = scene(h, GEN, "master");
    let lib = scene(h, GEN, "customGridLibrary");
    let field_of = |h: &mut Harness, obj: u64, f: &str| handle(&ok(h.request(json!({"field": f, "handle": obj, "main": true})), f), f);
    let set = |h: &mut Harness, obj: u64, f: &str, v: u64| {
        ok(h.request(json!({"field": f, "handle": obj, "set_handle": v, "main": true})), f);
    };
    let new_go = |h: &mut Harness, name: &str| {
        let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true})), "go"), "go");
        handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t")
    };
    // the grid, as in placemaker_bits.rs
    let gen = handle(&ok(h.request(json!({"new": GEN, "args": [], "main": true})), "new GridGenerator"), GEN);
    let bit_container = new_go(h, "ts_check editing bits");
    set(h, gen, "master", scene_master);
    set(h, gen, "customGridLibrary", lib);
    set(h, gen, "bitContainer", bit_container);
    let ty = |h: &mut Harness, n: &str| handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n);
    let func = ty(h, "System.Func`1[[System.Boolean]]");
    let app = ty(h, "UnityEngine.Application, UnityEngine.CoreModule");
    let mi = handle(&ok(h.request(json!({"call": "GetMethod", "class": "System.Type", "handle": app, "args": [arg::string("get_isPlaying")], "param_types": ["System.String"], "main": true, "keep": true})), "GetMethod"), "MethodInfo");
    let keep_going = handle(&ok(h.request(json!({"call": "System.Delegate.CreateDelegate", "args": [{"handle": func}, {"handle": mi}], "param_types": ["System.Type", "System.Reflection.MethodInfo"], "main": true, "keep": true})), "CreateDelegate"), "delegate");
    let mut grid = GridGenerator { custom_grid_library: our_library(), ..Default::default() };
    let scene_graph = field_of(h, scene_master, "graph");
    let b = i32s(&ret::bytes(&ok(h.request(json!({"field": "genBounds", "handle": scene_graph, "main": true})), "genBounds")));
    let bounds = Int3x2 { c0: i3(b[0], b[1], b[2]), c1: i3(b[3], b[4], b[5]) };
    grid.iterate_focus(bounds);
    ok(h.request(json!({"call": format!("{GEN}.IterateFocus"), "handle": gen, "args": [], "main": true})), "IterateFocus");
    while grid.iterate_bits(&mut || true) {}
    while ret::bytes(&ok(h.request(json!({"call": format!("{GEN}.IterateBits"), "handle": gen, "args": [{"handle": keep_going}], "main": true})), "IterateBits"))[0] != 0 {}
    // the stand-in master
    let master = handle(&ok(h.request(json!({"new": "Placemaker.WorldMaster", "args": [], "main": true})), "new WorldMaster"), "master");
    let graph = handle(&ok(h.request(json!({"new": G, "args": [], "main": true})), "new Graph"), "graph");
    let wfc = handle(&ok(h.request(json!({"new": "Placemaker.WaveFunctionCollapse", "args": [], "main": true})), "new WaveFunctionCollapse"), "wfc");
    let mb = handle(&ok(h.request(json!({"new": "Placemaker.ModuleBuilder", "args": [], "main": true})), "new ModuleBuilder"), "mb");
    set(h, mb, "master", master);
    set(h, wfc, "master", master);
    set(h, master, "grid", gen);
    set(h, master, "graph", graph);
    set(h, master, "waveFunctionCollapse", wfc);
    set(h, master, "moduleBuilder", mb);
    for f in ["uiMaster", "maker", "moduleLibrary", "worldMeshes", "materialMaster", "clickEffect", "sandbanker", "palette", "miscReferences"] {
        let v = field_of(h, scene_master, f);
        set(h, master, f, v);
    }
    set(h, graph, "master", master);
    let mut containers = HashMap::new();
    for name in ["corners", "cornerPool", "squares", "squarePool", "voxelPool", "looseVoxels", "qubePool"] {
        let t = new_go(h, name);
        set(h, graph, name, t);
        containers.insert(name, t);
    }
    let mut types = HashMap::new();
    for c in ["Placemaker.Graphs.Corner", "Placemaker.Graphs.Square", "Placemaker.Graphs.Qube", "Placemaker.Voxel"] {
        types.insert(c, ty(h, &format!("{c}, Assembly-CSharp")));
    }
    (Game { h, gen, graph, containers, types }, grid)
}

#[test]
fn editing_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid) = setup(&mut h);
    let _ = g.gen;
    let mut ours = Graph::default();
    let mut host = Recorder::default();
    // pools first (the game's keepGoing is a time budget: fill both to the end)
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    same(&g.snapshot(&ours), &ours_snapshot(&ours), "refilled pools");
    // corners: the corners of grid quads well inside the finished bits, in a small patch so squares are shared
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(91);
    let mut verts: Vec<Int2> = Vec::new();
    for _ in 0..200 {
        let q = grid.get_quad(f2(r.f(cx - 3.0, cx + 3.0), r.f(cz - 3.0, cz + 3.0)));
        if q.is_valid() {
            for v in [q.v0, q.v1, q.v2, q.v3] {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
    }
    eprintln!("{} corner positions", verts.len());
    assert!(verts.len() > 12, "the patch should have corners");
    let mut action_id = 0;
    for step in 0..160 {
        let existing: Vec<(Int2, VoxelId)> = ours.corners.iter().flat_map(|&c| ours.corner(c).voxels.iter().map(move |&v| (c, v))).map(|(c, v)| (ours.corner(c).hex_pos, v)).collect();
        let kind = r.next() % 8;
        let play_effect = r.next() % 4 != 0;
        let action = if existing.is_empty() || kind < 4 {
            let hex = r.pick(&verts);
            let height = (r.next() % 5) as u8;
            Action::new(hex, height, VoxelType::Empty, VoxelType::from_i8((r.next() % 15) as i8), action_id)
        } else {
            let (hex, v) = r.pick(&existing);
            let (t, height) = (ours.voxel(v).voxel_type, ours.voxel(v).height);
            if kind < 6 {
                Action::new(hex, height, t, VoxelType::from_i8((r.next() % 15) as i8), action_id)
            } else {
                Action::new(hex, height, t, VoxelType::Empty, action_id)
            }
        };
        action_id += 1;
        ours.apply_action(&mut grid, &mut host, action, play_effect);
        g.g("ApplyAction", vec![arg::raw(&bytes(&action)), arg::bool(play_effect)]);
        // now and then integrate the queued corners that still have voxels, as the graph's update would
        if step % 7 == 6 && !ours.corner_integration_queue.is_empty() {
            let queue = ours.corner_integration_queue.clone();
            let game_queue = handle(&ok(g.h.request(Game::field(g.graph, "cornerIntegrationQueue")), "queue"), "queue");
            let lc = list("Placemaker.Graphs.Corner, Assembly-CSharp");
            let items: Vec<u64> = (0..queue.len()).map(|i| handle(&ok(g.h.request(Game::call(game_queue, &lc, "get_Item", vec![arg::i32(i as i32)], true)), "item"), "item")).collect();
            ok(g.h.request(Game::call(game_queue, &lc, "Clear", vec![], false)), "Clear");
            ours.corner_integration_queue.clear();
            for (c, gc) in queue.into_iter().zip(items) {
                if ours.corner(c).voxels.is_empty() {
                    continue;
                }
                let o = ours.integrate_corner(&mut grid, &mut host, c);
                let gr = ret::bytes(&g.g("IntegrateCorner", vec![json!({"handle": gc})]))[0] != 0;
                assert_eq!(gr, o, "IntegrateCorner return");
            }
        }
        // the whole graph after each of the first steps, then every 10th (a snapshot is thousands of requests)
        if step < 24 || step % 10 == 9 {
            let what = format!("step {step}: {action:?} play_effect {play_effect}");
            same(&g.snapshot(&ours), &ours_snapshot(&ours), &what);
            eprintln!("step {step}: same ({} corners, {} squares, {} voxels)", ours.corners.len(), ours.squares.len(), ours.total_voxel_count);
        }
    }
    let adds = host.calls.iter().filter(|c| c.starts_with("GetMesh")).count();
    let removed = host.calls.iter().filter(|c| c.starts_with("QubeRemoved")).count();
    eprintln!("{} corner meshes, {} qubes removed, {} calls in all", adds, removed, host.calls.len());
    assert!(adds > 5 && removed > 5, "integration and removal should both happen");
}

