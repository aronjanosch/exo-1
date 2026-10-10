//! The kernel's content records, one JSON file each, in the folders `commodity/`, `location/`,
//! `progress_track/` and `unlock/` (#122). Systems load their own folders with `content_core` and
//! check their references against `Content`.
use std::collections::{BTreeMap, BTreeSet};

use content_core::{Loaded, Record, check_id, load_records};
use serde::{Deserialize, Serialize};

use crate::condition::Condition;
use crate::id::{CommodityId, LocationId, Tag, TextKey, TrackId, UnlockId};

pub use content_core::File;

/// The folders the kernel reads.
pub const FOLDERS: [&str; 4] = ["commodity", "location", "progress_track", "unlock"];

/// The crew track that is money.
pub const WALLET: &str = "wallet";

/// A kind of good.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commodity {
    pub id: CommodityId,
    pub name: TextKey,
    /// Money per crate; jobs and later markets start from it.
    pub base_price: i64,
    /// A size name from `content/cargo/crates.json`.
    pub crate_size: String,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

/// A place with content: a pad of a hand-placed place (`content/place/`, #129), with a name,
/// goods and jobs. A city is one place with many pads, so many locations.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub id: LocationId,
    pub name: TextKey,
    /// The place's id (`content/place/<id>.json`); the glue checks that it exists.
    pub place: String,
    /// A pad of that place.
    pub pad: String,
    #[serde(default)]
    pub tags: Vec<Tag>,
    /// When the crew may use it; none means from the start.
    #[serde(default)]
    pub available: Option<Condition>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// One value for the whole crew (in the host save).
    Crew,
    /// One value per player (client id).
    Player,
}

/// Money, XP, later reputation: a number with thresholds.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressTrack {
    pub id: TrackId,
    pub name: TextKey,
    pub owner: Owner,
    /// Value of a new crew or player.
    #[serde(default)]
    pub start: i64,
    /// The value never drops below this (a standing that a failure lowers but never empties
    /// for good); none means no floor.
    #[serde(default)]
    pub min: Option<i64>,
    /// Values at which the level rises, strictly ascending; may be empty.
    #[serde(default)]
    pub thresholds: Vec<i64>,
}

/// Something the crew buys with money; it grants tags that conditions ask for.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unlock {
    pub id: UnlockId,
    pub name: TextKey,
    pub price: i64,
    /// Who may buy it, read for the buyer; none means anyone.
    #[serde(default)]
    pub condition: Option<Condition>,
    pub grants: Vec<Tag>,
}

macro_rules! record {
    ($t:ty, $id:ty) => {
        impl Record for $t {
            type Id = $id;
            fn id(&self) -> &$id {
                &self.id
            }
        }
    };
}
record!(Commodity, CommodityId);
record!(Location, LocationId);
record!(ProgressTrack, TrackId);
record!(Unlock, UnlockId);

/// All kernel records, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    pub commodities: BTreeMap<CommodityId, Loaded<Commodity>>,
    pub locations: BTreeMap<LocationId, Loaded<Location>>,
    pub tracks: BTreeMap<TrackId, Loaded<ProgressTrack>>,
    pub unlocks: BTreeMap<UnlockId, Loaded<Unlock>>,
}

impl Content {
    /// Loads the kernel folders from `files` (other folders are left to their systems) and checks
    /// fields and references. `crate_sizes`: the size names of the crate table. Returns every error
    /// found, each naming the file and the field.
    pub fn load(files: &[File], crate_sizes: &[&str]) -> Result<Content, Vec<String>> {
        let mut e = Vec::new();
        let c = Content {
            commodities: load_records(files, "commodity", &mut e),
            locations: load_records(files, "location", &mut e),
            tracks: load_records(files, "progress_track", &mut e),
            unlocks: load_records(files, "unlock", &mut e),
        };
        c.check(crate_sizes, &mut e);
        if e.is_empty() { Ok(c) } else { Err(e) }
    }

