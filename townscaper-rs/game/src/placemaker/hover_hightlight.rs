//! `Placemaker.HoverHightlight` (row G1f, the game's spelling): the outline round the hovered spot, rebuilt each
//! frame the clicker draws it. On a top or bottom hover it covers the hovered vert's side of every square round
//! the corner (the qubes' house meshes, or a flat quad on the ground); on a side hover the two squares by the hit
//! side, both layers. Each qube's meshes are placed in the world as the module builder places them, with uvs
//! from a frame spanned by three qube corners (`Matrix4x4.LookAt`), so the shader draws the outline at the
//! hovered column's faces. The shader reads `_HoverPos` (the source vert's plane position).

use crate::os::hex;
use crate::placemaker::graphs::{Graph, QubeId, SquareId};
use crate::placemaker::hover_data::HoverData;
use crate::placemaker::module_builder;
use crate::placemaker::modules::{ModuleLibrary, ModuleMath};
use crate::placemaker::quads::Quad;
use crate::unity::mathematics::{f2, Float2};
use crate::unity::quaternion::Matrix4x4;
use crate::unity::vector::{v3, v4, Vector3, Vector4};
use crate::placemaker::modules::AssetRef;

/// `HoverHightlight.hoverPosId` (`.cctor` 0x3E34F0).
pub const HOVER_POS: &str = "_HoverPos";

/// The highlight mesh (`SetVertices`, `SetNormals`, `SetUVs(0)`, `SetUVs(1)`, `SetTriangles`; bounds centred at
/// 0 with every extent `float.MaxValue / 2`, so it is never culled).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HighlightMesh {
    pub verts: Vec<Vector3>,
    pub normals: Vec<Vector3>,
    pub uvs: Vec<Float2>,
    pub uv2s: Vec<Float2>,
    pub tris: Vec<i32>,
}

/// The `Draw` closure (`<>c__DisplayClass6_0`): the lists and the running vertex start and z offset.
#[derive(Default)]
struct Builder {
    mesh: HighlightMesh,
    z_offset: f32,
    start_vert_index: i32,
}

/// What `Draw` reads.
pub struct HighlightContext<'a> {
    pub graph: &'a Graph,
    pub lib: &'a ModuleLibrary,
    /// `master.materialMaster.houseMaterial`.
    pub house_material: AssetRef,
}

/// The component's state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HoverHightlight {
    pub last_draw_frame: i32,
    pub last_update_frame: i32,
    pub active: bool,
}

/// `ModuleMath.corners[i]` as a point in the qube (`(float3)corner + 0.5`).
fn corner_point(i: i32) -> Vector3 {
    let c = ModuleMath::get().corners[i as usize].to_vector3();
    v3(1.0 * 0.5 + c.x, c.y + 1.0 * 0.5, c.z + 1.0 * 0.5)
}

/// `<Draw>g__AddQube|6_0(qube, square, corner, oppositeCorner, upCorner)` 0x3E2770: the qube's collapsed module
/// (`possibleModules[0]`, nothing when it has no meshes); the frame `LookAt(corner, opposite, up - corner)` and
/// its inverse; each of the module's meshes with the house material, under the product of the module's and the
/// mesh's orientations: every vertex `p` (`+ 0.5`) and its tangent `n` (the game takes the tangents where it
/// means the normals), `l = inverse · p`: the uv `l.xy`, the second
/// uv `(l.z, (inverse · n).z · 2)` (where `l.xy` is 0, `zOffset = l.z`); the world vertex `qube position +
/// ModuleBuilder.MultiplyPoint(square, GetTs(p), p)` and normal `MultiplyVector`; the triangles reversed unless
/// the orientation is inverted.
fn add_qube(cx: &HighlightContext, b: &mut Builder, qube: QubeId, square: SquareId, corner: i32, opposite: i32, up: i32) {
    let q = cx.graph.qube(qube);
    let Some(first) = q.possible_modules.first() else { return };
    let om = first.oriented_module;
    let module = &cx.lib.modules[om.module_index as usize];
    if module.base.oriented_module_meshes.is_empty() {
        return;
    }
    let qpos = cx.graph.qube_position(qube);
    let c = corner_point(corner);
    let o = corner_point(opposite);
    let u = corner_point(up);
    let look = Matrix4x4::look_at(c, o, u.sub(c));
    let inv = look.inverse();
    let mm = ModuleMath::get();
    let sq = cx.graph.square(square);
    for mref in &module.base.oriented_module_meshes {
        let mesh = &cx.lib.module_meshes[mref.module_index as usize];
        if mesh.material != cx.house_material {
            continue;
        }
        let o = &mm.orientations[ModuleMath::mul_orientations(om.orientation(), mref.orientation()) as usize];
        let n_verts = mesh.verts.len() as i32;
        for i in 0..mesh.verts.len() {
            let pp = o.multiply_point(mesh.verts[i]).to_vector3();
            let p = v3(1.0 * 0.5 + pp.x, pp.y + 1.0 * 0.5, pp.z + 1.0 * 0.5);
            // (the game reads the mesh's tangents here, field 0x14, not its normals)
            let n = o.multiply_vector(mesh.tangents[i]).to_vector3();
            let l = inv.multiply_point(p);
            let ln = inv.multiply_vector(n);
            let w = ln.z + ln.z;
            if l.x == 0.0 && l.y == 0.0 {
                b.z_offset = l.z;
            }
            let ts = module_builder::get_ts(p);
            let wp = module_builder::multiply_point(sq, ts, p);
            b.mesh.verts.push(v3(qpos.x + wp.x, qpos.y + wp.y, qpos.z + wp.z));
            b.mesh.normals.push(module_builder::multiply_vector(sq, ts, n));
            b.mesh.uvs.push(f2(l.x, l.y));
            b.mesh.uv2s.push(f2(l.z, w));
        }
        let s = b.start_vert_index;
        if !o.inverted {
            for k in (0..mesh.tris.len()).step_by(3) {
                b.mesh.tris.push(s + mesh.tris[k + 2] as i32);
                b.mesh.tris.push(s + mesh.tris[k + 1] as i32);
                b.mesh.tris.push(s + mesh.tris[k] as i32);
            }
        } else {
            for &t in &mesh.tris {
                b.mesh.tris.push(s + t as i32);
            }
        }
        b.start_vert_index = n_verts + b.start_vert_index;
    }
}

