//! The big meshes (row E7b): `Placemaker.BigMeshMaster` keeps, per material, a list of big shared meshes
//! (`BigMesh`, up to 65,535 vertices each) that the modules' vertices are written into; each module's share is a
//! `BigMeshPart` (its vertex and triangle slots). Freed slots are reused before new ones are appended. Plain
//! data here: the renderer (Bevy) uploads a big mesh when `MaybeApply` would set it on its Unity `Mesh`. Checked
//! against the running game by `tests/placemaker_big_mesh.rs` (`TS_HARNESS=1`).

use crate::placemaker::modules::AssetRef;
use crate::unity::quaternion::Matrix4x4;
use crate::unity::vector::{Vector2, Vector3, Vector4};

/// `Placemaker.BigMesh` (`..ctor` 0x7B9E20: empty lists).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BigMesh {
    /// `createdCount`: vertices ever appended (slots 0..createdCount exist in `verts`).
    pub created_count: i32,
    pub dirty: bool,
    /// Freed vertex slots, reused from the back.
    pub available_indexes: Vec<u16>,
    /// Freed triangles (their first index in `tris`), reused from the back.
    pub available_tris: Vec<i32>,
    pub verts: Vec<Vector3>,
    pub normals: Vec<Vector3>,
    pub tangents: Vec<Vector4>,
    pub uvs: Vec<Vector2>,
    pub tris: Vec<i32>,
    /// The game object's active state (`MaybeApply` hides a mesh with nothing in it).
    pub active: bool,
    /// How many times `MaybeApply` set the data on the mesh (STANDIN: stands for the Unity `Mesh` upload).
    pub applied: u32,
}

impl BigMesh {
    /// `Placemaker.BigMesh.get_availableCount` 0x7B9F80: free slots plus the room left below 65,535.
    pub fn available_count(&self) -> i32 {
        self.available_indexes.len() as i32 - self.created_count + 0xFFFF
    }

    /// `Placemaker.BigMesh.get_remainingCount` 0x7B9FC0.
    pub fn remaining_count(&self) -> i32 {
        0xFFFF - self.created_count
    }

    /// `Placemaker.BigMesh.Clear` 0x7B9D10: every list emptied, `createdCount = 0`.
    pub fn clear(&mut self) {
        self.verts.clear();
        self.normals.clear();
        self.tangents.clear();
        self.uvs.clear();
        self.tris.clear();
        self.available_indexes.clear();
        self.available_tris.clear();
        self.created_count = 0;
    }

    /// `Placemaker.BigMesh.SetDirty` 0x7B9E10.
    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }
}

/// `Placemaker.BigMeshGroup` (`..ctor` 0x7B80A0): the big meshes of one material.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BigMeshGroup {
    pub material: AssetRef,
    pub big_meshes: Vec<BigMesh>,
    pub is_obj_exportable: bool,
}

/// Which big mesh a part lives in: group and index in the group's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BigMeshId {
    pub group: usize,
    pub mesh: usize,
}

/// `Placemaker.BigMeshPart` (`..ctor` 0x7B9C80): one module's slots in a big mesh.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BigMeshPart {
    pub big_mesh: Option<BigMeshId>,
    pub vert_indexes: Vec<u16>,
    pub tri_indexes: Vec<i32>,
}

/// `Placemaker.BigMeshList`: the five lists of a mesh to add (`Get` 0x7B8100 / `ReturnToPool` 0x7B8610 take
/// them from and give them back to `ListPool`s: plain new / dropped vectors here).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BigMeshList {
    pub verts: Vec<Vector3>,
    pub normals: Vec<Vector3>,
    pub tangents: Vec<Vector4>,
    pub uvs: Vec<Vector2>,
    pub tris: Vec<i32>,
}

impl BigMeshList {
    /// `Placemaker.BigMeshList.Populate` 0x7B8250: a mesh filter's shared mesh (its `GetVertices`, `GetNormals`,
    /// `GetTangents`, `GetUVs(0)`, `GetTriangles(0)` given as `mesh`) into world space by its transform's
    /// `localToWorldMatrix`: points by `MultiplyPoint`, normals by `MultiplyVector`, tangents' xyz by
    /// `MultiplyVector` with their w kept (`ExtraMath.SetW`). Each list is walked from the back.
    pub fn populate(&mut self, mesh: &BigMeshList, local_to_world: &Matrix4x4) {
        *self = mesh.clone();
        for v in self.verts.iter_mut().rev() {
            *v = local_to_world.multiply_point(*v);
        }
        for n in self.normals.iter_mut().rev() {
            *n = local_to_world.multiply_vector(*n);
        }
        for t in self.tangents.iter_mut().rev() {
            let d = local_to_world.multiply_vector(Vector3 { x: t.x, y: t.y, z: t.z });
            *t = Vector4 { x: d.x, y: d.y, z: d.z, w: t.w };
        }
    }
}

