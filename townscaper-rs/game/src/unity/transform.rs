//! `UnityEngine.Transform` (row E11): the hierarchy math the game reads back. The engine keeps each transform's
//! local position, rotation and scale as three SSE vectors in its hierarchy (`TransformHierarchy`, 0x30 bytes a
//! node, `w` = 0 for position and scale) and computes everything else from them with hand-vectorised native code
//! in UnityPlayer.dll. Those routines are transcribed here one instruction at a time on [`X`] registers, so the
//! results are the engine's to the bit. Checked against the running game by `tests/unity_transform.rs`
//! (`TS_HARNESS=1`).

use super::quaternion::{Matrix4x4, Quaternion};
use super::sse::X;
use super::vector::{v3, Vector3};

/// The engine's vector constants these routines load (UnityPlayer.dll `.rdata`).
mod k {
    use super::X;
    /// 0x111EA150: the squared-length floor of `SetLocalR`'s normalise (1e-30).
    pub const EPS30: X = X([0x0DA2_4260; 4]);
    /// 0x111E9CB0: the identity quaternion.
    pub const QUAT_ID: X = X([0, 0, 0, 0x3F80_0000]);
    /// 0x111E9C90 `(-2, 2, -2, 0)`, 0x111E9C60 `(-2, -2, 2, 0)`, 0x111E9CA0 `(2, -2, -2, 0)`.
    pub const C90: X = X([0xC000_0000, 0x4000_0000, 0xC000_0000, 0]);
    pub const C60: X = X([0xC000_0000, 0xC000_0000, 0x4000_0000, 0]);
    pub const CA0: X = X([0x4000_0000, 0xC000_0000, 0xC000_0000, 0]);
    /// 0x111E9CE0: the `w` lane.
    pub const W_MASK: X = X([0, 0, 0, 0xFFFF_FFFF]);
    /// 0x111E9CD0: every sign bit.
    pub const SIGNS: X = X([0x8000_0000; 4]);
    /// 0x111DB2C0: ones.
    pub const ONES: X = X([0x3F80_0000; 4]);
    /// 0x111E9C70: the `xyz` sign bits (a quaternion's conjugate).
    pub const CONJ: X = X([0x8000_0000, 0x8000_0000, 0x8000_0000, 0]);
    /// 0x111E9C40 `(-2, 2, 2, 0)`, 0x111E9C50 `(2, -2, 2, 0)`, 0x111E9C80 `(2, 2, -2, 0)`.
    pub const C40: X = X([0xC000_0000, 0x4000_0000, 0x4000_0000, 0]);
    pub const C50: X = X([0x4000_0000, 0xC000_0000, 0x4000_0000, 0]);
    pub const C80: X = X([0x4000_0000, 0x4000_0000, 0xC000_0000, 0]);
    /// 0x111E9BF0, 0x111E9C00, 0x111E9C30: the unit axes.
    pub const EX: X = X([0x3F80_0000, 0, 0, 0]);
    pub const EY: X = X([0, 0x3F80_0000, 0, 0]);
    pub const EZ: X = X([0, 0, 0x3F80_0000, 0]);
    /// 0x111EA650 (2.0000005), 0x111EA640 (2): the reciprocal's Newton steps.
    pub const TWO_UP: X = X([0x4000_0002; 4]);
    pub const TWO: X = X([0x4000_0000; 4]);
    /// 0x111EA630: below this a scale counts as zero (1e-9).
    pub const EPS9: X = X([0x3089_705F; 4]);
    /// 0x111E9CC0: the absolute-value mask.
    pub const ABS: X = X([0x7FFF_FFFF; 4]);
}

/// A rotation's matrix columns the way `CalculateGlobalMatrix` / `…Inverse` expand them (`1 ± 2(..)` from the
/// constant tables).
fn rot_columns(r: X) -> [X; 3] {
    let x3 = r.pshufd(0x71);
    let x7 = r.pshufd(0x8E);
    let x4 = r.pshufd(0xDB);
    let c0 = k::EX.add(k::C90.mul(r.pshufd(0x55)).mul(x3).add(k::C40.mul(r.pshufd(0xAA)).mul(x7)));
    let c1 = k::EY.add(k::C60.mul(r.pshufd(0xAA)).mul(x4).add(k::C50.mul(r.pshufd(0x00)).mul(x3)));
    let c2 = k::EZ.add(k::CA0.mul(r.pshufd(0x00)).mul(x7).add(k::C80.mul(r.pshufd(0x55)).mul(x4)));
    [c0, c1, c2]
}

