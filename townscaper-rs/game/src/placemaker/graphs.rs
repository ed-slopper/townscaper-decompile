//! `Placemaker.Graphs` and its records (rows E4a–E4e): the voxel graph of a town. The game keeps it as Unity
//! objects (`Corner`, `Square`, `Qube`, `Voxel` components on pooled `GameObject`s); here they live in arenas
//! of the `Graph` and refer to each other by id. Row E4a: the records; row E4b: the `Graph` structure. Checked against the running game by
//! `tests/placemaker_graphs.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;

use crate::os::{hex, utils};
use crate::placemaker::grid_generator::{self, GridGenerator};
use crate::placemaker::quads::Quad;
use crate::unity::mathematics::{f3, Float2, Float3, Int2, Int3, Int3x2};
use crate::unity::mathf;
use crate::unity::quaternion::Matrix4x4;
use crate::unity::vector::{v4, Vector2, Vector3};

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
    /// The transform's parent when it is a corner (`None` in the voxel pool).
    pub corner: Option<CornerId>,
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

impl FlowData {
    /// `Placemaker.Graphs.FlowData.empty` (`FlowData..cctor` 0x6CF900): no voxel, `dist = newDist = float.MaxValue`.
    pub const EMPTY: FlowData = FlowData {
        voxel: None,
        neighbour_count: 0,
        dist: f32::MAX,
        new_dist: f32::MAX,
        denom: 0.0,
        dir: Vector3 { x: 0.0, y: 0.0, z: 0.0 },
        new_dir: Vector3 { x: 0.0, y: 0.0, z: 0.0 },
        target: false,
    };
}

/// A corner's `colliderMesh` (a `WorldMeshes` pool mesh, also the `sharedMesh` of its voxels' `MeshCollider`
/// and `MeshFilter`): the prism round the corner that hovering hits.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ColliderMesh {
    pub vertices: Vec<Vector3>,
    pub triangles: Vec<i32>,
}

/// `Placemaker.Graphs.Corner`: a grid corner of the town, with the squares round it and its column of voxels
/// (the voxels are its transform's children, kept in `voxels` in sibling order).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Corner {
    pub hex_pos: Int2,
    pub squares: Vec<SquareId>,
    pub collider_mesh: Option<ColliderMesh>,
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
#[derive(Clone, Debug, PartialEq)]
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

