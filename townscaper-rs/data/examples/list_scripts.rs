//! Print every script a MonoBehaviour of the build uses, as `Assembly|Namespace|Class` (input of tools/layouts.ps1).
use std::collections::BTreeSet;
use ts_data::{install, objects::{class_id, AssetSet, MonoBehaviourHeader}};
fn main() {
    let a = AssetSet::load(&install::find_dir().expect("install").join("Townscaper_Data")).unwrap();
    let mut out = BTreeSet::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != class_id::MONO_BEHAVIOUR { continue; }
            let mb = MonoBehaviourHeader::read(f.bytes_of(o)).unwrap();
            if let Some(s) = a.script_of(f, &mb) {
                out.insert(format!("{}|{}|{}", s.assembly_name.trim_end_matches(".dll"), s.namespace, s.class_name));
            }
        }
    }
    for s in out { println!("{s}"); }
}