/// `Placemaker.BigMeshMaster` (`..ctor` 0x7B9C20; `OnStart` 0x7B99F0 gathers the groups under it, in hierarchy
/// order: given here as `bigMeshGroups`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BigMeshMaster {
    pub big_mesh_groups: Vec<BigMeshGroup>,
}

/// What `MaybeApply` logs (`Debug.Log`).
#[derive(Clone, Debug, PartialEq)]
pub enum BigMeshLog {
    /// `GetBigMesh` found no group for the material (`Debug.Log(material)`).
    NoGroup(AssetRef),
    /// A trailing empty triangle that was not in `availableTris` (the game's message and the index).
    MissingTri(i32),
}

impl BigMeshMaster {
    pub fn big_mesh(&self, id: BigMeshId) -> &BigMesh {
        &self.big_mesh_groups[id.group].big_meshes[id.mesh]
    }

    pub fn big_mesh_mut(&mut self, id: BigMeshId) -> &mut BigMesh {
        &mut self.big_mesh_groups[id.group].big_meshes[id.mesh]
    }

    /// `Placemaker.BigMeshMaster.GetBigMesh` 0x7B9100: in the first group of that material, the first big mesh
    /// with room for `vert_count` vertices (`availableCount`), else a new one (the group's template instantiated,
    /// with a fresh mesh from `WorldMeshes.GetMesh`). `None` (and a log) when no group has the material.
    pub fn get_big_mesh(&mut self, material: AssetRef, vert_count: i32, log: &mut Vec<BigMeshLog>) -> Option<BigMeshId> {
        let Some(g) = self.big_mesh_groups.iter().position(|g| g.material == material) else {
            log.push(BigMeshLog::NoGroup(material));
            return None;
        };
        let group = &mut self.big_mesh_groups[g];
        if let Some(m) = group.big_meshes.iter().position(|m| vert_count <= m.available_count()) {
            return Some(BigMeshId { group: g, mesh: m });
        }
        // the template is inactive in the prefab's child 0; the copy starts as its template does
        group.big_meshes.push(BigMesh::default());
        Some(BigMeshId { group: g, mesh: group.big_meshes.len() - 1 })
    }

    /// `Placemaker.BigMeshMaster.AddMesh(BigMeshPart, Material, BigMeshList)` 0x7B86A0.
    pub fn add_mesh_list(&mut self, part: &mut BigMeshPart, material: AssetRef, lists: &BigMeshList, log: &mut Vec<BigMeshLog>) {
        self.add_mesh(part, material, &lists.verts, &lists.normals, &lists.tangents, &lists.uvs, &lists.tris, log);
    }

    /// `Placemaker.BigMeshMaster.AddMesh` 0x7B86D0: the part's vertices go into freed slots (from the back of
    /// `availableIndexes`) and then new ones; normals, tangents and uvs only when given, the lists first padded
    /// with zeros up to `createdCount`. Its triangles likewise into freed triangles, then appended, with the
    /// big mesh's indexes. The part records its slots; the big mesh is dirty.
    #[allow(clippy::too_many_arguments)]
    pub fn add_mesh(
        &mut self,
        part: &mut BigMeshPart,
        material: AssetRef,
        verts: &[Vector3],
        normals: &[Vector3],
        tangents: &[Vector4],
        uvs: &[Vector2],
        tris: &[i32],
        log: &mut Vec<BigMeshLog>,
    ) {
        let vert_count = verts.len() as i32;
        let tri_count = tris.len() as i32 / 3;
        let id = self.get_big_mesh(material, vert_count, log);
        part.big_mesh = id;
        let Some(id) = id else { panic!("BigMeshMaster.AddMesh: NullReferenceException (no group for {material:?})") };
        let bm = self.big_mesh_mut(id);
        let avail = bm.available_indexes.len();
        let reuse = (vert_count as usize).min(avail);
        let (n_n, n_t, n_u) = (normals.len(), tangents.len(), uvs.len());
        if n_n > 0 {
            while (bm.normals.len() as i32) < bm.created_count {
                bm.normals.push(Vector3::default());
            }
        }
        if n_t > 0 {
            while (bm.tangents.len() as i32) < bm.created_count {
                bm.tangents.push(Vector4::default());
            }
        }
        if n_u > 0 {
            while (bm.uvs.len() as i32) < bm.created_count {
                bm.uvs.push(Vector2::default());
            }
        }
        let at = |v: &[Vector3], i: usize| *v.get(i).unwrap_or_else(|| panic!("ArgumentOutOfRangeException: [{i}]"));
        let mut i = 0usize;
        for k in 0..reuse {
            let idx = bm.available_indexes[avail - 1 - k] as usize;
            bm.verts[idx] = at(verts, i);
            if n_n > 0 {
                bm.normals[idx] = normals[i];
            }
            if n_t > 0 {
                bm.tangents[idx] = tangents[i];
            }
            if n_u > 0 {
                bm.uvs[idx] = uvs[i];
            }
            part.vert_indexes.push(idx as u16);
            i += 1;
        }
        bm.available_indexes.truncate(avail - reuse);
        for _ in reuse..vert_count as usize {
            let idx = bm.created_count;
            bm.created_count += 1;
            bm.verts.push(at(verts, i));
            if n_n > 0 {
                bm.normals.push(normals[i]);
            }
            if n_t > 0 {
                bm.tangents.push(tangents[i]);
            }
            if n_u > 0 {
                bm.uvs.push(uvs[i]);
            }
            part.vert_indexes.push(idx as u16);
            i += 1;
        }
        let vi = |t: usize| part.vert_indexes[tris[t] as usize] as i32;
        let at_tris = bm.available_tris.len();
        let reuse_t = (tri_count as usize).min(at_tris);
        let mut t = 0usize;
        let mut new_tris = Vec::with_capacity(tri_count as usize);
        for k in 0..reuse_t {
            let ti = bm.available_tris[at_tris - 1 - k];
            new_tris.push(ti);
            let (a, b, c) = (vi(t), vi(t + 1), vi(t + 2));
            bm.tris[ti as usize] = a;
            bm.tris[ti as usize + 1] = b;
            bm.tris[ti as usize + 2] = c;
            t += 3;
        }
        bm.available_tris.truncate(at_tris - reuse_t);
        let mut start = bm.tris.len() as i32;
        for _ in reuse_t..tri_count as usize {
            new_tris.push(start);
            let (a, b, c) = (vi(t), vi(t + 1), vi(t + 2));
            bm.tris.extend([a, b, c]);
            start += 3;
            t += 3;
        }
        part.tri_indexes.extend(new_tris);
        bm.dirty = true;
    }