impl Default for Qube {
    /// `Placemaker.Graphs.Qube..ctor` 0x843780: `shadowIndex = -1`, `cornerValues = new ByteQube(18)` (all
    /// `Empty`), a new `possibleModules` list.
    fn default() -> Self {
        Qube {
            voxel_count: 0,
            side_variation_mask: 0,
            decor_side_variation_mask: 0,
            shadow_index: -1,
            possible_module_list_index: 0,
            existance_index: 0,
            last_fill_index: 0,
            last_propagation: 0,
            cost: 0.0,
            corner_values: [VoxelType::Empty as u8; 8],
            awaiting_rebuilding: false,
            awaiting_grass: false,
            awaiting_graph_update: false,
            awaiting_module_clearing: false,
            possible_modules: Vec::new(),
            square: None,
            local_position: Vector3 { x: 0.0, y: 0.0, z: 0.0 },
        }
    }
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

// ---- Editing (row E4c) ----

/// What the graph calls on the rest of the game through `Graph.master` (its other systems). Each method names
/// the original it stands for; the rows that port those systems implement it. The defaults do nothing.
pub trait GraphHost {
    /// `Placemaker.WorldMaster.KeepGoing` 0x1F9160: whether this frame's time budget is left.
    fn keep_going(&mut self) -> bool {
        true
    }
    /// `Placemaker.Sandbanker.Reset` 0x856210.
    fn sandbanker_reset(&mut self) {}
    /// `Placemaker.WorldMeshes.GetMesh` 0x1FA480: a pooled mesh for a corner's `colliderMesh`.
    fn get_mesh(&mut self) {}
    /// `Placemaker.WorldMeshes.ReturnMesh(Mesh)` 0x1FABA0: a corner's `colliderMesh` back to the pool.
    fn return_mesh(&mut self) {}
    /// `Placemaker.WaveFunctionCollapse.QubeRemoved` 0x3D9D10 (row E6).
    fn qube_removed(&mut self, _graph: &mut Graph, _qube: QubeId, _side_mask: u8) {}
    /// `Placemaker.AoBaker.GetShadowMesh` 0x7B3500, and what `RemoveVoxel` does with it: the four shadow
    /// vertices at the index set to zero, the shadow mesh marked dirty.
    fn clear_shadow(&mut self, _shadow_index: i32) {}
    /// The module containers half of `Placemaker.ModuleBuilder.MarkQubeForDeconstruction` 0x330190 (each child
    /// container to `MarkModuleContainerForDeconstruction`); the graph does the rest (`Graph::deconstruct_qube`).
    fn mark_module_containers_for_deconstruction(&mut self, _qube: QubeId) {}
    /// `Placemaker.ClickEffect.Click(bool, Vector2, int, VoxelType)` 0x2008A0.
    fn click(&mut self, _add: bool, _plane_pos: Float2, _height: i32, _voxel_type: VoxelType) {}
    /// The Fury SDK's `IPlatformSDK` achievement progress (`id`, progress 1).
    fn achievement(&mut self, _id: &str) {}
    /// `UnityEngine.Debug.Log`.
    fn log(&mut self, _message: &str) {}
    /// `UnityEngine.Debug.LogError`.
    fn log_error(&mut self, _message: &str) {}
}

/// `Unity.Mathematics.int2.ToString`: `int2(x, y)`.
pub fn int2_to_string(v: Int2) -> String {
    format!("int2({}, {})", v.x, v.y)
}

/// `Transform.SetSiblingIndex` on the item at the end of `list`: a negative index leaves it there, a larger
/// one is clamped to the end (as Unity does).
fn set_sibling_index_of_last<T>(list: &mut Vec<T>, index: i32) {
    if index < 0 || list.is_empty() {
        return;
    }
    let item = list.pop().unwrap();
    let i = (index as usize).min(list.len());
    list.insert(i, item);
}

fn vec3(f: Float3) -> Vector3 {
    Vector3 { x: f.x, y: f.y, z: f.z }
}

/// The `int2` difference `a - b` as a `Vector2` (`HexMath.GetHex2` here is a folded `float2` to `Vector2`).
fn sub2(a: Int2, b: Int2) -> Vector2 {
    Vector2 { x: a.x.wrapping_sub(b.x) as f32, y: a.y.wrapping_sub(b.y) as f32 }
}

impl Graph {
    /// `Placemaker.Graphs.Graph.QubeUpdated` 0xE8B6C0: queues the qube for the graph's update once.
    pub fn qube_updated(&mut self, qube: QubeId) {
        if !self.qube(qube).awaiting_graph_update {
            self.qube_update_queue.push(qube);
            self.qube_mut(qube).awaiting_graph_update = true;
        }
    }

    /// The world position of a voxel (its corner's position plus its local position).
    pub fn voxel_position(&self, voxel: VoxelId) -> Vector3 {
        let v = self.voxel(voxel);
        let c = v.corner.map(|c| self.corner(c).local_position).unwrap_or_default();
        Vector3 { x: c.x + v.local_position.x, y: c.y + v.local_position.y, z: c.z + v.local_position.z }
    }

    /// The corner slot (0..=3) of `corner` in `square`: `c0`, `c1`, `c2`, else 3.
    fn corner_slot(&self, square: SquareId, corner: CornerId) -> usize {
        let c = &self.square(square).c;
        (0..3).find(|&k| c[k] == Some(corner)).unwrap_or(3)
    }

    fn square_corners(&self, square: SquareId) -> [CornerId; 4] {
        self.square(square).c.map(|c| c.expect("Graph: NullReferenceException (a square corner)"))
    }

    /// Moves a qube from its square to the end of the qube pool (`Transform.SetParent(qubePool)`).
    fn pool_qube(&mut self, qube: QubeId) {
        if let Some(s) = self.qube(qube).square {
            let qubes = &mut self.square_mut(s).qubes;
            if let Some(i) = qubes.iter().position(|&q| q == qube) {
                qubes.remove(i);
            }
        }
        self.qube_mut(qube).square = None;
        self.qube_pool.push(qube);
    }

