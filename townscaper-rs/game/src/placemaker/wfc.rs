//! `Placemaker.WaveFunctionCollapse` (row E6): which modules fit each qube, narrowed by its neighbours until one
//! is left. Row E6a: the queues' records and order, the per-qube work (`FillQubePossibilities`,
//! `RemovePossibleModule`) and the small entry points. The qubes live in the [`Graph`]; the module data in the
//! [`ModuleLibrary`]; the module builder and the log behind [`WfcHost`]. Checked against the running game by
//! `tests/placemaker_wfc.rs` (`TS_HARNESS=1`).

use crate::os::utils;
use crate::placemaker::graphs::{single_compare_to, Graph, OrientedModuleSides, QubeId, QubeRelation6};
use crate::placemaker::modules::{ModuleLibrary, ModuleMath};
use crate::unity::collections;
use crate::unity::mathematics as math;

/// What the wave function collapse calls outside the graph.
pub trait WfcHost {
    /// `ModuleBuilder.ApplyModule(qube, module)` (row E7): build the qube's one remaining module.
    fn apply_module(&mut self, _graph: &mut Graph, _qube: QubeId, _module: OrientedModuleSides) {}
    /// `Debug.LogError(message, qube)`.
    fn log_error(&mut self, _message: &str, _qube: QubeId) {}
}

/// `WaveFunctionCollapse.Propagation` (16 bytes): a qube queued at a propagation step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Propagation {
    pub qube: Option<QubeId>,
    pub propagation_index: u16,
    pub existance_index: u32,
    pub side_variation_mask: u8,
}

impl Propagation {
    /// `WaveFunctionCollapse.Propagation.System.IComparable<Propagation>.CompareTo` 0x4FC880: by propagation
    /// index, then the qube's world y, its cost (higher first), voxel count, material count (more first), and
    /// last `math.sin(z + x)` of the position. A missing qube on either side returns 0 past the first step.
    pub fn compare_to(&self, other: &Propagation, graph: &Graph) -> i32 {
        let c = self.propagation_index as i32 - other.propagation_index as i32;
        if c != 0 {
            return c;
        }
        let (Some(a), Some(b)) = (self.qube, other.qube) else { return 0 };
        let (pa, pb) = (graph.qube_position(a), graph.qube_position(b));
        let c = single_compare_to(pa.y, pb.y);
        if c != 0 {
            return c;
        }
        let (qa, qb) = (graph.qube(a), graph.qube(b));
        let c = single_compare_to(qb.cost, qa.cost);
        if c != 0 {
            return c;
        }
        let c = qa.voxel_count as i32 - qb.voxel_count as i32;
        if c != 0 {
            return c;
        }
        let mb = ModuleMath::get_material_count(crate::placemaker::modules::ByteQube { v: qb.corner_values });
        let ma = ModuleMath::get_material_count(crate::placemaker::modules::ByteQube { v: qa.corner_values });
        let c = mb.cmp(&ma) as i32;
        if c != 0 {
            return c;
        }
        single_compare_to(math::sin(pa.z + pa.x), math::sin(pb.z + pb.x))
    }
}

/// `WaveFunctionCollapse.LastRemovedStruct` (12 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LastRemovedStruct {
    pub qube: Option<QubeId>,
    pub side: u16,
    pub side_index: i8,
    pub propagation_index: u16,
}

/// `Placemaker.WaveFunctionCollapse`'s state (`..ctor` 0x3DA780: every list new and empty). `master` is passed
/// in where used (the graph, the library and the host).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaveFunctionCollapse {
    pub propagation_counter: u16,
    pub last_started_propagation: u16,
    pub remove_neighbors: Vec<Propagation>,
    pub updates: Vec<Propagation>,
    pub propagations: Vec<Propagation>,
    pub validations: Vec<Propagation>,
    pub to_collapse: Vec<Propagation>,
    pub decor_updates: Vec<Propagation>,
    pub decor_propagations: Vec<Propagation>,
    pub decor_to_collapse: Vec<Propagation>,
    pub decor_to_build: Vec<Propagation>,
    pub last_removals: Vec<LastRemovedStruct>,
    pub last_decor_removals: Vec<LastRemovedStruct>,
    pub qubes_to_rebuild: Vec<QubeId>,
    /// `hashset` (a `HashSet<int>`; used by the loops, row E6b).
    pub hashset: Vec<i32>,
    pub do_debug_break: bool,
}

