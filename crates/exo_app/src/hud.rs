//! The minimal HUD's content (#91): mode, speed, altitude and the boost gauge. The simulation
//! fills `HudReadout` every fixed step, headless too, so scenarios check what the player sees;
//! the view only copies it into the text nodes (`view::update_flight_hud`).
//!
//! TODO(initiator): which elements, their words (one decimal on the speed, "SHIP  DECOUPLED")
//! and the gauge look are starting points (#91).
use crate::env::PlanetRes;
use crate::ship::Ship;
use crate::view::NEAR_PLANET;
use crate::walker::Player;
use bevy::math::DVec3;
use bevy::prelude::*;

/// What the player is doing, as the mode element names it.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Walk,
    Cabin,
    Suit,
    Fly,
    Ship { assist: bool, decoupled: bool },
    /// The quantum drive's phase while it is not idle.
    Quantum(String),
}

/// The boost element's source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Boost {
    None,
    /// No meter: the suit (decided "no fuel") or the ship's speed stage (`drain_time: 0`); only
    /// whether boost is held.
    Held(bool),
    /// The ship's capacitor (#90): charge 0..1, a boost is running, enough charge to start one.
    Ship { charge: f64, active: bool, ready: bool },
}

/// Inputs of one readout, plain values (unit-tested without a world).
#[derive(Clone, Debug, PartialEq)]
pub struct HudIn {
    pub mode: Mode,
    /// m/s, the player's speed (in a cabin the ship's plus the walker's).
    pub speed: f64,
    /// m above the reference sphere; `None` far from every planet.
    pub altitude: Option<f64>,
    pub boost: Boost,
}

/// The four permanent elements as shown, plus the gauge for the bar.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct HudReadout {
    /// Mode, speed, altitude, boost.
    pub texts: [String; 4],
    /// The ship's charge 0..1 for the bar; `None` outside the seat.
    pub gauge: Option<f64>,
    pub boosting: bool,
    /// Charge at or above the start charge (the bar is dimmed below).
    pub ready: bool,
}

/// Two decimals below 1 m/s, so a ship at rest can be told from a slow drift (issue #6).
pub fn speed_text(v: f64) -> String {
    if v < 1.0 { format!("{v:.2}") } else { format!("{v:.1}") }
}

pub fn readout(i: &HudIn) -> HudReadout {
    let mode = match &i.mode {
        Mode::Walk => "WALK".into(),
        Mode::Cabin => "CABIN".into(),
        Mode::Suit => "SUIT".into(),
        Mode::Fly => "FLY".into(),
        Mode::Ship { assist: false, .. } => "SHIP  ASSIST OFF".into(),
        Mode::Ship { decoupled: true, .. } => "SHIP  DECOUPLED".into(),
        Mode::Ship { .. } => "SHIP".into(),
        Mode::Quantum(phase) => format!("QUANTUM {}", phase.to_uppercase()),
    };
    let alt = i.altitude.map_or(String::new(), |a| format!("ALT {a:.0} m"));
    let (boost, gauge, boosting, ready) = match i.boost {
        Boost::None => (String::new(), None, false, false),
        Boost::Held(held) => (if held { "BOOST".into() } else { String::new() }, None, held, false),
        Boost::Ship { charge, active, ready } => (format!("BOOST {:.0} %", charge * 100.0), Some(charge), active, ready),
    };
    HudReadout { texts: [mode, format!("{} m/s", speed_text(i.speed)), alt, boost], gauge, boosting, ready }
}

