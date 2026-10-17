//! `Placemaker.WaveFunctionCollapse` (row E6): which modules fit each qube, narrowed by its neighbours until one
//! is left. Row E6a: the queues' records and order, the per-qube work (`FillQubePossibilities`,
//! `RemovePossibleModule`) and the small entry points; row E6b: the loops (`Iterate0`, `Iterate1`,
//! `IterateApplyModules`, `LaterUpdate`); row E6c: the decor (`FillDecorPossibilities`,
//! `RemovePossibleDecor`, `IterateDecor`). The qubes live in the [`Graph`]; the module data in the
//! [`ModuleLibrary`]; the module builder and the log behind [`WfcHost`]. Checked against the running game by
//! `tests/placemaker_wfc.rs` (`TS_HARNESS=1`).

use crate::os::utils;
use crate::placemaker::graphs::{single_compare_to, Graph, OrientedModuleSides, QubeId, QubeRelation6};
use crate::placemaker::modules::{ByteQube, ModuleLibrary, ModuleMath, OrientedModule};
use crate::unity::collections;
use crate::unity::mathematics as math;

/// What the wave function collapse calls outside the graph.
pub trait WfcHost {
    /// `ModuleBuilder.ApplyModule(qube, module)` (row E7): build the qube's one remaining module.
    fn apply_module(&mut self, _graph: &mut Graph, _qube: QubeId, _module: OrientedModuleSides) {}
    /// `Debug.LogError(message, qube)`.
    fn log_error(&mut self, _message: &str, _qube: QubeId) {}
    /// `ModuleBuilder.MarkModuleContainerForDeconstruction` on each module container under the qube (row E7).
    fn mark_module_containers_for_deconstruction(&mut self, _graph: &mut Graph, _qube: QubeId) {}
    /// The module of the container under the qube (its first child's `ModuleContainer.orientedModule`), if it
    /// has one (row E7).
    fn current_module(&mut self, _graph: &Graph, _qube: QubeId) -> Option<OrientedModule> {
        None
    }
    /// `ModuleBuilder.ApplyDecor(qube)` (row E7): build the qube's decor.
    fn apply_decor(&mut self, _graph: &mut Graph, _qube: QubeId) {}
    /// `WorldMaster.KeepGoing`: false once the frame's time budget is spent.
    fn keep_going(&mut self) -> bool {
        true
    }
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
        let mb = ModuleMath::get_material_count(ByteQube { v: qb.corner_values });
        let ma = ModuleMath::get_material_count(ByteQube { v: qa.corner_values });
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

    /// `WaveFunctionCollapse.Iterate0` 0x3D7A00: one step of the start of a propagation. A new step first only
    /// starts (`lastStartedPropagation = propagationCounter`). Then the last of `updates`: its qube (still at
    /// the queued step and existance) gets its corner lookup (`cornerDict` of its corner values with the hidden
    /// corners made ambiguous, indexed; -1 when not found or all "any"), loses its module containers when empty
    /// or fully covered (a fully covered qube also its modules, and stops there), is filled at the next step and
    /// offered to `MaybeQueueQubeForRebuilding`; its propagation's mask is the fill's, plus for a qube that had
    /// modules the old mask and the sides whose first module's profile changed, or all four round sides for a
    /// bottom qube that had none. Else the last of `removeNeighbors`: its qube is filled at the next step and
    /// the mask narrowed by the fill's. Either way the propagation goes to `propagations`. False when both are
    /// empty.
    pub fn iterate0(&mut self, graph: &mut Graph, lib: &ModuleLibrary, host: &mut dyn WfcHost) -> bool {
        if self.propagation_counter != self.last_started_propagation {
            self.last_started_propagation = self.propagation_counter;
            return true;
        }
        let from_updates = !self.updates.is_empty();
        let list = if from_updates { &mut self.updates } else { &mut self.remove_neighbors };
        let Some(mut p) = list.pop() else { return false };
        let qube = p.qube.expect("WaveFunctionCollapse.Iterate0: NullReferenceException");
        let q = graph.qube(qube);
        if p.propagation_index != q.last_propagation || p.existance_index != q.existance_index {
            return true;
        }
        if !from_updates {
            p.propagation_index = p.propagation_index.wrapping_add(1);
            let q = graph.qube_mut(qube);
            q.last_propagation = q.last_propagation.wrapping_add(1);
            let m = self.fill_qube_possibilities(graph, lib, host, qube);
            p.side_variation_mask &= m;
            self.propagations.push(p);
            return true;
        }
        let cv = ByteQube { v: q.corner_values };
        let index = if cv != ModuleMath::get().any_byte_qube {
            let key = ModuleMath::get_indexed_materials(ModuleMath::mark_hidden_corners_ambigous(cv));
            lib.corner_dict.get(&key).copied().unwrap_or(-1)
        } else {
            -1
        };
        graph.qube_mut(qube).possible_module_list_index = index;
        let q = graph.qube(qube);
        if q.voxel_count == 0 || q.fully_covered() {
            host.mark_module_containers_for_deconstruction(graph, qube);
        }
        if graph.qube(qube).fully_covered() {
            graph.qube_mut(qube).possible_modules.clear();
            return true;
        }
        let q = graph.qube(qube);
        let had = q.possible_modules.len();
        let first = q.possible_modules.first().copied().unwrap_or_default();
        let old_mask = q.side_variation_mask;
        graph.qube_mut(qube).last_propagation = self.propagation_counter.wrapping_add(1);
        let mut mask = self.fill_qube_possibilities(graph, lib, host, qube);
        self.maybe_queue_qube_for_rebuilding(graph, host, qube);
        if had < 1 {
            if graph.qube_sibling_index(qube) == 0 {
                mask |= 0xF;
            }
        } else if let Some(&now) = graph.qube(qube).possible_modules.first() {
            mask |= old_mask;
            for s in 0..6 {
                if !utils::is_bit_set_u8(mask, s) && now.sides.get0(s) != first.sides.get0(s) {
                    utils::set_bit(&mut mask, s, true);
                }
            }
        }
        let q = graph.qube(qube);
        self.propagations.push(Propagation { qube: Some(qube), propagation_index: q.last_propagation, existance_index: q.existance_index, side_variation_mask: mask });
        true
    }

