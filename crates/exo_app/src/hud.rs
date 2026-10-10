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

pub fn plugin(app: &mut App) {
    app.init_resource::<HudReadout>();
    app.add_systems(FixedUpdate, update_readout.in_set(crate::phases::Fx::Effects));
}
pub use flight_core::hud::{Height, HudTuning};

/// What the player is doing, as the mode element names it.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Walk,
    Cabin,
    Suit,
    Fly,
    Ship { assist: bool, decoupled: bool },
    /// The SC flight model (F7, round 5): the switches a pilot most needs to see. The full flight
    /// panel is lane `sc-hud`'s.
    ShipSc { decoupled: bool, grav_comp: bool, nav: bool },
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

/// The flight panel's switches and numbers (#197), plain values. `None` off the seat.
#[derive(Clone, Debug, PartialEq)]
pub enum Panel {
    None,
    Sc {
        coupled: bool,
        /// 1 = coupled, 0 = decoupled; between: the blend runs.
        coupling: f64,
        grav_comp: bool,
        g_safe: bool,
        comstab: bool,
        proximity: bool,
        wind_comp: bool,
        nav: bool,
        /// Speed limiter share of the cap, 1 = off.
        limiter: f64,
        braking: bool,
        /// m/s, the cap in force.
        cap: f64,
        /// g, the felt acceleration.
        felt_g: f64,
    },
    Axis {
        assist: bool,
        coupled: bool,
        coupling: f64,
        braking: bool,
        /// m/s, the forward speed limit.
        cap: f64,
        felt_g: f64,
    },
}

/// Inputs of one readout, plain values (unit-tested without a world).
#[derive(Clone, Debug, PartialEq)]
pub struct HudIn {
    pub mode: Mode,
    pub panel: Panel,
    /// m/s, the player's speed (in a cabin the ship's plus the walker's).
    pub speed: f64,
    /// `None` far from every planet.
    pub altitude: Option<Height>,
    pub boost: Boost,
    /// `LANDING` in landing mode (K); empty otherwise and off the seat.
    pub landing: &'static str,
}

/// One switch or state of the flight panel. `on` is bright, off dimmed. `key` names it across
/// steps (a label may change while the key stays, e.g. `LIMIT 90 %` to `LIMIT 80 %`). A `pair`
/// half (COUPLED/DECOUPLED, SCM/NAV) toasts only its on change; its partner's on change says it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Badge {
    pub key: &'static str,
    pub label: String,
    pub on: bool,
    pub pair: bool,
    /// Held, not switched (BRAKE): shown, never toasted.
    pub quiet: bool,
}

impl Badge {
    fn new(key: &'static str, label: impl Into<String>, on: bool) -> Badge {
        Badge { key, label: label.into(), on, pair: false, quiet: false }
    }
    fn half(key: &'static str, on: bool) -> Badge {
        Badge { key, label: key.into(), on, pair: true, quiet: false }
    }
    fn held(key: &'static str, on: bool) -> Badge {
        Badge { quiet: true, ..Badge::new(key, key, on) }
    }
}

/// The panel's badge slots (the most the SC model shows at once).
pub const BADGE_SLOTS: usize = 13;
/// Seconds a toast stays in the tests (the game reads `hud.json` `toast_time`).
#[cfg(test)]
const TOAST_TIME: f64 = 1.5;
/// The coupling blend's bar shows between these two values (0 and 1 are at rest).
const BLEND_EPS: f64 = 1e-6;

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
    /// After it: `LANDING` in landing mode (K); empty otherwise and off the seat.
    pub landing: &'static str,
    /// The flight panel's badges in order (empty off the seat).
    pub badges: Vec<Badge>,
    /// The coupling blend 0..1 while it moves; `None` at rest.
    pub blend: Option<f64>,
    /// `123 / 150 m/s`: speed against the cap; empty off the seat.
    pub cap_text: String,
    /// `2.3 g`: the felt acceleration; empty off the seat.
    pub g_text: String,
    /// The last change, for `TOAST_TIME` s; set by `Toaster::step` in `update_readout`.
    pub toast: Option<String>,
}

