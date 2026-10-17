//! Print the prop prefabs the module library places (row E10b1): each prefab's tree with its components and
//! their script fields' names. Output is the game's data: look at it, don't commit it.
//!   cargo run -p ts_data --example dump_props
use std::collections::BTreeSet;

use ts_data::layout::Value;
use ts_data::objects::{class_id, AssetSet, MonoBehaviourHeader};
use ts_data::{install, modules, scene};

fn main() {
    let a = AssetSet::load(&install::find_dir().expect("install").join("Townscaper_Data")).unwrap();
    let lib = modules::module_library(&a).expect("ModuleLibrary");
    // the library's file
    let mut lib_file = String::new();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != class_id::MONO_BEHAVIOUR {
                continue;
            }
            let Ok(mb) = MonoBehaviourHeader::read(f.bytes_of(o)) else { continue };
            if a.script_of(f, &mb).map(|s| s.full_name()).as_deref() == Some("Placemaker.Modules.ModuleLibrary") {
                lib_file = f.name.clone();
            }
        }
    }
    let mut props = BTreeSet::new();
    for m in lib.modules.iter().chain(lib.decor_modules.iter()) {
        for p in &m.prop_placements {
            props.insert((p.prop.file_id, p.prop.path_id));
        }
    }
    println!("library in {lib_file}: {} distinct props", props.len());
    for (file_id, path_id) in props {
        let t = scene::subtree(&a, &lib_file, ts_data::objects::PPtr { file_id, path_id }).unwrap();
        println!("== {} ({} nodes, file {})", t.name, t.count(), t.file);
        t.walk(0, &mut |n, d| {
            let comps: Vec<String> = n
                .components
                .iter()
                .filter(|c| c.class_id != 4)
                .map(|c| match &c.fields {
                    Some(Value::Struct(f)) => format!("{}{{{}}}", c.kind, f.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(",")),
                    _ => c.kind.clone(),
                })
                .collect();
            println!("{}{}{} [{}]", "  ".repeat(d), n.name, if n.active { "" } else { " (inactive)" }, comps.join(", "));
        });
    }
}
