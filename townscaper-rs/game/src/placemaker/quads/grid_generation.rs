//! `Placemaker.Quads.GridGeneration`: the records `GridGenerator` (rows E3b–E3e) builds the grid in, and the
//! hand-made patch library it draws from (row E3a). The library's data is the game's `CustomGridLibrary`
//! asset, read by `ts_data::grid`.

use crate::unity::mathematics::{Float2, Int2, Int3, Random};

/// `Placemaker.Quads.GridGeneration.GridConstants` (all `const`).
pub mod grid_constants {
    /// `GridConstants.hexScale`: grid hex positions are hex coordinates × 12.
    pub const HEX_SCALE: u16 = 12;
    /// `GridConstants.hexInvScale`: 1/12.
    pub const HEX_INV_SCALE: f32 = 0.083333336;
    /// `GridConstants.planeToHex`.
    pub const PLANE_TO_HEX: f32 = 8.22;
    /// `GridConstants.hexToPlane`: 1/8.22.
    pub const HEX_TO_PLANE: f32 = 0.121654496;
    /// `GridConstants.hexDistance`.
    pub const HEX_DISTANCE: u16 = 72;
    /// `GridConstants.centroidScale`.
    pub const CENTROID_SCALE: u16 = 32;
}

/// `Placemaker.Quads.GridGeneration.MotivationCounter`: how many neighbours want a patch / cluster to exist.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MotivationCounter {
    pub count: u8,
}

impl MotivationCounter {
    /// `MotivationCounter.AddMotivation` 0x334480: `++count` (wraps as a byte); the source is not kept.
    pub fn add_motivation(&mut self, _source: Int2) -> i32 {
        self.count = self.count.wrapping_add(1);
        self.count as i32
    }

    /// `MotivationCounter.RemoveMotivation` 0x334500: `--count`.
    pub fn remove_motivation(&mut self, _source: Int2) -> i32 {
        self.count = self.count.wrapping_sub(1);
        self.count as i32
    }

    /// `MotivationCounter.GetEnumerator` 0x3344A0 (`<GetEnumerator>d__3`): the sources are not kept, so the
    /// enumeration is always empty (its `MoveNext` 0x1F0980 returns false).
    pub fn iter(&self) -> std::iter::Empty<Int2> {
        std::iter::empty()
    }
}

/// `HexPatch.State`: the steps `GridGenerator.IteratePatch` takes a patch through.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HexPatchState {
    #[default]
    Verts = 0,
    Triangulate = 1,
    Quadrangulate = 2,
    Subdivide = 3,
    Sort = 4,
    Done = 5,
}

/// `Placemaker.Quads.GridGeneration.HexPatch`: one hexagonal patch of the grid (its corners and its quads as
/// corner indexes, four per quad).
#[derive(Clone, Debug, PartialEq)]
pub struct HexPatch {
    pub hex_pos: Int2,
    pub quad_tri_cutoff: u16,
    pub quads: Vec<u8>,
    pub verts: Vec<Int2>,
    pub motivations: MotivationCounter,
    pub state: HexPatchState,
    pub subdivisions: u8,
}

impl Default for HexPatch {
    /// `HexPatch..ctor` 0x3DE030: `quads` with capacity 900, `verts` 256.
    fn default() -> Self {
        HexPatch {
            hex_pos: Int2::default(),
            quad_tri_cutoff: 0,
            quads: Vec::with_capacity(900),
            verts: Vec::with_capacity(256),
            motivations: MotivationCounter::default(),
            state: HexPatchState::Verts,
            subdivisions: 0,
        }
    }
}

impl HexPatch {
    /// `HexPatch.get_done` 0x3DE0D0.
    pub fn done(&self) -> bool {
        self.state == HexPatchState::Done
    }
}

/// `Placemaker.Quads.GridGeneration.HexCluster`: patches grouped for relaxation.
#[derive(Clone, Debug, PartialEq)]
pub struct HexCluster {
    pub hex_pos: Int2,
    pub motivations: MotivationCounter,
    pub relaxation_steps: u8,
    pub patch_count: u8,
    pub done: bool,
    pub patch_vert_offsets: Int3,
    pub vert_indexes: Vec<i16>,
    pub relaxed_verts: Vec<Float2>,
}

impl Default for HexCluster {
    /// `HexCluster..ctor` 0x3DBFE0: both lists with capacity 700.
    fn default() -> Self {
        HexCluster {
            hex_pos: Int2::default(),
            motivations: MotivationCounter::default(),
            relaxation_steps: 0,
            patch_count: 0,
            done: false,
            patch_vert_offsets: Int3::default(),
            vert_indexes: Vec::with_capacity(700),
            relaxed_verts: Vec::with_capacity(700),
        }
    }
}

/// `Placemaker.Quads.GridGeneration.CustomGridPatch`: a hand-made patch (`CustomGridPatch..ctor` 0x4499C0
/// makes both lists empty).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomGridPatch {
    pub verts: Vec<Int2>,
    pub quads: Vec<u8>,
    pub quad_tri_cutoff: u16,
    pub subdivisions: u8,
}

/// Which of the library's index lists `CustomGridLibrary.GetRandom` draws from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchList {
    Generic,
    Straight,
    Centroid,
    CentroidCurve,
}

/// `Placemaker.Quads.GridGeneration.CustomGridLibrary` (a `ScriptableObject`): every hand-made patch, and
/// lists of patch indexes by use (255: no patch).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomGridLibrary {
    pub all_custom_patches: Vec<CustomGridPatch>,
    pub generic_patches: Vec<u8>,
    pub straight_patches: Vec<u8>,
    pub centroid_patches: Vec<u8>,
    pub centroid_curve_patches: Vec<u8>,
}

impl CustomGridLibrary {
    pub fn list(&self, which: PatchList) -> &[u8] {
        match which {
            PatchList::Generic => &self.generic_patches,
            PatchList::Straight => &self.straight_patches,
            PatchList::Centroid => &self.centroid_patches,
            PatchList::CentroidCurve => &self.centroid_curve_patches,
        }
    }

    /// `CustomGridLibrary.GetRandom` 0x449820: `list[random.NextUInt((uint)list.Count)]` (inlined: the state
    /// before the step, times the count, high word), `None` for 255.
    pub fn get_random(&self, list: &[u8], random: &mut Random) -> Option<&CustomGridPatch> {
        let s = random.next_state();
        let i = ((list.len() as u32 as u64 * s as u64) >> 32) as usize;
        let index = *list.get(i).expect("CustomGridLibrary.GetRandom: ArgumentOutOfRangeException");
        if index == 0xFF {
            return None;
        }
        Some(self.all_custom_patches.get(index as usize).expect("CustomGridLibrary.GetRandom: ArgumentOutOfRangeException"))
    }
}
