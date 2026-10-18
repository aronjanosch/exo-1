//! Engineering's asks on the host: the reactor's main screen controls (Auto or manual, injection, converter, power, boost),
//! the grid (breakers, limits, the AUX switches, the battery's recharge) and the coolant pump. The SCRAM, the eject and the
//! start stand are the reactor room's parts (`reactor_room`), not asks.
use crate::dev_area::*;
use crate::dev_area::ship::{group_of, host_asks, pump_of, ReactorSim, ShipSet};
use crate::items::Items;
use item_core::wire::ReactorAsk;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
}

fn is_mine(a: &ReactorAsk) -> bool {
    use ReactorAsk::*;
    matches!(a, SetManual(_) | SetInjection(_) | SetConverter(_) | EmergencyCool | SetPower(_) | SetBreaker(..) | SetAuxSwitch(..) | SetLimit(..) | SetBatteryRecharge(_) | SetPumpMode(_) | Boost(_))
}

fn asks(mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>) {
    for a in host_asks(&mut items, is_mine) {
        apply(&mut sim, a);
    }
}

/// One of engineering's asks on the room; false for an ask that is not engineering's.
pub fn apply(s: &mut ReactorSim, a: ReactorAsk) -> bool {
    match a {
        ReactorAsk::SetManual(on) => {
            if on && !s.manual {
                // Start from where the reactor is, not from zero.
                let (f, t) = s.room.reactor().targets();
                s.injection = f;
                s.converter = t;
            }
            s.manual = on;
        }
        ReactorAsk::SetInjection(v) if s.manual => s.injection = v.clamp(0.0, 100.0),
        ReactorAsk::SetConverter(v) if s.manual => s.converter = v.clamp(0.0, 100.0),
        ReactorAsk::SetInjection(_) | ReactorAsk::SetConverter(_) => {}
        ReactorAsk::EmergencyCool => {
            if !s.room.emergency_cool() {
                info!("reactor: emergency cooling refused (cooldown, tank or pump)");
            }
        }
        ReactorAsk::SetPower(on) => {
            if !s.room.set_power(on) {
                info!("reactor: power on refused (broken)");
            }
        }
        ReactorAsk::SetBreaker(g, on) => s.room.set_breaker(group_of(g), on),
        // Engines is never aux-fed: the room ignores its switch.
        ReactorAsk::SetAuxSwitch(g, on) => s.room.set_aux_switch(group_of(g), on),
        ReactorAsk::SetLimit(g, v) => s.room.grid_mut().set_limit(group_of(g), v),
        ReactorAsk::SetBatteryRecharge(v) => s.room.battery_mut().set_recharge(v),
        ReactorAsk::SetPumpMode(m) => s.room.set_pump_mode(pump_of(m)),
        ReactorAsk::Boost(up) => {
            if !s.room.reactor_mut().boost(up) {
                info!("reactor: boost refused (off, broken, or not allowed now)");
            }
        }
        _ => return false,
    }
    true
}