    /// `WaveFunctionCollapse.Iterate1` 0x3D7F50: one step of a propagation. First the last of `lastRemovals`:
    /// on its qube (still at that step), every module showing the removed profile on that side is removed, with
    /// the relations of all but that side. Else the last of `propagations`: each neighbour (by its mask) that
    /// isn't fully covered and is behind the qube's step moves to it, is filled and queued in `propagations`
    /// (the fill's mask without the shared side); the propagation, with the qube's mask now, goes into
    /// `toCollapse` by its order. Else the last of `toCollapse`: a qube with one module is built and handed to
    /// the decor (`decorUpdates`, next step); with more, its dearest modules are removed one by one until a
    /// removal reaches a neighbour (then `MaybeQueueQubeForRebuilding`) or one is left. False when all three are
    /// empty.
    pub fn iterate1(&mut self, graph: &mut Graph, lib: &ModuleLibrary, host: &mut dyn WfcHost) -> bool {
        if let Some(l) = self.last_removals.pop() {
            let qube = l.qube.expect("WaveFunctionCollapse.Iterate1: NullReferenceException");
            let q = graph.qube(qube);
            if q.empty() || l.propagation_index != q.last_propagation {
                return true;
            }
            let rel = graph.get_qube_relation6(qube, !(1u8 << (l.side_index as u8 as u32 & 31)));
            let mut k = graph.qube(qube).possible_modules.len() as i32 - 1;
            while k >= 0 {
                let m = *graph.qube(qube).possible_modules.get(k as usize).unwrap_or_else(|| panic!("ArgumentOutOfRangeException: possibleModules[{k}]"));
                if m.sides.get1(l.side_index as i32) == l.side {
                    self.remove_possible_module(graph, host, qube, m, k, rel);
                }
                k -= 1;
            }
            return true;
        }
        if let Some(mut p) = self.propagations.pop() {
            let qube = p.qube.expect("WaveFunctionCollapse.Iterate1: NullReferenceException");
            let q = graph.qube(qube);
            if q.empty() || p.existance_index != q.existance_index || q.existance_index & 1 != 0 || p.propagation_index != q.last_propagation {
                return true;
            }
            let rel = graph.get_qube_relation6(qube, p.side_variation_mask);
            for s in 0..6 {
                let r = rel.get(s);
                if r.index_in_other == -1 {
                    continue;
                }
                let other = r.qube.expect("WaveFunctionCollapse.Iterate1: NullReferenceException");
                let step = graph.qube(qube).last_propagation;
                let o = graph.qube(other);
                if o.fully_covered() || step <= o.last_propagation {
                    continue;
                }
                graph.qube_mut(other).last_propagation = step;
                let existance_index = graph.qube(other).existance_index;
                let m = self.fill_qube_possibilities(graph, lib, host, other) & !(1u8 << (r.index_in_other as u32 & 31));
                self.propagations.push(Propagation { qube: Some(other), propagation_index: p.propagation_index, existance_index, side_variation_mask: m });
                // also `Debug.DrawLine` (editor only)
            }
            p.side_variation_mask = graph.qube(qube).side_variation_mask;
            insert_sorted(&mut self.to_collapse, p, graph);
            return true;
        }
        let Some(&p) = self.to_collapse.last() else { return false };
        let i = self.to_collapse.len() - 1;
        let qube = p.qube.expect("WaveFunctionCollapse.Iterate1: NullReferenceException");
        let q = graph.qube(qube);
        if !q.empty() && p.existance_index == q.existance_index && p.propagation_index == q.last_propagation {
            let n = q.possible_modules.len();
            if n == 1 {
                let m = q.possible_modules[0];
                host.apply_module(graph, qube, m);
                graph.qube_mut(qube).awaiting_rebuilding = false;
                self.to_collapse.remove(i);
                let step = p.propagation_index.wrapping_add(1);
                graph.qube_mut(qube).last_propagation = step;
                self.decor_updates.push(Propagation { propagation_index: step, ..p });
                return true;
            }
            if n != 0 {
                let rel = graph.get_qube_relation6(qube, p.side_variation_mask);
                let mut k = n as i32 - 1;
                while k >= 1 {
                    let m = *graph.qube(qube).possible_modules.get(k as usize).unwrap_or_else(|| panic!("ArgumentOutOfRangeException: possibleModules[{k}]"));
                    if self.remove_possible_module(graph, host, qube, m, k, rel) {
                        self.maybe_queue_qube_for_rebuilding(graph, host, qube);
                        return true;
                    }
                    k -= 1;
                }
                return true;
            }
        }
        self.to_collapse.remove(i);
        true
    }

