//! `Placemaker.Graphs` and its records (rows E4a–E4e): the voxel graph of a town. The game keeps it as Unity
//! objects (`Corner`, `Square`, `Qube`, `Voxel` components on pooled `GameObject`s); here they live in arenas
//! of the `Graph` and refer to each other by id. Row E4a: the records. Checked against the running game by
//! `tests/placemaker_graphs.rs` (`TS_HARNESS=1`).

use crate::unity::mathematics::{Int2, Int3};
use crate::unity::quaternion::Matrix4x4;
use crate::unity::vector::{Vector2, Vector3};

/// `Placemaker.VoxelType` (an `sbyte`): the 15 house colours, then the special kinds.
#[repr(i8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum VoxelType {
    #[default]
    Red = 0,
    Green = 1,
    Blue = 2,
    Yellow = 3,
    Color4 = 4,
    Color5 = 5,
    Color6 = 6,
    Color7 = 7,
    Color8 = 8,
    Color9 = 9,
    Color10 = 10,
    Color11 = 11,
    Color12 = 12,
    Color13 = 13,
    Color14 = 14,
    Ground = 15,
    Any = 16,
    Water = 17,
    Empty = 18,
}

impl VoxelType {
    pub fn from_i8(v: i8) -> VoxelType {
        use VoxelType::*;
        [Red, Green, Blue, Yellow, Color4, Color5, Color6, Color7, Color8, Color9, Color10, Color11, Color12, Color13, Color14, Ground, Any, Water, Empty]
            [v as usize]
    }
}

/// `Placemaker.MaterialType` (a `byte`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MaterialType {
    #[default]
    None = 0,
    Ground = 1,
    Dirt = 2,
    Water = 3,
}

/// `Placemaker.Graphs.Action`: one edit of the town (a voxel at `hexPos` / `height` changing type), as kept in
/// the undo queue and in saves. 16 bytes, padding included.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Action {
    pub hex_pos: Int2,
    pub height: u8,
    pub in_type: VoxelType,
    pub out_type: VoxelType,
    pub pad: u8,
    pub action_id: i32,
}

impl Action {
    /// `Placemaker.Graphs.Action..ctor` 0x7B2D20.
    pub fn new(hex_pos: Int2, height: u8, in_type: VoxelType, out_type: VoxelType, action_id: i32) -> Action {
        Action { hex_pos, height, in_type, out_type, pad: 0, action_id }
    }

    /// `Placemaker.Graphs.Action.GetReverse` 0x7B2A90: in and out swapped (the rest copied, padding too).
    pub fn get_reverse(&self) -> Action {
        Action { in_type: self.out_type, out_type: self.in_type, ..*self }
    }
}

macro_rules! id {
    ($name:ident, $what:literal) => {
        #[doc = concat!("A ", $what, " of the graph (its index in the graph's arena).")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);
    };
}
id!(CornerId, "`Corner`");
id!(SquareId, "`Square`");
id!(QubeId, "`Qube`");
id!(VoxelId, "`Voxel`");

/// `Placemaker.Voxel` (a component on its own `GameObject`, a child of its corner's transform).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Voxel {
    pub voxel_type: VoxelType,
    pub height: u8,
    pub cost: i32,
}

impl Voxel {
    /// `Placemaker.Voxel.System.IComparable<Placemaker.Voxel>.CompareTo` 0x3D5A30: by cost (`int.CompareTo`).
    pub fn compare_to(&self, other: &Voxel) -> i32 {
        self.cost.cmp(&other.cost) as i32
    }
}

/// `Placemaker.Graphs.Square.Relation`: the neighbouring square across one edge, and which edge of it this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SquareRelation {
    pub square: Option<SquareId>,
    pub index_in_other: i8,
}

impl SquareRelation {
    /// `Square.Relation.empty` (`Square.Relation..cctor` 0x4FCD10): no square, edge -1.
    pub const EMPTY: SquareRelation = SquareRelation { square: None, index_in_other: -1 };

    /// `Square.Relation..ctor` 0x4FCDB0.
    pub fn new(square: Option<SquareId>, index_in_other: i8) -> SquareRelation {
        SquareRelation { square, index_in_other }
    }
}

