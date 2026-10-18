//! `Placemaker.Life.ButterflyFlock`, `Butterfly`, `ButterflyLanding` (row G5c): the butterflies round the flowers.
//!
//! Props with a `ButterflyLanding` (flowers) register with the flock as they turn on and off; each new flower
//! may bring a butterfly (up to `0.045 × flowers`, at most 32), sitting on it or fluttering round it. A butterfly
//! always belongs to a flower (`landing`): it wanders round a point 0.3 above it on Perlin noise, sometimes
//! settles on it, sometimes picks another flower near; one whose flower goes away looks for a free one near and
//! is put away when there is none. Flowers are found through the prop placer's collider grid
//! (`PropRadiusIterator<ButterflyLanding>`, [`perches_in_grid`]).
//!
//! The flowers are prop objects (their `TransformId` in the prop scene) and the host's ([`ButterflyWorld`]); the
//! butterflies' own objects live in the flock's [`Transforms`] (`Butterfly` root → `Body` with the mesh, as the
//! scene's `srcButterfly`), under `activeButterflies` / `disabledButterflies`, whose child order the flock
//! follows.

use std::collections::HashMap;

use crate::placemaker::life::bird_flock::{perches_in_grid, ColliderRef, LandingId};
use crate::unity::mathematics::Int3;
use crate::unity::mathf;
use crate::unity::quaternion::{quat, Quaternion};
use crate::unity::random::Rand;
use crate::unity::transform::{Node, TransformId, Transforms};
use crate::unity::vector::{v3, Vector3};

/// `Butterfly.State`: fluttering round its flower, landing on it, sitting.
pub mod butterfly_state {
    pub const FLYING: u8 = 0;
    pub const LANDING: u8 = 1;
    pub const SITTING: u8 = 2;
}
use butterfly_state as st;

/// `ButterflyFlock.absoluteMaxButterflyCount`.
pub const ABSOLUTE_MAX_BUTTERFLY_COUNT: i32 = 32;

/// The flock's meshes (the body's `MeshFilter.sharedMesh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButterflyMesh {
    #[default]
    None,
    Flap,
    Sit,
    SitFlap,
}

/// A butterfly's index in [`ButterflyFlock::all`].
pub type ButterflyId = usize;

/// `Placemaker.Life.Butterfly`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Butterfly {
    /// Its root (the component's object) and the `Body` child with the mesh, in the flock's transforms.
    pub root: TransformId,
    pub body: TransformId,
    pub mesh: ButterflyMesh,
    pub rotation0: Quaternion,
    pub rotation1: Quaternion,
    pub pos_mid: Vector3,
    pub velocity_prev: Vector3,
    pub velocity_next: Vector3,
    pub pos: Vector3,
    pub time: f32,
    pub state: u8,
    pub landing: Option<LandingId>,
}

/// `Placemaker.Life.ButterflyLanding`'s state.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ButterflyLanding {
    pub butterfly: Option<ButterflyId>,
}

/// What the flock reaches outside itself.
pub trait ButterflyWorld {
    /// `Time.time`.
    fn time(&self) -> f32;
    /// `propPlacer.colliderDict[cell]` (its list in order), if the cell has one; `landing` is the
    /// `ButterflyLanding` on the collider's object.
    fn colliders(&self, cell: Int3) -> Option<Vec<ColliderRef>>;
    /// A flower's `transform.position`, `rotation`, `up`, `TransformPoint(p)`, `TransformVector(v)`.
    fn landing_position(&self, l: LandingId) -> Vector3;
    fn landing_rotation(&self, l: LandingId) -> Quaternion;
    fn landing_up(&self, l: LandingId) -> Vector3;
    fn landing_transform_point(&self, l: LandingId, p: Vector3) -> Vector3;
    fn landing_transform_vector(&self, l: LandingId, v: Vector3) -> Vector3;
    /// `master.graph.SampleNormalCoverage(worldPos)`.
    fn sample_normal_coverage(&self, world_pos: Vector3) -> (Vector3, f32);
}

/// The scene's `srcButterfly`: its world pose and its `Body`'s local pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButterflyPrefab {
    pub root: (Vector3, Quaternion, Vector3),
    pub body: (Vector3, Quaternion, Vector3),
}

