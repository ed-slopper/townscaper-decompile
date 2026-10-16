//! Print a scene file of the player's install as a tree: GameObject names and component kinds.
//! Output is the game's data: look at it, don't commit it.
//!   cargo run -p ts_data --example dump_scene -- level2
use ts_data::{install, objects::AssetSet, scene};
fn main() {
    let file = std::env::args().nth(1).unwrap_or_else(|| "level2".into());
    let a = AssetSet::load(&install::find_dir().expect("install").join("Townscaper_Data")).unwrap();
    for r in scene::roots(&a, &file).unwrap() {
        r.walk(0, &mut |n, d| {
            let comps: Vec<&str> = n.components.iter().filter(|c| c.class_id != 4 && c.class_id != 224).map(|c| c.kind.as_str()).collect();
            println!("{}{}{} [{}]", "  ".repeat(d), n.name, if n.active { "" } else { " (inactive)" }, comps.join(", "));
        });
    }
}
