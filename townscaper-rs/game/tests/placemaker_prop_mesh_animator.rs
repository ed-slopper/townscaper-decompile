//! `ts_game::placemaker::prop_mesh_animator` (E10c) against the original game. Needs `TS_HARNESS=1` and the
//! install. Props made by a fresh `PropPlacer` (`GetProp`, as `placemaker_prop_placer.rs`) stand in front of the
//! scene's camera under an active parent; a fresh `PropMeshAnimator` (its own master: not dimmed, past loading,
//! the scene's big meshes, a fresh `WorldMeshes`) grows them in (`OnTurnedOn`), they animate (`PropMeshReference.Update`
//! frame by frame with the frame's `Time.time` and decay factors), some turn off while growing, the animator
//! shrinks what it holds (`OnLateUpdate`), new props on the same spots take shrinking meshes over, the rest finish
//! (`OnDone`), turn off still, shrink away; then `OnReset`. After each phase every object — the props, the
//! animator's children, the mesh filter pool — must match: where it hangs, active, layer, local pose (bit for bit),
//! mesh and material (by name), renderer, and the references' `enabled` and `time`.

mod graph_common;
mod prop_common;

use std::collections::HashMap;

use graph_common::*;
use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::prop_mesh_animator::{AnimatorHost, MeshFilterPool, PropMeshAnimator};
use ts_game::placemaker::prop_placer::{MaterialId, MeshId, MeshRefItem, PropPlacer, PropScene};
use ts_game::unity::transform::{Node, TransformId};
use ts_game::unity::vector::{v3, Vector3};

const T: &str = "UnityEngine.Transform";
const PMA: &str = "Placemaker.Props.PropMeshAnimator";

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}
fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}
fn req(h: &mut Harness, r: Value, what: &str) -> Value {
    ok(h.request(r), what)
}
fn call(t: u64, class: &str, m: &str, args: Vec<Value>, keep: bool) -> Value {
    json!({"call": m, "class": class, "handle": t, "args": args, "main": true, "keep": keep})
}
fn field(t: u64, f: &str) -> Value {
    json!({"field": f, "handle": t, "main": true})
}
fn ty(h: &mut Harness, n: &str) -> u64 {
    handle(&req(h, json!({"call": "System.Type.GetType", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true}), n), n)
}
fn new_go(h: &mut Harness, n: &str, active: bool) -> (u64, u64) {
    let go = handle(&req(h, json!({"new": "UnityEngine.GameObject", "args": [arg::string(n)], "param_types": ["System.String"], "main": true, "keep": true}), n), n);
    if !active {
        req(h, call(go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false), "SetActive");
    }
    let t = handle(&req(h, call(go, "UnityEngine.GameObject", "get_transform", vec![], true), "t"), "t");
    (go, t)
}
fn add_component(h: &mut Harness, go: u64, t: u64) -> u64 {
    handle(&req(h, json!({"call": "AddComponent", "class": "UnityEngine.GameObject", "handle": go, "args": [{"handle": t}], "param_types": ["System.Type"], "main": true, "keep": true}), "AddComponent"), "AddComponent")
}
fn get_component(h: &mut Harness, t: u64, ty: u64) -> Option<u64> {
    opt_handle(&req(h, json!({"call": "GetComponent", "class": "UnityEngine.Component", "handle": t, "args": [{"handle": ty}], "param_types": ["System.Type"], "main": true, "keep": true}), "GetComponent"))
}
fn name_of(h: &mut Harness, o: u64) -> String {
    ret::string(&req(h, call(o, "UnityEngine.Object", "get_name", vec![], false), "name"))
}
fn v3_arg(v: Vector3) -> Value {
    arg::raw(&bytes(&v))
}

