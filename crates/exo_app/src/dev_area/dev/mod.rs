//! Dev tools: the faults screen, the look samples, the item table and the test console. They stand
//! in the dev area to try things; none of them is a part of the ship.
use crate::dev_area::*;

pub mod item_table;
pub mod look_samples;
pub mod test_console;

pub fn plugin(app: &mut App) {
    app.add_plugins((test_console::plugin, look_samples::plugin, item_table::plugin));
}
