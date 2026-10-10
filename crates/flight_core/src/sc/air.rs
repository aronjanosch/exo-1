//! Stage 3 of the SC step: what the atmosphere does to the ship. Lane `sc-air` (round 5, #199)
//! owns this file.
//!
//! Airspeed is the ship's velocity minus the wind. Drag acts per ship axis (sideways brakes more
//! than nose first), lift along the ship's up from the forward airspeed and the angle of attack
//! through a curve that stalls, weathervaning turns the nose into the airspeed, the wind blows
//! over a base wind and gusts, and turbulence shakes the ship near the ground. Everything goes to
//! `accel` (held by gravity compensation) or to `push` (never held), see `AirOut`. All values are
//! TODO(initiator) placeholders in `content/tuning/sc_air.json`.
use super::modes::Modes;
use super::StepState;
use crate::{lerp, smoothstep};
use glam::{DQuat, DVec3};
use serde::Deserialize;

/// Per-axis numbers of the ship: x sideways, y up/down, z along the nose.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Axes {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Axes {
    fn validate(&self, what: &str) -> Result<(), String> {
        for (n, v) in [("x", self.x), ("y", self.y), ("z", self.z)] {
            if !(v >= 0.0 && v.is_finite()) {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}

/// `sc_air.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AirTuning {
    /// Share of the thrust at full density (1 in vacuum).
    pub thrust_share: f64,
    /// Share of the speed caps at full density.
    pub cap_share: f64,
    /// Drag per ship axis: a = k * density * v² on that axis, against the airspeed.
    pub drag: Axes,
    /// Lift: `lift_k` * density * forward airspeed² * C(angle of attack).
    pub lift_k: f64,
    /// C(angle of attack) for angles from 0 up, (deg, coefficient) pairs with rising degrees; the
    /// peak is the stall angle, past it the curve falls. Negative angles mirror it.
    pub lift_curve: Vec<[f64; 2]>,
    /// Weathervaning: x pitch and y yaw gain (1/s² per rad of airspeed angle at
    /// `weathervane_ref_speed`), z roll damping in air (1/s).
    pub weathervane: Axes,
    /// m/s: the airspeed at which the weathervane gains are as given.
    pub weathervane_ref_speed: f64,
    /// 1/s: pitch and yaw rate damping in air.
    pub weathervane_damp: f64,
    /// Direction of the base wind about the planet's up (deg, from the planet's reference direction).
    pub wind_dir_deg: f64,
    /// m/s at the surface, falls to zero with the density.
    pub wind_speed: f64,
    /// m/s: the largest gust per horizontal direction.
    pub gust_speed: f64,
    /// s: the time scale of the gusts.
    pub gust_period: f64,
    /// m: full turbulence below this clearance, none above twice it.
    pub turbulence_height: f64,
    /// m/s: no speed-based turbulence below the minimum, full above the maximum (ground speed).
    pub turbulence_speed_min: f64,
    pub turbulence_speed_max: f64,
    /// rad/s² at full turbulence, fractal noise per ship axis.
    pub turbulence_angular: f64,
    /// m/s² at full turbulence, world (a little, next to the angular part).
    pub turbulence_linear: f64,
    /// Hz: the time scale of the turbulence noise.
    pub turbulence_rate: f64,
}

impl AirTuning {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.thrust_share > 0.0 && self.thrust_share <= 1.0) {
            return Err(format!("thrust_share {} out of range", self.thrust_share));
        }
        if !(self.cap_share > 0.0 && self.cap_share <= 1.0) {
            return Err(format!("cap_share {} out of range", self.cap_share));
        }
        self.drag.validate("drag")?;
        self.weathervane.validate("weathervane")?;
        let nonneg = [
            ("lift_k", self.lift_k),
            ("weathervane_damp", self.weathervane_damp),
            ("wind_speed", self.wind_speed),
            ("gust_speed", self.gust_speed),
            ("turbulence_angular", self.turbulence_angular),
            ("turbulence_linear", self.turbulence_linear),
            ("turbulence_speed_min", self.turbulence_speed_min),
        ];
        for (n, v) in nonneg {
            if !(v >= 0.0 && v.is_finite()) {
                return Err(format!("{n} {v} out of range"));
            }
        }
        let positive = [
            ("weathervane_ref_speed", self.weathervane_ref_speed),
            ("gust_period", self.gust_period),
            ("turbulence_height", self.turbulence_height),
            ("turbulence_rate", self.turbulence_rate),
            ("turbulence_speed_max", self.turbulence_speed_max),
        ];
        for (n, v) in positive {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{n} {v} out of range"));
            }
        }
        if !(self.turbulence_speed_max > self.turbulence_speed_min) {
            return Err(format!("turbulence speed band {}..{} is empty", self.turbulence_speed_min, self.turbulence_speed_max));
        }
        if !self.wind_dir_deg.is_finite() {
            return Err(format!("wind_dir_deg {} out of range", self.wind_dir_deg));
        }
        if self.lift_curve.len() < 2 {
            return Err("lift_curve needs at least two points".into());
        }
        for (i, p) in self.lift_curve.iter().enumerate() {
            if !(p[0].is_finite() && p[1].is_finite()) {
                return Err(format!("lift_curve point {i} not finite"));
            }
            if i > 0 && !(p[0] > self.lift_curve[i - 1][0]) {
                return Err(format!("lift_curve degrees must rise, point {i} is {}", p[0]));
            }
        }
        if self.lift_curve[0][0] < 0.0 {
            return Err("lift_curve starts below 0 deg".into());
        }
        Ok(())
    }

    /// C(angle of attack) for an angle in degrees: piecewise linear, the last value held past the
    /// end, odd in the angle.
    pub fn lift_coef(&self, deg: f64) -> f64 {
        let c = self.lift_curve_at(deg.abs());
        if deg < 0.0 { -c } else { c }
    }

    fn lift_curve_at(&self, a: f64) -> f64 {
        let p = &self.lift_curve;
        for w in p.windows(2) {
            if a <= w[1][0] {
                let t = (a - w[0][0]) / (w[1][0] - w[0][0]);
                return lerp(w[0][1], w[1][1], t.clamp(0.0, 1.0));
            }
        }
        p[p.len() - 1][1]
    }
}

