//! `Placemaker.Props.PropMeshAnimator` and `PropMeshReference`'s animation (row E10c): a prop that turns on in
//! view grows in on loose mesh filter objects (taken over from a shrinking one of the same mesh nearby, or new),
//! then joins the big meshes; one that turns off hands its meshes to the animator, which shrinks them away.
//! Outside the view (or with the world dimmed) a prop goes straight into, or out of, the big meshes. The meshes'
//! contents and the big meshes themselves are the host's ([`AnimatorHost`]); the objects, their poses, meshes,
//! materials and renderers are the [`PropScene`]'s.

use crate::os::extra_math;
use crate::placemaker::prop_placer::{MaterialId, MeshRefItem, PropObject, PropScene};
use crate::placemaker::timing::layer_master;
use crate::unity::mathf;
use crate::unity::quaternion::Quaternion;
use crate::unity::transform::{Node, TransformId};
use crate::unity::vector::{v3, Vector3};

/// What the animator reads and calls outside the props.
pub trait AnimatorHost {
    /// `master.isDimmed`.
    fn is_dimmed(&self) -> bool;
    /// `master.state` (`WorldMaster.State`).
    fn state(&self) -> u8;
    /// `Camera.main.WorldToViewportPoint(p)`, `None` without a main camera.
    fn viewport_point(&mut self, p: Vector3) -> Option<Vector3>;
    /// `Time.time`.
    fn time(&self) -> f32;
    /// `WorldMaster.expTFast` / `expTFaster` (this frame's decay factors).
    fn exp_t_fast(&self) -> f32;
    fn exp_t_faster(&self) -> f32;
    /// `BigMeshList.Populate(list, item.mf)` then `master.bigMeshMaster.AddMesh(item.bigMeshPart, material, list)`.
    fn big_mesh_add(&mut self, scene: &PropScene, item: &MeshRefItem, material: Option<MaterialId>);
    /// `master.bigMeshMaster.ReturnMesh(item.bigMeshPart)`.
    fn big_mesh_return(&mut self, scene: &PropScene, item: &MeshRefItem);
    /// `Debug.LogError`.
    fn log_error(&mut self, _message: &str) {}
}

/// `WorldMeshes`' loose mesh filter objects ("mesh filter only"), as prop scene objects.
/// STANDIN: a pool of their own in the prop scene, for `WorldMeshes`' one shared pool (row E7d); the objects are
/// interchangeable, so which one comes back is not observable.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshFilterPool {
    /// `WorldMeshes`' transform: the spare objects are its children.
    pub root: TransformId,
}

impl MeshFilterPool {
    /// `Placemaker.WorldMeshes.GetMeshFilterOnly` 0x1FA130: the pool's last object, activated and moved under
    /// `parent` keeping its (identity) local pose; else a new one there (`UnityFunctions.GetNewGameObject`, a
    /// `MeshFilter` and a `MeshRenderer` added).
    pub fn get_mesh_filter_only(&self, scene: &mut PropScene, parent: TransformId) -> TransformId {
        let n = scene.transforms.child_count(self.root);
        if n < 1 {
            let o = PropObject {
                name: "New Game Object".into(),
                layer: layer_master::DEFAULT_LAYER as u32,
                from: None,
                base: scene.transforms.nodes.len(),
                src_prop: None,
                node: None,
                mesh_ref: None,
                renderer_enabled: Some(true),
                material: None,
                mesh: None,
                colliders: Vec::new(),
                window: (None, None),
                anchor: None,
                wire: (None, None),
            };
            return scene.add_object(Node { parent: Some(parent), ..Node::default() }, true, o);
        }
        let t = scene.transforms.child(self.root, n - 1);
        scene.active[t] = true;
        scene.transforms.set_parent(t, Some(parent), false);
        t
    }

    /// `Placemaker.WorldMeshes.ReturnMeshFilterOnly` 0x1FA7A0: deactivated, on the default layer, back under the
    /// pool with an identity local pose (its mesh stays).
    pub fn return_mesh_filter_only(&self, scene: &mut PropScene, t: TransformId) {
        scene.active[t] = false;
        scene.object_mut(t).layer = layer_master::DEFAULT_LAYER as u32;
        scene.transforms.set_parent(t, Some(self.root), false);
        reset_pose(scene, t);
    }
}

