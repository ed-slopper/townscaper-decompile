//! `Placemaker.Graphs` and its records (rows E4a–E4e): the voxel graph of a town. The game keeps it as Unity
//! objects (`Corner`, `Square`, `Qube`, `Voxel` components on pooled `GameObject`s); here they live in arenas
//! of the `Graph` and refer to each other by id. Row E4a: the records; row E4b: the `Graph` structure. Checked against the running game by
//! `tests/placemaker_graphs.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;

use crate::os::hex;
use crate::unity::mathematics::{Int2, Int3, Int3x2};
use crate::unity::mathf;
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
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Voxel {
    pub voxel_type: VoxelType,
    pub height: u8,
    pub cost: i32,
    /// The transform's `localPosition` (`(0, height + 0.5, 0)` under its corner).
    pub local_position: Vector3,
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
    /// The transform's `localPosition` (the `corners` container sits at the origin: also its `position`).
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
    /// The transform's `localPosition` (the `squares` container sits at the origin: also its `position`).
    pub local_position: Vector3,
    /// The transform's children: the square's qubes, one per height, in sibling order.
    pub qubes: Vec<QubeId>,
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
    /// `Placemaker.Graphs.OrientedModuleSides.System.IComparable<Placemaker.Graphs.OrientedModuleSides>.CompareTo` 0x6FC550: by cost.
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
    /// The transform's parent: its square (`None` in the pool).
    pub square: Option<SquareId>,
    /// The transform's `localPosition` (`(0, height, 0)` in its square).
    pub local_position: Vector3,
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

/// `Graph.LoadState` (row E4d).
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LoadState {
    #[default]
    None = 0,
}

/// `Placemaker.Graphs.Graph`: the town's voxel graph. The object arenas hold every `Corner` / `Square` /
/// `Qube` / `Voxel` ever made (the game pools them, never destroys them); the lists hold the children of the
/// game's container transforms in sibling order: `corners` / `squares` (in use, sorted), the pools, and
/// `loose_voxels`. A voxel's parent is the corner whose `voxels` list holds it, a qube's its `square`.
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub corner_arena: Vec<Corner>,
    pub square_arena: Vec<Square>,
    pub qube_arena: Vec<Qube>,
    pub voxel_arena: Vec<Voxel>,
    /// `Graph.corners` (sorted by `hexPos`, x then y).
    pub corners: Vec<CornerId>,
    pub corner_pool: Vec<CornerId>,
    /// `Graph.squares` (sorted by `position.x`).
    pub squares: Vec<SquareId>,
    pub square_pool: Vec<SquareId>,
    pub voxel_pool: Vec<VoxelId>,
    pub loose_voxels: Vec<VoxelId>,
    pub qube_pool: Vec<QubeId>,
    pub qube_iterator: u32,
    pub corner_iterator: u32,
    pub square_iterator: u32,
    pub voxel_iterator: u32,
    pub total_voxel_count: u32,
    pub corner_dict: HashMap<Int2, CornerId>,
    pub square_dict: HashMap<Int2, SquareId>,
    /// `Graph.edgeDict` (an `int2x2` of the edge's two corner positions).
    pub edge_dict: HashMap<(Int2, Int2), SquareRelation>,
    pub corner_integration_queue: Vec<CornerId>,
    pub corner_flow_queue: Vec<CornerId>,
    pub qube_update_queue: Vec<QubeId>,
    pub shadow_mesh_indexes: Vec<i32>,
    pub shadow_mesh_index_iterator: i32,
    pub bounds_height: i32,
    pub bounds: Int3x2,
    pub border: Int3x2,
    pub gen_bounds: Int3x2,
    pub view_bounds: Int3x2,
    pub max_height: i32,
    pub bounds_updated: bool,
    pub recalculate_hex_bounds: bool,
    pub recalculate_height_bounds: bool,
    pub load_state: LoadState,
}

