//! Engineering: the reactor's glue (its asks, its screens), the coolant, the fuel rods, the blast
//! and the cold start.
use crate::dev_area::*;

pub mod blast;
pub mod coolant;
pub mod rods;
pub mod startup_screen;

pub fn plugin(app: &mut App) {
    app.add_plugins((coolant::plugin, rods::plugin));
}