/// `List<Propagation>.BinarySearch(p)` then `Insert` at the match or the complement: the sorted queues.
pub fn insert_sorted(list: &mut Vec<Propagation>, p: Propagation, graph: &Graph) -> usize {
    let i = collections::binary_search(list, 0, list.len(), &p, &mut |a, b| a.compare_to(b, graph));
    let i = if i < 0 { !i } else { i } as usize;
    list.insert(i, p);
    i
}

/// `|f| fmod 1` as the game computes a cost jitter: the float's absolute value widened to double, `fmod(x, 1)`,
/// back to float.
fn frac_abs(f: f32) -> f32 {
    ((f.abs() as f64) % 1.0) as f32
}

impl WaveFunctionCollapse {
    /// `WaveFunctionCollapse.QubeUpdated` 0x3D9ED0: queue the qube for a fresh fill at this propagation step.
    pub fn qube_updated(&mut self, graph: &mut Graph, qube: QubeId) {
        let q = graph.qube_mut(qube);
        q.last_propagation = self.propagation_counter;
        let existance_index = q.existance_index;
        self.updates.push(Propagation { qube: Some(qube), propagation_index: self.propagation_counter, existance_index, side_variation_mask: 0 });
        // `if (doDebugBreak) Debug.Break()`: editor only
    }

    /// `WaveFunctionCollapse.QubeRemoved` 0x3D9D10: the qube's modules are dropped and each neighbour on a side in
    /// `side_mask & sideVariationMask` (all four round sides for a bottom qube) that isn't fully covered is
    /// queued in `removeNeighbors`, its mask without the shared side.
    pub fn qube_removed(&mut self, graph: &mut Graph, qube: QubeId, side_mask: u8) {
        let q = graph.qube_mut(qube);
        q.awaiting_rebuilding = false;
        q.awaiting_grass = false;
        q.last_propagation = 0;
        q.possible_modules.clear();
        let mut mask = side_mask & q.side_variation_mask;
        if graph.qube_sibling_index(qube) == 0 {
            mask |= 0xF;
        }
        let rel = graph.get_qube_relation6(qube, mask);
        for s in 0..6 {
            let r = rel.get(s);
            if r.index_in_other as u8 == 0xFF {
                continue;
            }
            let Some(other) = r.qube else { break };
            let o = graph.qube_mut(other);
            if o.fully_covered() {
                continue;
            }
            o.last_propagation = self.propagation_counter;
            let existance_index = o.existance_index;
            self.remove_neighbors.push(Propagation {
                qube: Some(other),
                propagation_index: self.propagation_counter,
                existance_index,
                side_variation_mask: !(1u8 << (r.index_in_other as u32 & 31)),
            });
        }
        graph.qube_mut(qube).side_variation_mask = 0;
    }

    /// `WaveFunctionCollapse.IteratePropagationCount` 0x3D9160: a new step once the last one has started.
    pub fn iterate_propagation_count(&mut self) {
        if self.propagation_counter == self.last_started_propagation {
            self.propagation_counter = self.propagation_counter.wrapping_add(4);
        }
    }

    /// `WaveFunctionCollapse.ResetPropagationCounter` 0x3DA690: past 0x7FFF every qube of every square goes back
    /// to step 0, and so do both counters.
    pub fn reset_propagation_counter(&mut self, graph: &mut Graph) {
        if self.propagation_counter <= 0x7FFF {
            return;
        }
        for &s in graph.squares.clone().iter().rev() {
            for &q in graph.square(s).qubes.clone().iter().rev() {
                graph.qube_mut(q).last_propagation = 0;
            }
        }
        self.propagation_counter = 0;
        self.last_started_propagation = 0;
    }

    /// `WaveFunctionCollapse.MaybeQueueQubeForRebuilding` 0x3D98A0: a qube with one module left is built at once;
    /// any other qube with voxels is queued once in `qubesToRebuild` (true when queued).
    pub fn maybe_queue_qube_for_rebuilding(&mut self, graph: &mut Graph, host: &mut dyn WfcHost, qube: QubeId) -> bool {
        let q = graph.qube(qube);
        if q.voxel_count == 0 {
            return false;
        }
        if q.possible_modules.len() == 1 {
            let m = q.possible_modules[0];
            host.apply_module(graph, qube, m);
            graph.qube_mut(qube).awaiting_rebuilding = false;
            return false;
        }
        if q.awaiting_rebuilding {
            return false;
        }
        self.qubes_to_rebuild.push(qube);
        graph.qube_mut(qube).awaiting_rebuilding = true;
        true
    }

