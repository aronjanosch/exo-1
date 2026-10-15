//! The weapons' power side: one capacitor that the WEAPONS grid group recharges, and a shot that
//! takes one shot's energy and starts the reload. No aiming and no enemies yet: the FIRE button on
//! GN-01 stands in for the tactical officer. The gun also heats while it fires and can jam; the
//! heat calls for a pause and a jam for a hand at the gun (GN-01's CLEAR JAM hold).
//!
//! Source: Barotrauma's coilgun (content/Items/Weapons/coilgun.xml:16) has `powerconsumption="1000.0"`
//! kW and `reload="0.25"` s. The turret checks the draw before a shot (Turret.cs:650-655
//! `HasPowerToShoot`) and takes it from the batteries (Turret.cs:776-789), then waits the reload
//! (Turret.cs:876 `Launch`). Our energy per shot is the kW held for the reload: 1000 kW × 0.25 s,
//! scaled by 0.4 (our 2000 kW reactor vs their 5000, as the battery) to 400 kW × 0.25 s = 100 kW·s.
//! Continuous fire then draws 100 kW·s / 0.25 s = 400 kW, the group's demand.
//!
//! Heat (r41): Barotrauma has no turret heat (grep of Turret.cs and the coilgun XML finds none), so
//! the numbers are ours. PULSAR's manual has none either: it says only that turrets "fire a single
//! shot, others are beam weapons" (manual_plain.txt, "Ship Turrets"). Its talent names WPNS_COOLING
//! and E_TURRET_COOLING_CREW_ENGINEER (concept repo `docs/research/pulsar-catalog.md`, Talents) show
//! that turret cooling exists there, but the dump has no values. The shape is ours: a shot adds
//! `heat_per_shot`, the gun cools `cool_per_s` at all times, reaching `overheat_at` locks it until the
//! heat falls below `unlock_at` (the gap is the hysteresis, so the gun does not flicker at the line).
//! TODO(initiator): every heat value.
//!
//! Jam (r41): Barotrauma's skill check. `ItemComponent.DegreeOfSuccess` turns skills into a 0..1
//! degree (Items/Components/ItemComponent.cs:713-727), and the repair roll uses that degree (concept
//! repo `docs/research/barotrauma-mech-damage-repair.md` §3.1). The repo's own port of the roll is
//! device_core's `Condition::fail_chance` (device_core/src/lib.rs:254-259): the base chance times
//! (1 - skill / required_skill), clamped. We use that linear shape, so skill 0 gives the base chance
//! and the required skill gives none. The §3.1 degree formula ((average + 100) / 200) would give 0.25
//! at skill 0 against a required 50, not 0; that is why the device port is the one followed here.
//! TODO(initiator): the base chance (0.03), the required skill (50) and whether a jammed shot still
//! goes (it does here: the round leaves, the breech sticks).
//!
//! The skill: no player has a weapons skill yet. `Weapons::new` takes 0 and `with_skill` sets one.
//! The only skill value in the code is the repair config's `skill` (exo_app/src/repair.rs, the
//! `RepairConfig` read at repair.rs:442): a real weapons skill would come from the gunner's skills
//! once the skill system reaches the crew (concept repo `docs/research/skills-mapping.md`, the proposal).
//!
//! Units: energy kW·s, power kW, time s, heat in units of the overheat line (1 is the line).
#[cfg(test)]
mod tests;

use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponsTuning {
    /// Shots the capacitor holds when full (`TODO(initiator)`: 6).
    pub capacity_shots: u8,
    /// kW a shot draws over its reload (Barotrauma coilgun 1000 × 0.4 = 400).
    pub shot_kw: f64,
    /// Reload after a shot, s (Barotrauma coilgun 0.25).
    pub reload_s: f64,
    /// Recharge at full power, kW: the WEAPONS group's demand while the capacitor is not full.
    pub recharge_kw: f64,
    /// Heat one shot adds, in units of the overheat line (`TODO(initiator)`: 0.18, so six shots in a
    /// row reach the line).
    pub heat_per_shot: f64,
    /// Heat the gun gives off, per second, at any time (`TODO(initiator)`: 0.08).
    pub cool_per_s: f64,
    /// Heat at which the gun locks (1.0, the line; the heat caps here).
    pub overheat_at: f64,
    /// Heat below which a locked gun works again (`TODO(initiator)`: 0.5).
    pub unlock_at: f64,
    /// Chance that a shot jams at skill 0 (`TODO(initiator)`: 0.03); 0 at `jam_required_skill`.
    pub jam_chance: f64,
    /// The skill at which the jam chance is 0 (device_core's required skill, 50).
    pub jam_required_skill: f64,
    /// Seconds of hold at the gun to clear a jam (`TODO(initiator)`: 3).
    pub clear_jam_s: f64,
}

impl Default for WeaponsTuning {
    fn default() -> WeaponsTuning {
        WeaponsTuning {
            capacity_shots: 6,
            shot_kw: 400.0,
            reload_s: 0.25,
            recharge_kw: 400.0,
            heat_per_shot: 0.18,
            cool_per_s: 0.08,
            overheat_at: 1.0,
            unlock_at: 0.5,
            jam_chance: 0.03,
            jam_required_skill: 50.0,
            clear_jam_s: 3.0,
        }
    }
}

/// Below this energy (kW·s) the capacitor counts as empty: the float sums of the steps.
const EPS: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub struct Weapons {
    tuning: WeaponsTuning,
    /// Energy in the capacitor, kW·s.
    stored: f64,
    /// Seconds of reload left (0 when ready).
    reload_left: f64,
    /// Heat, 0..`overheat_at`.
    heat: f64,
    /// Locked by heat until the heat falls below `unlock_at`.
    overheated: bool,
    /// Jammed by a shot until a hold clears it (`clear_jam`).
    jammed: bool,
    /// The gunner's weapons skill, 0..100 (0 until a skill exists).
    skill: f64,
    /// Seconds of clearing hold so far (reset by `stop_clearing`).
    clearing_s: f64,
    /// Draws the jams. Seeded, so the same seed jams on the same shots.
    rng: Rng,
}