    /// Moves a corner from `corners` to the end of the corner pool (`Transform.SetParent(cornerPool)`).
    fn pool_corner(&mut self, corner: CornerId) {
        if let Some(i) = self.corners.iter().position(|&c| c == corner) {
            self.corners.remove(i);
        }
        self.corner_pool.push(corner);
    }

    /// The graph's half of `Placemaker.ModuleBuilder.MarkQubeForDeconstruction` 0x330190: `lastPropagation = 0`
    /// and the qube back to `master.graph.qubePool` (this graph's).
    pub fn deconstruct_qube(&mut self, host: &mut dyn GraphHost, qube: QubeId) {
        host.mark_module_containers_for_deconstruction(qube);
        self.qube_mut(qube).last_propagation = 0;
        self.pool_qube(qube);
    }

    /// `Placemaker.Graphs.Graph.PaintVoxel` 0xE8B3E0: a new type for a voxel above the ground, written into the
    /// corner values of the qubes above and below it round its corner (`corner`, else the voxel's parent).
    pub fn paint_voxel(&mut self, voxel: VoxelId, new_type: VoxelType, corner: Option<CornerId>) {
        let h = self.voxel(voxel).height as usize;
        if h == 0 {
            return;
        }
        let corner = corner.or(self.voxel(voxel).corner).expect("Graph.PaintVoxel: NullReferenceException (no corner)");
        self.voxel_mut(voxel).voxel_type = new_type;
        let t = new_type as u8;
        for i in 0..self.corner(corner).squares.len() {
            let s = self.corner(corner).squares[i];
            let (upper, lower) = (self.square(s).qubes[h + 1], self.square(s).qubes[h]);
            let k = self.corner_slot(s, corner);
            self.qube_mut(lower).corner_values[4 + k] = t;
            self.qube_mut(upper).corner_values[k] = t;
            self.qube_updated(upper);
            self.qube_updated(lower);
        }
    }

    /// `Placemaker.Graphs.Graph.ApplyAction` 0xE83110: adds (from `Empty`), paints or removes (to `Empty`) the
    /// voxel of an action, with the click effect when `play_effect` (which also integrates an added voxel's
    /// corner at once).
    pub fn apply_action(&mut self, grid: &mut GridGenerator, host: &mut dyn GraphHost, action: Action, play_effect: bool) {
        if action.in_type == VoxelType::Empty {
            let v = self.add_voxel(grid, host, action.hex_pos, action.height, action.out_type, play_effect);
            if play_effect {
                if let Some(v) = v {
                    let p = self.voxel_position(v);
                    host.click(true, hex::world_to_plane(f3(p.x, p.y, p.z)), action.height as i32, action.out_type);
                }
            }
            return;
        }
        let Some(&corner) = self.corner_dict.get(&action.hex_pos) else {
            host.log_error(&format!("Failed to get corner at {}", int2_to_string(action.hex_pos)));
            return;
        };
        let i = self.binary_tree_search_voxel(corner, action.height as i32);
        let voxel = *usize::try_from(i).ok().and_then(|i| self.corner(corner).voxels.get(i)).expect("Graph.ApplyAction: Transform child out of bounds");
        if action.out_type != VoxelType::Empty {
            self.paint_voxel(voxel, action.out_type, Some(corner));
            return;
        }
        if play_effect {
            let p = self.voxel_position(voxel);
            host.click(false, hex::world_to_plane(f3(p.x, p.y, p.z)), action.height as i32, action.in_type);
        }
        self.remove_voxel(host, voxel);
    }

