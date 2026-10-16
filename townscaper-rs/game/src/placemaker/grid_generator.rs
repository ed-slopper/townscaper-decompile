//! `Placemaker.GridGenerator` and its `GridBit`s: the irregular grid, generated around the camera in patches,
//! clusters and bits (rows E3a–E3e). Row E3a: `GridBit`.

use crate::unity::mathematics::{Float2, Int2};

/// `GridBit.State`: the steps `GridGenerator.IterateBit` takes a bit through.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridBitState {
    #[default]
    Verts = 0,
    Relax = 1,
    Clusters = 2,
    Compose = 3,
    Mesh = 4,
    Done = 5,
}

/// `Placemaker.GridBit` (a `MonoBehaviour` under `GridGenerator.bitContainer`): one meshed piece of the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct GridBit {
    pub hex_pos: Int2,
    pub motivations: u8,
    pub relaxation_steps: u8,
    pub cluster_count: u8,
    pub patch_done: bool,
    pub state: GridBitState,
    pub verts: Vec<Float2>,
}

impl Default for GridBit {
    /// `GridBit..ctor` 0xE285F0: `verts` with capacity 256.
    fn default() -> Self {
        GridBit {
            hex_pos: Int2::default(),
            motivations: 0,
            relaxation_steps: 0,
            cluster_count: 0,
            patch_done: false,
            state: GridBitState::Verts,
            verts: Vec::with_capacity(256),
        }
    }
}

impl GridBit {
    /// `GridBit.get_done` 0xE28660.
    pub fn done(&self) -> bool {
        self.state == GridBitState::Done
    }
}
