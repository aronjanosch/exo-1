use content_core::{parse_fragments, File};
use serde::{Deserialize, Serialize};

use crate::airlock::AirlockTuning;
use crate::bots::BotTuning;
use crate::enemy::EnemyTuning;
use crate::battery::BatteryTuning;
use crate::room_alarm::AlarmTuning;
use crate::weapons::WeaponsTuning;
use crate::director::DirectorTuning;
use crate::engine_room::RoomTuning;
use crate::grid::GridTuning;
use crate::helm::HelmTuning;
use crate::hull::{HitsTuning, HullTuning};
use crate::life_support::LifeSupportTuning;
use crate::jump::JumpTuning;
use crate::startup::StartupTuning;
use crate::reactor::{ReactorTuning, RodKinds};
use crate::shield::ShieldTuning;

/// The files of the ship's tuning and the blocks each owner holds: engineering (the reactor room's
/// machines and the grid), the bridge (shield, jump drive, helm), the gunnery (weapons), the hull
/// (walls, hits, alarms, airlock, life support), the crew bots, and the incidents (director, enemy).
/// A block in another owner's file is an error (`content_core::parse_fragments`).
pub const FILES: [(&str, &[&str]); 7] = [
    ("engineering/engineering.json", &["start_rods", "graph_points", "graph_interval_s", "rods", "reactor", "battery", "grid", "startup", "room"]),
    ("bridge/bridge.json", &["shield", "jump", "helm"]),
    ("gunnery/gunnery.json", &["weapons"]),
    ("hull/hull.json", &["hull", "hits", "alarm", "airlock", "life_support"]),
    ("crew/bots.json", &["bots"]),
    ("incidents/director.json", &["director"]),
    ("incidents/enemy.json", &["enemy"]),
];

/// What the errors call the tuning (the files' own paths name the file errors).
const WHAT: &str = "ship tuning";

/// The shipped files, embedded: the tests and the scenarios load them as the game does.
pub fn shipped_files() -> Vec<File> {
    macro_rules! shipped {
        ($($p:literal),*) => { vec![$(File::new($p, include_str!(concat!("../../../content/gameplay/", $p)))),*] };
    }
    shipped!("engineering/engineering.json", "bridge/bridge.json", "gunnery/gunnery.json", "hull/hull.json", "crew/bots.json", "incidents/director.json", "incidents/enemy.json")
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub start_rods: usize,
    /// The fuel rod kinds by id (`standard` is the one `start_rods` puts in).
    pub rods: RodKinds,
    pub reactor: ReactorTuning,
    pub room: RoomTuning,
    pub grid: GridTuning,
    /// The battery behind the reactor (`battery` in engineering/engineering.json).
    pub battery: BatteryTuning,
    /// The weapons' capacitor and the WEAPONS group's recharge (`weapons` in gunnery/gunnery.json, r40).
    pub weapons: WeaponsTuning,
    /// The status board's graph: points per curve (Barotrauma 25).
    pub graph_points: usize,
    /// Seconds between two graph samples (Barotrauma 0.5).
    pub graph_interval_s: f64,
    /// The incident director (`director` in incidents/director.json, r38).
    pub director: DirectorTuning,
    /// The shield (`shield` in bridge/bridge.json, r40).
    pub shield: ShieldTuning,
    /// The room alarms (`alarm` in hull/hull.json, r40, #165).
    pub alarm: AlarmTuning,
    /// The hit presets of the HIT LIGHT and HIT HEAVY dev faults (`hits` in hull/hull.json, r40).
    pub hits: HitsTuning,
    /// The jump drive's charge (`jump` in bridge/bridge.json, r40, #252).
    pub jump: JumpTuning,
    /// The ship's cold start (`startup` in engineering/engineering.json, r42, #268).
    pub startup: StartupTuning,
    /// The airlock and the outside (`airlock` in hull/hull.json, r40).
    pub airlock: AirlockTuning,
    /// The enemy ship of the ambush (`enemy` in incidents/enemy.json, r41).
    pub enemy: EnemyTuning,
    /// The bots that man the empty stations (`bots` in crew/bots.json, r42, #253).
    pub bots: BotTuning,
    /// The walls (`hull` in `hull/hull.json`).
    pub hull: HullTuning,
    /// The air (`life_support` in `hull/hull.json`).
    pub life_support: LifeSupportTuning,
    /// The helm's dev wander (`helm` in `bridge/bridge.json`).
    pub helm: HelmTuning,
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning { start_rods: 2, rods: RodKinds::default(), reactor: ReactorTuning::default(), room: RoomTuning::default(), grid: GridTuning::default(), battery: BatteryTuning::default(), graph_points: 25, graph_interval_s: 0.5, director: DirectorTuning::default(), shield: ShieldTuning::default(), alarm: AlarmTuning::default(), weapons: WeaponsTuning::default(), hits: HitsTuning::default(), jump: JumpTuning::default(), startup: StartupTuning::default(), airlock: AirlockTuning::default(), enemy: EnemyTuning::default(), bots: BotTuning::default(), hull: HullTuning::default(), life_support: LifeSupportTuning::default(), helm: HelmTuning::default() }
    }
}