/// The inverse scale `CalculateGlobalMatrixInverse` uses: `rcpps` with two Newton steps (`r(2.0000005 - rs)`,
/// then `r(2 - rs)`), the raw estimate where that gives NaN, 0 where `|s| < 1e-9`.
fn inv_scale(s: X) -> X {
    let x3 = s.rcp();
    let tiny = s.and(k::ABS).cmplt(k::EPS9).psrad(31);
    let x1 = k::TWO_UP.sub(x3.mul(s)).mul(x3);
    let x2 = k::TWO.sub(x1.mul(s)).mul(x1);
    let ok = x2.cmpeq(x2).psrad(31);
    let x0 = ok.and(x2).or(ok.andn(x3));
    tiny.andn(x0)
}

/// The matrix getters' last step: `w` lanes 0, 0, 0, 1 (`Transform::GetLocalToWorldMatrix` /
/// `GetWorldToLocalMatrix`).
fn to_matrix(m: [X; 4]) -> Matrix4x4 {
    let one = X::from_f([1.0; 4]).and(k::W_MASK);
    let cols = [k::W_MASK.andn(m[0]), k::W_MASK.andn(m[1]), k::W_MASK.andn(m[2]), k::W_MASK.andn(m[3]).or(one)];
    let mut out = [0f32; 16];
    for (i, c) in cols.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&c.f());
    }
    Matrix4x4 { m: out }
}

/// A transform, as the hierarchy numbers them.
pub type TransformId = usize;

/// One transform: its local position, rotation and scale as the engine stores them, and its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub t: X,
    pub r: X,
    pub s: X,
    pub parent: Option<TransformId>,
}

impl Default for Node {
    fn default() -> Self {
        Node { t: X::ZERO, r: k::QUAT_ID, s: X::from_f([1.0, 1.0, 1.0, 0.0]), parent: None }
    }
}

/// The transforms (a forest: each root a hierarchy of its own in the engine), with each one's children in
/// sibling order (`GetChild`; a transform given a new parent goes last).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Transforms {
    pub nodes: Vec<Node>,
    pub children: Vec<Vec<TransformId>>,
}

fn v3x(v: Vector3) -> X {
    X::from_f([v.x, v.y, v.z, 0.0])
}
fn xv3(x: X) -> Vector3 {
    let f = x.f();
    v3(f[0], f[1], f[2])
}
fn qx(q: Quaternion) -> X {
    X::from_f([q.x, q.y, q.z, q.w])
}
fn xq(x: X) -> Quaternion {
    let f = x.f();
    Quaternion { x: f[0], y: f[1], z: f[2], w: f[3] }
}

/// `q *= sign(sx) sign(sy) sign(sz)` per lane, as `CalculateGlobalRotation` / `InverseTransformRotation` fold a
/// mirrored parent's scale into the rotation: lane i of `q` flips where the product of the other two scale
/// signs' pattern is negative.
fn scale_sign_flip(scale: X, q: X) -> X {
    let x0 = k::SIGNS.and(scale).xor(k::ONES);
    let x4 = x0.pshufd(0x9A);
    let x3 = x0.pshufd(0x41).mul(x4);
    let x1 = X::ZERO.pshufd(0x00).and(k::W_MASK);
    let x2 = k::W_MASK.andn(x3).or(x1);
    k::SIGNS.and(x2).xor(q)
}

/// The engine's quaternion product `a * b` in the shuffle form `CalculateGlobalRotation` and
/// `InverseTransformRotation` inline (`a` the parent's rotation, `b` the running one).
fn quat_mul(a: X, b: X) -> X {
    let x4 = a.movlhps(a).mul(b.pshufd(0xF5));
    let x2 = b.mul(a.pshufd(0xAF));
    let x1 = a.mul(b.pshufd(0x88));
    let x5 = a.mul(b.pshufd(0xD2));
    let x5 = x5.sub(x1.pshufd(0x1E)).sub(x2.pshufd(0x8D)).sub(x4.pshufd(0x63));
    x5.pshufd(0xD2).xor(k::CONJ)
}

