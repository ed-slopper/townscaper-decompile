//! E5d: our `ModuleLibrary` (read from the install by `ts_data::modules`) against the game's loaded one, field
//! by field, then `Setup`'s `cornerDict` and `GetDecorOverlap`. Needs `TS_HARNESS=1`.

mod module_common;

use std::collections::HashMap;

use serde_json::{json, Value};
use ts_check::{arg, harness, ret, Harness};
use ts_game::placemaker::modules::{AssetRef, ByteQube, ModuleLibrary, OrientedModule, PropPlacement};

const LIB: &str = "Placemaker.Modules.ModuleLibrary";

fn ok(r: Value, what: &str) -> Value {
    assert_eq!(r["ok"], json!(true), "{what}: {r}");
    r
}
fn handle(r: &Value, what: &str) -> u64 {
    r["handle"].as_u64().unwrap_or_else(|| panic!("{what}: no handle in {r}"))
}
fn bytes<T: Copy>(v: &T) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }.to_vec()
}

fn list_class(elem: &str) -> String {
    format!("System.Collections.Generic.List`1[[{elem}]]")
}

struct G<'a> {
    h: &'a mut Harness,
}

impl G<'_> {
    fn batch(&mut self, reqs: Vec<Value>, what: &str) -> Vec<Value> {
        let mut out = Vec::with_capacity(reqs.len());
        for chunk in reqs.chunks(2000) {
            let r = self.h.batch(chunk.to_vec());
            assert_eq!(r.len(), chunk.len(), "{what}: batch replies");
            out.extend(r.into_iter().map(|v| ok(v, what)));
        }
        out
    }
    fn field(obj: u64, f: &str) -> Value {
        json!({"field": f, "handle": obj, "main": true})
    }
    fn call(obj: u64, class: &str, m: &str, args: Vec<Value>, keep: bool) -> Value {
        json!({"call": m, "class": class, "handle": obj, "args": args, "main": true, "keep": keep})
    }
    /// The objects of a `List<class>` held by handle `l`.
    fn items(&mut self, l: u64, elem: &str, n: usize) -> Vec<u64> {
        let lc = list_class(elem);
        let reqs = (0..n).map(|i| G::call(l, &lc, "get_Item", vec![arg::i32(i as i32)], true)).collect();
        self.batch(reqs, "get_Item").iter().map(|r| handle(r, "item")).collect()
    }
    /// One reference field of each object.
    fn fields(&mut self, objs: &[u64], f: &str) -> Vec<Value> {
        self.batch(objs.iter().map(|&o| G::field(o, f)).collect(), f)
    }
    /// The element bytes of a value-type list field of each object (`List.ToArray`).
    fn value_lists(&mut self, objs: &[u64], f: &str, elem: &str) -> Vec<Vec<u8>> {
        let ls: Vec<u64> = self.fields(objs, f).iter().map(|r| handle(r, f)).collect();
        let lc = list_class(elem);
        self.batch(ls.iter().map(|&l| G::call(l, &lc, "ToArray", vec![], false)).collect(), f).iter().map(ret::bytes).collect()
    }
    fn count(&mut self, l: u64, elem: &str) -> usize {
        ret::i32(&ok(self.h.request(G::call(l, &list_class(elem), "get_Count", vec![], false)), "Count")) as usize
    }
}

fn oriented_bytes(v: &[OrientedModule]) -> Vec<u8> {
    v.iter().flat_map(|o| [o.module_index.to_le_bytes()[0], o.module_index.to_le_bytes()[1], o.flags, 0]).collect()
}

/// A reference stored as an asset pointer must match the game's object pointers one to one.
struct RefMap {
    what: &'static str,
    ours_to_game: HashMap<AssetRef, u32>,
    game_to_ours: HashMap<u32, AssetRef>,
}
impl RefMap {
    fn new(what: &'static str) -> RefMap {
        RefMap { what, ours_to_game: HashMap::new(), game_to_ours: HashMap::new() }
    }
    fn check(&mut self, ours: AssetRef, game: u32) {
        assert_eq!(ours.path_id == 0, game == 0, "{}: null {ours:?} vs {game:#x}", self.what);
        assert_eq!(*self.ours_to_game.entry(ours).or_insert(game), game, "{}: {ours:?}", self.what);
        assert_eq!(*self.game_to_ours.entry(game).or_insert(ours), ours, "{}: {game:#x}", self.what);
    }
}

