//! Stages 2 and 6 of the SC step: the boost (`begin`) and what the thrusters really give
//! (`shape`). Lane `sc-body` (round 5, #198) owns this file, `tuning.rs` (`ShipBody`) and the
//! force glue in the app.
//!
//! `begin`: the boost is held for `boost_pre_delay`, ramps up over `boost_ramp_up`, runs on the
//! capacitor (with the idle cost while held) and ramps down over `boost_ramp_down` after release.
//! `shape`: four thruster groups (main forward, retro backward, vertical up and down, lateral
//! left and right) each wait `spool_delay` after a fresh request (a running boost skips the wait),
//! countering boost gets a share of the aligned boost, and every group's thrust and the angular
//! acceleration build up at most at their jerk (cutting is immediate).
use crate::limits::{Dirs, Rot};
use crate::{lerp, BoostCapacitor, BoostCapacitorTuning};
use glam::DVec3;
use serde::Deserialize;

/// One value per thruster group (main forward, retro backward, vertical up and down, lateral left
/// and right), in the unit of the field that holds it.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Groups {
    pub main: f64,
    pub retro: f64,
    pub vertical: f64,
    pub lateral: f64,
}

impl Groups {
    fn check(&self, what: &str, strict: bool) -> Result<(), String> {
        let lo = 0.0;
        for (n, v) in [("main", self.main), ("retro", self.retro), ("vertical", self.vertical), ("lateral", self.lateral)] {
            let ok = v.is_finite() && if strict { v > lo } else { v >= lo };
            if !ok {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}

/// `sc_drive.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DriveTuning {
    pub boost_capacitor: BoostCapacitorTuning,
    /// Multipliers on the thrust per direction at full boost.
    pub boost_thrust: Dirs,
    /// s: a fresh request in a group gives no thrust until it has lasted this long (a running
    /// boost skips it).
    pub spool_delay: Groups,
    /// m/s³ per group: how fast the group's thrust may change.
    pub jerk: Groups,
    /// rad/s³ per rotation axis: how fast the angular acceleration may change.
    pub angular_jerk: Rot,
    /// s from the press until the boost starts.
    pub boost_pre_delay: f64,
    /// s from the start of the boost to full strength.
    pub boost_ramp_up: f64,
    /// s from release to no boost.
    pub boost_ramp_down: f64,
    /// Countering boost (pushing against the velocity on that axis) gets this share of the
    /// aligned boost at full boost, 0..=1; unboosted countering is the plain box.
    pub boost_counter_share: f64,
    /// Charge 0..1 per s drained while the boost is held, on top of the capacitor's drain, with or
    /// without thrust.
    pub idle_cost: f64,
}

impl DriveTuning {
    pub fn validate(&self) -> Result<(), String> {
        self.boost_capacitor.validate()?;
        self.boost_thrust.validate("boost_thrust")?;
        self.spool_delay.check("spool_delay", false)?;
        self.jerk.check("jerk", true)?;
        self.angular_jerk.validate("angular_jerk")?;
        for (n, v) in [("boost_pre_delay", self.boost_pre_delay), ("boost_ramp_up", self.boost_ramp_up), ("boost_ramp_down", self.boost_ramp_down), ("idle_cost", self.idle_cost)] {
            if !(v >= 0.0 && v.is_finite()) {
                return Err(format!("{n} {v} out of range"));
            }
        }
        if !(0.0..=1.0).contains(&self.boost_counter_share) {
            return Err(format!("boost_counter_share {} out of range", self.boost_counter_share));
        }
        Ok(())
    }
}

impl Default for DriveTuning {
    fn default() -> Self {
        DriveTuning {
            boost_capacitor: BoostCapacitorTuning::default(),
            boost_thrust: Dirs { forward: 2.0, backward: 1.5, left: 1.25, right: 1.25, up: 1.25, down: 1.25 },
            spool_delay: Groups { main: 0.3, retro: 0.3, vertical: 0.2, lateral: 0.0 },
            jerk: Groups { main: 120.0, retro: 90.0, vertical: 400.0, lateral: 150.0 },
            angular_jerk: Rot { pitch: 600.0, yaw: 600.0, roll: 600.0 },
            boost_pre_delay: 0.15,
            boost_ramp_up: 0.5,
            boost_ramp_down: 0.3,
            boost_counter_share: 0.6,
            idle_cost: 0.02,
        }
    }
}

/// One thruster group's spool and jerk state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Group {
    /// Sign of the current request (0: no request).
    sign: f64,
    /// s the request has lasted (since it began or changed sign).
    waited: f64,
    /// The thrust this group gives now, signed along its axis (m/s²).
    value: f64,
}

const MAIN: usize = 0;
const RETRO: usize = 1;
const VERTICAL: usize = 2;
const LATERAL: usize = 3;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DriveState {
    pub boost: BoostCapacitor,
    /// s the boost key has been held (the pre-delay counts from here).
    press: f64,
    /// The boost's ramp, 0..1.
    level: f64,
    /// Strength of the last running boost (the ramp down fades it).
    strength: f64,
    groups: [Group; 4],
    /// The angular acceleration given now (rad/s², ship space).
    angular: DVec3,
}

/// The box `b` with the boost's multipliers at strength `boost` (0..1).
pub fn boosted(b: &Dirs, t: &DriveTuning, boost: f64) -> Dirs {
    let m = |x: f64| lerp(1.0, x, boost);
    let k = &t.boost_thrust;
    Dirs { forward: b.forward * m(k.forward), backward: b.backward * m(k.backward), left: b.left * m(k.left), right: b.right * m(k.right), up: b.up * m(k.up), down: b.down * m(k.down) }
}

/// The boost this step, 0..1: the capacitor's strength, held back by the pre-delay and faded by
/// the ramp. The brake stops a boost and does not drain the charge.
pub fn begin(s: &mut DriveState, boost: bool, braking: bool, t: &DriveTuning, dt: f64) -> f64 {
    s.press = if boost { s.press + dt } else { 0.0 };
    if s.boost.active {
        s.boost.charge = (s.boost.charge - t.idle_cost * dt).max(0.0);
    }
    let wants = boost && s.press >= t.boost_pre_delay;
    let strength = s.boost.step_braking(wants, braking, &t.boost_capacitor, dt);
    if strength > 0.0 {
        s.strength = strength;
        s.level = if t.boost_ramp_up > 0.0 { (s.level + dt / t.boost_ramp_up).min(1.0) } else { 1.0 };
    } else {
        s.level = if t.boost_ramp_down > 0.0 { (s.level - dt / t.boost_ramp_down).max(0.0) } else { 0.0 };
    }
    s.level * s.strength
}

/// What the flight computer asked of the thrusters this step.
#[derive(Clone, Copy, Debug)]
pub struct Asked {
    /// m/s², ship space.
    pub linear: DVec3,
    /// rad/s², ship space.
    pub angular: DVec3,
    pub boost: f64,
    pub thrust_box: Dirs,
    /// m/s, ship space: the ship's velocity, for the countering boost.
    pub velocity: DVec3,
    /// The pilot's request, -1..1 per ship axis (zero while braking): the spool counts from this,
    /// so the flight computer's own holds and corrections are not delayed.
    pub stick: DVec3,
}

/// What the thrusters give.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Given {
    pub linear: DVec3,
    pub angular: DVec3,
}