    fn check(&self, crate_sizes: &[&str], e: &mut Vec<String>) {
        for Loaded { path, record: r } in self.commodities.values() {
            check_id(path, "id", r.id.as_str(), e);
            check_tags(path, &r.tags, e);
            if r.base_price <= 0 {
                e.push(format!("{path}: base_price: must be positive"));
            }
            if !crate_sizes.contains(&r.crate_size.as_str()) {
                e.push(format!("{path}: crate_size: '{}' is not in the crate table ({})", r.crate_size, crate_sizes.join(", ")));
            }
        }
        for Loaded { path, record: r } in self.locations.values() {
            check_id(path, "id", r.id.as_str(), e);
            check_tags(path, &r.tags, e);
            if r.place.is_empty() {
                e.push(format!("{path}: place: empty"));
            }
            if r.pad.is_empty() {
                e.push(format!("{path}: pad: empty"));
            }
            if let Some(c) = &r.available {
                self.check_condition(path, "available", c, e);
            }
        }
        for Loaded { path, record: r } in self.tracks.values() {
            check_id(path, "id", r.id.as_str(), e);
            if r.thresholds.windows(2).any(|w| w[0] >= w[1]) {
                e.push(format!("{path}: thresholds: must be strictly ascending"));
            }
            if r.min.is_some_and(|m| m > r.start) {
                e.push(format!("{path}: min: must not be above start"));
            }
        }
        match self.tracks.get(&TrackId::new(WALLET)) {
            None => e.push(format!("progress_track: the crew track '{WALLET}' is missing")),
            Some(t) if t.record.owner != Owner::Crew => e.push(format!("{}: owner: '{WALLET}' must be a crew track", t.path)),
            Some(_) => {}
        }
        for Loaded { path, record: r } in self.unlocks.values() {
            check_id(path, "id", r.id.as_str(), e);
            check_tags(path, &r.grants, e);
            if r.price < 0 {
                e.push(format!("{path}: price: must not be negative"));
            }
            if r.grants.is_empty() {
                e.push(format!("{path}: grants: empty"));
            }
            if let Some(c) = &r.condition {
                self.check_condition(path, "condition", c, e);
            }
        }
    }

    /// Every tag some unlock grants.
    pub fn granted_tags(&self) -> BTreeSet<&Tag> {
        self.unlocks.values().flat_map(|u| &u.record.grants).collect()
    }

    /// Checks a condition's references: tracks exist, asked tags are granted by some unlock, flags
    /// are not empty. Systems call it for their own records.
    pub fn check_condition(&self, path: &str, field: &str, c: &Condition, e: &mut Vec<String>) {
        match c {
            Condition::TrackAtLeast { track, .. } => {
                if !self.tracks.contains_key(track) {
                    e.push(format!("{path}: {field}: unknown track '{track}'"));
                }
            }
            Condition::HasTag(t) => {
                if !self.granted_tags().contains(t) {
                    e.push(format!("{path}: {field}: no unlock grants the tag '{t}'"));
                }
            }
            Condition::FlagSet(f) => {
                if f.as_str().is_empty() {
                    e.push(format!("{path}: {field}: empty flag"));
                }
            }
            Condition::All(cs) | Condition::Any(cs) => {
                for c in cs {
                    self.check_condition(path, field, c, e);
                }
            }
            Condition::Not(c) => self.check_condition(path, field, c, e),
        }
    }

    /// Checks that a commodity exists; for systems' records.
    pub fn check_commodity(&self, path: &str, field: &str, id: &CommodityId, e: &mut Vec<String>) {
        if !self.commodities.contains_key(id) {
            e.push(format!("{path}: {field}: unknown commodity '{id}'"));
        }
    }

    /// Checks that a location exists; for systems' records.
    pub fn check_location(&self, path: &str, field: &str, id: &LocationId, e: &mut Vec<String>) {
        if !self.locations.contains_key(id) {
            e.push(format!("{path}: {field}: unknown location '{id}'"));
        }
    }
}

fn check_tags(path: &str, tags: &[Tag], e: &mut Vec<String>) {
    for t in tags {
        check_id(path, "tags", t.as_str(), e);
    }
}
