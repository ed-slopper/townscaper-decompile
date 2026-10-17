//! `placemaker::graphs::Graph` editing against the original game (E4c). Needs `TS_HARNESS=1` and the player's
//! install. Random actions (add, paint, remove) run on a fresh game graph and ours (`graph_common`), and the
//! whole graph is compared: every object in every container, the references between them (by address), the
//! dictionaries, queues and counters (after each of the first steps, then every 10th).

mod graph_common;

use graph_common::*;
use serde_json::json;
use ts_check::{arg, harness, ret};
use ts_game::placemaker::graphs::{Action, Graph, VoxelId, VoxelType};
use ts_game::placemaker::grid_generator::GridBitState;
use ts_game::unity::mathematics::{f2, Int2};
use ts_game::unity::vector::Vector3;

#[test]
fn editing_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, _) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    // a fresh BoxCollider's size
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
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

