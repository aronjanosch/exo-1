//! The `town` record (one JSON file per town in `town/`) and the `traffic_tuning` record (one
//! file in `traffic_tuning/`), with the checked loader and the text keys the tower speaks with.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::text::TextTable;
use gameplay_core::{Content, LocationId, TextKey, TownId};
use serde::Deserialize;

pub const TOWN_FOLDER: &str = "town";
pub const TUNING_FOLDER: &str = "traffic_tuning";
/// Pools of a tower's voice need this many lines.
pub const MIN_POOL: usize = 3;

/// A town with its own airspace and tower. TODO(initiator): every number here is a placeholder.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Town {
    pub id: TownId,
    pub name: TextKey,
    /// The text pool prefix of the tower's voice (`tower.grumpy`).
    pub voice: String,
    pub airspace_m: f64,
    pub speed_limit_ms: f64,
    /// The official pads, at least one. A pad belongs to one town only.
    pub pads: Vec<LocationId>,
    /// The pad a scan happens on at high heat; must be one of `pads`.
    #[serde(default)]
    pub inspection_pad: Option<LocationId>,
    /// No tower and no violations: hailing gets "no tower here".
    pub free_port: bool,
    /// Chance (0..1) that the tower holds the sender before the first grant.
    pub hold_chance: f64,
    /// The heat stars (1..=5) at which the tower sends the sender to the inspection pad.
    pub inspection_stars: u8,
}

impl Record for Town {
    type Id = TownId;
    fn id(&self) -> &TownId {
        &self.id
    }
}

/// The tower's clock, grades and fines, one record. TODO(initiator): starting values.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub id: String,
    /// Seconds before the tower answers a hail.
    pub answer_delay_s: f64,
    /// Seconds a clearance lasts from the grant.
    pub window_s: f64,
    /// Seconds a holding pattern lasts before the second answer.
    pub hold_s: f64,
    /// Touchdown speeds (m/s) that earn grades 3, 2 and 1; faster is grade 0. Ascending.
    pub grade_speeds: [f64; 3],
    /// Seconds above the speed limit before a speeding violation.
    pub speeding_grace_s: f64,
    /// Seconds between two speeding violations.
    pub speeding_cooldown_s: f64,
    /// Money (wallet) for landing without clearance.
    pub fine_no_clearance: i64,
    /// Money (wallet) for landing on another pad than the assigned one.
    pub fine_wrong_pad: i64,
    /// Money (wallet) for speeding.
    pub fine_speeding: i64,
}

impl Record for Tuning {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrafficContent {
    pub towns: BTreeMap<TownId, Loaded<Town>>,
    pub tuning: Loaded<Tuning>,
}

/// Text keys a tower voice speaks with: the pool prefix plus one of these endings.
pub fn pool_keys(voice: &str) -> Vec<String> {
    let mut keys: Vec<String> = ["granted", "held", "inspection", "expired", "out_of_range", "no_tower", "violation.no_clearance", "violation.wrong_pad", "violation.speeding"]
        .iter()
        .map(|k| format!("{voice}.{k}"))
        .collect();
    keys.extend((0..=3).map(|g| format!("{voice}.graded.{g}")));
    keys
}

/// The town of a pad, if the pad is an official pad of any town.
pub fn town_of_pad<'a>(tc: &'a TrafficContent, pad: &LocationId) -> Option<&'a TownId> {
    tc.towns.values().find(|t| t.record.pads.contains(pad)).map(|t| &t.record.id)
}

