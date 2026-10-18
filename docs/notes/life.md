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

## Birds: the flight update (G5b)

`BirdFlock.OnUpdate` 0x7BC490 and `GetLanding` 0x7BA960, checked with `tests/placemaker_bird_update.rs`: a fresh
flock with up to 12 clones of the scene's bird and 24 test perches put into the prop grid far from the town (a
`PropCollider` brings its own `PropNode`: the one to set is that one), plus leaving birds in front of the camera.
300 updates (2,155 birds, `Random` in a known state) match to the bit: every field, the list, the perches, the
birds' transforms, meshes and active flags. They took 197 landings, 16 hops, 11 perches in flight and 579
departures.

- Order: the list's count is read first; below the minimum, leaving birds come back (`lastActionTime = Time.time
  + Random.value`) until the ideal count. With any perch: past `lastOrbitTime` a new orbit target, 8..20 s on.
  It is a random perch's target (when `Random.value < 0.5` and one is found), else a random point of the graph's
  hex bounds widened by 8, at a random height `0..boundsHeight` (`HexToPlane`, `PlaneToWorld`). Either way it
  gets `insideUnitSphere × Range(0, 7)` and `y ≥ 0.4`. The orbit point then moves towards the target at
  `10 × deltaTime`.
- One bubble pass over the list by `pos.x` (the bigger one carries on). Then each bird whose step is due
  (`time + 0.3 < Time.time`) takes one step. It moves `posMid` by `velocityNext`, keeps the old velocity, mesh
  and rotation as the "previous" ones, and gets `dt = Time.time - lastActionTime`.
- Flying (boiding, leaving):
  - Above the maximum, a boiding bird leaves.
  - With a perch: past 9 s it lets the perch go. Otherwise it steers to the perch's target point (the distance
    clamped to 1, doubled), `Lerp(0.1, 0.8, InverseLerp(6, 9, dt))`. Within 1.5 it starts landing, and that is
    its whole step.
  - Without a perch: `y × 0.7`, then a fifth of the way towards the orbit point (leaving: along its old level
    heading), normalised.
  - Neighbours: up to 7 flying birds within 2 of `pos`, met alternately ahead and behind in the list. The game
    ends the behind side unless that bird is at least 2 ahead in x, so only birds ahead count. Each is weighted
    `w = 1 - d/4`: pushed by `(w - 0.6) w³` along the line and its interpolated velocity matched by `w³`. The sum
    over `Σ w²` is added.
  - A leaving bird out of view (`UnityFunctions.InView(Camera.main, posMid, 0.2)`) or 400 cells outside the
    graph goes to the disabled birds and out of the list (the active count was already lowered).
  - Ground: along twice the velocity in `ceil(2|v|) + 1` steps, the first point with normal coverage pushes the
    velocity by `normal × 2 coverage`.
  - Boiding past 8 s: `GetLanding`, the free turned-on perch within 1 of `posMid + velocity` whose target is
    within `dot ≥ 0.8` of the heading. The best is by `dot + 0.1 × neighbours`. It is taken (landing state) and
    the velocity moves 0.5 towards the target, clamped to 0.7.
  - Mesh: flap when `|v - v_prev + (0, 0.5, 0)| - |v| + 0.2 > 0.6`, else fly. It faces `v` with its y scaled
    by `InverseLerp(1, 0.7, that)`, up `v - v_prev + (0, 0.5, 0)`.
- Landing: land mesh, then two cases.
  - Within 0.3 of `GetLandingPos` it lands: velocity to the spot, rotation 30% of the way to facing off the
    perch, then stopping.
  - Otherwise the velocity moves 0.8 towards the point `Lerp(spot, target, d + 0.1)`, clamped to 0.7. It faces
    the velocity and turns 10% of the way to that facing.
- Stopping: still, stopped, half-sit. Its shade goes onto the perch (`rotation × Euler(-90, 90, 0)`). It faces
  away from the perch's up (y × 0.4) along the normal's level part, turned by `RandomRangeInt(-20, 20)`.
- Stopped: still. Past 3 s it acts and stamps `Time.time`.
  - 10%: a hop to the best free perch within 0.8. The score is neighbours × 0.3 − squared distance +
    `Random.value × 0.2` (each candidate draws), against staying (own neighbours × 0.3 + a draw × 0.2).
    It flies to the new spot facing the hop's level part (the y is zeroed, which the decompile hides in a bit
    mask) and is stopping.
  - Else 30%: a new pose. On a level perch (`normal.y > 0.95`) it stands (raised by `0.01 × scale.y`) or sits,
    and moves there. Elsewhere it sits or half-sits.
  - Else: a turn of up to 40° (level) or 20°.
- After a step that goes on (not the landing start or a departure): `mf0` is shown while its mesh differs from
  `mf1`'s, else hidden and `mf1` at scale 1.
- Then every bird is placed:
  - Position: on the quadratic Bézier from `posMid - v_prev/2` through `posMid` to `posMid + v/2` at `t` (its
    0.3 s step).
  - Rotation: `Quaternion.Lerp(rotation0, rotation1, t)`.
  - Shade: shown while either state is stopped, at `Lerp(a, b, t) × 0.22`. `a` and `b` are 1 when that state is
    stopped (× 0.7 when that mesh is the standing one).
  - Mesh scales: while `mf0` is shown, `mf0` shrinks by `1 - t³` and `mf1` grows by `1 - (1-t)³`.
  - Finally a nonzero take-off count is reset.
- `flapAudioClips`, `audioSource0/1`, `lastAudioTime` and the shake fields are never read in this build.

## Butterflies (G5c)

`placemaker::life::butterfly_flock`, checked with `tests/placemaker_butterflies.rs`. A fresh flock gets the scene's
master, meshes and `srcButterfly`, plus 24 test flowers in the prop grid: three clusters far out and one by a
perch of the town, for the ground push. One simulation runs on both sides for 300 rounds. Each round disturbs
some butterflies, changes the flower count, turns flowers off and on, and adds or removes a flower or resets
the flock, with `Random` in a known state. Every round matches to the bit: up to 12 butterflies, about 1,800
butterfly-steps, settlings, flower changes, take-offs and ground pushes. The compared state is the generator,
every butterfly's fields, mesh and pose, the two containers' child order and the flowers.

- A butterfly belongs to a flower (`ButterflyLanding`, a prop, `IPropEnable`: `OnEnable` → `AddLanding`,
  `OnDisable` → `RemoveLanding`; nothing on first enable) and lives under `activeButterflies` or
  `disabledButterflies`, whose child order the flock walks.
- `AddLanding`: one more flower. Below `Min(32, RoundToInt(n × 0.05 × 0.9))` active butterflies, one comes to it.
  - It is the first disabled one, else a new `Instantiate(srcButterfly)` scaled `Range(0.8, 1.2)`.
  - It goes under the active ones, at the flower's position and rotation, still.
  - Its clock is `Time.time - RandomRangeInt(0, 2)`.
  - Half of them sit (`Random.value ≥ 0.5`): sit, or sit-flap when a second draw is below 0.2. The rest flap.
  - `SetLanding` links it to the flower; `SetButterfly` logs when a flower already has one.
- `RemoveLanding`: one fewer. Within `RoundToInt(n × 0.05 × 1.1)` active butterflies, the flower's butterfly
  lets go and flies (a sitting one flaps). Above that, its butterfly (or the first active one) is put away and
  keeps its flower link. `OnReset` puts every active one away (from the last).
- `OnUpdate`: the active ones from the last; a due one (`Time.time > time + 0.3`) takes one step. It moves
  `posMid` by its velocity, keeps that velocity as the previous one, its rotation as `rotation0`, and its clock
  +0.3.
  - Sitting: still. With 0.4% it flies off along `TransformVector(0, 0.4, 0.2)`, flapping.
  - Flying or landing without a flower: it takes the last free flower (turned on, within 0.8 of the point ahead
    raised 0.4, `PropRadiusIterator<ButterflyLanding>`). With none it is put away and the count drops for the
    placing below.
  - With its flower: the velocity × 0.7. The goal is `p = TransformPoint(0, 0.01, 0)` plus
    `TransformVector(0, 0.3, 0)`; landing scales that offset by the squared distance to `p`, so the goal sinks as
    it nears.
    - Landing within `√0.02` of the goal: it sits and moves to `p`. It faces `LookRotation(flower.up, forward +
      up × 0.2 + insideUnitSphere × 0.5) × Euler(90, 0, 0)`, sitting (20%) or sit-flapping, and that is its step.
    - Landing farther: 10% of the way to the goal.
    - Flying: beyond `√0.5` it is pulled by `(d² - 0.5)/2`. It flutters by Perlin noise
      (`PerlinNoise(posMid.y | z | x, time × 0.01 + 0.5412 | 4.1235 | 9.8441) - 0.5`, × 0.4). A ground coverage
      above 0.3 at the next point less 0.1 pushes by `2 (coverage - 0.3) × normal` (not normalised).
  - Then an `insideUnitSphere` is drawn and not used. The velocity goes 30% of the way to its own direction ×
    0.2 (a speed of about 0.2 per step). A NaN is logged and resets it onto its flower. Landing averages the
    velocity with the goal offset.
  - Facing: its velocity with the climb halved, else its body's level heading (both only above `0.001²`).
  - Flying: 0.4% start landing. Else 5% look for another free flower within 0.8 of 0.4 below it, the smallest
    `d² + Random.value` (one draw per free flower).
- Placing: every active one on the birds' Bézier between its steps, rotation `Quaternion.Lerp(rotation0,
  rotation1, t)`.
- `Transform.TransformVector` (engine `TransformVector` 0x1065FDF0) is now in `unity::transform`: the node's and
  each ancestor's scale and rotation, the `TransformPoint` routine without the translation.
