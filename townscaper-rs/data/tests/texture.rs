//! F8d: every Texture2D of the reference install (skips without it).

use std::collections::BTreeMap;

use ts_data::install;
use ts_data::objects::AssetSet;
use ts_data::texture::{self, Texture2D};

#[test]
fn every_texture_reads_and_decodes() {
    let Some(dir) = install::find_dir() else { return };
    let data_dir = dir.join("Townscaper_Data");
    let a = AssetSet::load(&data_dir).unwrap();
    let mut res = BTreeMap::new();
    let mut formats: BTreeMap<i32, usize> = BTreeMap::new();
    let (mut n, mut decoded) = (0, 0);
    for f in &a.files {
        for o in &f.meta.objects {
            if f.class_id(o) != texture::CLASS_ID {
                continue;
            }
            let t = Texture2D::read(f.bytes_of(o)).unwrap_or_else(|e| panic!("{} texture {}: {e}", f.name, o.path_id));
            *formats.entry(t.format).or_default() += 1;
            let data: Vec<u8> = if t.image_data.is_empty() && t.stream_data.size > 0 {
                let file = t.stream_data.path.rsplit('/').next().unwrap().to_string();
                let all = res.entry(file.clone()).or_insert_with(|| std::fs::read(data_dir.join(&file)).unwrap());
                all[t.stream_data.offset as usize..][..t.stream_data.size as usize].to_vec()
            } else {
                t.image_data.clone()
            };
            assert_eq!(data.len() as u32, t.complete_image_size * t.image_count.max(1) as u32, "{}: image size", t.name);
            if let Some(m0) = t.mip0_size() {
                assert!(m0 <= data.len(), "{}: mip 0 larger than the data", t.name);
                let px = t.decode_rgba8(&data).expect("decode");
                assert_eq!(px.len(), (t.width * t.height * 4) as usize);
                decoded += 1;
            }
            n += 1;
        }
    }
    eprintln!("{n} textures, {decoded} decoded; formats (TextureFormat: count): {formats:?}");
    assert!(n > 10);
}
