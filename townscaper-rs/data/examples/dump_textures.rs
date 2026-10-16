//! Write the textures of the player's install as raw RGBA to a folder (for looking at them; never commit).
//!   cargo run -p ts_data --example dump_textures -- <out dir>
use ts_data::{install, objects::AssetSet, texture::{self, Texture2D}};
fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("out dir"));
    std::fs::create_dir_all(&out).unwrap();
    let data_dir = install::find_dir().expect("install").join("Townscaper_Data");
    let a = AssetSet::load(&data_dir).unwrap();
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != texture::CLASS_ID { continue; }
            let t = Texture2D::read(f.bytes_of(o)).unwrap();
            if t.image_data.is_empty() && t.stream_data.size == 0 { continue; }
            let data = if t.image_data.is_empty() {
                let all = std::fs::read(data_dir.join(t.stream_data.path.rsplit('/').next().unwrap())).unwrap();
                all[t.stream_data.offset as usize..][..t.stream_data.size as usize].to_vec()
            } else { t.image_data.clone() };
            if let Some(px) = t.decode_rgba8(&data) {
                let name = format!("{}_{}x{}_f{}.rgba", t.name.replace(|c: char| !c.is_ascii_alphanumeric(), "_"), t.width, t.height, t.format);
                std::fs::write(out.join(name), px).unwrap();
            }
        }
    }
}
