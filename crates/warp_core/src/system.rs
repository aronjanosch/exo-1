//! The system registry: planets with their quantum travel radii, and the drive settings.
//! One JSON file holds everything in metres, set explicitly (`DECISIONS.md`, "World values and
//! scale"): no global scale factor, no radius derived from another.
use glam::DVec3;
use serde::Deserialize;

/// A planet of the system: its index in `System::planets`. Ids from outside (network, files)
/// become one only through `System::id`, which checks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanetId(pub u8);

impl PlanetId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The id as sent in snapshots.
    pub fn wire(self) -> u32 {
        self.0 as u32
    }
}

impl std::fmt::Display for PlanetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PlanetDef {
    pub name: String,
    /// Surface recipe: `content/planet/<recipe>.json` (#64). Seed and radius stay here.
    #[serde(default)]
    pub recipe: String,
    pub seed: i32,
    pub centre: [f64; 3],
    pub radius: f64,
    /// Height of the atmosphere top above the radius (m).
    pub atmosphere_height: f64,
    /// A jump starts only above this many atmosphere heights over the radius (`DECISIONS.md`,
    /// "Quantum drive: where it may start": 1.5).
    pub jump_altitude_factor: f64,
    /// A path may not come closer than this to the centre (radius plus highest terrain).
    pub obstruction_radius: f64,
    /// Added to the obstruction radius when checking a path and a drop point.
    pub obstruction_margin: f64,
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

/// `name` must be finite and above `min` (or at least `min` when `or_equal`).
fn check(name: &str, v: f64, min: f64, or_equal: bool) -> Result<(), String> {
    let ok = v.is_finite() && (v > min || (or_equal && v == min));
    if ok { Ok(()) } else { Err(format!("{name} = {v}, must be {} {min}", if or_equal { ">=" } else { ">" })) }
}

impl PlanetDef {
    /// Radii nest: radius < obstruction radius, keep-out < arrival radius < frame radius.
    pub fn validate(&self) -> Result<(), String> {
        for (i, c) in self.centre.iter().enumerate() {
            check(&format!("centre[{i}]"), *c, f64::MIN, true)?;
        }
        check("radius", self.radius, 0.0, false)?;
        check("atmosphere_height", self.atmosphere_height, 0.0, true)?;
        check("jump_altitude_factor", self.jump_altitude_factor, 0.0, true)?;
        check("obstruction_radius", self.obstruction_radius, self.radius, false)?;
        check("obstruction_margin", self.obstruction_margin, 0.0, true)?;
        check("arrival_radius", self.arrival_radius, self.keep_out(), false)?;
        check("frame_radius", self.frame_radius, self.arrival_radius, false)
    }

    pub fn centre(&self) -> DVec3 {
        DVec3::from_array(self.centre)
    }

    /// Height above the radius below which no jump starts.
    pub fn min_jump_altitude(&self) -> f64 {
        self.atmosphere_height * self.jump_altitude_factor
    }

    /// Radius a path and a drop point must stay out of.
    pub fn keep_out(&self) -> f64 {
        self.obstruction_radius + self.obstruction_margin
    }
}

/// Something a path must not cross besides planets (another ship).
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub centre: DVec3,
    pub radius: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
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
    /// Speed at the exit point and at the end of an emergency drop.
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
    /// Hermite tangent length as a share of the chord.
    pub spline_tension: f64,
    /// Tunnel effect: starts at this speed, full at the next.
    pub vfx_start_speed: f64,
    pub vfx_full_speed: f64,
    /// Emergency exit: the pilot holds the key this long during the flight, then the ship drops
    /// from its speed to the exit speed in `emergency_drop_time` seconds along the path.
    pub emergency_hold_time: f64,
    pub emergency_drop_time: f64,
    /// A drop point too close to another ship moves on along the path in steps of this length.
    pub emergency_clear_step: f64,
}

/// The star: only a place. Its light is `daynight_core`'s business.
#[derive(Deserialize, Clone, Debug)]
pub struct StarDef {
    /// World position (m).
    pub position: [f64; 3],
}