    /// `WaveFunctionCollapse.IterateApplyModules` 0x3D8750: from the back of `qubesToRebuild`, each qube still
    /// awaiting rebuilding is built with its first module, while `WorldMaster.KeepGoing`; the handled ones are
    /// removed. False when the list is empty.
    pub fn iterate_apply_modules(&mut self, graph: &mut Graph, host: &mut dyn WfcHost) -> bool {
        let n = self.qubes_to_rebuild.len();
        if n < 1 {
            return false;
        }
        let mut i = n;
        loop {
            if !host.keep_going() || i < 1 {
                self.qubes_to_rebuild.drain(i..n);
                return true;
            }
            i -= 1;
            let qube = self.qubes_to_rebuild[i];
            if graph.qube(qube).awaiting_rebuilding {
                graph.qube_mut(qube).awaiting_rebuilding = false;
                if let Some(&m) = graph.qube(qube).possible_modules.first() {
                    host.apply_module(graph, qube, m);
                }
            }
        }
    }

    /// `WaveFunctionCollapse.LaterUpdate` 0x3D93C0: every qube left in `qubesToRebuild` (from the back) that still
    /// awaits rebuilding is built now, with its first module; but a qube with several modules at an odd step
    /// (`lastPropagation & 3 == 1`) that already shows a module keeps it if it is still possible, else takes the
    /// most similar one (the smallest `|moduleIndex - current|` minus 100 for each side profile in common). Then
    /// the list is cleared. (Inside a profiler marker.)
    pub fn later_update(&mut self, graph: &mut Graph, lib: &ModuleLibrary, host: &mut dyn WfcHost) {
        let mut i = self.qubes_to_rebuild.len();
        while i > 0 {
            i -= 1;
            let qube = self.qubes_to_rebuild[i];
            if !graph.qube(qube).awaiting_rebuilding {
                continue;
            }
            graph.qube_mut(qube).awaiting_rebuilding = false;
            let q = graph.qube(qube);
            let n = q.possible_modules.len();
            if n == 0 {
                continue;
            }
            let mut chosen = q.possible_modules[0];
            if 1 < n && q.last_propagation & 3 == 1 && q.last_propagation != 0 {
                if let Some(current) = host.current_module(graph, qube) {
                    let module = &lib.modules[current.module_index as usize];
                    let cs = lib.side_lookups[module.base.profile_indexes[current.orientation() as usize] as usize];
                    let mut best = f32::MAX;
                    for k in 0..n {
                        let m = graph.qube(qube).possible_modules[k];
                        if m.oriented_module.eq_cs(&current) {
                            chosen = m;
                            break;
                        }
                        let mut score = (m.oriented_module.module_index as i32 - current.module_index as i32).abs() as f32;
                        let s = m.sides;
                        for (a, b) in [(cs.v00, s.v00), (cs.v10, s.v10), (cs.v20, s.v20), (cs.v30, s.v30), (cs.v4, s.v4), (cs.v5, s.v5)] {
                            if a == b {
                                score -= 100.0;
                            }
                        }
                        if score < best {
                            chosen = m;
                            best = score;
                        }
                    }
                }
            }
            host.apply_module(graph, qube, chosen);
        }
        self.qubes_to_rebuild.clear();
    }

