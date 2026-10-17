//! The town's materials and palette (row E7d): `Placemaker.MaterialMaster` (the palette texture the module
//! uvs index, its shader globals), `Palette` and `PaletteHolder` (the voxel colours and the selected one). Plain
//! data: the materials are asset references, the texture its pixels, the shader globals go through
//! [`MaterialHost`]. Checked against the running game by `tests/placemaker_materials.rs` (`TS_HARNESS=1`).

use crate::placemaker::modules::AssetRef;
use crate::unity::geometry::{rgba, Color, Color32};
use crate::unity::vector::Vector4;

/// Where the material master's shader globals go (`Shader.SetGlobalVector` / `SetGlobalColor`).
pub trait MaterialHost {
    fn set_global_vector(&mut self, _name: &str, _value: Vector4) {}
    fn set_global_color(&mut self, _name: &str, _value: Color) {}
}

/// `Placemaker.Palette` (a `ScriptableObject`; `..ctor` 0x6FE670 makes 32 colours, all zero).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub voxel_colors: Vec<Color32>,
}

impl Default for Palette {
    fn default() -> Self {
        Palette { voxel_colors: vec![Color32::default(); 32] }
    }
}

/// `Placemaker.MaterialMaster.VoxelMaterial` (`..ctor` 0x1E73E0).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct VoxelMaterial {
    pub color: Color32,
}

/// `Placemaker.MaterialMaster`'s fields.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialMaster {
    pub window_material: AssetRef,
    pub ground_material: AssetRef,
    pub house_material: AssetRef,
    pub water_material: AssetRef,
    pub hole0: AssetRef,
    pub prop_material: AssetRef,
    pub door_material: AssetRef,
    pub mirror_house_material: AssetRef,
    pub house_outline_material: AssetRef,
    pub fence_material: AssetRef,
    pub grid_material: AssetRef,
    pub voxel_material: AssetRef,
    pub border_material: AssetRef,
    pub background_color: Color,
    pub grass_color: Color,
    pub roof_color: Color,
    pub plate_color: Color,
    pub stone_color: Color,
    /// `materialTexture` (set from `CreatePaletteTexture` by its users).
    pub material_texture: Option<PaletteTexture>,
}

impl Default for MaterialMaster {
    /// `Placemaker.MaterialMaster..ctor` 0x32CA40: background blue, grass green, roof yellow, plate red, stone
    /// gray (the scene's asset sets them all).
    fn default() -> Self {
        MaterialMaster {
            window_material: AssetRef::default(),
            ground_material: AssetRef::default(),
            house_material: AssetRef::default(),
            water_material: AssetRef::default(),
            hole0: AssetRef::default(),
            prop_material: AssetRef::default(),
            door_material: AssetRef::default(),
            mirror_house_material: AssetRef::default(),
            house_outline_material: AssetRef::default(),
            fence_material: AssetRef::default(),
            grid_material: AssetRef::default(),
            voxel_material: AssetRef::default(),
            border_material: AssetRef::default(),
            background_color: Color::BLUE,
            grass_color: Color::GREEN,
            roof_color: Color::YELLOW,
            plate_color: Color::RED,
            stone_color: Color::GRAY,
            material_texture: None,
        }
    }
}

/// The palette texture `CreatePaletteTexture` makes: 16 × 2 pixels of the default `RGBA32` format (`SetPixel`
/// stores each colour as bytes, `Color32(Color)`), clamped, point filtered, row-major from the bottom row
/// (`pixels[x + y * 16]`).
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteTexture {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<Color32>,
}

impl MaterialMaster {
    /// `Placemaker.MaterialMaster.CreatePaletteTexture` 0x32C7C0: `Mathf.NextPowerOfTwo(16)` × 2; row 1 all
    /// `(0, 0, 0, 0)`; row 0 the palette's first 15 colours, then `stoneColor` at x = 15.
    pub fn create_palette_texture(&self, palette: &Palette) -> PaletteTexture {
        let w = 16; // Mathf.NextPowerOfTwo(16)
        let mut pixels = vec![Color32::from_color(rgba(0.0, 0.0, 0.0, 0.0)); (w * 2) as usize];
        for i in 0..15 {
            let c = *palette.voxel_colors.get(i).unwrap_or_else(|| panic!("IndexOutOfRangeException: voxelColors[{i}]"));
            pixels[i] = Color32::from_color(c.to_color());
        }
        pixels[15] = Color32::from_color(self.stone_color);
        PaletteTexture { width: w, height: 2, pixels }
    }

    /// `Placemaker.MaterialMaster.OnStart` 0x32C900: `_ShadingTexLightStack = (1, 1, 1, 0)`.
    pub fn on_start(&self, host: &mut dyn MaterialHost) {
        host.set_global_vector("_ShadingTexLightStack", Vector4 { x: 1.0, y: 1.0, z: 1.0, w: 0.0 });
    }

    /// `Placemaker.MaterialMaster.Placemaker.WorldMaster.IOnOnEnable.OnOnEnable` 0x32C980: `_GrassColor`,
    /// `_GroundColor` (the background colour × 0.6, alpha too) and `_RoofColor`.
    pub fn on_on_enable(&self, host: &mut dyn MaterialHost) {
        host.set_global_color("_GrassColor", self.grass_color);
        let b = self.background_color;
        host.set_global_color("_GroundColor", rgba(b.r * 0.6, b.g * 0.6, b.b * 0.6, b.a * 0.6));
        host.set_global_color("_RoofColor", self.roof_color);
    }
}

/// `Placemaker.PaletteHolder` (`..ctor` 0x6FCAF0: `onNewColor` starts as a no-op; `OnStart` is empty).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PaletteHolder {
    pub palette: Palette,
    pub selected_color_index: i32,
}

impl PaletteHolder {
    /// `Placemaker.PaletteHolder.SelectNextPicker` 0x6FCA30: `(selected + count + delta) % count` (C#'s
    /// remainder: negative when the sum is), then `onNewColor(index)`.
    pub fn select_next_picker(&mut self, delta: i32, on_new_color: &mut dyn FnMut(i32)) {
        let n = self.palette.voxel_colors.len() as i32;
        if n == 0 {
            panic!("DivideByZeroException: PaletteHolder.SelectNextPicker");
        }
        self.selected_color_index = self.selected_color_index.wrapping_add(n).wrapping_add(delta).wrapping_rem(n);
        on_new_color(self.selected_color_index);
    }

    /// `Placemaker.PaletteHolder.SelectPicker` 0x6FCAA0.
    pub fn select_picker(&mut self, i: i32, on_new_color: &mut dyn FnMut(i32)) {
        self.selected_color_index = i;
        on_new_color(i);
    }
}