impl StarDef {
    pub fn position(&self) -> DVec3 {
        DVec3::from_array(self.position)
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct System {
    #[serde(rename = "_comment", default)]
    _comment: String,
    /// The one star of the system (#104).
    pub star: StarDef,
    pub planets: Vec<PlanetDef>,
    pub drive: DriveConfig,
}

impl System {
    /// Reads and checks the system file (#106): unknown fields and values the drive or the
    /// registry cannot work with are errors, never panics or hangs later.
    pub fn from_json(s: &str) -> Result<System, String> {
        let sys: System = serde_json::from_str(s).map_err(|e| e.to_string())?;
        sys.validate()?;
        Ok(sys)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.star.position().is_finite() {
            return Err("star position must be finite".into());
        }
        if self.planets.is_empty() || self.planets.len() > u8::MAX as usize {
            return Err(format!("{} planets: 1 to {} allowed", self.planets.len(), u8::MAX));
        }
        for p in &self.planets {
            p.validate().map_err(|e| format!("planet {}: {e}", p.name))?;
        }
        self.drive.validate().map_err(|e| format!("drive: {e}"))
    }

    /// Checks an id from outside (a snapshot, a file).
    pub fn id(&self, raw: u32) -> Option<PlanetId> {
        ((raw as usize) < self.planets.len()).then_some(PlanetId(raw as u8))
    }

    pub fn ids(&self) -> impl Iterator<Item = PlanetId> + '_ {
        (0..self.planets.len()).map(|i| PlanetId(i as u8))
    }

    pub fn planet(&self, id: PlanetId) -> &PlanetDef {
        &self.planets[id.index()]
    }

    /// Moves planet `i` (i >= 1) along x so it is `d` metres from planet 0. Frame zones that would
    /// reach past half the distance shrink to 45 % of it (returned as notes); a distance that
    /// leaves no room for the arrival radii is refused.
    pub fn set_distance(&mut self, d: f64) -> Result<Vec<String>, String> {
        let need = self.planets.iter().map(|p| p.arrival_radius.max(p.obstruction_radius)).fold(0.0, f64::max);
        let half = d * 0.5;
        if half <= need {
            return Err(format!("distance {d:.0} m: half of it must be more than the largest arrival radius ({need:.0} m)"));
        }
        let c0 = self.planets[0].centre();
        for p in self.planets.iter_mut().skip(1) {
            p.centre = (c0 + DVec3::X * d).to_array();
        }
        let mut notes = Vec::new();
        for p in &mut self.planets {
            if p.frame_radius >= half {
                let new = (0.45 * d).max(need);
                notes.push(format!("{}: frame zone {:.0} m reaches past half the distance ({half:.0} m), now {new:.0} m", p.name, p.frame_radius));
                p.frame_radius = new;
            }
        }
        Ok(notes)
    }

    /// The planet whose frame zone holds `world`, nearest centre first.
    pub fn frame_of(&self, world: DVec3) -> Option<PlanetId> {
        self.ids()
            .filter(|&i| self.planet(i).centre().distance(world) <= self.planet(i).frame_radius)
            .min_by(|&a, &b| self.planet(a).centre().distance(world).total_cmp(&self.planet(b).centre().distance(world)))
    }

    /// Nearest planet by distance to its centre.
    pub fn nearest(&self, world: DVec3) -> PlanetId {
        self.ids()
            .min_by(|&a, &b| self.planet(a).centre().distance(world).total_cmp(&self.planet(b).centre().distance(world)))
            .expect("a planet")
    }

    /// The planet a jump from `world` goes to when `selected` is picked: the selected one, or the
    /// next one when the ship is at the selected planet (in its frame zone). One rule for the
    /// drive and the HUD.
    pub fn effective_target(&self, selected: PlanetId, world: DVec3) -> PlanetId {
        if self.frame_of(world) == Some(selected) {
            PlanetId(((selected.index() + 1) % self.planets.len()) as u8)
        } else {
            selected
        }
    }

    /// Farthest a ship can be from the centre of the planet its snapshots refer to: the largest
    /// distance between two centres plus the largest frame zone (a ship keeps its old planet
    /// until it enters the next one's zone, and may drop out anywhere on the way).
    pub fn max_ship_offset(&self) -> f64 {
        let mut far: f64 = 0.0;
        for a in &self.planets {
            for b in &self.planets {
                far = far.max(a.centre().distance(b.centre()));
            }
        }
        far + self.planets.iter().map(|p| p.frame_radius).fold(0.0, f64::max)
    }

    /// Fastest a ship can move: twice the drive's top speed.
    pub fn max_ship_speed(&self) -> f64 {
        2.0 * self.drive.top_speed
    }
}

impl DriveConfig {
    /// Speeds, accelerations and the steps > 0 (a zero exit speed or clear step hangs the drive,
    /// #111), exit <= stage switch <= top speed, the warning band outside the calibration angle.
    pub fn validate(&self) -> Result<(), String> {
        for (k, v) in [
            ("top_speed", self.top_speed),
            ("accel_stage_one", self.accel_stage_one),
            ("accel_stage_two", self.accel_stage_two),
            ("decel_stage_one", self.decel_stage_one),
            ("decel_stage_two", self.decel_stage_two),
            ("engage_speed", self.engage_speed),
            ("exit_speed", self.exit_speed),
            ("calibration_time_min", self.calibration_time_min),
            ("calibration_time_max_distance", self.calibration_time_max_distance),
            ("calibration_angle", self.calibration_angle),
            ("spline_tension", self.spline_tension),
            ("emergency_drop_time", self.emergency_drop_time),
            ("emergency_clear_step", self.emergency_clear_step),
        ] {
            check(k, v, 0.0, false)?;
        }
        for (k, v) in [
            ("spool_time", self.spool_time),
            ("calibration_delay", self.calibration_delay),
            ("pre_ramp_time", self.pre_ramp_time),
            ("post_ramp_time", self.post_ramp_time),
            ("cooldown", self.cooldown),
            ("vfx_start_speed", self.vfx_start_speed),
            ("emergency_hold_time", self.emergency_hold_time),
        ] {
            check(k, v, 0.0, true)?;
        }
        check("stage_switch_speed", self.stage_switch_speed, self.exit_speed, true)?;
        check("top_speed", self.top_speed, self.stage_switch_speed, true)?;
        check("top_speed", self.top_speed, self.engage_speed, true)?;
        check("calibration_time_max", self.calibration_time_max, self.calibration_time_min, true)?;
        check("warning_angle", self.warning_angle, self.calibration_angle, true)?;
        if self.warning_angle > 180.0 {
            return Err(format!("warning_angle = {}, must be <= 180", self.warning_angle));
        }
        check("vfx_full_speed", self.vfx_full_speed, self.vfx_start_speed, false)
    }

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
