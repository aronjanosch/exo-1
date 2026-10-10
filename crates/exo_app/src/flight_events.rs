//! Flight events for the licence exam (#169): the ship's moments as domain events, nothing more.
//! `TookOff` when the ship leaves the ground under the pilot, `PadReached` when it comes over a
//! pad, `Landed` when it touches down, with the speed towards the ground just before. The exam
//! reads these events and never how the ship flies, so the flight model can change under it.
//!
//! The state machine is `FlightWatch`, plain data without Bevy, so it can be tested; the system
//! feeds it once per fixed step.
use bevy::math::DVec3;
use bevy::prelude::*;
use gameplay_core::{LocationId, WorldEvent};

use crate::gameplay::{Gameplay, HOST};

/// A ship this near a pad (m above its centre plane) in the air counts as having reached it.
/// TODO(initiator): a starting value.
pub const PAD_OVERFLIGHT_M: f64 = 120.0;
/// How many fixed steps the approach speed is remembered: the contact comes a step after the
/// solver has cut the approach.
const APPROACH_STEPS: usize = 10;

pub fn plugin(app: &mut App) {
    app.init_resource::<FlightWatch>();
    app.add_systems(FixedUpdate, watch_flight.after(crate::cargo::budget_step).in_set(crate::phases::Fx::Effects));
}

/// A pad the ship may be over.
#[derive(Clone, Debug, PartialEq)]
pub struct PadView {
    pub location: LocationId,
    /// Distance across the ground from the pad's centre (m) and height over its plane (m).
    pub across_m: f64,
    pub height_m: f64,
    pub radius_m: f64,
}

#[derive(Resource, Debug)]
pub struct FlightWatch {
    landed: bool,
    /// The last approach speeds, newest last.
    recent: Vec<f64>,
    /// Pads reached and not left since.
    over: Vec<LocationId>,
}

impl Default for FlightWatch {
    fn default() -> FlightWatch {
        FlightWatch { landed: true, recent: Vec::new(), over: Vec::new() }
    }
}

impl FlightWatch {
    /// One step. `piloted`: the player sits in the seat. `landed`: the ship's state. `approach`:
    /// speed towards the ground (m/s, negative when climbing). `pads`: where the ship is to each
    /// pad of the planet. Returns the events of this step.
    pub fn step(&mut self, piloted: bool, landed: bool, approach: f64, pads: &[PadView]) -> Vec<WorldEvent> {
        self.recent.push(approach.max(0.0));
        if self.recent.len() > APPROACH_STEPS {
            self.recent.remove(0);
        }
        let mut out = Vec::new();
        if !piloted {
            // Nobody flies it: no events, but keep the state so a hand-over does not count.
            self.landed = landed;
            self.over.clear();
            return out;
        }
        if self.landed && !landed {
            out.push(WorldEvent::TookOff);
        }
        // Over a pad: in the air within its radius and the overflight height; once per visit.
        let here: Vec<&PadView> = pads.iter().filter(|p| !landed && p.across_m <= p.radius_m && (0.0..=PAD_OVERFLIGHT_M).contains(&p.height_m)).collect();
        for p in &here {
            if !self.over.contains(&p.location) {
                out.push(WorldEvent::PadReached { at: p.location.clone() });
            }
        }
        self.over = here.iter().map(|p| p.location.clone()).collect();
        if !self.landed && landed {
            let at = pads.iter().find(|p| p.across_m <= p.radius_m && p.height_m <= 6.0).map(|p| p.location.clone());
            out.push(WorldEvent::Landed { at, speed: self.recent.iter().copied().fold(0.0, f64::max) });
        }
        self.landed = landed;
        out
    }
}

