//! `Placemaker.AoBaker` (rows E4d, E9b): ambient occlusion. The graph writes shadow quads into the baker's
//! shadow meshes (`GetShadowMesh`); `BakeAo` uploads them and lets the baker's camera render the voxels into a
//! texture atlas of height slices (`_VoxelTex`), which `OnPostRender` spreads into `_AoTex`. The marching-squares
//! lookup texture (`OnStart`) maps a cell's four corner values to a colour index; `ApplyMaterialColors` recolours
//! it. The CPU side (the atlas layout, the lookup texture) is plain Rust; the passes are described by
//! [`AoPasses`] for Bevy's renderer: checking them needs the shaders (row C3). Checked against the running game
//! by `tests/placemaker_ao_baker.rs` (`TS_HARNESS=1`).

use crate::unity::geometry::{Bounds, Color, Color32};
use crate::unity::mathematics::{Float2, Int4};
use crate::unity::mathf;
use crate::unity::vector::{v4, Vector2, Vector3, Vector4};

/// `AoBaker.shadowMeshMaxVertCount`: four vertices a quad, 16,383 quads a mesh.
pub const SHADOW_MESH_MAX_VERT_COUNT: i32 = 65532;
/// `AoBaker.texSize`: the voxel and AO textures are 2048².
pub const TEX_SIZE: i32 = 2048;
/// `AoBaker.texelSize` (1 / 2048).
pub const TEXEL_SIZE: f32 = 0.00048828125;
/// `AoBaker.uncoveredCol` (`.cctor` 0x7B4050: white) and `uncoveredCol32`.
pub const UNCOVERED_COL: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

/// The shader globals and properties (`.cctor` 0x7B4050: `tex3dUvSize`, `tex3dWorldBounds`; the rest by name).
pub mod ids {
    pub const TEX3D_UV_SIZE: &str = "_Tex3dUvSize";
    pub const TEX3D_WORLD_BOUNDS: &str = "_Tex3dWorldBounds";
    pub const MAIN_TEX: &str = "_MainTex";
    pub const VOXEL_TEX: &str = "_VoxelTex";
    pub const AO_TEX: &str = "_AoTex";
}

/// The baker's passes for Bevy's renderer (STANDIN: a description, not a pass; checking it needs the shaders,
/// row C3).
/// - `OnStart` 0x7B3980: `renderTexture0` and `renderTexture1` (2048², 24-bit depth; `onTextures` is told);
///   materials from `stackShader`, `shader`, `spreaderShader`, `marchingSquaresShader`; the lookup texture
///   ([`marching_squares_texture`]) blitted through the marching-squares material into
///   `coloredMarchingSquaresTex` (same size, 24-bit depth); then `Restore`.
/// - `Restore` 0x7B3FA0 (also `OnEnable` 0x7B3800 once there is a material): `_MainTex` of the AO and stack
///   materials = the coloured lookup; globals `_VoxelTex` = `renderTexture0`, `_AoTex` = `renderTexture1`; the
///   camera renders into `renderTexture0`.
/// - `BakeAo` turns the camera on; it renders the voxels (the shadow meshes) into `renderTexture0`;
///   `OnPostRender` 0x7B3870 blits `renderTexture0` into `renderTexture1` through the spreader and turns the
///   camera off.
#[derive(Clone, Debug, PartialEq)]
pub struct AoPasses {
    pub tex_size: i32,
    pub depth_bits: i32,
    pub lookup_size: (i32, i32),
}

impl Default for AoPasses {
    fn default() -> Self {
        let (w, h, _) = marching_squares_layout();
        AoPasses { tex_size: TEX_SIZE, depth_bits: 24, lookup_size: (w, h) }
    }
}

/// `OnStart`'s lookup size: `n = RoundToInt(17^4)` cells (four corners of 17 values each) in 2x2 texel blocks,
/// `w = 2 * NextPowerOfTwo((int)sqrt(n))`, `h = 2 * NextPowerOfTwo(n / that)`: (1024, 512, 83521).
pub fn marching_squares_layout() -> (i32, i32, i32) {
    let n = mathf::round_to_int(17f64.powi(4) as f32);
    let side = mathf::next_power_of_two((n as f64).sqrt() as i32);
    let w = side * 2;
    let h = mathf::next_power_of_two(n / side) * 2;
    (w, h, n)
}