/// The engine's quaternion-times-vector (expanded with the `±2` tables) every point and direction routine
/// inlines: `CalculateGlobalPosition`, `TransformPoint`, `TransformDirection`, `InverseTransform…`.
fn rotate(r: X, v: X) -> X {
    let x0 = r.pshufd(0x71);
    let xdb = r.pshufd(0xDB);
    let x8e = r.pshufd(0x8E);
    let m20 = r.pshufd(0x55).mul(k::C90).mul(x0);
    let x3 = r.pshufd(0x00).mul(k::C90).mul(x0);
    let x5 = r.pshufd(0xAA).mul(k::C60).mul(xdb);
    let x1 = r.pshufd(0x55).mul(k::C60).mul(xdb);
    let x2 = r.pshufd(0x00).mul(k::CA0).mul(x8e);
    let x7 = r.pshufd(0xAA).mul(k::CA0).mul(x8e);
    let x5 = x5.sub(x3);
    let x2 = x2.sub(x1);
    let x1 = m20.sub(x7);
    let x2 = x2.mul(v.pshufd(0xAA));
    let x5 = x5.mul(v.pshufd(0x55));
    let x1 = x1.mul(v.pshufd(0x00));
    let x5 = x5.add(x2);
    x1.add(v).add(x5)
}

/// `TransformDirection`'s mirror step: the lanes where `s` is negative flip sign (`sign(s)` as integers,
/// converted, its sign bit).
fn mirror(s: X, v: X) -> X {
    let pos = X::ZERO.cmplt(s);
    let neg = s.cmplt(X::ZERO);
    let d = X([0, 1, 2, 3].map(|i| neg.0[i].wrapping_sub(pos.0[i])));
    let f = X::from_f(d.0.map(|v| v as i32 as f32));
    k::SIGNS.and(f).xor(v)
}

impl Transforms {
    pub fn add(&mut self, n: Node) -> TransformId {
        let id = self.nodes.len();
        if let Some(p) = n.parent {
            self.children[p].push(id);
        }
        self.nodes.push(n);
        self.children.push(Vec::new());
        id
    }

    /// `Transform.IsChildOf` (native): `id` is `parent` or below it.
    pub fn is_child_of(&self, id: TransformId, parent: TransformId) -> bool {
        let mut p = Some(id);
        while let Some(i) = p {
            if i == parent {
                return true;
            }
            p = self.nodes[i].parent;
        }
        false
    }

    /// `Transform.forward` (managed): `rotation * Vector3.forward`.
    pub fn forward(&self, id: TransformId) -> Vector3 {
        Quaternion::rotate(self.rotation(id), Vector3::FORWARD)
    }

    /// `Transform.SetSiblingIndex`: moved to `index` among its parent's children (clamped to the ends).
    pub fn set_sibling_index(&mut self, id: TransformId, index: i32) {
        let Some(p) = self.nodes[id].parent else { return };
        let list = &mut self.children[p];
        let Some(at) = list.iter().position(|&c| c == id) else { return };
        list.remove(at);
        let i = index.clamp(0, list.len() as i32) as usize;
        list.insert(i, id);
    }

    /// `Transform.SetAsFirstSibling`.
    pub fn set_as_first_sibling(&mut self, id: TransformId) {
        self.set_sibling_index(id, 0);
    }

    /// `Transform.right` (managed): `rotation * Vector3.right`.
    pub fn right(&self, id: TransformId) -> Vector3 {
        Quaternion::rotate(self.rotation(id), Vector3::RIGHT)
    }

    /// `Transform.childCount`.
    pub fn child_count(&self, id: TransformId) -> usize {
        self.children[id].len()
    }
    /// `Transform.GetChild`.
    pub fn child(&self, id: TransformId, i: usize) -> TransformId {
        self.children[id][i]
    }

    fn reparent(&mut self, id: TransformId, parent: Option<TransformId>) {
        if let Some(old) = self.nodes[id].parent {
            self.children[old].retain(|&c| c != id);
        }
        if let Some(p) = parent {
            self.children[p].push(id);
        }
        self.nodes[id].parent = parent;
    }