    /// `Placemaker.Graphs.Graph.AddVoxel` 0xE829E0: a voxel (from the pool, else new) at `hex_pos` / `height`
    /// in its corner, sorted by height. A corner's first voxel grows the hex bounds and integrates the corner
    /// (at once when `instant_integration` and nothing waits, else through `cornerIntegrationQueue`); a later
    /// one is integrated when the corner already is. `None` (and an error) when there is a voxel there already.
    pub fn add_voxel(&mut self, grid: &mut GridGenerator, host: &mut dyn GraphHost, hex_pos: Int2, height: u8, voxel_type: VoxelType, instant_integration: bool) -> Option<VoxelId> {
        use crate::os::extra_math::{contains, encapsulate};
        let corner = self.get_or_create_corner(hex_pos);
        let i = self.binary_tree_search_voxel(corner, height as i32);
        if i >= 0 {
            host.log_error(&format!("Voxel already exists at {} {}", int2_to_string(hex_pos), height));
            return None;
        }
        let v = match self.voxel_pool.pop() {
            Some(v) => v,
            None => self.get_new_voxel(),
        };
        self.corner_mut(corner).voxels.push(v);
        // `int3(hexPos, height).GetHashCode()` (`math.hash`)
        let cost = hex_pos
            .y
            .wrapping_mul(0x4822_A3E9)
            .wrapping_add(hex_pos.x.wrapping_mul(0x4C7F_6DD1))
            .wrapping_add((height as i32).wrapping_mul(-0x553C_3DA3))
            .wrapping_sub(0x2DE2_F6BB);
        {
            let voxel = self.voxel_mut(v);
            voxel.corner = Some(corner);
            voxel.height = height;
            voxel.voxel_type = voxel_type;
            voxel.cost = cost;
        }
        set_sibling_index_of_last(&mut self.corner_mut(corner).voxels, !i);
        self.voxel_mut(v).local_position = Vector3 { x: 0.0, y: height as f32 + 0.5, z: 0.0 };
        if self.corner(corner).voxels.len() == 1 {
            let h = hex::get_hex3(hex_pos);
            if !contains(self.bounds, h) {
                self.bounds = if self.corners.len() == 1 { Int3x2 { c0: h, c1: h } } else { encapsulate(self.bounds, h) };
                self.bounds_updated = true;
            }
            if self.corner(corner).squares.is_empty() {
                let p = hex::plane_to_world(grid_generator::hex_to_plane_i2(hex_pos), 0.0);
                self.corner_mut(corner).local_position = vec3(p);
            }
            if !(instant_integration && self.corner_integration_queue.is_empty() && self.integrate_corner(grid, host, corner)) {
                self.corner_integration_queue.push(corner);
            }
        } else if self.corner(corner).collider_mesh.is_some() {
            self.integrate_voxel(host, v, corner);
        }
        let top = height as i32 + 1;
        if self.bounds_height < top {
            self.bounds_height = top;
            self.bounds_updated = true;
        }
        let n = self.total_voxel_count;
        self.total_voxel_count = n.wrapping_add(1);
        if n == 99 {
            host.log("Achevement: Blocks100");
            host.achievement("Blocks100");
        } else if n == 999 {
            host.log("Achevement: Blocks1000");
            host.achievement("Blocks1000");
        }
        Some(v)
    }

