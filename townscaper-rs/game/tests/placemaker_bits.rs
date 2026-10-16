//! `GridGenerator`'s clusters and bits against the original game (E3d). Needs `TS_HARNESS=1` and the player's
//! install. A fresh `GridGenerator` (made with `new`) gets the scene's `WorldMaster`, the patch library and its
//! own bit container; `keepGoing` is a delegate on `Application.isPlaying` (always true), so every relaxation
//! runs to the end in one call on both sides and every step can be compared.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::grid_generator::{self as gg, BitMesh, GridBit, GridBitState, GridGenerator};
use ts_game::placemaker::quads::grid_generation::{CustomGridLibrary, CustomGridPatch, HexCluster, HexPatch};
use ts_game::unity::mathematics::{f2, i3, Float2, Int2, Int3x2};
use ts_game::unity::vector::{v2, v3, Vector2, Vector3};

const G: &str = "Placemaker.GridGenerator";
fn list(t: &str) -> String {
    format!("System.Collections.Generic.List`1[[{t}]]")
}
const INT2: &str = "Unity.Mathematics.int2, Unity.Mathematics";
const FLOAT2: &str = "Unity.Mathematics.float2, Unity.Mathematics";

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}
fn i2b(v: Int2) -> Vec<u8> {
    [v.x.to_le_bytes(), v.y.to_le_bytes()].concat()
}
fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn i32s(b: &[u8]) -> Vec<i32> {
    b.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits_of<T: Copy>(v: &[T]) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }.to_vec()
}

struct Game<'a> {
    h: &'a mut Harness,
    gen: u64,
    keep_going: u64,
    mesh_filter_type: u64,
}

impl Game<'_> {
    fn req(&mut self, r: Value, what: &str) -> Value {
        ok(self.h.request(r), what)
    }
    fn field(&mut self, obj: u64, name: &str) -> Value {
        self.req(json!({"field": name, "handle": obj, "main": true}), name)
    }
    fn raw(&mut self, obj: u64, name: &str) -> Vec<u8> {
        ret::bytes(&self.field(obj, name))
    }
    fn obj(&mut self, obj: u64, name: &str) -> u64 {
        let r = self.field(obj, name);
        handle(&r, name)
    }
    fn call(&mut self, obj: u64, class: Option<&str>, m: &str, args: Vec<Value>, types: &[&str], keep: bool) -> Value {
        let mut r = json!({"call": m, "handle": obj, "args": args, "main": true, "keep": keep});
        if let Some(c) = class {
            r["class"] = json!(c);
        }
        if !types.is_empty() {
            r["param_types"] = json!(types);
        }
        self.req(r, m)
    }
    fn gen_call(&mut self, m: &str, args: Vec<Value>, keep: bool) -> Value {
        let gen = self.gen;
        self.req(json!({"call": format!("{G}.{m}"), "handle": gen, "args": args, "main": true, "keep": keep}), m)
    }
    fn count(&mut self, l: u64, class: &str) -> usize {
        ret::i32(&self.call(l, Some(class), "get_Count", vec![], &[], false)) as usize
    }
    fn item(&mut self, l: u64, class: &str, i: usize) -> u64 {
        let r = self.call(l, Some(class), "get_Item", vec![arg::i32(i as i32)], &[], true);
        handle(&r, "get_Item")
    }
    fn to_array(&mut self, l: u64, class: &str) -> Vec<u8> {
        ret::bytes(&self.call(l, Some(class), "ToArray", vec![], &[], false))
    }
    fn list_field(&mut self, obj: u64, name: &str, elem: &str) -> Vec<u8> {
        let l = self.obj(obj, name);
        self.to_array(l, &list(elem))
    }
    /// `Mesh.GetUVs(channel, List<Vector2>)` (the `uv` properties are stripped from the build).
    fn uvs(&mut self, mesh: u64, channel: i32) -> Vec<u8> {
        let lc = list("UnityEngine.Vector2, UnityEngine.CoreModule");
        let l = handle(&self.req(json!({"new": lc, "args": [], "main": true}), "new List<Vector2>"), "list");
        self.call(mesh, Some("UnityEngine.Mesh"), "GetUVs", vec![arg::i32(channel), json!({"handle": l})], &["System.Int32", "System.Collections.Generic.List<UnityEngine.Vector2>"], false);
        self.to_array(l, &lc)
    }
    fn array(&mut self, obj: u64, m: &str) -> Vec<u8> {
        let r = self.call(obj, None, m, vec![], &[], false);
        ret::bytes(&r)
    }
}