impl Default for ButterflyPrefab {
    /// As the GOG build's scene has it.
    fn default() -> ButterflyPrefab {
        let one = v3(1.0, 1.0, 1.0);
        ButterflyPrefab { root: (v3(0.0, 0.0459, 0.0), Quaternion::IDENTITY, one), body: (Vector3::ZERO, quat(-0.0, -0.0, -0.0, 1.0), one) }
    }
}

/// The component's state.
#[derive(Clone, Debug, PartialEq)]
pub struct ButterflyFlock {
    pub transforms: Transforms,
    /// `activeButterflies`, `disabledButterflies`: the containers.
    pub active: TransformId,
    pub disabled: TransformId,
    pub prefab: ButterflyPrefab,
    /// Every butterfly made (by id; where each is is its root's parent).
    pub all: Vec<Butterfly>,
    /// The flowers' state, by prop object.
    pub landings: HashMap<LandingId, ButterflyLanding>,
    pub landing_count: i32,
}

impl ButterflyFlock {
    /// The scene's containers (at the origin) and none made yet.
    pub fn new(prefab: ButterflyPrefab) -> ButterflyFlock {
        let mut transforms = Transforms::default();
        let active = transforms.add(Node::default());
        let disabled = transforms.add(Node::default());
        ButterflyFlock { transforms, active, disabled, prefab, all: vec![], landings: HashMap::new(), landing_count: 0 }
    }

    fn landing(&mut self, l: LandingId) -> &mut ButterflyLanding {
        self.landings.entry(l).or_default()
    }

    /// The butterfly whose root is `t`.
    pub fn of_root(&self, t: TransformId) -> ButterflyId {
        self.all.iter().position(|b| b.root == t).expect("a butterfly")
    }

    /// `Object.Instantiate(srcButterfly)` (`Butterfly..ctor` 0x7C5420: both rotations identity): at the
    /// prefab's pose, outside any container.
    pub fn instantiate(&mut self) -> ButterflyId {
        let (rp, rq, rs) = self.prefab.root;
        let (bp, bq, bs) = self.prefab.body;
        let t = &mut self.transforms;
        let root = t.add(Node::default());
        t.set_local_position(root, rp);
        t.set_local_rotation(root, rq);
        t.set_local_scale(root, rs);
        let body = t.add(Node { parent: Some(root), ..Default::default() });
        t.set_local_position(body, bp);
        t.set_local_rotation(body, bq);
        t.set_local_scale(body, bs);
        self.all.push(Butterfly {
            root,
            body,
            mesh: ButterflyMesh::None,
            rotation0: Quaternion::IDENTITY,
            rotation1: Quaternion::IDENTITY,
            pos_mid: Vector3::ZERO,
            velocity_prev: Vector3::ZERO,
            velocity_next: Vector3::ZERO,
            pos: Vector3::ZERO,
            time: 0.0,
            state: st::FLYING,
            landing: None,
        });
        self.all.len() - 1
    }

    /// `ButterflyFlock.SetButterfly(landing, butterfly)` 0x7C5220: logged when a flower that has one gets another.
    fn set_butterfly(&mut self, log_error: &mut dyn FnMut(&str), l: LandingId, b: Option<ButterflyId>) {
        let cur = self.landing(l).butterfly;
        if cur == b {
            return;
        }
        if cur.is_some() && b.is_some() {
            log_error("landing.butterfly");
        }
        self.landing(l).butterfly = b;
    }

    /// `ButterflyFlock.SetLanding(butterfly, landing)` 0x7C5330.
    fn set_landing(&mut self, log_error: &mut dyn FnMut(&str), b: ButterflyId, l: Option<LandingId>) {
        if self.all[b].landing == l {
            return;
        }
        if let Some(old) = self.all[b].landing {
            self.set_butterfly(log_error, old, None);
        }
        if let Some(new) = l {
            self.set_butterfly(log_error, new, Some(b));
        }
        self.all[b].landing = l;
    }