/// The landing mode's HUD word. TODO(initiator): the word (spike 13).
pub fn landing_word(landing: bool) -> &'static str {
    if landing { "LANDING" } else { "" }
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
        Mode::ShipSc { decoupled, grav_comp, nav } => {
            format!("SHIP SC  {}  {}{}", if *nav { "NAV" } else { "SCM" }, if *decoupled { "DECOUPLED" } else { "COUPLED" }, if *grav_comp { "" } else { "  NO GRAV COMP" })
        }
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
    let (blend, cap_text, g_text) = match &i.panel {
        Panel::None => (None, String::new(), String::new()),
        Panel::Sc { coupling, cap, felt_g, .. } | Panel::Axis { coupling, cap, felt_g, .. } => {
            (blend_of(*coupling), format!("{:.0} / {cap:.0} m/s", i.speed), format!("{felt_g:.1} g"))
        }
    };
    HudReadout {
        texts: [mode, format!("{} m/s", speed_text(i.speed)), alt, boost],
        gauge,
        boosting,
        ready,
        boost_mode,
        landing: i.landing,
        badges: badges(i),
        blend,
        cap_text,
        g_text,
        toast: None,
    }
}

/// The coupling while it moves; `None` when coupled or decoupled.
pub fn blend_of(coupling: f64) -> Option<f64> {
    (coupling > BLEND_EPS && coupling < 1.0 - BLEND_EPS).then_some(coupling)
}

/// The model's word (`SC`, `AXIS`); empty off the seat.
pub fn model_word(p: &Panel) -> &'static str {
    match p {
        Panel::None => "",
        Panel::Sc { .. } => "SC",
        Panel::Axis { .. } => "AXIS",
    }
}

fn coupling_badges(b: &mut Vec<Badge>, coupled: bool) {
    b.push(Badge::half("COUPLED", coupled));
    b.push(Badge::half("DECOUPLED", !coupled));
}

/// The panel's badges in order (#197): the model, the coupling pair, the switches, the limiter
/// (only below 100 %), landing and brake.
pub fn badges(i: &HudIn) -> Vec<Badge> {
    let mut b = Vec::new();
    let landing = Badge::new("LANDING", "LANDING", !i.landing.is_empty());
    match &i.panel {
        Panel::None => return b,
        Panel::Sc { coupled, grav_comp, g_safe, comstab, proximity, wind_comp, nav, limiter, braking, .. } => {
            b.push(Badge::new("MODEL", "SC", true));
            coupling_badges(&mut b, *coupled);
            b.push(Badge::new("GRAV COMP", "GRAV COMP", *grav_comp));
            b.push(Badge::new("G-SAFE", "G-SAFE", *g_safe));
            b.push(Badge::new("COMSTAB", "COMSTAB", *comstab));
            b.push(Badge::new("PROX", "PROX", *proximity));
            b.push(Badge::new("WIND", "WIND", *wind_comp));
            b.push(Badge::half("SCM", !*nav));
            b.push(Badge::half("NAV", *nav));
            if *limiter < 1.0 {
                b.push(Badge::new("LIMIT", format!("LIMIT {:.0} %", limiter * 100.0), true));
            }
            b.push(landing);
            b.push(Badge::held("BRAKE", *braking));
        }
        Panel::Axis { assist, coupled, braking, .. } => {
            b.push(Badge::new("MODEL", "AXIS", true));
            b.push(Badge::new("ASSIST", "ASSIST", *assist));
            coupling_badges(&mut b, *coupled);
            b.push(landing);
            b.push(Badge::held("BRAKE", *braking));
        }
    }
    b
}

/// The first change of the badges since the last step, as its toast: an on switch shows its
/// label, an off one `KEY OFF` (not for a pair half).
fn first_change(prev: &[Badge], now: &[Badge]) -> Option<String> {
    let mut changes = Vec::new();
    for b in now.iter().filter(|b| !b.quiet) {
        match prev.iter().find(|p| p.key == b.key) {
            Some(p) if p.on == b.on && p.label == b.label => {}
            _ if b.on => changes.push(b.label.clone()),
            _ if !b.pair => changes.push(format!("{} OFF", b.key)),
            _ => {}
        }
    }
    // A badge that left the panel (LIMIT back at 100 %) went off.
    for p in prev {
        if p.on && !p.pair && !now.iter().any(|b| b.key == p.key) {
            changes.push(format!("{} OFF", p.key));
        }
    }
    changes.into_iter().next()
}

