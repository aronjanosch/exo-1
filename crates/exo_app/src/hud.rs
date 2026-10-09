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
pub use flight_core::hud::{Height, HudTuning};

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
    /// The suit has no meter (decided "no fuel"): only whether boost is held.
    Held(bool),
    /// The ship's speed stage of #24 (F6 dev switch or `drain_time: 0`): held or not.
    Stage(bool),
    /// The ship's capacitor (#90): charge 0..1, a boost is running, enough charge to start one.
    Ship { charge: f64, active: bool, ready: bool },
}

/// Inputs of one readout, plain values (unit-tested without a world).
#[derive(Clone, Debug, PartialEq)]
pub struct HudIn {
    pub mode: Mode,
    /// m/s, the player's speed (in a cabin the ship's plus the walker's).
    pub speed: f64,
    /// `None` far from every planet.
    pub altitude: Option<Height>,
    pub boost: Boost,
    /// The flight model's word (F7, spike 13); empty off the seat.
    pub flight_model: &'static str,
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
    /// Next to the bar: which boost the ship flies, `CAPACITOR` or `STAGE` (F6); empty off the seat.
    pub boost_mode: &'static str,
    /// After it: which flight model the ship flies (F7, spike 13); empty off the seat.
    pub flight_model: &'static str,
}

/// The model's HUD word; the axis model adds `LANDING` in landing mode. TODO(initiator): words
/// (spike 13).
pub fn model_word(model: flight_core::FlightModel, landing: bool) -> &'static str {
    match model {
        flight_core::FlightModel::Axis if landing => "AXIS LANDING",
        m => m.label(),
    }
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
    let alt = match i.altitude {
        Some(Height::Agl(h)) => format!("AGL {h:.0} m"),
        Some(Height::Alt(h)) => format!("ALT {h:.0} m"),
        None => String::new(),
    };
    let (boost, gauge, boosting, ready, boost_mode) = match i.boost {
        Boost::None => (String::new(), None, false, false, ""),
        Boost::Held(held) => (if held { "BOOST".into() } else { String::new() }, None, held, false, ""),
        Boost::Stage(held) => (if held { "BOOST ON".into() } else { "BOOST".into() }, None, held, true, "STAGE"),
        Boost::Ship { charge, active, ready } => (format!("BOOST {:.0} %", charge * 100.0), Some(charge), active, ready, "CAPACITOR"),
    };
    HudReadout { texts: [mode, format!("{} m/s", speed_text(i.speed)), alt, boost], gauge, boosting, ready, boost_mode, flight_model: i.flight_model }
}

/// Fills `HudReadout` after the step (fixed step, also headless).
pub fn update_readout(
    planet: Res<PlanetRes>,
    tuning: Res<crate::tuning::Tuning>,
    actions: Res<crate::controls::Actions>,
    wd: Res<crate::warp::WarpDrive>,
    players: Query<&Player>,
    ships: Query<(&Ship, &avian3d::prelude::Position, &avian3d::prelude::LinearVelocity, &avian3d::prelude::Rotation)>,
    mut out: ResMut<HudReadout>,
) {
    let (Ok(pl), Ok((ship, sp, sv, sr))) = (players.single(), ships.single()) else { return };
    let model = if pl.seated { model_word(ship.ctl.model, ship.ctl.landing_mode) } else { "" };
    let (mode, v, pos, boost) = if pl.seated {
        let mode = if wd.drive.phase != warp_core::Phase::Idle {
            Mode::Quantum(format!("{:?}", wd.drive.phase))
        } else {
            Mode::Ship { assist: ship.ctl.hover_assist, decoupled: !ship.ctl.coupled }
        };
        let (cap, t) = (&ship.ctl.boost, &ship.ctl.tuning.boost_capacitor);
        let boost = if ship.ctl.boost_stage || t.drain_time <= 0.0 { Boost::Stage(cap.active) } else { Boost::Ship { charge: cap.charge, active: cap.active, ready: cap.ready(t) } };
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
    let altitude = (r.length() < NEAR_PLANET).then(|| Height::pick(planet.above_ground(pos), r.length() - planet.radius, tuning.hud.agl_below));
    let new = readout(&HudIn { mode, speed: v.length(), altitude, boost, flight_model: model });
    if *out != new {
        *out = new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(charge: f64, active: bool, ready: bool) -> HudIn {
        HudIn { mode: Mode::Ship { assist: true, decoupled: false }, speed: 123.44, altitude: Some(Height::Alt(450.4)), boost: Boost::Ship { charge, active, ready }, flight_model: "CLASSIC" }
    }

    #[test]
    fn ship_shows_mode_speed_altitude_and_gauge() {
        let r = readout(&ship(0.744, true, true));
        assert_eq!(r.texts, ["SHIP".to_string(), "123.4 m/s".into(), "ALT 450 m".into(), "BOOST 74 %".into()]);
        assert_eq!(r.gauge, Some(0.744));
        assert!(r.boosting && r.ready);
        assert_eq!(r.boost_mode, "CAPACITOR");
        assert_eq!(r.flight_model, "CLASSIC");
    }

    #[test]
    fn model_words() {
        use flight_core::FlightModel::*;
        assert_eq!(model_word(Classic, true), "CLASSIC");
        assert_eq!(model_word(Axis, false), "AXIS");
        assert_eq!(model_word(Axis, true), "AXIS LANDING");
    }

    #[test]
    fn the_speed_stage_shows_its_mode_and_no_gauge() {
        let mut i = ship(1.0, false, true);
        i.boost = Boost::Stage(true);
        let r = readout(&i);
        assert_eq!((r.texts[3].as_str(), r.gauge, r.boosting, r.boost_mode), ("BOOST ON", None, true, "STAGE"));
        i.boost = Boost::Stage(false);
        assert_eq!(readout(&i).texts[3], "BOOST");
    }

    #[test]
    fn agl_near_the_ground_alt_above() {
        assert_eq!(Height::pick(120.0, 80.0, 1000.0), Height::Agl(120.0));
        assert_eq!(Height::pick(1000.0, 980.0, 1000.0), Height::Alt(980.0));
        let mut i = ship(1.0, false, true);
        i.altitude = Some(Height::Agl(120.4));
        assert_eq!(readout(&i).texts[2], "AGL 120 m");
        i.altitude = Some(Height::Alt(4500.0));
        assert_eq!(readout(&i).texts[2], "ALT 4500 m");
    }

    #[test]
    fn shipped_file_loads_and_bad_values_are_refused() {
        assert_eq!(HudTuning::from_json(crate::tuning::HUD).unwrap(), HudTuning { agl_below: 1000.0 });
        assert!(HudTuning::from_json("{\"agl_below\": -1}").is_err());
        assert!(HudTuning::from_json("{\"agl_below\": 1, \"x\": 2}").is_err());
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
        let r = readout(&HudIn { mode: Mode::Walk, speed: 0.256, altitude: None, boost: Boost::None, flight_model: "" });
        assert_eq!((r.boost_mode, r.flight_model), ("", ""));
        assert_eq!(r.texts, ["WALK".to_string(), "0.26 m/s".into(), String::new(), String::new()]);
        assert_eq!(r.gauge, None);
    }

    #[test]
    fn suit_shows_boost_only_while_held() {
        let i = |held| HudIn { mode: Mode::Suit, speed: 2.0, altitude: Some(Height::Agl(10.0)), boost: Boost::Held(held), flight_model: "" };
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
