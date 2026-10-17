//! `Placemaker.AoBaker` (row E9): so far the shadow-mesh store the graph writes its shadow quads into
//! (`GetShadowMesh`, `ShadowMesh`) and the marching-squares inputs it reads (row E4d). The bakes themselves
//! (render passes) come with E9.

use crate::unity::mathematics::Int4;
use crate::unity::vector::{Vector2, Vector3};

/// `Placemaker.Graphs.ShadowMesh`: one dynamic mesh of shadow quads (four vertices each, two triangles), on its
/// own `GameObject` under the baker (with a second renderer for the stack material: Bevy's side).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShadowMesh {
    pub shadow_mesh_verts: Vec<Vector3>,
    pub shadow_mesh_uvs: Vec<Vector2>,
    pub shadow_mesh_tris: Vec<i32>,
    pub dirty: bool,
}

/// `Placemaker.AoBaker`: the parts the graph uses.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AoBaker {
    pub shadow_meshes: Vec<ShadowMesh>,
    /// `powers`: the weights of the four corner values in a marching-squares cell index.
    pub powers: Int4,
    /// `marchingSquaresTex`'s width and height (texels).
    pub marching_squares_tex_size: (i32, i32),
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

    /// `UnityEngine.Texture.get_texelSize` of `marchingSquaresTex`: `(1 / width, 1 / height)`.
    pub fn marching_squares_texel_size(&self) -> Vector2 {
        let (w, h) = self.marching_squares_tex_size;
        Vector2 { x: 1.0 / w as f32, y: 1.0 / h as f32 }
    }
}
