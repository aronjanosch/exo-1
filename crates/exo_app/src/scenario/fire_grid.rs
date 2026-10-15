//! Fire grid: the cell model of the engine-room fire, switched on at the faults screen.
use super::fire::take_extinguisher;
use super::items::look_at;
use super::*;
use crate::controls::Controls;
use crate::dev_area::{layout::ignition_at, hull::ShipAir, hull::doors::EngineDoor};
use crate::dev_area::fire::{FireGrid, FireModel};
use crate::dev_area::reactor_screen::FAULTS_ID;
use crate::dev_area::DevArea;
use crate::displays::DisplayAction;
use crate::items::Items;
use item_core::wire::{Alarm, ReactorAsk, ReactorFault};
use reactor_core::Source;

/// The two cells of the engine room at the door (area metres): where the fire starts for the
/// corridor test. Two cells, not one: with the seeded rolls the corridor catches within 60 s.
const DOOR_CELLS: [[f64; 2]; 2] = [[-14.5, -11.5], [-13.5, -11.5]];
/// The height of the spray's aim at a cell's floor (m).
const AIM_LIFT: f64 = 0.3;

fn air(w: &World) -> &ShipAir {
    w.resource::<ShipAir>()
}

fn grid(w: &World) -> &FireGrid {
    w.resource::<FireGrid>()
}

fn burning(w: &World) -> usize {
    grid(w).0.burning().count()
}

/// The ground at an area point (metres), with a lift.
fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn cell_of(w: &World, p: [f64; 2]) -> usize {
    let a = air(w);
    grid(w).0.cell_at(a.engine, DVec3::new(p[0], p[1], 0.0)).expect("the engine room has cells")
}

fn intensity_at(w: &World, p: [f64; 2]) -> f64 {
    let a = air(w);
    grid(w).0.intensity(a.engine, cell_of(w, p))
}

pub(super) fn fire_grid_steps(s: &mut Vec<Step>) {
    take_extinguisher(s);
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire grid: the faults screen's button switches the fire to the cells");
            let ready = w.resource::<DevArea>().frame.is_some() && w.contains_resource::<FireGrid>();
            if !ready && c.t < 5.0 {
                return false;
            }
            w.write_message(DisplayAction { panel: FAULTS_ID, action: "fire_model".into(), value: None });
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let m = *w.resource::<FireModel>();
        check(c, m == FireModel::Grid, format!("fire grid: the model is Grid after the click ({m:?})"));
        end(w, c, format!("{m:?}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire grid: the dev fault starts one burning cell at the reactor");
            let o0 = air(w).deck.room(air(w).engine).oxygen;
            c.v.insert("o0", o0);
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::Fire));
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let at = ignition_at(Source::Reactor);
        let v = intensity_at(w, at);
        check(c, v > 0.0, format!("fire grid: the ignition cell burns ({v:.2})"));
        check(c, burning(w) == 1, format!("fire grid: one cell burns ({})", burning(w)));
        let alarm = w.resource::<crate::dev_area::hull::alarms::ShipAlarm>().level;
        check(c, alarm >= Alarm::Warn, format!("fire grid: the alarm is at least Warn ({alarm:?})"));
        end(w, c, format!("intensity {v:.2}"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t < 30.0 {
            return false;
        }
        let n = burning(w) as f64;
        let (o0, o) = (c.v["o0"], air(w).deck.room(air(w).engine).oxygen);
        check(c, o < o0, format!("fire grid: the engine room's oxygen falls: {o0:.2} m3 to {o:.2} m3"));
        end(w, c, format!("{n} cells"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire grid: the spray puts the reactor's front cell out");
            let at = ignition_at(Source::Reactor);
            let feet = ground(w, [at[0], at[1] + 3.0], 0.0);
            place_walker(w, feet);
            w.resource_mut::<Controls>().primary_held = true;
            return false;
        }
        let at = ignition_at(Source::Reactor);
        let target = ground(w, at, AIM_LIFT);
        look_at(w, target);
        let v = intensity_at(w, at);
        if c.t < 1.0 {
            return false;
        }
        if v > 0.0 && c.t < 25.0 {
            return false;
        }
        w.resource_mut::<Controls>().primary_held = false;
        check(c, v <= 0.0, format!("fire grid: the spray takes the cell to 0 after {:.1} s (now {v:.2})", c.t));
        end(w, c, format!("{:.1} s", c.t));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fire grid: with the door open the fire catches in the corridor");
            w.resource_mut::<EngineDoor>().open = true;
            let engine = air(w).engine;
            w.resource_scope(|_, mut g: Mut<FireGrid>| {
                for p in DOOR_CELLS {
                    let _ = g.0.ignite(engine, DVec3::new(p[0], p[1], 0.0), 1.0);
                }
            });
            c.v.insert("corridor0", grid(w).0.total(air(w).corridor));
            return false;
        }
        let corridor = grid(w).0.total(air(w).corridor);
        if corridor <= c.v["corridor0"] && c.t < 60.0 {
            return false;
        }
        check(c, w.resource::<EngineDoor>().open, "fire grid: the door is open".into());
        end(w, c, format!("corridor {corridor:.2} after {:.1} s", c.t));
        true
    }));
}
