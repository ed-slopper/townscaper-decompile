//! `Placemaker.ClickParticleSound` (row G6a): the debris of a click hitting the town or the water makes a sound.

use crate::unity::vector::{sqrt_f64, Vector3};

/// `ClickParticleSound..ctor` 0x201EA0: `lowestSpeed`.
pub const LOWEST_SPEED: f32 = 10.0;

/// One `ParticleCollisionEvent`: where, how fast, and whether it hit the voxel just added
/// (`master.maker.lastAddedVoxel`'s object).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleCollision {
    pub intersection: Vector3,
    pub velocity: Vector3,
    pub hit_last_added_voxel: bool,
}

/// `ClickParticleSound.OnParticleCollision(other)` 0x201940: the fastest collision above `lowestSpeed` (not on the
/// voxel just added) makes a debris sound there, as loud as a tenth of its speed: `(SoundType.Debris, position,
/// volume)` for `ManySoundsManager.PlaySound(type, position, 0, volume)`.
pub fn on_particle_collision(lowest_speed: f32, events: &[ParticleCollision]) -> Option<(i32, Vector3, f32)> {
    let floor = lowest_speed * lowest_speed;
    let mut best = floor;
    let mut at = Vector3::ZERO;
    for e in events {
        if e.hit_last_added_voxel {
            continue;
        }
        let sq = e.velocity.sqr_magnitude();
        if best < sq {
            best = sq;
            at = e.intersection;
        }
    }
    (floor < best).then(|| (crate::placemaker::audio::many_sounds_manager::sound_type::DEBRIS, at, sqrt_f64(best) * 0.1))
}

/// `ClickParticleSound.OnParticleTrigger` 0x201C60: the same over the particles entering the water trigger
/// (`GetTriggerParticles(Enter)`: position, velocity), a splash.
pub fn on_particle_trigger(lowest_speed: f32, particles: &[(Vector3, Vector3)]) -> Option<(i32, Vector3, f32)> {
    let floor = lowest_speed * lowest_speed;
    let mut best = floor;
    let mut at = Vector3::ZERO;
    for &(position, velocity) in particles {
        let sq = velocity.sqr_magnitude();
        if best < sq {
            best = sq;
            at = position;
        }
    }
    (floor < best).then(|| (crate::placemaker::audio::many_sounds_manager::sound_type::DEBRIS_SPLASH, at, sqrt_f64(best) * 0.1))
}