    /// `WaveFunctionCollapse.RemovePossibleDecor` 0x3D9F90: drops entry `index` of the qube's list (decor after
    /// its module at 0). For each side the removed decor shows a profile on (not 0), a neighbour in `relations`
    /// that isn't fully covered and has the shared side in its decor mask is noted in `lastDecorRemovals` when
    /// no decor left in the qube (from index 1) shows that profile. A relation with an index but no qube ends it.
    pub fn remove_possible_decor(&mut self, graph: &mut Graph, qube: QubeId, possible_module: OrientedModuleSides, index: i32, relations: QubeRelation6) {
        let pm = &mut graph.qube_mut(qube).possible_modules;
        if index < 0 || index as usize >= pm.len() {
            panic!("ArgumentOutOfRangeException: RemoveAt({index})");
        }
        pm.remove(index as usize);
        for s in 0..6i32 {
            let v = possible_module.sides.get0(s);
            if v == 0 {
                continue;
            }
            let r = relations.get(s);
            if r.index_in_other == -1 {
                continue;
            }
            let Some(other) = r.qube else { return };
            let o = graph.qube(other);
            if o.fully_covered() || !utils::is_bit_set_u8(o.decor_side_variation_mask, r.index_in_other as i32) {
                continue;
            }
            let q = graph.qube(qube);
            if q.possible_modules.iter().skip(1).any(|m| m.sides.get0(s) == v) {
                continue;
            }
            self.last_decor_removals.push(LastRemovedStruct { qube: Some(other), side: v, side_index: r.index_in_other, propagation_index: q.last_propagation });
        }
    }

