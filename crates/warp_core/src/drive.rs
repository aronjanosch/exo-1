//! The quantum drive as a state machine. The caller feeds the ship's pose every tick; while the
//! drive is on rails (`RampUp` to `RampDown`, and an emergency drop) it returns the pose to put
//! the ship at.
//!
//! Sequence: `Spooling` -> `Calibrating` (aim held, gauge fills) -> `PreRamp` -> `RampUp` (two
//! acceleration stages) -> `Cruise` -> `RampDown` -> `PostRampDown` -> `Cooldown` -> `Idle`.
//! Aborts are their own transitions back to `Idle`; each has a reason. During the flight the
//! pilot can hold the exit key: `EmergencyDrop` brakes to the exit speed within a few seconds and
//! leaves the ship in open space, then `PostRampDown` and `Cooldown` as after an arrival.
use crate::path::{blocked_at, Blocker, Path};
use crate::system::{DriveConfig, Obstacle, PlanetId, System};
use glam::DVec3;

/// Most steps an end point moves to get clear of ships (a few ships, steps of a kilometre).
const MAX_CLEAR_STEPS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Spooling,
    Calibrating,
    PreRamp,
    RampUp,
    Cruise,
    RampDown,
    EmergencyDrop,
    PostRampDown,
    Cooldown,
}

impl Phase {
    /// The ship is moved by the drive along the path (not by the pilot).
    pub fn on_rails(self) -> bool {
        matches!(self, Phase::RampUp | Phase::Cruise | Phase::RampDown | Phase::EmergencyDrop)
    }

