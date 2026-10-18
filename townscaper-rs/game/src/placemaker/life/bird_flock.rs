//! `Placemaker.Life.BirdFlock`, `Bird`, `BirdLanding` (rows G5a, G5b): the birds that sit on the town's props.
//!
//! Props with a `BirdLanding` (perches) register with the flock as they turn on and off. The flock wants a bird
//! count from the number of perches (`CalculateOptimalLandings`), creates birds on random free perches up to it
//! (`IterateBirdCreation`), uproots birds near a click (`UprootRadius`, used by the click effect), and moves them
//! (`OnUpdate`, row G5b). Perches are found through the prop placer's collider grid (2-unit cells): a perch counts
//! when its prop is turned on; each perch counts the occupied perches within 1.1 (`neighbourCount`).
//!
//! Perches are prop objects (their `TransformId` in the prop scene); the scene, the prop grid, the squares, sounds
//! and particles are the host's ([`BirdWorld`]). The birds' own objects live in the flock's [`Transforms`]
//! (`Bird` root → `Shade`, `Body` → `Mesh0`, `Mesh1`, as the scene's `srcBird`).

use std::collections::HashMap;

use crate::os::extra_math;
use crate::os::hex;
use crate::os::utils;
use crate::placemaker::grid_generator;
use crate::unity::mathematics::{f3, Int3, Int3x2};
use crate::unity::mathf;
use crate::unity::quaternion::Quaternion;
use crate::unity::random::Rand;
use crate::unity::transform::{Node, TransformId, Transforms};
use crate::unity::vector::{sqrt_f64, v3, Vector3};

/// `BirdFlock.State`.
pub mod bird_state {
    pub const BOIDING: u8 = 0;
    pub const LANDING: u8 = 1;
    pub const STOPPING: u8 = 2;
    pub const STOPPED: u8 = 3;
    pub const LEAVING: u8 = 4;
}
use bird_state as st;

/// `BirdFlock.absoluteMaxBirdCount`.
pub const ABSOLUTE_MAX_BIRD_COUNT: i32 = 64;
/// `BirdFlock.dt`: the birds' simulation step.
pub const DT: f32 = 0.3;
/// `BirdFlock.landingNeigbourRadius` and its square.
pub const LANDING_NEIGHBOUR_RADIUS: f32 = 1.1;
pub const LANDING_NEIGHBOUR_SQ_RADIUS: f32 = 1.21;

/// The flock's meshes (`MeshFilter.sharedMesh` values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BirdMesh {
    #[default]
    None,
    Sit,
    HalfSit,
    Stand,
    Land,
    Flap,
    Fly,
    Glide,
}

/// A perch (a prop object with a `BirdLanding`).
pub type LandingId = TransformId;
/// A bird's index in [`BirdFlock::all_birds`].
pub type BirdId = usize;

/// `Placemaker.Life.BirdLanding`'s state.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BirdLanding {
    pub bird: Option<BirdId>,
    pub neighbour_count: u8,
    pub normal: Vector3,
}

impl BirdLanding {
    /// `BirdLanding.AddNeighbour` 0x7C1170.
    pub fn add_neighbour(&mut self) {
        self.neighbour_count = self.neighbour_count.wrapping_add(1);
    }
    /// `BirdLanding.RemoveNeighbour` 0x7C1490.
    pub fn remove_neighbour(&mut self) {
        self.neighbour_count = self.neighbour_count.wrapping_sub(1);
    }
    /// `BirdLanding.ClearNeighbours` 0x315B50.
    pub fn clear_neighbours(&mut self) {
        self.neighbour_count = 0;
    }
}

/// `Placemaker.Life.Bird`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bird {
    /// The bird's root, `body`, `shade`, and the two mesh objects (`mf0`, `mf1`) in the flock's transforms.
    pub root: TransformId,
    pub body: TransformId,
    pub shade: TransformId,
    pub mf0: TransformId,
    pub mf1: TransformId,
    pub mesh0: BirdMesh,
    pub mesh1: BirdMesh,
    /// `mf0.gameObject.activeSelf`, `shade.gameObject.activeSelf`.
    pub mf0_active: bool,
    pub shade_active: bool,
    pub rotation0: Quaternion,
    pub rotation1: Quaternion,
    pub pos_mid: Vector3,
    pub velocity_prev: Vector3,
    pub velocity_next: Vector3,
    pub pos: Vector3,
    pub time: f32,
    pub preferred_neighbour_count: u8,
    pub state_prev: u8,
    pub state_next: u8,
    pub last_action_time: f32,
    pub landing: Option<LandingId>,
}

impl Bird {
    /// `Bird.IComparable<Bird>.CompareTo` 0x7C1AA0 / `IComparable<Vector3>.CompareTo` 0x7C1AD0: by `pos.x`
    /// (`Single.CompareTo`: NaN first).
    pub fn compare_x(&self, x: f32) -> i32 {
        single_compare(self.pos.x, x)
    }
}

/// `Single.CompareTo(value)`.
pub fn single_compare(a: f32, b: f32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else if a.is_nan() {
        if b.is_nan() {
            0
        } else {
            -1
        }
    } else {
        1
    }
}

/// A `PropCollider` in a cell of `propPlacer.colliderDict`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderRef {
    /// `propCollider.transform.position`.
    pub position: Vector3,
    /// `TryGetComponent<BirdLanding>()` on its object.
    pub landing: Option<LandingId>,
    /// `GetComponent<PropNode>().turnedOn`.
    pub turned_on: bool,
}

/// What the flock reaches outside itself.
pub trait BirdWorld {
    /// `Time.time`.
    fn time(&self) -> f32;
    /// `propPlacer.colliderDict[cell]` (its list in order), if the cell has one.
    fn colliders(&self, cell: Int3) -> Option<Vec<ColliderRef>>;
    /// A perch's `transform.position`, `TransformPoint(p)`, `rotation`, `up`.
    fn landing_position(&self, l: LandingId) -> Vector3;
    fn landing_transform_point(&self, l: LandingId, p: Vector3) -> Vector3;
    fn landing_rotation(&self, l: LandingId) -> Quaternion;
    fn landing_up(&self, l: LandingId) -> Vector3;
    /// `graph.squares.childCount`, and the active `BirdLanding`s under square `i` (`GetComponentsInChildren`).
    fn square_count(&self) -> i32;
    fn landings_in_square(&self, i: i32) -> Vec<LandingId>;
    /// `master.manySoundsManager.PlaySound(sound, position, 0, 1)`.
    fn play_sound(&mut self, sound: i32, position: Vector3);
    /// `featherEffect` moved, turned and played.
    fn feathers(&mut self, position: Vector3, rotation: Quaternion);
    /// `Time.deltaTime`.
    fn delta_time(&self) -> f32;
    /// `master.graph.bounds`, `master.graph.boundsHeight`.
    fn graph_bounds(&self) -> Int3x2;
    fn graph_bounds_height(&self) -> i32;
    /// `master.graph.SampleNormalCoverage(worldPos)`.
    fn sample_normal_coverage(&self, world_pos: Vector3) -> (Vector3, f32);
    /// `Camera.main.WorldToViewportPoint(worldPos)`.
    fn world_to_viewport_point(&self, world_pos: Vector3) -> Vector3;
}