/// Our host: the frame's values as the game read them (one batch with the game's call).
struct Frame {
    time: f32,
    fast: f32,
    faster: f32,
    viewport: HashMap<[u32; 3], Vector3>,
}
impl AnimatorHost for Frame {
    fn is_dimmed(&self) -> bool {
        false
    }
    fn state(&self) -> u8 {
        0x13
    }
    fn viewport_point(&mut self, p: Vector3) -> Option<Vector3> {
        Some(*self.viewport.get(&[p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).expect("a viewport point read with the call"))
    }
    fn time(&self) -> f32 {
        self.time
    }
    fn exp_t_fast(&self) -> f32 {
        self.fast
    }
    fn exp_t_faster(&self) -> f32 {
        self.faster
    }
    fn big_mesh_add(&mut self, _scene: &PropScene, _item: &MeshRefItem, _material: Option<MaterialId>) {}
    fn big_mesh_return(&mut self, _scene: &PropScene, _item: &MeshRefItem) {}
}

struct Ctx<'a> {
    h: &'a mut Harness,
    scene: PropScene,
    anim: PropMeshAnimator,
    game_anim: u64,
    camera: u64,
    mesh_ref_ty: u64,
    props: &'a prop_common::Props,
    /// Our objects to the game's transforms.
    game_of: HashMap<TransformId, u64>,
}

