//! The bridge: the captain (CP-01 and the captain's consoles), the helm, the jump drive and the shield.
use crate::dev_area::*;

pub mod captain;
pub mod captain_screen;

pub fn plugin(app: &mut App) {
    app.add_plugins(captain::plugin);
}

pub fn window_plugin(app: &mut App) {
    captain::window_plugin(app);
}
