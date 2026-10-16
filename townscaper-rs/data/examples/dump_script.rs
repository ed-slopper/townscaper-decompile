//! Print the top-level fields of the MonoBehaviours of one script (sizes of arrays, scalars as is).
//! The game's data: look, don't commit.   cargo run -p ts_data --example dump_script -- Placemaker.Modules.ModuleLibrary
use ts_data::{install, layout::{self, Value}, objects::{class_id, AssetSet, MonoBehaviourHeader}};
fn short(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{} items]{}", a.len(), a.first().map(|f| format!(" first: {}", short(f))).unwrap_or_default()),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        Value::Struct(f) => format!("{{{}}}", f.iter().take(6).map(|(n, v)| format!("{n}: {}", short(v))).collect::<Vec<_>>().join(", ")),
        Value::Str(s) => format!("{:?}", s.chars().take(40).collect::<String>()),
        other => format!("{other:?}").chars().take(60).collect(),
    }
}
fn main() {
    let want = std::env::args().nth(1).expect("Namespace.Class");
    let a = AssetSet::load(&install::find_dir().expect("install").join("Townscaper_Data")).unwrap();
    for f in &a.files { for o in &f.meta.objects {
        if f.class_id(o) != class_id::MONO_BEHAVIOUR { continue; }
        let b = f.bytes_of(o);
        let mb = MonoBehaviourHeader::read(b).unwrap();
        let Some(s) = a.script_of(f, &mb) else { continue };
        if s.full_name() != want { continue; }
        let v = layout::read_mono_behaviour(b, &mb, &layout::script_key(&s.assembly_name, &s.namespace, &s.class_name)).unwrap();
        println!("{} {} {:?}", f.name, o.path_id, mb.name);
        if let Value::Struct(fields) = v { for (n, v) in fields { println!("  {n}: {}", short(&v)); } }
    }}
}