impl Ctx<'_> {
    /// The game's `time`, decay factors and (for `points`) viewport points, with `calls` in the same batch (one
    /// frame); the calls' replies.
    fn frame(&mut self, points: &[Vector3], calls: Vec<Value>) -> (Frame, Vec<Value>) {
        let mut reqs = vec![
            json!({"call": "UnityEngine.Time.get_time", "args": [], "main": true}),
            json!({"static_field": "Placemaker.WorldMaster.expTFast", "main": true}),
            json!({"static_field": "Placemaker.WorldMaster.expTFaster", "main": true}),
        ];
        for &p in points {
            reqs.push(json!({"call": "WorldToViewportPoint", "class": "UnityEngine.Camera", "handle": self.camera, "args": [v3_arg(p)], "param_types": ["UnityEngine.Vector3"], "main": true}));
        }
        let n = calls.len();
        reqs.extend(calls);
        let r: Vec<Value> = self.h.batch(reqs).into_iter().map(|v| ok(v, "frame")).collect();
        let f = |v: &Value| f32s(&ret::bytes(v))[0];
        let mut viewport = HashMap::new();
        for (k, &p) in points.iter().enumerate() {
            let v = f32s(&ret::bytes(&r[3 + k]));
            viewport.insert([p.x.to_bits(), p.y.to_bits(), p.z.to_bits()], v3(v[0], v[1], v[2]));
        }
        (Frame { time: f(&r[0]), fast: f(&r[1]), faster: f(&r[2]), viewport }, r[r.len() - n..].to_vec())
    }

    fn game_ref(&mut self, r: TransformId) -> u64 {
        let t = self.game_of[&r];
        get_component(self.h, t, self.mesh_ref_ty).expect("a PropMeshReference")
    }

    /// Our objects below `o` paired with the game's (child order), recorded in `game_of`.
    fn pair(&mut self, o: TransformId, g: u64, what: &str) {
        self.game_of.insert(o, g);
        let n = ret::i32(&req(self.h, call(g, T, "get_childCount", vec![], false), "childCount"));
        assert_eq!(n as usize, self.scene.transforms.child_count(o), "{what}: #{o}: child count");
        for i in 0..n {
            let c = handle(&req(self.h, call(g, T, "GetChild", vec![arg::i32(i)], true), "child"), "child");
            self.pair(self.scene.transforms.child(o, i as usize), c, what);
        }
    }

    /// Everything below the roots, both sides.
    fn check(&mut self, roots: &[(TransformId, u64)], what: &str) {
        self.game_of.clear();
        for &(o, g) in roots {
            self.pair(o, g, what);
        }
        let mut objs: Vec<(TransformId, u64)> = self.game_of.iter().map(|(&o, &g)| (o, g)).filter(|(o, _)| self.scene.objects[*o].is_some()).collect();
        objs.sort();
        let mf_ty = ty(self.h, "UnityEngine.MeshFilter, UnityEngine.CoreModule");
        let r_ty = ty(self.h, "UnityEngine.Renderer, UnityEngine.CoreModule");
        for (o, g) in objs {
            let ob = self.scene.object(o).clone();
            let w = format!("{what}: {} #{o}", ob.name);
            let go = handle(&req(self.h, call(g, "UnityEngine.Component", "get_gameObject", vec![], true), "go"), "go");
            let act = ret::bytes(&req(self.h, call(go, "UnityEngine.GameObject", "get_activeSelf", vec![], false), "active"))[0] != 0;
            assert_eq!(act, self.scene.active[o], "{w}: active");
            let layer = ret::i32(&req(self.h, call(go, "UnityEngine.GameObject", "get_layer", vec![], false), "layer"));
            assert_eq!(layer as u32, ob.layer, "{w}: layer");
            let gb = |h: &mut Harness, m: &str| bits(&f32s(&ret::bytes(&req(h, call(g, T, m, vec![], false), m))));
            let (lp, lq, ls) = (self.scene.transforms.local_position(o), self.scene.transforms.local_rotation(o), self.scene.transforms.local_scale(o));
            assert_eq!(gb(self.h, "get_localPosition"), bits(&[lp.x, lp.y, lp.z]), "{w}: localPosition");
            assert_eq!(gb(self.h, "get_localRotation"), bits(&[lq.x, lq.y, lq.z, lq.w]), "{w}: localRotation");
            assert_eq!(gb(self.h, "get_localScale"), bits(&[ls.x, ls.y, ls.z]), "{w}: localScale");
            if let Some(mf) = get_component(self.h, g, mf_ty) {
                let m = opt_handle(&req(self.h, call(mf, "UnityEngine.MeshFilter", "get_sharedMesh", vec![], true), "mesh"));
                let gn = m.map(|m| name_of(self.h, m));
                let on = ob.mesh.map(|m| match m {
                    MeshId::Asset(a) => self.props.mesh_names[&a].clone(),
                    MeshId::Clone(_) | MeshId::World(_) => panic!("{w}: no mesh copies here"),
                });
                assert_eq!(gn, on, "{w}: mesh");
            }
            if let Some(r) = get_component(self.h, g, r_ty) {
                let en = ret::bytes(&req(self.h, call(r, "UnityEngine.Renderer", "get_enabled", vec![], false), "enabled"))[0] != 0;
                assert_eq!(Some(en), ob.renderer_enabled, "{w}: renderer enabled");
                let m = opt_handle(&req(self.h, call(r, "UnityEngine.Renderer", "get_sharedMaterial", vec![], true), "material"));
                let gn = m.map(|m| name_of(self.h, m));
                assert_eq!(gn, ob.material.map(|m| self.props.material_names[&m].clone()), "{w}: material");
            }
            if let Some(mr) = &ob.mesh_ref {
                let c = get_component(self.h, g, self.mesh_ref_ty).expect("PropMeshReference");
                let en = ret::bytes(&req(self.h, call(c, "UnityEngine.Behaviour", "get_enabled", vec![], false), "enabled"))[0] != 0;
                assert_eq!(en, mr.enabled, "{w}: PropMeshReference enabled");
                let t = f32s(&ret::bytes(&req(self.h, field(c, "time"), "time")))[0];
                assert_eq!(t.to_bits(), mr.time.to_bits(), "{w}: PropMeshReference time");
            }
        }
    }
}