    /// `ButterflyFlock.AddLanding(landing)` 0x7C2D80 (`ButterflyLanding.IPropEnable.OnEnable` 0x1FBD50): one more
    /// flower; below `Min(32, RoundToInt(n × 0.05 × 0.9))` active butterflies, one (a disabled one, the first, or a
    /// new one sized 0.8..1.2) comes to it, posed as the flower, its clock 0 or 1 s back, half of them sitting
    /// (sit or, 20%, sit-flap), the rest flapping.
    pub fn add_landing(&mut self, world: &dyn ButterflyWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), l: LandingId) {
        self.landing_count += 1;
        let want = mathf::round_to_int(self.landing_count as f32 * 0.05 * 0.9).min(ABSOLUTE_MAX_BUTTERFLY_COUNT);
        if want <= self.transforms.child_count(self.active) as i32 {
            return;
        }
        let b = if self.transforms.child_count(self.disabled) == 0 {
            let b = self.instantiate();
            let s = random.range(0.8, 1.2);
            let one = Vector3::ONE;
            self.transforms.set_local_scale(self.all[b].root, v3(one.x * s, one.y * s, one.z * s));
            b
        } else {
            self.of_root(self.transforms.child(self.disabled, 0))
        };
        let root = self.all[b].root;
        self.transforms.set_parent(root, Some(self.active), true);
        self.transforms.set_position(root, world.landing_position(l));
        self.transforms.set_rotation(root, world.landing_rotation(l));
        let p = self.transforms.position(root);
        let q = world.landing_rotation(l);
        let now = world.time();
        let back = random.range_int(0, 2);
        let r = random.value();
        let sits = !(0.5 > r);
        let bf = &mut self.all[b];
        bf.pos_mid = p;
        bf.pos = p;
        bf.velocity_next = Vector3::ZERO;
        bf.velocity_prev = Vector3::ZERO;
        bf.rotation1 = q;
        bf.rotation0 = q;
        bf.time = now - back as f32;
        bf.state = if sits { st::SITTING } else { st::FLYING };
        bf.mesh = if !sits {
            ButterflyMesh::Flap
        } else if 0.2 > random.value() {
            ButterflyMesh::SitFlap
        } else {
            ButterflyMesh::Sit
        };
        self.set_landing(log_error, b, Some(l));
    }

    /// `ButterflyFlock.RemoveLanding(landing)` 0x7C5090 (`ButterflyLanding.IPropEnable.OnDisable` 0x1FBD20): one
    /// flower fewer; within `RoundToInt(n × 0.05 × 1.1)` active butterflies, its butterfly flies off (and has to
    /// find another); above, its butterfly (or the first active one) is put away.
    pub fn remove_landing(&mut self, log_error: &mut dyn FnMut(&str), l: LandingId) {
        self.landing_count -= 1;
        let want = mathf::round_to_int(self.landing_count as f32 * 0.05 * 1.1);
        let active = self.transforms.child_count(self.active) as i32;
        let lb = self.landing(l).butterfly;
        if active <= want {
            if let Some(b) = lb {
                self.set_landing(log_error, b, None);
                let bf = &mut self.all[b];
                if bf.state == st::SITTING {
                    bf.mesh = ButterflyMesh::Flap;
                }
                bf.state = st::FLYING;
            }
            return;
        }
        let t = match lb {
            Some(b) => self.all[b].root,
            None => self.transforms.child(self.active, 0),
        };
        self.transforms.set_parent(t, Some(self.disabled), true);
    }

    /// `ButterflyFlock.OnReset` 0x7C3190: every active butterfly (from the last) is put away.
    pub fn on_reset(&mut self) {
        for i in (0..self.transforms.child_count(self.active)).rev() {
            let t = self.transforms.child(self.active, i);
            self.transforms.set_parent(t, Some(self.disabled), true);
        }
    }

    /// `ButterflyFlock.OnUpdate` 0x7C3200: every active butterfly (from the last) whose 0.3 s step is due steps;
    /// then each is placed between its steps.
    pub fn on_update(&mut self, world: &dyn ButterflyWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str)) {
        let mut count = self.transforms.child_count(self.active);
        let mut i = count;
        while i > 0 {
            i -= 1;
            let b = self.of_root(self.transforms.child(self.active, i));
            if !(world.time() > self.all[b].time + 0.3) {
                continue;
            }
            if !self.step(world, random, log_error, b) {
                count -= 1;
            }
        }
        for i in 0..count {
            let b = self.of_root(self.transforms.child(self.active, i));
            let now = world.time();
            let bf = &mut self.all[b];
            let (pm, vp, vn) = (bf.pos_mid, bf.velocity_prev, bf.velocity_next);
            // the quadratic Bézier of the birds' (BirdFlock.OnUpdate)
            let p0 = v3(pm.x - vp.x * 0.5, pm.y - vp.y * 0.5, pm.z - vp.z * 0.5);
            let p2 = v3(vn.x * 0.5 + pm.x, vn.y * 0.5 + pm.y, vn.z * 0.5 + pm.z);
            let t = mathf::inverse_lerp(bf.time, bf.time + 0.3, now);
            let u = 1.0 - t;
            let u2 = u * u;
            let k = (u + u) * t;
            let t2 = t * t;
            bf.pos = v3(u2 * p0.x + k * pm.x + t2 * p2.x, u2 * p0.y + k * pm.y + t2 * p2.y, u2 * p0.z + k * pm.z + t2 * p2.z);
            let (root, pos, q) = (bf.root, bf.pos, Quaternion::lerp(bf.rotation0, bf.rotation1, t));
            self.transforms.set_position(root, pos);
            self.transforms.set_rotation(root, q);
        }
    }

    /// One step of butterfly `b`; false when it was put away (no flower near).
    fn step(&mut self, world: &dyn ButterflyWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), b: ButterflyId) -> bool {
        let colliders = |c: Int3| world.colliders(c);
        let bf = &mut self.all[b];
        let vn = bf.velocity_next;
        bf.pos_mid = v3(vn.x + bf.pos_mid.x, vn.y + bf.pos_mid.y, vn.z + bf.pos_mid.z);
        bf.velocity_prev = vn;
        bf.time += 0.3;
        bf.rotation0 = bf.rotation1;
        if bf.state == st::SITTING {
            bf.velocity_next = Vector3::ZERO;
            if random.value() < 0.004 {
                bf.state = st::FLYING;
                let root = bf.root;
                let v = self.transforms.transform_vector(root, v3(0.0, 0.4, 0.2));
                let bf = &mut self.all[b];
                bf.velocity_next = v;
                bf.mesh = ButterflyMesh::Flap;
            }
            return true;
        }
        if bf.state > st::SITTING {
            return true;
        }
        if bf.landing.is_none() {
            // a free flower near where it is going (0.4 below the point ahead)
            let pm = bf.pos_mid;
            let down = Vector3::DOWN;
            let c = v3((vn.x + pm.x) - down.x * 0.4, (vn.y + pm.y) - down.y * 0.4, (vn.z + pm.z) - down.z * 0.4);
            for (l, _) in perches_in_grid(&colliders, c, 0.8, 0.8 * 0.8) {
                if self.landings.get(&l).and_then(|s| s.butterfly).is_none() {
                    self.set_landing(log_error, b, Some(l));
                }
            }
            if self.all[b].landing.is_none() {
                let root = self.all[b].root;
                self.transforms.set_parent(root, Some(self.disabled), true);
                return false;
            }
        }
        let l = self.all[b].landing.expect("a flower");
        let state = self.all[b].state;
        let bf = &mut self.all[b];
        bf.velocity_next = v3(bf.velocity_next.x * 0.7, bf.velocity_next.y * 0.7, bf.velocity_next.z * 0.7);
        let p = world.landing_transform_point(l, v3(0.0, 0.01, 0.0));
        let off = world.landing_transform_vector(l, v3(0.0, 0.3, 0.0));
        let pm = bf.pos_mid;
        let target = if state == st::LANDING {
            // lower while far: the offset shrinks with the squared distance
            let m = mathf::max(0.0, v3(p.x - pm.x, p.y - pm.y, p.z - pm.z).sqr_magnitude());
            v3(off.x * m + p.x, p.y + off.y * m, p.z + off.z * m)
        } else {
            v3(p.x + off.x, p.y + off.y, p.z + off.z)
        };
        let d = v3(target.x - pm.x, target.y - pm.y, target.z - pm.z);
        let sq = d.sqr_magnitude();
        if state == st::LANDING {
            if sq < 0.02 {
                // down on the flower, along its up, roughly facing on, with a random tilt
                bf.state = st::SITTING;
                bf.velocity_next = v3(p.x - pm.x, p.y - pm.y, p.z - pm.z);
                let up = world.landing_up(l);
                let fwd = self.transforms.forward(self.all[b].root);
                let u = Vector3::UP;
                let f = v3(u.x * 0.2 + fwd.x, fwd.y + u.y * 0.2, fwd.z + u.z * 0.2);
                let s = random.inside_unit_sphere();
                let g = v3(s.x * 0.5 + f.x, s.y * 0.5 + f.y, s.z * 0.5 + f.z);
                let bf = &mut self.all[b];
                bf.rotation1 = Quaternion::mul(Quaternion::look_rotation_up(up, g), Quaternion::euler(90.0, 0.0, 0.0));
                bf.mesh = if 0.2 > random.value() { ButterflyMesh::Sit } else { ButterflyMesh::SitFlap };
                return true;
            }
            let vn = bf.velocity_next;
            bf.velocity_next = v3(d.x * 0.1 + vn.x, vn.y + d.y * 0.1, vn.z + d.z * 0.1);
        } else {
            if 0.5 < sq {
                let k = sq - 0.5;
                let vn = bf.velocity_next;
                bf.velocity_next = v3(d.x * k * 0.5 + vn.x, vn.y + d.y * k * 0.5, vn.z + d.z * k * 0.5);
            }
            // flutter: Perlin noise along the clock, one sample per axis
            let t = bf.time * 0.01;
            let n1 = mathf::perlin_noise(pm.y, t + f32::from_bits(0x3F0A_8C15)) - 0.5;
            let n2 = mathf::perlin_noise(pm.z, t + f32::from_bits(0x4083_F3B6)) - 0.5;
            let n3 = mathf::perlin_noise(pm.x, t + f32::from_bits(0x411D_816F)) - 0.5;
            let vn = bf.velocity_next;
            bf.velocity_next = v3(n1 * 0.4 + vn.x, vn.y + n2 * 0.4, vn.z + n3 * 0.4);
            // the ground (0.1 below the next point) pushes up
            let vn = bf.velocity_next;
            let probe = v3((vn.x + pm.x) - 0.0, (vn.y + pm.y) - 0.1, (vn.z + pm.z) - 0.0);
            let (n, cov) = world.sample_normal_coverage(probe);
            if 0.3 < cov {
                let k = cov - 0.3;
                bf.velocity_next = v3(k * n.x + k * n.x + vn.x, vn.y + (k * n.y + k * n.y), vn.z + (k * n.z + k * n.z));
            }
        }
        // (a random point drawn and not used)
        random.inside_unit_sphere();
        let vn = bf.velocity_next;
        let n = vn.normalized();
        bf.velocity_next = Vector3::lerp(vn, v3(n.x * 0.2, n.y * 0.2, n.z * 0.2), 0.3);
        let vn = bf.velocity_next;
        if vn.x.is_nan() || vn.y.is_nan() || vn.z.is_nan() {
            log_error("NaN velocity");
            let lp = world.landing_position(l);
            let bf = &mut self.all[b];
            bf.rotation0 = Quaternion::IDENTITY;
            bf.rotation1 = Quaternion::IDENTITY;
            bf.velocity_prev = Vector3::ZERO;
            bf.velocity_next = Vector3::ZERO;
            bf.pos = lp;
            bf.pos_mid = lp;
        }
        let bf = &mut self.all[b];
        if state == st::LANDING {
            let vn = bf.velocity_next;
            bf.velocity_next = v3((d.x + vn.x) * 0.5, (d.y + vn.y) * 0.5, (d.z + vn.z) * 0.5);
        }
        // facing its flight, its climb halved; still: its body's level heading
        let vn = bf.velocity_next;
        if !Vector3::approx_eq(vn, Vector3::ZERO) {
            let a = v3(vn.x, vn.y * 0.5, vn.z);
            if 0.001 < a.sqr_magnitude() {
                bf.rotation1 = Quaternion::look_rotation(a);
            }
        } else {
            let f = self.transforms.forward(self.all[b].body);
            let f = v3(f.x, 0.0, f.z);
            if 0.001 < f.sqr_magnitude() {
                self.all[b].rotation1 = Quaternion::look_rotation(f);
            }
        }
        if state == st::LANDING {
            return true;
        }
        if random.value() < 0.004 {
            self.all[b].state = st::LANDING;
            return true;
        }
        if random.value() < 0.05 {
            // another free flower 0.4 below it, the nearest by squared distance plus a random 0..1
            let pm = self.all[b].pos_mid;
            let c = v3(pm.x - 0.0, pm.y - 0.4, pm.z - 0.0);
            let mut best = f32::MAX;
            let mut found = None;
            for (o, op) in perches_in_grid(&colliders, c, 0.8, 0.8 * 0.8) {
                if self.landings.get(&o).and_then(|s| s.butterfly).is_some() {
                    continue;
                }
                let r = random.value();
                let sq = v3(c.x - op.x, c.y - op.y, c.z - op.z).sqr_magnitude();
                if sq + r < best {
                    best = sq + r;
                    found = Some(o);
                }
            }
            if let Some(o) = found {
                self.set_landing(log_error, b, Some(o));
            }
        }
        true
    }
}
