//! `Placemaker.WorldMeshes` (row E7d): pools of Unity meshes and mesh filter objects (`"Mesh"` game objects with
//! a `MeshRenderer` and a `MeshFilter`), kept under the pool's own transform while unused. Plain data: meshes and
//! mesh filters are ids, their Unity side (clearing, rendering) is the renderer's. Checked against the running
//! game by `tests/placemaker_world_meshes.rs` (`TS_HARNESS=1`).

/// A pooled Unity `Mesh` (`MarkDynamic` when made).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(pub u32);

/// A pooled mesh filter object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshFilterId(pub u32);

/// Who holds a mesh filter object: the pool (`WorldMeshes`' transform) or a caller's transform (an id the caller
/// chooses).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeshFilterParent {
    Pool,
    Other(u32),
}

/// A mesh filter object's state.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshFilterObject {
    pub shared_mesh: Option<MeshId>,
    pub active: bool,
    pub parent: MeshFilterParent,
    /// Back in the pool: on `LayerMaster`'s first layer, with an identity local transform.
    pub pooled_layer: bool,
}

/// `Placemaker.WorldMeshes` (`..ctor` 0x1FAC00: an empty `meshStack`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WorldMeshes {
    pub mf_iterator: i32,
    pub mesh_iterator: i32,
    /// `meshStack`: spare meshes, taken from the back.
    pub mesh_stack: Vec<MeshId>,
    /// Every mesh filter object made; the pool is the ones whose parent is `Pool`, in `pool` order.
    pub mesh_filters: Vec<MeshFilterObject>,
    /// The pool transform's children, in sibling order.
    pub pool: Vec<MeshFilterId>,
    /// Meshes made so far.
    pub mesh_count: u32,
    /// How many times a mesh was cleared (`Mesh.Clear`, STANDIN: the renderer's job).
    pub cleared: Vec<MeshId>,
}

impl WorldMeshes {
    /// `Placemaker.WorldMeshes.GetNewMesh` 0x1FA5A0.
    fn get_new_mesh(&mut self) -> MeshId {
        self.mesh_count += 1;
        MeshId(self.mesh_count - 1)
    }

    /// `Placemaker.WorldMeshes.GetMesh` 0x1FA480: the stack's last, or a new mesh.
    pub fn get_mesh(&mut self) -> MeshId {
        match self.mesh_stack.pop() {
            Some(m) => m,
            None => self.get_new_mesh(),
        }
    }

    /// `Placemaker.WorldMeshes.GetNewMeshFilter` 0x1FA520: a new `"Mesh"` object under `parent`.
    fn get_new_mesh_filter(&mut self, parent: MeshFilterParent) -> MeshFilterId {
        self.mesh_filters.push(MeshFilterObject { shared_mesh: None, active: true, parent, pooled_layer: false });
        let id = MeshFilterId(self.mesh_filters.len() as u32 - 1);
        if parent == MeshFilterParent::Pool {
            self.pool.push(id);
        }
        id
    }

    /// Moves an object to `parent` (its new last child when that is the pool).
    fn set_parent(&mut self, mf: MeshFilterId, parent: MeshFilterParent) {
        self.pool.retain(|&x| x != mf);
        if parent == MeshFilterParent::Pool {
            self.pool.push(mf);
        }
        self.mesh_filters[mf.0 as usize].parent = parent;
    }

    /// `Placemaker.WorldMeshes.GetMeshFilterOnly` 0x1FA130: the pool's last object, activated and moved under
    /// `parent`, else a new one there.
    pub fn get_mesh_filter_only(&mut self, parent: MeshFilterParent) -> MeshFilterId {
        match self.pool.last().copied() {
            None => self.get_new_mesh_filter(parent),
            Some(mf) => {
                self.mesh_filters[mf.0 as usize].active = true;
                self.set_parent(mf, parent);
                mf
            }
        }
    }

    /// `Placemaker.WorldMeshes.GetMeshFilter` 0x1FA280: `GetMeshFilterOnly`, then a mesh from the stack (or a new
    /// one) as its shared mesh (whatever mesh it had is dropped).
    pub fn get_mesh_filter(&mut self, parent: MeshFilterParent) -> MeshFilterId {
        let mf = self.get_mesh_filter_only(parent);
        let m = self.get_mesh();
        self.mesh_filters[mf.0 as usize].shared_mesh = Some(m);
        mf
    }

    /// `Placemaker.WorldMeshes.ReturnMesh` 0x1FABA0: cleared and back on the stack.
    pub fn return_mesh(&mut self, mesh: MeshId) {
        self.cleared.push(mesh);
        self.mesh_stack.push(mesh);
    }

    /// `Placemaker.WorldMeshes.ReturnMeshFilterOnly` 0x1FA7A0: deactivated, put on `LayerMaster`'s first layer,
    /// back under the pool with an identity local transform; its mesh stays.
    pub fn return_mesh_filter_only(&mut self, mf: MeshFilterId) {
        let o = &mut self.mesh_filters[mf.0 as usize];
        o.active = false;
        o.pooled_layer = true;
        self.set_parent(mf, MeshFilterParent::Pool);
    }

    /// `Placemaker.WorldMeshes.ReturnMeshFilter` 0x1FA8E0: its mesh (if any) cleared and back on the stack, then
    /// `ReturnMeshFilterOnly`.
    pub fn return_mesh_filter(&mut self, mf: MeshFilterId) {
        if let Some(m) = self.mesh_filters[mf.0 as usize].shared_mesh.take() {
            self.cleared.push(m);
            self.mesh_stack.push(m);
        }
        self.return_mesh_filter_only(mf);
    }

    /// `Placemaker.WorldMeshes.ReturnMeshFilters` 0x1FAAE0: the mesh filters among a container's children, from
    /// the last (`None` entries are children without one).
    pub fn return_mesh_filters(&mut self, children: &[Option<MeshFilterId>]) {
        for mf in children.iter().rev().flatten() {
            self.return_mesh_filter(*mf);
        }
    }

    /// `Placemaker.WorldMeshes.IterateRefillPools` 0x1FA5F0: up to 10 rounds of `GetMeshFilter` under the pool
    /// itself (counting from the pool's size; past the first new object it hands the same last one out again,
    /// each time with another mesh), then the mesh stack topped up to 10 new meshes; `KeepGoing` checked after
    /// each. True when time ran out.
    pub fn iterate_refill_pools(&mut self, keep_going: &mut dyn FnMut() -> bool) -> bool {
        let mut i = self.pool.len();
        while i < 10 {
            self.get_mesh_filter(MeshFilterParent::Pool);
            if !keep_going() {
                return true;
            }
            i += 1;
        }
        let mut n = self.mesh_stack.len();
        while n <= 9 {
            let m = self.get_new_mesh();
            self.mesh_stack.push(m);
            if !keep_going() {
                return true;
            }
            n += 1;
        }
        false
    }
}
