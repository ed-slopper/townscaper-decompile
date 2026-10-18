# Life: birds and butterflies (G5)

## Birds: perches, creation, take-off (G5a)

`placemaker::life::bird_flock`. Checked on a fresh `BirdFlock` with the scene's master
(`tests/placemaker_bird_flock.rs`): `CalculateOptimalLandings` on every perch count up to 5000,
`GetSearchCoordinates`, and 300 `SetState` / `UprootBird` calls with `UnityEngine.Random` in a known state (the
generator's state after, the bird's fields and the flock's take-off record to the bit).

- Perches are props with a `BirdLanding` (`IPropEnable`): on first enable the normal is
  `transform.TransformVector(-1, 0, 0)`, bent halfway to `Graph.SampleNormalCoverage` where that has coverage; on
  enable / disable they register with the flock (`AddOrRemoveLanding`). Positions: `pos` (the transform),
  `sortPos` (+ normal × 0.1), `targetPos` (+ normal × 0.3).
- Search: the prop placer's `colliderDict` (int3 cells of 2 units), cells `floor((p - r)/2) .. ceil((p + r)/2)`
  exclusive, z then y then x, each cell's colliders from the end, only those inside their own cell, within a
  squared radius, with `TryGetComponent<BirdLanding>` and a turned-on `PropNode`.
- Counts from the perch count `n`: `c = Clamp(sqrt(n) - 6, 0, 64)`; ideal `FloorToInt(c)`, min
  `(byte)Max(0, 0.9c - 2)`, max `(byte)Max(0, 1.1c + 2)` (recomputed when dirty).
- A perch knows how many occupied perches lie within 1.1 (itself included): `SetBird` adds or takes one from
  each; a new perch counts its occupied neighbours (when any bird is assigned); the first perch sets the orbit to
  its target position. A removed perch lets its bird take off, or past the maximum count makes it leave in a puff
  of feathers (to the disabled birds).
- `IterateBirdCreation` (below the minimum): `r = Range(0.2, 0.6)`; a random perch (up to 100 random squares,
  a random perch of the first square that has any, `GetComponentsInChildren`); every free perch within `r` of its
  sort position (search radius `r + 0.1`) gets a bird until the maximum: a disabled bird (the last) or a new one
  (`Instantiate(srcBird, activeBirds)`, clock `count / π + Time.time`, preferred neighbours `(count & 7) + 1`,
  size `Range(0.8, 1.2)`); its clock snapped to the 0.3 s step (`dt`); appended to `birds`; stopping; perched at
  `TransformPoint(-0.04 × size.x, 0, 0)`; still; mesh land (10%) else half-sit / sit (50%); the shade on the
  perch turned `rotation × Euler(-90, 90, 0)`; facing `Euler(0, RandomRange(-20, 20), 0) ×
  LookRotation(flat normal − up, the y part × 0.4)`; rested `Range(3, 25)` s more; sound 7.
- `UprootBird`: a stopped bird jumps along its perch normal (`insideUnitSphere × 0.5` added, times the speed), its
  mid position eased by how far into its 0.3 s step it was; off the perch; boiding; flap mesh; sounds 8 and 9. Only
  the first take-off since the update last read them is recorded (`takeOffCount` 0 → 1, `takeOffPos`).
- `UprootRadius(pos, radius, speed)` (the click effect, radius 1): from a binary search of the x-sorted `birds`
  (`ExtraMath.BinarySearch`), every bird within the radius until one is past `pos.x + radius`.
- `SetState` stamps `lastActionTime = Time.time + Random.value` (logging "No landing" when landing without a
  perch); `SetLanding` stamps `Time.time`.
- The flight update (`OnUpdate`, `GetLanding`) is G5b; butterflies G5c.
