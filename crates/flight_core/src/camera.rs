//! Speed-driven camera effects (#27): field of view and streaks from speed through curves, a
//! look-ahead that turns the pilot's view a little into the turn, and a short bump on touchdown.
//! Plain math for the view; values in `content/tuning/camera.json`.
use crate::{parse_tuning, Curve};
use glam::DVec2;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CameraTuning {
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
    /// Speed lost towards the ground within one step (m/s) that counts as a touchdown.
    pub bump_threshold: f64,
}

impl CameraTuning {
    pub fn from_json(s: &str) -> Result<CameraTuning, String> {
        let t: CameraTuning = parse_tuning("camera.json", s)?;
        t.fov_curve.validate().map_err(|e| format!("camera.json: fov_curve: {e}"))?;
        t.streak_curve.validate().map_err(|e| format!("camera.json: streak_curve: {e}"))?;
        Ok(t)
    }
}

impl Default for CameraTuning {
    fn default() -> Self {
        use crate::Interp;
        let c = |interp, pts: &[(f64, f64)]| Curve { interp, points: pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect() };
        CameraTuning {
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
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraFx {
    pub fov_deg: f64,
    pub streak: f64,
    /// View offset (radians): x pitch (up positive), y yaw (left positive), like the ship's turn.
    pub look: DVec2,
    bump_amp: f64,
    bump_age: f64,
    /// Touchdowns so far (scenario checks).
    pub bumps: u32,
    approach: f64,
}

impl CameraFx {
    /// One step. `turn` is the ship's turn rate in its own axes (x pitch, y yaw, rad/s),
    /// `approach` its speed towards the ground (m/s), `near_ground` whether it is within touching.
    pub fn step(&mut self, t: &CameraTuning, speed: f64, turn: DVec2, approach: f64, near_ground: bool, dt: f64) {
        self.fov_deg = t.fov_curve.eval(speed);
        self.streak = t.streak_curve.eval(speed).clamp(0.0, 1.0);
        let lead = |rate: f64, max_deg: f64| {
            let r = rate.abs() - t.look_ahead_deadzone;
            if r <= 0.0 { 0.0 } else { (r * t.look_ahead_gain).min(max_deg.to_radians()) * rate.signum() }
        };
        let target = DVec2::new(lead(turn.x, t.look_ahead_max_pitch_deg), lead(turn.y, t.look_ahead_max_yaw_deg));
        let k = if t.look_ahead_ease_time > 0.0 { 1.0 - (-dt / t.look_ahead_ease_time).exp() } else { 1.0 };
        self.look += (target - self.look) * k;
        if near_ground && self.approach - approach > t.bump_threshold {
            self.bump_amp = (self.approach * t.bump_per_speed).min(t.bump_max);
            self.bump_age = 0.0;
            self.bumps += 1;
        }
        self.approach = approach;
        self.bump_age += dt;
    }

    /// Camera drop (m, along the ship's down) now.
    pub fn bump(&self, t: &CameraTuning) -> f64 {
        self.bump_amp * (-t.bump_damping * self.bump_age).exp() * (std::f64::consts::TAU * t.bump_frequency * self.bump_age).sin().abs()
    }
}