impl Default for Graph {
    /// `Graph..ctor` 0xE91870: empty dictionaries and queues, `boundsUpdated = true`.
    fn default() -> Self {
        Graph {
            corner_arena: Vec::new(),
            square_arena: Vec::new(),
            qube_arena: Vec::new(),
            voxel_arena: Vec::new(),
            corners: Vec::new(),
            corner_pool: Vec::new(),
            squares: Vec::new(),
            square_pool: Vec::new(),
            voxel_pool: Vec::new(),
            loose_voxels: Vec::new(),
            qube_pool: Vec::new(),
            qube_iterator: 0,
            corner_iterator: 0,
            square_iterator: 0,
            voxel_iterator: 0,
            total_voxel_count: 0,
            corner_dict: HashMap::new(),
            square_dict: HashMap::new(),
            edge_dict: HashMap::new(),
            corner_integration_queue: Vec::new(),
            corner_flow_queue: Vec::new(),
            qube_update_queue: Vec::new(),
            shadow_mesh_indexes: Vec::new(),
            shadow_mesh_index_iterator: 0,
            bounds_height: 0,
            bounds: Int3x2::default(),
            border: Int3x2::default(),
            gen_bounds: Int3x2::default(),
            view_bounds: Int3x2::default(),
            max_height: 0,
            bounds_updated: true,
            recalculate_hex_bounds: false,
            recalculate_height_bounds: false,
            load_state: LoadState::None,
        }
    }
}

/// `~i` (an insertion point encoded as a negative index, as `List<T>.BinarySearch` does).
fn complement(i: i32) -> i32 {
    !i
}

impl Graph {
    pub fn corner(&self, id: CornerId) -> &Corner {
        &self.corner_arena[id.0 as usize]
    }
    pub fn corner_mut(&mut self, id: CornerId) -> &mut Corner {
        &mut self.corner_arena[id.0 as usize]
    }
    pub fn square(&self, id: SquareId) -> &Square {
        &self.square_arena[id.0 as usize]
    }
    pub fn square_mut(&mut self, id: SquareId) -> &mut Square {
        &mut self.square_arena[id.0 as usize]
    }
    pub fn qube(&self, id: QubeId) -> &Qube {
        &self.qube_arena[id.0 as usize]
    }
    pub fn qube_mut(&mut self, id: QubeId) -> &mut Qube {
        &mut self.qube_arena[id.0 as usize]
    }
    pub fn voxel(&self, id: VoxelId) -> &Voxel {
        &self.voxel_arena[id.0 as usize]
    }
    pub fn voxel_mut(&mut self, id: VoxelId) -> &mut Voxel {
        &mut self.voxel_arena[id.0 as usize]
    }

    /// `Placemaker.Graphs.Graph.GetNewCorner` 0xE838F0: a new corner (`UnityFunctions.GetNewObject`); the caller
    /// puts it in its container.
    pub fn get_new_corner(&mut self) -> CornerId {
        self.corner_arena.push(Corner::default());
        CornerId(self.corner_arena.len() as u32 - 1)
    }

    /// `Placemaker.Graphs.Graph.GetNewQube` 0xE83930.
    pub fn get_new_qube(&mut self) -> QubeId {
        self.qube_arena.push(Qube::default());
        QubeId(self.qube_arena.len() as u32 - 1)
    }

    /// `Placemaker.Graphs.Graph.GetNewSquare` 0xE83970: its four relations `Square.Relation.empty`.
    pub fn get_new_square(&mut self) -> SquareId {
        self.square_arena.push(Square { q: [SquareRelation::EMPTY; 4], ..Default::default() });
        SquareId(self.square_arena.len() as u32 - 1)
    }

    /// `Placemaker.Graphs.Graph.GetNewVoxel` 0xE83A00: a new voxel (on `LayerMaster`'s voxel layer, with a
    /// convex `MeshCollider`, a `MeshRenderer` with `MaterialMaster.voxelMaterial`, a `MeshFilter`: Bevy's side).
    pub fn get_new_voxel(&mut self) -> VoxelId {
        self.voxel_arena.push(Voxel::default());
        VoxelId(self.voxel_arena.len() as u32 - 1)
    }