/// The box side along one axis (0 x, 1 y, 2 z) in the direction of `positive`.
fn side(b: &Dirs, axis: usize, positive: bool) -> f64 {
    match (axis, positive) {
        (0, true) => b.right,
        (0, false) => b.left,
        (1, true) => b.up,
        (1, false) => b.down,
        (_, true) => b.backward,
        (_, false) => b.forward,
    }
}

/// What the thrusters really give: the countering cap, the spool per group, then the jerk.
pub fn shape(s: &mut DriveState, a: &Asked, t: &DriveTuning, dt: f64) -> Given {
    // Countering: pushing against the velocity on an axis gets the share of the aligned boost,
    // faded in with the boost's strength (unboosted: the plain box).
    let b = a.boost.clamp(0.0, 1.0);
    let v = a.velocity.to_array();
    let mut want = a.linear.to_array();
    for i in 0..3 {
        if want[i] != 0.0 && v[i] * want[i] < 0.0 {
            let cap = side(&a.thrust_box, i, want[i] > 0.0) * lerp(1.0, t.boost_counter_share, b);
            want[i] = want[i].clamp(-cap, cap);
        }
    }
    // Spool per group: a pilot request that has not lasted `spool_delay` gives nothing yet. The
    // flight computer's own thrust in a group with no request (the hold against gravity) passes.
    let targets = [want[2].min(0.0), want[2].max(0.0), want[1], want[0]];
    let st = a.stick.to_array();
    let requests = [st[2].min(0.0), st[2].max(0.0), st[1], st[0]];
    let delays = [t.spool_delay.main, t.spool_delay.retro, t.spool_delay.vertical, t.spool_delay.lateral];
    let jerks = [t.jerk.main, t.jerk.retro, t.jerk.vertical, t.jerk.lateral];
    // Spool: what each group may give now (a request still spooling gives nothing).
    let mut goals = [0.0; 4];
    for g in 0..4 {
        let grp = &mut s.groups[g];
        let req = requests[g];
        let passing = if req == 0.0 {
            grp.sign = 0.0;
            grp.waited = 0.0;
            true
        } else {
            let sign = req.signum();
            if sign != grp.sign {
                grp.sign = sign;
                grp.waited = 0.0;
            } else {
                grp.waited += dt;
            }
            grp.waited >= delays[g] || a.boost > 0.0
        };
        goals[g] = if passing { targets[g] } else { 0.0 };
    }
    // Jerk: a group cuts its thrust at once (a thruster stops when told) and builds it up at most
    // at its jerk; the groups that build up share one factor, so the thrust keeps its direction
    // while it grows (#146, time to full thrust).
    let mut k: f64 = 1.0;
    for g in 0..4 {
        let (cur, goal) = (s.groups[g].value, goals[g]);
        let base = if cur * goal > 0.0 { cur } else { 0.0 };
        let grow = goal.abs() - base.abs();
        if grow > 1e-12 {
            k = k.min(jerks[g] * dt / grow);
        }
    }
    for g in 0..4 {
        let grp = &mut s.groups[g];
        let goal = goals[g];
        let base = if grp.value * goal > 0.0 { grp.value } else { 0.0 };
        grp.value = if goal.abs() <= base.abs() { goal } else { base + (goal - base) * k };
    }
    let g = &s.groups;
    let linear = DVec3::new(g[LATERAL].value, g[VERTICAL].value, g[MAIN].value + g[RETRO].value);
    // Angular: the same rule per axis, one factor for the axes that build up.
    let aj = [t.angular_jerk.pitch, t.angular_jerk.yaw, t.angular_jerk.roll];
    let asked = a.angular.to_array();
    let mut ang = s.angular.to_array();
    let mut k: f64 = 1.0;
    for i in 0..3 {
        let base = if ang[i] * asked[i] > 0.0 { ang[i] } else { 0.0 };
        let grow = asked[i].abs() - base.abs();
        if grow > 1e-12 {
            k = k.min(aj[i] * dt / grow);
        }
    }
    for i in 0..3 {
        let base = if ang[i] * asked[i] > 0.0 { ang[i] } else { 0.0 };
        ang[i] = if asked[i].abs() <= base.abs() { asked[i] } else { base + (asked[i] - base) * k };
    }
    s.angular = DVec3::from_array(ang);
    Given { linear, angular: s.angular }
}
