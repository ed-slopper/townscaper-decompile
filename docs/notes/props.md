# Props (E10)

Chimneys, lamps, flags, wires, plants: the small objects modules carry. `placemaker::props`.

## The data (E10a, proven)

- `PropPlacement` (20 bytes: `pos` SbyteFloat3, `scale` ByteFloat3, `rotationByte` ByteFloat3, `prop`, `corner`,
  `prioAdd`). `rotation` = bytes × 1.40625° (256 to a turn). `set_rotation`: per angle
  `(byte)(int)((float)fmod((double)(a + 360f), 360.0) * 0.7111111f + 0.5f)` — truncation, so a full turn wraps
  to 0 through the byte. `GetMatrix` = `TRS(pos, Euler(rotation), scale * 2)`.
- `Prop`: a prefab; `voxelTypeMask` (all types by default), `srcProp` on instances.
- `PropNode`: one placed prop in the graph that decides which props show. `relations` is one list in five runs —
  parents, children, overlaps, motevees, anchors — with a count each; the `…Index` getters are the run starts.
  `Add…` inserts at the end of its run; `InserOverlap` inserts into the overlaps where `List.BinarySearch` (the
  node's own comparer: `priority`, `Single.CompareTo`, NaN first) puts it. Quirk kept: every `Remove…` uses
  `List.Remove` (the first occurrence anywhere) and lowers its own count even if the node was in another run, so
  a count can wrap at 0; later inserts past the end then throw (`ArgumentOutOfRangeException`, and
  `BinarySearch`'s range check an `ArgumentException`): the port returns false and changes nothing.
  `.ctor`: `motivations = 1`.

Checked (`tests/placemaker_props.rs`): 500 random placements (rotation, matrix, `set_rotation` on random and
whole-turn angles), and 400 random relation edits on fresh nodes (relations, counts, index getters, which edits
throw) and every pair through `CompareTo`.

## Placing props (E10b, read so far; blocked on E11)

`PropPlacer.PlacePropsInQube` 0x70A800 (read up to the prop modifiers), for a qube with possible modules:

- A `Unity.Mathematics.Random` seeded from the qube's world position: `s = Abs(position.GetHashCode())`, one
  xorshift step (`<< 13`, `>> 17`, `<< 5`).
- For each module container under the qube (the last child first) not yet holding props (`hasProps`), the
  matching `possibleModules[i]`: container 0 is the module, the others its decor modules (an out-of-range decor
  logs an error and breaks into the debugger). The orientation's matrix (`ModuleMath.orientations[o]`).
- For each of the module's prop placements (the last first): the prop prefab's `PropNode.priority`, and a draw
  `NextFloat(prioAdd + priority, priority + 1 + prioAdd)` (both `prioAdd` from the placement's second byte); a
  placement whose draw is negative is skipped. The corner value under the placement (`cornerTransform1`; none
  for a negative corner) must be in the prop's `voxelTypeMask`. The placement matrix (`orientation * GetMatrix`)
  gives the prop's columns, sent through the square (`GetTs` at `column3 + 0.5`, `ModuleBuilder.MultiplyVector`
  / `MultiplyPoint`). Props reaching past a side (`|x|` or `|z|` of the position over 0.4) are only kept where
  the side profile there is open (`SideProfile.Get0`; at the ground, not a water side) and, across into the
  neighbouring qube (`Graph.GetQubeRelation`), the neighbour's orientation and square are blended in (`Max`,
  `Min`, the doubled offset), with corner-position tests deciding which square owns a shared prop.
- The prop instance comes from `GetProp` (pooled), gets `localPosition`, `rotation = LookRotation(forward, up)`
  and `localScale` (x negated where the placement mirrors), then each transform under it: coloured meshes swap
  to the voxel type's mesh (`GetMeshWithVoxelType`, 0xF unless the placement's corner is ≥ 0 and its value < 15),
  `PropTransformer`s with `fitInSquare` refit through the world / local matrices…

Blocked here: from this point the placement reads Unity's native transform math back (`worldToLocalMatrix`,
`localToWorldMatrix`, `lossyScale`, and the rotation / scale round trips on prop instances it has just moved), so
an exact port needs row E11 (`unity::transform`) first.