    /// `WaveFunctionCollapse.RemovePossibleModule` 0x3DA190: drops module `index` of the qube unless it is the
    /// last one ("No Modules" logged, false). For each neighbour in `relations` that isn't fully covered, whose
    /// side variation mask has the shared side and whose propagation step is not behind the qube's: if no module
    /// left in the qube shows the removed module's profile on that side, the neighbour is noted in
    /// `lastRemovals` and the result is true. With one module left it is built.
    pub fn remove_possible_module(
        &mut self,
        graph: &mut Graph,
        host: &mut dyn WfcHost,
        qube: QubeId,
        possible_module: OrientedModuleSides,
        index: i32,
        relations: QubeRelation6,
    ) -> bool {
        let n = graph.qube(qube).possible_modules.len();
        if n == 1 {
            // also `Debug.DrawRay` (editor only)
            host.log_error("No Modules", qube);
            return false;
        }
        let pm = &mut graph.qube_mut(qube).possible_modules;
        if index < 0 || index as usize >= pm.len() {
            panic!("ArgumentOutOfRangeException: RemoveAt({index})");
        }
        pm.remove(index as usize);
        let mut removed = false;
        for s in 0..6i32 {
            let r = relations.get(s);
            if r.index_in_other == -1 {
                continue;
            }
            let other = r.qube.expect("WaveFunctionCollapse.RemovePossibleModule: NullReferenceException");
            let o = graph.qube(other);
            if o.fully_covered() {
                continue;
            }
            if !utils::is_bit_set_u8(o.side_variation_mask, r.index_in_other as i32) {
                continue;
            }
            let q = graph.qube(qube);
            if o.last_propagation < q.last_propagation {
                continue;
            }
            let v = possible_module.sides.get0(s);
            if q.possible_modules.iter().rev().any(|m| m.sides.get0(s) == v) {
                continue;
            }
            self.last_removals.push(LastRemovedStruct {
                qube: Some(other),
                side: v,
                side_index: r.index_in_other,
                propagation_index: q.last_propagation,
            });
            // also `Debug.DrawLine` (editor only)
            removed = true;
        }
        if n == 2 {
            let m = graph.qube(qube).possible_modules[0];
            host.apply_module(graph, qube, m);
            graph.qube_mut(qube).awaiting_rebuilding = false;
        }
        removed
    }