    /// `WaveFunctionCollapse.FillDecorPossibilities` 0x3D6240: the decor that fits a collapsed qube, after its
    /// module (kept at index 0). The module's `decorModules`, each turned by the module's orientation
    /// (`ModuleMath.MulOrientations`), with the decor module's side profile and cost (its `cost` + the same
    /// jitter as `FillQubePossibilities`), sorted by cost; a repeated non-zero profile index only replaces the
    /// previous entry when cheaper. A decor fits when it shows nothing (profile 0) towards open sides, and towards
    /// a neighbour at the same or a later step one of the neighbour's decor (from index 1) shows the profile.
    /// The mask has the sides whose profile differs from the previous decor's (the first against all zeros).
    /// Then "Validate Opposite Decors": a neighbour's decor that no decor of the qube matches is removed. A qube
    /// with no entry returns 0 untouched. (Inside a "FillDecorPossibilities" profiler marker.)
    pub fn fill_decor_possibilities(&mut self, graph: &mut Graph, lib: &ModuleLibrary, qube: QubeId) -> u8 {
        let Some(&first) = graph.qube(qube).possible_modules.first() else { return 0 };
        graph.qube_mut(qube).possible_modules.clear();
        let decor = &lib.modules[first.oriented_module.module_index as usize].decor_modules;
        let rel = graph.get_qube_relation6(qube, 0xFF);
        let _ = graph.qube_sibling_index(qube);
        let pos = graph.qube_position(qube);
        let mut mask = 0u8;
        let mut prev = crate::placemaker::modules::SideProfile::default();
        let mut last_index = -1i32;
        let mut valid = false;
        let mut last_side_index = 0xFFFFu32;
        let mut last_cost = 0.0f32;
        for (i, &om) in decor.iter().enumerate() {
            let mut om = om;
            om.set_orientation(ModuleMath::mul_orientations(first.oriented_module.orientation(), om.orientation()) as i32);
            let dm = &lib.decor_modules[om.module_index as usize];
            let side_index = dm.base.profile_indexes[om.orientation() as usize] as u32;
            let jitter = frac_abs((((i as i32) as f32 + pos.x + pos.z) + pos.y) * std::f32::consts::PI * 17.0);
            let cost = dm.base.cost as f32 + jitter;
            if last_side_index == side_index && last_side_index != 0 {
                if valid && cost < last_cost {
                    let sides = lib.side_lookups[side_index as usize];
                    graph.qube_mut(qube).possible_modules[last_index as usize] = OrientedModuleSides { sides, oriented_module: om, cost };
                    last_cost = cost;
                }
                continue;
            }
            last_side_index = side_index;
            let sides = lib.side_lookups[side_index as usize];
            valid = true;
            let mut checked_all = true;
            for s in 0..6i32 {
                let r = rel.get(s);
                let v = sides.get0(s);
                if r.index_in_other == -1 {
                    if v != 0 {
                        valid = false;
                        checked_all = false;
                        break;
                    }
                    continue;
                }
                if v == 0 || !valid {
                    continue;
                }
                let other = r.qube.expect("WaveFunctionCollapse.FillDecorPossibilities: NullReferenceException");
                let o = graph.qube(other);
                if o.last_propagation < graph.qube(qube).last_propagation {
                    continue;
                }
                valid = o.possible_modules.iter().skip(1).rev().any(|m| m.sides.get1(r.index_in_other as i32) == v);
            }
            if checked_all {
                let pairs = [(sides.v00, prev.v00), (sides.v10, prev.v10), (sides.v20, prev.v20), (sides.v30, prev.v30), (sides.v4, prev.v4), (sides.v5, prev.v5)];
                for (k, (a, b)) in pairs.iter().enumerate() {
                    if a != b {
                        mask |= 1 << k;
                    }
                }
            }
            prev = sides;
            if valid {
                let m = OrientedModuleSides { sides, oriented_module: om, cost };
                let pm = &mut graph.qube_mut(qube).possible_modules;
                let at = collections::binary_search(pm, 0, pm.len(), &m, &mut |a, b| a.compare_to(b));
                let at = if at < 0 { !at } else { at };
                pm.insert(at as usize, m);
                last_index = at;
                last_cost = cost;
            }
        }
        let q = graph.qube_mut(qube);
        q.possible_modules.insert(0, first);
        q.decor_side_variation_mask = mask;

        // "Validate Opposite Decors" (a profiler marker)
        for s in 0..6i32 {
            let r = rel.get(s);
            if r.index_in_other == -1 {
                continue;
            }
            let other = r.qube.expect("WaveFunctionCollapse.FillDecorPossibilities: NullReferenceException");
            if graph.qube(other).last_propagation < graph.qube(qube).last_propagation {
                continue;
            }
            let mut k = graph.qube(other).possible_modules.len() as i32 - 1;
            while k >= 1 {
                let Some(&y) = graph.qube(other).possible_modules.get(k as usize) else {
                    panic!("ArgumentOutOfRangeException: possibleModules[{k}]")
                };
                let v = y.sides.get1(r.index_in_other as i32);
                if v != 0 && !graph.qube(qube).possible_modules.iter().skip(1).any(|m| m.sides.get0(s) == v) {
                    let rel_other = graph.get_qube_relation6(other, graph.qube(other).decor_side_variation_mask);
                    self.remove_possible_decor(graph, other, y, k, rel_other);
                }
                k -= 1;
            }
        }
        mask
    }

