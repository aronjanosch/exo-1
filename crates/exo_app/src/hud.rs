//! TODO(initiator): which elements, their words (one decimal on the speed, "SHIP  DECOUPLED")
//! and the gauge look are starting points.
use crate::env::PlanetRes;
use crate::ship::Ship;
use crate::walker::Player;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<HudReadout>();
    app.add_systems(FixedUpdate, update_readout.in_set(crate::phases::Fx::Effects));
}
pub use flight_core::hud::{level, tilt_deg, Height, HudTuning, Level};

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Walk,
    Cabin,
    Suit,
    Fly,
    Ship { decoupled: bool, grav_comp: bool, nav: bool },
    Quantum(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Boost {
    None,
    /// The suit has no meter (decided "no fuel"): only whether boost is held.
    Held(bool),
    /// The ship's speed stage (no capacitor: `drain_time` 0): held or not.
    Stage(bool),
    Ship { charge: f64, active: bool, ready: bool },
}

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
}

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

pub const BADGE_SLOTS: usize = 12;
const BLEND_EPS: f64 = 1e-6;

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct HudReadout {
    pub texts: [String; 4],
    /// The ship's charge 0..1 for the bar; `None` outside the seat.
    pub gauge: Option<f64>,
    pub boosting: bool,
    /// Charge at or above the start charge (the bar is dimmed below).
    pub ready: bool,
    pub boost_mode: &'static str,
    pub landing: &'static str,
    pub badges: Vec<Badge>,
    pub blend: Option<f64>,
    pub cap_text: String,
    pub g_text: String,
    pub toast: Option<String>,
    pub flight: Option<FlightHud>,
}

/// The landing mode's HUD word. TODO(initiator): the word.
pub fn landing_word(landing: bool) -> &'static str {
    if landing { "LANDING" } else { "" }
}

/// Two decimals below 1 m/s, so a ship at rest can be told from a slow drift.
pub fn speed_text(v: f64) -> String {
    if v < 1.0 { format!("{v:.2}") } else { format!("{v:.1}") }
}

