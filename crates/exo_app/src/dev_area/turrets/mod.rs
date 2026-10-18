//! Turrets: the gun's host rules (FIRE, the CLEAR JAM hold, its heat and jams). The gun's screen GN-01 and its
//! flashes are gone with the old displays; the shot detectors stay for the audio.
use crate::dev_area::*;
use crate::dev_area::ship::{host_asks, ReactorSim, ShipSet};
use crate::items::{Items, Role};
use item_core::wire::{ReactorAsk, ReactorNews};

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

/// A shot the gun fired (r40): the shot count went down and the reload started. A reset (the count
/// falls with no reload) is no shot. The audio (`audio_slice`) uses it.
pub fn shot_fired(prev: Option<u8>, r: &ReactorNews) -> bool {
    prev.is_some_and(|p| r.weapon_shots < p) && r.weapon_reload_s > 0.0
}

/// An enemy shot the news shows (r41): the shot count went up in an encounter that runs. A new
/// encounter starts its count at 0 and so is no shot. The audio (`audio_slice`) uses it.
pub fn enemy_shot(prev: Option<u32>, r: &ReactorNews) -> bool {
    match (prev, r.enemy) {
        (Some(p), Some(e)) => e.shots > p,
        _ => false,
    }
}
