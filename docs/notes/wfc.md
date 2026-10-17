# The wave function collapse (E6)

`Placemaker.WaveFunctionCollapse` decides which module fills each qube: a qube's corner values pick a
`CornerLookup` (E5d); its modules are narrowed by the neighbours' modules until one is left, which
`ModuleBuilder.ApplyModule` (E7) builds. Port: `townscaper-rs/game/src/placemaker/wfc.rs`; the qubes are the
graph's (E4), the module data the `ModuleLibrary`; the module builder and `Debug.LogError` sit behind `WfcHost`.
Checked against the running game by `game/tests/placemaker_wfc.rs` (`TS_HARNESS=1`), on the graph tests'
stand-in master, whose fresh `WaveFunctionCollapse` the test drives directly.

## Queues and order (E6a, proven)

- `Propagation` (16 bytes): qube, `propagationIndex` (u16), `existanceIndex`, `sideVariationMask`.
  `LastRemovedStruct` (12 bytes): qube, side profile (u16), side index (sbyte), propagation index.
- The queues are sorted lists: `BinarySearch` then `Insert` at the match or the complement
  (`wfc::insert_sorted`). `Propagation.CompareTo` 0x4FC880 orders by `propagationIndex` (`ushort.CompareTo`:
  the difference), then the qube's world y, its `cost` **descending**, `voxelCount`, `ModuleMath.GetMaterialCount`
  of its corner values **descending**, and last `math.sin(z + x)` of its position (`float.CompareTo`). A null
  qube on either side gives 0 past the first key.
- `propagationCounter` steps by 4 (`IteratePropagationCount` 0x3D9160: only once the last step has started,
  `propagationCounter == lastStartedPropagation`). `ResetPropagationCounter` 0x3DA690: past 0x7FFF, every qube
  under every square (both in reverse sibling order) gets `lastPropagation = 0`, and both counters 0.
- `QubeUpdated` 0x3D9ED0: `lastPropagation = counter`, a `Propagation` (mask 0) added to `updates`.
- `QubeRemoved` 0x3D9D10: the qube's flags and `lastPropagation` cleared, its modules dropped; the sides in
  `sideMask & sideVariationMask` (all four round sides too for a bottom qube, sibling index 0) give the
  neighbours that are queued in `removeNeighbors` unless fully covered (their `lastPropagation = counter`, mask
  = all but the shared side). A relation whose index is set but whose qube is null stops the loop (a `break`
  where the other paths throw). Then the qube's mask is 0.
- `MaybeQueueQubeForRebuilding` 0x3D98A0: a qube without voxels: no. One module left: built now
  (`awaitingRebuilding = false`). Otherwise queued once in `qubesToRebuild`.
- `LastRemoved` 0x3D9180 is called only by itself: dead code, `read`.

## Filling a qube (E6a, proven)

`FillQubePossibilities` 0x3D6DD0 (returns the side variation mask):

1. Clear the modules; an empty qube (`possibleModuleListIndex == -1`) returns 0. `cost = float.MaxValue`.
2. For each oriented module `i` of `cornerLookups[possibleModuleListIndex]`: its side profile
   `sideLookups[module.profileIndexes[orientation]]` and cost `module.cost + frac` with
   `frac = (float)fmod((double)|(((i + x) + z) + y) · π · 17|, 1)` (x, y, z: the qube's world position).
   - Same profile index as the previous module: if that one was kept and this is cheaper, it **replaces** it in
     place (the list is not re-sorted). Nothing else.
   - Otherwise check the six sides. No neighbour: profile 0, except for a bottom qube: a `waterSides` profile
     round it, 1 below, 0 above; a failure ends the checks. Fully covered neighbour: profile 1, else the checks
     end. Other neighbour, while still valid and at the same or a later step than the qube: if it has modules,
     one of them must show this profile on the shared side (`Get1(indexInOther)`), else the profile must be
     0 or 1; the checks go on either way.
   - When all six sides were checked (valid or not): from the second such module on, the mask gets a bit for
     each side whose profile (`v00`, `v10`, `v20`, `v30`, `v4`, `v5`) differs from the previous module's; and if
     `cost < qube.cost`, `qube.cost = module.cost` (the integer cost: so the minimum is of whole costs).
   - Valid modules are inserted at their `BinarySearch` place (by cost).
3. `qube.sideVariationMask = mask`, `qube.cost = frac(|((z + x) + y) · π · 7|) + qube.cost`.
4. "Validate Opposite Modules" (a profiler marker): for each neighbour at the same or a later step, from its last
   module down: if no module of the qube shows that module's profile on the shared side (none: the profile must
   be 0 or 1), `RemovePossibleModule` on the neighbour, with the relations of the neighbour's mask minus the
   shared side.
5. No module at all: `Debug.LogError("No inital modules", qube)`.

`RemovePossibleModule` 0x3DA190: the last module is never removed (`"No Modules"` logged, false). Otherwise it
goes; each neighbour in the given relations that isn't fully covered, has the shared side in its mask and is
at the same or a later step is checked: if no module left shows the removed module's profile on that side, it
is noted in `lastRemovals` and the result is true. One module left: built now. The editor-only
`Debug.DrawRay` / `DrawLine` are left out.

Checked (E6a): three rounds of 12 random actions on both graphs (the hooks fill `updates` and
`removeNeighbors`; all equal), then every qube filled and offered to `MaybeQueueQubeForRebuilding` in a shuffled
order, the modules, costs, masks and flags of all qubes and the WFC lists compared every few qubes (up to 270
qubes a round); 600 `CompareTo` pairs; `ResetPropagationCounter` at 0x7FFF and 0x8004. The filling never
reached `RemovePossibleModule`'s `lastRemovals` branch (no neighbour had the shared side in its mask yet): that
row stays `ported` until the loops (E6b) run it. The gizmos (`OnDrawGizmos`, `DrawPropagationGizmo`) are
editor-only: `read`.