fn float2s(b: &[u8]) -> Vec<Float2> {
    f32s(b).chunks(2).map(|c| f2(c[0], c[1])).collect()
}
fn int2s(b: &[u8]) -> Vec<Int2> {
    i32s(b).chunks(2).map(|c| Int2 { x: c[0], y: c[1] }).collect()
}

/// The game generator's state in our types (lists by index, as ours).
#[derive(Debug, PartialEq)]
struct Snapshot {
    bits: Vec<GridBit>,
    clusters: Vec<HexCluster>,
    patches: Vec<(Int2, u8, u8, u16, u8, Vec<Int2>, Vec<u8>)>,
    deltas: Vec<Float2>,
    bit_process_index: u16,
    hex_origo: Int2,
    focus_updated: bool,
}

fn snapshot(g: &mut Game, meshes: bool) -> Snapshot {
    let gen = g.gen;
    let mut out = Snapshot {
        bits: vec![],
        clusters: vec![],
        patches: vec![],
        deltas: float2s(&g.list_field(gen, "deltas", FLOAT2)),
        bit_process_index: u16::from_le_bytes(g.raw(gen, "bitProcessIndex")[..2].try_into().unwrap()),
        hex_origo: int2s(&g.raw(gen, "hexOrigo"))[0],
        focus_updated: g.raw(gen, "focusUpdated")[0] != 0,
    };
    let bits = g.obj(gen, "bits");
    let bl = list("Placemaker.GridBit, Assembly-CSharp");
    for i in 0..g.count(bits, &bl) {
        let b = g.item(bits, &bl, i);
        let go = handle(&g.call(b, Some("UnityEngine.Component"), "get_gameObject", vec![], &[], true), "gameObject");
        let tr = handle(&g.call(b, Some("UnityEngine.Component"), "get_transform", vec![], &[], true), "transform");
        let p = f32s(&ret::bytes(&g.call(tr, Some("UnityEngine.Transform"), "get_position", vec![], &[], false)));
        let mut bit = GridBit {
            hex_pos: int2s(&g.raw(b, "hexPos"))[0],
            motivations: g.raw(b, "motivations")[0],
            relaxation_steps: g.raw(b, "relaxationSteps")[0],
            cluster_count: g.raw(b, "clusterCount")[0],
            patch_done: g.raw(b, "patchDone")[0] != 0,
            state: [GridBitState::Verts, GridBitState::Relax, GridBitState::Clusters, GridBitState::Compose, GridBitState::Mesh, GridBitState::Done][g.raw(b, "state")[0] as usize],
            verts: float2s(&g.list_field(b, "verts", FLOAT2)),
            name: ret::string(&g.call(b, Some("UnityEngine.Object"), "get_name", vec![], &[], false)),
            active: ret::bytes(&g.call(go, Some("UnityEngine.GameObject"), "get_activeSelf", vec![], &[], false))[0] != 0,
            position: v3(p[0], p[1], p[2]),
            meshes: vec![],
        };
        if meshes {
            let n = ret::i32(&g.call(tr, Some("UnityEngine.Transform"), "get_childCount", vec![], &[], false));
            for c in 0..n {
                let child = handle(&g.call(tr, Some("UnityEngine.Transform"), "GetChild", vec![arg::i32(c)], &[], true), "child");
                let mft = g.mesh_filter_type;
                let mf = handle(&g.call(child, Some("UnityEngine.Component"), "GetComponent", vec![json!({"handle": mft})], &["System.Type"], true), "MeshFilter");
                let mesh = handle(&g.call(mf, Some("UnityEngine.MeshFilter"), "get_sharedMesh", vec![], &[], true), "mesh");
                let vs = f32s(&g.array(mesh, "UnityEngine.Mesh.get_vertices"));
                let uv0 = f32s(&g.uvs(mesh, 0));
                let uv1 = f32s(&g.uvs(mesh, 1));
                bit.meshes.push(BitMesh {
                    vertices: vs.chunks(3).map(|c| v3(c[0], c[1], c[2])).collect(),
                    uv0: uv0.chunks(2).map(|c| v2(c[0], c[1])).collect(),
                    uv1: uv1.chunks(2).map(|c| v2(c[0], c[1])).collect(),
                    triangles: i32s(&ret::bytes(&g.call(mesh, Some("UnityEngine.Mesh"), "GetIndicesImpl", vec![arg::i32(0), arg::bool(true)], &["System.Int32", "System.Boolean"], false))),
                });
            }
        }
        out.bits.push(bit);
    }
    let cl = g.obj(gen, "clusters");
    let cll = list("Placemaker.Quads.GridGeneration.HexCluster, Assembly-CSharp");
    for i in 0..g.count(cl, &cll) {
        let c = g.item(cl, &cll, i);
        let o = i32s(&g.raw(c, "patchVertOffsets"));
        let vi = g.list_field(c, "vertIndexes", "System.Int16");
        out.clusters.push(HexCluster {
            hex_pos: int2s(&g.raw(c, "hexPos"))[0],
            motivations: ts_game::placemaker::quads::grid_generation::MotivationCounter { count: g.raw(c, "motivations")[0] },
            relaxation_steps: g.raw(c, "relaxationSteps")[0],
            patch_count: g.raw(c, "patchCount")[0],
            done: g.raw(c, "done")[0] != 0,
            patch_vert_offsets: i3(o[0], o[1], o[2]),
            vert_indexes: vi.chunks(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect(),
            relaxed_verts: float2s(&g.list_field(c, "relaxedVerts", FLOAT2)),
        });
    }
    let pl = g.obj(gen, "patches");
    let pll = list("Placemaker.Quads.GridGeneration.HexPatch, Assembly-CSharp");
    for i in 0..g.count(pl, &pll) {
        let p = g.item(pl, &pll, i);
        let cut = g.raw(p, "quadTriCutoff");
        out.patches.push((
            int2s(&g.raw(p, "hexPos"))[0],
            g.raw(p, "motivations")[0],
            g.raw(p, "state")[0],
            u16::from_le_bytes([cut[0], cut[1]]),
            g.raw(p, "subdivisions")[0],
            int2s(&g.list_field(p, "verts", INT2)),
            g.list_field(p, "quads", "System.Byte"),
        ));
    }
    out
}

fn ours_snapshot(o: &GridGenerator, meshes: bool) -> Snapshot {
    let p = |p: &HexPatch| (p.hex_pos, p.motivations.count, p.state as u8, p.quad_tri_cutoff, p.subdivisions, p.verts.clone(), p.quads.clone());
    Snapshot {
        bits: o.bits.iter().map(|b| GridBit { meshes: if meshes { b.meshes.clone() } else { vec![] }, ..b.clone() }).collect(),
        clusters: o.clusters.clone(),
        patches: o.patches.iter().map(p).collect(),
        deltas: o.deltas.clone(),
        bit_process_index: o.bit_process_index,
        hex_origo: o.hex_origo,
        focus_updated: o.focus_updated,
    }
}

/// Field-by-field comparison with float lists compared bit for bit (NaN-safe) and a readable first difference.
fn same(game: &Snapshot, ours: &Snapshot, what: &str) {
    let key = |s: &Snapshot| format!("{s:?}");
    if key(game) == key(ours) {
        return;
    }
    assert_eq!(game.bits.len(), ours.bits.len(), "{what}: bit count");
    for (i, (a, b)) in game.bits.iter().zip(&ours.bits).enumerate() {
        assert_eq!(GridBit { verts: vec![], meshes: vec![], ..a.clone() }, GridBit { verts: vec![], meshes: vec![], ..b.clone() }, "{what}: bit {i}");
        assert_eq!(bits_of(&a.verts), bits_of(&b.verts), "{what}: bit {i} verts\ngame {:?}\nours {:?}", a.verts, b.verts);
        assert_eq!(a.meshes.len(), b.meshes.len(), "{what}: bit {i} mesh count");
        for (k, (ma, mb)) in a.meshes.iter().zip(&b.meshes).enumerate() {
            assert_eq!(bits_of(&ma.vertices), bits_of(&mb.vertices), "{what}: bit {i} mesh {k} vertices");
            assert_eq!(bits_of(&ma.uv0), bits_of(&mb.uv0), "{what}: bit {i} mesh {k} uv0");
            assert_eq!(bits_of(&ma.uv1), bits_of(&mb.uv1), "{what}: bit {i} mesh {k} uv1");
            assert_eq!(ma.triangles, mb.triangles, "{what}: bit {i} mesh {k} triangles");
        }
    }
    assert_eq!(game.clusters.len(), ours.clusters.len(), "{what}: cluster count");
    for (i, (a, b)) in game.clusters.iter().zip(&ours.clusters).enumerate() {
        assert_eq!(HexCluster { relaxed_verts: vec![], ..a.clone() }, HexCluster { relaxed_verts: vec![], ..b.clone() }, "{what}: cluster {i}");
        assert_eq!(bits_of(&a.relaxed_verts), bits_of(&b.relaxed_verts), "{what}: cluster {i} relaxedVerts");
    }
    assert_eq!(game.patches, ours.patches, "{what}: patches");
    assert_eq!(bits_of(&game.deltas), bits_of(&ours.deltas), "{what}: deltas");
    assert_eq!((game.bit_process_index, game.hex_origo, game.focus_updated), (ours.bit_process_index, ours.hex_origo, ours.focus_updated), "{what}: generator fields");
}

fn our_library() -> CustomGridLibrary {
    let dir = ts_data::install::find_dir().expect("the player's install (TOWNSCAPER_DIR)");
    let a = ts_data::objects::AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let d = ts_data::grid::custom_grid_library(&a).expect("CustomGridLibrary");
    CustomGridLibrary {
        all_custom_patches: d.all_custom_patches.into_iter().map(|p| CustomGridPatch {
            verts: p.verts.into_iter().map(|[x, y]| Int2 { x, y }).collect(),
            quads: p.quads,
            quad_tri_cutoff: p.quad_tri_cutoff,
            subdivisions: p.subdivisions,
        }).collect(),
        generic_patches: d.generic_patches,
        straight_patches: d.straight_patches,
        centroid_patches: d.centroid_patches,
        centroid_curve_patches: d.centroid_curve_patches,
    }
}

/// A fresh game `GridGenerator` wired to the scene, and the always-true `keepGoing` delegate.
fn setup(h: &mut Harness) -> Game<'_> {
    let scene_gen = |h: &mut Harness, f: &str| handle(&ok(h.request(json!({"field": f, "on": G, "main": true})), f), f);
    let master = scene_gen(h, "master");
    let lib = scene_gen(h, "customGridLibrary");
    let gen = handle(&ok(h.request(json!({"new": G, "args": [], "main": true})), "new GridGenerator"), G);
    let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string("ts_check bits")], "param_types": ["System.String"], "main": true})), "new GameObject"), "GameObject");
    let container = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "transform"), "transform");
    for (f, v) in [("master", master), ("customGridLibrary", lib), ("bitContainer", container)] {
        ok(h.request(json!({"field": f, "handle": gen, "set_handle": v, "main": true})), f);
    }
    let ty = |h: &mut Harness, n: &str| handle(&ok(h.request(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true})), n), n);
    let func = ty(h, "System.Func`1[[System.Boolean]]");
    let app = ty(h, "UnityEngine.Application, UnityEngine.CoreModule");
    let mesh_filter_type = ty(h, "UnityEngine.MeshFilter, UnityEngine.CoreModule");
    let mi = handle(&ok(h.request(json!({"call": "GetMethod", "class": "System.Type", "handle": app, "args": [arg::string("get_isPlaying")], "param_types": ["System.String"], "main": true, "keep": true})), "GetMethod"), "MethodInfo");
    let keep_going = handle(&ok(h.request(json!({"call": "System.Delegate.CreateDelegate", "args": [{"handle": func}, {"handle": mi}], "param_types": ["System.Type", "System.Reflection.MethodInfo"], "main": true, "keep": true})), "CreateDelegate"), "delegate");
    Game { h, gen, keep_going, mesh_filter_type }
}