/// What a bird's step did (`OnUpdate`'s loop body).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Went on to the mesh swap (`mf0` shown while the two meshes differ).
    Show,
    /// Went straight to the next bird.
    Done,
    /// Left the flock (`birds[i]` removed).
    Removed,
}

/// The scene's `srcBird`'s local poses (root, shade, body, mesh 0, mesh 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BirdPrefab {
    pub locals: [(Vector3, Quaternion, Vector3); 5],
}

impl Default for BirdPrefab {
    /// As the GOG build's scene has it.
    fn default() -> BirdPrefab {
        let one = v3(1.0, 1.0, 1.0);
        let q = Quaternion::IDENTITY;
        BirdPrefab {
            locals: [
                (v3(0.0, 0.1113, 0.0), q, one),
                (v3(0.0, -0.064, 0.0), q, v3(0.20958, 0.20958, 0.20958)),
                (Vector3::ZERO, q, one),
                (Vector3::ZERO, q, one),
                (Vector3::ZERO, q, one),
            ],
        }
    }
}

/// The component's state.
#[derive(Clone, Debug, PartialEq)]
pub struct BirdFlock {
    pub transforms: Transforms,
    /// `activeBirds`, `disabledBirds`: the containers.
    pub active_birds: TransformId,
    pub disabled_birds: TransformId,
    pub prefab: BirdPrefab,
    /// Every bird made (by id; where each sits is its root's parent).
    pub all_birds: Vec<Bird>,
    /// `birds`: the active ones, kept sorted by `pos.x`.
    pub birds: Vec<BirdId>,
    /// The perches' state, by prop object.
    pub landings: HashMap<LandingId, BirdLanding>,
    pub bird_landing_count: i32,
    pub assigned_birds_count: u8,
    pub active_birds_count: u8,
    pub total_bird_count: u8,
    pub max_bird_count: u8,
    pub min_bird_count: u8,
    pub ideal_bird_count: u8,
    pub optimal_bird_count_dirty: bool,
    pub take_off_count: i32,
    pub take_off_pos: Vector3,
    pub last_audio_time: f32,
    pub orbit_pos: Vector3,
    pub orbit_target: Vector3,
    pub last_orbit_time: f32,
    pub sqr_shake_detection_threshold: f32,
    pub min_shake_interval: f32,
    pub time_since_last_shake: f32,
}

impl BirdFlock {
    /// `BirdFlock..ctor` 0x7C1110 with the scene's containers (at the origin) and values.
    pub fn new(prefab: BirdPrefab) -> BirdFlock {
        let mut transforms = Transforms::default();
        let active_birds = transforms.add(Node::default());
        let disabled_birds = transforms.add(Node::default());
        BirdFlock {
            transforms,
            active_birds,
            disabled_birds,
            prefab,
            all_birds: vec![],
            birds: vec![],
            landings: HashMap::new(),
            bird_landing_count: 0,
            assigned_birds_count: 0,
            active_birds_count: 0,
            total_bird_count: 0,
            max_bird_count: 0,
            min_bird_count: 0,
            ideal_bird_count: 0,
            optimal_bird_count_dirty: false,
            take_off_count: 0,
            take_off_pos: Vector3::ZERO,
            last_audio_time: 0.0,
            orbit_pos: Vector3::ZERO,
            orbit_target: Vector3::ZERO,
            last_orbit_time: 0.0,
            sqr_shake_detection_threshold: 42.0,
            min_shake_interval: 1.0,
            time_since_last_shake: 0.0,
        }
    }

    fn landing(&mut self, l: LandingId) -> &mut BirdLanding {
        self.landings.entry(l).or_default()
    }

    /// `BirdFlock.CalculateOptimalLandings` 0x7BA790: from the perch count `n`: ideal `floor(c)`, min
    /// `max(0, 0.9c - 2)`, max `max(0, 1.1c + 2)` with `c = clamp(sqrt(n) - 6, 0, 64)`.
    pub fn calculate_optimal_landings(&mut self) {
        if !self.optimal_bird_count_dirty {
            return;
        }
        self.optimal_bird_count_dirty = false;
        if self.bird_landing_count < 1 {
            self.min_bird_count = 0;
            self.ideal_bird_count = 0;
            self.max_bird_count = 0;
            return;
        }
        let c = mathf::clamp(((self.bird_landing_count as f64).sqrt() as f32) - 6.0, 0.0, 64.0);
        self.ideal_bird_count = mathf::floor_to_int(c) as u8;
        self.min_bird_count = mathf::max(0.0, c * 0.9 - 2.0) as i32 as u8;
        self.max_bird_count = mathf::max(0.0, c * 1.1 + 2.0) as i32 as u8;
    }

    /// `BirdFlock.GetSearchCoordinates(pos, radius)` 0x7BB060: the collider grid's cells (2 units) round
    /// `pos`: `floor((p - r) / 2)` .. `ceil((p + r) / 2)` (exclusive).
    pub fn get_search_coordinates(pos: Vector3, radius: f32) -> (Int3, Int3) {
        let one = v3(1.0, 1.0, 1.0);
        let (ry, rz) = (one.y * radius, one.z * radius);
        let min = Int3 {
            x: mathf::floor_to_int((pos.x - one.x * radius) * 0.5),
            y: mathf::floor_to_int((pos.y - ry) * 0.5),
            z: mathf::floor_to_int((pos.z - rz) * 0.5),
        };
        let max = Int3 {
            x: mathf::ceil_to_int((one.x * radius + pos.x) * 0.5),
            y: mathf::ceil_to_int((ry + pos.y) * 0.5),
            z: mathf::ceil_to_int((rz + pos.z) * 0.5),
        };
        (min, max)
    }

