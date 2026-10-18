//! `Placemaker.HoverData` (row G1a): what the pointer is over. `SetHover` casts the camera's ray at the voxels'
//! colliders (`LayerMaster.clickMask`, row E12's `unity::physics`), then works out the voxel hit, the grid vert
//! it stands on (`srcVert` / `srcHeight`) and where a new voxel would go (`dstVert` / `dstHeight`: above, below,
//! or beside it through the square sector the hit lies in); a ray that misses lands on the ground plane. The
//! `valid…` flags say what the tools may do there. `Iterate` grows the bucket tool's flood from the hovered voxel
//! along the collapsed modules' corner links. Checked against the game's own HoverData in
//! `tests/placemaker_hover_data.rs` (`TS_HARNESS=1`).

use crate::os::extra_math::rotate2d_90;
use crate::os::hex;
use crate::placemaker::graphs::{CornerId, Graph, VoxelId};
use crate::placemaker::grid_generator::GridGenerator;
use crate::placemaker::modules::{ModuleLibrary, ModuleMath};
use crate::placemaker::quads::Vert;
use crate::placemaker::timing::layer_master;
use crate::unity::camera::Camera;
use crate::unity::geometry::{Plane, Ray};
use crate::unity::mathematics::{f2, f3, Float2, Int2, Int3};
use crate::unity::physics::{self, Collider, ConvexHull, Pose, Shape};
use crate::unity::quaternion::Quaternion;
use crate::unity::transform::Transforms;
use crate::unity::vector::{v3, Vector2, Vector3};

/// The camera `Camera.main` is (`None` when there is none).
pub struct CameraView<'a> {
    pub camera: &'a Camera,
    pub transforms: &'a Transforms,
    /// The screen in pixels (`Screen.width`, `Screen.height`).
    pub screen: Vector2,
}

/// The voxels' colliders as the physics scene holds them: each voxel of an integrated corner (one with a
/// collider mesh) at its world position, the corner's prism as a convex hull, on the voxel layer.
pub struct VoxelColliders {
    pub colliders: Vec<Collider>,
    pub voxels: Vec<VoxelId>,
}

impl VoxelColliders {
    pub fn build(graph: &Graph) -> VoxelColliders {
        let mut out = VoxelColliders { colliders: vec![], voxels: vec![] };
        for &c in &graph.corners {
            let corner = graph.corner(c);
            let Some(mesh) = &corner.collider_mesh else { continue };
            let hull = ConvexHull::from_points(&mesh.vertices);
            for &v in &corner.voxels {
                let lp = graph.voxel(v).local_position;
                let cp = corner.local_position;
                // (the corners and their voxels are unrotated: the transforms keep `(-0, -0, -0, 1)`)
                let pose = Pose { q: Quaternion { x: -0.0, y: -0.0, z: -0.0, w: 1.0 }, p: v3(lp.x + cp.x, lp.y + cp.y, lp.z + cp.z) };
                out.colliders.push(Collider { pose, shape: Shape::Convex { hull: hull.clone(), scale: v3(1.0, 1.0, 1.0) }, layer: layer_master::VOXEL_LAYER });
                out.voxels.push(v);
            }
        }
        out
    }
}

/// The HoverData component's state.
#[derive(Clone, Debug, Default)]
pub struct HoverData {
    pub last_id: i32,
    pub ray: Ray,
    pub src_vert: Vert,
    pub dst_vert: Vert,
    pub voxel: Option<VoxelId>,
    pub src_height: i32,
    pub dst_height: i32,
    pub last_hash: i32,
    pub pointer_hit_pos: Vector3,
    pub pointer_hit_dir: Vector3,
    pub side: i8,
    pub occupied: bool,
    pub inside_borders: bool,
    pub valid: bool,
    pub valid_paint: bool,
    pub valid_add: bool,
    pub valid_remove: bool,
    pub valid_bucket: bool,
    pub gamepad_plane: Plane,
    pub flood_index: i32,
    pub flood_voxels: Vec<VoxelId>,
}

/// `default(Vert)` with `quadCount = -1`: no vert.
fn no_vert() -> Vert {
    Vert { quad_count: -1, ..Vert::default() }
}

fn plane_of(p: Vector3) -> Float2 {
    hex::world_to_plane(f3(p.x, p.y, p.z))
}

