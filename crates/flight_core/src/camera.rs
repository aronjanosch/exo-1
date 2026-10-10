//! Camera effects (#27, #148, #149): field of view and streaks from speed through curves, a
//! look-ahead that turns the pilot's view a little into the turn, a short bump on touchdown; the
//! trauma shake (boost, thrust, touchdown), the spring lag of the chase camera and the field of
//! view from forward G. Plain math for the view; values in `content/tuning/camera.json`.
use crate::axis::G0;
use crate::{Curve, CHASE_CAMERA_OFFSET, CHASE_CAMERA_PITCH_DEG};
use glam::{DVec2, DVec3};
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CameraTuning {
    /// Chase camera (seated) in ship space: x right, y up, z back (m).
    pub chase_offset: [f64; 3],
    /// Chase camera pitch against the nose (degrees, negative looks down). The HUD's aim circle
    /// follows the nose wherever the camera looks.
    pub chase_pitch_deg: f64,
    /// Speed (m/s) to field of view (degrees, vertical).
    pub fov_curve: Curve,
    /// Speed (m/s) to streak level 0..1 (the warp tunnel's streaks, without its colour).
    pub streak_curve: Curve,
    /// Look-ahead angle per turn rate (s): the view leads a turn of 1 rad/s by this many radians.
    pub look_ahead_gain: f64,
    /// Turn rates below this (rad/s) give no look-ahead.
    pub look_ahead_deadzone: f64,
    pub look_ahead_max_yaw_deg: f64,
    pub look_ahead_max_pitch_deg: f64,
    /// Time constant (s) of easing in and out.
    pub look_ahead_ease_time: f64,
    /// Touchdown: camera drop in metres per m/s of approach speed before contact, at most
    /// `bump_max`.
    pub bump_per_speed: f64,
    pub bump_max: f64,
    pub bump_frequency: f64,
    /// Decay rate of the bump (1/s).
    pub bump_damping: f64,
    /// Approach speed (m/s) below which a touchdown gives no bump.
    pub bump_threshold: f64,
    /// Trauma (0..1) added per s while boosting.
    pub shake_trauma_boost: f64,
    /// Trauma per s at full thrust (the thrust share scales it).
    pub shake_trauma_thrust: f64,
    /// Trauma per m/s of approach speed at a touchdown (only above `bump_threshold`).
    pub shake_trauma_touchdown: f64,
    /// Trauma lost per s at rest (linear decay).
    pub shake_decay: f64,
    /// Rate (Hz) at which the shake noise changes.
    pub shake_frequency: f64,
    /// Largest shake offset (m) at full trauma.
    pub shake_max_offset: f64,
    /// Largest shake rotation (degrees) at full trauma.
    pub shake_max_angle_deg: f64,
    /// Share of the shake the seated chase camera gets, and the walker in a flying cabin.
    pub shake_scale_chase: f64,
    pub shake_scale_cabin: f64,
    /// Chase camera lag per g of felt acceleration (m per g, opposite to it).
    pub lag_per_g: f64,
    /// Spring of the chase camera: stiffness (1/s²) and damping (1/s). Damping ratio 0.7 at the
    /// default, so it swings back a little and settles.
    pub lag_stiffness: f64,
    pub lag_damping: f64,
    /// Largest lag (m) of the chase camera.
    pub lag_max: f64,
    /// Forward felt G (g) to extra field of view (degrees), on top of the speed curve.
    pub g_fov_curve: Curve,
}

impl CameraTuning {
    pub fn from_json(s: &str) -> Result<CameraTuning, String> {
        let t: CameraTuning = content_core::parse_strict("camera.json", s)?;
        t.validate().map_err(|e| format!("camera.json: {e}"))?;
        Ok(t)
    }

