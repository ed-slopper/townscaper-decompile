//! `Placemaker.ModuleBuilder` (row E7): turns the collapsed qubes' modules into mesh data. Row E7a: the geometry
//! that bends a module's vertices into a square (its four corner matrices, blended bilinearly); row E7c: the
//! module containers under the qubes, `ApplyModule` / `ApplyDecor` writing the modules into the big meshes
//! (`big_mesh`), their taking down, and the grass queue. Checked against the running game by
//! `tests/placemaker_module_builder.rs` and `tests/placemaker_wfc.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;

use crate::placemaker::big_mesh::{BigMeshLog, BigMeshMaster, BigMeshPart};
use crate::placemaker::graphs::{Graph, OrientedModuleSides, QubeId, Square};
use crate::placemaker::modules::{AssetRef, ModuleLibrary, ModuleMath, Orientation, OrientedModule};
use crate::unity::vector::{v3, v4, Vector2, Vector3, Vector4};

/// `Placemaker.ModuleBuilder.GetTs` 0x32FE30: the bilinear weights of the square's four corners for a vertex
/// at `(x, z)` in the unit square: `((1 - z)(1 - x), (1 - z)x, zx, (1 - x)z)`.
pub fn get_ts(src_vert: Vector3) -> Vector4 {
    let (x, z) = (src_vert.x, src_vert.z);
    v4((1.0 - z) * (1.0 - x), (1.0 - z) * x, z * x, (1.0 - x) * z)
}

/// The weights `ts` applied to the four corners' results: `((a·x + b·y) + c·z) + d·w` per component.
fn blend(ts: Vector4, r: [Vector3; 4]) -> Vector3 {
    let c = |f: fn(&Vector3) -> f32| ((f(&r[0]) * ts.x + f(&r[1]) * ts.y) + f(&r[2]) * ts.z) + f(&r[3]) * ts.w;
    v3(c(|v| v.x), c(|v| v.y), c(|v| v.z))
}

/// `Placemaker.ModuleBuilder.MultiplyPoint` 0x3302D0: a module vertex into the square: `(0, y, 0)` through each
/// corner matrix (`Matrix4x4.MultiplyPoint`), blended by `ts` (the vertex's x and z only enter through `ts`).
pub fn multiply_point(square: &Square, ts: Vector4, vector: Vector3) -> Vector3 {
    let p = v3(0.0, vector.y, 0.0);
    blend(ts, square.m.map(|m| m.multiply_point(p)))
}

/// `Placemaker.ModuleBuilder.MultiplyVector` 0x3304E0: a direction (normal, tangent) through each corner matrix
/// (`Matrix4x4.MultiplyVector`), blended by `ts`.
pub fn multiply_vector(square: &Square, ts: Vector4, vector: Vector3) -> Vector3 {
    blend(ts, square.m.map(|m| m.multiply_vector(vector)))
}

// ---- The module builder (row E7c) ----

/// A module container (`Placemaker.Graphs.ModuleContainer`, `..ctor` 0x330990): the object under a qube that
/// owns one module's (or one decor's) slots in the big meshes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleContainer {
    pub oriented_module: OrientedModule,
    pub has_props: bool,
    pub big_mesh_parts: Vec<BigMeshPart>,
}

/// A module container of the [`ModuleBuilder`] (its index in the builder's arena).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContainerId(pub u32);

/// What the module builder calls outside the graph, the library and the big meshes.
pub trait ModuleBuilderHost {
    /// `master.sandbanker.Reset()`.
    fn sandbanker_reset(&mut self) {}
    /// `master.isDimmed`.
    fn is_dimmed(&self) -> bool {
        false
    }
    /// `Camera.main.transform.forward`, or `None` when there is no main camera.
    fn camera_forward(&self) -> Option<Vector3> {
        None
    }
    /// `master.manySoundsManager.PlaySound(sound, position, pitch, volume)`.
    fn play_sound(&mut self, _sound: i32, _position: Vector3, _pitch: f32, _volume: f32) {}
    /// `master.propPlacer.RemovePropsFromContainer(container)` (row E10).
    fn remove_props_from_container(&mut self, _id: ContainerId, _container: &mut ModuleContainer) {}
    /// `master.propPlacer.PlacePropsInQube(qube)` (row E10).
    fn place_props_in_qube(&mut self, _graph: &mut Graph, _qube: QubeId) {}
    /// `WorldMaster.KeepGoing`.
    fn keep_going(&mut self) -> bool {
        true
    }
}

