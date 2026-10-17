//! The hull's part of the ship tick on the host: its asks (the airlock's presses, the captain's
//! SEAL, the vents' valves, SILENCE), the air the oxygen generator and the breaches move, the
//! ship's alarm, and the hull's part of the news (the rooms' air and fire, the airlock, the alarms).
use crate::dev_area::*;
use crate::dev_area::fire::{FireAlarm, FireGrid, FireModel};
use crate::dev_area::hull::airlock;
use crate::dev_area::hull::{ShipAir, ShipAlarm};
use crate::dev_area::layout::ROOM;
use crate::dev_area::ship::{alarm_of, host_asks, ReactorConfig, ReactorSim, ShipSet};
use crate::items::{Items, Role};
use item_core::wire::{Alarm, AlarmNews, ReactorAsk, RoomAir, RoomAlarmKind};
use reactor_core::room_alarm::{AlarmState, Kind};
use reactor_core::RoomAlarm;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
    app.add_systems(FixedUpdate, air_step.in_set(ShipSet::React));
    app.add_systems(FixedUpdate, alarm_level.in_set(ShipSet::Settle));
    app.add_systems(FixedUpdate, news.in_set(ShipSet::Fill));
}

fn is_mine(a: &ReactorAsk) -> bool {
    matches!(a, ReactorAsk::Airlock(_) | ReactorAsk::SealAirlock | ReactorAsk::SetVent(..) | ReactorAsk::SilenceAlarms)
}

fn asks(mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>, mut air: ResMut<ShipAir>) {
    for a in host_asks(&mut items, is_mine) {
        match a {
            // The captain's SEAL (r41, CP-01): shuts the inner door when it is open and no cycle runs.
            ReactorAsk::SealAirlock => {
                if !air.seal_inner() {
                    info!("hull: seal refused (the inner door is shut, or a cycle runs)");
                }
            }
            // The airlock's panels (r40, #309): the interlock ignores what it must (a push during a cycle).
            ReactorAsk::Airlock(ask) => {
                if !airlock::apply(&mut air, ask) {
                    info!("hull: airlock {ask:?} ignored (a cycle runs, or the inner door is locked)");
                }
            }
            a => {
                apply_sim(&mut sim, a);
            }
        }
    }
}

/// The hull's asks that the room's switches keep (the vents' valves, SILENCE); false for any other.
pub fn apply_sim(s: &mut ReactorSim, a: ReactorAsk) -> bool {
    match a {
        ReactorAsk::SetVent(i, open) => match s.vents.get_mut(i as usize) {
            Some(v) => *v = open,
            None => info!("hull: no vent {i}"),
        },
        ReactorAsk::SilenceAlarms => s.alarms.iter_mut().for_each(RoomAlarm::silence),
        _ => return false,
    }
    true
}

/// Host: the oxygen generator feeds the air through its vents (r38, `ShipAir::add_oxygen_through_vents`;
/// `EngineRoom::oxygen_flow` is the life support's power and the generator's condition squared), and
/// each hull section's hole leaks the room's air to space at its openness (r37, by section in r40).
fn air_step(time: Res<Time>, items: Res<Items>, cfg: Res<ReactorConfig>, sim: Res<ReactorSim>, mut air: ResMut<ShipAir>) {
    if items.role == Role::Client {
        return;
    }
    let dt = time.delta_secs_f64();
    let o2 = sim.room.oxygen_flow(cfg.0.life_support.oxygen_generator_rate);
    air.add_oxygen_through_vents(o2 * dt, sim.vents);
    let holes = air.breaches;
    for (i, hole) in holes.into_iter().enumerate() {
        air.deck.set_leak(hole, cfg.0.hull.breach_area_m2, sim.room.hull().open(i));
    }
}

/// Host: the ship's alarm is the room's level or the fire's, whichever is worse; it stays until the
/// level falls to none.
fn alarm_level(items: Res<Items>, mut sim: ResMut<ReactorSim>, fire_alarm: Res<FireAlarm>, mut alarm: ResMut<ShipAlarm>) {
    if items.role == Role::Client {
        return;
    }
    let level = alarm_of(sim.room.level()).max(fire_alarm.0);
    if level != Alarm::None || sim.alarmed {
        alarm.level = level;
    }
    sim.alarmed = level != Alarm::None;
}

/// Host: the hull's part of the news: the engine room's air, the airlock, the outside's vacuum, each
/// room's air and fire (the status monitor's rows, r38 and r44: the engine room, the corridor, the
/// bridge) and the room alarms (r40, #165: the breach on the engine room only, the one room with
/// hull sections).
fn news(time: Res<Time>, mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>, air: Res<ShipAir>, grid: Res<FireGrid>, model: Res<FireModel>) {
    if items.role == Role::Client {
        return;
    }
    let dt = time.delta_secs_f64();
    let Some(mut now) = items.reactor else { return };
    now.oxygen = (air.oxygen_at(ROOM.centre) * 100.0).round() / 100.0;
    now.airlock = air.airlock_news();
    now.vacuum = air.vacuum;
    now.rooms = [air.engine, air.corridor, air.bridge].map(|room| RoomAir {
        o2: (air.deck.room(room).oxygen_fraction() * 100.0).round() / 100.0,
        fire: match *model {
            FireModel::Point => air.deck.fires_in(room).next().is_some(),
            FireModel::Grid => grid.0.burning().any(|c| c.room == room),
        },
    });
    let breaching = now.breaches.iter().any(|&d| reactor_core::hull::openness(d) > 0.0);
    now.alarms = std::array::from_fn(|i| alarm_news(sim.alarms[i].step(now.rooms[i].fire, i == 0 && breaching, now.rooms[i].o2, dt)));
    items.reactor = Some(now);
}

/// The wire's alarm of a room (r40): the core's kind and flags as they travel.
pub fn alarm_news(a: AlarmState) -> AlarmNews {
    let kind = match a.kind {
        Kind::Clear => RoomAlarmKind::Clear,
        Kind::Fire => RoomAlarmKind::Fire,
        Kind::Breach => RoomAlarmKind::Breach,
        Kind::LowO2 => RoomAlarmKind::LowO2,
    };
    AlarmNews { kind, sounding: a.sounding, silenced: a.silenced }
}
