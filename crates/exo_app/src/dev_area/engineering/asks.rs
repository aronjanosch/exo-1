//! Engineering's asks on the host: the reactor's controls (Auto or manual, injection, converter, power,
//! boost, the eject's cover and button), the cold start (the master breaker, BOOT, OVERRIDE), the
//! grid (breakers, limits, the AUX switches, the battery's recharge) and the coolant pump.
use crate::dev_area::*;
use crate::dev_area::ship::{group_of, host_asks, pump_of, route, ReactorConfig, ReactorCounts, ReactorSim, ShipSet};
use crate::items::Items;
use item_core::wire::ReactorAsk;
use reactor_core::Tuning;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
}

fn is_mine(a: &ReactorAsk) -> bool {
    use ReactorAsk::*;
    matches!(a, SetManual(_) | SetInjection(_) | SetConverter(_) | ArmEject | Eject | EmergencyCool | SetPower(_) | SetBreaker(..) | SetAuxSwitch(..) | SetLimit(..) | SetBatteryRecharge(_) | SetPumpMode(_) | Boost(_))
}

fn asks(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<ReactorConfig>, mut sim: ResMut<ReactorSim>, mut counts: ResMut<ReactorCounts>) {
    for a in host_asks(&mut items, is_mine) {
        apply(&mut sim, a, &cfg.0, &mut counts);
    }
    if items.role != crate::items::Role::Client {
        sim.tick_eject(time.delta_secs_f64());
    }
}

/// One of engineering's asks on the room; false for an ask that is not engineering's.
pub fn apply(s: &mut ReactorSim, a: ReactorAsk, cfg: &Tuning, counts: &mut ReactorCounts) -> bool {
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
        ReactorAsk::ArmEject => {
            if !s.room.reactor().ejected() && s.eject_left.is_none() {
                s.eject_left = Some(cfg.reactor.eject_arm_s);
                info!("reactor: EJECT cover open for {} s", cfg.reactor.eject_arm_s);
            }
        }
        ReactorAsk::Eject => {
            if s.eject_left.take().is_some() {
                for e in s.room.eject_core() {
                    route(e, counts);
                }
            } else {
                info!("reactor: eject refused (the cover is shut)");
            }
        }
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
