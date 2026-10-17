//! `placemaker::graphs` sampling against the original game (E4e). Needs `TS_HARNESS=1` and the player's
//! install. A town is built on a fresh game graph and ours (`graph_common`) with the full update, then the
//! samplers are compared bit for bit at random points round it: `GetQubeWithContext`, `SampleNormal`,
//! `SampleNormalCoverage` (by point, and for a given qube), `SampleNormalDistance` / `Distance2`, and
//! `Qube.SampleNormalCoverage` / `GetNormal` at random points in random qubes.

mod graph_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::json;
use ts_check::{arg, harness, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, QubeId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::maker::Maker;
use ts_game::unity::mathematics::{f2, Int2};
use ts_game::unity::vector::{v3, Vector3};

const G: &str = "Placemaker.Graphs.Graph";

#[derive(Default)]
struct Host {
    ao: AoBaker,
    maker: Maker,
}
impl GraphHost for Host {
    fn ao_baker(&mut self) -> Option<&mut AoBaker> {
        Some(&mut self.ao)
    }
    fn maker(&mut self) -> Option<&mut Maker> {
        Some(&mut self.maker)
    }
    fn qube_removed(&mut self, graph: &mut Graph, qube: QubeId, _side_mask: u8) {
        let q = graph.qube_mut(qube);
        q.last_propagation = 0;
        q.side_variation_mask = 0;
    }
}

fn settle(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut Host) {
    while ret::bytes(&g.g("Iterate", vec![]))[0] != 0 {}
    while ours.iterate(grid, host) {}
    while ret::bytes(&g.g("IterateFocus", vec![]))[0] != 0 {}
    while ours.iterate_focus(grid, host) {}
}

fn f32s(b: &[u8]) -> Vec<u32> {
    u32s(b)
}
fn v3bits_of(v: Vector3) -> Vec<u32> {
    vec![v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}
fn nc_bits((n, c): (Vector3, f32)) -> Vec<u32> {
    let mut v = v3bits_of(n);
    v.push(c.to_bits());
    v
}

#[test]
fn sampling_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = Host { ao, ..Default::default() };
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    // a town: random voxels round the middle of the grid, stacked up to 5 high
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(95);
    let mut verts: Vec<Int2> = Vec::new();
    for _ in 0..300 {
        let q = grid.get_quad(f2(r.f(cx - 4.0, cx + 4.0), r.f(cz - 4.0, cz + 4.0)));
        if q.is_valid() {
            for v in [q.v0, q.v1, q.v2, q.v3] {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
    }
    for id in 0..70 {
        let hex = r.pick(&verts);
        let a = if id % 9 == 8 && !ours.corners.is_empty() {
            // now and then a removal, so that flow data and squares go too
            let c = ours.corners[r.next() as usize % ours.corners.len()];
            match ours.corner(c).voxels.last() {
                Some(&v) => Action::new(ours.corner(c).hex_pos, ours.voxel(v).height, ours.voxel(v).voxel_type, VoxelType::Empty, id),
                None => continue,
            }
        } else {
            let top = (0..6).find(|&k| !ours.check_voxel(hex, k)).unwrap_or(6);
            if top >= 6 {
                continue;
            }
            Action::new(hex, top as u8, VoxelType::Empty, VoxelType::from_i8((r.next() % 15) as i8), id)
        };
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
    }
    same(&g.snapshot(&ours), &ours_snapshot(&ours), "the town");
    eprintln!("town: {} voxels, {} squares", ours.total_voxel_count, ours.squares.len());
    // the game's squares and qubes by address
    let sq_container = g.containers["squares"];
    let kids = g.children(&[sq_container]);
    let squares = g.components(&kids[0], "Placemaker.Graphs.Square");
    let mut square_of: HashMap<u64, u32> = HashMap::new();
    for (a, s) in g.addrs(&squares).into_iter().zip(&ours.squares) {
        square_of.insert(a, s.0);
    }
    let qube_kids = g.children(&kids[0]);
    let mut qube_handle: HashMap<u32, u64> = HashMap::new();
    let mut qube_of: HashMap<u64, u32> = HashMap::new();
    for (k, s) in ours.squares.iter().enumerate() {
        let qs = g.components(&qube_kids[k], "Placemaker.Graphs.Qube");
        let addrs = g.addrs(&qs);
        for ((a, hq), q) in addrs.into_iter().zip(qs).zip(&ours.square(*s).qubes) {
            qube_of.insert(a, q.0);
            qube_handle.insert(q.0, hq);
        }
    }
    let ptr = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as u64;
    let (mut found, mut covered, mut distance_hits) = (0, 0, 0);
    for k in 0..600 {
        let p = v3(r.f(cx - 5.0, cx + 5.0), r.f(-2.5, 7.5), r.f(cz - 5.0, cz + 5.0));
        let pb = arg::raw(&bits_of(&[p.x, p.y, p.z]));
        // GetQubeWithContext
        let ctx = ours.get_qube_with_context(p);
        let gb = ret::bytes(&g.g("GetQubeWithContext", vec![pb.clone()]));
        let gq = ptr(&gb, 0);
        let gs = ptr(&gb, 4);
        let game_q = if gq == 0 { None } else { Some(*qube_of.get(&gq).unwrap_or(&u32::MAX)) };
        let game_s = if gs == 0 { None } else { Some(*square_of.get(&gs).unwrap_or(&u32::MAX)) };
        assert_eq!((game_q, game_s), (ctx.qube.map(|q| q.0), ctx.square.map(|s| s.0)), "GetQubeWithContext({p:?}) #{k}: qube, square");
        let ours_rest: Vec<u32> = [ctx.c0, ctx.c1, ctx.c2, ctx.c3].iter().flat_map(|c| [c.x.to_bits(), c.y.to_bits()]).chain([ctx.height as u32]).collect();
        assert_eq!(f32s(&gb[8..0x2C]), ours_rest, "GetQubeWithContext({p:?}) #{k}: corners, height");
        if ctx.qube.is_some() {
            found += 1;
        }
        // SampleNormalCoverage(point), SampleNormal
        let oc = ours.sample_normal_coverage(p);
        let gc = ret::bytes(&ok(g.h.request(json!({"call": "SampleNormalCoverage", "class": G, "handle": g.graph, "args": [pb.clone()], "param_types": ["UnityEngine.Vector3"], "main": true})), "SampleNormalCoverage"));
        assert_eq!(f32s(&gc), nc_bits(oc), "SampleNormalCoverage({p:?}) #{k}");
        if oc.1 > 0.0 && oc.1 < 1.0 {
            covered += 1;
        }
        let gn = ret::bytes(&g.g("SampleNormal", vec![pb.clone()]));
        assert_eq!(f32s(&gn), v3bits_of(ours.sample_normal(p)), "SampleNormal({p:?}) #{k}");
        // SampleNormalCoverage(point, qube): the context's qube (or none), or a random one
        let q = if k % 3 == 0 { ctx.qube } else { Some(QubeId(*qube_handle.keys().nth(r.next() as usize % qube_handle.len()).unwrap())) };
        let qa = match q {
            Some(q) => json!({"handle": qube_handle[&q.0]}),
            None => json!({"null": true}),
        };
        let gq = ret::bytes(&ok(g.h.request(json!({"call": "SampleNormalCoverage", "class": G, "handle": g.graph, "args": [pb.clone(), qa], "param_types": ["UnityEngine.Vector3", "Placemaker.Graphs.Qube"], "main": true})), "SampleNormalCoverage(qube)"));
        assert_eq!(f32s(&gq), nc_bits(ours.sample_normal_coverage_qube(p, q)), "SampleNormalCoverage({p:?}, {q:?}) #{k}");
        // the distance samplers
        let od = ours.sample_normal_distance(&grid, p);
        let gd = ret::bytes(&g.g("SampleNormalDistance", vec![pb.clone()]));
        assert_eq!(f32s(&gd), nc_bits(od), "SampleNormalDistance({p:?}) #{k}");
        if od.1 != 2.0 && od.0 != Vector3::default() {
            distance_hits += 1;
        }
        let gd2 = ret::bytes(&g.g("SampleNormalDistance2", vec![pb]));
        assert_eq!(f32s(&gd2), nc_bits(ours.sample_normal_distance2(&grid, p)), "SampleNormalDistance2({p:?}) #{k}");
    }
    eprintln!("{found} points in a qube, {covered} partly covered, {distance_hits} distance normals");
    assert!(found > 100 && covered > 50 && distance_hits > 50, "the samplers should see the town");
    // the qube's own sampler at random points of random qubes
    let ids: Vec<u32> = qube_handle.keys().copied().collect();
    for k in 0..400 {
        let q = QubeId(ids[r.next() as usize % ids.len()]);
        let ts = if k % 10 == 0 { v3(0.5, 0.5, 0.5) } else { v3(r.f(-0.1, 1.1), r.f(-0.1, 1.1), r.f(-0.1, 1.1)) };
        let s = ours.qube(q).square.unwrap();
        let m = ours.square(s).m;
        let gq = ret::bytes(&ok(g.h.request(json!({"call": "SampleNormalCoverage", "class": "Placemaker.Graphs.Qube", "handle": qube_handle[&q.0], "args": [arg::raw(&bits_of(&[ts.x, ts.y, ts.z]))], "main": true})), "Qube.SampleNormalCoverage"));
        assert_eq!(f32s(&gq), nc_bits(ours.qube(q).sample_normal_coverage(&m, ts)), "Qube({}).SampleNormalCoverage({ts:?})", q.0);
        if k % 10 == 0 {
            let gn = ret::bytes(&ok(g.h.request(json!({"call": "GetNormal", "class": "Placemaker.Graphs.Qube", "handle": qube_handle[&q.0], "args": [], "main": true})), "GetNormal"));
            assert_eq!(f32s(&gn), v3bits_of(ours.qube(q).get_normal(&m)), "Qube({}).GetNormal", q.0);
        }
    }
}