    /// `Placemaker.Graphs.Graph.IntegrateCorner` 0xE84F60: builds the squares round a corner from its grid vert
    /// (`GridGenerator.GetVertOrIterate`): each missing square from the pool (else new) with its four corner
    /// matrices, linked to its neighbours through `edgeDict`, its corners made and angled; then the corner's
    /// collider prism and blit verts, and every voxel of the corner integrated. A vert that does not close
    /// round the corner removes its voxels instead. False when the vert has no valid position.
    pub fn integrate_corner(&mut self, grid: &mut GridGenerator, host: &mut dyn GraphHost, corner: CornerId) -> bool {
        let hex_pos = self.corner(corner).hex_pos;
        let vert = {
            let mut keep_going = || host.keep_going();
            grid.get_vert_or_iterate(hex_pos, Some(&mut keep_going))
        };
        if !vert.valid_position() {
            return false;
        }
        if !vert.full() {
            let mut i = self.corner(corner).voxels.len();
            while i > 0 {
                i -= 1;
                let v = self.corner(corner).voxels[i];
                self.remove_voxel(host, v);
            }
            host.log(&format!("Removing invalid corner at {}", int2_to_string(vert.hex_pos)));
            return true;
        }
        for qi in 0..vert.quad_count as i32 {
            let quad = vert.get_quad(qi);
            let square = match self.square_dict.get(&quad.hex_pos_sum()) {
                Some(&s) => s,
                None => self.new_square(quad),
            };
            let m = &mut self.square_mut(square).motivations;
            *m = m.wrapping_add(1);
        }
        host.get_mesh();
        let n = vert.quad_count as i32;
        let nn = n * 2;
        let mut mesh = ColliderMesh::default();
        let mut j = 1;
        for i in 0..n {
            let q = vert.get_quad(i);
            let p = q.plane_pos();
            let (dx, dz) = (p.x - vert.plane_pos.x, p.y - vert.plane_pos.y);
            mesh.vertices.push(Vector3 { x: dx, y: -0.7, z: dz });
            mesh.vertices.push(Vector3 { x: dx, y: 0.3, z: dz });
            mesh.triangles.extend([(j - 1) % nn, (j + 1) % nn, j % nn, j % nn, (j + 1) % nn, (j + 2) % nn]);
            let (a, b) = if q.v0 == hex_pos {
                (q.p1, q.p2)
            } else if q.v1 == hex_pos {
                (q.p2, q.p3)
            } else if q.v2 == hex_pos {
                (q.p3, q.p0)
            } else {
                (q.p0, q.p1)
            };
            let blit = &mut self.corner_mut(corner).blit_verts;
            blit.push(Vector2 { x: a.x - vert.plane_pos.x, y: a.y - vert.plane_pos.y });
            blit.push(Vector2 { x: b.x - vert.plane_pos.x, y: b.y - vert.plane_pos.y });
            j += 2;
        }
        let mut j = 3;
        for _ in 1..n {
            mesh.triangles.extend([0, (j + 1) % nn, (j - 1) % nn, 1, j % nn, (j + 2) % nn]);
            j += 2;
        }
        self.corner_mut(corner).collider_mesh = Some(mesh);
        let mut i = 0;
        while i < self.corner(corner).voxels.len() {
            let v = self.corner(corner).voxels[i];
            self.integrate_voxel(host, v, corner);
            i += 1;
        }
        true
    }

    /// The new-square part of `IntegrateCorner` 0xE84F60: a square for `quad` at its centre (kept sorted by x in
    /// `squares`), its four corner matrices, its edges and its corners.
    fn new_square(&mut self, quad: Quad) -> SquareId {
        let pos = vec3(hex::plane_to_world(quad.plane_pos(), 0.0));
        let idx = self.binary_tree_search_square(pos);
        let s = match self.square_pool.pop() {
            Some(s) => s,
            None => self.get_new_square(),
        };
        self.squares.push(s);
        self.square_mut(s).quad_index = quad.quad_index;
        assert!(self.square_dict.insert(quad.hex_pos_sum(), s).is_none(), "IntegrateCorner: Dictionary.Add of an existing square");
        self.square_mut(s).local_position = pos;
        set_sibling_index_of_last(&mut self.squares, !idx);
        let w = [quad.p0, quad.p1, quad.p2, quad.p3].map(|p| vec3(hex::plane_to_world(p, -0.1)));
        let d = |a: Vector3, b: Vector3| v4(a.x - b.x, a.y - b.y, a.z - b.z, 0.0);
        let (e01, e03, e12, e32) = (d(w[1], w[0]), d(w[3], w[0]), d(w[2], w[1]), d(w[2], w[3]));
        let up = v4(0.0, 1.0, 0.0, 0.0);
        let at = |p: Vector3| v4(p.x, p.y, p.z, 1.0);
        // `transform.worldToLocalMatrix` (the squares container sits at the origin: a translation by -position)
        let world_to_local = Matrix4x4::translate(Vector3 { x: -pos.x, y: -pos.y, z: -pos.z });
        let base = Matrix4x4::mul(&world_to_local, &Matrix4x4::translate(Vector3 { x: 0.0, y: -0.5, z: 0.0 }));
        let m = [
            Matrix4x4::from_columns(e01, up, e03, at(w[0])),
            Matrix4x4::from_columns(e01, up, e12, at(w[1])),
            Matrix4x4::from_columns(e32, up, e12, at(w[2])),
            Matrix4x4::from_columns(e32, up, e03, at(w[3])),
        ];
        for (k, m) in m.iter().enumerate() {
            self.square_mut(s).m[k] = Matrix4x4::mul(&base, m);
        }
        let (mut a, mut b, mut c, mut d) = (quad.v1, quad.v0, quad.v2, quad.v3);
        for k in 0..4i8 {
            match self.edge_dict.get(&(a, b)).copied() {
                Some(r) => {
                    self.square_mut(s).set_relation(k, r.index_in_other, r.square);
                    let other = r.square.expect("IntegrateCorner: NullReferenceException (edge square)");
                    self.square_mut(other).set_relation(r.index_in_other, k, Some(s));
                    self.edge_dict.remove(&(a, b));
                }
                None => {
                    assert!(self.edge_dict.insert((b, a), SquareRelation::new(Some(s), k)).is_none(), "IntegrateCorner: Dictionary.Add of an existing edge");
                }
            }
            let corner = self.get_or_create_corner(b);
            self.corner_mut(corner).local_position = vec3(hex::plane_to_world(quad.get_plane_pos(k as i32), 0.0));
            self.square_mut(s).set_corner(k, Some(corner));
            let positions: Vec<Vector3> = self.square_arena.iter().map(|q| q.local_position).collect();
            self.corner_mut(corner).add_square(s, |q| positions[q.0 as usize]);
            let angle = self.corner(corner).angle;
            self.corner_mut(corner).angle = Vector2::signed_angle(sub2(d, b), sub2(a, b)) + angle;
            // `ExtraMath.Swap(ref b, ref a, ref c, ref d)`: on to the next edge
            (b, a, c, d) = (a, c, d, b);
        }
        s
    }