/// `localPosition = zero`, `localScale = one`, `localRotation = identity`.
fn reset_pose(scene: &mut PropScene, t: TransformId) {
    scene.transforms.set_local_position(t, Vector3::ZERO);
    scene.transforms.set_local_scale(t, Vector3::ONE);
    scene.transforms.set_local_rotation(t, Quaternion::IDENTITY);
}

/// `Placemaker.Props.PropMeshAnimator` (on its own object: the shrinking meshes are its children, sorted by x).
#[derive(Clone, Debug, PartialEq)]
pub struct PropMeshAnimator {
    pub root: TransformId,
    pub pool: MeshFilterPool,
}

impl PropMeshAnimator {
    /// `ExtraMath.BinarySearchTransformX` over the animator's children, as an insertion point.
    fn insertion_point(&self, scene: &PropScene, x: f32) -> i32 {
        let xs: Vec<f32> = (0..scene.transforms.child_count(self.root)).map(|i| scene.transforms.local_position(scene.transforms.child(self.root, i)).x).collect();
        let i = extra_math::binary_search_transform_x(&xs, x, 0, 0);
        if i < 0 {
            !i
        } else {
            i
        }
    }

    fn items(scene: &PropScene, r: TransformId) -> Vec<MeshRefItem> {
        scene.object(r).mesh_ref.as_ref().expect("a PropMeshReference").items.clone()
    }

    /// `Placemaker.Props.PropMeshAnimator.PosInView` 0x706B00: in front of the main camera and inside the viewport
    /// (strictly).
    pub fn pos_in_view(host: &mut dyn AnimatorHost, p: Vector3) -> bool {
        match host.viewport_point(p) {
            None => false,
            Some(v) => 0.0 < v.z && 0.0 < v.x && 0.0 < v.y && v.x < 1.0 && v.y < 1.0,
        }
    }

    /// `Placemaker.Props.PropMeshAnimator.OnTurnedOn` 0x706290 (`PropMeshReference`'s `IPropEnable.OnEnable`):
    /// in view and not dimmed, the reference animates for half a second: each item's mesh (from the last) goes onto
    /// a loose mesh filter under it — a shrinking one of the animator's near it (within 0.7 along x, 0.7 away,
    /// facing within 60°, same material and mesh), else a new one on the no-shadows layer at scale 0 — and its own
    /// renderer goes off. Otherwise each item shows: unbatched by its renderer, batched in the big meshes.
    pub fn on_turned_on(&self, scene: &mut PropScene, host: &mut dyn AnimatorHost, r: TransformId) {
        if !host.is_dimmed() && Self::pos_in_view(host, scene.transforms.position(r)) {
            let t = host.time();
            {
                let m = scene.object_mut(r).mesh_ref.as_mut().unwrap();
                m.enabled = true;
                m.time = t + 0.5;
            }
            for it in Self::items(scene, r).iter().rev() {
                let mf = it.mf;
                let mesh = scene.object(mf).mesh;
                let mat = scene.object(mf).material;
                let pos = scene.transforms.position(mf);
                let fwd = scene.transforms.forward(mf);
                let mut i = self.insertion_point(scene, pos.x + 0.7);
                let mut found: Option<TransformId> = None;
                loop {
                    i -= 1;
                    if i < 0 {
                        break;
                    }
                    let c = scene.transforms.child(self.root, i as usize);
                    if scene.transforms.local_position(c).x < pos.x - 0.7 {
                        break;
                    }
                    let cp = scene.transforms.position(c);
                    let d = v3(cp.x - pos.x, cp.y - pos.y, cp.z - pos.z);
                    if 0.48999998 < d.sqr_magnitude() {
                        continue;
                    }
                    let cf = scene.transforms.forward(c);
                    if (fwd.y * cf.y + fwd.x * cf.x) + fwd.z * cf.z < 0.5 {
                        continue;
                    }
                    if scene.object(c).material != mat || scene.object(c).mesh != mesh {
                        continue;
                    }
                    scene.transforms.set_parent(c, Some(mf), true);
                    found = Some(c);
                    break;
                }
                let f = match found {
                    Some(c) => c,
                    None => {
                        let f = self.pool.get_mesh_filter_only(scene, mf);
                        scene.object_mut(f).layer = layer_master::NO_SHADOWS_LAYER as u32;
                        scene.object_mut(f).material = mat;
                        scene.transforms.set_local_scale(f, Vector3::ZERO);
                        f
                    }
                };
                scene.object_mut(mf).renderer_enabled = Some(false);
                scene.object_mut(f).mesh = mesh;
                scene.transforms.set_as_first_sibling(f);
            }
            return;
        }
        for it in Self::items(scene, r).iter().rev() {
            if !it.should_be_batched {
                scene.object_mut(it.mf).renderer_enabled = Some(true);
            } else {
                scene.object_mut(it.mf).renderer_enabled = Some(false);
                let mat = scene.object(it.mf).material;
                host.big_mesh_add(scene, it, mat);
            }
        }
    }

