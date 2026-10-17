//! `placemaker::graphs::Graph` structure against the original game (E4b). Needs `TS_HARNESS=1`.
//! A fresh `Graph` (made with `new`) gets the scene's `WorldMaster` and new container transforms.

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::graphs::{CornerId, Graph, QubeId, QubeRelation, SquareId, SquareRelation};
use ts_game::unity::mathematics::{i3, Int2, Int3x2};
use ts_game::unity::vector::{v3, Vector3};

const G: &str = "Placemaker.Graphs.Graph";

fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}
fn hexs(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn i32s(b: &[u8]) -> Vec<i32> {
    b.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn i(&mut self, n: u32) -> i32 {
        (self.next() % (2 * n + 1)) as i32 - n as i32
    }
}

struct Game<'a> {
    h: &'a mut Harness,
    graph: u64,
    containers: Vec<(String, u64)>,
}

impl Game<'_> {
    fn req(&mut self, r: Value, what: &str) -> Value {
        ok(self.h.request(r), what)
    }
    fn call(&mut self, obj: u64, class: &str, m: &str, args: Vec<Value>, keep: bool) -> Value {
        self.req(json!({"call": m, "class": class, "handle": obj, "args": args, "main": true, "keep": keep}), m)
    }
    fn g(&mut self, m: &str, args: Vec<Value>, keep: bool) -> Value {
        let gr = self.graph;
        self.call(gr, G, m, args, keep)
    }
    fn container(&self, name: &str) -> u64 {
        self.containers.iter().find(|(n, _)| n == name).unwrap().1
    }
    fn child_count(&mut self, t: u64) -> usize {
        ret::i32(&self.call(t, "UnityEngine.Transform", "get_childCount", vec![], false)) as usize
    }
    fn child(&mut self, t: u64, i: usize) -> u64 {
        handle(&self.call(t, "UnityEngine.Transform", "GetChild", vec![arg::i32(i as i32)], true), "child")
    }
    fn component(&mut self, t: u64, class: &str) -> u64 {
        let ty = handle(&self.req(json!({"call": "System.Type.GetType", "args": [arg::string(&format!("{class}, Assembly-CSharp"))], "param_types": ["System.String"], "main": true, "keep": true}), "GetType"), "type");
        handle(&self.call(t, "UnityEngine.Component", "GetComponent", vec![json!({"handle": ty})], true), "component")
    }
    fn transform(&mut self, c: u64) -> u64 {
        handle(&self.call(c, "UnityEngine.Component", "get_transform", vec![], true), "transform")
    }
    fn raw(&mut self, obj: u64, f: &str) -> Vec<u8> {
        ret::bytes(&self.req(json!({"field": f, "handle": obj, "main": true}), f))
    }
    fn set(&mut self, obj: u64, f: &str, raw: &[u8]) {
        self.req(json!({"field": f, "handle": obj, "set_raw": hexs(raw), "main": true}), f);
    }
    fn addr(&mut self, obj: u64) -> u64 {
        self.req(json!({"addr": obj}), "addr")["addr"].as_u64().unwrap()
    }
    /// The hexPos of each corner under a container.
    fn corner_positions(&mut self, container: &str) -> Vec<Int2> {
        let t = self.container(container);
        (0..self.child_count(t))
            .map(|i| {
                let c = self.child(t, i);
                let corner = self.component(c, "Placemaker.Graphs.Corner");
                let v = i32s(&self.raw(corner, "hexPos"));
                Int2 { x: v[0], y: v[1] }
            })
            .collect()
    }
}

fn setup(h: &mut Harness) -> Game<'_> {
    h.wait_for_object("Placemaker.GridGenerator");
    let master = handle(&ok(h.request(json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true})), "master"), "master");
    let graph = handle(&ok(h.request(json!({"new": G, "args": [], "main": true})), "new Graph"), "graph");
    ok(h.request(json!({"field": "master", "handle": graph, "set_handle": master, "main": true})), "set master");
    let mut containers = Vec::new();
    for name in ["corners", "cornerPool", "squares", "squarePool", "voxelPool", "looseVoxels", "qubePool"] {
        let go = handle(&ok(h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true})), "go"), "go");
        let t = handle(&ok(h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
        ok(h.request(json!({"field": name, "handle": graph, "set_handle": t, "main": true})), name);
        containers.push((name.to_string(), t));
    }
    Game { h, graph, containers }
}