    /// `Placemaker.BigMeshMaster.ReturnMesh` 0x7B9A30: the part's triangles zeroed and their slots and its
    /// vertex slots given back (the vertices keep their data); the part forgets its big mesh. Nothing when the
    /// part has none.
    pub fn return_mesh(&mut self, part: &mut BigMeshPart) {
        let Some(id) = part.big_mesh else { return };
        let bm = self.big_mesh_mut(id);
        for &ti in &part.tri_indexes {
            let ti = ti as usize;
            bm.tris[ti] = 0;
            bm.tris[ti + 1] = 0;
            bm.tris[ti + 2] = 0;
        }
        bm.available_indexes.extend_from_slice(&part.vert_indexes);
        bm.available_tris.extend_from_slice(&part.tri_indexes);
        part.vert_indexes.clear();
        part.tri_indexes.clear();
        part.big_mesh = None;
        bm.dirty = true;
    }

    /// `Placemaker.BigMeshMaster.MaybeApply` 0x7B9460: a dirty big mesh is applied: one with every created slot
    /// free is hidden and cleared; else it is shown, its normals, tangents and uvs (when it has any) padded with
    /// zeros to `createdCount`, and its trailing all-zero triangles cut off (each must be in `availableTris`,
    /// which loses it; a missing one is logged and stops the cut). Then the data goes to the mesh.
    pub fn maybe_apply(&mut self, id: BigMeshId, log: &mut Vec<BigMeshLog>) {
        let bm = self.big_mesh_mut(id);
        if !bm.dirty {
            return;
        }
        bm.dirty = false;
        if bm.created_count == bm.available_indexes.len() as i32 {
            bm.active = false;
            bm.clear();
            bm.applied += 1;
            return;
        }
        bm.active = true;
        if !bm.normals.is_empty() {
            while (bm.normals.len() as i32) < bm.created_count {
                bm.normals.push(Vector3::default());
            }
        }
        if !bm.tangents.is_empty() {
            while (bm.tangents.len() as i32) < bm.created_count {
                bm.tangents.push(Vector4::default());
            }
        }
        if !bm.uvs.is_empty() {
            while (bm.uvs.len() as i32) < bm.created_count {
                bm.uvs.push(Vector2::default());
            }
        }
        let n = bm.tris.len() as i32;
        let mut cut = 0;
        let mut i = n;
        while i > 0 {
            i -= 1;
            if bm.tris[i as usize] != 0 {
                break;
            }
            i -= 1;
            if bm.tris[i as usize] != 0 {
                break;
            }
            i -= 1;
            if bm.tris[i as usize] != 0 {
                break;
            }
            if let Some(k) = bm.available_tris.iter().position(|&t| t == i) {
                bm.available_tris.remove(k);
                cut += 1;
            } else {
                log.push(BigMeshLog::MissingTri(i));
                break;
            }
        }
        if cut > 0 {
            bm.tris.truncate((n - cut * 3) as usize);
        }
        bm.applied += 1;
    }

    /// `Placemaker.BigMeshMaster.OnLateUpdate` 0x7B9910: `MaybeApply` on every big mesh, groups and meshes from
    /// the back.
    pub fn on_late_update(&mut self, log: &mut Vec<BigMeshLog>) {
        for g in (0..self.big_mesh_groups.len()).rev() {
            for m in (0..self.big_mesh_groups[g].big_meshes.len()).rev() {
                self.maybe_apply(BigMeshId { group: g, mesh: m }, log);
            }
        }
    }
}