    /// `Placemaker.Props.PropMeshAnimator.OnTurnedOff` 0x705CA0 (`PropMeshReference`'s `IPropEnable.OnDisable`).
    /// Past the loading states (`state > 18`) with the reference active in the hierarchy: a still one hands each
    /// item's mesh (from the last) to a loose mesh filter (no-shadows layer, its material) put among the animator's
    /// children by x, and leaves the big meshes; a growing one hands over its growing objects and stops. Otherwise a
    /// growing one drops its growing objects (one missing logs an error) and stops; a still one leaves the big
    /// meshes.
    pub fn on_turned_off(&self, scene: &mut PropScene, host: &mut dyn AnimatorHost, r: TransformId) {
        let items = Self::items(scene, r);
        let enabled = scene.object(r).mesh_ref.as_ref().unwrap().enabled;
        if host.state() > 0x12 && scene.active_in_hierarchy(r) {
            if !enabled {
                for it in items.iter().rev() {
                    let f = self.pool.get_mesh_filter_only(scene, it.mf);
                    scene.object_mut(f).layer = layer_master::NO_SHADOWS_LAYER as u32;
                    let x = scene.transforms.position(f).x;
                    let i = self.insertion_point(scene, x);
                    scene.transforms.set_parent(f, Some(self.root), true);
                    scene.transforms.set_sibling_index(f, i);
                    scene.object_mut(f).mesh = scene.object(it.mf).mesh;
                    scene.object_mut(f).material = scene.object(it.mf).material;
                    host.big_mesh_return(scene, it);
                }
                return;
            }
            for it in items.iter().rev() {
                let c = scene.transforms.child(it.mf, 0);
                let x = scene.transforms.position(c).x;
                let i = self.insertion_point(scene, x);
                scene.transforms.set_parent(c, Some(self.root), true);
                scene.transforms.set_sibling_index(c, i);
            }
            scene.object_mut(r).mesh_ref.as_mut().unwrap().enabled = false;
            return;
        }
        if enabled {
            for it in items.iter().rev() {
                if scene.transforms.child_count(it.mf) == 0 {
                    host.log_error(&format!("PropMeshReference without its growing mesh: {}", scene.object(r).name));
                }
                let c = scene.transforms.child(it.mf, 0);
                scene.object_mut(c).mesh = None;
                self.pool.return_mesh_filter_only(scene, c);
            }
            scene.object_mut(r).mesh_ref.as_mut().unwrap().enabled = false;
            return;
        }
        for it in items.iter().rev() {
            host.big_mesh_return(scene, it);
        }
    }

