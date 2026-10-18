//! `ts_game::placemaker::texture_png_master` (G3d) on a fake folder: missing files are written from the
//! game's textures, changed ones read into copies (a palette re-applying the material colours), unchanged ones
//! left; house or palette changes re-bake; reset deletes the house and type files and writes them again.
//! (On the harness game, after its enable on the player's files, all three were read into copies, stamped,
//! and the bake marked dirty: what this does for files newer than the default read time.)

use std::collections::HashMap;

use ts_game::placemaker::texture_png_master::{PngHost, PngTexture, TexturePngMaster};
use ts_game::unity::dotnet::DateTime;

#[derive(Default)]
struct Fake {
    files: HashMap<String, (Vec<u8>, i64)>,
    now: i64,
    next: u64,
    log: Vec<String>,
    globals: Vec<(String, u64)>,
    blits: usize,
    material_colors: usize,
    enabled: bool,
}

impl PngHost for Fake {
    fn default_path(&self, folder: &str) -> String {
        format!("C:\\save\\{folder}")
    }
    fn file_exists(&self, p: &str) -> bool {
        self.files.contains_key(p)
    }
    fn directory_exists(&self, _p: &str) -> bool {
        true
    }
    fn create_directory(&mut self, _p: &str) {}
    fn last_write_time_utc(&self, p: &str) -> DateTime {
        DateTime::from_ticks(self.files[p].1)
    }
    fn read_all_bytes(&self, p: &str) -> Vec<u8> {
        self.files[p].0.clone()
    }
    fn write_all_bytes(&mut self, p: &str, b: &[u8]) {
        self.files.insert(p.to_string(), (b.to_vec(), self.now));
    }
    fn delete_file(&mut self, p: &str) {
        self.files.remove(p);
    }
    fn utc_now(&self) -> DateTime {
        DateTime::from_ticks(self.now)
    }
    fn log(&mut self, m: &str) {
        self.log.push(m.to_string());
    }
    fn instantiate_texture(&mut self, _t: u64) -> u64 {
        self.next += 1;
        100 + self.next
    }
    fn load_image(&mut self, _t: u64, _b: &[u8]) {}
    fn encode_to_png(&mut self, t: u64) -> Vec<u8> {
        vec![t as u8]
    }
    fn set_global_texture(&mut self, n: &str, t: u64) {
        self.globals.push((n.to_string(), t));
    }
    fn apply_material_colors(&mut self) {
        self.material_colors += 1;
    }
    fn create_palette_texture(&mut self) -> u64 {
        3
    }
    fn texture_size(&self, _t: u64) -> (i32, i32) {
        (16, 16)
    }
    fn new_render_texture(&mut self, w: i32, h: i32) -> u64 {
        assert_eq!((w, h), (64, 64));
        50
    }
    fn new_bake_material(&mut self) -> u64 {
        60
    }
    fn blit(&mut self, _s: u64, _d: u64, _m: u64) {
        self.blits += 1;
    }
    fn set_enabled(&mut self, e: bool) {
        self.enabled = e;
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
}

fn png(src: Option<u64>, disk: &str, shader: &str) -> PngTexture {
    PngTexture { src_tex: src, game_tex: None, last_read_time: DateTime::default(), disk_name: disk.into(), shader_name: shader.into() }
}

#[test]
fn writes_reads_and_resets() {
    let mut host = Fake { now: 1000, ..Default::default() };
    let mut m = TexturePngMaster {
        house_tex: png(Some(1), "TownColor", "_HouseTex"),
        type_tex: png(Some(2), "TownMaterial", "_TypeTex"),
        palette_tex: png(None, "TownPalette", "_PaletteTex"),
        baked_tex: None,
        bake_material: None,
        bake_dirty: false,
    };
    // first run: no files: all written, nothing re-baked
    m.on_on_enable(&mut host);
    assert_eq!(host.files.len(), 3);
    assert!(host.enabled && !m.bake_dirty && host.blits == 0);
    assert_eq!(m.palette_tex.src_tex, Some(3));
    // a player edits the palette: read, colours re-applied, re-baked
    host.now = 2000;
    host.files.get_mut("C:\\save\\Textures\\TownPalette.png").unwrap().1 = 1500;
    m.on_application_focus(&mut host, true);
    assert_eq!(host.material_colors, 1);
    assert!(m.bake_dirty && host.blits == 1);
    // nothing changed: (the flag stays set, so the bake runs again)
    m.on_application_focus(&mut host, true);
    assert_eq!(host.material_colors, 1);
    assert_eq!(host.blits, 2);
    // reset: house and type rewritten from the textures
    host.now = 3000;
    m.reset_color_and_material(&mut host);
    assert_eq!(host.files["C:\\save\\Textures\\TownColor.png"].1, 3000);
    assert!(host.log.iter().any(|l| l.starts_with("Deleting PNG")));
}
