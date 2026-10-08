//! The quantum drive as a state machine. The caller feeds the ship's pose every tick; while the
//! drive is on rails (`RampUp` to `RampDown`) it returns the pose to put the ship at.
//!
//! Sequence: `Spooling` -> `Calibrating` (aim held, gauge fills) -> `PreRamp` -> `RampUp` (two
//! acceleration stages) -> `Cruise` -> `RampDown` -> `PostRampDown` -> `Cooldown` -> `Idle`.
//! Aborts are their own transitions back to `Idle`; each has a reason.
use crate::path::{Blocker, Path};
use crate::system::{DriveConfig, Obstacle, System};
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Spooling,
    Calibrating,
    PreRamp,
    RampUp,
    Cruise,
    RampDown,
    PostRampDown,
    Cooldown,
}

impl Phase {
    /// The ship is moved by the drive (not by the pilot).
    pub fn on_rails(self) -> bool {
        matches!(self, Phase::RampUp | Phase::Cruise | Phase::RampDown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// Start refused: no such planet, or the ship's own.
    NoTarget,
    /// Start refused: below the minimum altitude (inside the atmosphere).
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
    pub target: Option<usize>,
    /// Seconds in the current phase.
    pub timer: f64,
    /// Calibration gauge 0..1.
    pub gauge: f64,
    /// The aim is between the calibration and the warning angle (the gauge waits).
    pub warning: bool,
    /// Angle between the ship's nose and the course (degrees), while aligning.
    pub angle: f64,
    path: Option<Path>,
    calibration_time: f64,
    s: f64,
    v: f64,
    exit_pos: DVec3,
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
            path: None,
            calibration_time: 0.0,
            s: 0.0,
            v: 0.0,
            exit_pos: DVec3::ZERO,
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
    /// sphere, on the side facing `from`, turned by the exit angle towards the side the ship
    /// leaves on (so the path arrives along the sphere, not straight at the planet).
    pub fn exit_point(sys: &System, target: usize, from: DVec3) -> DVec3 {
        let p = &sys.planets[target];
        let origin = &sys.planets[sys.nearest(from)];
        let facing = (from - p.centre()).normalize_or_zero();
        let up_start = (from - origin.centre()).normalize_or_zero();
        let mut side = up_start - facing * up_start.dot(facing);
        if side.length_squared() < 1e-12 {
            let a = if facing.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
            side = facing.cross(a);
        }
        let ang = sys.drive.exit_angle.to_radians();
        let dir = facing * ang.cos() + side.normalize() * ang.sin();
        p.centre() + dir * p.arrival_radius
    }

    fn make_path(&self, sys: &System, target: usize, from: DVec3) -> Path {
        let origin = &sys.planets[sys.nearest(from)];
        let to = &sys.planets[target];
        let exit = Self::exit_point(sys, target, from);
        Path::new(from, (from - origin.centre()).normalize_or_zero(), exit, (exit - to.centre()).normalize_or_zero(), self.cfg.spline_tension)
    }

    /// Pick `target` and start spooling. Refused (and nothing changes) when not ready, too low,
    /// or the path is blocked.
    pub fn begin(&mut self, target: usize, ship: &ShipView, sys: &System, obstacles: &[Obstacle]) -> Result<(), Abort> {
        if self.phase != Phase::Idle {
            return Err(Abort::NotReady);
        }
        if target >= sys.planets.len() {
            return Err(Abort::NoTarget);
        }
        let here = sys.nearest(ship.pos);
        if target == here {
            return Err(Abort::NoTarget);
        }
        let p = &sys.planets[here];
        if ship.pos.distance(p.centre()) - p.radius < self.cfg.min_altitude {
            return Err(Abort::TooLow);
        }
        let path = self.make_path(sys, target, ship.pos);
        if let Some(b) = path.blocked(&sys.planets, self.cfg.obstruction_margin, obstacles) {
            return Err(Abort::Obstructed(b));
        }
        self.calibration_time = self.cfg.calibration_time(path.length());
        self.exit_pos = path.end();
        self.path = Some(path);
        self.target = Some(target);
        self.timer = 0.0;
        self.gauge = 0.0;
        self.warning = false;
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
                // The ship drifts and turns, so course and obstruction follow it until the start.
                let path = self.make_path(sys, target, ship.pos);
                if let Some(b) = path.blocked(&sys.planets, self.cfg.obstruction_margin, obstacles) {
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
                            self.v = ship.speed.clamp(self.cfg.engage_speed, self.cfg.top_speed);
                            self.go(Phase::RampUp, &mut ev);
                        }
                    }
                }
            }
            Phase::RampUp | Phase::Cruise | Phase::RampDown => {
                let len = self.path.as_ref().expect("path on rails").length();
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
                if len - self.s <= self.v * dt {
                    self.s = len;
                    ev.push(Event::Arrived);
                    self.go(Phase::PostRampDown, &mut ev);
                } else if next != self.phase {
                    self.go(next, &mut ev);
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
                    self.go(Phase::Idle, &mut ev);
                }
            }
        }
        ev
    }

    /// Ship position and velocity while on rails; the exit point and exit velocity right after
    /// arriving (for the hand-over to the pilot).
    pub fn pose(&self) -> Option<(DVec3, DVec3)> {
        let path = self.path.as_ref()?;
        match self.phase {
            Phase::RampUp | Phase::Cruise | Phase::RampDown => {
                let (p, d) = path.at(self.s);
                Some((p, d * self.v))
            }
            Phase::PostRampDown if self.timer == 0.0 => {
                let (p, d) = path.at(path.length());
                Some((p, d * self.cfg.exit_speed))
            }
            _ => None,
        }
    }

    /// Where the path ends (the exit point).
    pub fn exit(&self) -> DVec3 {
        self.exit_pos
    }

    /// Tunnel look 0..1 for the current speed (keyed to speed, not to the phase).
    pub fn tunnel(&self) -> f64 {
        self.cfg.tunnel_level(self.speed())
    }
}