#[test]
fn prop_mesh_animator_matches_the_game() {
    let props = prop_common::load();
    let Some(mut guard) = harness() else { return };
    let h: &mut Harness = &mut guard;
    h.wait_for_object("Placemaker.GridGenerator");
    let prefab_handles = prop_common::game_prefabs(h, &props);
    let master = handle(&req(h, json!({"field": "master", "on": "Placemaker.GridGenerator", "main": true}), "master"), "master");
    let mm = handle(&req(h, field(master, "materialMaster"), "materialMaster"), "mm");
    let door = handle(&req(h, field(mm, "doorMaterial"), "door"), "door");
    let door_name = name_of(h, door);
    let door_id = props.material_names.iter().find(|(_, n)| **n == door_name).map(|(&i, _)| i);
    // the game side: a placer on the scene master, an animator with a master of its own
    let (pgo, _) = new_go(h, "ts_check prop placer", false);
    let pty = ty(h, "Placemaker.PropPlacer, Assembly-CSharp");
    let placer = add_component(h, pgo, pty);
    req(h, json!({"field": "master", "handle": placer, "set_handle": master, "main": true}), "placer master");
    let mut scene = PropScene::default();
    let mut placer_ts = vec![];
    for f in ["propPoolContainer", "secondaryPropsContainer", "deconstructionContainer"] {
        let (_, t) = new_go(h, &format!("ts_check {f}"), false);
        req(h, json!({"field": f, "handle": placer, "set_handle": t, "main": true}), f);
        placer_ts.push(scene.add_host(Node::default(), false));
    }
    req(h, call(placer, "Placemaker.PropPlacer", "OnEnable", vec![], false), "OnEnable");
    let mut ours_placer = PropPlacer {
        src_props: vec![],
        prop_pools: HashMap::new(),
        prop_pool_container: placer_ts[0],
        secondary_props_container: placer_ts[1],
        deconstruction_container: placer_ts[2],
        door_material: door_id,
        hole_material: None,
        collider_dict: HashMap::new(),
        material_meshes: vec![],
        prefab_of: HashMap::new(),
        italian_meshes: vec![],
    };
    ours_placer.on_enable(&mut scene, &props.prefabs, &[]);
    let anim_master = handle(&req(h, json!({"new": "Placemaker.WorldMaster", "args": [], "main": true}), "new master"), "master");
    req(h, json!({"field": "state", "handle": anim_master, "set_raw": "13", "main": true}), "state");
    let bmm = handle(&req(h, field(master, "bigMeshMaster"), "bigMeshMaster"), "bmm");
    req(h, json!({"field": "bigMeshMaster", "handle": anim_master, "set_handle": bmm, "main": true}), "bmm");
    let (wgo, wt) = new_go(h, "ts_check world meshes", false);
    let wty = ty(h, "Placemaker.WorldMeshes, Assembly-CSharp");
    let wm = add_component(h, wgo, wty);
    req(h, json!({"field": "worldMeshes", "handle": anim_master, "set_handle": wm, "main": true}), "worldMeshes");
    let (ago, at) = new_go(h, "ts_check prop mesh animator", false);
    let aty = ty(h, "Placemaker.Props.PropMeshAnimator, Assembly-CSharp");
    let game_anim = add_component(h, ago, aty);
    req(h, json!({"field": "master", "handle": game_anim, "set_handle": anim_master, "main": true}), "anim master");
    let anim_root = scene.add_host(Node::default(), false);
    let pool_root = scene.add_host(Node::default(), false);
    // the parent, active, in front of the camera
    let camera = handle(&req(h, json!({"call": "UnityEngine.Camera.get_main", "args": [], "main": true, "keep": true}), "camera"), "camera");
    let ct = handle(&req(h, call(camera, "UnityEngine.Component", "get_transform", vec![], true), "t"), "t");
    let cpos = f32s(&ret::bytes(&req(h, call(ct, T, "get_position", vec![], false), "pos")));
    let cfwd = f32s(&ret::bytes(&req(h, call(ct, T, "get_forward", vec![], false), "fwd")));
    let at_pos = v3(cpos[0] + cfwd[0] * 14.0, cpos[1] + cfwd[1] * 14.0, cpos[2] + cfwd[2] * 14.0);
    // (inactive, so that Unity runs no `PropMeshReference.Update` of its own; active only while turning off)
    let (parent_go, parent_t) = new_go(h, "ts_check animated props", false);
    req(h, json!({"call": "set_position", "class": T, "handle": parent_t, "args": [v3_arg(at_pos)], "main": true}), "set_position");
    let parent = scene.add_host(Node::default(), false);
    scene.transforms.set_position(parent, at_pos);
    let mesh_ref_ty = ty(h, "Placemaker.Props.PropMeshReference, Assembly-CSharp");
    let mut cx = Ctx { h, scene, anim: PropMeshAnimator { root: anim_root, pool: MeshFilterPool { root: pool_root } }, game_anim, camera, mesh_ref_ty, props: &props, game_of: HashMap::new() };
    let roots = |parent_t: u64| vec![(parent, parent_t), (anim_root, at), (pool_root, wt)];
    // props with mesh references, in a row along x
    let with_refs: Vec<usize> = (0..props.prefabs.len()).filter(|&i| props.prefabs[i].nodes.iter().any(|n| n.mesh.is_some() && n.has_mesh_filter)).take(8).collect();
    let mut made: Vec<(TransformId, u64)> = vec![];
    for (k, &pf) in with_refs.iter().enumerate() {
        let gp = handle(&req(cx.h, call(placer, "Placemaker.PropPlacer", "GetProp", vec![json!({"handle": prefab_handles[pf]}), json!({"handle": parent_t})], true), "GetProp"), "prop");
        let gt = handle(&req(cx.h, call(gp, "UnityEngine.Component", "get_transform", vec![], true), "t"), "t");
        let ot = ours_placer.get_prop(&mut cx.scene, &props.prefabs, pf, Some(parent));
        let lp = v3(k as f32 * 0.9 - 3.0, 0.0, 0.0);
        req(cx.h, json!({"call": "set_localPosition", "class": T, "handle": gt, "args": [v3_arg(lp)], "main": true}), "lp");
        cx.scene.transforms.set_local_position(ot, lp);
        made.push((ot, gt));
    }
    cx.check(&roots(parent_t), "made");
    // the references, by our objects
    let refs: Vec<TransformId> = (0..cx.scene.objects.len()).filter(|&o| cx.scene.objects[o].as_ref().is_some_and(|p| p.mesh_ref.is_some())).collect();
    eprintln!("{} props, {} mesh references", made.len(), refs.len());
    // their worldMaster (`IPropEnable.OnFirstEnable`), whose animator `OnDone` goes through
    req(cx.h, json!({"field": "propMeshAnimator", "handle": anim_master, "set_handle": game_anim, "main": true}), "propMeshAnimator");
    for &r in &refs {
        let gr = cx.game_ref(r);
        req(cx.h, json!({"field": "worldMaster", "handle": gr, "set_handle": anim_master, "main": true}), "worldMaster");
    }
    // time stands still while they grow (the harness's round trips outlast the half second)
    let time_scale = |cx: &mut Ctx, s: f32| {
        req(cx.h, json!({"call": "UnityEngine.Time.set_timeScale", "args": [arg::f32(s)], "param_types": ["System.Single"], "main": true}), "timeScale");
    };
    time_scale(&mut cx, 0.0);
    let turn_on = |cx: &mut Ctx, rs: &[TransformId], what: &str| {
        for &r in rs {
            let gr = cx.game_ref(r);
            let p = cx.scene.transforms.position(r);
            let (mut f, _) = cx.frame(&[p], vec![call(cx.game_anim, PMA, "OnTurnedOn", vec![json!({"handle": gr})], false)]);
            cx.anim.on_turned_on(&mut cx.scene, &mut f, r);
        }
        cx.check(&roots(parent_t), what);
    };
    let update = |cx: &mut Ctx, rs: &[TransformId], what: &str| {
        let calls: Vec<Value> = rs.iter().map(|&r| {
            let gr = cx.game_ref(r);
            call(gr, "Placemaker.Props.PropMeshReference", "Update", vec![], false)
        }).collect();
        let (mut f, _) = cx.frame(&[], calls);
        for &r in rs {
            cx.anim.update_reference(&mut cx.scene, &mut f, r);
        }
        cx.check(&roots(parent_t), what);
    };
    let late = |cx: &mut Ctx, what: &str| {
        let (mut f, _) = cx.frame(&[], vec![call(cx.game_anim, PMA, "OnLateUpdate", vec![], false)]);
        cx.anim.on_late_update(&mut cx.scene, &mut f);
        cx.check(&roots(parent_t), what);
    };
    let turn_off = |cx: &mut Ctx, rs: &[TransformId], what: &str| {
        for &r in rs {
            let gr = cx.game_ref(r);
            let (mut f, _) = cx.frame(
                &[],
                vec![
                    call(parent_go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(true)], false),
                    call(cx.game_anim, PMA, "OnTurnedOff", vec![json!({"handle": gr})], false),
                    call(parent_go, "UnityEngine.GameObject", "SetActive", vec![arg::bool(false)], false),
                ],
            );
            cx.scene.active[parent] = true;
            cx.anim.on_turned_off(&mut cx.scene, &mut f, r);
            cx.scene.active[parent] = false;
        }
        cx.check(&roots(parent_t), what);
    };
    turn_on(&mut cx, &refs, "turned on");
    for k in 0..3 {
        update(&mut cx, &refs, &format!("growing {k}"));
    }
    let (half, rest) = refs.split_at(refs.len() / 2);
    turn_off(&mut cx, half, "turned off while growing");
    late(&mut cx, "shrinking 0");
    // the same props again on the same spots: they take the shrinking meshes over
    let mut again: Vec<TransformId> = vec![];
    for &(ot, gt) in &made {
        if !half.iter().any(|&r| cx.scene.transforms.is_child_of(r, ot)) {
            continue;
        }
        let pf = cx.scene.object(ot).src_prop.unwrap();
        let gp = handle(&req(cx.h, call(placer, "Placemaker.PropPlacer", "GetProp", vec![json!({"handle": prefab_handles[pf]}), json!({"handle": parent_t})], true), "GetProp"), "prop");
        let gt2 = handle(&req(cx.h, call(gp, "UnityEngine.Component", "get_transform", vec![], true), "t"), "t");
        let ot2 = ours_placer.get_prop(&mut cx.scene, &props.prefabs, pf, Some(parent));
        let lp = cx.scene.transforms.local_position(ot);
        req(cx.h, json!({"call": "set_localPosition", "class": T, "handle": gt2, "args": [v3_arg(lp)], "main": true}), "lp");
        cx.scene.transforms.set_local_position(ot2, lp);
        let _ = gt;
        again.extend((0..cx.scene.objects.len()).filter(|&o| cx.scene.objects[o].as_ref().is_some_and(|p| p.mesh_ref.is_some()) && cx.scene.transforms.is_child_of(o, ot2)));
    }
    cx.check(&roots(parent_t), "made again");
    for &r in &again {
        let gr = cx.game_ref(r);
        req(cx.h, json!({"field": "worldMaster", "handle": gr, "set_handle": anim_master, "main": true}), "worldMaster");
    }
    turn_on(&mut cx, &again, "taken over");
    let live: Vec<TransformId> = rest.iter().chain(&again).copied().collect();
    update(&mut cx, &live, "growing again");
    // past their time: done
    time_scale(&mut cx, 1.0);
    std::thread::sleep(std::time::Duration::from_millis(700));
    time_scale(&mut cx, 0.0);
    update(&mut cx, &live, "done");
    turn_off(&mut cx, &live, "turned off still");
    for k in 0..40 {
        late(&mut cx, &format!("shrinking {k}"));
        if cx.scene.transforms.child_count(anim_root) == 0 {
            break;
        }
    }
    let (_, _) = cx.frame(&[], vec![call(cx.game_anim, PMA, "OnReset", vec![], false)]);
    cx.anim.on_reset(&mut cx.scene);
    time_scale(&mut cx, 1.0);
    cx.check(&roots(parent_t), "reset");
}