impl TrafficContent {
    /// Loads `town/` and `traffic_tuning/` from `files` and checks fields and references.
    /// Returns every error, each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<TrafficContent, Vec<String>> {
        let mut e = Vec::new();
        let towns = load_records::<Town>(files, TOWN_FOLDER, &mut e);
        let mut tunings: Vec<Loaded<Tuning>> = load_records::<Tuning>(files, TUNING_FOLDER, &mut e).into_values().collect();
        if tunings.len() != 1 && e.is_empty() {
            e.push(format!("{TUNING_FOLDER}: exactly one tuning record is needed, found {}", tunings.len()));
        }
        let Some(tuning) = tunings.pop() else { return Err(e) };
        let tc = TrafficContent { towns, tuning };
        let mut owner: BTreeMap<LocationId, String> = BTreeMap::new();
        for Loaded { path, record: t } in tc.towns.values() {
            tc.check_town(path, t, kernel, &mut owner, &mut e);
        }
        tc.check_tuning(&mut e);
        if e.is_empty() { Ok(tc) } else { Err(e) }
    }

    fn check_town(&self, path: &str, t: &Town, k: &Content, owner: &mut BTreeMap<LocationId, String>, e: &mut Vec<String>) {
        check_id(path, "id", t.id.as_str(), e);
        if t.voice.is_empty() {
            e.push(format!("{path}: voice: empty"));
        }
        if !(t.airspace_m > 0.0) {
            e.push(format!("{path}: airspace_m: must be positive"));
        }
        if !(t.speed_limit_ms > 0.0) {
            e.push(format!("{path}: speed_limit_ms: must be positive"));
        }
        if !(0.0..=1.0).contains(&t.hold_chance) {
            e.push(format!("{path}: hold_chance: must be in 0..1"));
        }
        if !(1..=5).contains(&t.inspection_stars) {
            e.push(format!("{path}: inspection_stars: must be in 1..5"));
        }
        if t.pads.is_empty() {
            e.push(format!("{path}: pads: needs at least one pad"));
        }
        for pad in &t.pads {
            k.check_location(path, "pads", pad, e);
            match owner.get(pad) {
                Some(other) => e.push(format!("{path}: pads: '{pad}' is already a pad of {other}")),
                None => {
                    owner.insert(pad.clone(), path.to_string());
                }
            }
        }
        if let Some(insp) = &t.inspection_pad {
            if !t.pads.contains(insp) {
                e.push(format!("{path}: inspection_pad: '{insp}' must also be one of the pads"));
            }
            if t.pads.len() < 2 {
                e.push(format!("{path}: pads: an inspection pad needs at least one other pad"));
            }
        }
    }

    fn check_tuning(&self, e: &mut Vec<String>) {
        let Loaded { path, record: r } = &self.tuning;
        check_id(path, "id", &r.id, e);
        for (field, v) in [("answer_delay_s", r.answer_delay_s), ("window_s", r.window_s), ("hold_s", r.hold_s)] {
            if !(v > 0.0) {
                e.push(format!("{path}: {field}: must be positive"));
            }
        }
        if r.grade_speeds.iter().any(|s| !(*s > 0.0)) || !r.grade_speeds.windows(2).all(|w| w[0] < w[1]) {
            e.push(format!("{path}: grade_speeds: must be positive and ascending"));
        }
        for (field, v) in [("speeding_grace_s", r.speeding_grace_s), ("speeding_cooldown_s", r.speeding_cooldown_s)] {
            if v < 0.0 {
                e.push(format!("{path}: {field}: must not be negative"));
            }
        }
        for (field, v) in [("fine_no_clearance", r.fine_no_clearance), ("fine_wrong_pad", r.fine_wrong_pad), ("fine_speeding", r.fine_speeding)] {
            if v < 0 {
                e.push(format!("{path}: {field}: must not be negative"));
            }
        }
    }

    /// The text keys and pools the towns need: the name, the notices, each voice's pools.
    pub fn check_texts(&self, table: &TextTable) -> Vec<String> {
        let mut e = Vec::new();
        for key in ["notice.traffic.out_of_range", "notice.traffic.no_tower"] {
            if !table.has(key) {
                e.push(format!("text: no text '{key}'"));
            }
        }
        for Loaded { path, record: t } in self.towns.values() {
            if !table.has(t.name.as_str()) {
                e.push(format!("{path}: name: no text '{}'", t.name));
            }
            for key in pool_keys(&t.voice) {
                match table.lines(&key) {
                    None => e.push(format!("{path}: voice: no text '{key}'")),
                    Some(l) if l.len() < MIN_POOL => e.push(format!("{path}: voice: '{key}' needs at least {MIN_POOL} lines (it has {})", l.len())),
                    Some(_) => {}
                }
            }
        }
        e
    }
}