    /// `WaveFunctionCollapse.FillQubePossibilities` 0x3D6DD0: the qube's modules from its corner lookup, each
    /// with its sides and a cost (`module.cost` + a position-based jitter), kept sorted by cost; a module whose
    /// side profile repeats the previous one only replaces it when cheaper. A module must fit every open side
    /// (nothing beyond the graph: profile 0, or for a bottom qube a water side round it, 1 below, 0 above), a
    /// fully covered neighbour (profile 1) and a neighbour at the same or a later step (one of its modules shows
    /// the profile, or profile 0/1 when it has none). Then the neighbours' modules that no longer fit any of the
    /// qube's are removed ("Validate Opposite Modules"). Returns the side variation mask: the sides whose profile
    /// differs between consecutive modules.
    pub fn fill_qube_possibilities(&mut self, graph: &mut Graph, lib: &ModuleLibrary, host: &mut dyn WfcHost, qube: QubeId) -> u8 {
        graph.qube_mut(qube).possible_modules.clear();
        if graph.qube(qube).empty() {
            return 0;
        }
        let list_index = graph.qube(qube).possible_module_list_index;
        let lookup = lib
            .corner_lookups
            .get(usize::try_from(list_index).unwrap_or(usize::MAX))
            .unwrap_or_else(|| panic!("ArgumentOutOfRangeException: cornerLookups[{list_index}]"));
        let rel = graph.get_qube_relation6(qube, 0xFF);
        let bottom = graph.qube_sibling_index(qube) == 0;
        let pos = graph.qube_position(qube);
        let mut mask = 0u8;
        let mut prev = crate::placemaker::modules::SideProfile::default();
        let mut first_done = false;
        let mut last_index = -1i32;
        let mut valid = false;
        let mut last_side_index = 0xFFFFu32;
        let mut last_cost = 0.0f32;
        let mut count = 0i32;
        graph.qube_mut(qube).cost = f32::MAX;
        for (i, &om) in lookup.oriented_modules.iter().enumerate() {
            let module = &lib.modules[om.module_index as usize];
            let side_index = module.base.profile_indexes[om.orientation() as usize] as u32;
            let jitter = frac_abs((((i as i32) as f32 + pos.x + pos.z) + pos.y) * std::f32::consts::PI * 17.0);
            let cost = module.base.cost as f32 + jitter;
            if last_side_index == side_index {
                if valid && cost < last_cost {
                    let sides = lib.side_lookups[side_index as usize];
                    graph.qube_mut(qube).possible_modules[last_index as usize] = OrientedModuleSides { sides, oriented_module: om, cost };
                    last_cost = cost;
                }
                continue;
            }
            let sides = lib.side_lookups[side_index as usize];
            valid = true;
            let mut checked_all = true;
            for s in 0..6i32 {
                let r = rel.get(s);
                let v = sides.get0(s);
                if r.index_in_other == -1 {
                    let ok = if !bottom {
                        v == 0
                    } else if s < 4 {
                        lib.water_sides.contains(&(v as u32))
                    } else if s == 4 {
                        v == 1
                    } else {
                        v == 0
                    };
                    if !ok {
                        valid = false;
                        checked_all = false;
                        break;
                    }
                    continue;
                }
                let other = r.qube.expect("WaveFunctionCollapse.FillQubePossibilities: NullReferenceException");
                let o = graph.qube(other);
                if o.fully_covered() {
                    if v != 1 {
                        valid = false;
                        checked_all = false;
                        break;
                    }
                } else if valid && graph.qube(qube).last_propagation <= o.last_propagation {
                    if o.possible_modules.is_empty() {
                        valid = v < 2;
                    } else {
                        valid = o.possible_modules.iter().rev().any(|m| m.sides.get1(r.index_in_other as i32) == v);
                    }
                }
            }
            if checked_all {
                if !first_done {
                    first_done = true;
                } else {
                    let pairs = [(sides.v00, prev.v00), (sides.v10, prev.v10), (sides.v20, prev.v20), (sides.v30, prev.v30), (sides.v4, prev.v4), (sides.v5, prev.v5)];
                    for (k, (a, b)) in pairs.iter().enumerate() {
                        if a != b {
                            mask |= 1 << k;
                        }
                    }
                }
                if cost < graph.qube(qube).cost {
                    graph.qube_mut(qube).cost = module.base.cost as f32;
                }
            }
            prev = sides;
            last_side_index = side_index;
            if valid {
                let m = OrientedModuleSides { sides, oriented_module: om, cost };
                let pm = &mut graph.qube_mut(qube).possible_modules;
                let at = collections::binary_search(pm, 0, pm.len(), &m, &mut |a, b| a.compare_to(b));
                let at = if at < 0 { !at } else { at };
                pm.insert(at as usize, m);
                last_cost = cost;
                count += 1;
                last_index = at;
            }
        }
        let min_cost = graph.qube(qube).cost;
        let q = graph.qube_mut(qube);
        q.side_variation_mask = mask;
        q.cost = frac_abs(((pos.z + pos.x) + pos.y) * std::f32::consts::PI * 7.0) + min_cost;

        // "Validate Opposite Modules" (a profiler marker)
        for s in 0..6i32 {
            let r = rel.get(s);
            if r.index_in_other == -1 {
                continue;
            }
            let other = r.qube.expect("WaveFunctionCollapse.FillQubePossibilities: NullReferenceException");
            if graph.qube(other).last_propagation < graph.qube(qube).last_propagation {
                continue;
            }
            let mut j = graph.qube(other).possible_modules.len() as i32;
            loop {
                j -= 1;
                if j < 0 {
                    break;
                }
                let Some(&pm) = graph.qube(other).possible_modules.get(j as usize) else {
                    panic!("ArgumentOutOfRangeException: possibleModules[{j}]")
                };
                let v = pm.sides.get1(r.index_in_other as i32);
                let fits = if count < 1 {
                    v < 2
                } else {
                    graph.qube(qube).possible_modules[..count as usize].iter().rev().any(|m| m.sides.get0(s) == v)
                };
                if !fits {
                    let m = graph.qube(other).side_variation_mask & !(1u8 << (r.index_in_other as u32 & 31));
                    let rel_other = graph.get_qube_relation6(other, m);
                    self.remove_possible_module(graph, host, other, pm, j, rel_other);
                }
            }
        }
        if graph.qube(qube).possible_modules.is_empty() {
            host.log_error("No inital modules", qube);
        }
        mask
    }
}