/// `Placemaker.ModuleBuilder`'s state (`..ctor` 0x330930 makes `qubesToGrass`; `OnStart` 0x3306E0 makes the two
/// inactive container transforms, `moduleContainerPool` and `moduleContainersToDeconstruct`). The containers
/// live in an arena; each qube's containers are kept in its transform's child order (the module's first, then
/// its decor).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModuleBuilder {
    pub containers: Vec<ModuleContainer>,
    /// `moduleContainerPool`'s children.
    pub container_pool: Vec<ContainerId>,
    /// `moduleContainersToDeconstruct`'s children.
    pub containers_to_deconstruct: Vec<ContainerId>,
    /// Each qube's children.
    pub qube_containers: HashMap<QubeId, Vec<ContainerId>>,
    pub qubes_to_grass: Vec<QubeId>,
    /// `materialMaster.fenceMaterial`: meshes of this material are doubled back to back.
    pub fence_material: AssetRef,
}

/// The lists a module mesh is gathered in before `BigMeshMaster.AddMesh` (`ListPool` lists of 256).
#[derive(Default)]
struct Lists {
    verts: Vec<Vector3>,
    normals: Vec<Vector3>,
    tangents: Vec<Vector4>,
    uvs: Vec<Vector2>,
    tris: Vec<i32>,
}

impl Lists {
    fn clear(&mut self) {
        self.verts.clear();
        self.normals.clear();
        self.tangents.clear();
        self.uvs.clear();
        self.tris.clear();
    }
}

/// The corner a module mesh takes its colour from: the index of `corner`'s top bit (7 when none is below 7),
/// through the orientation's `cornerTransform1`.
fn colour_corner(corner: u8, orientation: &Orientation) -> usize {
    let mut i = 0;
    while i < 7 && corner >> i != 1 {
        i += 1;
    }
    orientation.corner_transform1.v[i] as usize
}