const PROP_PLACEMENT_SIZE: usize = 0x14;

fn check_props(ours: &[PropPlacement], game: &[u8], props: &mut RefMap, what: &str) {
    assert_eq!(game.len(), ours.len() * PROP_PLACEMENT_SIZE, "{what}: prop placements");
    for (p, g) in ours.iter().zip(game.chunks(PROP_PLACEMENT_SIZE)) {
        let mut want = bytes(&p.pos);
        want.extend(bytes(&p.scale));
        want.extend(bytes(&p.rotation_byte));
        assert_eq!(g[..9], want[..], "{what}: prop placement {p:?}");
        assert_eq!([g[0x10], g[0x11]], [p.corner as u8, p.prio_add], "{what}: prop corner / prio");
        props.check(p.prop, u32::from_le_bytes(g[0xC..0x10].try_into().unwrap()));
    }
}

fn base_lists(g: &mut G, objs: &[u64], is_module: bool) -> [Vec<Vec<u8>>; 4] {
    [
        g.value_lists(objs, "profileIndexes", "System.UInt16"),
        g.value_lists(objs, "orientedModuleMeshes", "Placemaker.Modules.OrientedModule, Assembly-CSharp"),
        g.value_lists(objs, "propPlacements", "Placemaker.PropPlacement, Assembly-CSharp"),
        if is_module { g.value_lists(objs, "decorModules", "Placemaker.Modules.OrientedModule, Assembly-CSharp") } else { vec![] },
    ]
}