fn watch_flight(
    mut watch: ResMut<FlightWatch>,
    mut gp: ResMut<Gameplay>,
    players: Query<&crate::walker::Player>,
    ships: Query<(&crate::ship::Ship, &avian3d::prelude::Position, &avian3d::prelude::LinearVelocity)>,
    planet: Res<crate::env::PlanetRes>,
) {
    let (Ok(pl), Ok((ship, pos, lv))) = (players.single(), ships.single()) else { return };
    let up = planet.up(pos.0);
    let pads: Vec<PadView> = gp
        .pads
        .iter()
        .map(|p| {
            let d: DVec3 = pos.0 - p.centre;
            let h = d.dot(p.up);
            PadView { location: p.location.clone(), across_m: (d - p.up * h).length(), height_m: h, radius_m: p.radius }
        })
        .collect();
    for e in watch.step(pl.seated && ship.piloted, ship.lag.landed, -lv.0.dot(up), &pads) {
        gp.push_world(HOST, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(across: f64, height: f64) -> Vec<PadView> {
        vec![PadView { location: LocationId::new("drip_rock"), across_m: across, height_m: height, radius_m: 15.0 }]
    }

    #[test]
    fn take_off_pad_and_landing_come_as_events_in_order() {
        let mut w = FlightWatch::default();
        let none: Vec<PadView> = Vec::new();
        assert!(w.step(true, true, 0.0, &none).is_empty(), "sitting on the ground is nothing");
        assert_eq!(w.step(true, false, -3.0, &none), vec![WorldEvent::TookOff]);
        assert!(w.step(true, false, -3.0, &none).is_empty(), "once");
        // Over the pad at 40 m: reached, once.
        assert_eq!(w.step(true, false, 0.0, &pad(5.0, 40.0)), vec![WorldEvent::PadReached { at: LocationId::new("drip_rock") }]);
        assert!(w.step(true, false, 2.0, &pad(5.0, 30.0)).is_empty());
        // Down at 4.5 m/s, then contact: landed on the pad with the approach just before.
        w.step(true, false, 4.5, &pad(3.0, 5.0));
        let ev = w.step(true, true, 0.0, &pad(3.0, 1.0));
        assert_eq!(ev, vec![WorldEvent::Landed { at: Some(LocationId::new("drip_rock")), speed: 4.5 }]);
    }

    #[test]
    fn a_landing_off_the_pad_has_no_place_and_too_high_or_too_far_is_not_reaching() {
        let mut w = FlightWatch::default();
        w.step(true, false, 0.0, &pad(50.0, 10.0));
        assert!(w.step(true, false, 0.0, &pad(50.0, 10.0)).is_empty(), "off the pad");
        assert!(w.step(true, false, 0.0, &pad(5.0, 500.0)).is_empty(), "far above");
        let ev = w.step(true, true, 0.0, &pad(50.0, 1.0));
        assert!(matches!(ev.as_slice(), [WorldEvent::Landed { at: None, .. }]), "{ev:?}");
    }

    #[test]
    fn leaving_a_pad_and_coming_back_reaches_it_again() {
        let mut w = FlightWatch::default();
        w.step(true, false, 0.0, &pad(50.0, 10.0));
        assert_eq!(w.step(true, false, 0.0, &pad(5.0, 40.0)).len(), 1);
        assert!(w.step(true, false, 0.0, &pad(60.0, 40.0)).is_empty());
        assert_eq!(w.step(true, false, 0.0, &pad(5.0, 40.0)).len(), 1);
    }

    #[test]
    fn nobody_in_the_seat_means_no_events_and_no_phantom_take_off() {
        let mut w = FlightWatch::default();
        let none: Vec<PadView> = Vec::new();
        assert!(w.step(false, false, 0.0, &none).is_empty(), "a drifting ship nobody flies");
        // The pilot sits down in the air: not a take-off.
        assert!(w.step(true, false, 0.0, &none).is_empty());
        // Climbing speed is no approach.
        w.step(true, false, -50.0, &none);
        let ev = w.step(true, true, 0.0, &pad(1.0, 1.0));
        assert!(matches!(ev.as_slice(), [WorldEvent::Landed { speed, .. }] if *speed == 0.0), "{ev:?}");
    }
}