/// `(i + k) & 3` as C# computes `% 4` on non-negative values.
fn wrap4(i: i32) -> i32 {
    i.rem_euclid(4)
}

impl HoverHightlight {
    /// `HoverHightlight.Update` 0x3E34B0: not drawn this frame: switched off.
    pub fn update(&mut self, frame: i32) {
        if frame != self.last_draw_frame {
            self.active = false;
        }
    }

    /// `HoverHightlight.Draw` 0x3E18F0: switched on, `_HoverPos = (srcVert.planePos, 0, 0)`; the mesh cleared, and
    /// unless the source vert is not full or outside the borders, rebuilt (see the module doc). Returns the
    /// global and the mesh (`None`: left empty).
    pub fn draw(&mut self, cx: &HighlightContext, hover: &HoverData, frame: i32) -> (Vector4, Option<HighlightMesh>) {
        self.last_draw_frame = frame;
        self.active = true;
        let v = hover.src_vert;
        let p = hex::get_hex2_f(crate::unity::mathematics::f3(v.plane_pos.x, v.plane_pos.y, 0.0));
        let global = v4(p.x, p.y, 0.0, 0.0);
        if !v.full() || !hover.inside_borders {
            return (global, None);
        }
        let mut b = Builder::default();
        let g = cx.graph;
        let corner_of = |q: &Quad| (0..4).find(|&u| q.get_vert(u) == v.hex_pos);
        if hover.side != -1 {
            for k in 0..2 {
                let quad = v.get_quad((hover.side as i32 + k) % v.quad_count as i32);
                let Some(&square) = g.square_dict.get(&quad.hex_pos_sum()) else { continue };
                let Some(u) = corner_of(&quad) else { continue };
                let w = wrap4((k != 0) as i32 * 2 + 3 + u);
                let (mut c, mut up_off) = (u + 4, 0);
                for j in 0..2 {
                    let h = j + hover.src_height;
                    let qubes = &g.square(square).qubes;
                    if -1 < h && (h as usize) < qubes.len() {
                        add_qube(cx, &mut b, qubes[h as usize], square, c, (w - up_off) + 4, up_off + u);
                    }
                    up_off += 4;
                    c -= 4;
                }
            }
        } else {
            let dh = hover.dst_height - hover.src_height;
            for qi in 0..v.quad_count as i32 {
                let quad = v.get_quad(qi);
                let square = g.square_dict.get(&quad.hex_pos_sum()).copied();
                let Some(u) = corner_of(&quad) else { continue };
                let h = (0 < dh) as i32 + hover.src_height;
                if h == 0 {
                    // the ground: a flat quad from the vert round the square
                    let s = b.start_vert_index;
                    for k in 0..4 {
                        let pp = quad.get_plane_pos(wrap4(u + k));
                        let w = hex::plane_to_world(pp, 0.0);
                        b.mesh.verts.push(v3(w.x, w.y, w.z));
                    }
                    for _ in 0..4 {
                        b.mesh.normals.push(Vector3::UP);
                    }
                    b.mesh.uvs.extend([f2(0.0, 0.0), f2(1.0, 0.0), f2(1.0, 1.0), f2(0.0, 1.0)]);
                    b.mesh.uv2s.extend([f2(0.0, 1.0); 4]);
                    b.mesh.tris.extend([s, s + 1, s + 2, s + 2, s + 3, s]);
                    b.start_vert_index = s + 4;
                } else if let Some(square) = square {
                    let qubes = &g.square(square).qubes;
                    if (h as usize) < qubes.len() {
                        let a = if dh < 1 { 4 } else { 0 };
                        let bb = if dh < 1 { 0 } else { 4 };
                        add_qube(cx, &mut b, qubes[h as usize], square, a + u, bb + u, wrap4(u + 1) + a);
                    }
                }
            }
        }
        for uv in b.mesh.uv2s.iter_mut().rev() {
            *uv = f2(uv.x - b.z_offset, uv.y - 0.0);
        }
        (global, Some(b.mesh))
    }
}