#[test]
fn one_bit_from_scratch_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = GridGenerator { custom_grid_library: our_library(), ..Default::default() };
    // bits sit at `hexOrigo + HexOnionIterator.dualPos2 * 72` (a position off that lattice throws
    // ArgumentOutOfRangeException in the game's Compose step, and panics in ours)
    let positions = [Int2 { x: 0, y: 0 }, gg::get_starting_hex(1), gg::get_starting_hex(25), gg::get_starting_hex(60)];
    for (k, pos) in positions.into_iter().enumerate() {
        let b = ours.motivate_bit(pos);
        let bit = handle(&g.gen_call("MotivateBit", vec![arg::raw(&i2b(pos))], true), "MotivateBit");
        same(&snapshot(&mut g, false), &ours_snapshot(&ours, false), &format!("bit {k} motivated"));
        let mut step = 0;
        while ours.bits[b].state != GridBitState::Done {
            let before = ours.bits[b].state;
            let kg = g.keep_going;
            g.gen_call("IterateBit", vec![json!({"handle": bit}), json!({"handle": kg})], false);
            ours.iterate_bit(b, &mut || true);
            step += 1;
            // the full state at every bit state change, the bit's own fields every step
            if ours.bits[b].state != before || step % 25 == 0 {
                let meshes = ours.bits[b].state == GridBitState::Done;
                same(&snapshot(&mut g, meshes), &ours_snapshot(&ours, meshes), &format!("bit {k} step {step} ({before:?} -> {:?})", ours.bits[b].state));
            }
            assert!(step < 5000, "bit {k} never finished");
        }
        eprintln!("bit {k} at {pos:?}: {step} steps, {} meshes", ours.bits[b].meshes.len());
    }
}