    /// The drive holds the ship: the pilot's flight input does nothing (from the pre-ramp on).
    pub fn holds_ship(self) -> bool {
        self == Phase::PreRamp || self.on_rails()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// Start refused: no such planet, or the ship's own.
    NoTarget,
    /// Start refused: below the planet's jump altitude (1.5 atmosphere heights).
    TooLow,
    /// Start refused or lost: a planet or ship is on the path.
    Obstructed(Blocker),
    /// Start refused: drive still cooling down or busy.
    NotReady,
    /// The pilot cancelled while spooling or calibrating.
    Cancelled,
    /// The aim left the warning angle during calibration.
    CalibrationLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Phase(Phase),
    Aborted(Abort),
    /// Reached the exit point.
    Arrived,
    /// An emergency drop ended: the ship is in open space at the exit speed.
    DroppedOut,
}

/// What the drive needs to know about the ship each tick.
#[derive(Clone, Copy, Debug)]
pub struct ShipView {
    pub pos: DVec3,
    /// Unit vector the ship points along.
    pub forward: DVec3,
    pub speed: f64,
}

#[derive(Clone, Debug)]
pub struct Drive {
    pub cfg: DriveConfig,
    pub phase: Phase,
    pub target: Option<PlanetId>,
    /// Seconds in the current phase.
    pub timer: f64,
    /// Calibration gauge 0..1.
    pub gauge: f64,
    /// The aim is between the calibration and the warning angle (the gauge waits).
    pub warning: bool,
    /// Angle between the ship's nose and the course (degrees), while aligning.
    pub angle: f64,
    /// Seconds the exit key has been held during the flight.
    pub exit_hold: f64,
    path: Option<Path>,
    calibration_time: f64,
    s: f64,
    v: f64,
    /// Arc length where the rails end: the exit point, a drop point, or short of either when a
    /// ship is there. 0 until the ramp-up.
    end: f64,
    /// Emergency drop: its deceleration.
    drop: Option<f64>,
}

impl Drive {
    pub fn new(cfg: DriveConfig) -> Drive {
        Drive {
            cfg,
            phase: Phase::Idle,
            target: None,
            timer: 0.0,
            gauge: 0.0,
            warning: false,
            angle: 0.0,
            exit_hold: 0.0,
            path: None,
            calibration_time: 0.0,
            s: 0.0,
            v: 0.0,
            end: 0.0,
            drop: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }

    /// Speed along the path while on rails (m/s), else 0.
    pub fn speed(&self) -> f64 {
        if self.phase.on_rails() { self.v } else { 0.0 }
    }

    /// 1 below the stage switch speed, 2 above (while on rails).
    pub fn stage(&self) -> u8 {
        if self.v < self.cfg.stage_switch_speed { 1 } else { 2 }
    }

    /// Where the ship comes out at planet `target` when leaving from `from`: on its arrival
    /// sphere, on the line from its centre to `from`, so the ship arrives facing the centre.
    pub fn exit_point(sys: &System, target: PlanetId, from: DVec3) -> DVec3 {
        let p = sys.planet(target);
        let facing = (from - p.centre()).normalize_or(DVec3::X);
        p.centre() + facing * p.arrival_radius
    }

    fn make_path(&self, sys: &System, target: PlanetId, from: DVec3) -> Path {
        // Leaving along the tangent only matters at a planet; in open space the path starts straight.
        let from_up = sys.frame_of(from).map(|o| (from - sys.planet(o).centre()).normalize_or_zero());
        let exit = Self::exit_point(sys, target, from);
        // The last part comes in radially: towards the target's centre.
        Path::new(from, from_up, exit, sys.planet(target).centre() - exit, self.cfg.spline_tension)
    }

    /// Pick `target` and start spooling. Refused (and nothing changes) when not ready, too low,
    /// or the path is blocked.
    pub fn begin(&mut self, target: PlanetId, ship: &ShipView, sys: &System, obstacles: &[Obstacle]) -> Result<(), Abort> {
        if self.phase != Phase::Idle {
            return Err(Abort::NotReady);
        }
        if target.index() >= sys.planets.len() || sys.frame_of(ship.pos) == Some(target) {
            return Err(Abort::NoTarget);
        }
        let near = sys.planet(sys.nearest(ship.pos));
        if ship.pos.distance(near.centre()) - near.radius < near.min_jump_altitude() {
            return Err(Abort::TooLow);
        }
        let path = self.make_path(sys, target, ship.pos);
        if let Some(b) = path.blocked(&sys.planets, obstacles) {
            return Err(Abort::Obstructed(b));
        }
        self.calibration_time = self.cfg.calibration_time(path.length());
        self.path = Some(path);
        self.target = Some(target);
        self.timer = 0.0;
        self.gauge = 0.0;
        self.warning = false;
        self.exit_hold = 0.0;
        self.drop = None;
        self.end = 0.0;
        self.phase = Phase::Spooling;
        Ok(())
    }

    /// The pilot cancels. Only spooling and calibrating can be cancelled.
    pub fn cancel(&mut self) -> Option<Event> {
        match self.phase {
            Phase::Spooling | Phase::Calibrating => Some(self.abort(Abort::Cancelled)),
            _ => None,
        }
    }

    /// The exit key, every tick: held for `emergency_hold_time` during the flight it starts an
    /// emergency drop. The drop ends `emergency_drop_time` seconds of braking further along the
    /// path; if that point is closer to a ship than its radius it moves on by
    /// `emergency_clear_step` until it is clear (planets cannot be there: the path keeps out of them),
    /// at most to the exit point and `MAX_CLEAR_STEPS` times.
    pub fn hold_exit(&mut self, pressed: bool, dt: f64, sys: &System, obstacles: &[Obstacle]) -> Option<Event> {
        if !pressed || !matches!(self.phase, Phase::RampUp | Phase::Cruise | Phase::RampDown) {
            self.exit_hold = 0.0;
            return None;
        }
        self.exit_hold += dt;
        if self.exit_hold < self.cfg.emergency_hold_time {
            return None;
        }
        let path = self.path.as_ref()?;
        let (v0, ve) = (self.v, self.cfg.exit_speed);
        let len = path.length();
        let mut end = (self.s + 0.5 * (v0 + ve) * self.cfg.emergency_drop_time).min(len);
        let step = self.cfg.emergency_clear_step;
        for _ in 0..MAX_CLEAR_STEPS {
            if !(step > 0.0) || end >= len || blocked_at(path.at(end).0, &sys.planets, obstacles).is_none() {
                break;
            }
            end = (end + step).min(len);
        }
        let decel = (v0 * v0 - ve * ve).max(0.0) / (2.0 * (end - self.s).max(1e-9));
        self.end = end;
        self.drop = Some(decel);
        self.exit_hold = 0.0;
        self.phase = Phase::EmergencyDrop;
        self.timer = 0.0;
        Some(Event::Phase(Phase::EmergencyDrop))
    }

    /// Where an emergency drop ends (while dropping and after it).
    pub fn drop_point(&self) -> Option<DVec3> {
        self.drop?;
        Some(self.path.as_ref()?.at(self.end).0)
    }

    /// A ship at the end of the rails (one that got to the exit first, or drifted onto the drop
    /// point): the end moves back along the path by `emergency_clear_step` until clear, never
    /// behind the ship (#111). Only ships: the path keeps out of planets.
    fn clear_end(&mut self, obstacles: &[Obstacle]) {
        let Some(path) = self.path.as_ref() else { return };
        let step = self.cfg.emergency_clear_step;
        for _ in 0..MAX_CLEAR_STEPS {
            if !(step > 0.0) || self.end - step <= self.s || blocked_at(path.at(self.end).0, &[], obstacles).is_none() {
                break;
            }
            self.end -= step;
        }
    }

    fn abort(&mut self, why: Abort) -> Event {
        self.phase = Phase::Idle;
        self.target = None;
        self.path = None;
        self.gauge = 0.0;
        self.warning = false;
        self.timer = 0.0;
        Event::Aborted(why)
    }

    fn go(&mut self, p: Phase, ev: &mut Vec<Event>) {
        self.phase = p;
        self.timer = 0.0;
        ev.push(Event::Phase(p));
    }

    /// Speed that can still be braked to the exit speed within `rem` metres.
    fn brake_speed(&self, rem: f64) -> f64 {
        let c = &self.cfg;
        let (vs, ve) = (c.stage_switch_speed, c.exit_speed);
        let at_switch = ((vs * vs - ve * ve) / (2.0 * c.decel_stage_one)).max(0.0);
        if rem >= at_switch {
            (vs * vs + 2.0 * c.decel_stage_two * (rem - at_switch)).sqrt()
        } else {
            (ve * ve + 2.0 * c.decel_stage_one * rem.max(0.0)).sqrt()
        }
    }

    /// One tick. Returns the events of this tick; `pose()` has the ship's place on rails.
    pub fn step(&mut self, dt: f64, ship: &ShipView, sys: &System, obstacles: &[Obstacle]) -> Vec<Event> {
        let mut ev = Vec::new();
        self.timer += dt;
        match self.phase {
            Phase::Idle => {}
            Phase::Spooling | Phase::Calibrating | Phase::PreRamp => {
                let target = self.target.expect("target while spooling");
                // The ship drifts and turns, so course, exit point and obstruction follow it
                // until the start.
                let path = self.make_path(sys, target, ship.pos);
                if let Some(b) = path.blocked(&sys.planets, obstacles) {
                    ev.push(self.abort(Abort::Obstructed(b)));
                    return ev;
                }
                self.angle = ship.forward.dot(path.start_dir()).clamp(-1.0, 1.0).acos().to_degrees();
                self.path = Some(path);
                self.warning = false;
                match self.phase {
                    Phase::Spooling => {
                        if self.timer >= self.cfg.spool_time {
                            self.go(Phase::Calibrating, &mut ev);
                        }
                    }
                    Phase::Calibrating => {
                        if self.timer > self.cfg.calibration_delay {
                            if self.angle > self.cfg.warning_angle {
                                ev.push(self.abort(Abort::CalibrationLost));
                                return ev;
                            }
                            if self.angle > self.cfg.calibration_angle {
                                self.warning = true;
                            } else {
                                self.gauge = (self.gauge + dt / self.calibration_time).min(1.0);
                            }
                            if self.gauge >= 1.0 {
                                self.go(Phase::PreRamp, &mut ev);
                            }
                        }
                    }
                    _ => {
                        if self.timer >= self.cfg.pre_ramp_time {
                            self.s = 0.0;
                            self.end = self.path.as_ref().expect("path").length();
                            self.v = ship.speed.clamp(self.cfg.engage_speed, self.cfg.top_speed);
                            self.go(Phase::RampUp, &mut ev);
                        }
                    }
                }
            }
            Phase::RampUp | Phase::Cruise | Phase::RampDown => {
                self.clear_end(obstacles);
                let len = self.end;
                let c = &self.cfg;
                let rem = len - self.s;
                let accel = if self.v < c.stage_switch_speed { c.accel_stage_one } else { c.accel_stage_two };
                let cand = (self.v + accel * dt).min(c.top_speed);
                let lim = self.brake_speed(rem - self.v * dt);
                let braking = self.phase == Phase::RampDown || lim < cand;
                let v_new = if braking { self.v.min(lim) } else { cand };
                self.s += 0.5 * (self.v + v_new) * dt;
                self.v = v_new;
                let next = if braking {
                    Phase::RampDown
                } else if self.v >= c.top_speed {
                    Phase::Cruise
                } else {
                    Phase::RampUp
                };
                // A speed braked to zero (bad values) still gets there instead of stopping short
                // for ever.
                if len - self.s <= self.v * dt || self.v <= 0.0 {
                    self.s = len;
                    ev.push(Event::Arrived);
                    self.go(Phase::PostRampDown, &mut ev);
                } else if next != self.phase {
                    self.go(next, &mut ev);
                }
            }
            Phase::EmergencyDrop => {
                self.clear_end(obstacles);
                let (end, decel) = (self.end, self.drop.expect("drop"));
                let ve = self.cfg.exit_speed;
                let lim = (ve * ve + 2.0 * decel * (end - self.s - self.v * dt).max(0.0)).sqrt();
                let v_new = (self.v - decel * dt).min(lim).max(ve);
                self.s += 0.5 * (self.v + v_new) * dt;
                self.v = v_new;
                if end - self.s <= self.v * dt || self.v <= 0.0 {
                    self.s = end;
                    ev.push(Event::DroppedOut);
                    self.go(Phase::PostRampDown, &mut ev);
                }
            }
            Phase::PostRampDown => {
                if self.timer >= self.cfg.post_ramp_time {
                    self.go(Phase::Cooldown, &mut ev);
                }
            }
            Phase::Cooldown => {
                if self.timer >= self.cfg.cooldown {
                    self.path = None;
                    self.target = None;
                    self.drop = None;
                    self.go(Phase::Idle, &mut ev);
                }
            }
        }
        ev
    }

    /// Ship position and velocity while on rails; the end point and exit velocity right after
    /// arriving or dropping out (for the hand-over to the pilot).
    pub fn pose(&self) -> Option<(DVec3, DVec3)> {
        let path = self.path.as_ref()?;
        match self.phase {
            p if p.on_rails() => {
                let (p, d) = path.at(self.s);
                Some((p, d * self.v))
            }
            Phase::PostRampDown if self.timer == 0.0 => {
                let (p, d) = path.at(self.s);
                Some((p, d * self.cfg.exit_speed))
            }
            _ => None,
        }
    }

    /// Where the path ends (the exit point); follows the path while it is rebuilt before the start.
    /// From the ramp-up on it is the hand-over point of an arrival, short of the exit point when
    /// a ship was there.
    pub fn exit(&self) -> Option<DVec3> {
        let path = self.path.as_ref()?;
        Some(if self.end > 0.0 && self.drop.is_none() { path.at(self.end).0 } else { path.end() })
    }

    /// Tunnel look 0..1 for the current speed (keyed to speed, not to the phase).
    pub fn tunnel(&self) -> f64 {
        self.cfg.tunnel_level(self.speed())
    }
}