    /// The perches within `sq_radius` of `pos` (squared) on turned-on props, in the game's search order (cells
    /// z, y, x; each cell's list from the end; only colliders inside their own cell).
    fn perches_near(world: &dyn BirdWorld, pos: Vector3, radius: f32, sq_radius: f32) -> Vec<(LandingId, Vector3)> {
        let (min, max) = Self::get_search_coordinates(pos, radius);
        let mut out = vec![];
        for z in min.z..max.z {
            for y in min.y..max.y {
                for x in min.x..max.x {
                    let Some(list) = world.colliders(Int3 { x, y, z }) else { continue };
                    let lo = v3(x as f32 + x as f32, y as f32 + y as f32, z as f32 + z as f32);
                    let hi = v3((x + 1) as f32 + (x + 1) as f32, (y + 1) as f32 + (y + 1) as f32, (z + 1) as f32 + (z + 1) as f32);
                    for c in list.iter().rev() {
                        let p = c.position;
                        if lo.x <= p.x && lo.y <= p.y && lo.z <= p.z && p.x < hi.x && p.y < hi.y && p.z < hi.z {
                            let d = v3(pos.x - p.x, pos.y - p.y, pos.z - p.z);
                            if d.sqr_magnitude() <= sq_radius {
                                if let Some(l) = c.landing {
                                    if c.turned_on {
                                        out.push((l, p));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// `BirdLanding.get_sortPos` 0x7C15A0: `normal * 0.1 + position`.
    pub fn sort_pos(&self, world: &dyn BirdWorld, l: LandingId) -> Vector3 {
        let p = world.landing_position(l);
        let n = self.landings.get(&l).map_or(Vector3::ZERO, |s| s.normal);
        v3(n.x * 0.1 + p.x, n.y * 0.1 + p.y, n.z * 0.1 + p.z)
    }

    /// `BirdLanding.IComparable<BirdLanding>.CompareTo` 0x7C14A0 / `IComparable<Vector3>.CompareTo` 0x7C1500: by
    /// the sort position's x.
    pub fn landing_compare_x(&self, world: &dyn BirdWorld, l: LandingId, x: f32) -> i32 {
        single_compare(self.sort_pos(world, l).x, x)
    }

    /// `BirdLanding.get_targetPos` 0x7C1670: `normal * 0.3 + position`.
    pub fn target_pos(&self, world: &dyn BirdWorld, l: LandingId) -> Vector3 {
        let p = world.landing_position(l);
        let n = self.landings.get(&l).map_or(Vector3::ZERO, |s| s.normal);
        v3(n.x * 0.3 + p.x, n.y * 0.3 + p.y, n.z * 0.3 + p.z)
    }

    /// `BirdLanding.IPropEnable.OnFirstEnable` 0x7C1300: the perch's normal: `TransformVector(-1, 0, 0)`, bent
    /// halfway to the graph's sampled normal where it has coverage (`normal_coverage`: `graph.SampleNormalCoverage
    /// (position, transform)`).
    pub fn landing_on_first_enable(&mut self, l: LandingId, transformed_left: Vector3, normal_coverage: (Vector3, f32)) {
        let mut n = transformed_left;
        let (sn, cov) = normal_coverage;
        if 0.0 < cov {
            n = v3(sn.x * 0.5 + n.x, sn.y * 0.5 + n.y, sn.z * 0.5 + n.z).normalized();
        }
        self.landing(l).normal = n;
    }

    /// `BirdFlock.SetBird(landing, bird)` 0x7C00E0: a perch gets (or loses) its bird; the assigned count and
    /// every turned-on perch within 1.1 (itself too) count it.
    fn set_bird(&mut self, world: &mut dyn BirdWorld, log_error: &mut dyn FnMut(&str), l: LandingId, bird: Option<BirdId>) {
        let cur = self.landing(l).bird;
        if cur == bird {
            return;
        }
        if cur.is_some() && bird.is_some() {
            log_error("landing.bird");
        }
        self.landing(l).bird = bird;
        let pos = world.landing_position(l);
        self.assigned_birds_count = if bird.is_some() { self.assigned_birds_count.wrapping_add(1) } else { self.assigned_birds_count.wrapping_sub(1) };
        for (other, _) in Self::perches_near(world, pos, 1.2, LANDING_NEIGHBOUR_SQ_RADIUS) {
            let s = self.landing(other);
            s.neighbour_count = if bird.is_some() { s.neighbour_count.wrapping_add(1) } else { s.neighbour_count.wrapping_sub(1) };
        }
    }

    /// `BirdFlock.SetLanding(bird, landing)` 0x7C0610.
    fn set_landing(&mut self, world: &mut dyn BirdWorld, log_error: &mut dyn FnMut(&str), b: BirdId, l: Option<LandingId>) {
        if self.all_birds[b].landing == l {
            return;
        }
        if let Some(old) = self.all_birds[b].landing {
            self.set_bird(world, log_error, old, None);
        }
        if let Some(new) = l {
            self.set_bird(world, log_error, new, Some(b));
        }
        self.all_birds[b].landing = l;
        self.all_birds[b].last_action_time = world.time();
    }

    /// `BirdFlock.SetState(bird, state)` 0x7C0720: logged when landing without a perch.
    pub fn set_state(&mut self, world: &dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), b: BirdId, state: u8) {
        if state == st::LANDING && self.all_birds[b].landing.is_none() {
            log_error("No landing");
        }
        self.all_birds[b].state_next = state;
        let t = world.time();
        let r = random.value();
        self.all_birds[b].last_action_time = r + t;
    }

    /// `BirdFlock.GetLandingPos(bird)` 0x7BA8A0: on the perch, shifted by the bird's size.
    pub fn get_landing_pos(&self, world: &dyn BirdWorld, b: BirdId) -> Vector3 {
        let bird = &self.all_birds[b];
        let l = bird.landing.expect("GetLandingPos: no landing");
        let s = self.transforms.local_scale(bird.root);
        let neg = f32::from_bits(std::hint::black_box(s.x.to_bits()) ^ 0x8000_0000);
        world.landing_transform_point(l, v3(neg * 0.04, 0.0, 0.0))
    }

    /// `BirdFlock.AddOrRemoveLanding(landing, landingAdded)` 0x7BA100 (the perches' `OnEnable` / `OnDisable`):
    /// a new perch counts its occupied neighbours (the first one sets the orbit); a perch that goes away lets its
    /// bird fly on (or, past the maximum, puffs it away in feathers).
    pub fn add_or_remove_landing(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), l: LandingId, added: bool) {
        if added {
            if self.assigned_birds_count != 0 {
                let pos = world.landing_position(l);
                for (other, _) in Self::perches_near(world, pos, 1.2, LANDING_NEIGHBOUR_SQ_RADIUS) {
                    if self.landings.get(&other).and_then(|s| s.bird).is_some() {
                        let s = self.landing(l);
                        s.neighbour_count = s.neighbour_count.wrapping_add(1);
                    }
                }
            }
            if self.bird_landing_count == 0 {
                let t = self.target_pos(world, l);
                self.orbit_pos = t;
                self.orbit_target = t;
            }
            self.optimal_bird_count_dirty = true;
            self.bird_landing_count += 1;
            return;
        }
        self.bird_landing_count -= 1;
        self.optimal_bird_count_dirty = true;
        let Some(b) = self.landing(l).bird else {
            self.landing(l).neighbour_count = 0;
            return;
        };
        self.calculate_optimal_landings();
        if self.active_birds_count <= self.max_bird_count {
            self.uproot_bird(world, random, log_error, b, 1.0);
            self.landing(l).neighbour_count = 0;
            return;
        }
        self.set_landing(world, log_error, b, None);
        self.all_birds[b].state_next = st::LEAVING;
        let t = world.time();
        let r = random.value();
        self.all_birds[b].last_action_time = r + t;
        self.birds.retain(|&x| x != b);
        let root = self.all_birds[b].root;
        self.transforms.set_parent(root, Some(self.disabled_birds), true);
        let n = self.landings.get(&l).map_or(Vector3::ZERO, |s| s.normal);
        world.feathers(world.landing_position(l), Quaternion::look_rotation(n));
        self.active_birds_count = self.active_birds_count.wrapping_sub(1);
        self.landing(l).neighbour_count = 0;
    }

    /// `BirdFlock.TryGetRandomLanding(out landing)` 0x7C0810: up to 100 random squares tried for perches; one of
    /// the first square's that has any, at random.
    fn try_get_random_landing(world: &dyn BirdWorld, random: &mut Rand) -> Option<LandingId> {
        let n = world.square_count();
        if n == 0 {
            return None;
        }
        for _ in 0..100 {
            let i = random.range_int(0, n);
            let ls = world.landings_in_square(i);
            if !ls.is_empty() {
                let k = random.range_int(0, ls.len() as i32);
                return Some(ls[k as usize]);
            }
        }
        None
    }

    /// `ExtraMath.BinarySearch<Bird, Vector3>(birds, value)` over the active birds by `pos.x`.
    fn search_birds_x(&self, x: f32) -> i32 {
        let (mut lo, mut hi) = (0i32, self.birds.len() as i32 - 1);
        let (mut mid, mut d) = (0i32, 0i32);
        while lo <= hi {
            mid = (lo + hi) / 2;
            d = -self.all_birds[self.birds[mid as usize]].compare_x(x);
            if d == 0 {
                return mid;
            }
            if d < 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }
        !(if d <= 0 { mid } else { mid + 1 })
    }

    /// `BirdFlock.UprootRadius(pos, radius, speed)` 0x7C0F00: every active bird within `radius` (from the first
    /// at `x >= pos.x - radius` in the x-sorted list, until one is past `pos.x + radius`) takes off.
    pub fn uproot_radius(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), pos: Vector3, radius: f32, speed: f32) {
        let mut i = self.search_birds_x(pos.x - radius);
        if i < 0 {
            i = !i;
        }
        let max = pos.x + radius;
        let n = self.birds.len() as i32;
        while i < n {
            let b = self.birds[i as usize];
            let p = self.all_birds[b].pos;
            if max < p.x {
                return;
            }
            if v3(p.x - pos.x, p.y - pos.y, p.z - pos.z).sqr_magnitude() <= radius * radius {
                self.uproot_bird(world, random, log_error, b, speed);
            }
            i += 1;
        }
    }

    /// `BirdFlock.UprootAllBirds` 0x7C09B0 (a context menu): every bird on a perch takes off.
    pub fn uproot_all_birds(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str)) {
        for b in self.birds.clone() {
            if self.all_birds[b].landing.is_some() {
                self.uproot_bird(world, random, log_error, b, 1.0);
            }
        }
    }

    /// `BirdFlock.UprootBird(bird, speed)` 0x7C0B10: a sitting bird jumps off along its perch's normal (with a
    /// random kick); off its perch, boiding, flapping, with the take-off sounds; the first take-off counted and
    /// its position kept.
    pub fn uproot_bird(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), b: BirdId, speed: f32) {
        if self.all_birds[b].state_next == st::STOPPED {
            let l = self.all_birds[b].landing.expect("UprootBird: no landing");
            let n = self.landings.get(&l).map_or(Vector3::ZERO, |s| s.normal);
            let t0 = self.all_birds[b].time;
            let now = world.time();
            let t = mathf::inverse_lerp(t0, t0 + 0.3, now);
            let r = random.inside_unit_sphere();
            let vx = (r.x * 0.5 + n.x) * speed;
            let vy = (r.y * 0.5 + n.y) * speed;
            let vz = (r.z * 0.5 + n.z) * speed;
            let bird = &mut self.all_birds[b];
            bird.velocity_next = v3(vx, vy, vz);
            bird.velocity_prev = v3(vx, vy, vz);
            let half = v3(vx * 0.5, vy * 0.5, vz * 0.5);
            let p = bird.pos;
            let a = v3(p.x - half.x, p.y - half.y, p.z - half.z);
            let k = mathf::clamp01(1.0 - t);
            bird.pos_mid = v3(((half.x + p.x) - a.x) * k + a.x, ((p.y + half.y) - a.y) * k + a.y, ((half.z + p.z) - a.z) * k + a.z);
        }
        self.set_landing(world, log_error, b, None);
        self.all_birds[b].state_next = st::BOIDING;
        let t = world.time();
        let r = random.value();
        self.all_birds[b].last_action_time = r + t;
        self.all_birds[b].mesh1 = BirdMesh::Flap;
        let p = self.all_birds[b].pos;
        world.play_sound(8, p);
        world.play_sound(9, p);
        // (only the first take-off since the update last read them is summed)
        if self.take_off_count == 0 {
            let root = self.all_birds[b].root;
            let wp = self.transforms.position(root);
            self.take_off_count += 1;
            let tp = self.take_off_pos;
            self.take_off_pos = v3(tp.x + wp.x, tp.y + wp.y, tp.z + wp.z);
        }
    }

    /// A new bird object from the prefab under `activeBirds` (`Object.Instantiate(srcBird, activeBirds)`).
    pub fn instantiate_bird(&mut self) -> BirdId {
        let pf = self.prefab.locals;
        let node = |t: &mut Transforms, parent: TransformId, (p, q, s): (Vector3, Quaternion, Vector3)| {
            let id = t.add(Node { parent: Some(parent), ..Default::default() });
            t.set_local_position(id, p);
            t.set_local_rotation(id, q);
            t.set_local_scale(id, s);
            id
        };
        let t = &mut self.transforms;
        let root = node(t, self.active_birds, pf[0]);
        let shade = node(t, root, pf[1]);
        let body = node(t, root, pf[2]);
        let mf0 = node(t, body, pf[3]);
        let mf1 = node(t, body, pf[4]);
        self.all_birds.push(Bird {
            root,
            body,
            shade,
            mf0,
            mf1,
            mesh0: BirdMesh::None,
            mesh1: BirdMesh::None,
            mf0_active: true,
            shade_active: true,
            rotation0: Quaternion::IDENTITY,
            rotation1: Quaternion::IDENTITY,
            pos_mid: Vector3::ZERO,
            velocity_prev: Vector3::ZERO,
            velocity_next: Vector3::ZERO,
            pos: Vector3::ZERO,
            time: 0.0,
            preferred_neighbour_count: 0,
            state_prev: 0,
            state_next: 0,
            last_action_time: 0.0,
            landing: None,
        });
        self.all_birds.len() - 1
    }

    /// `BirdFlock.IterateBirdCreation` 0x7BB220: below the minimum, birds are put on the free perches round a
    /// random one (within a random 0.2..0.6) until the maximum: a disabled bird reused (its clock snapped to the
    /// 0.3 s step) or a new one (clock offset by its number, a size 0.8..1.2), placed sitting, facing away from
    /// the perch's up with a random turn, settling (stopping), rested 3..25 s, with a landing sound. False when there are enough.
    pub fn iterate_bird_creation(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str)) -> bool {
        self.calculate_optimal_landings();
        if self.min_bird_count <= self.active_birds_count {
            return false;
        }
        let r = random.range(0.2, 0.6);
        let sq = r * r;
        let Some(start) = Self::try_get_random_landing(world, random) else { return true };
        let sp = self.sort_pos(world, start);
        for (l, _) in Self::perches_near(world, sp, r + 0.1, sq) {
            if self.landings.get(&l).and_then(|s| s.bird).is_some() {
                continue;
            }
            let b = if self.transforms.child_count(self.disabled_birds) < 1 {
                let b = self.instantiate_bird();
                let now = world.time();
                self.all_birds[b].time = self.total_bird_count as f32 / 3.1415927 + now;
                self.all_birds[b].preferred_neighbour_count = (self.total_bird_count & 7) + 1;
                let s = random.range(0.8, 1.2);
                let one = v3(1.0, 1.0, 1.0);
                let root = self.all_birds[b].root;
                self.transforms.set_local_scale(root, v3(one.x * s, one.y * s, one.z * s));
                self.total_bird_count = self.total_bird_count.wrapping_add(1);
                b
            } else {
                let n = self.transforms.child_count(self.disabled_birds);
                let t = self.transforms.child(self.disabled_birds, n - 1);
                let b = self.all_birds.iter().position(|x| x.root == t).expect("a disabled bird");
                self.transforms.set_parent(t, Some(self.active_birds), true);
                b
            };
            let t0 = self.all_birds[b].time;
            let now = world.time();
            let k = mathf::floor_to_int((now - t0) / DT);
            self.all_birds[b].time = k as f32 * DT + t0;
            self.insert_sorted(b);
            self.all_birds[b].state_next = st::STOPPING;
            let now = world.time();
            let v = random.value();
            self.all_birds[b].last_action_time = v + now;
            self.set_landing(world, log_error, b, Some(l));
            let lp = self.get_landing_pos(world, b);
            self.all_birds[b].pos_mid = lp;
            self.all_birds[b].pos = lp;
            let root = self.all_birds[b].root;
            self.transforms.set_position(root, lp);
            self.all_birds[b].velocity_prev = Vector3::ZERO;
            self.all_birds[b].velocity_next = Vector3::ZERO;
            self.all_birds[b].mesh0 = BirdMesh::None;
            let v = random.value();
            self.all_birds[b].mesh1 = if v < 0.1 {
                BirdMesh::Land
            } else if random.value() < 0.5 {
                BirdMesh::HalfSit
            } else {
                BirdMesh::Sit
            };
            let shade = self.all_birds[b].shade;
            self.transforms.set_position(shade, world.landing_position(l));
            let lr = world.landing_rotation(l);
            self.transforms.set_rotation(shade, Quaternion::mul(lr, Quaternion::euler(-90.0, 90.0, 0.0)));
            let up = world.landing_up(l);
            let n = self.landings.get(&l).map_or(Vector3::ZERO, |s| s.normal);
            let flat = v3(n.x, 0.0, n.z);
            let neg = |x: f32| f32::from_bits(x.to_bits() ^ 0x8000_0000);
            let dir = v3(neg(up.x) + flat.x, (neg(up.y) + flat.y) * 0.4, flat.z + neg(up.z));
            let turn = random.range_int(-20, 20);
            let rot = Quaternion::mul(Quaternion::euler(0.0, turn as f32, 0.0), Quaternion::look_rotation(dir));
            self.all_birds[b].rotation1 = rot;
            self.all_birds[b].rotation0 = rot;
            let t = self.all_birds[b].last_action_time;
            self.all_birds[b].last_action_time = random.range(3.0, 25.0) + t;
            self.active_birds_count = self.active_birds_count.wrapping_add(1);
            if self.active_birds_count == self.max_bird_count {
                break;
            }
            world.play_sound(7, self.all_birds[b].pos);
        }
        true
    }

    /// `BirdFlock.GetLanding(bird)` 0x7BA960: the free perch ahead of a flying bird (within 1 of `posMid +
    /// velocityNext`) whose target lies within about 37° of its heading (`dot >= 0.8`), the best by `dot +
    /// 0.1 × neighbourCount` (the first of equals).
    pub fn get_landing(&self, world: &dyn BirdWorld, b: BirdId) -> Option<LandingId> {
        let bird = &self.all_birds[b];
        let (pm, vn) = (bird.pos_mid, bird.velocity_next);
        let ahead = v3(pm.x + vn.x, pm.y + vn.y, pm.z + vn.z);
        let heading = vn.normalized();
        let mut best = -f32::MAX;
        let mut found = None;
        for (l, _) in Self::perches_near(world, ahead, 1.1, 1.0) {
            let s = self.landings.get(&l).copied().unwrap_or_default();
            if s.bird.is_some() {
                continue;
            }
            let tp = self.target_pos(world, l);
            let to = v3(tp.x - pm.x, tp.y - pm.y, tp.z - pm.z).normalized();
            let dot = Vector3::dot(heading, to);
            if 0.8 <= dot {
                let score = s.neighbour_count as f32 * 0.1 + dot;
                if best < score {
                    best = score;
                    found = Some(l);
                }
            }
        }
        found
    }

    /// `BirdFlock.OnUpdate` 0x7BC490: leaving birds come back below the minimum; the orbit point moves (a new
    /// random target every 8..20 s); the x-sorted list gets one bubble pass; every bird whose 0.3 s step is
    /// due steps (one step per frame); then every bird is placed between its steps.
    pub fn on_update(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str)) {
        let mut count = self.birds.len();
        self.calculate_optimal_landings();
        if self.active_birds_count < self.min_bird_count {
            for i in 0..count {
                let b = self.birds[i];
                if self.all_birds[b].state_next == st::LEAVING {
                    self.all_birds[b].state_next = st::BOIDING;
                    let t = world.time();
                    let r = random.value();
                    self.all_birds[b].last_action_time = r + t;
                    self.active_birds_count = self.active_birds_count.wrapping_add(1);
                    if self.ideal_bird_count <= self.active_birds_count {
                        break;
                    }
                }
            }
        }
        if 0 < self.bird_landing_count {
            if self.last_orbit_time < world.time() {
                let now = world.time();
                let k = random.range_int(8, 20);
                self.last_orbit_time = k as f32 + now;
                let perch = if random.value() < 0.5 { Self::try_get_random_landing(world, random) } else { None };
                let t = match perch {
                    Some(l) => self.target_pos(world, l),
                    None => {
                        // a random point over the graph's bounds (8 cells wider), at a random height
                        let bd = world.graph_bounds();
                        let x = random.range(bd.c0.x as f32 - 8.0, bd.c1.x as f32 + 8.0);
                        let y = random.range(bd.c0.y as f32 - 8.0, bd.c1.y as f32 + 8.0);
                        let z = random.range(bd.c0.z as f32 - 8.0, bd.c1.z as f32 + 8.0);
                        let p = grid_generator::hex_to_plane(f3(x, y, z));
                        let h = random.range_int(0, world.graph_bounds_height());
                        let w = hex::plane_to_world(p, h as f32);
                        v3(w.x, w.y, w.z)
                    }
                };
                let s = random.inside_unit_sphere();
                let r = random.range(0.0, 7.0);
                self.orbit_target = v3(s.x * r + t.x, r * s.y + t.y, r * s.z + t.z);
                self.orbit_target.y = mathf::max(self.orbit_target.y, 0.4);
            }
            let d = world.delta_time();
            self.orbit_pos = Vector3::move_towards(self.orbit_pos, self.orbit_target, d * 10.0);
        }
        if 1 < count {
            let mut prev = self.birds[0];
            for i in 1..count {
                let cur = self.birds[i];
                if self.all_birds[cur].pos.x < self.all_birds[prev].pos.x {
                    self.birds[i] = prev;
                    self.birds[i - 1] = cur;
                } else {
                    prev = cur;
                }
            }
        }
        let mut i = 0;
        while i < count {
            let b = self.birds[i];
            if self.all_birds[b].time + 0.3 < world.time() && self.step_bird(world, random, log_error, i, b) == Step::Removed {
                count -= 1;
                continue;
            }
            i += 1;
        }
        let one = v3(1.0, 1.0, 1.0);
        for i in 0..count {
            let b = self.birds[i];
            let now = world.time();
            let bird = &mut self.all_birds[b];
            let (pm, vp, vn) = (bird.pos_mid, bird.velocity_prev, bird.velocity_next);
            // a quadratic Bézier through the step's mid point, from half the previous velocity behind it to
            // half the next one ahead
            let p0 = v3(pm.x - vp.x * 0.5, pm.y - vp.y * 0.5, pm.z - vp.z * 0.5);
            let p2 = v3(vn.x * 0.5 + pm.x, vn.y * 0.5 + pm.y, vn.z * 0.5 + pm.z);
            let t = mathf::inverse_lerp(bird.time, bird.time + 0.3, now);
            let u = 1.0 - t;
            let u2 = u * u;
            let k = (u + u) * t;
            let t2 = t * t;
            bird.pos = v3(u2 * p0.x + k * pm.x + t2 * p2.x, u2 * p0.y + k * pm.y + t2 * p2.y, u2 * p0.z + k * pm.z + t2 * p2.z);
            if bird.state_prev == st::STOPPED || bird.state_next == st::STOPPED {
                let mut a = if bird.state_prev == st::STOPPED { 1.0 } else { 0.0 };
                let mut c = if bird.state_next == st::STOPPED { 1.0 } else { 0.0 };
                if bird.mesh0 == BirdMesh::Stand {
                    a *= 0.7;
                }
                if bird.mesh1 == BirdMesh::Stand {
                    c *= 0.7;
                }
                let s = mathf::lerp(a, c, t);
                self.transforms.set_local_scale(bird.shade, v3(s * one.x * 0.22, s * one.y * 0.22, s * one.z * 0.22));
                bird.shade_active = true;
            } else {
                bird.shade_active = false;
            }
            self.transforms.set_position(bird.body, bird.pos);
            self.transforms.set_rotation(bird.body, Quaternion::lerp(bird.rotation0, bird.rotation1, t));
            if bird.mf0_active {
                let f0 = 1.0 - t2 * t;
                self.transforms.set_local_scale(bird.mf0, v3(one.x * f0, one.y * f0, one.z * f0));
                let f1 = 1.0 - u2 * u;
                self.transforms.set_local_scale(bird.mf1, v3(one.x * f1, one.y * f1, one.z * f1));
            }
        }
        if 0 < self.take_off_count {
            self.take_off_count = 0;
        }
    }

    /// One 0.3 s step of the bird at `birds[i]` (`OnUpdate`'s loop body).
    fn step_bird(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), i: usize, b: BirdId) -> Step {
        let bird = &mut self.all_birds[b];
        let vn = bird.velocity_next;
        bird.pos_mid = v3(vn.x + bird.pos_mid.x, vn.y + bird.pos_mid.y, vn.z + bird.pos_mid.z);
        bird.velocity_prev = vn;
        bird.time += 0.3;
        bird.state_prev = bird.state_next;
        bird.mesh0 = bird.mesh1;
        bird.rotation0 = bird.rotation1;
        let dt = world.time() - bird.last_action_time;
        let step = match bird.state_next {
            st::BOIDING | st::LEAVING => self.step_flying(world, random, log_error, i, b, dt),
            st::LANDING => self.step_landing(world, random, log_error, b),
            st::STOPPING => self.step_stopping(world, random, log_error, b),
            st::STOPPED => self.step_stopped(world, random, log_error, b, dt),
            _ => Step::Show,
        };
        if step == Step::Show {
            let bird = &mut self.all_birds[b];
            if bird.mesh0 != bird.mesh1 {
                bird.mf0_active = true;
            } else {
                bird.mf0_active = false;
                self.transforms.set_local_scale(bird.mf1, v3(1.0, 1.0, 1.0));
            }
        }
        step
    }

    /// The flying step (boiding or leaving): too many birds sends one away; towards its perch, or else on with
    /// the orbit (leaving: on level); the neighbours ahead pull it along; leaving out of sight ends it; the
    /// ground pushes it up; a perch ahead is taken; flapping when it climbs or turns.
    fn step_flying(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), i: usize, b: BirdId, dt: f32) -> Step {
        if self.all_birds[b].state_next == st::BOIDING && self.max_bird_count < self.active_birds_count {
            self.active_birds_count = self.active_birds_count.wrapping_sub(1);
            self.all_birds[b].state_next = st::LEAVING;
            let t = world.time();
            let r = random.value();
            self.all_birds[b].last_action_time = r + t;
        }
        if let Some(l) = self.all_birds[b].landing {
            if 9.0 < dt {
                self.set_landing(world, log_error, b, None);
            } else {
                let tp = self.target_pos(world, l);
                let pm = self.all_birds[b].pos_mid;
                let mut d = v3(tp.x - pm.x, tp.y - pm.y, tp.z - pm.z);
                let m = d.magnitude();
                if 1.0 < m {
                    d = v3(d.x / m, d.y / m, d.z / m);
                }
                d = v3(d.x * 2.0, d.y + d.y, d.z + d.z);
                let t = mathf::lerp(0.1, 0.8, mathf::inverse_lerp(6.0, 9.0, dt));
                let bird = &mut self.all_birds[b];
                bird.velocity_next = Vector3::lerp(bird.velocity_next, d, t);
                if m < 1.5 {
                    self.set_state(world, random, log_error, b, st::LANDING);
                    return Step::Done;
                }
            }
        } else {
            let orbit = self.orbit_pos;
            let bird = &mut self.all_birds[b];
            bird.velocity_next.y *= 0.7;
            let pm = bird.pos_mid;
            let dir = if bird.state_next == st::LEAVING {
                let vp = bird.velocity_prev;
                v3(vp.x, 0.0, vp.z)
            } else {
                v3(orbit.x - pm.x, orbit.y - pm.y, orbit.z - pm.z)
            };
            bird.velocity_next = Vector3::lerp(bird.velocity_next, dir.normalized(), 0.2);
            bird.velocity_next = bird.velocity_next.normalized();
        }
        self.boid(world, i, b);
        if self.all_birds[b].state_next == st::LEAVING {
            let pm = self.all_birds[b].pos_mid;
            // (the game asks twice)
            let mut stays = utils::in_view(world.world_to_viewport_point(pm), 0.2);
            if stays {
                let g = world.graph_bounds();
                let wide = Int3x2 {
                    c0: Int3 { x: g.c0.x.wrapping_sub(400), y: g.c0.y.wrapping_sub(400), z: g.c0.z.wrapping_sub(400) },
                    c1: Int3 { x: g.c1.x.wrapping_add(400), y: g.c1.y.wrapping_add(400), z: g.c1.z.wrapping_add(400) },
                };
                stays = extra_math::contains_f(wide, grid_generator::plane_to_hex(hex::world_to_plane(f3(pm.x, pm.y, pm.z))));
            }
            if !stays {
                let root = self.all_birds[b].root;
                self.transforms.set_parent(root, Some(self.disabled_birds), true);
                self.birds.remove(i);
                return Step::Removed;
            }
        }
        // the ground: the first point along twice the velocity (in steps of at most 0.5) the graph covers pushes up
        let vn = self.all_birds[b].velocity_next;
        let m = vn.magnitude();
        if 0.0 < m {
            let n = mathf::ceil_to_int(m * 2.0) + 1;
            let step = 1.0 / n as f32;
            for k in 1..=n {
                let f = k as f32 * step;
                let a = v3(vn.x * f, vn.y * f, vn.z * f);
                let pm = self.all_birds[b].pos_mid;
                let p = v3(a.x + a.x + pm.x, a.y + a.y + pm.y, a.z + a.z + pm.z);
                let (normal, coverage) = world.sample_normal_coverage(p);
                if 0.0 < coverage {
                    let nn = normal.normalized();
                    let c2 = coverage + coverage;
                    self.all_birds[b].velocity_next = v3(vn.x + nn.x * c2, vn.y + nn.y * c2, vn.z + nn.z * c2);
                    break;
                }
            }
        }
        if self.all_birds[b].state_next == st::BOIDING && 8.0 < dt {
            if let Some(l) = self.get_landing(world, b) {
                self.set_landing(world, log_error, b, Some(l));
                self.set_state(world, random, log_error, b, st::LANDING);
                let tp = self.target_pos(world, l);
                let bird = &mut self.all_birds[b];
                let pm = bird.pos_mid;
                let d = extra_math::clamped_magnitude(v3(tp.x - pm.x, tp.y - pm.y, tp.z - pm.z), 0.7);
                bird.velocity_next = Vector3::move_towards(bird.velocity_next, d, 0.5);
            }
        }
        let bird = &mut self.all_birds[b];
        let (vp, vn) = (bird.velocity_prev, bird.velocity_next);
        let up = v3((vn.x - vp.x) + 0.0, (vn.y - vp.y) + 0.5, (vn.z - vp.z) + 0.0);
        let c = (up.magnitude() - vn.magnitude()) + 0.2;
        let t = mathf::inverse_lerp(1.0, 0.7, c);
        bird.mesh1 = if 0.6 < c { BirdMesh::Flap } else { BirdMesh::Fly };
        bird.rotation1 = Quaternion::look_rotation_up(v3(vn.x, t * vn.y, vn.z), up);
        Step::Show
    }