#[test]
fn focus_and_bits_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = GridGenerator { custom_grid_library: our_library(), ..Default::default() };
    // the scene's Graph.genBounds, then a moved focus
    let master = g.obj(g.gen, "master");
    let graph = g.obj(master, "graph");
    let b = i32s(&g.raw(graph, "genBounds"));
    let bounds = Int3x2 { c0: i3(b[0], b[1], b[2]), c1: i3(b[3], b[4], b[5]) };
    let moved = Int3x2 { c0: i3(b[0] + 300, b[1], b[2] - 300), c1: i3(b[3] + 300, b[4], b[5] - 300) };
    let gen = g.gen;
    for (round, bounds) in [bounds, moved].into_iter().enumerate() {
        // IterateFocus reads the graph's bounds: set them in the game's graph for the round
        g.req(json!({"field": "genBounds", "handle": graph, "set_raw": hexs(&bits_of(&[bounds.c0.x, bounds.c0.y, bounds.c0.z, bounds.c1.x, bounds.c1.y, bounds.c1.z])), "main": true}), "set genBounds");
        g.req(json!({"field": "focusUpdated", "handle": gen, "set_raw": "01", "main": true}), "set focusUpdated");
        ours.focus_updated = true;
        let r = ours.iterate_focus(bounds);
        assert_eq!(ret::bytes(&g.gen_call("IterateFocus", vec![], false))[0] != 0, r, "IterateFocus");
        same(&snapshot(&mut g, false), &ours_snapshot(&ours, false), &format!("round {round} focus"));
        let mut steps = 0;
        loop {
            let r = ours.iterate_bits(&mut || true);
            let kg = g.keep_going;
            let gr = ret::bytes(&g.gen_call("IterateBits", vec![json!({"handle": kg})], false))[0] != 0;
            assert_eq!(gr, r, "round {round} IterateBits step {steps}");
            steps += 1;
            if !r || steps % 400 == 0 {
                same(&snapshot(&mut g, !r), &ours_snapshot(&ours, !r), &format!("round {round} IterateBits step {steps}"));
            }
            if !r {
                break;
            }
        }
        eprintln!("round {round}: {} bits, {} clusters, {} patches, {steps} steps", ours.bits.len(), ours.clusters.len(), ours.patches.len());
    }
    // OnEnable: rebuilds the dictionaries; a bit set part-way is started over
    if let Some(i) = ours.bits.iter().position(|b| b.state == GridBitState::Done) {
        ours.bits[i].relaxation_steps = 7;
        let bl = list("Placemaker.GridBit, Assembly-CSharp");
        let bits = g.obj(gen, "bits");
        let bh = g.item(bits, &bl, i);
        g.req(json!({"field": "relaxationSteps", "handle": bh, "set_raw": "07", "main": true}), "set relaxationSteps");
    }
    ours.on_enable();
    g.gen_call("OnEnable", vec![], false);
    same(&snapshot(&mut g, false), &ours_snapshot(&ours, false), "OnEnable");
    let _ = (Vector2::ZERO, Vector3::ZERO);
}