    /// `Placemaker.Graphs.Graph.IntegrateVoxel` 0xE86B70: a voxel's part in the squares round its corner: flow
    /// data and qubes up to its height (+1) made, its type written into the corner values of the qubes above and
    /// below it (the ground as `Ground`), the counts of voxels round the corners raised, the qubes queued.
    pub fn integrate_voxel(&mut self, host: &mut dyn GraphHost, voxel: VoxelId, corner: CornerId) {
        // (the voxel's `MeshCollider.sharedMesh` and `MeshFilter.sharedMesh` become the corner's collider mesh)
        let h = self.voxel(voxel).height as i32;
        let t = if h == 0 {
            host.sandbanker_reset();
            VoxelType::Ground as u8
        } else {
            self.voxel(voxel).voxel_type as u8
        };
        let mut si = 0;
        while si < self.corner(corner).squares.len() {
            let s = self.corner(corner).squares[si];
            let cs = self.square_corners(s);
            for &c in &cs {
                let fd = &mut self.corner_mut(c).flow_datas;
                let mut j = fd.len() as i32 - 1;
                while j <= h {
                    fd.push(FlowData::EMPTY);
                    j += 1;
                }
            }
            let mut j = self.square(s).qubes.len() as i32;
            while j <= h + 1 {
                let q = match self.qube_pool.pop() {
                    Some(q) => {
                        let e = &mut self.qube_mut(q).existance_index;
                        *e = e.wrapping_add(1);
                        q
                    }
                    None => self.get_new_qube(),
                };
                self.square_mut(s).qubes.push(q);
                {
                    let qube = self.qube_mut(q);
                    qube.square = Some(s);
                    qube.last_propagation = 0;
                    qube.voxel_count = if j != 0 { 0 } else { 0xF };
                    let v = if j != 0 { VoxelType::Empty } else { VoxelType::Water };
                    qube.corner_values[..4].fill(v as u8);
                    qube.local_position = Vector3 { x: 0.0, y: j as f32, z: 0.0 };
                }
                if j <= h - 1 {
                    self.qube_updated(q);
                }
                j += 1;
            }
            let (upper, lower) = (self.square(s).qubes[(h + 1) as usize], self.square(s).qubes[h as usize]);
            let k = self.corner_slot(s, corner);
            self.qube_mut(lower).corner_values[4 + k] = t;
            self.qube_mut(upper).corner_values[k] = t;
            if k < 3 {
                self.qube_mut(upper).voxel_count ^= 1 << k;
                self.qube_mut(lower).voxel_count ^= 0x10 << k;
            } else {
                // the fourth corner sets its bits (`or`) where the others flip theirs (`xor`)
                self.qube_mut(upper).voxel_count |= 8;
                self.qube_mut(lower).voxel_count |= 0x80;
            }
            if h == 0 {
                for &c in &cs {
                    let g = &mut self.corner_mut(c).ground_coverage;
                    *g = g.wrapping_add(1);
                }
            }
            for j in mathf::max_i32(0, h - 1)..h + 2 {
                for &c in &cs {
                    let n = &mut self.corner_mut(c).flow_datas[j as usize].neighbour_count;
                    *n = n.wrapping_add(1);
                }
            }
            self.qube_updated(upper);
            self.qube_updated(lower);
            si += 1;
        }
        self.corner_mut(corner).flow_datas[h as usize].voxel = Some(voxel);
    }

