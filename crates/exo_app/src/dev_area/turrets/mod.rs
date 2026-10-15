//! Turrets: the gun GN-01 (FIRE, the CLEAR JAM hold, its heat and jams), its muzzle flash and the
//! enemy's hits on the hull.
use crate::dev_area::*;
use crate::dev_area::ship::{host_asks, ReactorSim, ShipSet};
use crate::items::{Items, Role};
use item_core::wire::ReactorAsk;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
}

fn is_mine(a: &ReactorAsk) -> bool {
    matches!(a, ReactorAsk::Fire | ReactorAsk::ClearJam(_))
}

fn asks(time: Res<Time>, mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>) {
    for a in host_asks(&mut items, is_mine) {
        apply(&mut sim, a);
    }
    if items.role != Role::Client {
        sim.tick_jam(time.delta_secs_f64());
    }
}

/// One of the gun's asks on the room; false for an ask that is not the gun's.
pub fn apply(s: &mut ReactorSim, a: ReactorAsk) -> bool {
    match a {
        // The GN-01's FIRE (r40): one shot, refused while reloading or without a shot's energy.
        ReactorAsk::Fire => {
            if !s.room.fire_weapon() {
                info!("reactor: gun refused (reloading or no shot's energy)");
            }
        }
        // GN-01's CLEAR JAM hold (r41): letting go restarts the count.
        ReactorAsk::ClearJam(on) => {
            s.clearing = on;
            if !on {
                s.room.weapons_mut().stop_clearing();
            }
        }
        _ => return false,
    }
    true
}
