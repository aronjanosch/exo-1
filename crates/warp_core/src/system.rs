//! The system registry: planets with their quantum travel radii, and the drive settings.
//! One JSON file holds everything in metres, set explicitly (`DECISIONS.md`, "World values and
//! scale"): no global scale factor, no radius derived from another.
use glam::DVec3;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct PlanetDef {
    pub name: String,
    pub seed: i32,
    pub centre: [f64; 3],
    pub radius: f64,
    /// A path may not come closer than this to the centre (radius plus highest terrain).
    pub obstruction_radius: f64,
    /// Exit point distance from the centre (above the atmosphere).
    pub arrival_radius: f64,
    /// Zone where a ship belongs to this planet's frame (render and load), from the centre.
    pub frame_radius: f64,
    /// Colour of the distant sphere (linear-ish sRGB), look only.
    #[serde(default = "grey")]
    pub color: [f32; 3],
}

fn grey() -> [f32; 3] {
    [0.6, 0.6, 0.6]
}

impl PlanetDef {
    pub fn centre(&self) -> DVec3 {
        DVec3::from_array(self.centre)
    }
}

/// Something a path must not cross besides planets (another ship).
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub centre: DVec3,
    pub radius: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DriveConfig {
    pub top_speed: f64,
    /// Speed where stage one hands over to stage two.
    pub stage_switch_speed: f64,
    pub accel_stage_one: f64,
    pub accel_stage_two: f64,
    pub decel_stage_one: f64,
    pub decel_stage_two: f64,
    /// Speed the drive takes over at (lower bound of the ramp's start).
    pub engage_speed: f64,
    /// Speed at the exit point.
    pub exit_speed: f64,
    pub spool_time: f64,
    pub calibration_delay: f64,
    /// Calibration time grows from min to max with the trip length, reaching max at the distance below.
    pub calibration_time_min: f64,
    pub calibration_time_max: f64,
    pub calibration_time_max_distance: f64,
    /// Degrees: the gauge fills within this angle, pauses with a warning up to `warning_angle`,
    /// and the jump is lost beyond it.
    pub calibration_angle: f64,
    pub warning_angle: f64,
    pub pre_ramp_time: f64,
    pub post_ramp_time: f64,
    pub cooldown: f64,
    /// Metres above the ground (the planet's radius) below which no jump starts.
    pub min_altitude: f64,
    /// Hermite tangent length as a share of the chord.
    pub spline_tension: f64,
    /// Degrees the exit point is turned away from the line to the departure side, so the path
    /// arrives along the arrival sphere.
    pub exit_angle: f64,
    /// Tunnel effect: starts at this speed, full at the next.
    pub vfx_start_speed: f64,
    pub vfx_full_speed: f64,
    /// Added to every obstruction radius when checking a path.
    pub obstruction_margin: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct System {
    pub planets: Vec<PlanetDef>,
    pub drive: DriveConfig,
}

impl System {
    pub fn from_json(s: &str) -> Result<System, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Moves planet `i` (i >= 1) along x so it is `d` metres from planet 0.
    pub fn set_distance(&mut self, d: f64) {
        let c0 = self.planets[0].centre();
        for p in self.planets.iter_mut().skip(1) {
            p.centre = (c0 + DVec3::X * d).to_array();
        }
    }

    /// The planet whose frame zone holds `world`, nearest centre first.
    pub fn frame_of(&self, world: DVec3) -> Option<usize> {
        self.planets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.centre().distance(world) <= p.frame_radius)
            .min_by(|a, b| a.1.centre().distance(world).total_cmp(&b.1.centre().distance(world)))
            .map(|(i, _)| i)
    }

    /// Nearest planet by distance to its centre.
    pub fn nearest(&self, world: DVec3) -> usize {
        (0..self.planets.len())
            .min_by(|&a, &b| self.planets[a].centre().distance(world).total_cmp(&self.planets[b].centre().distance(world)))
            .unwrap()
    }
}

impl DriveConfig {
    /// Tunnel look 0..1 from speed, not time: the same effect for every distance and duration.
    pub fn tunnel_level(&self, speed: f64) -> f64 {
        let t = ((speed - self.vfx_start_speed) / (self.vfx_full_speed - self.vfx_start_speed).max(1e-9)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    pub fn calibration_time(&self, distance: f64) -> f64 {
        let t = (distance / self.calibration_time_max_distance).clamp(0.0, 1.0);
        self.calibration_time_min + (self.calibration_time_max - self.calibration_time_min) * t
    }
}
