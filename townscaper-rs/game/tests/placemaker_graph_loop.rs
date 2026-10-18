//! `placemaker::graphs::Graph`'s loop, saving and loading against the original game (E4d). Needs `TS_HARNESS=1`
//! and the player's install. On a fresh game graph and ours (`graph_common`): random actions, each followed by
//! `Iterate` and `IterateFocus` until both are done; then `Save`, more actions, and `BeginLoad` / `IterateLoad`
//! of the save (through a fresh `Maker`), and the update again. The whole graph, the shadow quads and the
//! maker's action list are compared along the way. The game's `Iterate` / `IterateLoad` stop when the frame's
//! time budget runs out, so both sides are run to the end before comparing.

mod graph_common;

use graph_common::*;
use serde_json::json;
use ts_check::{arg, harness, ret};
use ts_game::placemaker::ao_baker::AoBaker;
use ts_game::placemaker::graphs::{Action, Graph, GraphHost, QubeId, VoxelType};
use ts_game::placemaker::grid_generator::{GridBitState, GridGenerator};
use ts_game::placemaker::maker::Maker;
use ts_game::placemaker::save_data::{SaveCorner, SaveData, SaveVoxel};
use ts_game::unity::mathematics::{f2, Int2};
use ts_game::unity::vector::Vector3;

/// Our side of the systems round the graph: the stand-in's shadow meshes and maker, what its fresh wave
/// function collapse does to the graph (its propagation counter: 0, then 4 at the first
/// `IteratePropagationCount`), and a record of the rest.
#[derive(Default)]
struct LoopHost {
    ao: AoBaker,
    maker: Maker,
    propagation_counter: u16,
    last_started_propagation: u16,
    calls: Vec<String>,
}
impl GraphHost for LoopHost {
    fn ao_baker(&mut self) -> Option<&mut AoBaker> {
        Some(&mut self.ao)
    }
    fn maker(&mut self) -> Option<&mut Maker> {
        Some(&mut self.maker)
    }
    fn bake_ao(&mut self) {
        self.calls.push("BakeAo".into());
    }
    fn wfc_qube_updated(&mut self, graph: &mut Graph, qube: QubeId) {
        graph.qube_mut(qube).last_propagation = self.propagation_counter;
    }
    fn wfc_iterate_propagation_count(&mut self) {
        if self.propagation_counter == self.last_started_propagation {
            self.propagation_counter += 4;
        }
    }
    fn qube_removed(&mut self, graph: &mut Graph, qube: QubeId, side_mask: u8) {
        let q = graph.qube_mut(qube);
        q.awaiting_rebuilding = false;
        q.awaiting_grass = false;
        q.last_propagation = 0;
        q.possible_modules.clear();
        let mut mask = side_mask & q.side_variation_mask;
        let s = q.square.unwrap();
        if graph.square(s).qubes.first() == Some(&qube) {
            mask |= 0xF;
        }
        let r = graph.get_qube_relation6(qube, mask);
        for k in 0..6 {
            let rel = r.get(k);
            if rel.index_in_other != -1 {
                let n = rel.qube.unwrap();
                if !graph.qube(n).fully_covered() {
                    graph.qube_mut(n).last_propagation = self.propagation_counter;
                }
            }
        }
        graph.qube_mut(qube).side_variation_mask = 0;
    }
    fn on_bounds_updated(&mut self) {
        self.calls.push("onBoundsUpdated".into());
    }
    fn log(&mut self, m: &str) {
        self.calls.push(format!("Log({m})"));
    }
}

/// Both sides' `Iterate`, then `IterateFocus`, until each says it is done.
fn settle(g: &mut Game, grid: &mut GridGenerator, ours: &mut Graph, host: &mut LoopHost) {
    let mut n = 0;
    while ret::bytes(&g.g("Iterate", vec![]))[0] != 0 {
        n += 1;
        assert!(n < 10_000, "the game's Iterate never ends");
    }
    while ours.iterate(grid, host) {}
    while ret::bytes(&g.g("IterateFocus", vec![]))[0] != 0 {}
    while ours.iterate_focus(grid, host) {}
}

