# Audio (G6)

## Sound effects (G6a)

`placemaker::audio::many_sounds_manager`, `placemaker::audio_source_pool`, `placemaker::ui_audio`,
`placemaker::click_particle_sound`.

Checked with `tests/placemaker_sounds.rs`. A fresh `ManySoundsManager` plays clones of the scene's 13
`ManySound`s on clones of its 3 pools, with the listener muted. Over 300 rounds the sums, clocks, damping, ring
positions and counter are set at random, `Random` is in a known state, and up to 12 requests come per round (on
and off screen, every type, two without a sound). Every sound's fields, the counter, the rings, the generator and
every source played (volume, pitch, pan, position, clip) match to the bit: about 1,900 requests and 1,800
playings. `UiAudio.Play` is checked over 200 calls on the scene's UI audio.

- The scene: `ManySoundsManager` → `ManySounds` (one `ManySound` per `SoundType`, named after it: Debris,
  DebrisSplash, Tile, Plant, Prop, Window, BirdTakeoff, Propeller, BirdAppear, ItalianWire, Lighthouse,
  BirdTakeoff1, Spire) and `Pools` (`Debris`, `PropsPool`, `BiggerProps`, 8 sources each).
- `OnEnable`: the `ManySound`s under it (`EnumerateHirearchy`, inactive too) go into `soundDict` keyed by
  `Enum.Parse(SoundType, name)`. The dictionary's order is the hierarchy's; it decides the clip draws in
  `OnUpdate`.
- `PlaySound(type, pos, pitch = 0, volume = 1)`: each frame takes about 51 requests (`counter` +1 each, +4 more
  for each one used).
  - It needs `Camera.main`, an undimmed world, and the point on screen (`0 ≤ z`, x and y in 0..1).
  - Not before the sound's `lastTimePlayed`, nor within `clickSoundSpacing` of the last click.
  - A type without a sound logs `No sound = <type>`.
  - The weight is `w = (1 - 2|x - ½| · 2|y - ½|)² × volume`. `denom += w`, `pan += x·w`, `pos += pos·w`, and
    `pitch += pitch` (unweighted).
- `OnUpdate`: `counter = 0`; then every sound in dictionary order. One with `denom > 0` plays:
  - It averages `pos`, `pan` and `pitch` by `denom`. Volume is `Clamp01(denom) × attenuation`; `attenuation ×=
    attenuator`.
  - The point moves by `normal × coverage` (`Graph.SampleNormalCoverage`).
  - Pitch is `exp((PerlinNoise(t + vx, t + vy) - ½) × usePropPitch + pitch)` over the moved point's viewport
    point.
  - A voxel on the camera's ray to the point (`Physics.Raycast(ViewportPointToRay(v), distance,
    LayerMaster.voxelMask)`) means volume × 0.4 and pitch × 0.8.
  - Then the next pooled source plays: volume, pitch × `TimeManager.slowmoScale`, pan `2 pan - 1`, the averaged
    position (not the moved one), a random clip (`RandomRangeInt(0, clips.Length)`). `lastTimePlayed = Time.time
    + timeSpacing`.
  - Every sound's sums are then cleared and `attenuation = Lerp(1, attenuation, exp(-Δt/10))`.
  - Pitches add unweighted but are divided by the weight, so a faint sound's pitch can overflow to infinity. The
    engine then keeps the source's pitch, which the pool reset to 1 (seen on the harness).
- `ManySound..ctor`: `timeSpacing` 0.1, `clickSoundSpacing` 0.1, `attenuator` 0.9, `usePropPitch` 1, `attenuation`
  0 (the scene's sounds override all but `attenuation`; the scene's values are in the test's dump).
- `AudioSourcePool`: count 16 by the constructor (the scene's pools have 8). `Start` copies the first child
  `count - 1` times. `GetAudioSource` takes the next child round the ring (`(index + 1) % count`) with pan 0,
  pitch 1, volume 1, stopped.
- `UiAudio`: `Start` copies the first child 9 times (10 sources). `Play(clip, randomizePitch)` puts the clip on
  the next source round the ring, at pitch 1 or `Range(0.8, 1.2)`, and plays it.
- `ClickParticleSound` (ported, no harness test: the events are the particle system's). On the click debris's
  collisions it picks the fastest one faster than `lowestSpeed` (10), skipping the voxel just added, and makes a
  Debris sound at its point. On the water trigger's entering particles it picks the fastest one and makes a
  DebrisSplash at its position. Either way the volume is `speed / 10` (`PlaySound(type, pos, 0, volume)`).
- The harness learned `"ret": "f32"` for internal calls: `AudioSource.panStereo`'s managed getter is stripped.
