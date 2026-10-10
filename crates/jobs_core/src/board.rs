//! Board generation (#126): per location, 3 to 5 offers from templates whose availability
//! condition holds; offers rotate after a lifetime (TODO(initiator) default 600s).
use std::collections::BTreeMap;

use gameplay_core::rng::Rng;
use gameplay_core::{Content, LocationId, Progress};
use serde::{Deserialize, Serialize};

use crate::id::JobId;
use crate::job::Leg;
use crate::template::{CommoditySpec, JobTemplate, ObjectiveSpec, PlaceSpec};

/// Lifetime of an offer before it rotates, in seconds. TODO(initiator): initial value.
pub const OFFER_LIFETIME_S: f64 = 600.0;

/// Board state per location: which offers are active and their lifetimes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    /// Per location, the offers currently shown.
    pub offers_per_location: BTreeMap<String, BoardLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoardLocation {
    /// Job ids currently on this board. The jobs themselves are in Jobs.jobs.
    pub offer_ids: Vec<JobId>,
    /// Seconds since these offers were generated (used for lifetime rotation).
    pub age_s: f64,
    /// The seed used to generate these offers, for determinism on reload.
    pub seed: u64,
}

impl Default for Board {
    fn default() -> Self {
        Board { offers_per_location: BTreeMap::new() }
    }
}

impl Board {
    /// Tick the board forward by `dt` seconds; rotate offers whose lifetime expired.
    pub fn tick(&mut self, dt: f64) {
        for loc in self.offers_per_location.values_mut() {
            loc.age_s += dt;
        }
    }

    /// Clear the board entries (used for save/load).
    pub fn clear(&mut self) {
        self.offers_per_location.clear();
    }
}

/// Generate concrete legs from a template's deliver objectives.
/// Returns the legs, or an error if tag search fails.
pub fn generate_legs(
    template: &JobTemplate,
    kernel: &Content,
    progress: &Progress,
    rng: &mut Rng,
) -> Result<Vec<Leg>, String> {
    let mut legs = Vec::new();

    for objective in &template.objectives {
        match objective {
            ObjectiveSpec::Deliver { from, to, commodity, amount } => {
                let pickup = resolve_place_spec(from, kernel, progress, rng)?;
                let mut dropoff = resolve_place_spec(to, kernel, progress, rng)?;

                // Ensure pickup != dropoff
                let mut attempts = 0;
                while pickup == dropoff && attempts < 10 {
                    dropoff = resolve_place_spec(to, kernel, progress, rng)?;
                    attempts += 1;
                }

                if pickup == dropoff {
                    return Err("could not find different pickup/dropoff locations".into());
                }

                // Pick a commodity from the pool
                let CommoditySpec::OneOf(pool) = commodity;
                if pool.is_empty() {
                    return Err("commodity pool is empty".into());
                }
                let commodity_idx = rng.below(pool.len());
                let commodity = pool[commodity_idx].clone();

                // Use minimum amount for boards
                let amt = amount[0];

                legs.push(Leg::new(pickup, dropoff, commodity, amt));
            }
            _ => {
                // Non-deliver objectives are not handled in board generation
            }
        }
    }

    Ok(legs)
}

/// Resolve a place spec to a concrete location.
/// For fixed locations, return it if available.
/// For tags, pick a random available location with that tag.
fn resolve_place_spec(
    place: &PlaceSpec,
    kernel: &Content,
    progress: &Progress,
    rng: &mut Rng,
) -> Result<LocationId, String> {
    match place {
        PlaceSpec::Location(loc) => {
            // Check if this location is available
            if progress.location_available(kernel, loc) {
                Ok(loc.clone())
            } else {
                Err(format!("location {} is not available", loc))
            }
        }
        PlaceSpec::Tagged(tag) => {
            // Find all available locations with this tag
            let mut candidates: Vec<LocationId> = kernel
                .locations
                .values()
                .filter(|l| {
                    l.record.tags.contains(tag) && progress.location_available(kernel, &l.record.id)
                })
                .map(|l| l.record.id.clone())
                .collect();

            if candidates.is_empty() {
                return Err(format!("no available location with tag '{}'", tag));
            }

            let idx = rng.below(candidates.len());
            Ok(candidates.swap_remove(idx))
        }
    }
}