fn check(g: &mut Game, ours: &Graph, host: &LoopHost, what: &str) {
    same(&g.snapshot(ours), &ours_snapshot(ours), what);
    let skip = g.ao_skip;
    let (gm, om) = (g.shadow_meshes(), ours_shadow_meshes(&host.ao, skip));
    assert_eq!(gm.len(), om.len(), "{what}: shadow mesh count");
    for (k, (a, b)) in gm.iter().zip(&om).enumerate() {
        assert_eq!(a.0, b.0, "{what}: shadow mesh {k} vertices");
        assert_eq!(a.1, b.1, "{what}: shadow mesh {k} uvs");
        assert_eq!(a.2, b.2, "{what}: shadow mesh {k} triangles");
        // (`dirty` is not compared: the scene's baker uploads dirty meshes every frame and clears it)
    }
    assert_eq!(g.maker_state(), ours_maker(&host.maker), "{what}: maker actions");
}

fn random_action(r: &mut Rng, ours: &Graph, verts: &[Int2], id: i32) -> Action {
    let existing: Vec<(Int2, u8, VoxelType)> = ours
        .corners
        .iter()
        .flat_map(|&c| ours.corner(c).voxels.iter().map(move |&v| (c, v)))
        .map(|(c, v)| (ours.corner(c).hex_pos, ours.voxel(v).height, ours.voxel(v).voxel_type))
        .collect();
    let kind = r.next() % 8;
    if existing.is_empty() || kind < 5 {
        Action::new(r.pick(verts), (r.next() % 6) as u8, VoxelType::Empty, VoxelType::from_i8((r.next() % 15) as i8), id)
    } else {
        let (hex, h, t) = r.pick(&existing);
        let out = if kind < 7 { VoxelType::from_i8((r.next() % 15) as i8) } else { VoxelType::Empty };
        Action::new(hex, h, t, out, id)
    }
}

