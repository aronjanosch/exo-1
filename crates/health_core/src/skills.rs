//! Skills: what a player can do at a machine (repair) or on a body (treatment). Kept per player,
//! cut back when they die (#254). Gains come from the glue (`exo_app::repair`).
//!
//! Death loss, ported from Barotrauma (`/home/agent/exo-research/barotrauma/source/Barotrauma/`):
//! - `BarotraumaShared/SharedSource/Networking/RespawnManager.cs:401-409` (`GetReducedSkill`): a
//!   skill at or below the job's default level stays; above it, the skill falls towards that
//!   level by the percentage (`Lerp(current, default, pct / 100)`).
//! - `BarotraumaShared/SharedSource/Networking/ServerSettings.cs:481-482`: the percentage is
//!   `SkillLossPercentageOnDeath`, default 20.
//! - Not ported: the extra 10 % of `SkillLossPercentageOnImmediateRespawn` (RespawnManager.cs:25),
//!   which Barotrauma takes when a player respawns at once. Ours respawns on the timer only.
//!   TODO(initiator): an immediate respawn choice, or not.
//!
//! The default level is `START_SKILL` for every skill here: the old constants of the repair
//! (`repair.json` "skill") and the treatment (`TREAT_SKILL`). TODO(initiator): skills per role.
use std::collections::BTreeMap;

/// The level a new player starts at, and the level a death does not take a skill below.
pub const START_SKILL: f64 = 50.0;
pub const SKILL_MAX: f64 = 100.0;
/// Barotrauma's `SkillLossPercentageOnDeath` default (ServerSettings.cs:481-482). TODO(initiator): tune.
pub const SKILL_LOSS_ON_DEATH_PCT: f64 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    Mechanical,
    /// No gain yet (no source in the ticket). TODO(initiator): what raises it.
    Electrical,
    /// No gain yet; treatment still uses `TREAT_SKILL`. TODO(initiator): what raises it.
    Medical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skills {
    pub mechanical: f64,
    pub electrical: f64,
    pub medical: f64,
}

impl Default for Skills {
    fn default() -> Skills {
        Skills { mechanical: START_SKILL, electrical: START_SKILL, medical: START_SKILL }
    }
}

impl Skills {
    pub fn get(&self, s: Skill) -> f64 {
        match s {
            Skill::Mechanical => self.mechanical,
            Skill::Electrical => self.electrical,
            Skill::Medical => self.medical,
        }
    }

    fn slot(&mut self, s: Skill) -> &mut f64 {
        match s {
            Skill::Mechanical => &mut self.mechanical,
            Skill::Electrical => &mut self.electrical,
            Skill::Medical => &mut self.medical,
        }
    }

    pub fn gain(&mut self, s: Skill, amount: f64) {
        let v = self.slot(s);
        *v = (*v + amount).clamp(0.0, SKILL_MAX);
    }

    /// Barotrauma's `GetReducedSkill`: each skill above the start level falls towards it by `pct` percent of the way.
    pub fn lose_on_death(&mut self, pct: f64) {
        for s in [Skill::Mechanical, Skill::Electrical, Skill::Medical] {
            let v = self.slot(s);
            if *v > START_SKILL {
                *v = START_SKILL + (*v - START_SKILL) * (1.0 - pct / 100.0);
            }
        }
    }
}

/// Every player's skills, by player id. A player without a record has the start level.
#[derive(Clone, Debug)]
pub struct CrewSkills<K: Ord> {
    by: BTreeMap<K, Skills>,
}

impl<K: Ord + Copy> Default for CrewSkills<K> {
    fn default() -> Self {
        CrewSkills::new()
    }
}

impl<K: Ord + Copy> CrewSkills<K> {
    pub fn new() -> Self {
        CrewSkills { by: BTreeMap::new() }
    }

    pub fn of(&self, id: K) -> Skills {
        self.by.get(&id).copied().unwrap_or_default()
    }

    pub fn gain(&mut self, id: K, s: Skill, amount: f64) {
        self.by.entry(id).or_default().gain(s, amount);
    }

    /// The player died: their skills above the start level fall by `pct`.
    pub fn died(&mut self, id: K, pct: f64) {
        self.by.entry(id).or_default().lose_on_death(pct);
    }

    pub fn clear(&mut self) {
        self.by.clear();
    }
}