/// `OnStart`'s `powers`: `RoundToInt(17^k)`, k = 0..3 (a cell's index from its corners' values).
pub fn marching_squares_powers() -> Int4 {
    let p = |k: i32| mathf::round_to_int(17f64.powi(k) as f32);
    Int4 { x: p(0), y: p(1), z: p(2), w: p(3) }
}

/// `OnStart`'s `marchingSquaresTex` pixels (row-major from the bottom row, as `SetPixels32` takes them). The
/// colours are 16 greys `(i + 0.5) / 16` with alpha 0, then `uncoveredCol32`. Cell `i` is the 2x2 block at
/// `(2i mod w, 2 * (2i / w))`: bottom-left corner value `a`, bottom-right `b`, top-right `c`, top-left `d`, its
/// digits base 17 (`a` the lowest). Texels past the last cell stay clear.
pub fn marching_squares_texture() -> Vec<Color32> {
    let (w, h, n) = marching_squares_layout();
    let mut colors: Vec<Color32> = (0..16)
        .map(|i| {
            let g = (i as f32 + 0.5) * 0.0625;
            Color32::from_color(Color { r: g, g, b: g, a: 0.0 })
        })
        .collect();
    colors.push(Color32::from_color(UNCOVERED_COL));
    let mut px = vec![Color32 { r: 0, g: 0, b: 0, a: 0 }; (w * h) as usize];
    let (mut a, mut b, mut c, mut d) = (0usize, 0usize, 0usize, 0usize);
    let wu = w as usize;
    for i in 0..n {
        let l = i * 2;
        let base = ((l / w) * w + l) as usize;
        px[base] = colors[a];
        px[base + 1] = colors[b];
        px[base + wu + 1] = colors[c];
        px[base + wu] = colors[d];
        a += 1;
        if a > 16 {
            a = 0;
            b += 1;
            if b > 16 {
                b = 0;
                c += 1;
                if c > 16 {
                    c = 0;
                    d += 1;
                    if d > 16 {
                        break;
                    }
                }
            }
        }
    }
    px
}

/// What `BakeAo` reaches in the engine.
pub trait AoHost {
    /// `Shader.SetGlobalVector`.
    fn set_global_vector(&mut self, name: &str, v: Vector4);
    /// A dirty shadow mesh's `Mesh.SetVertices` / `SetUVs(0, ..)` / `SetTriangles(.., 0)` and `bounds =
    /// new Bounds(zero, float.MaxValue each way)` (always drawn).
    fn upload_shadow_mesh(&mut self, _index: usize, _mesh: &ShadowMesh) {}
}

/// `Placemaker.Graphs.ShadowMesh`: one dynamic mesh of shadow quads (four vertices each, two triangles), on its
/// own `GameObject` under the baker (with a second renderer for the stack material: Bevy's side).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShadowMesh {
    pub shadow_mesh_verts: Vec<Vector3>,
    pub shadow_mesh_uvs: Vec<Vector2>,
    pub shadow_mesh_tris: Vec<i32>,
    pub dirty: bool,
}

/// `Placemaker.AoBaker` (`.ctor` 0x7B41B0: no shadow meshes).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AoBaker {
    pub shadow_meshes: Vec<ShadowMesh>,
    /// `powers`: the weights of the four corner values in a marching-squares cell index.
    pub powers: Int4,
    /// `marchingSquaresTex`'s width and height (texels).
    pub marching_squares_tex_size: (i32, i32),
    /// `size`: a height slice's share of the atlas (after `BakeAo`).
    pub size: Float2,
    /// `texParams` (`_Tex3dUvSize`).
    pub tex_params: Vector4,
    pub count_x: i32,
    pub count_y: i32,
    pub total_count: i32,
    /// `cam.enabled`: the voxels render into `_VoxelTex` this frame.
    pub camera_enabled: bool,
}

impl AoBaker {
    /// `Placemaker.AoBaker.GetShadowMesh` 0x7B3500: the mesh and first vertex of shadow quad `shadow_index`
    /// (16,383 quads per mesh, `index * 4 / 65532`). Past the last mesh a new one is made and returned, whatever
    /// the index asked for (the original does the same).
    pub fn get_shadow_mesh(&mut self, shadow_index: i32) -> (usize, i32) {
        let v = shadow_index.wrapping_mul(4);
        let (mesh, vert) = (v / 0xFFFC, v % 0xFFFC);
        if (mesh as i64) < self.shadow_meshes.len() as i64 {
            return (mesh as usize, vert);
        }
        self.shadow_meshes.push(ShadowMesh::default());
        (self.shadow_meshes.len() - 1, vert)
    }