/// Fills `HudReadout` after the step (fixed step, also headless).
pub fn update_readout(
    planet: Res<PlanetRes>,
    actions: Res<crate::controls::Actions>,
    wd: Res<crate::warp::WarpDrive>,
    players: Query<&Player>,
    ships: Query<(&Ship, &avian3d::prelude::Position, &avian3d::prelude::LinearVelocity, &avian3d::prelude::Rotation)>,
    mut out: ResMut<HudReadout>,
) {
    let (Ok(pl), Ok((ship, sp, sv, sr))) = (players.single(), ships.single()) else { return };
    let (mode, v, pos, boost) = if pl.seated {
        let mode = if wd.drive.phase != warp_core::Phase::Idle {
            Mode::Quantum(format!("{:?}", wd.drive.phase))
        } else {
            Mode::Ship { assist: ship.ctl.hover_assist, decoupled: !ship.ctl.coupled }
        };
        let (cap, t) = (&ship.ctl.boost, &ship.ctl.tuning.boost_capacitor);
        let boost = if t.drain_time <= 0.0 { Boost::Held(cap.active) } else { Boost::Ship { charge: cap.charge, active: cap.active, ready: cap.ready(t) } };
        (mode, sv.0, sp.0, boost)
    } else if pl.ship.is_some() {
        (Mode::Cabin, sv.0 + sr.0 * pl.w.vel, sp.0, Boost::None)
    } else if pl.fly {
        (Mode::Fly, pl.w.vel, pl.w.pos, Boost::None)
    } else if pl.body.is_some() {
        (Mode::Suit, pl.w.vel, pl.w.pos, Boost::Held(actions.boost))
    } else {
        (Mode::Walk, pl.w.vel, pl.w.pos, Boost::None)
    };
    let r: DVec3 = pos - planet.centre;
    let altitude = (r.length() < NEAR_PLANET).then(|| r.length() - planet.radius);
    let new = readout(&HudIn { mode, speed: v.length(), altitude, boost });
    if *out != new {
        *out = new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(charge: f64, active: bool, ready: bool) -> HudIn {
        HudIn { mode: Mode::Ship { assist: true, decoupled: false }, speed: 123.44, altitude: Some(450.4), boost: Boost::Ship { charge, active, ready } }
    }

    #[test]
    fn ship_shows_mode_speed_altitude_and_gauge() {
        let r = readout(&ship(0.744, true, true));
        assert_eq!(r.texts, ["SHIP".to_string(), "123.4 m/s".into(), "ALT 450 m".into(), "BOOST 74 %".into()]);
        assert_eq!(r.gauge, Some(0.744));
        assert!(r.boosting && r.ready);
    }

    #[test]
    fn mode_words() {
        let mut i = ship(1.0, false, true);
        i.mode = Mode::Ship { assist: false, decoupled: true };
        assert_eq!(readout(&i).texts[0], "SHIP  ASSIST OFF");
        i.mode = Mode::Ship { assist: true, decoupled: true };
        assert_eq!(readout(&i).texts[0], "SHIP  DECOUPLED");
        i.mode = Mode::Quantum("Cruise".into());
        assert_eq!(readout(&i).texts[0], "QUANTUM CRUISE");
        for (m, w) in [(Mode::Walk, "WALK"), (Mode::Cabin, "CABIN"), (Mode::Suit, "SUIT"), (Mode::Fly, "FLY")] {
            i.mode = m;
            assert_eq!(readout(&i).texts[0], w);
        }
    }

    #[test]
    fn far_from_planets_no_altitude_and_slow_speeds_get_two_decimals() {
        let r = readout(&HudIn { mode: Mode::Walk, speed: 0.256, altitude: None, boost: Boost::None });
        assert_eq!(r.texts, ["WALK".to_string(), "0.26 m/s".into(), String::new(), String::new()]);
        assert_eq!(r.gauge, None);
    }

    #[test]
    fn suit_shows_boost_only_while_held() {
        let i = |held| HudIn { mode: Mode::Suit, speed: 2.0, altitude: Some(10.0), boost: Boost::Held(held) };
        assert_eq!(readout(&i(true)).texts[3], "BOOST");
        assert_eq!(readout(&i(false)).texts[3], "");
        assert_eq!(readout(&i(true)).gauge, None);
    }

    #[test]
    fn no_debug_words_in_the_permanent_elements() {
        let r = readout(&ship(0.5, false, true));
        for t in &r.texts {
            for debug in ["ms", "chunk", "limit", "grounded", "coupling", "(H)", "(L)"] {
                assert!(!t.contains(debug), "{t:?} contains {debug:?}");
            }
        }
    }
}