    /// `CalculateGlobalPosition` (UnityPlayer 0x10099250, `Transform.position`): the local position through each
    /// ancestor: scaled, rotated, translated (the same steps as [`transform_point`](Self::transform_point)).
    pub fn position(&self, id: TransformId) -> Vector3 {
        let n = &self.nodes[id];
        xv3(self.point_up(n.parent, n.t))
    }

    fn point_up(&self, mut p: Option<TransformId>, mut res: X) -> X {
        while let Some(pi) = p {
            let n = &self.nodes[pi];
            res = rotate(n.r, n.s.mul(res)).add(n.t);
            p = n.parent;
        }
        res
    }

    /// `TransformPoint` (UnityPlayer 0x10113FB0, `Transform.TransformPoint`): the point through the node and its
    /// ancestors.
    pub fn transform_point(&self, id: TransformId, p: Vector3) -> Vector3 {
        xv3(self.point_up(Some(id), v3x(p)))
    }

    /// `TransformVector` (UnityPlayer 0x1065FDF0, `Transform.TransformVector` via `Transform::TransformVector`
    /// 0x1065FF00): scaled and rotated by the node and each ancestor (no translation).
    pub fn transform_vector(&self, id: TransformId, v: Vector3) -> Vector3 {
        let mut res = v3x(v);
        let mut p = Some(id);
        while let Some(pi) = p {
            let n = &self.nodes[pi];
            res = rotate(n.r, n.s.mul(res));
            p = n.parent;
        }
        xv3(res)
    }

    /// `TransformDirection` (UnityPlayer 0x1065FAF0): rotated by the node, then for each ancestor mirrored where
    /// its scale is negative and rotated (no scaling).
    pub fn transform_direction(&self, id: TransformId, d: Vector3) -> Vector3 {
        let n = &self.nodes[id];
        let mut res = rotate(n.r, v3x(d));
        let mut p = n.parent;
        while let Some(pi) = p {
            let n = &self.nodes[pi];
            res = rotate(n.r, mirror(n.s, res));
            p = n.parent;
        }
        xv3(res)
    }

    /// `InverseTransformPosition` (UnityPlayer 0x10113590): into the node's space, the root first: `-t`,
    /// rotated back, times the inverse scale ([`inv_scale`]).
    fn inverse_transform_position(&self, id: TransformId, p: X) -> X {
        let n = &self.nodes[id];
        let p = match n.parent {
            Some(q) => self.inverse_transform_position(q, p),
            None => p,
        };
        rotate(k::CONJ.xor(n.r), p.sub(n.t)).mul(inv_scale(n.s))
    }

    /// `Transform.InverseTransformPoint` (`Transform::InverseTransformPoint` 0x1065C650).
    pub fn inverse_transform_point(&self, id: TransformId, p: Vector3) -> Vector3 {
        xv3(self.inverse_transform_position(id, v3x(p)))
    }

    /// `InverseTransformDirection` (UnityPlayer 0x1065C450): the root first, each ancestor rotating back and
    /// mirroring, the node itself only rotating back (the flag the wrapper passes).
    fn inverse_transform_direction_x(&self, id: TransformId, d: X, mirror_too: bool) -> X {
        let n = &self.nodes[id];
        let d = match n.parent {
            Some(q) => self.inverse_transform_direction_x(q, d, true),
            None => d,
        };
        let r = rotate(k::CONJ.xor(n.r), d);
        if mirror_too {
            mirror(n.s, r)
        } else {
            r
        }
    }

    /// `Transform.InverseTransformDirection` (`Transform::InverseTransformDirection` 0x1065C5C0).
    pub fn inverse_transform_direction(&self, id: TransformId, d: Vector3) -> Vector3 {
        xv3(self.inverse_transform_direction_x(id, v3x(d), false))
    }

    /// `Transform.position` setter (`Transform::SetPosition` 0x1065EF80): into the parent's space
    /// (`InverseTransformPosition`), stored.
    pub fn set_position(&mut self, id: TransformId, p: Vector3) {
        let mut v = v3x(p);
        if let Some(q) = self.nodes[id].parent {
            v = self.inverse_transform_position(q, v);
        }
        self.nodes[id].t = v;
    }

