//! Ids. Content ids are strings from the data (lower case, digits, `_`); runtime ids are numbers.
use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! text_id {
    ($($(#[$m:meta])* $name:ident;)*) => {$(
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> $name {
                $name(s.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    )*};
}

text_id! {
    /// A `commodity` record.
    CommodityId;
    /// A `location` record.
    LocationId;
    /// A `progress_track` record.
    TrackId;
    /// An `unlock` record.
    UnlockId;
    /// A label on content (`dusty`, `route_bent_spoon`). The crew owns the tags its unlocks grant.
    Tag;
    /// A fact a system raised at runtime (`job_completed:first_haul`); conditions can ask for it.
    Flag;
    /// A key into the localisation table (#131); never shown text itself.
    TextKey;
}

/// One client (player's game install), created once and kept in its game directory; the save keys
/// personal tracks by it. Not the network slot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientId(pub u64);

/// A crate's stable id (not the engine entity), #130.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CrateId(pub u64);