impl Default for AirTuning {
    fn default() -> Self {
        AirTuning {
            thrust_share: 0.6,
            cap_share: 0.6,
            drag: Axes { x: 0.008, y: 0.002, z: 0.001 },
            lift_k: 0.002,
            lift_curve: vec![[0.0, 0.0], [5.0, 0.5], [10.0, 0.9], [15.0, 1.2], [25.0, 0.6], [45.0, 0.2]],
            weathervane: Axes { x: 0.4, y: 0.4, z: 0.3 },
            weathervane_ref_speed: 100.0,
            weathervane_damp: 0.3,
            wind_dir_deg: 90.0,
            wind_speed: 10.0,
            gust_speed: 4.0,
            gust_period: 6.0,
            turbulence_height: 40.0,
            turbulence_speed_min: 10.0,
            turbulence_speed_max: 60.0,
            turbulence_angular: 0.25,
            turbulence_linear: 0.5,
            turbulence_rate: 2.0,
        }
    }
}

/// The air's clock (drives the gusts and the turbulence noise).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AirState {
    /// s since the first step.
    pub time: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirOut {
    /// m/s², world: drag, lift and (with wind compensation on) the wind's drag on the ship; the
    /// flight computer holds against it (with gravity compensation on). Integrated otherwise.
    pub accel: DVec3,
    /// m/s², world: what the air does that the flight computer does not hold against (the wind's
    /// drag with wind compensation off, the turbulence's push). Integrated, never compensated.
    pub push: DVec3,
    /// rad/s², ship space: what the air turns (turbulence, weathervaning, damping).
    pub angular: DVec3,
    /// rad/s², ship space: what the air turns that the flight computer holds against (the
    /// weathervaning and the air's damping); felt only when the torque runs out.
    pub angular_hold: DVec3,
    /// Share of the thrust the air leaves.
    pub thrust_scale: f64,
    /// Share of the speed caps the air leaves.
    pub cap_scale: f64,
    /// m/s, world.
    pub wind: DVec3,
    /// 0..1.
    pub turbulence: f64,
}

/// Quadratic drag per axis: -k * density * v * |v| on each axis of `v` (any frame).
fn drag(v: DVec3, density: f64, k: &Axes) -> DVec3 {
    let q = |v: f64, k: f64| -k * density * v * v.abs();
    DVec3::new(q(v.x, k.x), q(v.y, k.y), q(v.z, k.z))
}

/// Horizontal unit vector `deg` about `up` from the reference direction (-Z, or X at the poles).
fn horizontal_dir(up: DVec3, deg: f64) -> DVec3 {
    let mut r = DVec3::NEG_Z - up * up.dot(DVec3::NEG_Z);
    if r.length_squared() < 1e-9 {
        r = DVec3::X - up * up.dot(DVec3::X);
    }
    DQuat::from_axis_angle(up, deg.to_radians()) * r.normalize()
}