    /// Everything finite; gains, limits, times and the bump not negative (#106 point 6).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(v) = self.chase_offset.iter().find(|v| !v.is_finite()) {
            return Err(format!("chase_offset {v} not finite"));
        }
        if !self.chase_pitch_deg.is_finite() {
            return Err(format!("chase_pitch_deg {} not finite", self.chase_pitch_deg));
        }
        for (what, v) in [
            ("look_ahead_gain", self.look_ahead_gain),
            ("look_ahead_deadzone", self.look_ahead_deadzone),
            ("look_ahead_max_yaw_deg", self.look_ahead_max_yaw_deg),
            ("look_ahead_max_pitch_deg", self.look_ahead_max_pitch_deg),
            ("look_ahead_ease_time", self.look_ahead_ease_time),
            ("bump_per_speed", self.bump_per_speed),
            ("bump_max", self.bump_max),
            ("bump_frequency", self.bump_frequency),
            ("bump_damping", self.bump_damping),
            ("bump_threshold", self.bump_threshold),
            ("shake_trauma_boost", self.shake_trauma_boost),
            ("shake_trauma_thrust", self.shake_trauma_thrust),
            ("shake_trauma_touchdown", self.shake_trauma_touchdown),
            ("shake_decay", self.shake_decay),
            ("shake_frequency", self.shake_frequency),
            ("shake_max_offset", self.shake_max_offset),
            ("shake_max_angle_deg", self.shake_max_angle_deg),
            ("shake_scale_chase", self.shake_scale_chase),
            ("shake_scale_cabin", self.shake_scale_cabin),
            ("lag_per_g", self.lag_per_g),
            ("lag_stiffness", self.lag_stiffness),
            ("lag_damping", self.lag_damping),
            ("lag_max", self.lag_max),
        ] {
            if !(v >= 0.0 && v.is_finite()) {
                return Err(format!("{what} {v} out of range"));
            }
        }
        self.fov_curve.validate().map_err(|e| format!("fov_curve: {e}"))?;
        self.streak_curve.validate().map_err(|e| format!("streak_curve: {e}"))?;
        self.g_fov_curve.validate().map_err(|e| format!("g_fov_curve: {e}"))
    }
}

impl Default for CameraTuning {
    fn default() -> Self {
        use crate::Interp;
        let c = |interp, pts: &[(f64, f64)]| Curve { interp, points: pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect() };
        CameraTuning {
            chase_offset: CHASE_CAMERA_OFFSET.to_array(),
            chase_pitch_deg: CHASE_CAMERA_PITCH_DEG,
            fov_curve: c(Interp::Smooth, &[(0.0, 75.0), (100.0, 77.0), (400.0, 84.0), (2000.0, 90.0)]),
            streak_curve: c(Interp::Smooth, &[(60.0, 0.0), (200.0, 0.2), (2000.0, 0.35)]),
            look_ahead_gain: 0.25,
            look_ahead_deadzone: 0.1,
            look_ahead_max_yaw_deg: 12.0,
            look_ahead_max_pitch_deg: 8.0,
            look_ahead_ease_time: 0.3,
            bump_per_speed: 0.08,
            bump_max: 0.5,
            bump_frequency: 3.0,
            bump_damping: 6.0,
            bump_threshold: 0.3,
            shake_trauma_boost: 1.0,
            shake_trauma_thrust: 0.8,
            shake_trauma_touchdown: 0.1,
            shake_decay: 0.5,
            shake_frequency: 6.0,
            shake_max_offset: 0.15,
            shake_max_angle_deg: 0.6,
            shake_scale_chase: 1.0,
            shake_scale_cabin: 0.4,
            lag_per_g: 0.5,
            lag_stiffness: 16.0,
            lag_damping: 5.6,
            lag_max: 1.5,
            g_fov_curve: c(Interp::Linear, &[(0.0, 0.0), (3.0, 6.0)]),
        }
    }
}

/// Felt acceleration (m/s², ship space) and the other inputs of one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxInput {
    /// Ship speed (m/s).
    pub speed: f64,
    /// Turn rate in the ship's own axes (x pitch, y yaw, rad/s).
    pub turn: DVec2,
    /// Speed towards the ground (m/s).
    pub approach: f64,
    /// The hull touches the ground.
    pub grounded: bool,
    /// Felt acceleration in ship space (forward -Z, up +Y): the change of velocity per s, without a
    /// gravity term, so a hover that holds the ship feels nothing. Zero on the ground.
    pub accel: DVec3,
    /// Thrust share 0..1 (the thrusters' ramp output).
    pub thrust: f64,
    /// Boost active.
    pub boost: bool,
    /// Turbulence (#147): trauma per s, 0..1. No source yet, so 0.
    pub turbulence: f64,
    /// The walker in a flying ship's cabin: shake only, no lag, no G field of view.
    pub cabin: bool,
    /// The player's `camera_shake` setting, 0..1.
    pub shake_scale: f64,
    /// Fixed-step length (s).
    pub dt: f64,
}