pub fn readout(i: &HudIn) -> HudReadout {
    let mode = match &i.mode {
        Mode::Walk => "WALK".into(),
        Mode::Cabin => "CABIN".into(),
        Mode::Suit => "SUIT".into(),
        Mode::Fly => "FLY".into(),
        Mode::Ship { decoupled, grav_comp, nav } => {
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
        Panel::Sc { coupling, cap, felt_g, .. } => {
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
        flight: None,
    }
}

pub fn blend_of(coupling: f64) -> Option<f64> {
    (coupling > BLEND_EPS && coupling < 1.0 - BLEND_EPS).then_some(coupling)
}

fn coupling_badges(b: &mut Vec<Badge>, coupled: bool) {
    b.push(Badge::half("COUPLED", coupled));
    b.push(Badge::half("DECOUPLED", !coupled));
}

/// (only below 100 %), landing and brake.
pub fn badges(i: &HudIn) -> Vec<Badge> {
    let mut b = Vec::new();
    let landing = Badge::new("LANDING", "LANDING", !i.landing.is_empty());
    match &i.panel {
        Panel::None => return b,
        Panel::Sc { coupled, grav_comp, g_safe, comstab, proximity, wind_comp, nav, limiter, braking, .. } => {
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
    }
    b
}

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

/// change replaces the toast.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toaster {
    prev: Option<Vec<Badge>>,
    left: f64,
    text: Option<String>,
}

impl Toaster {
    /// One fixed step of `dt` s; returns the toast to show.
    pub fn step(&mut self, badges: &[Badge], dt: f64, time: f64) -> Option<String> {
        let change = match &self.prev {
            None => None,
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
        self.text.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlightIn {
    /// m/s, ship space (x right, y up, z back): the ship's velocity.
    pub lv: DVec3,
    /// Signed thrust share per ship axis, -1..1 (x right, y up, z back).
    pub thrust: DVec3,
    /// m/s: the cruise cap at full stick, the first mark on the tape.
    pub cruise: f64,
    /// m/s: the boost cap, the top of the tape.
    pub boost: f64,
    /// Speed limiter share of the cruise cap, 1 = off.
    pub limiter: f64,
    /// g: the felt acceleration.
    pub felt_g: f64,
    /// g: the G-safe forward limit.
    pub g_limit: f64,
    /// Pitch and roll in degrees; `None` above the atmosphere (no horizon).
    pub horizon: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeedTape {
    pub fill: f64,
    pub number: String,
    pub cruise: f64,
    pub limiter: Option<f64>,
}

/// The G bar: fill and the G-safe mark as shares of the full scale; `over` above the mark.
#[derive(Clone, Debug, PartialEq)]
pub struct GBar {
    pub fill: f64,
    pub mark: f64,
    pub over: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlightHud {
    pub speed_tape: SpeedTape,
    /// Right, left, up, down, forward, back: each bar's length as a share of full thrust, 0..1.
    pub thrust: [f64; 6],
    pub g_bar: GBar,
    /// Pitch and roll in degrees (`None` in space).
    pub horizon: Option<(f64, f64)>,
    /// The tilt against the planet's up (deg) and how level that is (`None` in space).
    pub tilt: Option<(f64, Level)>,
    /// Unit direction of the velocity in ship space; `None` below `velocity_min`.
    pub velocity_dir: Option<DVec3>,
}

pub fn speed_tape(speed: f64, cruise: f64, limiter: f64, boost: f64) -> SpeedTape {
    let share = |v: f64| (v / boost).clamp(0.0, 1.0);
    SpeedTape {
        fill: share(speed),
        number: format!("{speed:.0}"),
        cruise: share(cruise),
        limiter: (limiter < 1.0).then(|| share(cruise * limiter)),
    }
}

/// The thrust cross's six bar lengths, 0..1: right, left, up, down, forward, back (z back is +).
pub fn thrust_bars(share: DVec3) -> [f64; 6] {
    let c = |v: f64| v.clamp(0.0, 1.0);
    [c(share.x), c(-share.x), c(share.y), c(-share.y), c(-share.z), c(share.z)]
}

/// The G bar: the felt g over the full scale, the G-safe limit as the mark.
pub fn g_bar(felt_g: f64, limit: f64, full: f64) -> GBar {
    GBar { fill: (felt_g / full).clamp(0.0, 1.0), mark: (limit / full).clamp(0.0, 1.0), over: felt_g > limit }
}

/// The flight path marker's direction in ship space; `None` below `min_speed` m/s.
pub fn velocity_dir(lv: DVec3, min_speed: f64) -> Option<DVec3> {
    (lv.length() >= min_speed && lv.length() > 0.0).then(|| lv.normalize())
}

/// up. The roll is measured in the plane across the forward axis, so it holds when pitched.
pub fn horizon_angles(rot: DQuat, planet_up: DVec3) -> (f64, f64) {
    let (fwd, right, up_ship) = (rot * DVec3::NEG_Z, rot * DVec3::X, rot * DVec3::Y);
    let pitch = fwd.dot(planet_up).clamp(-1.0, 1.0).asin().to_degrees();
    let p = planet_up - fwd * planet_up.dot(fwd);
    let roll = p.dot(right).atan2(p.dot(up_ship)).to_degrees();
    (pitch, roll)
}

pub fn flight_hud(i: &FlightIn, t: &HudTuning) -> FlightHud {
    FlightHud {
        speed_tape: speed_tape(i.lv.length(), i.cruise, i.limiter, i.boost),
        thrust: thrust_bars(i.thrust),
        g_bar: g_bar(i.felt_g, i.g_limit, t.g_full),
        horizon: i.horizon,
        tilt: i.horizon.map(|(p, r)| {
            let tilt = tilt_deg(p, r);
            (tilt, level(tilt, t))
        }),
        velocity_dir: velocity_dir(i.lv, t.velocity_min),
    }
}

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
    let landing = if pl.seated { landing_word(ship.sc.modes.landing) } else { "" };
    let (mode, v, pos, boost) = if pl.seated {
        let mode = if wd.drive.phase != warp_core::Phase::Idle {
            Mode::Quantum(format!("{:?}", wd.drive.phase))
        } else {
            let m = &ship.sc.modes;
            Mode::Ship { decoupled: !m.coupled, grav_comp: m.grav_comp, nav: m.master == flight_core::sc::Master::Nav }
        };
        let (cap, t) = (&ship.sc.drive.boost, &ship.sc.tuning.drive.boost_capacitor);
        let boost = if t.drain_time <= 0.0 { Boost::Stage(cap.active) } else { Boost::Ship { charge: cap.charge, active: cap.active, ready: cap.ready(t) } };
        (mode, sv.0, sp.0, boost)
    } else if pl.ship.is_some() {
        (Mode::Cabin, sv.0 + sr.0 * pl.w.vel, sp.0, Boost::None)
    } else if pl.fly || pl.noclip {
        (Mode::Fly, pl.w.vel, pl.w.pos, Boost::None)
    } else if pl.body.is_some() {
        (Mode::Suit, pl.w.vel, pl.w.pos, Boost::Held(actions.boost))
    } else {
        (Mode::Walk, pl.w.vel, pl.w.pos, Boost::None)
    };
    let r: DVec3 = pos - planet.centre;
    let altitude = (r.length() < crate::sky::impostor_from(planet.radius, planet.field.atmosphere_height)).then(|| Height::pick(planet.above_ground(pos), r.length() - planet.radius, tuning.hud.agl_below));
    let panel = if !pl.seated {
        Panel::None
    } else {
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
    };
    let mut new = readout(&HudIn { mode, panel: panel.clone(), speed: v.length(), altitude, boost, landing });
    if pl.seated {
        new.texts[0].push_str(&format!("  {}", ship.model.name));
    }
    new.toast = toaster.step(&new.badges, time.delta_secs_f64(), tuning.hud.toast_time);
    if pl.seated || pl.ship.is_some() {
        let lin = &ship.sc.tuning.linear;
        let caps = lin.caps(ship.sc.modes.master);
        let (cruise, boost, limiter, felt_g, g_limit, thrust) = (caps.cruise, caps.boost_forward, ship.sc.status.limiter, ship.sc.status.felt_g, lin.g_limit.forward, ship.thrust_signal());
        // The planet's up at the ship: the horizon shows only in the atmosphere (initiator: off in
        // space).
        let in_air = flight_core::PlanetEnv::density_at(planet.as_ref(), sp.0) > 0.0;
        let horizon = altitude.filter(|_| in_air).map(|_| horizon_angles(sr.0, (sp.0 - planet.centre).normalize_or_zero()));
        let lv = sr.0.inverse() * sv.0;
        new.flight = Some(flight_hud(&FlightIn { lv, thrust, cruise, boost, limiter, felt_g, g_limit, horizon }, &tuning.hud));
    }
    if *out != new {
        *out = new;
    }
}

#[cfg(test)]
mod flight_tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn horizon_pitch_and_roll_of_a_ship_pitched_10_and_rolled_30() {
        // Pitch about x (nose up), then roll about the ship's forward axis (right wing up).
        let q = DQuat::from_rotation_x(10f64.to_radians()) * DQuat::from_rotation_z(30f64.to_radians());
        let (pitch, roll) = horizon_angles(q, DVec3::Y);
        assert!(close(pitch, 10.0), "pitch {pitch}");
        assert!(close(roll, 30.0), "roll {roll}");
        let (p, r) = horizon_angles(DQuat::from_rotation_y(1.0), DVec3::Y);
        assert!(close(p, 0.0) && close(r, 0.0), "level: {p} {r}");
    }
}