impl HoverData {
    /// `HoverData.srcHexPos` 0x3E18C0.
    pub fn src_hex_pos(&self) -> Int2 {
        self.src_vert.hex_pos
    }
    /// `HoverData.dstHexPos` 0x3E1870.
    pub fn dst_hex_pos(&self) -> Int2 {
        self.dst_vert.hex_pos
    }
    /// `HoverData.srcHexPosHeight` 0x3E1890.
    pub fn src_hex_pos_height(&self) -> Int3 {
        Int3 { x: self.src_vert.hex_pos.x, y: self.src_vert.hex_pos.y, z: self.src_height }
    }
    /// `HoverData.dstHexPosHeight` 0x3E1840.
    pub fn dst_hex_pos_height(&self) -> Int3 {
        Int3 { x: self.dst_vert.hex_pos.x, y: self.dst_vert.hex_pos.y, z: self.dst_height }
    }
    /// `HoverData.vertical` 0x3E18D0: `side == -1`.
    pub fn vertical(&self) -> bool {
        self.side == -1
    }
    /// `HoverData.GetSrcPos` 0x3DFB00: the source vert at `srcHeight + 0.5`.
    pub fn get_src_pos(&self) -> Vector3 {
        let p = hex::plane_to_world(self.src_vert.plane_pos, self.src_height as f32 + 0.5);
        v3(p.x, p.y, p.z)
    }
    /// `HoverData.GetDstPos` 0x3DFA30: the destination vert at `dstHeight + 0.5`.
    pub fn get_dst_pos(&self) -> Vector3 {
        let p = hex::plane_to_world(self.dst_vert.plane_pos, self.dst_height as f32 + 0.5);
        v3(p.x, p.y, p.z)
    }

    /// `HoverData.ClearHover` 0x3DE530: nothing valid.
    pub fn clear_hover(&mut self) {
        self.valid = false;
        self.valid_paint = false;
        self.valid_add = false;
        self.valid_remove = false;
        self.valid_bucket = false;
    }

    /// `HoverData.OnGamepadMoveAdd` 0x3E04D0: after adding, the new voxel becomes the source.
    pub fn on_gamepad_move_add(&mut self) {
        self.src_height = self.dst_height;
        self.src_vert = self.dst_vert;
        self.valid_add = false;
        self.occupied = true;
    }

    /// `HoverData.OnGamepadRemove` 0x3E0510.
    pub fn on_gamepad_remove(&mut self) {
        self.valid_remove = false;
        self.voxel = None;
    }

    /// `HoverData.IsOccupied` 0xE3DFBC0: a voxel at `hex_pos` / `height` inside the allowed coordinates.
    pub fn is_occupied(&self, graph: &Graph, hex_pos: Int2, height: i32) -> bool {
        if !graph.is_coordinate_allowed(hex_pos, height) {
            return false;
        }
        match graph.corner_dict.get(&hex_pos) {
            Some(&c) => graph.binary_tree_search_voxel(c, height) >= 0,
            None => false,
        }
    }

    /// `HoverData.TryGetVert` 0x3E1490: the grid vert closest to `plane_pos` when it closes round its corner
    /// (`GetClostestVertPosition`, then `GetVertOrIterate`), else no vert at `plane_pos`.
    fn try_get_vert(grid: &mut GridGenerator, plane_pos: Float2) -> Vert {
        let v = grid.get_clostest_vert_position(plane_pos);
        if v.valid_position() {
            let v = grid.get_vert_or_iterate(v.hex_pos, None);
            if v.full() {
                return v;
            }
        }
        Vert { plane_pos, ..no_vert() }
    }