#[test]
fn scene_process_iteration_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = GridGenerator { custom_grid_library: our_library(), ..Default::default() };
    // something to clear first
    for pos in [gg::get_starting_hex(3), gg::get_starting_hex(9)] {
        ours.motivate_bit(pos);
        g.gen_call("MotivateBit", vec![arg::raw(&i2b(pos))], false);
    }
    ours.scene_process_iteration();
    g.gen_call("SceneProcessIteration", vec![], false);
    same(&snapshot(&mut g, false), &ours_snapshot(&ours, false), "SceneProcessIteration");
    assert_eq!(ours.bits[0].state, GridBitState::Mesh);
}

/// `IterateGenerateCenter` uses the game's `WorldMaster.KeepGoing` (a time budget), so the game may need more
/// calls; the finished state is the same however the relaxation was split.
#[test]
fn generate_center_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = GridGenerator { custom_grid_library: our_library(), ..Default::default() };
    let origo = gg::get_starting_hex(7);
    ours.hex_origo = origo;
    let gen = g.gen;
    g.req(json!({"field": "hexOrigo", "handle": gen, "set_raw": hexs(&i2b(origo)), "main": true}), "set hexOrigo");
    let mut our_calls = 1u32;
    while ours.iterate_generate_center(&mut || true) {
        our_calls += 1;
    }
    let mut calls = 1u32;
    while ret::bytes(&g.gen_call("IterateGenerateCenter", vec![], false))[0] != 0 {
        calls += 1;
        assert!(calls < 20000, "the game never finished the centre");
    }
    // every call motivates the centre bit once more: its count is each side's number of calls
    let mut game = snapshot(&mut g, true);
    assert_eq!(game.bits[0].motivations, calls as u8, "game centre motivations");
    assert_eq!(ours.bits[0].motivations, our_calls as u8, "our centre motivations");
    game.bits[0].motivations = ours.bits[0].motivations;
    same(&game, &ours_snapshot(&ours, true), "IterateGenerateCenter finished");
}

