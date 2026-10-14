//! The fire's values, from `content/gameplay/fire/fire.json` (strict loader, like `reactor_core`:
//! unknown fields and wrong types are errors, and every error names the file and the field).
use crate::FireTuning;
use content_core::{parse_strict, File};
use serde::{Deserialize, Serialize};

/// The file's path under `content/gameplay/`.
pub const FIRE_PATH: &str = "fire/fire.json";

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FireSettings {
    /// The fire's physics (`fire_core::FireTuning`).
    pub fire: FireTuning,
    /// A fire this big (m) in the engine room or the corridor sets the ship's alarm to Danger.
    pub alarm_danger_size: f64,
    /// The size (m) of the dev button's fire.
    pub start_size: f64,
    /// The extinguisher's spray reach (m) and half-angle (degrees). TODO(initiator): start values.
    pub spray_reach: f64,
    pub spray_half_angle_deg: f64,
}

impl Default for FireSettings {
    fn default() -> FireSettings {
        FireSettings { fire: FireTuning::default(), alarm_danger_size: 1.0, start_size: 0.5, spray_reach: 4.0, spray_half_angle_deg: 20.0 }
    }
}

impl FireSettings {
    /// Parses and checks `fire/fire.json` from `files`. Every error names the field.
    pub fn load(files: &[File]) -> Result<FireSettings, Vec<String>> {
        let Some(f) = files.iter().find(|f| f.path == FIRE_PATH) else { return Err(vec![format!("{FIRE_PATH}: missing")]) };
        FireSettings::parse(&f.text)
    }

    /// Parses and checks the file's text.
    pub fn parse(text: &str) -> Result<FireSettings, Vec<String>> {
        let s = parse_strict::<FireSettings>(FIRE_PATH, text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        s.check(&mut e);
        if e.is_empty() { Ok(s) } else { Err(e) }
    }

    fn check(&self, e: &mut Vec<String>) {
        let mut need = |ok: bool, field: &str, rule: &str| {
            if !ok {
                e.push(format!("{FIRE_PATH}: {field}: {rule}"));
            }
        };
        let t = &self.fire;
        need(t.max_size > 0.0 && t.out_below > 0.0 && t.out_below < t.max_size, "fire.max_size", "must be > out_below > 0");
        need(t.grow_full > t.stall && t.stall >= 0.0 && t.grow_full <= 1.0, "fire.grow_full", "must be > stall >= 0 and <= 1");
        need(t.growth >= 0.0 && t.oxygen_use >= 0.0, "fire.growth", "growth and oxygen_use must be >= 0");
        need(t.spread_interval_s > 0.0, "fire.spread_interval_s", "must be > 0");
        need((0.0..=1.0).contains(&t.spread_chance), "fire.spread_chance", "must be 0..1");
        need(t.burn_cap_size > 0.0 && t.max_range >= 0.0 && t.range >= 0.0, "fire.burn_cap_size", "must be > 0, ranges >= 0");
        need(t.extinguish >= 0.0 && t.spray_radius >= 0.0, "fire.extinguish", "must be >= 0");
        need(t.air_flow >= 0.0 && t.leak_flow >= 0.0, "fire.air_flow", "must be >= 0");
        need(self.alarm_danger_size > 0.0, "alarm_danger_size", "must be > 0");
        need(self.start_size > t.out_below && self.start_size <= t.max_size, "start_size", "must be above out_below and within max_size");
        need(self.spray_reach > 0.0, "spray_reach", "must be > 0");
        need(self.spray_half_angle_deg > 0.0 && self.spray_half_angle_deg < 90.0, "spray_half_angle_deg", "must be between 0 and 90 degrees");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped file parses and checks, and holds the defaults.
    #[test]
    fn the_shipped_file_loads_and_is_the_default() {
        let text = include_str!("../../../content/gameplay/fire/fire.json");
        let s = FireSettings::parse(text).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(s, FireSettings::default());
    }

    #[test]
    fn unknown_fields_and_bad_values_are_named() {
        let good = include_str!("../../../content/gameplay/fire/fire.json");
        let typo = good.replacen("\"spread_chance\"", "\"spread_chanse\"", 1);
        let e = FireSettings::parse(&typo).unwrap_err();
        assert!(e[0].contains(FIRE_PATH) && e[0].contains("spread_chanse"), "{e:?}");
        let bad = good.replacen("\"spread_chance\": 0.15", "\"spread_chance\": 1.5", 1);
        let e = FireSettings::parse(&bad).unwrap_err();
        assert!(e.iter().any(|m| m.contains("fire.spread_chance")), "{e:?}");
        let missing = FireSettings::load(&[]).unwrap_err();
        assert!(missing[0].contains("missing"));
    }
}