    /// `Transform.SetParent(parent, worldPositionStays)` (`Transform::SetParent` 0x1065E740, the math only: the
    /// sibling order and change messages are the host's). Keeping the world pose: the global position and
    /// rotation, and `SM` (`CalculateGlobalSM`: the global rotation's inverse times the global
    /// rotation-and-scale), are taken before the move; after it, `SetGlobalMatrixLossy` (0x1009B880) brings
    /// position and rotation into the new parent's space (`InverseTransformPositionAndRotation`, the same steps
    /// as [`inverse_transform_position`] / [`inverse_transform_rotation`]), stores them (`SetLocalTR`: the
    /// rotation normalised as by `SetLocalR`) and sets the scale to `SM`'s diagonal times the inverse
    /// ([`inv_scale`]) of the diagonal of `inverse(globalRotation) * parentRS * localRotation`.
    /// Nothing changes (true) when `parent` is already the parent; nothing changes (false) when it is the node
    /// itself or under it.
    pub fn set_parent(&mut self, id: TransformId, parent: Option<TransformId>, world_position_stays: bool) -> bool {
        if parent == self.nodes[id].parent {
            return true;
        }
        let mut a = parent;
        while let Some(n) = a {
            if n == id {
                return false;
            }
            a = self.nodes[n].parent;
        }
        if !world_position_stays {
            self.reparent(id, parent);
            return true;
        }
        let pos = self.point_up(self.nodes[id].parent, self.nodes[id].t);
        let rot = self.global_rotation_x(id);
        let [a0, a1, a2] = rot_columns(k::CONJ.xor(rot));
        let sm = {
            let m = self.global_matrix(id);
            let col = |v: X| a0.mul(v.pshufd(0x00)).add(a1.mul(v.pshufd(0x55)).add(a2.mul(v.pshufd(0xAA))));
            [col(m[0]), col(m[1]), col(m[2])]
        };
        let diag = |c: [X; 3]| X([c[0].0[0], c[1].0[1], c[2].0[2], c[2].0[3]]);
        self.reparent(id, parent);
        let (mut p, mut q) = (pos, rot);
        if let Some(pp) = parent {
            p = self.inverse_transform_position(pp, p);
            q = self.inverse_transform_rotation(pp, q);
        }
        self.nodes[id].t = p;
        self.set_local_r(id, q);
        let mut scale = diag(sm);
        if let Some(pp) = parent {
            let [b0, b1, b2] = rot_columns(self.nodes[id].r);
            let rs = self.global_matrix(pp);
            let pc = |v: X| rs[0].mul(v.pshufd(0x00)).add(rs[1].mul(v.pshufd(0x55)).add(rs[2].mul(v.pshufd(0xAA))));
            let (c0, c1, c2) = (pc(b0), pc(b1), pc(b2));
            let ac = |v: X| a0.mul(v.pshufd(0x00)).add(a1.mul(v.pshufd(0x55)).add(a2.mul(v.pshufd(0xAA))));
            let d = diag([ac(c0), ac(c1), ac(c2)]);
            scale = scale.mul(inv_scale(d));
        }
        self.nodes[id].s = scale;
        true
    }

    /// `Transform.lossyScale` (`Transform::GetWorldScaleLossy` 0x1065BC10): the diagonal of the global
    /// rotation's inverse times the global rotation-and-scale (`CalculateGlobalSM` over `CalculateGlobalRS`,
    /// which is the global matrix's first three columns, computed the same way).
    pub fn lossy_scale(&self, id: TransformId) -> Vector3 {
        let q = self.global_rotation_x(id);
        let [c0, c1, c2] = rot_columns(k::CONJ.xor(q));
        let m = self.global_matrix(id);
        let col = |v: X| c0.mul(v.pshufd(0x00)).add(c1.mul(v.pshufd(0x55)).add(c2.mul(v.pshufd(0xAA))));
        let (s0, s1, s2) = (col(m[0]), col(m[1]), col(m[2]));
        v3(s0.lane(0), s1.lane(1), s2.lane(2))
    }

    /// `CalculateGlobalRotation` (UnityPlayer 0x100995D0, `Transform.rotation`): each ancestor's rotation times
    /// the running one, signs folded for mirrored scales.
    pub fn rotation(&self, id: TransformId) -> Quaternion {
        xq(self.global_rotation_x(id))
    }

