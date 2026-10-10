//! Production state: stations and save section.
use std::collections::BTreeMap;

use gameplay_core::save::{Envelope, SaveError};
use serde::{Deserialize, Serialize};

use crate::station::Station;
use crate::recipe::StationKind;

pub const SECTION: &str = "production";
pub const SECTION_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Production {
    stations: BTreeMap<String, Station>,
}

impl Production {
    pub fn new() -> Production {
        Production { stations: BTreeMap::new() }
    }

    pub fn add_station(&mut self, name: impl Into<String>, kind: StationKind) -> &mut Station {
        self.stations.entry(name.into()).or_insert_with(|| Station::new(kind))
    }

    pub fn get_station(&self, name: &str) -> Option<&Station> {
        self.stations.get(name)
    }

    pub fn get_station_mut(&mut self, name: &str) -> Option<&mut Station> {
        self.stations.get_mut(name)
    }

    pub fn stations(&self) -> &BTreeMap<String, Station> {
        &self.stations
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no production section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Production>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}

impl Default for Production {
    fn default() -> Self {
        Self::new()
    }
}