    /// `HoverData.SetHover(position, actionId)` 0x3E0530. A new action id restarts the flood. The camera's
    /// ray through `position` is cast at the click layers (`Physics.Raycast`, 1e7); on a voxel the source is
    /// its vert and height and the destination: above it (`normal.y > 0.5`), below it (`< -0.5`) or across the
    /// side of the square sector the hit lies in (the next corner of that square), `occupied` if a voxel is
    /// there. A miss is cast at the ground plane `y = 0`: source and destination the vert there (height -1 / 0),
    /// the voxel the ground one if the corner has it. Then the flood restarts from a newly hovered voxel and the
    /// flags are set: `insideBorders` (`Graph.IsCoordinateAllowed`), paint / remove / bucket on a voxel, add on a
    /// free, valid destination within 0..=255.
    pub fn set_hover(&mut self, graph: &Graph, grid: &mut GridGenerator, colliders: &VoxelColliders, cam: Option<&CameraView>, position: Vector2, action_id: i32) {
        let Some(cam) = cam else {
            self.valid = false;
            self.valid_paint = false;
            self.valid_add = false;
            self.valid_remove = false;
            self.valid_bucket = false;
            return;
        };
        let mut prev = self.voxel;
        if action_id != self.last_id {
            prev = None;
            self.last_id = action_id;
            self.flood_voxels.clear();
            self.last_hash = -1;
            self.flood_index = 0;
        }
        let cam_matrix = cam.transforms.world_to_local_matrix(cam.camera.transform);
        self.last_hash = position.hash_code().wrapping_add(action_id.wrapping_mul(0xF67)).wrapping_add(cam_matrix.hash_code());
        self.ray = cam.camera.screen_point_to_ray(cam.transforms, cam.screen, position);
        self.occupied = false;
        self.dst_vert = no_vert();
        self.dst_height = 0;
        self.voxel = None;
        self.src_vert = no_vert();
        let hit = physics::raycast(&colliders.colliders, self.ray.origin, self.ray.direction, 1e7, layer_master::CLICK_MASK);
        if let Some(hit) = hit {
            self.voxel = Some(colliders.voxels[hit.collider]);
            self.pointer_hit_pos = hit.point;
            self.pointer_hit_dir = hit.normal;
            let voxel = colliders.voxels[hit.collider];
            let corner = graph.voxel(voxel).corner.expect("HoverData.SetHover: NullReferenceException (corner)");
            let vert = grid.get_vert_or_iterate(graph.corner(corner).hex_pos, None);
            self.src_vert = vert;
            let height = graph.voxel(voxel).height as i32;
            self.src_height = height;
            self.dst_vert = vert;
            let target: Option<(CornerId, i32)> = if 0.5 < self.pointer_hit_dir.y {
                self.side = -1;
                Some((corner, height + 1))
            } else if -0.5 <= self.pointer_hit_dir.y {
                self.dst_height = height;
                self.side_target(graph, corner)
            } else {
                self.side = -1;
                Some((corner, height - 1))
            };
            if let Some((c, h)) = target {
                self.dst_height = h;
                self.occupied = graph.binary_tree_search_voxel(c, h) >= 0;
            }
        } else {
            let ground = Plane::new(Vector3::UP, 0.0);
            let (ok, enter) = ground.raycast(&self.ray);
            if ok {
                self.src_height = -1;
                self.dst_height = 0;
                self.side = -1;
                self.pointer_hit_pos = self.ray.point(enter);
                self.pointer_hit_dir = Vector3::UP;
                let v = Self::try_get_vert(grid, plane_of(self.pointer_hit_pos));
                self.src_vert = v;
                self.dst_vert = v;
                if self.src_vert.valid_position() {
                    if let Some(&c) = graph.corner_dict.get(&self.dst_vert.hex_pos) {
                        let i = graph.binary_tree_search_voxel(c, self.dst_height);
                        if i >= 0 {
                            self.voxel = Some(graph.corner(c).voxels[i as usize]);
                            self.occupied = true;
                        }
                    }
                }
                self.flood_index = 0;
            }
        }
        if prev != self.voxel {
            if let Some(v) = self.voxel {
                if !self.flood_voxels.contains(&v) {
                    self.flood_voxels.clear();
                    self.flood_voxels.push(v);
                    self.flood_index = 0;
                }
            }
        }
        self.inside_borders = graph.is_coordinate_allowed(self.dst_vert.hex_pos, self.dst_height);
        let on_voxel = self.voxel.is_some();
        self.valid_paint = on_voxel;
        self.valid_remove = on_voxel;
        self.valid_bucket = on_voxel;
        self.valid_add = !self.occupied && self.dst_vert.valid_position() && -1 < self.dst_height && self.dst_height < 0x100 && self.inside_borders;
    }

    /// `SetHover`'s side case: the square sector round the corner that the hit lies in (the hit's offset from
    /// the corner between square `i - 1`'s and square `i`'s offsets, each turned 90°); `side` is `i - 1` and the
    /// destination the corner after this one in square `i` (its plane position the corner's).
    fn side_target(&mut self, graph: &Graph, corner: CornerId) -> Option<(CornerId, i32)> {
        let squares = &graph.corner(corner).squares;
        let n = squares.len() as i32;
        if n <= 0 {
            return None;
        }
        let cp = plane_of(graph.corner(corner).local_position);
        let hp = plane_of(self.pointer_hit_pos);
        let (dx, dy) = (hp.x - cp.x, hp.y - cp.y);
        let mut prev = plane_of(graph.square(squares[(n - 1) as usize]).local_position);
        for i in 0..n {
            let cur = plane_of(graph.square(squares[i as usize]).local_position);
            let a = rotate2d_90(f2(prev.x - cp.x, prev.y - cp.y));
            if 0.0 <= a.x * dx + a.y * dy {
                let b = rotate2d_90(f2(cur.x - cp.x, cur.y - cp.y));
                if b.x * dx + b.y * dy <= 0.0 {
                    self.side = ((i + n - 1) % n) as i8;
                    let sq = graph.square(squares[i as usize]);
                    let me = graph.corner(corner).hex_pos;
                    let hex_of = |k: usize| sq.c[k].map(|c| graph.corner(c).hex_pos);
                    let next = if hex_of(0) == Some(me) {
                        sq.c[1]
                    } else if hex_of(1) == Some(me) {
                        sq.c[2]
                    } else if hex_of(2) == Some(me) {
                        sq.c[3]
                    } else {
                        sq.c[0]
                    };
                    let next = next.expect("HoverData.SetHover: NullReferenceException (square corner)");
                    self.dst_vert.hex_pos = graph.corner(next).hex_pos;
                    self.dst_vert.plane_pos = plane_of(graph.corner(next).local_position);
                    return Some((next, self.dst_height));
                }
            }
            prev = cur;
        }
        None
    }

