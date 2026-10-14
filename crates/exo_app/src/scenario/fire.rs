//! #298, fire: at the dev area's engine room a fire starts at the reactor (the dev fault, asked
//! through the reactor's request as the dev button would). It grows, uses the room's air, and the
//! ship's alarm goes to Warn, then Danger. A walker beside it catches fire through the heat. Then a
//! fire starts at the pump and at the junction box too, and the three parts lose condition. The
//! extinguisher is taken from the table, selected, and sprayed at each fire from 3 m, LMB held:
//! each goes out, and the alarm is back to the reactor's level (none). Not checked: the spread
//! through the door (it needs a fire at the gap; the seeded rolls are not worth a check here).
//! Without a window; run with `--start=dev_area`.
use super::items::{collect, items, look_at, outside, tap_then, DELAY};
use super::*;
use crate::controls::Controls;
use crate::dev_area::engine_room::{ignition_at, EngineAir, ShipAlarm};
use crate::dev_area::fire::{ignite, Sprays};
use crate::dev_area::reactor::{ReactorCounts, ReactorSim};
use crate::dev_area::{DevArea, ENGINEERING};
use crate::health::HealthState;
use crate::items::{Items, LOCAL};
use health_core::Life;
use crate::dev_area::reactor::alarm_of;
use item_core::wire::{Alarm, ReactorAsk, ReactorFault};
use reactor_core::Source;

/// The fire's start size (m) for the test (the dev button's value in `fire.json`), and the height
/// of a flame's centre above the floor (`dev_area::fire`).
const START: f64 = 0.5;
const FLAME_LIFT: f64 = 0.3;

fn air(w: &World) -> &EngineAir {
    w.resource::<EngineAir>()
}

fn sim(w: &World) -> &ReactorSim {
    w.resource::<ReactorSim>()
}

fn alarm(w: &World) -> Alarm {
    w.resource::<ShipAlarm>().level
}

fn counts(w: &World) -> ReactorCounts {
    *w.resource::<ReactorCounts>()
}

/// The size of the fire whose centre is within 0.6 m of a point (area metres), if there is one.
fn fire_at(w: &World, p: [f64; 2]) -> Option<f64> {
    air(w).deck.fires().iter().find(|f| (f.pos.x - p[0]).hypot(f.pos.y - p[1]) < 0.6).map(|f| f.size)
}

fn burning(w: &World) -> bool {
    w.resource::<HealthState>().bodies.get(&LOCAL).is_some_and(|b| b.health.burning())
}

fn dead(w: &World) -> bool {
    w.resource::<HealthState>().bodies.get(&LOCAL).is_some_and(|b| b.health.state() == Life::Dead)
}

/// The ground at an area point (metres), with a lift.
fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

/// LMB held or let go: the spray runs while it is held (the mouse is captured, no display in use).
fn lmb(w: &mut World, down: bool) {
    w.resource_mut::<Controls>().primary_held = down;
}