/// `Unity.Mathf.Max(float, float)`: `a > b ? a : b`.
fn max_f(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// `Transform.SetSiblingIndex` on a child list.
fn set_sibling_index(list: &mut Vec<ContainerId>, c: ContainerId, index: i32) {
    let Some(at) = list.iter().position(|&x| x == c) else { return };
    list.remove(at);
    let i = if index < 0 { list.len() } else { (index as usize).min(list.len()) };
    list.insert(i, c);
}

impl ModuleBuilder {
    pub fn container(&self, c: ContainerId) -> &ModuleContainer {
        &self.containers[c.0 as usize]
    }

    /// The qube's containers (its transform's children).
    pub fn children(&self, qube: QubeId) -> &[ContainerId] {
        self.qube_containers.get(&qube).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Take `c` out of whatever list holds it (a qube's children, the pool, the deconstruction list).
    fn unparent(&mut self, c: ContainerId) {
        for v in self.qube_containers.values_mut() {
            v.retain(|&x| x != c);
        }
        self.container_pool.retain(|&x| x != c);
        self.containers_to_deconstruct.retain(|&x| x != c);
    }

    /// `Placemaker.ModuleBuilder.GetModuleContainer` 0x32FD80: the pool's last container, or a new one.
    pub fn get_module_container(&mut self) -> ContainerId {
        if let Some(&c) = self.container_pool.last() {
            return c;
        }
        self.containers.push(ModuleContainer::default());
        ContainerId(self.containers.len() as u32 - 1)
    }

    /// `Placemaker.ModuleBuilder.MaybeAddToGrassQueue` 0x330270.
    pub fn maybe_add_to_grass_queue(&mut self, graph: &mut Graph, qube: QubeId) {
        if !graph.qube(qube).awaiting_grass {
            self.qubes_to_grass.push(qube);
            graph.qube_mut(qube).awaiting_grass = true;
        }
    }

    /// `Placemaker.ModuleBuilder.MarkModuleContainerForDeconstruction` 0x330090: its props removed, the container
    /// moved to the deconstruction list and its parts given back to the big meshes (from the back).
    pub fn mark_module_container_for_deconstruction(&mut self, bmm: &mut BigMeshMaster, host: &mut dyn ModuleBuilderHost, c: ContainerId) {
        host.remove_props_from_container(c, &mut self.containers[c.0 as usize]);
        self.unparent(c);
        self.containers_to_deconstruct.push(c);
        for p in self.containers[c.0 as usize].big_mesh_parts.iter_mut().rev() {
            bmm.return_mesh(p);
        }
    }

    /// The module builder's half of `Placemaker.ModuleBuilder.MarkQubeForDeconstruction` 0x330190: every
    /// container under the qube (from the last) to `MarkModuleContainerForDeconstruction` (the graph does the
    /// rest, `Graph::deconstruct_qube`).
    pub fn mark_qube_containers_for_deconstruction(&mut self, bmm: &mut BigMeshMaster, host: &mut dyn ModuleBuilderHost, qube: QubeId) {
        for c in self.children(qube).to_vec().into_iter().rev() {
            self.mark_module_container_for_deconstruction(bmm, host, c);
        }
    }

    /// `Placemaker.ModuleBuilder.DeconstructModuleContainer` 0x32FB90: back to the pool with `OrientedModule.empty`
    /// (its child mesh filters, which the shipped game's modules don't make, would go back to `WorldMeshes`).
    pub fn deconstruct_module_container(&mut self, c: ContainerId) {
        self.containers[c.0 as usize].oriented_module = OrientedModule::EMPTY;
        self.unparent(c);
        self.container_pool.push(c);
    }

    /// `Placemaker.ModuleBuilder.IterateDeconstructQubes` 0x32FED0: the containers waiting for deconstruction,
    /// from the last, while `KeepGoing`. False when there were none.
    pub fn iterate_deconstruct_qubes(&mut self, host: &mut dyn ModuleBuilderHost) -> bool {
        if self.containers_to_deconstruct.is_empty() {
            return false;
        }
        let mut i = self.containers_to_deconstruct.len();
        while i >= 1 {
            i -= 1;
            if !host.keep_going() {
                return true;
            }
            let c = self.containers_to_deconstruct[i];
            self.deconstruct_module_container(c);
        }
        true
    }

    /// `Placemaker.ModuleBuilder.Iterate` 0x32FF90: the grass queue from the back while `KeepGoing`: a qube still
    /// awaiting gets its props (`PropPlacer.PlacePropsInQube`); the handled part of the queue is removed. False
    /// when it was empty.
    pub fn iterate(&mut self, graph: &mut Graph, host: &mut dyn ModuleBuilderHost) -> bool {
        let n = self.qubes_to_grass.len();
        if n < 1 {
            return false;
        }
        let mut i = n;
        while i >= 1 {
            if !host.keep_going() {
                break;
            }
            let q = self.qubes_to_grass[i - 1];
            i -= 1;
            if graph.qube(q).awaiting_grass {
                host.place_props_in_qube(graph, q);
                graph.qube_mut(q).awaiting_grass = false;
            }
        }
        self.qubes_to_grass.drain(i..n);
        true
    }

    /// The next part of a container (`<ApplyModule>g__GetNewBigMeshPart|8_0` 0x330870 /
    /// `<ApplyDecor>g__GetNewBigMeshPart|9_0` 0x3307B0): its old parts in order while there are any, then new ones
    /// added to it.
    fn new_part(&mut self, c: ContainerId, index: &mut usize, count: usize) -> &mut BigMeshPart {
        let parts = &mut self.containers[c.0 as usize].big_mesh_parts;
        if *index < count {
            *index += 1;
            &mut parts[*index - 1]
        } else {
            parts.push(BigMeshPart::default());
            parts.last_mut().unwrap()
        }
    }

    /// The vertices of one module mesh vertex `k`, as `ApplyModule` (with `world` the orientation's sbyte
    /// transform and the bottom clamp) and `ApplyDecor` (the orientation's float matrix) compute them.
    fn push_vertex(lists: &mut Lists, square: &Square, pos: Vector3, p0: Vector3, n0: Vector3, t0: Vector3, bottom: bool) {
        let mut p = v3(0.5 + p0.x, p0.y + 0.5, p0.z + 0.5);
        if bottom {
            p.y = max_f(0.0, p.y);
        }
        let ts = get_ts(p);
        let w = multiply_point(square, ts, p);
        lists.verts.push(v3(pos.x + w.x, pos.y + w.y, pos.z + w.z));
        lists.normals.push(multiply_vector(square, ts, n0).normalized());
        let t = multiply_vector(square, ts, t0).normalized();
        lists.tangents.push(v4(t.x, t.y, t.z, 1.0));
    }

    /// `Placemaker.ModuleBuilder.ApplyModule` 0x32E5F0: builds the qube's module. A module with an index (not
    /// the empty one) in a qube that isn't fully covered: unless the qube already shows it, its shown container
    /// is taken down; a bottom qube resets the sandbanker; a qube facing away from the camera plays sound 3; a
    /// container (from the pool) goes first under the qube, the qube joins the grass queue, and each of the
    /// module's meshes is turned by the module's orientation, bent into the square and written to the big meshes,
    /// meshes of one material in a row together. Anything else takes down all the qube's containers.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_module(&mut self, graph: &mut Graph, lib: &ModuleLibrary, bmm: &mut BigMeshMaster, host: &mut dyn ModuleBuilderHost, log: &mut Vec<BigMeshLog>, qube: QubeId, possible_module: OrientedModuleSides) {
        let om = possible_module.oriented_module;
        if om.module_index == 0 || graph.qube(qube).fully_covered() {
            for c in self.children(qube).to_vec().into_iter().rev() {
                self.mark_module_container_for_deconstruction(bmm, host, c);
            }
            return;
        }
        if let Some(&c0) = self.children(qube).first() {
            if self.container(c0).oriented_module.eq_cs(&om) {
                return;
            }
            self.mark_module_container_for_deconstruction(bmm, host, c0);
        }
        let bottom = graph.qube_sibling_index(qube) == 0;
        if bottom {
            host.sandbanker_reset();
        }
        let square_id = graph.qube(qube).square.expect("ModuleBuilder.ApplyModule: NullReferenceException (a pooled qube)");
        if !host.is_dimmed() {
            if let Some(f) = host.camera_forward() {
                let n = graph.qube(qube).get_normal(&graph.square(square_id).m);
                if n.y * f.y + f.x * n.x + n.z * f.z < 0.0 {
                    host.play_sound(3, graph.qube_position(qube), -0.2, 1.0);
                }
            }
        }
        let c = self.get_module_container();
        self.unparent(c);
        self.qube_containers.entry(qube).or_default().insert(0, c);
        self.maybe_add_to_grass_queue(graph, qube);
        self.containers[c.0 as usize].oriented_module = om;
        let square = graph.square(square_id).clone();
        let meshes = &lib.modules[om.module_index as usize].base.oriented_module_meshes;
        if meshes.is_empty() {
            return;
        }
        let mut lists = Lists::default();
        let (mut part_index, part_count) = (0usize, self.containers[c.0 as usize].big_mesh_parts.len());
        let pos = graph.qube_position(qube);
        let corner_values = graph.qube(qube).corner_values;
        let mut current: Option<AssetRef> = None;
        let (mut vbase, mut tbase) = (0usize, 0usize);
        let mm = ModuleMath::get();
        for mo in meshes {
            let o = mm.orientations[ModuleMath::mul_orientations(om.orientation(), mo.orientation()) as usize];
            let mesh = &lib.module_meshes[mo.module_index as usize];
            let (nv, nt) = (mesh.verts.len(), mesh.tris.len());
            let material = (mesh.material.path_id != 0).then_some(mesh.material);
            if current.is_none() || current != material {
                if let Some(m) = current {
                    let part = self.new_part(c, &mut part_index, part_count);
                    bmm.add_mesh(part, m, &lists.verts, &lists.normals, &lists.tangents, &lists.uvs, &lists.tris, log);
                }
                current = material;
                lists.clear();
                vbase = 0;
                tbase = 0;
            }
            let colour = if corner_values[0] == 17 { 15 } else { corner_values[colour_corner(mesh.corner, &o)] as u32 };
            for k in 0..nv {
                let p0 = o.multiply_point(mesh.verts[k]).to_vector3();
                let n0 = o.multiply_vector(mesh.normals[k]).to_vector3();
                let t0 = o.multiply_vector(mesh.tangents[k]).to_vector3();
                Self::push_vertex(&mut lists, &square, pos, p0, n0, t0, bottom);
                let uv = mesh.uvs[k].to_vector2();
                let u = ((colour & 3) as f32 + uv.x) * 0.25;
                let v = ((colour >> 2) as f32 + uv.y) * 0.25;
                lists.uvs.push(Vector2 { x: u, y: (mesh.outline_uvs[k].b as i32 * 2) as f32 + v });
            }
            let base = vbase as i32;
            if !o.inverted {
                for t in (0..nt).step_by(3) {
                    lists.tris.extend([base + mesh.tris[t + 2] as i32, base + mesh.tris[t + 1] as i32, base + mesh.tris[t] as i32]);
                }
            } else {
                lists.tris.extend(mesh.tris.iter().map(|&t| base + t as i32));
            }
            if Some(self.fence_material) == current {
                for k in vbase..vbase + nv {
                    let t = lists.tangents[k];
                    lists.tangents.push(v4(t.x, t.y, t.z, 0.0));
                    lists.verts.push(lists.verts[k]);
                    lists.normals.push(lists.normals[k]);
                    lists.uvs.push(lists.uvs[k]);
                }
                for t in (tbase..tbase + nt).step_by(3) {
                    let n = nv as i32;
                    lists.tris.extend([lists.tris[t + 2] + n, lists.tris[t + 1] + n, lists.tris[t] + n]);
                }
                tbase += 2 * nt;
                vbase += 2 * nv;
            } else {
                tbase += nt;
                vbase += nv;
            }
        }
        if let Some(m) = current {
            let part = self.new_part(c, &mut part_index, part_count);
            bmm.add_mesh(part, m, &lists.verts, &lists.normals, &lists.tangents, &lists.uvs, &lists.tris, log);
        }
    }

    /// `Placemaker.ModuleBuilder.ApplyDecor` 0x32D340: builds the decor of a collapsed qube (its modules after
    /// index 0). A decor whose container is already under the qube keeps it (moved to its index); the others get
    /// a new container at their index and each of their meshes written to the big meshes (turned by the float
    /// matrix of the decor's orientation, coloured by its corner, or 15 for dirt; no bottom clamp, no outline),
    /// one part per mesh. When any was built: sound 3 and the grass queue. Containers past the decor are taken
    /// down.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_decor(&mut self, graph: &mut Graph, lib: &ModuleLibrary, bmm: &mut BigMeshMaster, host: &mut dyn ModuleBuilderHost, log: &mut Vec<BigMeshLog>, qube: QubeId) {
        let n = graph.qube(qube).possible_modules.len();
        let mut last = self.children(qube).len() as i32 - 1;
        let mut built = false;
        if n > 1 {
            let square_id = graph.qube(qube).square.expect("ModuleBuilder.ApplyDecor: NullReferenceException (a pooled qube)");
            let square = graph.square(square_id).clone();
            let corner_values = graph.qube(qube).corner_values;
            let mm = ModuleMath::get();
            let mut lists = Lists::default();
            for i in 1..n {
                let om = graph.qube(qube).possible_modules[i].oriented_module;
                let dm = &lib.decor_modules[om.module_index as usize];
                let mut found = false;
                let mut j = 0i32;
                while j < last {
                    let kids = self.qube_containers.entry(qube).or_default();
                    let child = *kids.get((j + i as i32) as usize).unwrap_or_else(|| panic!("ModuleBuilder.ApplyDecor: child {} out of range", j + i as i32));
                    if self.containers[child.0 as usize].oriented_module.eq_cs(&om) {
                        set_sibling_index(self.qube_containers.get_mut(&qube).unwrap(), child, i as i32);
                        last -= 1;
                        found = true;
                        break;
                    }
                    j += 1;
                }
                if found {
                    continue;
                }
                built = true;
                let c = self.get_module_container();
                self.unparent(c);
                let kids = self.qube_containers.entry(qube).or_default();
                kids.push(c);
                set_sibling_index(kids, c, i as i32);
                self.containers[c.0 as usize].oriented_module = om;
                let (mut part_index, part_count) = (0usize, self.containers[c.0 as usize].big_mesh_parts.len());
                let pos = graph.qube_position(qube);
                for mo in &dm.base.oriented_module_meshes {
                    let o = mm.orientations[ModuleMath::mul_orientations(om.orientation(), mo.orientation()) as usize];
                    let mesh = &lib.module_meshes[mo.module_index as usize];
                    let m = o.matrix();
                    let ct = colour_corner(mesh.corner, &o);
                    let colour = if mesh.material_type as u8 == 2 { 15 } else { corner_values[ct] as u32 };
                    lists.clear();
                    for k in 0..mesh.verts.len() {
                        let p0 = m.multiply_point(mesh.verts[k].to_vector3());
                        let n0 = m.multiply_vector(mesh.normals[k].to_vector3());
                        let t0 = m.multiply_vector(mesh.tangents[k].to_vector3());
                        Self::push_vertex(&mut lists, &square, pos, p0, n0, t0, false);
                        let uv = mesh.uvs[k].to_vector2();
                        lists.uvs.push(Vector2 { x: ((colour & 3) as f32 + uv.x) * 0.25, y: ((colour >> 2) as f32 + uv.y) * 0.25 });
                    }
                    let nt = mesh.tris.len();
                    if !o.inverted {
                        for t in (0..nt).step_by(3) {
                            lists.tris.extend([mesh.tris[t + 2] as i32, mesh.tris[t + 1] as i32, mesh.tris[t] as i32]);
                        }
                    } else {
                        lists.tris.extend(mesh.tris.iter().map(|&t| t as i32));
                    }
                    if mesh.material == self.fence_material {
                        let nv = lists.verts.len();
                        lists.verts.extend_from_within(..);
                        lists.normals.extend_from_within(..);
                        lists.uvs.extend_from_within(..);
                        for k in 0..nv {
                            let t = lists.tangents[k];
                            lists.tangents.push(v4(t.x, t.y, t.z, 0.0));
                        }
                        for t in (0..nt).step_by(3) {
                            let n = nv as i32;
                            lists.tris.extend([lists.tris[t + 2] + n, lists.tris[t + 1] + n, lists.tris[t] + n]);
                        }
                    }
                    let part = self.new_part(c, &mut part_index, part_count);
                    bmm.add_mesh(part, mesh.material, &lists.verts, &lists.normals, &lists.tangents, &lists.uvs, &lists.tris, log);
                }
            }
            if built {
                host.play_sound(3, graph.qube_position(qube), 0.2, 1.0);
                self.maybe_add_to_grass_queue(graph, qube);
            }
        }
        let kids = self.children(qube).to_vec();
        for idx in (n..kids.len()).rev() {
            self.mark_module_container_for_deconstruction(bmm, host, kids[idx]);
        }
    }
}