#[test]
fn module_library_matches_the_game() {
    let Some(mut h) = harness() else { return };
    let mut ours = module_common::our_module_library();
    let mut g = G { h: &mut h };
    let lf = |g: &mut G, f: &str| ok(g.h.request(json!({"field": f, "on": LIB, "main": true})), f);
    let mut props = RefMap::new("prop");

    // value lists of the library itself
    let vlist = |g: &mut G, f: &str, elem: &str| {
        let l = handle(&lf(g, f), f);
        ret::bytes(&ok(g.h.request(G::call(l, &list_class(elem), "ToArray", vec![], false)), f))
    };
    let ws: Vec<u8> = ours.water_sides.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(vlist(&mut g, "waterSides", "System.UInt32"), ws, "waterSides");
    let dov: Vec<u8> = ours.decor_overlaps.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(vlist(&mut g, "decorOverlaps", "System.Int32"), dov, "decorOverlaps");
    let sl: Vec<u8> = ours.side_lookups.iter().flat_map(bytes).collect();
    assert_eq!(vlist(&mut g, "sideLookups", "Placemaker.SideProfile, Assembly-CSharp"), sl, "sideLookups");
    assert_eq!(ret::bytes(&lf(&mut g, "groundUv0")), bytes(&ours.ground_uv0), "groundUv0");
    assert_eq!(ret::bytes(&lf(&mut g, "groundUv1")), bytes(&ours.ground_uv1), "groundUv1");

    // modules
    let l = handle(&lf(&mut g, "modules"), "modules");
    assert_eq!(g.count(l, "Placemaker.Module, Assembly-CSharp"), ours.modules.len(), "modules");
    let ms = g.items(l, "Placemaker.Module, Assembly-CSharp", ours.modules.len());
    let cost = g.fields(&ms, "cost");
    let links = g.fields(&ms, "cornerLinks");
    let [pi, omm, pp, dm] = base_lists(&mut g, &ms, true);
    for (i, m) in ours.modules.iter().enumerate() {
        let what = format!("modules[{i}]");
        assert_eq!(ret::bytes(&cost[i])[..2], m.base.cost.to_le_bytes(), "{what}.cost");
        assert_eq!(ret::i32(&links[i]), m.corner_links, "{what}.cornerLinks");
        assert_eq!(pi[i], m.base.profile_indexes.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>(), "{what}.profileIndexes");
        assert_eq!(omm[i], oriented_bytes(&m.base.oriented_module_meshes), "{what}.orientedModuleMeshes");
        assert_eq!(dm[i], oriented_bytes(&m.decor_modules), "{what}.decorModules");
        check_props(&m.base.prop_placements, &pp[i], &mut props, &what);
    }

    // decor modules
    let l = handle(&lf(&mut g, "decorModules"), "decorModules");
    assert_eq!(g.count(l, "Placemaker.DecorModule, Assembly-CSharp"), ours.decor_modules.len(), "decorModules");
    let ds = g.items(l, "Placemaker.DecorModule, Assembly-CSharp", ours.decor_modules.len());
    let cost = g.fields(&ds, "cost");
    let [pi, omm, pp, _] = base_lists(&mut g, &ds, false);
    for (i, m) in ours.decor_modules.iter().enumerate() {
        let what = format!("decorModules[{i}]");
        assert_eq!(ret::bytes(&cost[i])[..2], m.base.cost.to_le_bytes(), "{what}.cost");
        assert_eq!(pi[i], m.base.profile_indexes.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>(), "{what}.profileIndexes");
        assert_eq!(omm[i], oriented_bytes(&m.base.oriented_module_meshes), "{what}.orientedModuleMeshes");
        check_props(&m.base.prop_placements, &pp[i], &mut props, &what);
    }

    // corner lookups
    let l = handle(&lf(&mut g, "cornerLookups"), "cornerLookups");
    let cl_elem = "Placemaker.Modules.CornerLookup, Assembly-CSharp";
    assert_eq!(g.count(l, cl_elem), ours.corner_lookups.len(), "cornerLookups");
    let cs = g.items(l, cl_elem, ours.corner_lookups.len());
    let values = g.fields(&cs, "values");
    let oms = g.value_lists(&cs, "orientedModules", "Placemaker.Modules.OrientedModule, Assembly-CSharp");
    for (i, c) in ours.corner_lookups.iter().enumerate() {
        assert_eq!(ret::bytes(&values[i]), c.values.v.to_vec(), "cornerLookups[{i}].values");
        assert_eq!(oms[i], oriented_bytes(&c.oriented_modules), "cornerLookups[{i}].orientedModules");
    }

    // module meshes (outline uvs are objects: every 8th mesh read in full)
    let l = handle(&lf(&mut g, "moduleMeshes"), "moduleMeshes");
    let mm_elem = "Placemaker.ModuleMesh, Assembly-CSharp";
    assert_eq!(g.count(l, mm_elem), ours.module_meshes.len(), "moduleMeshes");
    let mesh = g.items(l, mm_elem, ours.module_meshes.len());
    let s3 = "Os.Utils.SbyteFloat3, Assembly-CSharp";
    let verts = g.value_lists(&mesh, "verts", s3);
    let normals = g.value_lists(&mesh, "normals", s3);
    let tangents = g.value_lists(&mesh, "tangents", s3);
    let concavity = g.value_lists(&mesh, "concavity", "Os.Utils.ByteFloat, Assembly-CSharp");
    let uvs = g.value_lists(&mesh, "uvs", "Os.Utils.ByteFloat2, Assembly-CSharp");
    let tris = g.value_lists(&mesh, "tris", "System.Byte");
    let mtype = g.fields(&mesh, "materialType");
    let corner = g.fields(&mesh, "corner");
    let mats: Vec<u64> = g.fields(&mesh, "material").iter().map(|r| handle(r, "material")).collect();
    let mat_addr = g.batch(mats.iter().map(|&m| json!({"addr": m})).collect(), "addr");
    let mut materials = RefMap::new("material");
    let ol: Vec<u64> = g.fields(&mesh, "outlineUvs").iter().map(|r| handle(r, "outlineUvs")).collect();
    let ol_elem = "Placemaker.Modules.OutlineUv, Assembly-CSharp";
    for (i, m) in ours.module_meshes.iter().enumerate() {
        let what = format!("moduleMeshes[{i}]");
        let flat = |v: &[ts_game::os::utils::SbyteFloat3]| v.iter().flat_map(|s| [s.x as u8, s.y as u8, s.z as u8]).collect::<Vec<u8>>();
        assert_eq!(verts[i], flat(&m.verts), "{what}.verts");
        assert_eq!(normals[i], flat(&m.normals), "{what}.normals");
        assert_eq!(tangents[i], flat(&m.tangents), "{what}.tangents");
        assert_eq!(concavity[i], m.concavity.iter().map(|c| c.value).collect::<Vec<u8>>(), "{what}.concavity");
        assert_eq!(uvs[i], m.uvs.iter().flat_map(|u| [u.x, u.y]).collect::<Vec<u8>>(), "{what}.uvs");
        assert_eq!(tris[i], m.tris, "{what}.tris");
        assert_eq!(ret::bytes(&mtype[i])[0], m.material_type as u8, "{what}.materialType");
        assert_eq!(ret::bytes(&corner[i])[0], m.corner, "{what}.corner");
        materials.check(m.material, mat_addr[i]["addr"].as_u64().unwrap() as u32);
        assert_eq!(g.count(ol[i], ol_elem), m.outline_uvs.len(), "{what}.outlineUvs");
        if i % 8 == 0 && !m.outline_uvs.is_empty() {
            let us = g.items(ol[i], ol_elem, m.outline_uvs.len());
            let b = g.fields(&us, "b");
            for (k, u) in m.outline_uvs.iter().enumerate() {
                assert_eq!(ret::bytes(&b[k])[0], u.b, "{what}.outlineUvs[{k}]");
            }
        }
    }

    // Setup: cornerDict
    ours.setup();
    let d = handle(&lf(&mut g, "cornerDict"), "cornerDict");
    let dc = "System.Collections.Generic.Dictionary`2[[Placemaker.ByteQube, Assembly-CSharp],[System.Int32]]";
    assert_eq!(ret::i32(&ok(g.h.request(G::call(d, dc, "get_Count", vec![], false)), "Count")) as usize, ours.corner_dict.len(), "cornerDict.Count");
    let keys: Vec<ByteQube> = ours.corner_lookups.iter().map(|c| c.values).collect();
    let got = g.batch(keys.iter().map(|k| G::call(d, dc, "get_Item", vec![arg::raw(&k.v)], false)).collect(), "cornerDict[k]");
    for (k, r) in keys.iter().zip(&got) {
        assert_eq!(ret::i32(r), ours.corner_dict[k], "cornerDict[{k:?}]");
    }

    // GetDecorOverlap: pairs that overlap (found by us) and random ones
    let mut s = 77u32;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let mut pairs = vec![];
    // the decor references the modules hold, paired with each other
    let decor: Vec<OrientedModule> = {
        let mut v: Vec<OrientedModule> = ours.modules.iter().flat_map(|m| m.decor_modules.iter().copied()).collect();
        v.sort_by_key(|o| (o.module_index, o.flags));
        v.dedup();
        v
    };
    'find: for &x in &decor {
        for &y in &decor {
            {
                if ours.get_decor_overlap(x, y) {
                    pairs.push((x, y));
                    if pairs.len() == 150 {
                        break 'find;
                    }
                }
            }
        }
    }
    assert!(!pairs.is_empty(), "some decor pairs overlap");
    for _ in 0..350 {
        let m = |r: u32| OrientedModule { module_index: (r % 140) as u16, flags: (r >> 8) as u8, pad: 0 };
        pairs.push((m(rnd()), m(rnd())));
    }
    let got = g.batch(
        pairs.iter().map(|(a, b)| json!({"call": format!("{LIB}.GetDecorOverlap"), "on": LIB, "args": [arg::raw(&bytes(a)), arg::raw(&bytes(b))], "main": true})).collect(),
        "GetDecorOverlap",
    );
    for ((a, b), r) in pairs.iter().zip(&got) {
        assert_eq!(ret::bytes(r)[0] != 0, ours.get_decor_overlap(*a, *b), "GetDecorOverlap({a:?}, {b:?})");
    }
    let _: &ModuleLibrary = &ours;
}