    /// `Placemaker.Graphs.Graph.RemoveVoxel` 0xE8B720: the voxel back to the pool and its part in the squares
    /// round its corner undone (corner values back to `Empty`, counts lowered, corners queued for the flow); a
    /// column left empty is taken down to its ground qube (`WaveFunctionCollapse.QubeRemoved`, its shadow
    /// cleared, the qube deconstructed). The last voxel of a corner also lets go of the squares only it kept
    /// (their edges back into `edgeDict`), of the corners only they used, and of the corner itself.
    pub fn remove_voxel(&mut self, host: &mut dyn GraphHost, voxel: VoxelId) {
        use crate::os::extra_math::on_edge;
        let Some(corner) = self.voxel(voxel).corner else {
            host.log_error("Trying to remove voxel without a corner");
            return;
        };
        let h = self.voxel(voxel).height as i32;
        if h == 0 {
            host.sandbanker_reset();
        }
        if self.corner(corner).collider_mesh.is_some() {
            self.corner_mut(corner).flow_datas[h as usize].voxel = None;
        }
        // (the voxel's `MeshCollider` and `MeshFilter` meshes cleared) and the voxel to `voxelPool`
        let voxels = &mut self.corner_mut(corner).voxels;
        if let Some(i) = voxels.iter().position(|&v| v == voxel) {
            voxels.remove(i);
        }
        self.voxel_mut(voxel).corner = None;
        self.voxel_pool.push(voxel);
        let mut si = self.corner(corner).squares.len();
        while si > 0 {
            si -= 1;
            let s = self.corner(corner).squares[si];
            let (upper, lower) = (self.square(s).qubes[(h + 1) as usize], self.square(s).qubes[h as usize]);
            let k = self.corner_slot(s, corner);
            let empty = VoxelType::Empty as u8;
            self.qube_mut(lower).corner_values[4 + k] = empty;
            self.qube_mut(upper).corner_values[k] = empty;
            self.qube_mut(upper).voxel_count ^= 1 << k;
            self.qube_mut(lower).voxel_count ^= 0x10 << k;
            let cs = self.square_corners(s);
            if h == 0 {
                for &c in &cs {
                    let g = &mut self.corner_mut(c).ground_coverage;
                    *g = g.wrapping_sub(1);
                }
            }
            for &c in &cs {
                if !self.corner(c).in_flow_update_queue {
                    self.corner_mut(c).in_flow_update_queue = true;
                    self.corner_flow_queue.push(c);
                }
            }
            for j in mathf::max_i32(0, h - 1)..h + 2 {
                for &c in &cs {
                    let n = &mut self.corner_mut(c).flow_datas[j as usize].neighbour_count;
                    *n = n.wrapping_sub(1);
                }
            }
            if self.qube(upper).voxel_count == 0 && h + 2 == self.square(s).qubes.len() as i32 {
                let q = &self.square(s).q;
                let side_mask = utils::get_byte([q[0].index_in_other != -1, q[1].index_in_other != -1, q[2].index_in_other != -1, q[3].index_in_other != -1, true, false, false, false]);
                let mut j = self.square(s).qubes.len();
                while j > 0 {
                    j -= 1;
                    let qube = self.square(s).qubes[j];
                    if self.qube(qube).voxel_count != if j == 0 { 0xF } else { 0 } {
                        break;
                    }
                    host.qube_removed(self, qube, side_mask);
                    if j == 0 && !self.qube(qube).empty() {
                        host.sandbanker_reset();
                    }
                    let shadow = self.qube(qube).shadow_index;
                    if shadow >= 0 {
                        self.shadow_mesh_indexes.push(shadow);
                        host.clear_shadow(shadow);
                        self.qube_mut(qube).shadow_index = -1;
                    }
                    self.deconstruct_qube(host, qube);
                    let e = &mut self.qube_mut(qube).existance_index;
                    *e = e.wrapping_add(1);
                }
            }
            self.qube_updated(upper);
            self.qube_updated(lower);
        }
        if self.corner(corner).voxels.is_empty() {
            let mut si = self.corner(corner).squares.len();
            while si > 0 {
                si -= 1;
                let s = self.corner(corner).squares[si];
                let m = &mut self.square_mut(s).motivations;
                *m = m.wrapping_sub(1);
                if *m == 0 {
                    self.release_square(s, corner);
                }
            }
            if self.corner(corner).collider_mesh.is_some() {
                host.return_mesh();
                self.corner_mut(corner).collider_mesh = None;
                self.corner_mut(corner).blit_verts.clear();
            }
            if !self.recalculate_hex_bounds && on_edge(self.bounds, hex::get_hex3(self.corner(corner).hex_pos)) {
                self.recalculate_hex_bounds = true;
            }
            if self.corner(corner).squares.is_empty() {
                let hp = self.corner(corner).hex_pos;
                self.corner_dict.remove(&hp);
                self.corner_mut(corner).hex_pos = Int2 { x: -1, y: -1 };
                self.pool_corner(corner);
            }
        }
        self.total_voxel_count = self.total_voxel_count.wrapping_sub(1);
        if h + 1 == self.bounds_height {
            self.recalculate_height_bounds = true;
        }
    }

