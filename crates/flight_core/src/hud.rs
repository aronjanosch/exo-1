//! HUD values that are data (`content/tuning/hud.json`, #91) and the altitude rule. Plain Rust.
use serde::Deserialize;

/// HUD values (`content/tuning/hud.json`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudTuning {
    /// m: below this height above the ground the altitude element shows AGL, above it ALT.
    pub agl_below: f64,
    /// s: a flight panel toast stays this long after the last switch change (#197).
    pub toast_time: f64,
    /// g: the G bar's full scale (#200).
    pub g_full: f64,
    /// m/s: below this speed the flight path marker is hidden (#200).
    pub velocity_min: f64,
    /// deg: at or below this tilt against the planet's up the horizon shows the ship as level.
    pub level_deg: f64,
    /// deg: at or below this tilt the horizon shows the ship as nearly level.
    pub near_level_deg: f64,
}

impl HudTuning {
    pub fn from_json(s: &str) -> Result<HudTuning, String> {
        let t: HudTuning = content_core::parse_strict("hud.json", s)?;
        if !(t.agl_below >= 0.0) {
            return Err(format!("hud.json: agl_below {} out of range", t.agl_below));
        }
        if !(t.toast_time > 0.0 && t.toast_time.is_finite()) {
            return Err(format!("hud.json: toast_time {} out of range", t.toast_time));
        }
        if !(t.g_full > 0.0 && t.g_full.is_finite()) {
            return Err(format!("hud.json: g_full {} out of range", t.g_full));
        }
        if !(t.velocity_min >= 0.0 && t.velocity_min.is_finite()) {
            return Err(format!("hud.json: velocity_min {} out of range", t.velocity_min));
        }
        if !(t.level_deg > 0.0 && t.near_level_deg > t.level_deg && t.near_level_deg < 90.0) {
            return Err(format!("hud.json: level_deg {} and near_level_deg {} out of range", t.level_deg, t.near_level_deg));
        }
        Ok(t)
    }
}

/// How level the ship sits against the planet's up, for the horizon's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Level,
    Near,
    Off,
}

/// The tilt of the ship's up against the planet's up (deg) from the horizon's pitch and roll.
pub fn tilt_deg(pitch_deg: f64, roll_deg: f64) -> f64 {
    (pitch_deg.to_radians().cos() * roll_deg.to_radians().cos()).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Level, nearly level or off, by the tilt (deg) and the bands in `t`.
pub fn level(tilt: f64, t: &HudTuning) -> Level {
    if tilt <= t.level_deg {
        Level::Level
    } else if tilt <= t.near_level_deg {
        Level::Near
    } else {
        Level::Off
    }
}

/// Which height the altitude element shows (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    /// Above the terrain under the player, near the ground.
    Agl(f64),
    /// Above the planet's reference sphere.
    Alt(f64),
}

impl Height {
    /// AGL below `agl_below` m above the ground, else ALT.
    pub fn pick(above_ground: f64, above_sphere: f64, agl_below: f64) -> Height {
        if above_ground < agl_below { Height::Agl(above_ground) } else { Height::Alt(above_sphere) }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn bands() -> HudTuning {
        HudTuning { agl_below: 1000.0, toast_time: 1.5, g_full: 10.0, velocity_min: 1.0, level_deg: 1.0, near_level_deg: 5.0 }
    }

    #[test]
    fn tilt_joins_pitch_and_roll_and_picks_the_band() {
        assert!(tilt_deg(0.0, 0.0).abs() < 1e-9);
        assert!((tilt_deg(3.0, 0.0) - 3.0).abs() < 1e-9 && (tilt_deg(0.0, -4.0) - 4.0).abs() < 1e-9);
        let both = tilt_deg(3.0, 4.0);
        assert!(both > 4.0 && both < 5.1, "3 and 4 deg tilt about 5 deg: {both}");
        let t = bands();
        assert_eq!((level(0.5, &t), level(3.0, &t), level(8.0, &t)), (Level::Level, Level::Near, Level::Off));
    }
}
