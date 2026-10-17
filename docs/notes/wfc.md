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
qubes a round); 600 `CompareTo` pairs; `ResetPropagationCounter` at 0x7FFF and 0x8004. The filling alone never
reached `RemovePossibleModule`'s `lastRemovals` branch (no neighbour had the shared side in its mask yet); the
loops below do. The gizmos (`OnDrawGizmos`, `DrawPropagationGizmo`) are
editor-only: `read`.

## The loops (E6b)

The world master runs, between the graph's update and the shadows: `Iterate0`, `IterateApplyModules`, then
later `Iterate1`, `IterateDecor` and `ResetPropagationCounter`, each until it returns false or the frame's time
is up. `LaterUpdate` builds what is left in `qubesToRebuild`.

- `Iterate0` 0x3D7A00: a new step only starts (`lastStartedPropagation = propagationCounter`, true). Then the
  last of `updates`: if still current (step and existance), the qube's `possibleModuleListIndex` is
  `cornerDict[GetIndexedMaterials(MarkHiddenCornersAmbigous(cornerValues))]` (-1 if missing, or when the corners
  are all "any"); an empty or fully covered qube loses its module containers; a fully covered one also its
  modules (and no propagation). Otherwise it is filled at `counter + 1`, offered to
  `MaybeQueueQubeForRebuilding`, and its propagation (mask: the fill's; for a qube that had modules, plus its
  old mask and each side whose first module's profile changed; for a bottom qube that had none, plus the four
  round sides) goes to `propagations`. Else the last of `removeNeighbors`: still current, it is refilled one
  step on and its mask narrowed by the fill's.
- `Iterate1` 0x3D7F50, first non-empty of: `lastRemovals` (on a current, non-empty qube, every module showing
  the removed profile on that side is removed, relations of all but that side); `propagations` (a current,
  non-empty qube with an even existance: each neighbour by the mask, not fully covered and behind, moves to the
  qube's step, is filled and propagated with the fill's mask minus the shared side; the propagation, with the
  qube's mask, is inserted into `toCollapse` by `CompareTo`); `toCollapse` (its last entry: one module left →
  built, removed, one step on and passed to `decorUpdates`; more → its dearest modules removed one at a time
  down to index 1 until a removal reaches a neighbour, then `MaybeQueueQubeForRebuilding`; stale or empty →
  removed).
- `IterateApplyModules` 0x3D8750: from the back of `qubesToRebuild`, the qubes still awaiting are built with
  their first module while `WorldMaster.KeepGoing`; the handled part of the list is removed.
- `LaterUpdate` 0x3D93C0 (marker "Apply Modules"): every qube left awaiting is built now; one with several
  modules at a step with `lastPropagation & 3 == 1` that already shows a module (its first child's
  `ModuleContainer`) keeps that module if still possible, else takes the closest: `|moduleIndex - current|`
  minus 100 per side profile in common, lowest first. Then the list is cleared.

## The decor (E6c)

After a qube collapses, its `possibleModules` keeps the module at index 0 and lists its possible decor after it.

- `FillDecorPossibilities` 0x3D6240 (marker of the same name): nothing for an empty list (returns 0). Else the
  module's `decorModules`, each turned by `MulOrientations(module, decor)`, with the decor module's profile and
  cost (`cost` + the same position jitter as the modules). A repeated non-zero profile index replaces the
  previous entry when cheaper. A decor fits when it shows profile 0 towards every open side, and towards each
  neighbour at the same or a later step a profile one of the neighbour's decor (index 1 on) shows (no
  fully-covered check here). The mask has the sides whose profile differs from the previous decor's (the first
  against zeros, unlike the modules). Then the module goes back to index 0, and "Validate Opposite Decors"
  removes each neighbour's decor (index 1 on, non-zero profile) that no decor of the qube matches, with the
  neighbour's full decor mask as relations.
- `RemovePossibleDecor` 0x3D9F90: removes the entry; for each non-zero profile side whose neighbour (not fully
  covered, the side in its decor mask) has no other decor showing it, a `lastDecorRemovals` entry. No step check.
- `IterateDecor` 0x3D88D0, first non-empty of: `decorUpdates` (one step on; queued in `decorToBuild`; a qube with
  only its module gets its decor filled and propagated, with its old decor mask added); `lastDecorRemovals`;
  `decorPropagations` (neighbours in the mask, not fully covered and behind: filled, moved to the step,
  propagated and queued to build; then into `decorToCollapse`); `decorToCollapse` (from index 1, each decor
  removes the later ones it overlaps, by `ModuleLibrary.GetDecorOverlap`); `decorToBuild`
  (`ModuleBuilder.ApplyDecor`). Stale entries are dropped.

Checked (E6b, E6c): 12 random actions on both graphs, each followed by the world master's order on the game's
fresh WFC and ours (a new step, `Iterate0`, `IterateApplyModules`, `Iterate1`, `IterateDecor`, every third step
`LaterUpdate`), each loop run until false and its round count compared; every three steps all queues, both
removal lists, both counters, every qube's modules, costs, masks and flags, and the module each qube shows (the
game's first module container under the qube against the module our host last built) are compared. Up to 88
qubes, 2,133 `Iterate1` and 2,469 `IterateDecor` rounds, 796 modules and 357 decor built; the removal branches
ran (1,899 `lastRemovals`, 47,672 `lastDecorRemovals` entries seen). For the game's real `ModuleBuilder` to run
on the stand-in, the test gives the fresh builder its two containers and the stand-in master the scene's
`propPlacer`, `bigMeshMaster` and `manySoundsManager`; `IterateApplyModules` is called one request at a time
(past the frame's time budget, `WorldMaster.KeepGoing` stops it without progress inside one batch).
`awaitingGrass` is the module builder's (E7) and is not compared there.
