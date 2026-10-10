//! The save model (#128): an envelope with a version and one section per system, each section with
//! its own version. No file I/O here: the glue reads and writes the text (#135). A system brings
//! its own section (`jobs_core::Jobs::save`); the kernel and the world are below.
//!
//! Loading a version this build does not know is a clear error, never a guess.
use std::collections::BTreeMap;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::content::Content;
use crate::event::{Dedup, Event, WorldEvent};
use crate::id::{CommodityId, CrateId};
use crate::progress::{Progress, Refusal};

/// Version of the envelope itself (its shape: version plus named sections).
pub const ENVELOPE_VERSION: u32 = 1;

/// Section names and versions of the kernel and the world.
pub const KERNEL: &str = "kernel";
pub const KERNEL_VERSION: u32 = 1;
pub const WORLD: &str = "world";
pub const WORLD_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub enum SaveError {
    /// The text is not a save, or a section's data does not fit its type.
    Parse(String),
    EnvelopeVersion { found: u32, supported: u32 },
    SectionVersion { section: String, found: u32, supported: u32 },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Parse(e) => write!(f, "save: {e}"),
            SaveError::EnvelopeVersion { found, supported } => write!(f, "save: envelope version {found}, this build reads version {supported}"),
            SaveError::SectionVersion { section, found, supported } => write!(f, "save: section '{section}' has version {found}, this build reads version {supported}"),
        }
    }
}

impl std::error::Error for SaveError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Section {
    version: u32,
    data: serde_json::Value,
}

/// The whole save: sections by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    version: u32,
    sections: BTreeMap<String, Section>,
}

impl Default for Envelope {
    fn default() -> Envelope {
        Envelope::new()
    }
}

impl Envelope {
    pub fn new() -> Envelope {
        Envelope { version: ENVELOPE_VERSION, sections: BTreeMap::new() }
    }

    /// Stores a section, replacing one of the same name.
    pub fn put<T: Serialize>(&mut self, name: &str, version: u32, data: &T) {
        let data = serde_json::to_value(data).expect("section data is plain data");
        self.sections.insert(name.to_string(), Section { version, data });
    }

    /// Reads a section. None if the save has none (a system added after the save was made);
    /// an error if its version is not `supported` or the data does not fit.
    pub fn get<T: DeserializeOwned>(&self, name: &str, supported: u32) -> Result<Option<T>, SaveError> {
        let Some(s) = self.sections.get(name) else { return Ok(None) };
        if s.version != supported {
            return Err(SaveError::SectionVersion { section: name.to_string(), found: s.version, supported });
        }
        serde_json::from_value(s.data.clone()).map(Some).map_err(|e| SaveError::Parse(format!("section '{name}': {e}")))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.sections.keys().map(String::as_str)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("an envelope is plain data")
    }

    pub fn from_json(text: &str) -> Result<Envelope, SaveError> {
        let e: Envelope = serde_json::from_str(text).map_err(|e| SaveError::Parse(e.to_string()))?;
        if e.version != ENVELOPE_VERSION {
            return Err(SaveError::EnvelopeVersion { found: e.version, supported: ENVELOPE_VERSION });
        }
        Ok(e)
    }
}

/// What the kernel saves: progress (tracks, tags, unlocks, flags) and which events were applied,
/// so a retry that arrives after a restart is still ignored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KernelState {
    pub progress: Progress,
    pub dedup: Dedup,
}

impl KernelState {
    pub fn new(content: &Content) -> KernelState {
        KernelState { progress: Progress::new(content), dedup: Dedup::default() }
    }

    /// Applies a world event once: Ok(false) for an id seen before, Ok(true) when applied.
    pub fn apply(&mut self, content: &Content, ev: &Event<WorldEvent>) -> Result<bool, Refusal> {
        if !self.dedup.first_time(ev.id) {
            return Ok(false);
        }
        self.progress.apply(content, ev).map(|()| true)
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(KERNEL, KERNEL_VERSION, self);
    }

    /// None if the save has no kernel section.
    pub fn load(env: &Envelope) -> Result<Option<KernelState>, SaveError> {
        env.get(KERNEL, KERNEL_VERSION)
    }
}

/// Where a saved crate is. Positions are plain f64 triples: in the planet's frame (relative to its
/// centre, so a shifted render origin never matters) or in the ship's own frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CratePlace {
    Planet { planet: u32, pos: [f64; 3] },
    Ship { pos: [f64; 3] },
}

/// A crate that belongs to an active job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrateSave {
    pub id: CrateId,
    pub commodity: CommodityId,
    /// The job's number (`jobs_core::JobId`), plain so the kernel needs no system.
    pub job: Option<u64>,
    /// 0..1.
    pub condition: f64,
    pub place: CratePlace,
}

/// The ship's pose in the planet's frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShipPose {
    pub planet: u32,
    pub pos: [f64; 3],
    /// Quaternion x, y, z, w.
    pub rot: [f64; 4],
    pub vel: [f64; 3],
}

/// The world's part of a save: the crates of active jobs and the ship.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldSave {
    /// The next crate id to hand out.
    pub next_crate: u64,
    pub crates: Vec<CrateSave>,
    pub ship: Option<ShipPose>,
}

impl WorldSave {
    pub fn save(&self, env: &mut Envelope) {
        env.put(WORLD, WORLD_VERSION, self);
    }

    pub fn load(env: &Envelope) -> Result<Option<WorldSave>, SaveError> {
        env.get(WORLD, WORLD_VERSION)
    }
}

