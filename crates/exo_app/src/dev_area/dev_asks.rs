//! The dev asks on the host: each fault, the reset, the director's toggle, the bots' count and the
//! ambush. They poke every owner through the owners' own calls. The faults panel that sent them is
//! gone from the dev zone (#307); scenarios ask directly (`Items::ask_reactor`), and a dev aid can
//! send the same `ReactorAsk`s.
use crate::dev_area::*;
use crate::dev_area::fire::FireConfig;
use crate::dev_area::hull::ShipAir;
use crate::dev_area::ship::{fault, host_asks, route, ReactorConfig, ReactorCounts, ReactorSim, ShipEvents, ShipSet};
use crate::items::Items;
use item_core::wire::{ReactorAsk, ReactorFault};
use reactor_core::{RoomEvent, Source, Tuning};

/// The dev fault SHIELD HIT's hit, points (`EngineRoom::absorb_hit`). TODO(initiator): a dev value.
const DEV_SHIELD_HIT: f64 = 300.0;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
}

fn is_mine(a: &ReactorAsk) -> bool {
    matches!(a, ReactorAsk::Trigger(_) | ReactorAsk::Reset | ReactorAsk::SetDirector(_) | ReactorAsk::SetBots(_) | ReactorAsk::Ambush(_))
}

fn asks(mut items: ResMut<Items>, cfg: Res<ReactorConfig>, fires: Res<FireConfig>, mut sim: ResMut<ReactorSim>, mut air: ResMut<ShipAir>, mut counts: ResMut<ReactorCounts>, mut events: ResMut<ShipEvents>) {
    for a in host_asks(&mut items, is_mine) {
        match a {
            // The fire starts in the air (the fire step), so it goes out as the room's event.
            ReactorAsk::Trigger(ReactorFault::Fire) => {
                info!("dev: fire");
                events.0.push(RoomEvent::Fire { size: fires.0.start_size, source: Source::Reactor });
            }
            // The fires are the air's: a fresh room starts with a fresh air (no fire survives).
            ReactorAsk::Reset => {
                *air = ShipAir::at_start(&cfg.0.airlock);
                apply_sim(&mut sim, a, &cfg.0, &mut counts);
            }
            a => {
                apply_sim(&mut sim, a, &cfg.0, &mut counts);
            }
        }
    }
}

/// A dev ask on the room; false for an ask that is not a dev one.
pub fn apply_sim(s: &mut ReactorSim, a: ReactorAsk, cfg: &Tuning, counts: &mut ReactorCounts) -> bool {
    match a {
        ReactorAsk::Trigger(ReactorFault::DrainCoolant) => {
            s.room.pump_mut().set_tank(0.0);
            info!("dev: drain of the coolant tank");
        }
        ReactorAsk::Trigger(ReactorFault::WarmStart) => {
            let secs = s.room.warm_start();
            info!("dev: warm start, settled after {secs:.1} s of sim time");
        }
        ReactorAsk::Trigger(ReactorFault::ShieldHit) => {
            let hull = s.room.absorb_hit(DEV_SHIELD_HIT);
            info!("dev: shield hit of {DEV_SHIELD_HIT} points, {hull:.1} past the shield");
        }
        ReactorAsk::Trigger(ReactorFault::JumpFull) => {
            s.room.jump_mut().fill();
            info!("dev: jump drive full");
        }
        ReactorAsk::Trigger(ReactorFault::ColdShip) => {
            s.room.cold_ship();
            info!("dev: COLD SHIP");
        }
        ReactorAsk::Trigger(ReactorFault::Aux) => {
            let on = !s.room.aux_enabled();
            s.room.set_aux(on);
            info!("dev: aux reactor {}", if on { "on" } else { "off" });
        }
        ReactorAsk::Trigger(f) => {
            if let Some(f) = fault(f) {
                let events = s.room.trigger(f);
                info!("dev: fault {f:?}");
                for e in events {
                    route(e, counts);
                }
            }
        }
        ReactorAsk::Reset => {
            s.reset(cfg);
            info!("dev: reset");
        }
        ReactorAsk::SetDirector(on) => {
            s.director.set_enabled(on);
            info!("dev: director {}", if on { "on" } else { "off" });
        }
        // The dev BOTS count (r42).
        ReactorAsk::SetBots(n) => s.bots.set_count(n),
        // The dev AMBUSH (r41): a new encounter, refused while one runs; off ends it.
        ReactorAsk::Ambush(true) => {
            if !s.room.start_ambush() {
                info!("dev: ambush refused (an encounter runs)");
            }
        }
        ReactorAsk::Ambush(false) => s.room.stop_ambush(),
        _ => return false,
    }
    true
}
