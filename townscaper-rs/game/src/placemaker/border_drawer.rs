//! `Placemaker.BorderDrawer` (row E9d): the outline of the buildable area under the cursor. Each frame it is asked
//! to, it fills its mesh with `GridGenerator.PopulateBorderMesh` round the hovered vert (row E3, checked) and
//! draws it with the border material, the marching-squares lookup as `_MainTex`.

use crate::placemaker::ao_baker::{self, AoBaker};
use crate::placemaker::grid_generator::{BorderMesh, GridGenerator};
use crate::placemaker::quads::Vert;
use crate::unity::mathematics::Int3x2;

/// What `Draw` hands the renderer: `Graphics.DrawMesh(mesh, Matrix4x4.identity, material, layer 0, no camera,
/// submesh 0, block)` with `block._MainTex = aoBaker.marchingSquaresTex` (`SetupBlock` 0x7C2C20).
/// STANDIN: a description for Bevy's renderer (the border shader is row C3).
#[derive(Clone, Debug, PartialEq)]
pub struct BorderDraw {
    pub mesh: BorderMesh,
    pub layer: i32,
    pub main_tex: &'static str,
}

/// `Placemaker.BorderDrawer` (`.ctor` 0x1F53D0; `BoundsUpdated` 0x1D4430 does nothing).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BorderDrawer {
    /// `mesh` (`OnStart` 0x7C2BD0 makes it; `OnEnable` 0x7C2B50 / `OnStart` set up the property block once the
    /// lookup texture exists).
    pub mesh: BorderMesh,
}

impl BorderDrawer {
    /// `Placemaker.BorderDrawer.Draw` 0x7C2A40: the border quads round `hoverData.srcVert`, drawn this frame.
    pub fn draw(&mut self, grid: &GridGenerator, src_vert: &Vert, ao: &AoBaker, border: Int3x2) -> BorderDraw {
        self.mesh = grid.populate_border_mesh(src_vert, ao.marching_squares_texel_size(), ao.powers, border);
        BorderDraw { mesh: self.mesh.clone(), layer: 0, main_tex: ao_baker::ids::MAIN_TEX }
    }
}