    /// `Placemaker.Graphs.Graph.IterateRefillPools` 0xE89B70: tops the pools up (a voxel; qubes to 24, made
    /// with `existanceIndex = 1`; corners to 7; squares to 6), asking `keep_going` after each qube / corner /
    /// square; true when it stopped early.
    pub fn iterate_refill_pools(&mut self, keep_going: &mut dyn FnMut() -> bool) -> bool {
        if self.voxel_pool.is_empty() {
            let v = self.get_new_voxel();
            self.voxel_pool.push(v);
        }
        while self.qube_pool.len() < 24 {
            self.qube_arena.push(Qube { existance_index: 1, ..Default::default() });
            self.qube_pool.push(QubeId(self.qube_arena.len() as u32 - 1));
            if !keep_going() {
                return true;
            }
        }
        while self.corner_pool.len() < 7 {
            let c = self.get_new_corner();
            self.corner_pool.push(c);
            if !keep_going() {
                return true;
            }
        }
        while self.square_pool.len() <= 5 {
            let s = self.get_new_square();
            self.square_pool.push(s);
            if !keep_going() {
                return true;
            }
        }
        false
    }

    /// `Placemaker.Graphs.Graph.BinaryTreeSearchCorner` 0xE83460: the corner at `hex_pos` in `corners` (sorted by
    /// x, then y; compared by `int` subtraction), or `~insertion point`.
    pub fn binary_tree_search_corner(&self, hex_pos: Int2) -> i32 {
        let (mut lo, mut hi) = (0i32, self.corners.len() as i32 - 1);
        let (mut mid, mut d) = (0i32, 0i32);
        while lo <= hi {
            mid = (lo + hi) / 2;
            let c = self.corner(self.corners[mid as usize]).hex_pos;
            d = if hex_pos.x == c.x { hex_pos.y.wrapping_sub(c.y) } else { hex_pos.x.wrapping_sub(c.x) };
            if d == 0 {
                return mid;
            }
            if d < 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }
        complement(if d < 1 { mid } else { mid + 1 })
    }