    /// `HoverData.JumpForwardOnAdd` 0x3E01C0: after an add, the gamepad plane through the new voxel: horizontal
    /// when the camera looks steeply (`|forward.y|` above 0.3 for vertical hovers, 0.8 otherwise; `side` -1),
    /// else facing the camera's flattened forward (`side` 0, the destination's vert refreshed); the destination
    /// becomes the source.
    pub fn jump_forward_on_add(&mut self, grid: &mut GridGenerator, cam: &CameraView) {
        let _src = self.get_src_pos();
        let dst = self.get_dst_pos();
        let fwd = cam.transforms.forward(cam.camera.transform);
        let limit = if self.side == -1 { 0.3 } else { 0.8 };
        let (plane, side) = if limit < fwd.y.abs() {
            (Plane::from_point(Vector3::UP, dst), -1)
        } else {
            self.dst_vert = grid.get_vert_or_iterate(self.dst_vert.hex_pos, None);
            (Plane::from_point(v3(fwd.x, 0.0, fwd.z), dst), 0)
        };
        self.gamepad_plane = plane;
        self.side = side;
        self.src_height = self.dst_height;
        self.src_vert = self.dst_vert;
        self.occupied = true;
        self.valid_add = false;
        self.valid = true;
    }

    /// `HoverData.Iterate` 0x3DFCA0: one step of the bucket flood. The next queued voxel's corner, each of its
    /// squares and the two qubes the voxel is a corner of (its height: the voxel on top; one up: on the bottom):
    /// the qube's collapsed module (`possibleModules[0]`) and orientation map the voxel to a module corner, and
    /// every module corner linked to it (`cornerLinks` by `GetCornerLinkIndex`) names a voxel to queue. False
    /// when the queue is done.
    pub fn iterate(&mut self, graph: &Graph, lib: &ModuleLibrary) -> bool {
        let Some(&voxel) = self.flood_voxels.get(self.flood_index as usize) else { return false };
        self.flood_index += 1;
        let Some(corner) = graph.voxel(voxel).corner else { return true };
        let mm = ModuleMath::get();
        let height = graph.voxel(voxel).height as i32;
        let squares = graph.corner(corner).squares.clone();
        for s in squares {
            let sq = graph.square(s);
            let ci = if sq.c[0] == Some(corner) {
                0
            } else if sq.c[1] == Some(corner) {
                1
            } else if sq.c[2] == Some(corner) {
                2
            } else {
                3
            };
            for j in 0..2 {
                let h = height + j;
                let qube = *sq.qubes.get(h as usize).expect("HoverData.Iterate: UnityException (GetChild)");
                let q = graph.qube(qube);
                if q.possible_modules.is_empty() {
                    continue;
                }
                let om = q.possible_modules[0].oriented_module;
                let module = &lib.modules[om.module_index as usize];
                let o = &mm.orientations[om.orientation() as usize];
                let k = o.corner_transform0.v[(ci - 4 * j + 4) as usize] as i32;
                for u in 0..8 {
                    if u == k {
                        continue;
                    }
                    if module.corner_links & (1 << (ModuleMath::get_corner_link_index(k, u) & 31)) == 0 {
                        continue;
                    }
                    let t = o.corner_transform1.v[u as usize];
                    let c2 = sq.get_corner((t & 3) as i8).expect("HoverData.Iterate: NullReferenceException (corner)");
                    let i = graph.binary_tree_search_voxel(c2, (t >> 2) as i32 + h - 1);
                    let v2 = *graph.corner(c2).voxels.get(i as usize).expect("HoverData.Iterate: UnityException (GetChild)");
                    if !self.flood_voxels.contains(&v2) {
                        self.flood_voxels.push(v2);
                    }
                }
            }
        }
        true
    }
}