/// `Placemaker.Graphs.Qube.Relation`: the neighbouring qube across one side, and which side of it this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct QubeRelation {
    pub qube: Option<QubeId>,
    pub index_in_other: i8,
}

impl QubeRelation {
    /// `Qube.Relation.empty` (`Qube.Relation..cctor` 0x4FCD60): no qube, side -1.
    pub const EMPTY: QubeRelation = QubeRelation { qube: None, index_in_other: -1 };

    /// `Qube.Relation..ctor` 0x4FCDB0 (folded with `Square.Relation`'s).
    pub fn new(qube: Option<QubeId>, index_in_other: i8) -> QubeRelation {
        QubeRelation { qube, index_in_other }
    }
}

/// `Placemaker.Graphs.Qube.Relation6`: a qube's six neighbours (four sides, up, down).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct QubeRelation6 {
    pub r: [QubeRelation; 6],
}

impl QubeRelation6 {
    /// `Qube.Relation6.get_Item` 0x4FCA60 (the game throws `IndexOutOfRangeException` outside 0..=5).
    pub fn get(&self, key: i32) -> QubeRelation {
        *self.r.get(usize::try_from(key).unwrap_or(6)).unwrap_or_else(|| panic!("Relation6[{key}]: IndexOutOfRangeException"))
    }

    /// `Qube.Relation6.set_Item` 0x4FCB00.
    pub fn set(&mut self, key: i32, value: QubeRelation) {
        let slot = self.r.get_mut(usize::try_from(key).unwrap_or(6)).unwrap_or_else(|| panic!("Relation6[{key}]: IndexOutOfRangeException"));
        *slot = value;
    }
}

/// `Placemaker.Graphs.FlowData`: per voxel of a corner, the state of the flow field (row E4e).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FlowData {
    pub voxel: Option<VoxelId>,
    pub neighbour_count: u8,
    pub dist: f32,
    pub new_dist: f32,
    pub denom: f32,
    pub dir: Vector3,
    pub new_dir: Vector3,
    pub target: bool,
}

/// `Placemaker.Graphs.Corner`: a grid corner of the town, with the squares round it and its column of voxels
/// (the voxels are its transform's children, kept in `voxels` in sibling order).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Corner {
    pub hex_pos: Int2,
    pub squares: Vec<SquareId>,
    pub blit_verts: Vec<Vector2>,
    pub flow_datas: Vec<FlowData>,
    pub in_flow_update_queue: bool,
    pub angle: f32,
    pub ground_coverage: u8,
    /// The transform's `localPosition`.
    pub local_position: Vector3,
    /// The transform's children: the corner's voxels, in sibling order.
    pub voxels: Vec<VoxelId>,
}

impl Corner {
    /// `Placemaker.Graphs.Corner.AddSquare` 0x443F70: inserts the square by the angle of its position round
    /// the corner (`math.atan2(dx, dz)` of the local positions, ascending); `square_position` gives a square's
    /// `localPosition`.
    pub fn add_square(&mut self, square: SquareId, square_position: impl Fn(SquareId) -> Vector3) {
        let angle_to = |p: Vector3| {
            let (dx, dz) = (p.x - self.local_position.x, p.z - self.local_position.z);
            crate::unity::mathematics::atan2(dx, dz)
        };
        let a = angle_to(square_position(square));
        for i in 0..self.squares.len() {
            if a < angle_to(square_position(self.squares[i])) {
                self.squares.insert(i, square);
                return;
            }
        }
        self.squares.push(square);
    }
}

/// `Placemaker.Graphs.Square`: one grid quad of the town at its four corners, with its neighbours across each
/// edge and the corner matrices the modules are placed with.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Square {
    pub q: [SquareRelation; 4],
    pub c: [Option<CornerId>; 4],
    pub m: [Matrix4x4; 4],
    pub motivations: u8,
    pub quad_index: u8,
    /// The transform's `localPosition`.
    pub local_position: Vector3,
}

impl Square {
    /// `Placemaker.Graphs.Square.GetCorner` 0x316B50 (throws outside 0..=3).
    pub fn get_corner(&self, index: i8) -> Option<CornerId> {
        *self.c.get(index as u8 as usize).unwrap_or_else(|| panic!("Square.GetCorner({index}): IndexOutOfRangeException"))
    }

