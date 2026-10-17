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