#[test]
fn pools_and_corners_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = Graph::default();
    // pools: the game's keepGoing is a time budget, so compare once both are full
    while ret::bytes(&g.g("IterateRefillPools", vec![], false))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    for (name, n) in [("voxelPool", ours.voxel_pool.len()), ("qubePool", ours.qube_pool.len()), ("cornerPool", ours.corner_pool.len()), ("squarePool", ours.square_pool.len())] {
        let t = g.container(name);
        assert_eq!(g.child_count(t), n, "{name} count");
    }
    let t = g.container("qubePool");
    let q = g.child(t, 3);
    let q = g.component(q, "Placemaker.Graphs.Qube");
    assert_eq!(i32s(&g.raw(q, "existanceIndex"))[0] as u32, ours.qube(ours.qube_pool[3]).existance_index, "pooled qube existanceIndex");
    let t = g.container("squarePool");
    let s = g.child(t, 0);
    let s = g.component(s, "Placemaker.Graphs.Square");
    assert_eq!(g.raw(s, "q2")[4] as i8, ours.square(ours.square_pool[0]).q[2].index_in_other, "new square relation");
    // corners: random positions with repeats, from the pool first
    let mut r = Rng(81);
    let mut seen = Vec::new();
    for k in 0..60 {
        let p = if k > 0 && r.next() % 4 == 0 { seen[(r.next() as usize) % seen.len()] } else { Int2 { x: r.i(30) * 12, y: r.i(30) * 12 } };
        seen.push(p);
        ours.get_or_create_corner(p);
        g.g("GetOrCreateCorner", vec![arg::raw(&bytes(&p))], false);
    }
    let game = g.corner_positions("corners");
    let ours_pos: Vec<Int2> = ours.corners.iter().map(|&c| ours.corner(c).hex_pos).collect();
    assert_eq!(game, ours_pos, "corners order");
    let t = g.container("cornerPool");
    assert_eq!(g.child_count(t), ours.corner_pool.len(), "corner pool after use");
    for _ in 0..200 {
        let p = if r.next() % 3 == 0 { seen[(r.next() as usize) % seen.len()] } else { Int2 { x: r.i(31) * 12 + r.i(1), y: r.i(31) * 12 } };
        let gi = ret::i32(&g.g("BinaryTreeSearchCorner", vec![arg::raw(&bytes(&p))], false));
        assert_eq!(gi, ours.binary_tree_search_corner(p), "BinaryTreeSearchCorner({p:?})");
    }
    assert_eq!(ret::bytes(&g.g("get_anyVoxels", vec![], false))[0] != 0, ours.any_voxels());
    // voxels under a corner, by height
    let c0 = ours.corners[0];
    let ct = g.container("corners");
    let ct0 = g.child(ct, 0);
    let mut heights: Vec<i32> = (0..12).map(|_| (r.next() % 40) as i32).collect();
    heights.sort();
    heights.dedup();
    for &hgt in &heights {
        let v = ours.get_new_voxel();
        ours.voxel_mut(v).local_position = v3(0.0, hgt as f32 + 0.5, 0.0);
        ours.corner_mut(c0).voxels.push(v);
        let gv = handle(&g.g("GetNewVoxel", vec![json!({"handle": ct0})], true), "voxel");
        let gt = g.transform(gv);
        g.call(gt, "UnityEngine.Transform", "set_localPosition", vec![arg::raw(&bytes(&v3(0.0, hgt as f32 + 0.5, 0.0)))], false);
    }
    for hgt in -2..45 {
        let gi = ret::i32(&g.g("BinaryTreeSearchVoxel", vec![json!({"handle": ct0}), arg::i32(hgt)], false));
        assert_eq!(gi, ours.binary_tree_search_voxel(c0, hgt), "BinaryTreeSearchVoxel({hgt})");
        let p = ours.corner(c0).hex_pos;
        assert_eq!(ret::bytes(&g.g("CheckVoxel", vec![arg::raw(&bytes(&p)), arg::i32(hgt)], false))[0] != 0, ours.check_voxel(p, hgt), "CheckVoxel");
    }
}