/// The camera state. `enabled` is the F9 switch: off, the shake, lag and G field of view are zero
/// and the other effects are exactly as before.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraFx {
    pub enabled: bool,
    pub fov_deg: f64,
    pub streak: f64,
    /// View offset (radians): x pitch (up positive), y yaw (left positive), like the ship's turn.
    pub look: DVec2,
    bump_amp: f64,
    bump_age: f64,
    /// Touchdowns so far (scenario checks).
    pub bumps: u32,
    /// Recent approach speed, falling off over `APPROACH_MEMORY`: the contact comes a step after
    /// the solver has cut the approach.
    approach: f64,
    grounded: bool,
    /// Trauma 0..1: the shake's strength (its square scales the offset).
    pub trauma: f64,
    /// Shake offset (m, camera space) and rotation (rad: x pitch, y yaw), already scaled.
    pub shake_offset: DVec3,
    pub shake_angle: DVec2,
    shake_time: f64,
    /// Felt acceleration (m/s², ship space), smoothed.
    felt: DVec3,
    /// Chase camera lag (m, ship space) and its speed.
    pub lag: DVec3,
    lag_vel: DVec3,
    /// Extra field of view from forward G (degrees), already part of `fov_deg`.
    pub g_fov: f64,
}

impl Default for CameraFx {
    fn default() -> Self {
        CameraFx {
            enabled: true,
            fov_deg: 0.0,
            streak: 0.0,
            look: DVec2::ZERO,
            bump_amp: 0.0,
            bump_age: 0.0,
            bumps: 0,
            approach: 0.0,
            grounded: false,
            trauma: 0.0,
            shake_offset: DVec3::ZERO,
            shake_angle: DVec2::ZERO,
            shake_time: 0.0,
            felt: DVec3::ZERO,
            lag: DVec3::ZERO,
            lag_vel: DVec3::ZERO,
            g_fov: 0.0,
        }
    }
}

/// s: time constant of the remembered approach speed.
const APPROACH_MEMORY: f64 = 0.1;
/// s: time constant of the smoothed felt acceleration (one solver step does not spike it).
const ACCEL_SMOOTH: f64 = 0.05;
/// s: longest spring substep, so the spring is stable at any frame rate.
const SPRING_STEP: f64 = 1.0 / 240.0;
/// Seed of the shake noise: the same inputs give the same offsets.
const SHAKE_SEED: u64 = 0x5EED_CAFE;

/// A hash of a lattice point to -1..1.
fn lattice(i: i64, channel: u64) -> f64 {
    let mut z = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ channel.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ SHAKE_SEED;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
}

/// Smooth value noise in -1..1 along `u` (lattice points one unit apart), per channel.
fn noise(u: f64, channel: u64) -> f64 {
    let i = u.floor();
    let f = u - i;
    let s = f * f * (3.0 - 2.0 * f);
    let (a, b) = (lattice(i as i64, channel), lattice(i as i64 + 1, channel));
    a + (b - a) * s
}

impl CameraFx {
    /// One step with the view inputs only (no ship inputs: no spring, no G field of view, the
    /// player's shake setting at full). `turn` is the ship's turn rate in its own axes (x pitch,
    /// y yaw, rad/s), `approach` its speed towards the ground (m/s), `grounded` whether the hull
    /// touches; the bump comes when contact starts (#110 point 4).
    pub fn step(&mut self, t: &CameraTuning, speed: f64, turn: DVec2, approach: f64, grounded: bool, dt: f64) {
        self.step_with(t, &FxInput { speed, turn, approach, grounded, shake_scale: 1.0, dt, ..FxInput::default() });
    }