    /// `Placemaker.Props.PropMeshReference.Update` 0x706C90 (while enabled): before its `time`, each item's growing
    /// object (from the last) closes in on its place: position × `expTFaster`, rotation `Lerp(identity, rotation,
    /// expTFast)`, scale `(scale - 1)·clamp01(expTFast) + 1`; after it, `OnDone`.
    pub fn update_reference(&self, scene: &mut PropScene, host: &mut dyn AnimatorHost, r: TransformId) {
        let m = scene.object(r).mesh_ref.as_ref().expect("a PropMeshReference");
        if !m.enabled {
            return;
        }
        if host.time() < m.time {
            let (fast, faster) = (host.exp_t_fast(), host.exp_t_faster());
            for it in Self::items(scene, r).iter().rev() {
                let c = scene.transforms.child(it.mf, 0);
                let lp = scene.transforms.local_position(c);
                scene.transforms.set_local_position(c, v3(lp.x * faster, lp.y * faster, lp.z * faster));
                let lr = scene.transforms.local_rotation(c);
                scene.transforms.set_local_rotation(c, Quaternion::lerp(Quaternion::IDENTITY, lr, fast));
                let one = Vector3::ONE;
                let s = scene.transforms.local_scale(c);
                let t = mathf::clamp01(fast);
                scene.transforms.set_local_scale(c, v3((s.x - one.x) * t + one.x, (s.y - one.y) * t + one.y, (s.z - one.z) * t + one.z));
            }
        } else {
            self.on_done(scene, host, r);
        }
    }

    /// `Placemaker.Props.PropMeshAnimator.OnDone` 0x7056A0: the reference stops; each item (from the last) shows
    /// (unbatched by its renderer, batched in the big meshes) and its growing object, scale 1 and meshless, goes
    /// back to the pool.
    pub fn on_done(&self, scene: &mut PropScene, host: &mut dyn AnimatorHost, r: TransformId) {
        scene.object_mut(r).mesh_ref.as_mut().unwrap().enabled = false;
        for it in Self::items(scene, r).iter().rev() {
            if !it.should_be_batched {
                scene.object_mut(it.mf).renderer_enabled = Some(true);
            } else {
                let mat = scene.object(it.mf).material;
                host.big_mesh_add(scene, it, mat);
            }
            let c = scene.transforms.child(it.mf, 0);
            scene.transforms.set_local_scale(c, Vector3::ONE);
            scene.object_mut(c).mesh = None;
            self.pool.return_mesh_filter_only(scene, c);
        }
    }

    /// `Placemaker.Props.PropMeshAnimator.OnLateUpdate` 0x7058E0: each shrinking object (from the last) scales by
    /// `expTFast`; once its x scale is below 0.01 it is reset, meshless, back in the pool.
    pub fn on_late_update(&self, scene: &mut PropScene, host: &mut dyn AnimatorHost) {
        let f = host.exp_t_fast();
        let mut i = scene.transforms.child_count(self.root) as i32;
        loop {
            i -= 1;
            if i < 0 {
                return;
            }
            let c = scene.transforms.child(self.root, i as usize);
            let s = scene.transforms.local_scale(c);
            if s.x < 0.01 {
                reset_pose(scene, c);
                scene.object_mut(c).mesh = None;
                self.pool.return_mesh_filter_only(scene, c);
            } else {
                scene.transforms.set_local_scale(c, v3(s.x * f, s.y * f, s.z * f));
            }
        }
    }

    /// `Placemaker.Props.PropMeshAnimator.OnReset` 0x705B20: every shrinking object (from the last) reset,
    /// meshless, back in the pool.
    pub fn on_reset(&self, scene: &mut PropScene) {
        let mut i = scene.transforms.child_count(self.root) as i32;
        loop {
            i -= 1;
            if i < 0 {
                return;
            }
            let c = scene.transforms.child(self.root, i as usize);
            reset_pose(scene, c);
            scene.object_mut(c).mesh = None;
            self.pool.return_mesh_filter_only(scene, c);
        }
    }
}

/// `Placemaker.Props.ShiftingMesh`'s comparers 0x976190 / 0x976210: by `transform.position.x`
/// (`Single.CompareTo`). (No shipped object has a `ShiftingMesh`.)
pub fn shifting_mesh_compare(x: f32, other_x: f32) -> i32 {
    crate::placemaker::props::single_compare_to(x, other_x)
}