/// Deterministic lattice hash to -1..1.
fn hash(i: i64, ch: u64) -> f64 {
    let mut x = (i as u64).wrapping_add(ch.wrapping_mul(0x632B_E59B_D9B4_E019)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    ((x >> 11) as f64) / ((1u64 << 53) as f64) * 2.0 - 1.0
}

/// Smooth value noise in -1..1 along `u` (lattice points one unit apart), channel `ch`.
fn value_noise(u: f64, ch: u64) -> f64 {
    let i = u.floor();
    let f = u - i;
    let s = f * f * (3.0 - 2.0 * f);
    let (a, b) = (hash(i as i64, ch), hash(i as i64 + 1, ch));
    a + (b - a) * s
}

/// Fractal noise (two octaves) in about -1..1.
fn fractal(u: f64, ch: u64) -> f64 {
    0.65 * value_noise(u, ch) + 0.35 * value_noise(2.03 * u + 17.0, ch)
}

/// The wind at the ship (world): base wind plus gusts, both falling with the density.
fn wind_at(s: &AirState, f: &StepState, t: &AirTuning) -> DVec3 {
    let up = if f.up.length_squared() > 0.5 { f.up } else { DVec3::Y };
    let dir = horizontal_dir(up, t.wind_dir_deg);
    let perp = up.cross(dir);
    let u = s.time / t.gust_period;
    let gust = dir * fractal(u, 0) + perp * fractal(u, 1);
    (dir * t.wind_speed + gust * t.gust_speed) * f.density
}

pub fn step(s: &mut AirState, f: &StepState, m: &Modes, t: &AirTuning) -> AirOut {
    let d = f.density;
    let wind = wind_at(s, f, t);

    // Airspeed: the ship's velocity through the air, in ship space.
    let av = f.inv * (f.v - wind);
    // Forward airspeed (m/s, the nose is -z).
    let fwd = -av.z;

    // Drag: the part from the ship's own motion is held with the lift; the part from the wind
    // follows wind compensation.
    let own_local = drag(f.lv, d, &t.drag);
    let air_local = drag(av, d, &t.drag);
    let own = f.rot * own_local;
    let wind_drag = f.rot * (air_local - own_local);

    // Lift along the ship's up. The angle of attack is the nose above the airflow.
    let lift = if fwd > 0.0 {
        let alpha = (-av.y).atan2(fwd).to_degrees();
        t.lift_k * d * fwd * fwd * t.lift_coef(alpha)
    } else {
        0.0
    };
    let lift_world = f.rot * DVec3::new(0.0, lift, 0.0);

    let (accel, pushed) = if m.wind_comp { (own + lift_world + wind_drag, DVec3::ZERO) } else { (own + lift_world, wind_drag) };

    // Weathervaning: the nose turns towards the airspeed, in air only (q and the damping are zero
    // in space). Elevation and azimuth of the airspeed from the nose.
    let q = d * av.length() / t.weathervane_ref_speed;
    let (elev, azim) = if fwd > 0.0 { (av.y.atan2(fwd), av.x.atan2(fwd)) } else { (0.0, 0.0) };
    let w = f.w_local;
    let weathervane = DVec3::new(
        t.weathervane.x * elev * q - t.weathervane_damp * d * w.x,
        -t.weathervane.y * azim * q - t.weathervane_damp * d * w.y,
        -t.weathervane.z * d * w.z,
    );

    // Turbulence: height band times ground-speed band, times the density.
    let height = smoothstep(2.0 * t.turbulence_height, t.turbulence_height, f.clearance);
    let speed = smoothstep(t.turbulence_speed_min, t.turbulence_speed_max, f.v.length());
    let turbulence = (d * height * speed).clamp(0.0, 1.0);
    let u = s.time * t.turbulence_rate;
    let shake = DVec3::new(fractal(u, 10), fractal(u, 11), fractal(u, 12));
    let kick = DVec3::new(fractal(u, 20), fractal(u, 21), fractal(u, 22));
    // The flight computer holds the weathervaning (`angular_hold`), not the turbulence (`angular`).
    let angular = shake * (t.turbulence_angular * turbulence);
    let push = pushed + kick * (t.turbulence_linear * turbulence);

    s.time += f.dt;
    AirOut {
        accel,
        push,
        angular,
        angular_hold: weathervane,
        thrust_scale: lerp(1.0, t.thrust_share, d),
        cap_scale: lerp(1.0, t.cap_share, d),
        wind,
        turbulence,
    }
}
