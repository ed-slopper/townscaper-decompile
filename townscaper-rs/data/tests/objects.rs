//! F8b: built-in object layouts against the reference install (skips without it).

use std::collections::BTreeMap;

use ts_data::install;
use ts_data::objects::{class_id as cid, AssetSet, GameObject, MonoBehaviourHeader, MonoScript, Transform};

fn assets() -> Option<AssetSet> {
    let dir = install::find_dir()?;
    Some(AssetSet::load(&dir.join("Townscaper_Data")).expect("load"))
}

#[test]
fn every_builtin_object_reads_to_its_exact_size() {
    let Some(a) = assets() else { return };
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &a.files {
        for o in &f.meta.objects {
            let b = f.bytes_of(o);
            let what = match f.class_id(o) {
                cid::GAME_OBJECT => GameObject::read(b).map(|_| "GameObject"),
                cid::TRANSFORM => Transform::read(b, false).map(|_| "Transform"),
                cid::RECT_TRANSFORM => Transform::read(b, true).map(|_| "RectTransform"),
                cid::MONO_SCRIPT => MonoScript::read(b).map(|_| "MonoScript"),
                cid::MONO_BEHAVIOUR => MonoBehaviourHeader::read(b).map(|_| "MonoBehaviour"),
                _ => continue,
            };
            let what = what.unwrap_or_else(|e| panic!("{} path id {} class {}: {e}", f.name, o.path_id, f.class_id(o)));
            *counts.entry(what).or_default() += 1;
        }
    }
    eprintln!("{counts:?}");
    assert!(counts["GameObject"] > 100 && counts["MonoScript"] > 100 && counts["MonoBehaviour"] > 100);
}

#[test]
fn transforms_and_game_objects_agree() {
    let Some(a) = assets() else { return };
    let mut checked = 0;
    for f in &a.files {
        for o in &f.meta.objects {
            let c = f.class_id(o);
            if c != cid::TRANSFORM && c != cid::RECT_TRANSFORM {
                continue;
            }
            let t = Transform::read(f.bytes_of(o), c == cid::RECT_TRANSFORM).unwrap();
            let (gf, go) = a.resolve(f, t.game_object).expect("transform's game object resolves");
            assert_eq!(gf.class_id(go), cid::GAME_OBJECT);
            let g = GameObject::read(gf.bytes_of(go)).unwrap();
            assert!(g.components.iter().any(|p| p.file_id == 0 && p.path_id == o.path_id), "{}: {} does not list its transform", f.name, g.name);
            for ch in &t.children {
                let (cf, co) = a.resolve(f, *ch).expect("child resolves");
                let child = Transform::read(cf.bytes_of(co), cf.class_id(co) == cid::RECT_TRANSFORM).unwrap();
                assert_eq!(child.father.path_id, o.path_id, "{}: child's father", f.name);
            }
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} transforms");
}

#[test]
fn every_mono_behaviour_names_a_known_script() {
    let Some(a) = assets() else { return };
    let mut by_class: BTreeMap<String, usize> = BTreeMap::new();
    let mut unresolved = 0;
    let mut null_script = 0;
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != cid::MONO_BEHAVIOUR {
                continue;
            }
            let mb = MonoBehaviourHeader::read(f.bytes_of(o)).unwrap();
            if mb.script.is_null() {
                // globalgamemanagers.assets 1115 "InputSystem.inputsettings": its package is not in the build
                null_script += 1;
                continue;
            }
            match a.script_of(f, &mb) {
                Some(s) => *by_class.entry(format!("{} [{}]", s.full_name(), s.assembly_name)).or_default() += 1,
                None => unresolved += 1,
            }
        }
    }
    for (k, v) in &by_class {
        eprintln!("{v:5} {k}");
    }
    assert_eq!(unresolved, 0, "MonoBehaviours whose script did not resolve");
    assert_eq!(null_script, 1, "MonoBehaviours with a null script");
    assert!(by_class.keys().any(|k| k.starts_with("Placemaker.WorldMaster ")));
}