    /// `WaveFunctionCollapse.IterateDecor` 0x3D88D0: one step of the decor, which runs on collapsed qubes like the
    /// modules on all of them. In order: the last of `decorUpdates` (next step; queued to build, and a qube with
    /// only its module gets its decor filled and propagated, mask with the old one); the last of
    /// `lastDecorRemovals` (the decor showing the removed profile removed); the last of `decorPropagations` (each
    /// neighbour in the mask that isn't fully covered and is behind refilled, propagated and queued to build;
    /// the propagation into `decorToCollapse` by its order); the last of `decorToCollapse` (decor overlapping an
    /// earlier one, by `ModuleLibrary.GetDecorOverlap`, removed); the last of `decorToBuild`
    /// (`ModuleBuilder.ApplyDecor`). Stale entries (step or existance changed) are dropped. False when all are
    /// empty.
    pub fn iterate_decor(&mut self, graph: &mut Graph, lib: &ModuleLibrary, host: &mut dyn WfcHost) -> bool {
        let stale = |graph: &Graph, p: &Propagation| {
            let q = graph.qube(p.qube.expect("WaveFunctionCollapse.IterateDecor: NullReferenceException"));
            p.propagation_index != q.last_propagation || p.existance_index != q.existance_index
        };
        if let Some(mut p) = self.decor_updates.pop() {
            if stale(graph, &p) {
                return true;
            }
            let qube = p.qube.unwrap();
            let q = graph.qube_mut(qube);
            q.last_propagation = q.last_propagation.wrapping_add(1);
            p.propagation_index = p.propagation_index.wrapping_add(1);
            self.decor_to_build.push(p);
            if graph.qube(qube).possible_modules.len() == 1 {
                let old = graph.qube(qube).decor_side_variation_mask;
                let m = self.fill_decor_possibilities(graph, lib, qube) | old;
                let existance_index = graph.qube(qube).existance_index;
                self.decor_propagations.push(Propagation { qube: Some(qube), propagation_index: p.propagation_index, existance_index, side_variation_mask: m });
            }
            return true;
        }
        if let Some(l) = self.last_decor_removals.pop() {
            let qube = l.qube.expect("WaveFunctionCollapse.IterateDecor: NullReferenceException");
            if l.propagation_index != graph.qube(qube).last_propagation {
                return true;
            }
            let m = graph.qube(qube).decor_side_variation_mask & !(1u8 << (l.side_index as i32 as u32 & 31));
            let rel = graph.get_qube_relation6(qube, m);
            let mut k = graph.qube(qube).possible_modules.len() as i32 - 1;
            while k >= 1 {
                let y = *graph.qube(qube).possible_modules.get(k as usize).unwrap_or_else(|| panic!("ArgumentOutOfRangeException: possibleModules[{k}]"));
                if y.sides.get1(l.side_index as i32) == l.side {
                    self.remove_possible_decor(graph, qube, y, k, rel);
                }
                k -= 1;
            }
            return true;
        }
        if let Some(mut p) = self.decor_propagations.pop() {
            if stale(graph, &p) {
                return true;
            }
            let qube = p.qube.unwrap();
            if graph.qube(qube).possible_modules.len() < 2 {
                return true;
            }
            let rel = graph.get_qube_relation6(qube, p.side_variation_mask);
            for s in 0..6 {
                if !utils::is_bit_set_u8(p.side_variation_mask, s) {
                    continue;
                }
                let r = rel.get(s);
                if r.index_in_other == -1 {
                    continue;
                }
                let other = r.qube.expect("WaveFunctionCollapse.IterateDecor: NullReferenceException");
                let step = graph.qube(qube).last_propagation;
                let o = graph.qube(other);
                if o.fully_covered() || step <= o.last_propagation {
                    continue;
                }
                let existance_index = o.existance_index;
                let m = self.fill_decor_possibilities(graph, lib, other) & !(1u8 << (r.index_in_other as u32 & 31));
                graph.qube_mut(other).last_propagation = step;
                let np = Propagation { qube: Some(other), propagation_index: p.propagation_index, existance_index, side_variation_mask: m };
                self.decor_propagations.push(np);
                self.decor_to_build.push(np);
            }
            p.side_variation_mask = graph.qube(qube).decor_side_variation_mask;
            insert_sorted(&mut self.decor_to_collapse, p, graph);
            return true;
        }
        if let Some(p) = self.decor_to_collapse.pop() {
            if stale(graph, &p) {
                return true;
            }
            let qube = p.qube.unwrap();
            if graph.qube(qube).possible_modules.len() == 1 {
                return true;
            }
            let rel = graph.get_qube_relation6(qube, p.side_variation_mask);
            let mut i = 1usize;
            while i < graph.qube(qube).possible_modules.len() {
                let x = graph.qube(qube).possible_modules[i];
                let mut j = graph.qube(qube).possible_modules.len() as i32 - 1;
                while (i as i32) < j {
                    let y = *graph.qube(qube).possible_modules.get(j as usize).unwrap_or_else(|| panic!("ArgumentOutOfRangeException: possibleModules[{j}]"));
                    if lib.get_decor_overlap(x.oriented_module, y.oriented_module) {
                        self.remove_possible_decor(graph, qube, y, j, rel);
                    }
                    j -= 1;
                }
                i += 1;
            }
            return true;
        }
        if let Some(p) = self.decor_to_build.pop() {
            if stale(graph, &p) {
                return true;
            }
            host.apply_decor(graph, p.qube.unwrap());
            return true;
        }
        false
    }
}