    fn global_rotation_x(&self, id: TransformId) -> X {
        let mut res = self.nodes[id].r;
        let mut p = self.nodes[id].parent;
        while let Some(pi) = p {
            let n = &self.nodes[pi];
            res = scale_sign_flip(n.s, res);
            res = quat_mul(n.r, res);
            p = n.parent;
        }
        res
    }

    /// `InverseTransformRotation` (UnityPlayer 0x1065C6E0): a world rotation into `id`'s space, the root first.
    fn inverse_transform_rotation(&self, id: TransformId, q: X) -> X {
        let n = &self.nodes[id];
        let q = match n.parent {
            Some(p) => self.inverse_transform_rotation(p, q),
            None => q,
        };
        let a = k::CONJ.xor(n.r);
        let q = quat_mul_inv(a, q);
        scale_sign_flip(n.s, q)
    }

    /// `SetLocalR` (UnityPlayer 0x1008EC10): the rotation normalised the engine's way (a horizontal sum whose
    /// order differs per lane, `q / sqrt(sum)`; the identity below 1e-30) and stored.
    fn set_local_r(&mut self, id: TransformId, q: X) {
        let sq = q.mul(q);
        let s1 = sq.pshufd(0x39).add(sq);
        let s = s1.pshufd(0x4E).add(s1);
        let m = k::EPS30.cmplt(s).psrad(31);
        let qn = q.div(s.sqrt());
        self.nodes[id].r = m.andn(k::QUAT_ID).or(qn.and(m));
    }

    /// `Transform.localRotation` setter (`Transform::SetLocalRotation`): normalised ([`set_local_r`]).
    pub fn set_local_rotation(&mut self, id: TransformId, q: Quaternion) {
        self.set_local_r(id, qx(q));
    }

    /// `Transform.rotation` setter (`Transform::SetRotation` 0x1065F2A0): into the parent's space
    /// (`InverseTransformRotation`), then normalised.
    pub fn set_rotation(&mut self, id: TransformId, q: Quaternion) {
        let mut q = qx(q);
        if let Some(p) = self.nodes[id].parent {
            q = self.inverse_transform_rotation(p, q);
        }
        self.set_local_r(id, q);
    }

    /// `CalculateGlobalMatrix` (UnityPlayer 0x1013FB80) for `Transform.localToWorldMatrix`: the node's
    /// rotation columns times its scale with its position, then each ancestor's applied to it.
    pub fn local_to_world_matrix(&self, id: TransformId) -> Matrix4x4 {
        to_matrix(self.global_matrix(id))
    }

    fn global_matrix(&self, id: TransformId) -> [X; 4] {
        let n = &self.nodes[id];
        let [r0, r1, r2] = rot_columns(n.r);
        let mut m = [n.s.pshufd(0x00).mul(r0), n.s.pshufd(0x55).mul(r1), n.s.pshufd(0xAA).mul(r2), n.t];
        let mut p = n.parent;
        while let Some(pi) = p {
            let n = &self.nodes[pi];
            let [r0, r1, r2] = rot_columns(n.r);
            let a = n.s.pshufd(0x00).mul(r0);
            let b = n.s.pshufd(0x55).mul(r1);
            let c = n.s.pshufd(0xAA).mul(r2);
            let col = |v: X| a.mul(v.pshufd(0x00)).add(b.mul(v.pshufd(0x55)).add(c.mul(v.pshufd(0xAA))));
            let c3 = a.mul(m[3].pshufd(0x00)).add(b.mul(m[3].pshufd(0x55)).add(c.mul(m[3].pshufd(0xAA)))).add(n.t);
            m = [col(m[0]), col(m[1]), col(m[2]), c3];
            p = n.parent;
        }
        m
    }

