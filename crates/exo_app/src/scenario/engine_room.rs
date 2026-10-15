//! Engine room: the corridor, the door, the room's air and the alarm.
use super::*;
use crate::crew_hud::Vitals;
use crate::dev_area::{hull::ShipAir, hull::doors::{EngineDoor, Slab}, layout::{Place, CORRIDOR, DOOR_AT, ROOM}, hull::alarms::ShipAlarm};
use crate::dev_area::{area_of, DevArea};
use crate::interact::Interaction;
use crate::items::Items;
use avian3d::prelude::Position;
use bevy::math::DVec3;
use item_core::wire::{Alarm, RoomAsk};

/// A point on the ground at area metres.
fn ground_at(w: &mut World, p: [f64; 2]) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, 0.0).expect("the dev area is there").0
}

/// The walker's place in the area, in metres.
fn area_of_walker(w: &mut World) -> [f64; 2] {
    let pl = planet(w);
    let f = w.resource::<DevArea>().frame.expect("the dev area is there");
    let at = player_world(w);
    area_of(&f, &pl, at)
}

fn place(w: &mut World) -> Place {
    crate::dev_area::layout::place_of(area_of_walker(w))
}

fn prompt(w: &World) -> String {
    w.resource::<Interaction>().prompt.clone()
}

fn gap_open(w: &World) -> f64 {
    let a = w.resource::<ShipAir>();
    a.deck.gap(a.door).open
}

fn room_air(w: &World) -> f64 {
    let a = w.resource::<ShipAir>();
    a.oxygen_at(ROOM.centre)
}

fn door_centre(w: &mut World) -> DVec3 {
    w.query::<(&Slab, &Position)>().iter(w).next().map(|(_, p)| p.0).expect("the door is there")
}

/// The walker's oxygen as the HUD shows it (None while the air is good).
fn oxygen_shown(w: &World) -> Option<f64> {
    w.resource::<Vitals>().oxygen
}

pub(super) fn engine_room_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: into the corridor");
            if w.resource::<DevArea>().frame.is_none() && c.t < 5.0 {
                return false;
            }
            let p = ground_at(w, [CORRIDOR.centre[0], -6.0]);
            place_walker(w, p);
            return false;
        }
        if c.t < 1.0 {
            return false;
        }
        let at = place(w);
        check(c, at == Place::Corridor, format!("engine room: the walker is in the corridor ({at:?})"));
        end(w, c, format!("{at:?}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: walls block");
            let p = ground_at(w, [CORRIDOR.centre[0], -6.0]);
            place_walker(w, p);
            let target = ground_at(w, [-22.0, -6.0]);
            face_towards(w, target);
            c.v.insert("x0", area_of_walker(w)[0]);
            keys(w, &[KeyCode::KeyW], true);
            return false;
        }
        if c.t < 2.0 {
            return false;
        }
        keys(w, &[KeyCode::KeyW], false);
        let x = area_of_walker(w)[0];
        // The corridor's west wall stands at its edge, x -15: the walker's centre stays east of it.
        let wall_face = CORRIDOR.centre[0] - CORRIDOR.half[0];
        check(c, x > wall_face, format!("engine room: the corridor's west wall stops the walker ({:.2} m from the start, x {x:.2})", c.v["x0"] - x));
        end(w, c, format!("x {x:.2}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: F opens the door");
            let p = ground_at(w, [DOOR_AT[0], -9.0]);
            place_walker(w, p);
            let door = door_centre(w);
            face_towards(w, door);
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        if !c.v.contains_key("tapped_open") {
            c.v.insert("tapped_open", 1.0);
            let p = prompt(w);
            check(c, p.starts_with("[F] open"), format!("engine room: the prompt offers to open the door ({p:?})"));
            tap(w, KeyCode::KeyF);
            return false;
        }
        if c.t > 6.0 {
            check(c, false, "engine room: the door did not open within 6 s".into());
            return true;
        }
        if !w.resource::<EngineDoor>().open || c.t < 2.5 {
            return false;
        }
        let g = gap_open(w);
        check(c, (g - 1.0).abs() < 1e-9, format!("engine room: the door is open, the gap open {g:.2}"));
        end(w, c, format!("gap {g:.2}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: into the room");
            let p = ground_at(w, [-14.0, -13.0]);
            place_walker(w, p);
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let at = place(w);
        check(c, at == Place::Engine, format!("engine room: the walker is in the engine room ({at:?})"));
        end(w, c, format!("{at:?}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: the air runs low");
            let r = w.resource::<ShipAir>().engine;
            w.resource_mut::<ShipAir>().deck.room_mut(r).oxygen = 0.0;
            return false;
        }
        if c.t < 4.0 {
            return false;
        }
        let o2 = oxygen_shown(w);
        check(c, o2.is_some_and(|o| o < 1.0), format!("engine room: the walker's air shows on the HUD ({o2:?}), the room at {:.2}", room_air(w)));
        end(w, c, format!("{o2:?}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: the air comes back");
            let r = w.resource::<ShipAir>().engine;
            let mut a = w.resource_mut::<ShipAir>();
            let full = a.deck.room(r).volume;
            a.deck.room_mut(r).oxygen = full;
            return false;
        }
        if c.t < 3.0 {
            return false;
        }
        let air = room_air(w);
        check(c, air > 0.9, format!("engine room: the room's air is back ({air:.2})"));
        end(w, c, format!("{air:.2}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "engine room: F shuts the door from inside");
            let p = ground_at(w, [-12.0, -13.5]);
            place_walker(w, p);
            let door = door_centre(w);
            face_towards(w, door);
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        if !c.v.contains_key("tapped_shut") {
            c.v.insert("tapped_shut", 1.0);
            let p = prompt(w);
            check(c, p.starts_with("[F] close"), format!("engine room: the prompt offers to close the door ({p:?})"));
            tap(w, KeyCode::KeyF);
            return false;
        }
        if c.t > 6.0 {
            check(c, false, "engine room: the door did not shut within 6 s".into());
            return true;
        }
        if w.resource::<EngineDoor>().open || c.t < 2.5 {
            return false;
        }
        let g = gap_open(w);
        check(c, g.abs() < 1e-9, format!("engine room: the door is shut, the gap open {g:.2}"));
        end(w, c, format!("gap {g:.2}"));
        true
    }));

    for (name, want) in [("Warn", Alarm::Warn), ("Danger", Alarm::Danger), ("None", Alarm::None)] {
        s.push(Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, &format!("engine room: the alarm to {name}"));
                w.resource_mut::<Items>().ask_room(RoomAsk::CycleAlarm);
                return false;
            }
            if c.t < 0.2 {
                return false;
            }
            let level = w.resource::<ShipAlarm>().level;
            check(c, level == want, format!("engine room: the alarm is {level:?} (wanted {want:?})"));
            end(w, c, format!("{level:?}"));
            true
        }));
    }
}