    /// `Placemaker.Graphs.Square.SetCorner` 0x3170F0.
    pub fn set_corner(&mut self, index: i8, value: Option<CornerId>) {
        *self.c.get_mut(index as u8 as usize).unwrap_or_else(|| panic!("Square.SetCorner({index}): IndexOutOfRangeException")) = value;
    }

    /// `Placemaker.Graphs.Square.GetRelation` 0x316BD0.
    pub fn get_relation(&self, index: i8) -> SquareRelation {
        *self.q.get(index as u8 as usize).unwrap_or_else(|| panic!("Square.GetRelation({index}): IndexOutOfRangeException"))
    }

    /// `Placemaker.Graphs.Square.SetRelation` 0x317180: `new Relation(square, indexInOther)` into slot `index`.
    pub fn set_relation(&mut self, index: i8, index_in_other: i8, square: Option<SquareId>) {
        *self.q.get_mut(index as u8 as usize).unwrap_or_else(|| panic!("Square.SetRelation({index}): IndexOutOfRangeException")) =
            SquareRelation::new(square, index_in_other);
    }

    /// `Placemaker.Graphs.Square.get_bitQuadId` 0x3172A0: `(quadIndex, c2.hexPos.x, c2.hexPos.y)` (`c2` must exist).
    pub fn bit_quad_id(&self, corners: &[Corner]) -> Int3 {
        let c2 = &corners[self.c[2].expect("Square.bitQuadId: NullReferenceException").0 as usize];
        Int3 { x: self.quad_index as i32, y: c2.hex_pos.x, z: c2.hex_pos.y }
    }
}

/// `Placemaker.SideProfile` (row E5 ports its methods): the profile of a module's sides.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SideProfile {
    pub v00: u16,
    pub v01: u16,
    pub v10: u16,
    pub v11: u16,
    pub v20: u16,
    pub v21: u16,
    pub v30: u16,
    pub v31: u16,
    pub v4: u16,
    pub v5: u16,
}

/// `Placemaker.Modules.OrientedModule` (row E5 ports its methods): a module index and its orientation flags.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OrientedModule {
    pub module_index: u16,
    pub flags: u8,
    pub pad: u8,
}

/// `Placemaker.Graphs.OrientedModuleSides`: a module that fits a qube, with its sides and cost.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct OrientedModuleSides {
    pub sides: SideProfile,
    pub oriented_module: OrientedModule,
    pub cost: f32,
}

/// `float.CompareTo(float)` (.NET): NaN sorts before every number and equals NaN.
pub fn single_compare_to(a: f32, b: f32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else if a.is_nan() {
        if b.is_nan() { 0 } else { -1 }
    } else {
        1
    }
}

impl OrientedModuleSides {
    /// `Placemaker.Graphs.OrientedModuleSides.System.IComparable<...>.CompareTo` 0x6FC550: by cost.
    pub fn compare_to(&self, other: &OrientedModuleSides) -> i32 {
        single_compare_to(self.cost, other.cost)
    }
}

/// `Placemaker.Graphs.SideCounts`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SideCounts {
    pub side: u16,
    pub count: u32,
}

/// `Placemaker.Graphs.Qube`: one cell of the voxel graph (a square at a height), filled with a module by the
/// wave function collapse. `cornerValues` is a `ByteQube` (row E5): its 8 bytes here.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Qube {
    pub voxel_count: u8,
    pub side_variation_mask: u8,
    pub decor_side_variation_mask: u8,
    pub shadow_index: i32,
    pub possible_module_list_index: i32,
    pub existance_index: u32,
    pub last_fill_index: u16,
    pub last_propagation: u16,
    pub cost: f32,
    pub corner_values: [u8; 8],
    pub awaiting_rebuilding: bool,
    pub awaiting_grass: bool,
    pub awaiting_graph_update: bool,
    pub awaiting_module_clearing: bool,
    pub possible_modules: Vec<OrientedModuleSides>,
}

impl Qube {
    /// `Placemaker.Graphs.Qube.get_empty` 0x843810: `possibleModuleListIndex == -1`.
    pub fn empty(&self) -> bool {
        self.possible_module_list_index == -1
    }

    /// `Placemaker.Graphs.Qube.get_fullyCovered` 0x843820: `voxelCount == 255`.
    pub fn fully_covered(&self) -> bool {
        self.voxel_count == 0xFF
    }
}
