//! The dev area's skills (#284): one place for the five skill values every role lane reads
//! (`VISION.md`, section 6; the real tracks come with #125). All start at the middle of the scale so
//! a lane sees both a risk and a gain; a lane that wants a different value in a scenario sets it.
use bevy::prelude::*;

/// Skill values 0..100 of the player standing in the dev area (`TODO(initiator)`: the five names and
/// the starting value follow the skill decision; Barotrauma's are piloting, weapons, mechanical,
/// electrical, medical).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct DevSkills {
    pub piloting: f64,
    pub weapons: f64,
    pub mechanical: f64,
    pub electrical: f64,
    pub medical: f64,
}

impl Default for DevSkills {
    fn default() -> Self {
        DevSkills { piloting: 50.0, weapons: 50.0, mechanical: 50.0, electrical: 50.0, medical: 50.0 }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<DevSkills>();
}
