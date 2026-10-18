//! `Placemaker.TexturePngMaster` (row G3d): the colour textures as PNG files players can edit, in
//! `Textures\` under the save folder (`TownColor.png`, `TownMaterial.png`, `TownPalette.png`, by each texture's
//! `diskName`). On enable each file is written from the game's texture when missing, and read back into a
//! copy of it when it changed since the last read (also on regaining focus); each is set as its shader global;
//! a new palette re-applies the AO baker's material colours, and a changed house or palette texture re-bakes
//! `_BakedTex` (the house texture blitted through `bakeShader` into a render texture four times its size, with
//! mip maps). Reset deletes the house and type files and reads again.
//!
//! The textures, PNG coding, render texture, blit and files are the host's ([`PngHost`]).

use crate::unity::dotnet::DateTime;

/// `TexturePngMaster.PngTexture`.
#[derive(Clone, Debug, PartialEq)]
pub struct PngTexture {
    pub src_tex: Option<u64>,
    pub game_tex: Option<u64>,
    pub last_read_time: DateTime,
    pub disk_name: String,
    pub shader_name: String,
}

impl PngTexture {
    /// `PngTexture.get_tex` 0x4FC810: the game's copy when there is one, else the source.
    pub fn tex(&self) -> Option<u64> {
        self.game_tex.or(self.src_tex)
    }
}

/// What the master reaches outside itself.
pub trait PngHost {
    /// `master.uiMaster.DefaultPath(folder)` (G7a).
    fn default_path(&self, folder: &str) -> String;
    fn file_exists(&self, path: &str) -> bool;
    fn directory_exists(&self, path: &str) -> bool;
    fn create_directory(&mut self, path: &str);
    /// `File.GetLastWriteTimeUtc(path)`.
    fn last_write_time_utc(&self, path: &str) -> DateTime;
    fn read_all_bytes(&self, path: &str) -> Vec<u8>;
    /// `File.Create(path)` and the bytes written to it.
    fn write_all_bytes(&mut self, path: &str, bytes: &[u8]);
    fn delete_file(&mut self, path: &str);
    /// `DateTime.UtcNow`.
    fn utc_now(&self) -> DateTime;
    fn log(&mut self, message: &str);
    /// `Object.Instantiate(texture)`.
    fn instantiate_texture(&mut self, tex: u64) -> u64;
    /// `ImageConversion.LoadImage(tex, bytes)`.
    fn load_image(&mut self, tex: u64, bytes: &[u8]);
    /// `ImageConversion.EncodeToPNG(tex)`.
    fn encode_to_png(&mut self, tex: u64) -> Vec<u8>;
    /// `Shader.SetGlobalTexture(name, tex)`.
    fn set_global_texture(&mut self, name: &str, tex: u64);
    /// `master.aoBaker.ApplyMaterialColors()`.
    fn apply_material_colors(&mut self);
    /// `master.materialMaster.CreatePaletteTexture()`.
    fn create_palette_texture(&mut self) -> u64;
    /// The texture's width and height.
    fn texture_size(&self, tex: u64) -> (i32, i32);
    /// `new RenderTexture(width, height, 24) { useMipMap = true }`.
    fn new_render_texture(&mut self, width: i32, height: i32) -> u64;
    /// `new Material(bakeShader)`.
    fn new_bake_material(&mut self) -> u64;
    /// `Graphics.Blit(src, dst, material)`.
    fn blit(&mut self, src: u64, dst: u64, material: u64);
    /// `enabled`.
    fn set_enabled(&mut self, enabled: bool);
    fn enabled(&self) -> bool;
}

/// `TexturePngMaster.bakedTex`'s shader global.
pub const BAKED_TEX: &str = "_BakedTex";
/// The folder under the save folder.
pub const TEXTURES_FOLDER: &str = "Textures";

/// The component.
#[derive(Clone, Debug, PartialEq)]
pub struct TexturePngMaster {
    pub house_tex: PngTexture,
    pub type_tex: PngTexture,
    pub palette_tex: PngTexture,
    pub baked_tex: Option<u64>,
    pub bake_material: Option<u64>,
    pub bake_dirty: bool,
}

/// Which of the three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    House,
    Type,
    Palette,
}

impl TexturePngMaster {
    fn png(&mut self, w: Which) -> &mut PngTexture {
        match w {
            Which::House => &mut self.house_tex,
            Which::Type => &mut self.type_tex,
            Which::Palette => &mut self.palette_tex,
        }
    }

    /// `Textures\<diskName>.png` (`"{0}{1}{2}.png"` with the folder and the separator).
    fn file(host: &dyn PngHost, png: &PngTexture) -> (String, String) {
        let dir = host.default_path(TEXTURES_FOLDER);
        let file = format!("{dir}\\{}.png", png.disk_name);
        (dir, file)
    }