    /// `CalculateGlobalMatrixInverse` (UnityPlayer 0x10659450) for `Transform.worldToLocalMatrix`: each node's
    /// inverse (conjugate rotation's columns times the inverse scale, `-t` through them), the node's first,
    /// then times each ancestor's.
    pub fn world_to_local_matrix(&self, id: TransformId) -> Matrix4x4 {
        let inverse = |n: &Node| {
            let [r0, r1, r2] = rot_columns(k::CONJ.xor(n.r));
            let is = inv_scale(n.s);
            let (b0, b1, b2) = (is.mul(r0), is.mul(r1), is.mul(r2));
            let mt = k::SIGNS.xor(n.t);
            let t = mt.pshufd(0x00).mul(b0).add(mt.pshufd(0x55).mul(b1).add(mt.pshufd(0xAA).mul(b2)));
            [b0, b1, b2, t]
        };
        let n = &self.nodes[id];
        let mut m = inverse(n);
        let mut p = n.parent;
        while let Some(pi) = p {
            let b = inverse(&self.nodes[pi]);
            let col = |v: X| m[0].mul(v.pshufd(0x00)).add(m[1].mul(v.pshufd(0x55)).add(m[2].mul(v.pshufd(0xAA))));
            let t = b[3];
            let c3 = m[0].mul(t.pshufd(0x00)).add(m[1].mul(t.pshufd(0x55)).add(m[2].mul(t.pshufd(0xAA)))).add(m[3]);
            m = [col(b[0]), col(b[1]), col(b[2]), c3];
            p = self.nodes[pi].parent;
        }
        to_matrix(m)
    }

    /// `Transform::GetWorldToLocalMatrixNoScale` 0x1065BE50 (`CalculateInverseGlobalMatrixNoScale` 0x10659800,
    /// used by `Camera::GetWorldToCameraMatrix`): the global rotation's columns (`rot_columns`) transposed, and
    /// `-position` through them (`r0·(-x) + (r1·(-y) + r2·(-z))`). `CalculateGlobalPositionAndRotation`
    /// 0x100A0D10 walks up with the same steps as [`position`](Self::position) and
    /// [`rotation`](Self::rotation).
    pub fn world_to_local_matrix_no_scale(&self, id: TransformId) -> Matrix4x4 {
        let n = &self.nodes[id];
        let t = self.point_up(n.parent, n.t);
        let [c0, c1, c2] = rot_columns(self.global_rotation_x(id));
        let lo = c0.unpcklps(c1);
        let hi = c0.unpckhps(c1);
        let c2l = c2.unpcklps(c2);
        let c2h = c2.unpckhps(c2);
        let r0 = lo.movlhps(c2l);
        let r1 = c2l.movhlps(lo);
        let r2 = hi.movlhps(c2h);
        let mt = k::SIGNS.xor(t);
        let c3 = r0.mul(mt.pshufd(0x00)).add(r1.mul(mt.pshufd(0x55)).add(r2.mul(mt.pshufd(0xAA))));
        to_matrix([r0, r1, r2, c3])
    }

    /// `Transform.localRotation` (stored).
    pub fn local_rotation(&self, id: TransformId) -> Quaternion {
        xq(self.nodes[id].r)
    }
    /// `Transform.localPosition` (stored).
    pub fn local_position(&self, id: TransformId) -> Vector3 {
        xv3(self.nodes[id].t)
    }
    /// `Transform.localScale` (stored).
    pub fn local_scale(&self, id: TransformId) -> Vector3 {
        xv3(self.nodes[id].s)
    }
    /// `Transform.localPosition` setter (`SetLocalT`: stored with `w = 0`).
    pub fn set_local_position(&mut self, id: TransformId, v: Vector3) {
        self.nodes[id].t = v3x(v);
    }
    /// `Transform.localScale` setter (`SetLocalS`).
    pub fn set_local_scale(&mut self, id: TransformId, v: Vector3) {
        self.nodes[id].s = v3x(v);
    }
}

/// `InverseTransformRotation`'s product: the same shuffle form as [`quat_mul`] with the conjugated rotation
/// (it loads `a` conjugated, so the final conjugation of the result is its own `pxor`).
fn quat_mul_inv(a: X, b: X) -> X {
    // registers as in the routine: xmm3 = a (conjugated), xmm1 = b
    let x4 = a.movlhps(a).mul(b.pshufd(0xF5));
    let x2 = b.mul(a.pshufd(0xAF));
    let x0 = b.pshufd(0x88).mul(a);
    let x1 = b.pshufd(0xD2).mul(a);
    let x1 = x1.sub(x0.pshufd(0x1E)).sub(x2.pshufd(0x8D)).sub(x4.pshufd(0x63));
    k::CONJ.xor(x1.pshufd(0xD2))
}
