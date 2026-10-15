use content_core::{parse_strict_commented, File};
use serde::{Deserialize, Serialize};

use crate::battery::BatteryTuning;
use crate::director::DirectorTuning;
use crate::engine_room::RoomTuning;
use crate::grid::GridTuning;
use crate::reactor::{ReactorTuning, RodKinds};

pub const REACTOR_PATH: &str = "reactor/reactor.json";

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub start_rods: usize,
    /// The fuel rod kinds by id (`standard` is the one `start_rods` puts in).
    pub rods: RodKinds,
    pub reactor: ReactorTuning,
    pub room: RoomTuning,
    pub grid: GridTuning,
    /// The battery behind the reactor (`battery` in reactor.json).
    pub battery: BatteryTuning,
    /// The status board's graph: points per curve (Barotrauma 25).
    pub graph_points: usize,
    /// Seconds between two graph samples (Barotrauma 0.5).
    pub graph_interval_s: f64,
    /// The incident director (`director` in reactor.json, r38).
    pub director: DirectorTuning,
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning { start_rods: 2, rods: RodKinds::default(), reactor: ReactorTuning::default(), room: RoomTuning::default(), grid: GridTuning::default(), battery: BatteryTuning::default(), graph_points: 25, graph_interval_s: 0.5, director: DirectorTuning::default() }
    }
}

impl Tuning {
    /// Parses and checks `reactor/reactor.json` from `files`. Every error names the field.
    pub fn load(files: &[File]) -> Result<Tuning, Vec<String>> {
        let Some(f) = files.iter().find(|f| f.path == REACTOR_PATH) else { return Err(vec![format!("{REACTOR_PATH}: missing")]) };
        Tuning::parse(&f.text)
    }

    /// Parses and checks the file's text.
    pub fn parse(text: &str) -> Result<Tuning, Vec<String>> {
        let t = parse_strict_commented::<Tuning>(REACTOR_PATH, text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        t.check(&mut e);
        if e.is_empty() { Ok(t) } else { Err(e) }
    }

    fn check(&self, e: &mut Vec<String>) {
        let mut need = |ok: bool, field: &str, rule: &str| {
            if !ok {
                e.push(format!("{REACTOR_PATH}: {field}: {rule}"));
            }
        };
        let r = &self.reactor;
        need(r.slots >= 1, "reactor.slots", "must be at least 1");
        need(self.start_rods <= r.slots, "start_rods", "must fit in reactor.slots");
        need(self.graph_points >= 1 && self.graph_interval_s > 0.0, "graph_points", "must be >= 1, graph_interval_s > 0");
        let d = &self.director;
        need(d.min_interval_s > 0.0 && d.max_interval_s >= d.min_interval_s, "director.*_interval_s", "min > 0, max >= min");
        need((0.0..=1.0).contains(&d.threshold) && d.rise_per_s > 0.0 && d.fall_per_s > 0.0, "director", "threshold 0..=1, rise_per_s and fall_per_s > 0");
        need(self.rods.0.contains_key("standard"), "rods.standard", "the rod start_rods uses must be listed");
        for (id, k) in &self.rods.0 {
            need(k.heat > 0.0 && k.life > 0.0 && k.life <= 100.0, &format!("rods.{id}"), "heat > 0, life in 0..=100");
        }
        need(r.max_output_kw > 0.0, "reactor.max_output_kw", "must be > 0");
        need(self.battery.capacity_kwmin > 0.0 && self.battery.max_output_kw >= 0.0 && self.battery.max_recharge_kw >= 0.0, "battery", "capacity_kwmin > 0, the powers >= 0");
        need(self.battery.efficiency > 0.0 && self.battery.efficiency <= 1.0, "battery.efficiency", "must be > 0 and <= 1");
        need(self.room.aux_output_kw >= 0.0, "room.aux_output_kw", "must be >= 0");
        need(self.room.oxygen_generator_rate >= 0.0, "room.oxygen_generator_rate", "must be >= 0");
        need(self.room.breath_m3_per_s >= 0.0, "room.breath_m3_per_s", "must be >= 0");
        need(self.room.breach_area_m2 >= 0.0, "room.breach_area_m2", "must be >= 0");
        need((0.0..=1.0).contains(&self.room.blast_breach_damage), "room.blast_breach_damage", "must be 0..1");
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
        need(self.room.wander_every_s > 0.0, "room.wander_every_s", "must be > 0");
        need((0.0..=1.0).contains(&self.room.wander_min), "room.wander_min", "must be 0..1");
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

    #[test]
    fn unknown_fields_and_bad_values_are_named() {
        let good = include_str!("../../../content/gameplay/reactor/reactor.json");
        let typo = good.replacen("\"start_rods\"", "\"start_rod\"", 1);
        let e = Tuning::parse(&typo).unwrap_err();
        assert!(e[0].contains(REACTOR_PATH) && e[0].contains("start_rod"), "{e:?}");
        let bad = good.replacen("\"max_output_kw\": 2000.0", "\"max_output_kw\": 0.0", 1);
        let e = Tuning::parse(&bad).unwrap_err();
        assert!(e.iter().any(|m| m.contains("reactor.max_output_kw")), "{e:?}");
        let missing = Tuning::load(&[]).unwrap_err();
        assert!(missing[0].contains("missing"));
    }
}