// ---- E3e: queries on a finished neighbourhood ----

use ts_game::placemaker::quads::{Quad, Vert};
use ts_game::unity::mathematics::Int4;

fn qbytes(q: &Quad) -> Vec<u8> {
    bits_of(std::slice::from_ref(q))
}
fn vbytes(v: &Vert) -> Vec<u8> {
    bits_of(std::slice::from_ref(v))
}
fn quad_of(b: &[u8]) -> Quad {
    assert_eq!(b.len(), 68);
    let mut q = Quad::default();
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), &mut q as *mut Quad as *mut u8, 68) };
    q
}

/// Both sides grow the scene's focus to the end (IterateFocus + IterateBits).
fn finished_world(g: &mut Game) -> GridGenerator {
    let mut ours = GridGenerator { custom_grid_library: our_library(), ..Default::default() };
    let master = g.obj(g.gen, "master");
    let graph = g.obj(master, "graph");
    let b = i32s(&g.raw(graph, "genBounds"));
    let bounds = Int3x2 { c0: i3(b[0], b[1], b[2]), c1: i3(b[3], b[4], b[5]) };
    ours.iterate_focus(bounds);
    g.gen_call("IterateFocus", vec![], false);
    while ours.iterate_bits(&mut || true) {}
    let kg = g.keep_going;
    while ret::bytes(&g.gen_call("IterateBits", vec![json!({"handle": kg})], false))[0] != 0 {}
    same(&snapshot(g, false), &ours_snapshot(&ours, false), "finished world");
    ours
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
}

fn delegate(g: &mut Game, class: &str, getter: &str) -> u64 {
    let ty = |g: &mut Game, n: &str| {
        let r = g.req(json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true}), n);
        handle(&r, n)
    };
    let func = ty(g, "System.Func`1[[System.Boolean]]");
    let t = ty(g, class);
    let mi = handle(&g.req(json!({"call": "GetMethod", "class": "System.Type", "handle": t, "args": [arg::string(getter)], "param_types": ["System.String"], "main": true, "keep": true}), "GetMethod"), "mi");
    handle(&g.req(json!({"call": "System.Delegate.CreateDelegate", "args": [{"handle": func}, {"handle": mi}], "param_types": ["System.Type", "System.Reflection.MethodInfo"], "main": true, "keep": true}), "CreateDelegate"), "delegate")
}

