//! F8g: every MonoBehaviour of the install reads to its exact size with the generated layouts.

use std::collections::BTreeMap;

use ts_data::install;
use ts_data::layout;
use ts_data::objects::{class_id as cid, AssetSet, MonoBehaviourHeader};

#[test]
fn every_mono_behaviour_reads_its_fields_exactly() {
    let Some(dir) = install::find_dir() else { return };
    let a = AssetSet::load(&dir.join("Townscaper_Data")).unwrap();
    let (mut ok, mut no_layout) = (0, BTreeMap::<String, usize>::new());
    let mut failed: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != cid::MONO_BEHAVIOUR {
                continue;
            }
            let b = f.bytes_of(o);
            let mb = MonoBehaviourHeader::read(b).unwrap();
            let Some(s) = a.script_of(f, &mb) else { continue };
            let key = layout::script_key(&s.assembly_name, &s.namespace, &s.class_name);
            if !layout::has_script(&key) {
                *no_layout.entry(key).or_default() += 1;
                continue;
            }
            match layout::read_mono_behaviour(b, &mb, &key) {
                Ok(_) => ok += 1,
                Err(e) => {
                    let ent = failed.entry(key).or_insert((0, String::new()));
                    ent.0 += 1;
                    ent.1 = format!("{} {}: {e}", f.name, o.path_id);
                }
            }
        }
    }
    for (k, (n, e)) in &failed {
        eprintln!("FAIL {n:4} {k}  ({e})");
    }
    for (k, n) in &no_layout {
        eprintln!("NOLAYOUT {n:4} {k}");
    }
    eprintln!("{ok} read exactly, {} failed, {} without layout", failed.values().map(|v| v.0).sum::<usize>(), no_layout.values().sum::<usize>());
    assert!(failed.is_empty(), "{} scripts fail", failed.len());
    assert!(no_layout.is_empty(), "{} scripts have no layout", no_layout.len());
}
