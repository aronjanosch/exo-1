//! The `tuning` record of `heat/` (#227): one JSON file, checked when loaded. Every number is a
//! placeholder (TODO(initiator)).
use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::ViolationKind;
use serde::Deserialize;

pub const FOLDER: &str = "heat";
/// The star notices: a pool of lines each, with `{stars}` for the new level.
pub const NOTICE_UP: &str = "notice.heat.up";
pub const NOTICE_DOWN: &str = "notice.heat.down";
pub const STARS_MAX: u8 = 5;
/// Seconds without anything illegal after which the clean decay rate applies.
/// TODO(initiator): 5 minutes is a starting value.
pub const CLEAN_AFTER_S: f64 = 300.0;

/// Heat points per violation kind.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViolationPoints {
    pub no_clearance: f64,
    pub wrong_pad: f64,
    pub speeding: f64,
    pub fled_scan: f64,
}

impl ViolationPoints {
    pub fn of(&self, kind: ViolationKind) -> f64 {
        match kind {
            ViolationKind::NoClearance => self.no_clearance,
            ViolationKind::WrongPad => self.wrong_pad,
            ViolationKind::Speeding => self.speeding,
            ViolationKind::FledScan => self.fled_scan,
        }
    }
}

/// The heat tuning. Points are `f64`, money is whole credits. `scan_chance` and `talk_chance` have
/// one entry per star (0 to 5), checked to be six long.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub id: String,
    /// Points per star; stars are points / this, capped at 5.
    pub points_per_star: f64,
    pub max_points: f64,
    /// Points lost per minute while something illegal happened lately.
    pub decay_per_min: f64,
    /// Points lost per minute after a clean scan or 5 quiet minutes (`CLEAN_AFTER_S`).
    pub decay_per_min_clean: f64,
    pub violation_points: ViolationPoints,
    /// Points per settled product deal.
    pub deal_points: f64,
    /// Chance of a scan at an official pad, per star. (Serde gives no field name for a wrong
    /// array length, so this is a list checked by the loader.)
    pub scan_chance: Vec<f64>,
    pub jammer_factor: f64,
    pub fine_per_crate: i64,
    pub fine_base: i64,
    pub bribe_base: i64,
    /// Chance that talk works, per star.
    pub talk_chance: Vec<f64>,
    pub talk_fail_fine_factor: f64,
    pub talk_fail_points: f64,
    pub flee_points: f64,
    /// Points change of a scan that comes out clean or is complied with (negative: a relief).
    pub comply_points: f64,
    /// From this star on a scan may find the hidden hold.
    pub thorough_from_stars: u8,
    pub thorough_chance: f64,
    /// Money per heat point, for paying down at a fixer.
    pub paydown_per_point: i64,
    /// Air patrols (#231): from this star level on a town's patrol may hail a hot ship.
    /// TODO(initiator): 2 is a starting value.
    pub patrol_from_stars: u8,
    /// Seconds spent inside an official airspace per patrol roll. TODO(initiator)
    pub patrol_check_s: f64,
    /// Chance per roll = stars x this, capped at 0.9 (in `patrol`). TODO(initiator)
    pub patrol_chance_per_star: f64,
    /// Speed (m/s) under which the ship counts as halted. TODO(initiator)
    pub patrol_halt_speed_ms: f64,
    /// Seconds the ship has to get below the halt speed. TODO(initiator)
    pub patrol_halt_s: f64,
    /// Seconds after an order ended before a new one. TODO(initiator)
    pub patrol_cooldown_s: f64,
}

impl Record for Tuning {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HeatContent {
    pub tuning: Loaded<Tuning>,
}

impl HeatContent {
    /// Loads the one tuning record from `heat/`. Returns every error, each naming the file and
    /// the field.
    pub fn load(files: &[File]) -> Result<HeatContent, Vec<String>> {
        let mut e = Vec::new();
        let mut found = load_records::<Tuning>(files, FOLDER, &mut e);
        if found.len() != 1 && e.is_empty() {
            e.push(format!("{FOLDER}: exactly one tuning record is needed, found {}", found.len()));
        }
        let Some((_, tuning)) = found.pop_first() else { return Err(e) };
        check(&tuning.path, &tuning.record, &mut e);
        if e.is_empty() { Ok(HeatContent { tuning }) } else { Err(e) }
    }
}

fn check(path: &str, t: &Tuning, e: &mut Vec<String>) {
    check_id(path, "id", &t.id, e);
    let mut bad = |field: &str, msg: &str| e.push(format!("{path}: {field}: {msg}"));
    if !(t.points_per_star > 0.0) {
        bad("points_per_star", "must be positive");
    }
    if t.max_points < t.points_per_star * f64::from(STARS_MAX) {
        bad("max_points", "must reach 5 stars (at least 5 x points_per_star)");
    }
    for (field, v) in [
        ("decay_per_min", t.decay_per_min),
        ("decay_per_min_clean", t.decay_per_min_clean),
        ("violation_points.no_clearance", t.violation_points.no_clearance),
        ("violation_points.wrong_pad", t.violation_points.wrong_pad),
        ("violation_points.speeding", t.violation_points.speeding),
        ("violation_points.fled_scan", t.violation_points.fled_scan),
        ("deal_points", t.deal_points),
        ("talk_fail_points", t.talk_fail_points),
        ("flee_points", t.flee_points),
        ("talk_fail_fine_factor", t.talk_fail_fine_factor),
    ] {
        if !(v >= 0.0) {
            bad(field, "must not be negative");
        }
    }
    if t.fine_base < 0 || t.fine_per_crate < 0 || t.bribe_base < 0 {
        bad("fine_base", "fines and bribes must not be negative");
    }
    if t.paydown_per_point <= 0 {
        bad("paydown_per_point", "must be positive (money per heat point)");
    }
    for (field, list) in [("scan_chance", &t.scan_chance), ("talk_chance", &t.talk_chance)] {
        if list.len() != usize::from(STARS_MAX) + 1 {
            bad(field, &format!("needs {} values, one per star 0 to 5", usize::from(STARS_MAX) + 1));
        } else if list.iter().any(|v| !(0.0..=1.0).contains(v)) {
            bad(field, "every value must be in 0..1");
        }
    }
    for (field, v) in [("jammer_factor", t.jammer_factor), ("thorough_chance", t.thorough_chance)] {
        if !(0.0..=1.0).contains(&v) {
            bad(field, "must be in 0..1");
        }
    }
    if t.thorough_from_stars > STARS_MAX {
        bad("thorough_from_stars", "must be a star level 0 to 5");
    }
    if t.patrol_from_stars > STARS_MAX {
        bad("patrol_from_stars", "must be a star level 0 to 5");
    }
    if !(t.patrol_check_s > 0.0) {
        bad("patrol_check_s", "must be positive (seconds per roll)");
    }
    if !(0.0..=1.0).contains(&t.patrol_chance_per_star) {
        bad("patrol_chance_per_star", "must be in 0..1");
    }
    for (field, v) in [
        ("patrol_halt_speed_ms", t.patrol_halt_speed_ms),
        ("patrol_halt_s", t.patrol_halt_s),
        ("patrol_cooldown_s", t.patrol_cooldown_s),
    ] {
        if !(v >= 0.0) {
            bad(field, "must not be negative");
        }
    }
}