#[test]
fn grid_queries_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = finished_world(&mut g);
    // the plane area the finished bits cover
    let ps: Vec<Vector3> = ours.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let (x0, x1) = (ps.iter().map(|p| p.x).fold(f32::MAX, f32::min) - 12.0, ps.iter().map(|p| p.x).fold(f32::MIN, f32::max) + 12.0);
    let (z0, z1) = (ps.iter().map(|p| p.z).fold(f32::MAX, f32::min) - 12.0, ps.iter().map(|p| p.z).fold(f32::MIN, f32::max) + 12.0);
    let mut r = Rng(61);
    let mut found = Vec::new();
    let (mut hits, mut misses) = (0, 0);
    for k in 0..400 {
        let p = f2(r.f(x0, x1), r.f(z0, z1));
        let q = ours.get_quad(p);
        let game = ret::bytes(&g.gen_call("GetQuad", vec![arg::raw(&bits_of(&[p.x, p.y]))], false));
        assert_eq!(game, qbytes(&q), "GetQuad({p:?}) #{k}\ngame {:?}\nours {q:?}", quad_of(&game));
        if q.is_valid() {
            hits += 1;
            found.push(q)
        } else {
            misses += 1
        }
        let v = ours.get_clostest_vert_position(p);
        let game = ret::bytes(&g.gen_call("GetClostestVertPosition", vec![arg::raw(&bits_of(&[p.x, p.y]))], false));
        assert_eq!(game, vbytes(&v), "GetClostestVertPosition({p:?})");
    }
    eprintln!("GetQuad: {hits} hits, {misses} misses");
    assert!(hits > 100 && misses > 10, "both answers should occur");
    let ql = list("Placemaker.Quads.Quad, Assembly-CSharp");
    let mut strip_quads = 0;
    for (k, q) in found.iter().enumerate().take(120) {
        // GetQuad by an edge, from each corner
        let vs = [q.v0, q.v1, q.v2, q.v3];
        let i = k % 4;
        let (c0, c1) = (vs[i], vs[(i + 1) % 4]);
        let ours_q = ours.get_quad_by_edge(c0, c0, c1);
        let game = ret::bytes(&g.gen_call("GetQuad", vec![arg::raw(&i2b(c0)), arg::raw(&i2b(c0)), arg::raw(&i2b(c1))], false));
        assert_eq!(game, qbytes(&ours_q), "GetQuad({c0:?}, {c0:?}, {c1:?})");
        // the neighbour across v0-v3
        let adj = ours.get_adjecent_quad(q);
        let game = ret::bytes(&g.gen_call("GetAdjecentQuad", vec![arg::raw(&qbytes(q))], false));
        assert_eq!(game, qbytes(&adj), "GetAdjecentQuad({q:?})");
        // a strip from it
        let l = handle(&g.req(json!({"new": ql.clone(), "args": [], "main": true}), "new List<Quad>"), "list");
        g.call(l, Some(&ql), "Add", vec![arg::raw(&qbytes(q))], &[], false);
        let a = r.f(0.0, 6.283);
        let dir = v2(a.cos(), a.sin());
        let steps = 1 + (r.next() % 12) as i32;
        let hover = (r.next() % 3) as i32;
        let mut nodes = vec![*q];
        let mut our_hover = hover;
        let our_ret = ours.append_quad_list(&mut nodes, dir, &mut our_hover, steps);
        let rr = g.gen_call("AppendQuadList", vec![json!({"handle": l}), arg::raw(&bits_of(&[dir.x, dir.y])), arg::raw(&hover.to_le_bytes()), arg::i32(steps)], false);
        assert_eq!(ret::bytes(&rr)[0] != 0, our_ret, "AppendQuadList return");
        assert_eq!(i32::from_le_bytes(ret::hex_field(&rr["args_after"][2])[..4].try_into().unwrap()), our_hover, "AppendQuadList hoverIndex");
        let game = g.to_array(l, &ql);
        assert_eq!(game, bits_of(&nodes), "AppendQuadList({q:?}, {dir:?}, {steps}): {} vs {} quads", game.len() / 68, nodes.len());
        strip_quads += nodes.len() - 1;
    }
    eprintln!("AppendQuadList added {strip_quads} quads over 120 strips");
    assert!(strip_quads > 120, "the strips should grow");
    // GetVertOrIterate on the found quads' corners (finished bits: no keepGoing needed), and their border meshes
    let master = g.obj(g.gen, "master");
    let ao = g.obj(master, "aoBaker");
    let graph = g.obj(master, "graph");
    let pw = i32s(&g.raw(ao, "powers"));
    let powers = Int4 { x: pw[0], y: pw[1], z: pw[2], w: pw[3] };
    let tex = g.obj(ao, "marchingSquaresTex");
    let ts = f32s(&ret::bytes(&g.call(tex, Some("UnityEngine.Texture"), "get_texelSize", vec![], &[], false)));
    let texel = v2(ts[0], ts[1]);
    // a border round the middle of the grid, so that quads cross it
    let c3 = ts_game::os::hex::get_hex3(ours.bits[0].hex_pos);
    let border = Int3x2 { c0: i3(c3.x - 40, c3.y - 40, c3.z - 40), c1: i3(c3.x + 40, c3.y + 40, c3.z + 40) };
    g.req(json!({"field": "border", "handle": graph, "set_raw": hexs(&bits_of(&[border.c0.x, border.c0.y, border.c0.z, border.c1.x, border.c1.y, border.c1.z])), "main": true}), "set border");
    let mut border_quads = 0;
    for (k, q) in found.iter().enumerate().take(150) {
        let corner = [q.v0, q.v1, q.v2, q.v3][k % 4];
        let v = ours.get_vert_or_iterate(corner, None);
        let game = ret::bytes(&g.gen_call("GetVertOrIterate", vec![arg::raw(&i2b(corner)), json!({"null": true})], false));
        assert_eq!(game, vbytes(&v), "GetVertOrIterate({corner:?})");
        let ours_mesh = ours.populate_border_mesh(&v, texel, powers, border);
        let mesh = handle(&g.req(json!({"new": "UnityEngine.Mesh", "args": [], "main": true}), "new Mesh"), "mesh");
        g.gen_call("PopulateBorderMesh", vec![arg::raw(&game), json!({"handle": mesh})], false);
        let vs = f32s(&g.array(mesh, "UnityEngine.Mesh.get_vertices"));
        let uv = f32s(&g.uvs(mesh, 0));
        let tris = i32s(&ret::bytes(&g.call(mesh, Some("UnityEngine.Mesh"), "GetIndicesImpl", vec![arg::i32(0), arg::bool(true)], &["System.Int32", "System.Boolean"], false)));
        assert_eq!(bits_of(&vs), bits_of(&ours_mesh.vertices), "PopulateBorderMesh({corner:?}) vertices");
        assert_eq!(bits_of(&uv), bits_of(&ours_mesh.uv0), "PopulateBorderMesh({corner:?}) uv\ngame {uv:?}\nours {:?}", ours_mesh.uv0);
        assert_eq!(tris, ours_mesh.triangles, "PopulateBorderMesh({corner:?}) triangles");
        border_quads += ours_mesh.triangles.len() / 6;
    }
    eprintln!("border quads drawn: {border_quads}");
    assert!(border_quads > 0, "some border should be drawn");
    // no bit there (-1); unfinished bits with no keepGoing (-2), or one that says no (-3)
    let far = Int2 { x: 72 * 400, y: -72 * 400 };
    let v = ours.get_vert_or_iterate(far, None);
    assert_eq!(v.quad_count, -1);
    assert_eq!(ret::bytes(&g.gen_call("GetVertOrIterate", vec![arg::raw(&i2b(far)), json!({"null": true})], false)), vbytes(&v), "GetVertOrIterate far");
    let fresh = gg::get_starting_hex(200);
    ours.motivate_bit(fresh);
    g.gen_call("MotivateBit", vec![arg::raw(&i2b(fresh))], false);
    let v = ours.get_vert_or_iterate(fresh, None);
    assert_eq!(v.quad_count, -2);
    assert_eq!(ret::bytes(&g.gen_call("GetVertOrIterate", vec![arg::raw(&i2b(fresh)), json!({"null": true})], false)), vbytes(&v), "GetVertOrIterate -2");
    let never = delegate(&mut g, "UnityEngine.Application, UnityEngine.CoreModule", "get_isEditor");
    let v = ours.get_vert_or_iterate(fresh, Some(&mut || false));
    assert_eq!(v.quad_count, -3);
    assert_eq!(ret::bytes(&g.gen_call("GetVertOrIterate", vec![arg::raw(&i2b(fresh)), json!({"handle": never})], false)), vbytes(&v), "GetVertOrIterate -3");
}
