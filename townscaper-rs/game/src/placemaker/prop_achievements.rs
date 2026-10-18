//! `Placemaker.Props.PropAchievements` (row E10d): counts of shown props per achievement (a `PropNode`'s
//! `achievement`, counted by `RecursiveCheck` through [`crate::placemaker::prop_placer::PropHost::achievement`]);
//! `TriggerAchievements` reports each counted one to the platform once.

/// `PropAchievements.Achievement` (a byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Achievement {
    None = 0,
    Garden = 1,
    Propeller = 2,
    Lighthouse = 3,
    Spire = 4,
    GardenSculpture = 5,
    Hook = 6,
    ChurchDoor = 7,
    Count = 8,
}

impl Achievement {
    /// The enum's `ToString` (the platform's achievement id).
    pub fn name(v: u8) -> String {
        match v {
            0 => "None",
            1 => "Garden",
            2 => "Propeller",
            3 => "Lighthouse",
            4 => "Spire",
            5 => "GardenSculpture",
            6 => "Hook",
            7 => "ChurchDoor",
            8 => "Count",
            // an undefined value prints as its number
            _ => return v.to_string(),
        }
        .to_string()
    }
}

/// What `TriggerAchievements` calls outside.
pub trait AchievementHost {
    /// `Debug.Log`.
    fn log(&mut self, _message: &str) {}
    /// `Platform.SDK.SetAchievementProgress(new AchievementID(id), progress)`.
    fn set_achievement_progress(&mut self, id: &str, progress: f32);
}

/// `Placemaker.Props.PropAchievements`: `propCounts`, one per achievement.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropAchievements {
    pub prop_counts: Vec<i32>,
}

impl PropAchievements {
    /// `PropAchievements.OnStart` 0x704D90: zeros appended until there are nine counts.
    pub fn on_start(&mut self) {
        while self.prop_counts.len() <= 8 {
            self.prop_counts.push(0);
        }
    }

    /// `PropAchievements.PropEnabled` 0x704E70 (the original throws past the end).
    pub fn prop_enabled(&mut self, achievement: u8) {
        let c = self.prop_counts.get_mut(achievement as usize).expect("ArgumentOutOfRangeException");
        *c = c.wrapping_add(1);
    }

    /// `PropAchievements.PropDisabled` 0x704E00.
    pub fn prop_disabled(&mut self, achievement: u8) {
        let c = self.prop_counts.get_mut(achievement as usize).expect("ArgumentOutOfRangeException");
        *c = c.wrapping_sub(1);
    }

    /// `PropAchievements.TriggerAchievements` 0x704EF0: each achievement 1..=7 with a positive count is logged
    /// ("Achievement: " + its name), set to progress 1 on the platform, and its count lowered by a million (so it
    /// fires once).
    pub fn trigger_achievements(&mut self, host: &mut dyn AchievementHost) {
        for k in 1..8usize {
            let c = *self.prop_counts.get(k).expect("ArgumentOutOfRangeException");
            if 0 < c {
                let name = Achievement::name(k as u8);
                host.log(&format!("Achievement: {name}"));
                host.set_achievement_progress(&name, 1.0);
                self.prop_counts[k] = c.wrapping_sub(1_000_000);
            }
        }
    }
}