impl Weapons {
    /// An empty capacitor, ready, cold, not jammed, at skill 0. `seed` seeds the jams.
    pub fn new(tuning: WeaponsTuning, seed: u64) -> Weapons {
        Weapons { tuning, stored: 0.0, reload_left: 0.0, heat: 0.0, overheated: false, jammed: false, skill: 0.0, clearing_s: 0.0, rng: Rng::new(seed) }
    }

    /// The same gun at the gunner's weapons skill (0..100).
    pub fn with_skill(mut self, skill: f64) -> Weapons {
        self.skill = skill.clamp(0.0, 100.0);
        self
    }

    /// Sets the gunner's skill (0..100): the bot's while a bot mans the gun (`bots.rs`), else 0.
    pub fn set_skill(&mut self, skill: f64) {
        self.skill = skill.clamp(0.0, 100.0);
    }

    pub fn tuning(&self) -> &WeaponsTuning {
        &self.tuning
    }

    /// Energy one shot takes, kW·s (`shot_kw × reload_s`).
    pub fn shot_kws(&self) -> f64 {
        self.tuning.shot_kw * self.tuning.reload_s
    }

    /// The capacitor's energy when full, kW·s.
    pub fn capacity_kws(&self) -> f64 {
        f64::from(self.tuning.capacity_shots) * self.shot_kws()
    }

    pub fn stored_kws(&self) -> f64 {
        self.stored
    }

    /// Whole shots in the capacitor, 0..=capacity_shots.
    pub fn shots(&self) -> u8 {
        let n = ((self.stored + EPS) / self.shot_kws()).floor();
        (n.max(0.0) as u8).min(self.tuning.capacity_shots)
    }

    /// Energy over capacity, 0..1.
    pub fn charge(&self) -> f64 {
        (self.stored / self.capacity_kws()).clamp(0.0, 1.0)
    }

    /// Seconds of reload left (0 when ready).
    pub fn reload_left(&self) -> f64 {
        self.reload_left
    }

    /// Heat, 0..`overheat_at` (GN-01's heat bar reads it as a share of the line).
    pub fn heat(&self) -> f64 {
        self.heat
    }

    /// Locked by heat: no shot until the heat is below `unlock_at`.
    pub fn overheated(&self) -> bool {
        self.overheated
    }

    /// Jammed: no shot until a hold clears it.
    pub fn jammed(&self) -> bool {
        self.jammed
    }

    /// The chance that a shot jams at the gun's skill: `jam_chance × (1 − skill / jam_required_skill)`,
    /// clamped (device_core's `fail_chance` shape).
    pub fn jam_chance(&self) -> f64 {
        if self.tuning.jam_required_skill <= 0.0 {
            return 0.0;
        }
        self.tuning.jam_chance * (1.0 - self.skill / self.tuning.jam_required_skill).clamp(0.0, 1.0)
    }

    /// The group's demand share: 1 while the capacitor is not full, else 0 (the room sets the
    /// WEAPONS demand to `recharge_kw` times this, so a full capacitor asks for nothing).
    pub fn demand_share(&self) -> f64 {
        if self.stored + EPS >= self.capacity_kws() { 0.0 } else { 1.0 }
    }

    /// Advances `dt` seconds: the reload runs down, the heat falls, and the capacitor takes
    /// `recharge_kw × power_factor` (the group's delivered share, 0..1) while it is not full. No
    /// power, no recharge. The gun cools with or without power.
    pub fn step(&mut self, dt: f64, power_factor: f64) {
        self.reload_left = (self.reload_left - dt).max(0.0);
        self.heat = (self.heat - self.tuning.cool_per_s * dt).max(0.0);
        if self.overheated && self.heat < self.tuning.unlock_at {
            self.overheated = false;
        }
        if self.demand_share() > 0.0 {
            let kw = self.tuning.recharge_kw * power_factor.clamp(0.0, 1.0);
            self.stored = (self.stored + kw * dt).min(self.capacity_kws());
        }
    }

    /// Fires a shot: refused while reloading, overheated, jammed or without one shot's energy. An
    /// accepted shot takes `shot_kws`, starts the reload and adds its heat; it may then jam (the
    /// round still goes).
    pub fn fire(&mut self) -> bool {
        if self.reload_left > 0.0 || self.overheated || self.jammed || self.stored + EPS < self.shot_kws() {
            return false;
        }
        self.stored = (self.stored - self.shot_kws()).max(0.0);
        self.reload_left = self.tuning.reload_s;
        self.heat = (self.heat + self.tuning.heat_per_shot).min(self.tuning.overheat_at);
        if self.heat >= self.tuning.overheat_at {
            self.overheated = true;
        }
        if self.rng.unit() < self.jam_chance() {
            self.jammed = true;
        }
        true
    }

    /// One step of the clearing hold (`dt` seconds, the player still holding at the gun). Returns
    /// true on the step that clears the jam (after `clear_jam_s` of hold). Nothing to clear: false.
    pub fn clear_jam(&mut self, dt: f64) -> bool {
        if !self.jammed {
            return false;
        }
        self.clearing_s += dt;
        if self.clearing_s + EPS >= self.tuning.clear_jam_s {
            self.jammed = false;
            self.clearing_s = 0.0;
            return true;
        }
        false
    }

    /// The hold is let go before the jam clears: the count starts over.
    pub fn stop_clearing(&mut self) {
        self.clearing_s = 0.0;
    }
}