    /// The neighbours' pull on the bird at `birds[i]`: up to 7 flying birds within 2 of its position, met
    /// alternately ahead and behind in the x-sorted list (a bird behind ends that side unless it is 2 or more
    /// ahead in x, so in practice only those ahead count). Each, weighted by `w = 1 - d/4`: pushed away when
    /// closer than 1.6 and pulled in beyond (`w - 0.6`), and its current velocity matched.
    fn boid(&mut self, world: &dyn BirdWorld, i: usize, b: BirdId) {
        let count = self.birds.len();
        let me = self.all_birds[b].pos;
        let mut sum = Vector3::ZERO;
        let mut weight = 0.0f32;
        let mut n = 0;
        let (mut ahead, mut behind) = (i + 1, i as i32 - 1);
        let (mut ahead_left, mut behind_left) = (ahead < count, 0 <= behind);
        let mut forward = true;
        while ahead_left || behind_left {
            let this_forward = forward;
            forward = !forward;
            let idx = if this_forward {
                if !ahead_left {
                    continue;
                }
                ahead += 1;
                ahead_left = ahead < count;
                ahead - 1
            } else {
                if !behind_left {
                    continue;
                }
                behind -= 1;
                behind_left = 0 <= behind;
                (behind + 1) as usize
            };
            let o = self.all_birds[self.birds[idx]];
            let d = v3(o.pos.x - me.x, o.pos.y - me.y, o.pos.z - me.z);
            if this_forward {
                if 2.0 < d.x {
                    ahead_left = false;
                    continue;
                }
            } else if 2.0 > d.x {
                behind_left = false;
                continue;
            }
            if o.state_next != st::BOIDING && o.state_next != st::LEAVING {
                continue;
            }
            let sq = d.sqr_magnitude();
            if sq >= 4.0 {
                continue;
            }
            let dist = sqrt_f64(sq);
            let w = 1.0 - dist * 0.25;
            let w2 = w * w;
            let k = w - 0.6;
            sum = v3(sum.x - d.x / dist * k * w2 * w, sum.y - d.y / dist * k * w2 * w, sum.z - d.z / dist * k * w2 * w);
            let tt = mathf::inverse_lerp(o.time, o.time + 0.3, world.time());
            let ov = Vector3::lerp(o.velocity_prev, o.velocity_next, tt);
            let vn = self.all_birds[b].velocity_next;
            sum = v3((ov.x - vn.x) * w2 * w + sum.x, (ov.y - vn.y) * w2 * w + sum.y, (ov.z - vn.z) * w2 * w + sum.z);
            weight = w2 + weight;
            n += 1;
            if n > 6 {
                break;
            }
        }
        if 0.0 < weight {
            let inv = 1.0 / weight;
            let bird = &mut self.all_birds[b];
            let vn = bird.velocity_next;
            bird.velocity_next = v3(sum.x * inv + vn.x, sum.y * inv + vn.y, sum.z * inv + vn.z);
        }
    }