/// Walks the walker to a point beside a fire, aims at the fire's centre, sprays while LMB is held
/// until the fire is out (or 25 s), then lets go. The fire must be out, and the spray must reach
/// the host.
fn spray_out(name: &'static str, at: [f64; 2], stand: [f64; 2]) -> Vec<Step> {
    // The size before the spray and whether its checks ran are this step's own state.
    let mut size0 = 0.0;
    let mut checked = false;
    let step = Box::new(move |w: &mut World, c: &mut Ctx| {
        if c.t == 0.0 {
            begin(w, c, name);
            let feet = ground(w, stand, 0.0);
            place_walker(w, feet);
            size0 = fire_at(w, at).unwrap_or(0.0);
            lmb(w, true);
        }
        // The aim follows the flame's centre while the spray runs.
        let target = ground(w, at, FLAME_LIFT);
        look_at(w, target);
        if c.t < 1.0 {
            return false;
        }
        let size = fire_at(w, at);
        if !checked {
            checked = true;
            check(c, w.resource::<Sprays>().0.contains_key(&LOCAL), format!("{name}: the spray reaches the host"));
            check(c, size.is_some_and(|s| s < size0), format!("{name}: sprayed, the fire shrinks ({size0:.2} m to {size:?})"));
        }
        if size.is_none() || c.t > 25.0 {
            lmb(w, false);
            check(c, size.is_none(), format!("{name}: out after {:.1} s of spray (size {size:?})", c.t));
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        false
    });
    vec![step]
}

pub(super) fn fire_steps(s: &mut Vec<Step>) {
    // The table: the walker in front of it, the extinguisher taken into the hand.
    s.push(Box::new(|w, c| {
        begin(w, c, "fire: the table and the host's body for the walker");
        // The item messages take the same 75 ms each way as in the items scenario.
        w.resource_mut::<Items>().delay = DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "extinguisher").is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "fire: the table and the host's body for the walker are there".into());
        let p = ground(w, [ENGINEERING.centre[0] - 3.0, 1.6], 0.0);
        place_walker(w, p);
        end(w, c, "in front of the table".into());
        true
    }));
    s.extend(collect("extinguisher", "Extinguisher"));
    s.push(tap_then("fire: 1 takes the extinguisher into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "extinguisher")));

    // The dev fault: a fire at the reactor. It is in the air, the room's oxygen falls, the alarm
    // goes to Warn and then to Danger once the fire is a metre across.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire: the dev fault starts a fire at the reactor");
            let o0 = air(w).deck.room(air(w).engine).oxygen;
            c.v.insert("fire_o0", o0);
            c.v.insert("reactor_fresh", sim(w).room.reactor().condition().value());
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::Fire));
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let size = fire_at(w, ignition_at(Source::Reactor));
        check(c, size.is_some_and(|s| (START..START + 0.2).contains(&s)), format!("fire: a fire of about {START} m at the reactor ({size:?})"));
        check(c, counts(w).fires >= 1, format!("fire: the fire is counted ({:?})", counts(w)));
        c.v.insert("fire_s0", size.unwrap_or(0.0));
        end(w, c, format!("{size:?}"));
        true
    }));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire: it grows, uses the room's air, and the alarm follows its size");
            return false;
        }
        if c.t < 3.0 {
            return false;
        }
        let size = fire_at(w, ignition_at(Source::Reactor)).unwrap_or(0.0);
        let o = air(w).deck.room(air(w).engine).oxygen;
        let (s0, o0) = (c.v.get("fire_s0").copied().unwrap_or(0.0), c.v.get("fire_o0").copied().unwrap_or(0.0));
        check(c, size > s0 + 0.3, format!("fire: it grows: {s0:.2} m to {size:.2} m in 3 s"));
        check(c, o < o0, format!("fire: the engine room's oxygen falls: {o0:.2} m3 to {o:.2} m3"));
        // The fresh fire starts 0.3 m out from the reactor's front face: it burns the reactor at once.
        let (r0, r) = (c.v.get("reactor_fresh").copied().unwrap_or(0.0), sim(w).room.reactor().condition().value());
        check(c, r < r0 - 0.5, format!("fire: a fresh fire lowers the reactor's condition within 3 s: {r0:.1} to {r:.1}"));
        let lvl = alarm(w);
        check(c, lvl >= Alarm::Warn, format!("fire: the alarm is at least Warn ({lvl:?})"));
        if size >= 1.0 {
            check(c, lvl == Alarm::Danger, format!("fire: a fire of {size:.2} m is Danger ({lvl:?})"));
        }
        end(w, c, format!("{size:.2} m, alarm {lvl:?}"));
        true
    }));

    // A walker 1.2 m from the fire: the heat burns, the buildup fills, the walker is on fire.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire: a walker beside it catches fire");
            let at = ignition_at(Source::Reactor);
            let p = ground(w, [at[0] + 1.2, at[1]], 0.0);
            place_walker(w, p);
            return false;
        }
        if burning(w) {
            check(c, !dead(w), "fire: the walker is alive on fire".into());
            end(w, c, format!("on fire after {:.1} s", c.t));
            return true;
        }
        if c.t > 8.0 {
            check(c, false, "fire: the walker is on fire within 8 s".into());
            return true;
        }
        false
    }));

    // The pump and the junction box get fires of their own; the three parts lose condition.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire: the pump, the junction box and the reactor lose condition");
            let mut a = w.resource_mut::<EngineAir>();
            ignite(&mut a, Source::Pump, START);
            ignite(&mut a, Source::Junction, START);
            drop(a);
            let s = sim(w);
            c.v.insert("pump0", s.room.pump().condition().value());
            c.v.insert("junction0", s.room.junction().condition().value());
            c.v.insert("reactor0", s.room.reactor().condition().value());
            return false;
        }
        if c.t < 5.0 {
            return false;
        }
        let s = sim(w);
        let (p, j, r) = (s.room.pump().condition().value(), s.room.junction().condition().value(), s.room.reactor().condition().value());
        check(c, p < c.v["pump0"] - 0.5, format!("fire: the pump loses condition: {:.1} to {p:.1}", c.v["pump0"]));
        check(c, j < c.v["junction0"] - 0.5, format!("fire: the junction box loses condition: {:.1} to {j:.1}", c.v["junction0"]));
        check(c, r < c.v["reactor0"] - 0.5, format!("fire: the reactor loses condition: {:.1} to {r:.1}", c.v["reactor0"]));
        end(w, c, format!("pump {p:.1}, junction {j:.1}, reactor {r:.1}"));
        true
    }));

    // The extinguisher, from 3 m north of each fire: the reactor's, the pump's, the junction box's.
    let spray_names = [
        (Source::Reactor, "fire: the spray puts out the reactor's fire"),
        (Source::Pump, "fire: the spray puts out the pump's fire"),
        (Source::Junction, "fire: the spray puts out the junction box's fire"),
    ];
    for (src, name) in spray_names {
        let at = ignition_at(src);
        s.extend(spray_out(name, at, [at[0], at[1] + 3.0]));
    }

    // No fire left: the alarm is back to the reactor's level, which is calm.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire: no fire left, the alarm is back to the reactor's level");
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let fires = air(w).deck.fires().len();
        let lvl = alarm(w);
        check(c, fires == 0, format!("fire: no fire left in the air ({fires})"));
        // The reactor's own level stays: a part the fire burnt may need repair (Warn). The fire adds nothing.
        let room = alarm_of(sim(w).room.level());
        check(c, lvl == room, format!("fire: the alarm is back to the reactor's level ({room:?}, now {lvl:?})"));
        end(w, c, format!("{fires} fires, alarm {lvl:?}"));
        true
    }));
}