#[test]
fn squares_search_and_bounds_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = Graph::default();
    let mut r = Rng(82);
    // empty: the special answer
    for x in [-1.0f32, 0.0, 2.0] {
        let p = v3(x, 0.0, 0.0);
        assert_eq!(ret::i32(&g.g("BinaryTreeSearchSquare", vec![arg::raw(&bytes(&p))], false)), ours.binary_tree_search_square(p), "empty BinaryTreeSearchSquare({x})");
    }
    let mut xs: Vec<f32> = (0..30).map(|_| (r.i(200) as f32) * 0.25).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let st = g.container("squares");
    for &x in &xs {
        let s = ours.get_new_square();
        ours.square_mut(s).local_position = v3(x, 0.0, 1.0);
        ours.squares.push(s);
        let gs = handle(&g.g("GetNewSquare", vec![json!({"handle": st})], true), "square");
        let gt = g.transform(gs);
        g.call(gt, "UnityEngine.Transform", "set_position", vec![arg::raw(&bytes(&v3(x, 0.0, 1.0)))], false);
    }
    for _ in 0..200 {
        let x = if r.next() % 3 == 0 { xs[(r.next() as usize) % xs.len()] } else { r.i(220) as f32 * 0.2 };
        let p = v3(x, 3.0, -2.0);
        assert_eq!(ret::i32(&g.g("BinaryTreeSearchSquare", vec![arg::raw(&bytes(&p))], false)), ours.binary_tree_search_square(p), "BinaryTreeSearchSquare({x})");
    }
    // bounds
    let gr = g.graph;
    for _ in 0..300 {
        let c = Int2 { x: r.i(400), y: r.i(400) };
        let ext = Int3x2 { c0: i3(r.i(600), r.i(600), r.i(600)), c1: i3(r.i(600), r.i(600), r.i(600)) };
        let ht = (r.next() % 300) as i32;
        let gx = i32s(&ret::bytes(&g.req(json!({"call": format!("{G}.GetMaxExtents"), "args": [arg::raw(&bytes(&ext)), arg::i32(ht)], "main": true}), "GetMaxExtents")));
        let (a, b) = Graph::get_max_extents(ext, ht);
        assert_eq!((gx[0], gx[1]), (a, b), "GetMaxExtents({ext:?}, {ht})");
        ours.set_empty_bounds_center(c);
        g.g("SetEmptyBoundsCenter", vec![arg::raw(&bytes(&c))], false);
        assert_eq!(g.raw(gr, "bounds"), bytes(&ours.bounds), "SetEmptyBoundsCenter");
        // a random state for IsCoordinateAllowed
        let b0 = i3(r.i(300), r.i(300), r.i(300));
        ours.bounds = Int3x2 { c0: b0, c1: i3(b0.x + (r.next() % 400) as i32, b0.y + (r.next() % 400) as i32, b0.z + (r.next() % 400) as i32) };
        ours.view_bounds = Int3x2 { c0: i3(-600, -600, -600), c1: i3(r.i(600) + 300, 600, 600) };
        ours.bounds_height = (r.next() % 260) as i32;
        let bh = ours.bounds_height;
        g.set(gr, "bounds", &bytes(&ours.bounds));
        g.set(gr, "viewBounds", &bytes(&ours.view_bounds));
        g.set(gr, "boundsHeight", &bh.to_le_bytes());
        let p = Int2 { x: r.i(500), y: r.i(500) };
        let ph = (r.next() % 300) as i32;
        assert_eq!(ret::bytes(&g.g("IsCoordinateAllowed", vec![arg::raw(&bytes(&p)), arg::i32(ph)], false))[0] != 0, ours.is_coordinate_allowed(p, ph), "IsCoordinateAllowed({p:?}, {ph})");
    }
}

