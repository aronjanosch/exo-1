//! Dev zone: the item table, the only thing that stands there. Dev aids live in the F10 dev menu.
use crate::dev_area::*;

pub mod item_table;

pub fn plugin(app: &mut App) {
    app.add_plugins(item_table::plugin);
}