    /// The square part of `RemoveVoxel` 0xE8B720: a square no voxel keeps any more, unlinked from its neighbours
    /// (their edges back into `edgeDict`, its own open edges out of it) and from its corners (a corner left with
    /// no squares, other than `removing`, to the pool), then itself to the pool.
    fn release_square(&mut self, s: SquareId, removing: CornerId) {
        let [mut a, mut b, mut c, mut d] = self.square_corners(s);
        let mut sum = Int2::default();
        for k in 0..4i8 {
            let r = self.square(s).get_relation(k);
            let (ah, bh) = (self.corner(a).hex_pos, self.corner(b).hex_pos);
            sum = Int2 { x: sum.x.wrapping_add(ah.x), y: sum.y.wrapping_add(ah.y) };
            match r.square {
                Some(other) => {
                    self.square_mut(s).set_relation(k, -1, None);
                    self.square_mut(other).set_relation(r.index_in_other, -1, None);
                    assert!(self.edge_dict.insert((bh, ah), r).is_none(), "RemoveVoxel: Dictionary.Add of an existing edge");
                }
                None => {
                    self.edge_dict.remove(&(ah, bh));
                }
            }
            let squares = &mut self.corner_mut(a).squares;
            if let Some(i) = squares.iter().position(|&q| q == s) {
                squares.remove(i);
            }
            if self.corner(a).squares.is_empty() && a != removing {
                self.corner_dict.remove(&ah);
                self.pool_corner(a);
                self.corner_mut(a).angle = 0.0;
            } else {
                let dh = self.corner(d).hex_pos;
                let angle = self.corner(a).angle;
                self.corner_mut(a).angle = angle - Vector2::signed_angle(sub2(dh, ah), sub2(bh, ah));
            }
            // `ExtraMath.Swap(ref a, ref b, ref c, ref d)`: on to the next edge
            (a, b, c, d) = (b, c, d, a);
        }
        self.square_dict.remove(&sum);
        if let Some(i) = self.squares.iter().position(|&q| q == s) {
            self.squares.remove(i);
        }
        self.square_pool.push(s);
    }
}