    /// `Placemaker.Graphs.Graph.BinaryTreeSearchSquare` 0xE83540: by `position.x` in `squares`, or `~insertion
    /// point` (with no squares: `~(0 < x ? 1 : 0)`).
    pub fn binary_tree_search_square(&self, pos: Vector3) -> i32 {
        let (mut lo, mut hi) = (0i32, self.squares.len() as i32 - 1);
        if hi < 0 {
            return complement((0.0 < pos.x) as i32);
        }
        loop {
            let mid = (hi + lo) / 2;
            let x = self.square(self.squares[mid as usize]).local_position.x;
            if pos.x == x {
                return mid;
            }
            if pos.x < x {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
            if hi < lo {
                return complement(if pos.x <= x { mid } else { mid + 1 });
            }
        }
    }

    /// `Placemaker.Graphs.Graph.BinaryTreeSearchVoxel` 0xE83620: the voxel at `height` among a corner's voxels
    /// (by `FloorToInt(localPosition.y)`), or `~insertion point`.
    pub fn binary_tree_search_voxel(&self, corner: CornerId, height: i32) -> i32 {
        let voxels = &self.corner(corner).voxels;
        let (mut lo, mut hi) = (0i32, voxels.len() as i32 - 1);
        let (mut mid, mut h) = (0i32, 0i32);
        while lo <= hi {
            mid = (hi + lo) / 2;
            h = mathf::floor_to_int(self.voxel(voxels[mid as usize]).local_position.y);
            if height == h {
                return mid;
            }
            if height < h {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }
        complement(if height <= h { mid } else { mid + 1 })
    }

    /// `Placemaker.Graphs.Graph.CheckVoxel` 0xE836E0: a voxel at `hex_pos` / `height`.
    pub fn check_voxel(&self, hex_pos: Int2, height: i32) -> bool {
        match self.corner_dict.get(&hex_pos) {
            Some(&c) => self.binary_tree_search_voxel(c, height) >= 0,
            None => false,
        }
    }

    /// `Placemaker.Graphs.Graph.get_anyVoxels` 0xE91B20: any corner in use.
    pub fn any_voxels(&self) -> bool {
        !self.corners.is_empty()
    }

    /// `Placemaker.Graphs.Graph.GetOrCreateCorner` 0xE83B50: the corner at `hex_pos`, else one from the pool
    /// (the last) or a new one, put at its sorted place in `corners`.
    pub fn get_or_create_corner(&mut self, hex_pos: Int2) -> CornerId {
        if let Some(&c) = self.corner_dict.get(&hex_pos) {
            return c;
        }
        let i = self.binary_tree_search_corner(hex_pos);
        assert!(i < 0, "GetOrCreateCorner: a corner at {hex_pos:?} that cornerDict lacks");
        let c = match self.corner_pool.pop() {
            Some(c) => c,
            None => self.get_new_corner(),
        };
        self.corners.insert(complement(i) as usize, c);
        assert!(self.corner_dict.insert(hex_pos, c).is_none(), "GetOrCreateCorner: Dictionary.Add of an existing key");
        self.corner_mut(c).hex_pos = hex_pos;
        c
    }

    /// `Placemaker.Graphs.Graph.SetEmptyBoundsCenter` 0xE8FC80.
    pub fn set_empty_bounds_center(&mut self, center_pos: Int2) {
        let h = hex::get_hex3(center_pos);
        self.bounds = Int3x2 { c0: h, c1: h };
        self.bounds_updated = true;
    }

    /// `Placemaker.Graphs.Graph.GetMaxExtents` 0xE83770: (border extent, height extent) for bounds and a height:
    /// the taller the town, the smaller it may spread, and the other way round.
    pub fn get_max_extents(bounds: Int3x2, height: i32) -> (i32, i32) {
        let mut a = mathf::inverse_lerp(64.0, 256.0, height as f32);
        let m = mathf::max_i32(mathf::max_i32(bounds.c1.x.wrapping_sub(bounds.c0.x), bounds.c1.y.wrapping_sub(bounds.c0.y)), bounds.c1.z.wrapping_sub(bounds.c0.z));
        let mut b = mathf::inverse_lerp(256.0, 512.0, m as f32 * 0.5);
        if b + a > 1.0 {
            let s = 1.0 / (b + a);
            b *= s;
            a *= s;
        }
        let height_extent = mathf::floor_to_int(mathf::lerp(256.0, 64.0, b));
        let border_extent = mathf::round_to_int(mathf::lerp(512.0, 256.0, a));
        (border_extent, height_extent)
    }

    /// `Placemaker.Graphs.Graph.IsCoordinateAllowed` 0xE875C0: inside the view bounds, below 256, and — when it
    /// grows the bounds or their height — not past the trade-off between height (64..256) and spread
    /// (256..512).
    pub fn is_coordinate_allowed(&self, hex_pos: Int2, height: i32) -> bool {
        use crate::os::extra_math::{contains, encapsulate, inverse_lerp_unclamped};
        let h = hex::get_hex3(hex_pos);
        if !contains(self.bounds, h) || self.bounds_height < height + 1 {
            let top = mathf::max_i32(self.bounds_height, height + 1);
            let a = mathf::max(0.0, inverse_lerp_unclamped(64.0, 256.0, top as f32));
            if 1.0 < a {
                return false;
            }
            let e = encapsulate(self.bounds, h);
            let m = mathf::max_i32(mathf::max_i32(e.c1.x.wrapping_sub(e.c0.x), e.c1.y.wrapping_sub(e.c0.y)), e.c1.z.wrapping_sub(e.c0.z));
            let b = mathf::max(0.0, inverse_lerp_unclamped(256.0, 512.0, m as f32 * 0.5));
            if 1.0 < b || 1.0 < b + a {
                return false;
            }
        }
        if !contains(self.view_bounds, h) {
            return false;
        }
        height < 256
    }

    /// The index of a qube among its square's qubes (its sibling index).
    fn qube_index(&self, qube: QubeId) -> (Option<SquareId>, i32) {
        match self.qube(qube).square {
            Some(s) => (Some(s), self.square(s).qubes.iter().position(|&q| q == qube).map_or(-1, |i| i as i32)),
            None => (None, self.qube_pool.iter().position(|&q| q == qube).map_or(-1, |i| i as i32)),
        }
    }

    /// The qube across side `side` (0..=3) of a qube at `index` in square `s`.
    fn side_relation(&self, s: SquareId, index: i32, side: i8) -> QubeRelation {
        let r = self.square(s).get_relation(side);
        if r.index_in_other == -1 {
            return QubeRelation::EMPTY;
        }
        let other = r.square.expect("Graph.GetQubeRelation: NullReferenceException");
        match self.square(other).qubes.get(index as usize) {
            Some(&q) if index >= 0 => QubeRelation::new(Some(q), r.index_in_other),
            _ => QubeRelation::EMPTY,
        }
    }

    /// `Placemaker.Graphs.Graph.GetQubeRelation` 0xE841B0: the qube next to `qube` on side `index` (0..=3 across
    /// its square's edges at the same height, 4 below, 5 above), or `Qube.Relation.empty`.
    pub fn get_qube_relation(&self, qube: QubeId, index: i8) -> QubeRelation {
        let (s, i) = self.qube_index(qube);
        let s = s.expect("Graph.GetQubeRelation: a pooled qube");
        let column = &self.square(s).qubes;
        match index {
            4 => {
                if i < 1 { QubeRelation::EMPTY } else { QubeRelation::new(Some(column[(i - 1) as usize]), 5) }
            }
            5 => {
                if column.len() as i32 - 1 <= i { QubeRelation::EMPTY } else { QubeRelation::new(Some(column[(i + 1) as usize]), 4) }
            }
            _ => self.side_relation(s, i, index),
        }
    }

    /// `Placemaker.Graphs.Graph.GetQubeRelation6` 0xE83D90: the relations of the sides in `mask` (bits 0..=5),
    /// the others `empty`.
    pub fn get_qube_relation6(&self, qube: QubeId, mask: u8) -> QubeRelation6 {
        let (s, i) = self.qube_index(qube);
        let mut out = QubeRelation6::default();
        if mask & 0xF == 0 {
            for k in 0..4 {
                out.r[k] = QubeRelation::EMPTY;
            }
        } else {
            let s = s.expect("Graph.GetQubeRelation6: a pooled qube");
            for k in 0..4i8 {
                let r = if mask & (1 << k) == 0 { QubeRelation::EMPTY } else { self.side_relation(s, i, k) };
                out.set(k as i32, r);
            }
        }
        let column: &[QubeId] = match s {
            Some(s) => &self.square(s).qubes,
            None => &self.qube_pool,
        };
        out.r[4] = if mask & 0x10 != 0 && 0 < i { QubeRelation::new(Some(column[(i - 1) as usize]), 5) } else { QubeRelation::EMPTY };
        out.r[5] = if mask & 0x20 != 0 && i < column.len() as i32 - 1 { QubeRelation::new(Some(column[(i + 1) as usize]), 4) } else { QubeRelation::EMPTY };
        out
    }

    /// `Placemaker.Graphs.Graph.OnEnable` 0xE8AE10: rebuilds `cornerDict` from `corners`, `squareDict` (keyed by
    /// the sum of a square's corner positions) and `edgeDict` (each open edge, by its two corners) from
    /// `squares`.
    pub fn on_enable(&mut self) {
        self.edge_dict.clear();
        self.square_dict.clear();
        self.corner_dict.clear();
        for &c in &self.corners {
            let h = self.corner(c).hex_pos;
            assert!(self.corner_dict.insert(h, c).is_none(), "OnEnable: Dictionary.Add of an existing corner");
        }
        for s in self.squares.clone() {
            let sq = self.square(s).clone();
            for i in 0..4i8 {
                if sq.get_relation(i).square.is_none() {
                    let a = self.corner(sq.get_corner(i).expect("OnEnable: square corner")).hex_pos;
                    let b = self.corner(sq.get_corner((i + 1) % 4).expect("OnEnable: square corner")).hex_pos;
                    assert!(self.edge_dict.insert((a, b), SquareRelation::new(Some(s), i)).is_none(), "OnEnable: Dictionary.Add of an existing edge");
                }
            }
            let mut sum = Int2::default();
            for i in 0..4i8 {
                let h = self.corner(sq.get_corner(i).expect("OnEnable: square corner")).hex_pos;
                sum = Int2 { x: sum.x.wrapping_add(h.x), y: sum.y.wrapping_add(h.y) };
            }
            assert!(self.square_dict.insert(sum, s).is_none(), "OnEnable: Dictionary.Add of an existing square");
        }
    }
}
