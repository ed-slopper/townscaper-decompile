//! `placemaker::graphs::flow` against the original game (E4f). Needs `TS_HARNESS=1` and the player's install.
//! A town is built on a fresh game graph and ours (`graph_common`); a `Flow` component on a new `GameObject`
//! (disabled, so that only the test steps it), whose master is the stand-in and whose children are two flow
//! targets, runs `Update` frame by frame on both sides; the whole graph (every corner's flow data) is compared
//! after each frame, then `AddFlowTarget` and `SampleFlow` at random points.

mod graph_common;

use graph_common::*;
use serde_json::json;
use ts_check::{arg, harness, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::graphs::flow::Flow;
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, QubeId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::maker::Maker;
use ts_game::unity::mathematics::{f2, f3, Int2};
use ts_game::unity::vector::{v3, Vector3};

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

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}

#[test]
fn flow_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = Host { ao, ..Default::default() };
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(97);
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
    for id in 0..60 {
        let hex = r.pick(&verts);
        let top = (0..5).find(|&k| !ours.check_voxel(hex, k)).unwrap_or(5);
        if top >= 5 {
            continue;
        }
        let a = Action::new(hex, top as u8, VoxelType::Empty, VoxelType::from_i8((r.next() % 15) as i8), id);
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
    }
    same(&g.snapshot(&ours), &ours_snapshot(&ours), "the town");
    eprintln!("town: {} voxels", ours.total_voxel_count);
    // the flow component, with two targets in the open
    let new_go = |g: &mut Game, name: &str| {
        let go = handle(&ok(g.h.request(json!({"new": "UnityEngine.GameObject", "args": [arg::string(name)], "param_types": ["System.String"], "main": true})), "go"), "go");
        let t = handle(&ok(g.h.request(json!({"call": "get_transform", "class": "UnityEngine.GameObject", "handle": go, "args": [], "main": true, "keep": true})), "t"), "t");
        (go, t)
    };
    let (fgo, ft) = new_go(&mut g, "ts_check flow");
    let flow_type = handle(&ok(g.h.request(json!({"call": "System.Type.GetType", "args": [arg::string("Placemaker.Graphs.Flow, Assembly-CSharp")], "param_types": ["System.String"], "main": true, "keep": true})), "type"), "type");
    let flow = handle(&ok(g.h.request(json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": fgo, "args": [{"handle": flow_type}], "param_types": ["System.Type"], "main": true, "keep": true})), "AddComponent"), "flow");
    ok(g.h.request(json!({"call": "set_enabled", "class": "UnityEngine.Behaviour", "handle": flow, "args": [arg::bool(false)], "main": true})), "disable");
    let master = handle(&ok(g.h.request(Game::field(g.graph, "master")), "master"), "master");
    ok(g.h.request(json!({"field": "master", "handle": flow, "set_handle": master, "main": true})), "flow master");
    let mut ours_flow = Flow::default();
    for k in 0..2 {
        let p = v3(r.f(cx - 2.5, cx + 2.5), 0.5 + k as f32 * 1.7, r.f(cz - 2.5, cz + 2.5));
        let (_, t) = new_go(&mut g, "ts_check flow target");
        ok(g.h.request(json!({"call": "SetParent", "class": "UnityEngine.Transform", "handle": t, "args": [{"handle": ft}], "param_types": ["UnityEngine.Transform"], "main": true})), "SetParent");
        ok(g.h.request(json!({"call": "set_position", "class": "UnityEngine.Transform", "handle": t, "args": [arg::raw(&bits_of(&[p.x, p.y, p.z]))], "main": true})), "set_position");
        ours_flow.targets.push(p);
    }
    for frame in 0..14 {
        ok(g.h.request(json!({"call": "Update", "class": "Placemaker.Graphs.Flow", "handle": flow, "args": [], "main": true})), "Flow.Update");
        ours_flow.update(&mut ours, &grid);
        if frame < 4 || frame % 3 == 2 {
            same(&g.snapshot(&ours), &ours_snapshot(&ours), &format!("flow frame {frame}"));
            eprintln!("frame {frame}: same");
        }
    }
    // AddFlowTarget at random points, then the field read back
    for k in 0..30 {
        let p = v3(r.f(cx - 5.0, cx + 5.0), r.f(-1.0, 6.0), r.f(cz - 5.0, cz + 5.0));
        ok(g.h.request(json!({"call": "AddFlowTarget", "class": "Placemaker.Graphs.Flow", "handle": flow, "args": [arg::raw(&bits_of(&[p.x, p.y, p.z]))], "main": true})), "AddFlowTarget");
        ours_flow.add_flow_target(&mut ours, &grid, f3(p.x, p.y, p.z));
        if k % 10 == 9 {
            ok(g.h.request(json!({"call": "Update", "class": "Placemaker.Graphs.Flow", "handle": flow, "args": [], "main": true})), "Flow.Update");
            ours_flow.update(&mut ours, &grid);
            same(&g.snapshot(&ours), &ours_snapshot(&ours), &format!("after targets {k}"));
        }
    }
    let (mut moving, mut solid) = (0, 0);
    for k in 0..500 {
        let p = v3(r.f(cx - 5.0, cx + 5.0), r.f(-1.5, 7.0), r.f(cz - 5.0, cz + 5.0));
        let gb = ret::bytes(&ok(g.h.request(json!({"call": "SampleFlow", "class": "Placemaker.Graphs.Flow", "handle": flow, "args": [arg::raw(&bits_of(&[p.x, p.y, p.z]))], "main": true})), "SampleFlow"));
        let (d, n, c, w) = ours_flow.sample_flow(&ours, &grid, p);
        assert_eq!(u32s(&gb), bits(&[d.x, d.y, d.z, n.x, n.y, n.z, c, w]), "SampleFlow({p:?}) #{k}");
        if d != Vector3::default() {
            moving += 1;
        }
        if n != Vector3::default() {
            solid += 1;
        }
    }
    eprintln!("SampleFlow: {moving} with a direction, {solid} with a normal");
    assert!(moving > 100 && solid > 50, "the field should have flow and walls");
}