impl Tuning {
    /// Parses and checks the ship's tuning from its `FILES` in `files`. Every error names the field.
    pub fn load(files: &[File]) -> Result<Tuning, Vec<String>> {
        let t = parse_fragments::<Tuning>(WHAT, files, &FILES)?;
        let mut e = Vec::new();
        t.check(&mut e);
        if e.is_empty() { Ok(t) } else { Err(e) }
    }

    /// The shipped values (`shipped_files`).
    pub fn shipped() -> Tuning {
        Tuning::load(&shipped_files()).unwrap_or_else(|e| panic!("the shipped ship tuning: {e:?}"))
    }

    fn check(&self, e: &mut Vec<String>) {
        let mut need = |ok: bool, field: &str, rule: &str| {
            if !ok {
                e.push(format!("{WHAT}: {field}: {rule}"));
            }
        };
        let r = &self.reactor;
        need(r.slots >= 1, "reactor.slots", "must be at least 1");
        need(self.alarm.low_o2 < self.alarm.low_o2_clear, "alarm.low_o2_clear", "must be above alarm.low_o2 (the band)");
        need(self.start_rods <= r.slots, "start_rods", "must fit in reactor.slots");
        need(self.graph_points >= 1 && self.graph_interval_s > 0.0, "graph_points", "must be >= 1, graph_interval_s > 0");
        let d = &self.director;
        need(d.min_interval_s > 0.0 && d.max_interval_s >= d.min_interval_s, "director.*_interval_s", "min > 0, max >= min");
        need((0.0..=1.0).contains(&d.threshold) && d.rise_per_s > 0.0 && d.fall_per_s > 0.0, "director", "threshold 0..=1, rise_per_s and fall_per_s > 0");
        let sh = &self.shield;
        need(sh.max > 0.0 && sh.charge_rate_max >= 0.0 && sh.recovery_delay_s >= 0.0 && sh.quantum_offline_s >= 0.0, "shield", "max > 0, charge_rate_max, recovery_delay_s, quantum_offline_s >= 0");
        need((0.0..=1.0).contains(&sh.deflection) && (0.0..=1.0).contains(&sh.quantum_min) && (0.0..=1.0).contains(&sh.upkeep), "shield", "deflection, quantum_min and upkeep 0..=1");
        need(self.airlock.cycle_s > 0.0, "airlock.cycle_s", "must be > 0");
        need((0.0..=1.0).contains(&self.airlock.pump_share), "airlock.pump_share", "must be 0..1");
        need(self.rods.0.contains_key("standard"), "rods.standard", "the rod start_rods uses must be listed");
        for (id, k) in &self.rods.0 {
            need(k.heat > 0.0 && k.life > 0.0 && k.life <= 100.0, &format!("rods.{id}"), "heat > 0, life in 0..=100");
        }
        need(r.max_output_kw > 0.0, "reactor.max_output_kw", "must be > 0");
        need(self.battery.capacity_kwmin > 0.0 && self.battery.max_output_kw >= 0.0 && self.battery.max_recharge_kw >= 0.0, "battery", "capacity_kwmin > 0, the powers >= 0");
        need(self.battery.efficiency > 0.0 && self.battery.efficiency <= 1.0, "battery.efficiency", "must be > 0 and <= 1");
        need(self.weapons.capacity_shots >= 1 && self.weapons.shot_kw > 0.0 && self.weapons.reload_s > 0.0, "weapons", "capacity_shots >= 1, shot_kw and reload_s > 0");
        need(self.weapons.recharge_kw >= 0.0, "weapons.recharge_kw", "must be >= 0");
        need(self.weapons.heat_per_shot >= 0.0 && self.weapons.cool_per_s >= 0.0, "weapons.heat_per_shot", "heat_per_shot and cool_per_s must be >= 0");
        need(self.weapons.overheat_at > 0.0 && (0.0..self.weapons.overheat_at).contains(&self.weapons.unlock_at), "weapons.unlock_at", "unlock_at must be below overheat_at (> 0)");
        need((0.0..=1.0).contains(&self.weapons.jam_chance) && self.weapons.jam_required_skill > 0.0, "weapons.jam_chance", "jam_chance must be 0..1, jam_required_skill > 0");
        need(self.weapons.clear_jam_s > 0.0, "weapons.clear_jam_s", "must be > 0");
        need(self.startup.boot_s >= 0.0, "startup", "boot_s >= 0");
        need(self.jump.prep_s >= 0.0 && self.jump.charge_s > 0.0 && self.jump.active_s > 0.0, "jump", "prep_s >= 0, charge_s > 0, active_s > 0");
        need(self.room.aux_output_kw >= 0.0, "room.aux_output_kw", "must be >= 0");
        need(self.life_support.oxygen_generator_rate >= 0.0, "life_support.oxygen_generator_rate", "must be >= 0");
        need(self.life_support.breath_m3_per_s >= 0.0, "life_support.breath_m3_per_s", "must be >= 0");
        need(self.hull.breach_area_m2 >= 0.0, "hull.breach_area_m2", "must be >= 0");
        need(self.hull.hit_fire_size >= 0.0, "hull.hit_fire_size", "must be >= 0");
        for (name, h) in [("hits.light", &self.hits.light), ("hits.heavy", &self.hits.heavy)] {
            need(h.structure >= 0.0 && h.device_damage >= 0.0 && h.radius > 0.0, name, "structure and device_damage >= 0, radius > 0");
            need((0.0..=1.0).contains(&h.fire_chance), &format!("{name}.fire_chance"), "must be 0..1");
        }
        need((0.0..=1.0).contains(&self.hull.blast_breach_damage), "hull.blast_breach_damage", "must be 0..1");
        let en = &self.enemy;
        need(en.shield.max > 0.0 && en.hull > 0.0 && en.shot_damage >= 0.0, "enemy", "shield.max and hull > 0, shot_damage >= 0");
        need(en.fire_every_s > 0.0 && en.first_shot_s >= 0.0, "enemy.fire_every_s", "must be > 0, first_shot_s >= 0");
        need((0.0..=1.0).contains(&en.light_share), "enemy.light_share", "must be 0..1");
        need(r.fuel_burn >= 0.0 && r.temp_rate > 0.0, "reactor.temp_rate", "must be > 0, fuel_burn >= 0");
        need(r.temp_stiffness > 0.0 && r.temp_damping >= 0.0, "reactor.temp_stiffness", "must be > 0, temp_damping >= 0");
        need(r.fission_lag_s > 0.0 && r.heat_output_factor >= 0.0, "reactor.fission_lag_s", "must be > 0, heat_output_factor >= 0");
        need(r.decay_lag_s > 0.0, "reactor.decay_lag_s", "must be > 0");
        need(r.boost_amount >= 0.0 && r.boost_decay > 0.0, "reactor.boost_*", "boost_amount >= 0, boost_decay > 0");
        let b = &r.blast;
        need(b.min_fuel >= 0.0 && b.device_damage >= 0.0 && b.burn >= 0.0 && b.stun_s >= 0.0, "reactor.blast", "min_fuel, device_damage, burn, stun_s >= 0");
        need(b.range_m >= 0.0 && b.radiation_range_m >= 0.0 && b.radiation >= 0.0, "reactor.blast", "range_m, radiation_range_m, radiation >= 0");
        need(r.fire_delay_s > 0.0 && r.meltdown_delay_s > 0.0, "reactor.*_delay_s", "must be > 0");
                need(r.auto_turbine_speed > 0.0 && r.auto_fission_speed > 0.0, "reactor.auto_*_speed", "must be > 0");
        need(r.alarm_hysteresis >= 0.0 && (0.0..=100.0).contains(&r.fuel_low), "reactor.fuel_low", "must be 0..100");
        for (name, d) in [("reactor.condition", &r.condition), ("room.pump", &self.room.pump), ("room.junction", &self.room.junction), ("room.oxygen_generator", &self.room.oxygen_generator)] {
            need(d.max > 0.0 && d.broken_below >= 0.0 && d.broken_below < d.repair_threshold && d.repair_threshold <= d.max, name, "must be 0 <= broken_below < repair_threshold <= max");
            need((0.0..=1.0).contains(&d.fail_chance), &format!("{name}.fail_chance"), "must be 0..1");
        }
        need((0.0..=100.0).contains(&self.room.ecool_tank_cost) && self.room.ecool_duration_s > 0.0, "room.ecool_*", "ecool_tank_cost in 0..=100, ecool_duration_s > 0");
        need(self.room.ecool_strength >= 0.0 && self.room.ecool_cooldown_s >= 0.0, "room.ecool_*", "ecool_strength and ecool_cooldown_s >= 0");
        need(self.room.overvolt_ratio > 0.0, "room.overvolt_ratio", "must be > 0");
        need((0.0..=1.0).contains(&self.room.break_fire_chance), "room.break_fire_chance", "must be 0..1");
        need(self.room.overvolt_hits_per_s >= 0.0, "room.overvolt_hits_per_s", "must be >= 0");
        need(self.helm.wander_every_s > 0.0, "helm.wander_every_s", "must be > 0");
        need((0.0..=1.0).contains(&self.helm.wander_min), "helm.wander_min", "must be 0..1");
        for g in crate::grid::Group::ALL {
            let s = self.grid.group(g);
            let name = format!("grid.{g:?}");
            need(s.demand_kw >= 0.0, &format!("{name}.demand_kw"), "must be >= 0");
            need((0.0..=1.0).contains(&s.min_ratio), &format!("{name}.min_ratio"), "must be 0..1");
            need((0.0..=1.0).contains(&s.limit), &format!("{name}.limit"), "must be 0..1");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped files with `from` replaced by `to` in the file at `path`.
    fn edited(path: &str, from: &str, to: &str) -> Vec<File> {
        let mut files = shipped_files();
        let f = files.iter_mut().find(|f| f.path == path).unwrap();
        assert!(f.text.contains(from), "{path} has no {from}");
        f.text = f.text.replacen(from, to, 1);
        files
    }

    #[test]
    fn unknown_fields_and_bad_values_are_named() {
        let e = Tuning::load(&edited("engineering/engineering.json", "\"start_rods\"", "\"start_rod\"")).unwrap_err();
        assert!(e[0].contains("start_rod"), "{e:?}");
        let e = Tuning::load(&edited("engineering/engineering.json", "\"max_output_kw\": 2000.0", "\"max_output_kw\": 0.0")).unwrap_err();
        assert!(e.iter().any(|m| m.contains("reactor.max_output_kw")), "{e:?}");
        let missing = Tuning::load(&[]).unwrap_err();
        assert!(missing[0].contains("missing"));
    }

    /// A block sits in its owner's file only: the airlock is the hull's, not the bridge's.
    #[test]
    fn a_block_in_the_wrong_owners_file_is_refused() {
        let mut files = shipped_files();
        let hull = files.iter_mut().find(|f| f.path == "hull/hull.json").unwrap();
        hull.text = hull.text.replacen("\"airlock\"", "\"jump\"", 1);
        let e = Tuning::load(&files).unwrap_err();
        assert!(e.iter().any(|m| m.contains("hull/hull.json: jump: belongs in bridge/bridge.json")), "{e:?}");
    }

    /// r40: the airlock block is its own, with its values checked.
    #[test]
    fn the_airlock_block_is_parsed_and_checked() {
        let t = Tuning::shipped();
        assert!(t.airlock.cycle_s == 6.0 && t.airlock.pump_share == 0.95);
        let e = Tuning::load(&edited("hull/hull.json", "\"cycle_s\": 6.0", "\"cycle_s\": 0.0")).unwrap_err();
        assert!(e.iter().any(|m| m.contains("airlock.cycle_s")), "{e:?}");
        assert!(Tuning::load(&edited("hull/hull.json", "\"pump_share\": 0.95", "\"pump_share\": 1.5")).unwrap_err().iter().any(|m| m.contains("airlock.pump_share")));
    }
}