/// Turns the badges' changes into toasts (#197). The first step only records the state. A model
/// switch toasts `MODEL SC` or `MODEL AXIS` alone. A later change replaces the toast.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toaster {
    prev: Option<Vec<Badge>>,
    model: &'static str,
    left: f64,
    text: Option<String>,
}

impl Toaster {
    /// One fixed step of `dt` s; returns the toast to show now.
    pub fn step(&mut self, badges: &[Badge], model: &'static str, dt: f64, time: f64) -> Option<String> {
        let change = match &self.prev {
            None => None,
            Some(_) if model != self.model => Some(format!("MODEL {model}")),
            Some(prev) => first_change(prev, badges),
        };
        match change {
            Some(t) => {
                self.text = Some(t);
                self.left = time;
            }
            None => {
                self.left -= dt;
                if self.left <= 0.0 {
                    self.text = None;
                }
            }
        }
        self.prev = Some(badges.to_vec());
        self.model = model;
        self.text.clone()
    }
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
    time: Res<Time>,
    mut toaster: Local<Toaster>,
) {
    let (Ok(pl), Ok((ship, sp, sv, sr))) = (players.single(), ships.single()) else { return };
    let sc = ship.model == crate::ship::FlightModel::Sc;
    let landing = if pl.seated { landing_word(if sc { ship.sc.modes.landing } else { ship.ctl.landing_mode }) } else { "" };
    let (mode, v, pos, boost) = if pl.seated {
        let mode = if wd.drive.phase != warp_core::Phase::Idle {
            Mode::Quantum(format!("{:?}", wd.drive.phase))
        } else if sc {
            let m = &ship.sc.modes;
            Mode::ShipSc { decoupled: !m.coupled, grav_comp: m.grav_comp, nav: m.master == flight_core::sc::Master::Nav }
        } else {
            Mode::Ship { assist: ship.ctl.hover_assist, decoupled: !ship.ctl.coupled }
        };
        let (cap, t, stage) = if sc { (&ship.sc.drive.boost, &ship.sc.tuning.drive.boost_capacitor, false) } else { (&ship.ctl.boost, &ship.ctl.tuning.boost_capacitor, ship.ctl.boost_stage) };
        let boost = if stage || t.drain_time <= 0.0 { Boost::Stage(cap.active) } else { Boost::Ship { charge: cap.charge, active: cap.active, ready: cap.ready(t) } };
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
    let panel = if !pl.seated {
        Panel::None
    } else if sc {
        let s = &ship.sc.status;
        Panel::Sc {
            coupled: s.coupled,
            coupling: s.coupling,
            grav_comp: s.grav_comp,
            g_safe: s.g_safe,
            comstab: s.comstab,
            proximity: s.proximity,
            wind_comp: s.wind_comp,
            nav: s.master == flight_core::sc::Master::Nav,
            limiter: s.limiter,
            braking: s.braking,
            cap: s.cap,
            felt_g: s.felt_g,
        }
    } else {
        Panel::Axis { assist: ship.ctl.hover_assist, coupled: ship.ctl.coupled, coupling: ship.ctl.coupling, braking: ship.ctl.brake_active, cap: ship.ctl.forward_speed_limit, felt_g: ship.ctl.axis.felt_g }
    };
    let mut new = readout(&HudIn { mode, panel: panel.clone(), speed: v.length(), altitude, boost, landing });
    new.toast = toaster.step(&new.badges, model_word(&panel), time.delta_secs_f64(), tuning.hud.toast_time);
    if *out != new {
        *out = new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(charge: f64, active: bool, ready: bool) -> HudIn {
        HudIn { mode: Mode::Ship { assist: true, decoupled: false }, panel: Panel::None, speed: 123.44, altitude: Some(Height::Alt(450.4)), boost: Boost::Ship { charge, active, ready }, landing: "" }
    }

    #[test]
    fn ship_shows_mode_speed_altitude_and_gauge() {
        let r = readout(&ship(0.744, true, true));
        assert_eq!(r.texts, ["SHIP".to_string(), "123.4 m/s".into(), "ALT 450 m".into(), "BOOST 74 %".into()]);
        assert_eq!(r.gauge, Some(0.744));
        assert!(r.boosting && r.ready);
        assert_eq!(r.boost_mode, "CAPACITOR");
        assert_eq!(r.landing, "");
    }

    #[test]
    fn landing_words() {
        assert_eq!(landing_word(false), "");
        assert_eq!(landing_word(true), "LANDING");
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
        assert_eq!(HudTuning::from_json(crate::tuning::HUD).unwrap(), HudTuning { agl_below: 1000.0, toast_time: 1.5 });
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
        let r = readout(&HudIn { mode: Mode::Walk, panel: Panel::None, speed: 0.256, altitude: None, boost: Boost::None, landing: "" });
        assert_eq!((r.boost_mode, r.landing), ("", ""));
        assert_eq!(r.texts, ["WALK".to_string(), "0.26 m/s".into(), String::new(), String::new()]);
        assert_eq!(r.gauge, None);
    }

    #[test]
    fn suit_shows_boost_only_while_held() {
        let i = |held| HudIn { mode: Mode::Suit, panel: Panel::None, speed: 2.0, altitude: Some(Height::Agl(10.0)), boost: Boost::Held(held), landing: "" };
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

#[cfg(test)]
mod panel_tests {
    use super::*;

    fn sc(coupling: f64, grav_comp: bool, nav: bool, limiter: f64) -> HudIn {
        HudIn {
            mode: Mode::ShipSc { decoupled: coupling < 0.5, grav_comp, nav },
            panel: Panel::Sc { coupled: coupling > 0.5, coupling, grav_comp, g_safe: true, comstab: true, proximity: true, wind_comp: true, nav, limiter, braking: false, cap: 150.0, felt_g: 2.3 },
            speed: 123.44,
            altitude: None,
            boost: Boost::None,
            landing: "",
        }
    }

    fn axis(assist: bool, coupling: f64, braking: bool) -> HudIn {
        HudIn {
            mode: Mode::Ship { assist, decoupled: coupling < 0.5 },
            panel: Panel::Axis { assist, coupled: coupling > 0.5, coupling, braking, cap: 90.0, felt_g: 1.0 },
            speed: 10.0,
            altitude: None,
            boost: Boost::None,
            landing: "LANDING",
        }
    }

    fn on(r: &HudReadout, key: &str) -> Option<bool> {
        r.badges.iter().find(|b| b.key == key).map(|b| b.on)
    }

    #[test]
    fn decoupled_at_half_blend_shows_the_pair_and_the_blend() {
        let r = readout(&sc(0.5, true, false, 1.0));
        assert_eq!((on(&r, "COUPLED"), on(&r, "DECOUPLED")), (Some(false), Some(true)));
        assert_eq!(r.blend, Some(0.5));
        assert_eq!(on(&r, "MODEL"), Some(true));
        // At rest the blend is gone.
        assert_eq!(readout(&sc(0.0, true, false, 1.0)).blend, None);
        assert_eq!(readout(&sc(1.0, true, false, 1.0)).blend, None);
    }

    #[test]
    fn the_sc_panel_lists_every_switch_in_order() {
        let r = readout(&sc(1.0, true, false, 1.0));
        let keys: Vec<_> = r.badges.iter().map(|b| b.key).collect();
        assert_eq!(keys, ["MODEL", "COUPLED", "DECOUPLED", "GRAV COMP", "G-SAFE", "COMSTAB", "PROX", "WIND", "SCM", "NAV", "LANDING", "BRAKE"]);
        assert!(keys.len() <= BADGE_SLOTS);
        assert_eq!(on(&r, "SCM"), Some(true));
        assert_eq!(on(&r, "NAV"), Some(false));
    }

    #[test]
    fn the_limiter_badge_shows_only_below_full_speed() {
        assert!(on(&readout(&sc(1.0, true, false, 1.0)), "LIMIT").is_none());
        let r = readout(&sc(1.0, true, false, 0.9));
        let limit = r.badges.iter().find(|b| b.key == "LIMIT").unwrap();
        assert_eq!((limit.label.as_str(), limit.on), ("LIMIT 90 %", true));
    }

    #[test]
    fn axis_panel_shows_assist_and_axis() {
        let r = readout(&axis(false, 1.0, true));
        assert_eq!(r.badges.iter().map(|b| b.key).collect::<Vec<_>>(), ["MODEL", "ASSIST", "COUPLED", "DECOUPLED", "LANDING", "BRAKE"]);
        assert_eq!(r.badges[0].label, "AXIS");
        assert_eq!((on(&r, "ASSIST"), on(&r, "BRAKE"), on(&r, "LANDING")), (Some(false), Some(true), Some(true)));
    }

    #[test]
    fn cap_and_felt_g_texts() {
        let r = readout(&sc(1.0, true, false, 1.0));
        assert_eq!((r.cap_text.as_str(), r.g_text.as_str()), ("123 / 150 m/s", "2.3 g"));
        let r = readout(&HudIn { mode: Mode::Walk, panel: Panel::None, speed: 1.0, altitude: None, boost: Boost::None, landing: "" });
        assert_eq!((r.cap_text.as_str(), r.g_text.as_str(), r.badges.len()), ("", "", 0));
    }

    #[test]
    fn grav_comp_off_toasts_once_and_ends_after_the_toast_time() {
        let mut t = Toaster::default();
        let dt = 1.0 / 60.0;
        let on_step = readout(&sc(1.0, true, false, 1.0));
        assert_eq!(t.step(&on_step.badges, "SC", dt, TOAST_TIME), None, "the first step only records");
        let off = readout(&sc(1.0, false, false, 1.0));
        assert_eq!(t.step(&off.badges, "SC", dt, TOAST_TIME).as_deref(), Some("GRAV COMP OFF"));
        let mut shown = 0.0;
        while shown < TOAST_TIME - 0.1 {
            assert_eq!(t.step(&off.badges, "SC", dt, TOAST_TIME).as_deref(), Some("GRAV COMP OFF"));
            shown += dt;
        }
        for _ in 0..(0.2 / dt) as usize + 1 {
            t.step(&off.badges, "SC", dt, TOAST_TIME);
        }
        assert_eq!(t.step(&off.badges, "SC", dt, TOAST_TIME), None);
    }

    #[test]
    fn a_new_change_replaces_the_toast() {
        let mut t = Toaster::default();
        let dt = 1.0 / 60.0;
        t.step(&readout(&sc(1.0, true, false, 1.0)).badges, "SC", dt, TOAST_TIME);
        // Decouple: the DECOUPLED half toasts, the COUPLED half going off does not.
        let r = readout(&sc(0.4, true, false, 1.0));
        assert_eq!(t.step(&r.badges, "SC", dt, TOAST_TIME).as_deref(), Some("DECOUPLED"));
        let r = readout(&sc(0.4, true, true, 1.0));
        assert_eq!(t.step(&r.badges, "SC", dt, TOAST_TIME).as_deref(), Some("NAV"));
    }

    #[test]
    fn a_model_switch_toasts_the_model_alone() {
        let mut t = Toaster::default();
        let dt = 1.0 / 60.0;
        t.step(&readout(&sc(1.0, true, false, 1.0)).badges, "SC", dt, TOAST_TIME);
        let a = readout(&axis(true, 1.0, false));
        assert_eq!(t.step(&a.badges, "AXIS", dt, TOAST_TIME).as_deref(), Some("MODEL AXIS"));
    }

    #[test]
    fn a_limiter_that_returns_to_full_speed_toasts_off() {
        let mut t = Toaster::default();
        let dt = 1.0 / 60.0;
        t.step(&readout(&sc(1.0, true, false, 0.9)).badges, "SC", dt, TOAST_TIME);
        assert_eq!(t.step(&readout(&sc(1.0, true, false, 0.8)).badges, "SC", dt, TOAST_TIME).as_deref(), Some("LIMIT 80 %"));
        assert_eq!(t.step(&readout(&sc(1.0, true, false, 1.0)).badges, "SC", dt, TOAST_TIME).as_deref(), Some("LIMIT OFF"));
    }

    #[test]
    fn blend_only_between_the_ends() {
        assert_eq!(blend_of(0.3), Some(0.3));
        assert_eq!(blend_of(1.0), None);
        assert_eq!(blend_of(0.0), None);
    }
}