    /// `Placemaker.AoBaker.OnStart` 0x7B3980, the CPU side: the lookup texture's size and `powers` (the texture
    /// is [`marching_squares_texture`]; the passes [`AoPasses`]).
    pub fn on_start(&mut self) {
        let (w, h, _) = marching_squares_layout();
        self.marching_squares_tex_size = (w, h);
        self.powers = marching_squares_powers();
    }

    /// `Placemaker.AoBaker.BakeAo` 0x7B2F60: the atlas for the town's shadow meshes. The graph's
    /// `shadowMeshBounds` grows 2 in x and z; a slice is `size = (size.x, size.z)`, there are `boundsHeight + 1`
    /// of them, laid out `countX` across and `countY = Ceil(total / countX)` down, `countX` the first that keeps
    /// the atlas's longer side shortest; `size` is then the slice's share of that side. `_Tex3dUvSize = (size,
    /// size.x / bounds.x, countX)`, `_Tex3dWorldBounds = (min.x, min.z, size.x, size.z)`. The dirty shadow meshes
    /// are uploaded (the last first) and the camera turned on.
    pub fn bake_ao(&mut self, shadow_mesh_bounds: Bounds, bounds_height: i32, host: &mut dyn AoHost) {
        let mut b = shadow_mesh_bounds;
        let e = b.extents;
        b.extents = Vector3 { x: e.x + 2.0, y: e.y + 0.0, z: e.z + 2.0 };
        let bs = b.size();
        self.size = Float2 { x: bs.x, y: bs.z };
        self.total_count = bounds_height.wrapping_add(1);
        let mut best = f32::MAX;
        let mut i = 1;
        while i <= self.total_count {
            let cy = mathf::ceil_to_int(self.total_count as f32 / i as f32);
            let wx = i as f32 * self.size.x;
            let wy = cy as f32 * self.size.y;
            let m = if wy.is_nan() || wy < wx { wx } else { wy };
            if best <= m {
                break;
            }
            self.count_x = i;
            self.count_y = cy;
            best = m;
            i += 1;
        }
        let sx = self.size.x / best;
        let sy = self.size.y / best;
        self.size = Float2 { x: sx, y: sy };
        self.tex_params = v4(sx, sy, sx / b.size().x, self.count_x as f32);
        host.set_global_vector(ids::TEX3D_UV_SIZE, self.tex_params);
        let (min, size) = (b.min(), b.size());
        host.set_global_vector(ids::TEX3D_WORLD_BOUNDS, v4(min.x, min.z, size.x, size.z));
        for k in (0..self.shadow_meshes.len()).rev() {
            if self.shadow_meshes[k].dirty {
                host.upload_shadow_mesh(k, &self.shadow_meshes[k]);
                self.shadow_meshes[k].dirty = false;
            }
        }
        self.camera_enabled = true;
    }

    /// `Placemaker.AoBaker.OnPostRender` 0x7B3870: after the spreader blit ([`AoPasses`]) the camera turns off.
    pub fn on_post_render(&mut self) {
        self.camera_enabled = false;
    }

    /// `Placemaker.AoBaker.ApplyMaterialColors` 0x7B2E50: once the lookup textures and material exist, the
    /// lookup is blitted through the marching-squares material again (the host's) and the AO rebaked.
    pub fn apply_material_colors(&mut self, has_textures: bool, shadow_mesh_bounds: Bounds, bounds_height: i32, host: &mut dyn AoHost) {
        if has_textures {
            self.bake_ao(shadow_mesh_bounds, bounds_height, host);
        }
    }

    /// `UnityEngine.Texture.get_texelSize` of `marchingSquaresTex`: `(1 / width, 1 / height)`.
    pub fn marching_squares_texel_size(&self) -> Vector2 {
        let (w, h) = self.marching_squares_tex_size;
        Vector2 { x: 1.0 / w as f32, y: 1.0 / h as f32 }
    }
}
