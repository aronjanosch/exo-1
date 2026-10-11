//! Tallies (#230): a fact in the events counts on a crew track. Each kind of match raises
//! `TrackChanged{track, delta: 1}`; the systems that own the events raise them, this only reads.
//! Folder `tally/`, one record per file, loaded and checked by `Tallies::load`.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records};
use serde::Deserialize;

use crate::content::{Content, Owner};
use crate::event::{Event, WorldEvent};
use crate::id::TrackId;

pub const FOLDER: &str = "tally";
/// The prefix of a flag tally: `flag:job_completed:` counts every finished job.
const FLAG_PREFIX: &str = "flag:";

/// What a tally counts. In JSON: `"product_order_settled"`, `"recipe_discovered"`,
/// `"landing_perfect"`, `"scan_clean"`, or `"flag:<prefix>"`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub enum TallyOn {
    /// A product order settled (any result).
    ProductOrderSettled,
    /// A brew found a mix the crew had not made before (`Brewed` with `discovered`).
    RecipeDiscovered,
    /// A landing graded 3 and inside the clearance window (`LandingGraded`).
    LandingPerfect,
    /// A scan ended with no fine and no confiscation (`ScanResolved`).
    ScanClean,
    /// A flag raised with this prefix (`FlagRaised`).
    FlagPrefix(String),
}

impl TryFrom<String> for TallyOn {
    type Error = String;

    fn try_from(s: String) -> Result<TallyOn, String> {
        Ok(match s.as_str() {
            "product_order_settled" => TallyOn::ProductOrderSettled,
            "recipe_discovered" => TallyOn::RecipeDiscovered,
            "landing_perfect" => TallyOn::LandingPerfect,
            "scan_clean" => TallyOn::ScanClean,
            _ => match s.strip_prefix(FLAG_PREFIX) {
                Some(p) if !p.is_empty() => TallyOn::FlagPrefix(p.to_string()),
                Some(_) => return Err("on: 'flag:' needs a prefix, e.g. 'flag:job_completed:'".to_string()),
                None => return Err(format!("on: unknown tally '{s}'")),
            },
        })
    }
}

impl TallyOn {
    /// Whether the event is a match for this kind.
    pub fn matches(&self, p: &WorldEvent) -> bool {
        match (self, p) {
            (TallyOn::ProductOrderSettled, WorldEvent::ProductOrderSettled { .. }) => true,
            (TallyOn::RecipeDiscovered, WorldEvent::Brewed { discovered, .. }) => *discovered,
            (TallyOn::LandingPerfect, WorldEvent::LandingGraded { grade, on_time, .. }) => *grade == 3 && *on_time,
            (TallyOn::ScanClean, WorldEvent::ScanResolved { fine, confiscate, .. }) => *fine == 0 && !*confiscate,
            (TallyOn::FlagPrefix(prefix), WorldEvent::FlagRaised { flag }) => flag.as_str().starts_with(prefix.as_str()),
            _ => false,
        }
    }
}

/// One tally: the kind, and the crew track it adds 1 to on each match.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tally {
    pub id: String,
    pub on: TallyOn,
    pub track: TrackId,
}

impl Record for Tally {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

/// The loaded tallies, by id.
#[derive(Clone, Debug, PartialEq)]
pub struct Tallies {
    pub tallies: BTreeMap<String, Loaded<Tally>>,
}

impl Tallies {
    /// Loads `tally/` from `files` and checks ids and tracks (a crew track that exists). Returns
    /// every error, each naming the file and the field.
    pub fn load(files: &[File], k: &Content) -> Result<Tallies, Vec<String>> {
        let mut e = Vec::new();
        let tallies = load_records::<Tally>(files, FOLDER, &mut e);
        for Loaded { path, record: r } in tallies.values() {
            check_id(path, "id", &r.id, &mut e);
            match k.tracks.get(&r.track) {
                None => e.push(format!("{path}: track: unknown track '{}'", r.track)),
                Some(t) if t.record.owner != Owner::Crew => e.push(format!("{path}: track: '{}' must be a crew track", r.track)),
                Some(_) => {}
            }
        }
        if e.is_empty() { Ok(Tallies { tallies }) } else { Err(e) }
    }

    /// The `TrackChanged` events this event makes (one per matching tally, in id order). A
    /// repeat of the event is the caller's to drop (`Dedup`).
    pub fn apply_world(&self, ev: &Event<WorldEvent>) -> Vec<WorldEvent> {
        self.tallies
            .values()
            .filter(|t| t.record.on.matches(&ev.payload))
            .map(|t| WorldEvent::TrackChanged { track: t.record.track.clone(), delta: 1, player: None })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_parse_and_bad_ones_are_named() {
        assert_eq!(TallyOn::try_from("scan_clean".to_string()), Ok(TallyOn::ScanClean));
        assert_eq!(TallyOn::try_from("flag:job_completed:".to_string()), Ok(TallyOn::FlagPrefix("job_completed:".into())));
        assert!(TallyOn::try_from("flag:".to_string()).is_err());
        assert!(TallyOn::try_from("sold".to_string()).unwrap_err().contains("sold"));
    }
}