    /// The facing a bird settles to on its perch: away from the perch's up, levelled (`flat_y` of the perch's
    /// up kept), along the normal's level part (`normal_weight`).
    fn perch_look(&self, world: &dyn BirdWorld, l: LandingId, normal_weight: f32, flat_y: f32) -> Vector3 {
        let up = world.landing_up(l);
        let n = self.landings.get(&l).map_or(Vector3::ZERO, |s| s.normal);
        let sy = v3(n.x, 0.0, n.z);
        v3(sy.x * normal_weight + -up.x, (sy.y * normal_weight + -up.y) * flat_y, sy.z * normal_weight + -up.z)
    }

    /// The landing step: towards the perch (closing on its target point, turning to the perch's facing);
    /// within 0.3 it lands (stopping).
    fn step_landing(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), b: BirdId) -> Step {
        let lp = self.get_landing_pos(world, b);
        let pm = self.all_birds[b].pos_mid;
        let d = v3(lp.x - pm.x, lp.y - pm.y, lp.z - pm.z);
        let m = d.magnitude();
        self.all_birds[b].mesh1 = BirdMesh::Land;
        let l = self.all_birds[b].landing.expect("landing without a perch");
        let q = Quaternion::look_rotation(self.perch_look(world, l, 0.7, 0.0));
        if m < 0.3 {
            let bird = &mut self.all_birds[b];
            bird.velocity_next = d;
            bird.rotation1 = Quaternion::lerp(bird.rotation0, q, 0.3);
            self.set_state(world, random, log_error, b, st::STOPPING);
        } else {
            let tp = self.target_pos(world, l);
            let v = Vector3::lerp(lp, tp, m + 0.1);
            let bird = &mut self.all_birds[b];
            let dd = extra_math::clamped_magnitude(v3(v.x - pm.x, v.y - pm.y, v.z - pm.z), 0.7);
            bird.velocity_next = Vector3::move_towards(bird.velocity_next, dd, 0.8);
            let (vp, vn) = (bird.velocity_prev, bird.velocity_next);
            let up = v3((vn.x - vp.x) + 0.0, (vn.y - vp.y) + 0.5, (vn.z - vp.z) + 0.0);
            let r = Quaternion::look_rotation_up(vn, up);
            bird.rotation1 = Quaternion::lerp(r, q, 0.1);
        }
        Step::Show
    }

    /// The stopping step: still, stopped, half-sitting; its shade on the perch; facing off the perch with a
    /// random turn of up to 20°.
    fn step_stopping(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), b: BirdId) -> Step {
        self.all_birds[b].velocity_next = Vector3::ZERO;
        self.set_state(world, random, log_error, b, st::STOPPED);
        self.all_birds[b].mesh1 = BirdMesh::HalfSit;
        let l = self.all_birds[b].landing.expect("stopping without a perch");
        let shade = self.all_birds[b].shade;
        self.transforms.set_position(shade, world.landing_position(l));
        let lr = world.landing_rotation(l);
        self.transforms.set_rotation(shade, Quaternion::mul(lr, Quaternion::euler(-90.0, 90.0, 0.0)));
        let look = self.perch_look(world, l, 1.0, 0.4);
        let turn = random.range_int(-20, 20);
        self.all_birds[b].rotation1 = Quaternion::mul(Quaternion::euler(0.0, turn as f32, 0.0), Quaternion::look_rotation(look));
        Step::Show
    }

    /// The stopped step: still; past 3 s since its last action, it acts: 10% hop to the best free perch within
    /// 0.8 (by neighbours × 0.3 - squared distance + a random 0.2, against staying), 27% change pose (on a
    /// level perch: stand up, a little higher, or sit; else sit or half-sit), else turn (up to 40° on a level
    /// perch, 20° else).
    fn step_stopped(&mut self, world: &mut dyn BirdWorld, random: &mut Rand, log_error: &mut dyn FnMut(&str), b: BirdId, dt: f32) -> Step {
        self.all_birds[b].velocity_next = Vector3::ZERO;
        if !(3.0 < dt) {
            return Step::Show;
        }
        self.all_birds[b].last_action_time = world.time();
        let l = self.all_birds[b].landing.expect("stopped without a perch");
        let s = self.landings.get(&l).copied().unwrap_or_default();
        let level = 0.95 < s.normal.y;
        if random.value() < 0.1 {
            let r = random.value();
            let mut best = s.neighbour_count as f32 * 0.3 + r * 0.2;
            let mut found = None;
            let here = world.landing_position(l);
            for (c, p) in Self::perches_near(world, here, 0.90000004, 0.64000005) {
                let cs = self.landings.get(&c).copied().unwrap_or_default();
                if cs.bird.is_some() {
                    continue;
                }
                let sq = v3(here.x - p.x, here.y - p.y, here.z - p.z).sqr_magnitude();
                let r = random.value();
                let score = (cs.neighbour_count as f32 * 0.3 - sq) + r * 0.2;
                if best <= score {
                    best = score;
                    found = Some(c);
                }
            }
            if let Some(c) = found {
                self.set_landing(world, log_error, b, None);
                self.set_landing(world, log_error, b, Some(c));
                let lp = self.get_landing_pos(world, b);
                let bird = &mut self.all_birds[b];
                let pm = bird.pos_mid;
                bird.velocity_next = v3(lp.x - pm.x, lp.y - pm.y, lp.z - pm.z);
                bird.mesh1 = BirdMesh::Land;
                // facing the hop's level part
                let vn = bird.velocity_next;
                bird.rotation1 = Quaternion::look_rotation(v3(vn.x, 0.0, vn.z));
                self.set_state(world, random, log_error, b, st::STOPPING);
            }
        } else if random.value() < 0.3 {
            if !level {
                self.all_birds[b].mesh1 = if 0.5 <= random.value() { BirdMesh::Sit } else { BirdMesh::HalfSit };
            } else {
                let lp = self.get_landing_pos(world, b);
                let mesh = if 0.5 <= random.value() { BirdMesh::Sit } else { BirdMesh::Stand };
                self.all_birds[b].mesh1 = mesh;
                let y = if mesh == BirdMesh::Stand { self.transforms.local_scale(self.all_birds[b].root).y * 0.01 + lp.y } else { lp.y };
                let bird = &mut self.all_birds[b];
                let pm = bird.pos_mid;
                bird.velocity_next = v3(lp.x - pm.x, y - pm.y, lp.z - pm.z);
            }
        } else {
            let look = self.perch_look(world, l, 1.0, 0.4);
            let a = if level { 40.0 } else { 20.0 };
            let turn = random.range(-a, a);
            self.all_birds[b].rotation1 = Quaternion::mul(Quaternion::euler(0.0, turn, 0.0), Quaternion::look_rotation(look));
        }
        Step::Show
    }

    /// `birds.Add(bird)`, as the list's order is kept by the update's sort (G5b): appended.
    fn insert_sorted(&mut self, b: BirdId) {
        self.birds.push(b);
    }

    /// `BirdFlock.OnReset` 0x7BC360: every bird (from the last) glides, leaves its perch and goes to the
    /// disabled ones; none active, the counts recomputed.
    pub fn on_reset(&mut self, world: &mut dyn BirdWorld, log_error: &mut dyn FnMut(&str)) {
        for i in (0..self.birds.len()).rev() {
            let b = self.birds[i];
            self.all_birds[b].mesh1 = BirdMesh::Glide;
            self.all_birds[b].mesh0 = BirdMesh::Glide;
            self.set_landing(world, log_error, b, None);
            let root = self.all_birds[b].root;
            self.transforms.set_parent(root, Some(self.disabled_birds), true);
            self.all_birds[b].state_prev = st::LEAVING;
            self.all_birds[b].state_next = st::LEAVING;
        }
        self.birds.clear();
        self.active_birds_count = 0;
        self.optimal_bird_count_dirty = true;
        self.calculate_optimal_landings();
    }
}