    pub fn step_with(&mut self, t: &CameraTuning, i: &FxInput) {
        let dt = i.dt;
        let lead = |rate: f64, max_deg: f64| {
            let r = rate.abs() - t.look_ahead_deadzone;
            if r <= 0.0 { 0.0 } else { (r * t.look_ahead_gain).min(max_deg.to_radians()) * rate.signum() }
        };
        let target = DVec2::new(lead(i.turn.x, t.look_ahead_max_pitch_deg), lead(i.turn.y, t.look_ahead_max_yaw_deg));
        let k = if t.look_ahead_ease_time > 0.0 { 1.0 - (-dt / t.look_ahead_ease_time).exp() } else { 1.0 };
        self.look += (target - self.look) * k;
        self.streak = t.streak_curve.eval(i.speed).clamp(0.0, 1.0);
        self.approach = i.approach.max(self.approach * (-dt / APPROACH_MEMORY).exp());
        let touchdown = i.grounded && !self.grounded && self.approach > t.bump_threshold;
        if touchdown {
            self.bump_amp = (self.approach * t.bump_per_speed).min(t.bump_max);
            self.bump_age = 0.0;
            self.bumps += 1;
        }
        self.grounded = i.grounded;
        self.bump_age += dt;
        let kf = 1.0 - (-dt / ACCEL_SMOOTH).exp();
        self.felt += (i.accel - self.felt) * kf;
        let forward_g = (-self.felt.z / G0).max(0.0);
        if !self.enabled {
            self.trauma = 0.0;
            self.shake_offset = DVec3::ZERO;
            self.shake_angle = DVec2::ZERO;
            self.lag = DVec3::ZERO;
            self.lag_vel = DVec3::ZERO;
            self.g_fov = 0.0;
        } else {
            self.step_effects(t, i, touchdown, forward_g);
        }
        self.fov_deg = t.fov_curve.eval(i.speed) + self.g_fov;
    }

    /// The switch on: trauma, shake offset, lag and G field of view.
    fn step_effects(&mut self, t: &CameraTuning, i: &FxInput, touchdown: bool, forward_g: f64) {
        let dt = i.dt;
        // Trauma: sources add, the level decays at a constant rate (Eiserloh, GDC 2016).
        let rate = if i.boost { t.shake_trauma_boost } else { 0.0 } + i.thrust.clamp(0.0, 1.0) * t.shake_trauma_thrust + i.turbulence.clamp(0.0, 1.0);
        self.trauma = (self.trauma + rate * dt).clamp(0.0, 1.0);
        if touchdown {
            self.trauma = (self.trauma + self.approach * t.shake_trauma_touchdown).min(1.0);
        }
        self.trauma = (self.trauma - t.shake_decay * dt).max(0.0);
        // Spring lag: the camera is pushed back against the felt acceleration and swings back.
        let target = if i.cabin { DVec3::ZERO } else { -(self.felt / G0) * t.lag_per_g };
        let n = (dt / SPRING_STEP).ceil().max(1.0);
        let h = dt / n;
        for _ in 0..n as usize {
            let acc = (target - self.lag) * t.lag_stiffness - self.lag_vel * t.lag_damping;
            self.lag_vel += acc * h;
            self.lag += self.lag_vel * h;
        }
        if self.lag.length() > t.lag_max {
            self.lag *= t.lag_max / self.lag.length();
            self.lag_vel = DVec3::ZERO;
        }
        self.g_fov = if i.cabin { 0.0 } else { t.g_fov_curve.eval(forward_g) };
        // Offset: noise times trauma squared, bounded.
        self.shake_time += dt;
        let u = self.shake_time * t.shake_frequency;
        let scale = if i.cabin { t.shake_scale_cabin } else { t.shake_scale_chase } * i.shake_scale.clamp(0.0, 1.0);
        let amp = self.trauma * self.trauma * scale;
        let v = DVec3::new(noise(u, 0), noise(u, 1), noise(u, 2));
        let v = if v.length() > 1.0 { v / v.length() } else { v };
        self.shake_offset = v * t.shake_max_offset * amp;
        self.shake_angle = DVec2::new(noise(u, 3), noise(u, 4)) * t.shake_max_angle_deg.to_radians() * amp;
    }

    /// Camera drop (m, along the ship's down) now.
    pub fn bump(&self, t: &CameraTuning) -> f64 {
        self.bump_amp * (-t.bump_damping * self.bump_age).exp() * (std::f64::consts::TAU * t.bump_frequency * self.bump_age).sin().abs()
    }
}