    /// `TexturePngMaster.BakePalette` 0x31F190: the render texture (made once, 4× the house texture, mip
    /// mapped, `_BakedTex`) and the material (once), then the house texture blitted through it.
    fn bake_palette(&mut self, host: &mut dyn PngHost) {
        if self.baked_tex.is_none() {
            let src = self.house_tex.tex().expect("BakePalette: no house texture");
            let (w, h) = host.texture_size(src);
            let rt = host.new_render_texture(w * 4, h * 4);
            self.baked_tex = Some(rt);
            host.set_global_texture(BAKED_TEX, rt);
        }
        if self.bake_material.is_none() {
            self.bake_material = Some(host.new_bake_material());
        }
        let src = self.house_tex.tex().expect("BakePalette: no house texture");
        host.blit(src, self.baked_tex.unwrap(), self.bake_material.unwrap());
    }

    /// `TexturePngMaster.CleanTexture(pngTexture)` 0x31F350: its file deleted (logged) if there is one.
    fn clean_texture(&mut self, host: &mut dyn PngHost, w: Which) {
        let (_, file) = Self::file(host, self.png(w));
        if host.file_exists(&file) {
            host.log(&format!("Deleting PNG {file}"));
            host.delete_file(&file);
        }
    }

    /// `TexturePngMaster.ReadOrWriteTexture(pngTexture)` 0x31F680: a file changed since the last read is loaded
    /// into the game's copy (made from the source when missing), set as its global (a palette re-applies the
    /// material colours): true; a missing file is written from the source (the folder made) and the source set
    /// as the global: false; an unchanged file: false.
    fn read_or_write_texture(&mut self, host: &mut dyn PngHost, w: Which) -> bool {
        let (dir, file) = Self::file(host, self.png(w));
        if host.file_exists(&file) {
            if host.last_write_time_utc(&file).ticks() > self.png(w).last_read_time.ticks() {
                host.log(&format!("Loading PNG {file}"));
                if self.png(w).game_tex.is_none() {
                    let src = self.png(w).src_tex.expect("no source texture");
                    let copy = host.instantiate_texture(src);
                    self.png(w).game_tex = Some(copy);
                }
                let bytes = host.read_all_bytes(&file);
                let game = self.png(w).game_tex.unwrap();
                host.load_image(game, &bytes);
                let now = host.utc_now();
                let p = self.png(w);
                p.last_read_time = now;
                let name = p.shader_name.clone();
                host.set_global_texture(&name, game);
                if w == Which::Palette {
                    host.apply_material_colors();
                }
                return true;
            }
        } else {
            if !host.directory_exists(&dir) {
                host.create_directory(&dir);
            }
            host.log(&format!("Creating texture {file}"));
            let src = self.png(w).src_tex.expect("no source texture");
            let bytes = host.encode_to_png(src);
            host.write_all_bytes(&file, &bytes);
            let now = host.utc_now();
            let p = self.png(w);
            p.last_read_time = now;
            let name = p.shader_name.clone();
            host.set_global_texture(&name, src);
        }
        false
    }

    /// `TexturePngMaster.ReadAllFromDisk` 0x31F620: the three read or written; a changed house or palette
    /// texture marks the bake dirty; a dirty bake re-bakes (the flag stays set, as compiled).
    fn read_all_from_disk(&mut self, host: &mut dyn PngHost) {
        if self.read_or_write_texture(host, Which::House) {
            self.bake_dirty = true;
        }
        self.read_or_write_texture(host, Which::Type);
        if self.read_or_write_texture(host, Which::Palette) {
            self.bake_dirty = true;
        }
        if self.bake_dirty {
            self.bake_palette(host);
        }
    }

    /// `TexturePngMaster.IOnOnEnable.OnOnEnable(worldMaster)` 0x31F4E0: a palette texture made by the material
    /// master (and copied) when there is none, then [`Self::read_all_from_disk`], and the component enabled.
    pub fn on_on_enable(&mut self, host: &mut dyn PngHost) {
        if self.palette_tex.src_tex.is_none() {
            let t = host.create_palette_texture();
            self.palette_tex.src_tex = Some(t);
            let copy = host.instantiate_texture(t);
            self.palette_tex.game_tex = Some(copy);
        }
        self.read_all_from_disk(host);
        host.set_enabled(true);
    }

    /// `TexturePngMaster.OnApplicationFocus(focus)` 0x31F490: regaining focus while enabled reads again.
    pub fn on_application_focus(&mut self, host: &mut dyn PngHost, focus: bool) {
        if focus && host.enabled() {
            self.read_all_from_disk(host);
        }
    }

    /// `TexturePngMaster.ResetColorAndMaterial` 0x31FA00: the house and type files deleted, then read again
    /// (so written from the game's textures).
    pub fn reset_color_and_material(&mut self, host: &mut dyn PngHost) {
        self.clean_texture(host, Which::House);
        self.clean_texture(host, Which::Type);
        self.read_all_from_disk(host);
    }

    /// `TexturePngMaster.IOnScenePostProcess.OnScenePostProcess` 0x31F4C0: disabled in the built scene.
    pub fn on_scene_post_process(&mut self, host: &mut dyn PngHost) {
        host.set_enabled(false);
    }
}