#[test]
fn loop_save_and_load_match_the_game() {
    let Some(mut h) = harness() else { return };
    let (mut g, mut grid, ao) = setup(&mut h);
    let mut ours = Graph { shadow_mesh_index_iterator: (g.ao_skip as i32) * 16383, ..Default::default() };
    ours.ground_collider.size = Vector3 { x: 1.0, y: 1.0, z: 1.0 };
    let mut host = LoopHost { ao, ..Default::default() };
    while ret::bytes(&g.g("IterateRefillPools", vec![]))[0] != 0 {}
    while ours.iterate_refill_pools(&mut || true) {}
    // an empty graph's focus first (the bounds start updated)
    settle(&mut g, &mut grid, &mut ours, &mut host);
    check(&mut g, &ours, &host, "empty graph settled");
    // corners: a patch in the middle of the grid
    let ps: Vec<Vector3> = grid.bits.iter().filter(|b| b.state == GridBitState::Done).map(|b| b.position).collect();
    let cx = ps.iter().map(|p| p.x).sum::<f32>() / ps.len() as f32;
    let cz = ps.iter().map(|p| p.z).sum::<f32>() / ps.len() as f32;
    let mut r = Rng(93);
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
    let mut id = 0;
    for step in 0..40 {
        let a = random_action(&mut r, &ours, &verts, id);
        id += 1;
        // through the queue (no click effect): Iterate integrates the corners
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
        if step % 5 == 4 {
            check(&mut g, &ours, &host, &format!("step {step}: {a:?}"));
            eprintln!("step {step}: same ({} voxels, {} shadow quads)", ours.total_voxel_count, ours.shadow_mesh_index_iterator - (g.ao_skip as i32) * 16383);
        }
    }
    // Save
    let sd = handle(&ok(g.h.request(json!({"new": "Placemaker.SaveData", "args": [], "main": true})), "new SaveData"), "SaveData");
    for (f, t) in [("corners", "Placemaker.SaveData+C, Assembly-CSharp"), ("voxels", "Placemaker.SaveData+V, Assembly-CSharp")] {
        if ok(g.h.request(Game::field(sd, f)), f)["null"] == json!(true) {
            let l = handle(&ok(g.h.request(json!({"new": list(t), "args": [], "main": true})), "new List"), "list");
            ok(g.h.request(json!({"field": f, "handle": sd, "set_handle": l, "main": true})), f);
        }
    }
    g.g("Save", vec![json!({"handle": sd})]);
    let mut save = SaveData::default();
    ours.save(&mut host, &mut save);
    let read = |g: &mut Game, f: &str, t: &str| {
        let l = handle(&ok(g.h.request(Game::field(sd, f)), f), f);
        ret::bytes(&ok(g.h.request(Game::call(l, &list(t), "ToArray", vec![], false)), "ToArray"))
    };
    assert_eq!(read(&mut g, "corners", "Placemaker.SaveData+C, Assembly-CSharp"), bits_of(&save.corners), "Save corners");
    assert_eq!(read(&mut g, "voxels", "Placemaker.SaveData+V, Assembly-CSharp"), bits_of(&save.voxels), "Save voxels");
    eprintln!("saved {} corners, {} voxels", save.corners.len(), save.voxels.len());
    assert!(save.voxels.len() > 10);
    // more building, then load the save back over it
    for _ in 0..15 {
        let a = random_action(&mut r, &ours, &verts, id);
        id += 1;
        ours.apply_action(&mut grid, &mut host, a, false);
        g.g("ApplyAction", vec![arg::raw(&bytes(&a)), arg::bool(false)]);
        settle(&mut g, &mut grid, &mut ours, &mut host);
    }
    check(&mut g, &ours, &host, "before loading");
    ours.begin_load();
    g.g("BeginLoad", vec![json!({"handle": sd})]);
    let mut n = 0;
    while ret::bytes(&g.g("IterateLoad", vec![json!({"handle": sd})]))[0] != 0 {
        n += 1;
        assert!(n < 10_000, "the game's IterateLoad never ends");
    }
    while ours.iterate_load(&mut grid, &mut host, &save) {}
    check(&mut g, &ours, &host, "loaded");
    settle(&mut g, &mut grid, &mut ours, &mut host);
    check(&mut g, &ours, &host, "loaded and settled");
    // the loaded town saves back to the same lists
    let mut again = SaveData::default();
    ours.save(&mut host, &mut again);
    assert_eq!(again, save, "a loaded town saves as it was saved");
    // an empty save: the bounds move to its single corner
    let empty = SaveData { corners: vec![SaveCorner { x: verts[0].x, y: verts[0].y, count: 0 }], voxels: Vec::<SaveVoxel>::new(), ..SaveData::default() };
    let l = handle(&ok(g.h.request(Game::field(sd, "corners")), "corners"), "corners");
    ok(g.h.request(Game::call(l, &list("Placemaker.SaveData+C, Assembly-CSharp"), "Clear", vec![], false)), "Clear");
    ok(g.h.request(Game::call(l, &list("Placemaker.SaveData+C, Assembly-CSharp"), "Add", vec![arg::raw(&bytes(&empty.corners[0]))], false)), "Add");
    let l = handle(&ok(g.h.request(Game::field(sd, "voxels")), "voxels"), "voxels");
    ok(g.h.request(Game::call(l, &list("Placemaker.SaveData+V, Assembly-CSharp"), "Clear", vec![], false)), "Clear");
    ours.begin_load();
    g.g("BeginLoad", vec![json!({"handle": sd})]);
    while ret::bytes(&g.g("IterateLoad", vec![json!({"handle": sd})]))[0] != 0 {}
    while ours.iterate_load(&mut grid, &mut host, &empty) {}
    settle(&mut g, &mut grid, &mut ours, &mut host);
    check(&mut g, &ours, &host, "empty save loaded");
    assert_eq!(ours.total_voxel_count, 0);
    eprintln!("{} host calls", host.calls.len());
}
