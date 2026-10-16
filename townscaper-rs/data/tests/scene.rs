//! F8f: the scene files as GameObject trees (skips without the install).

use ts_data::install;
use ts_data::objects::{class_id as cid, AssetSet};
use ts_data::scene;

#[test]
fn every_scene_is_a_tree_of_all_its_game_objects() {
    let Some(dir) = install::find_dir() else { return };
    let a = AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    for name in ["level0", "level1", "level2"] {
        let roots = scene::roots(&a, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let f = a.file(name).unwrap();
        let game_objects = f.meta.objects.iter().filter(|o| f.class_id(o) == cid::GAME_OBJECT).count();
        let in_tree: usize = roots.iter().map(|r| r.count()).sum();
        assert_eq!(in_tree, game_objects, "{name}: every GameObject once in the tree");
        let mut unknown = Vec::new();
        for r in &roots {
            r.walk(0, &mut |n, _| {
                assert!(n.components.iter().any(|c| c.class_id == cid::TRANSFORM || c.class_id == cid::RECT_TRANSFORM), "{}: no transform", n.name);
                for c in &n.components {
                    if c.kind.starts_with("class ") || c.kind.contains("(?)") {
                        unknown.push(format!("{}: {}", n.name, c.kind));
                    }
                }
            });
        }
        assert!(unknown.is_empty(), "{name}: components without a name: {unknown:?}");
        eprintln!("{name}: {} roots, {in_tree} game objects", roots.len());
    }
}