#[test]
fn qube_relations_and_on_enable_match_the_game() {
    let Some(mut h) = harness() else { return };
    let mut g = setup(&mut h);
    let mut ours = Graph::default();
    let mut r = Rng(83);
    // a ring of squares, each with a column of qubes, related across edges; corners for OnEnable
    let n = 6usize;
    let st = g.container("squares");
    let mut gsq = Vec::new();
    let mut gq: Vec<Vec<u64>> = Vec::new();
    for k in 0..n {
        let s = ours.get_new_square();
        ours.squares.push(s);
        let gs = handle(&g.g("GetNewSquare", vec![json!({"handle": st})], true), "square");
        let gt = g.transform(gs);
        let mut col = Vec::new();
        for i in 0..(2 + r.next() % 4) {
            let q = ours.get_new_qube();
            ours.qube_mut(q).square = Some(s);
            ours.square_mut(s).qubes.push(q);
            let gqube = handle(&g.g("GetNewQube", vec![json!({"handle": gt})], true), "qube");
            let _ = i;
            col.push(gqube);
        }
        gsq.push(gs);
        gq.push(col);
        let _ = k;
    }
    // corners: square k has corners (k, k+1, 100+k+1, 100+k)
    let mut corner_ids: Vec<(Int2, CornerId, u64)> = Vec::new();
    let mut corner_of = |g: &mut Game, ours: &mut Graph, key: i32| -> (CornerId, u64) {
        let p = Int2 { x: key * 12, y: -key * 12 };
        if let Some(&(_, c, gc)) = corner_ids.iter().find(|(q, _, _)| *q == p) {
            return (c, gc);
        }
        let c = ours.get_or_create_corner(p);
        let gc = handle(&g.g("GetOrCreateCorner", vec![arg::raw(&bytes(&p))], true), "corner");
        corner_ids.push((p, c, gc));
        (c, gc)
    };
    for k in 0..n {
        let keys = [k as i32, k as i32 + 1, 101 + k as i32, 100 + k as i32];
        for (i, &key) in keys.iter().enumerate() {
            let (c, gc) = corner_of(&mut g, &mut ours, key);
            ours.square_mut(SquareId(k as u32)).set_corner(i as i8, Some(c));
            g.call(gsq[k], "Placemaker.Graphs.Square", "SetCorner", vec![arg::raw(&[i as u8]), json!({"handle": gc})], false);
        }
    }
    // relations: square k side 1 <-> square k+1 side 3
    for k in 0..n - 1 {
        ours.square_mut(SquareId(k as u32)).set_relation(1, 3, Some(SquareId(k as u32 + 1)));
        ours.square_mut(SquareId(k as u32 + 1)).set_relation(3, 1, Some(SquareId(k as u32)));
        let (a, b) = (gsq[k], gsq[k + 1]);
        g.call(a, "Placemaker.Graphs.Square", "SetRelation", vec![arg::raw(&[1]), arg::raw(&[3]), json!({"handle": b})], false);
        g.call(b, "Placemaker.Graphs.Square", "SetRelation", vec![arg::raw(&[3]), arg::raw(&[1]), json!({"handle": a})], false);
    }
    // the game's qube addresses, to read the relations back
    let mut addr_of = std::collections::HashMap::new();
    for k in 0..n {
        for (i, &gqube) in gq[k].iter().enumerate() {
            let a = g.addr(gqube);
            addr_of.insert(a, ours.square(SquareId(k as u32)).qubes[i]);
        }
    }
    let decode = |raw: &[u8], addr_of: &std::collections::HashMap<u64, QubeId>| -> QubeRelation {
        let p = u32::from_le_bytes(raw[..4].try_into().unwrap()) as u64;
        QubeRelation { qube: if p == 0 { None } else { Some(*addr_of.get(&p).expect("a known qube")) }, index_in_other: raw[4] as i8 }
    };
    for k in 0..n {
        for (i, &gqube) in gq[k].iter().enumerate() {
            let q = ours.square(SquareId(k as u32)).qubes[i];
            for side in 0..6i8 {
                let raw = ret::bytes(&g.g("GetQubeRelation", vec![json!({"handle": gqube}), arg::raw(&[side as u8])], false));
                assert_eq!(decode(&raw, &addr_of), ours.get_qube_relation(q, side), "GetQubeRelation(square {k} qube {i}, {side})");
            }
            let mask = (r.next() % 64) as u8;
            let raw = ret::bytes(&g.g("GetQubeRelation6", vec![json!({"handle": gqube}), arg::raw(&[mask])], false));
            let ours6 = ours.get_qube_relation6(q, mask);
            for side in 0..6 {
                assert_eq!(decode(&raw[side * 8..side * 8 + 8], &addr_of), ours6.r[side], "GetQubeRelation6(square {k} qube {i}, {mask:#x})[{side}]");
            }
        }
    }
    // OnEnable rebuilds the dictionaries
    ours.on_enable();
    g.g("OnEnable", vec![], false);
    let gr = g.graph;
    for (f, class, n) in [
        ("cornerDict", "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2, Unity.Mathematics],[Placemaker.Graphs.Corner, Assembly-CSharp]]", ours.corner_dict.len()),
        ("squareDict", "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2, Unity.Mathematics],[Placemaker.Graphs.Square, Assembly-CSharp]]", ours.square_dict.len()),
        ("edgeDict", "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2x2, Unity.Mathematics],[Placemaker.Graphs.Square+Relation, Assembly-CSharp]]", ours.edge_dict.len()),
    ] {
        let d = handle(&g.req(json!({"field": f, "handle": gr, "main": true}), f), f);
        assert_eq!(ret::i32(&g.call(d, class, "get_Count", vec![], false)) as usize, n, "{f}.Count");
    }
    // every key of ours is a key of the game's
    let sd = handle(&g.req(json!({"field": "squareDict", "handle": gr, "main": true}), "squareDict"), "d");
    for key in ours.square_dict.keys() {
        let class = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2, Unity.Mathematics],[Placemaker.Graphs.Square, Assembly-CSharp]]";
        assert!(ret::bytes(&g.call(sd, class, "ContainsKey", vec![arg::raw(&bytes(key))], false))[0] != 0, "squareDict key {key:?}");
    }
    let ed = handle(&g.req(json!({"field": "edgeDict", "handle": gr, "main": true}), "edgeDict"), "d");
    for (a, b) in ours.edge_dict.keys() {
        let class = "System.Collections.Generic.Dictionary`2[[Unity.Mathematics.int2x2, Unity.Mathematics],[Placemaker.Graphs.Square+Relation, Assembly-CSharp]]";
        let key = [bytes(a), bytes(b)].concat();
        assert!(ret::bytes(&g.call(ed, class, "ContainsKey", vec![arg::raw(&key)], false))[0] != 0, "edgeDict key {a:?} {b:?}");
    }
    let _ = (SquareRelation::EMPTY, Vector3::ZERO);
}
