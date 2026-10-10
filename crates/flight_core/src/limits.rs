//! Per-axis and per-direction limits of the ship: the thrust box (`Dirs`), the turn box (`Rot`)
//! and one g (`G0`), shared by the SC model and the ground rules.
use glam::DVec3;
use serde::Deserialize;

/// m/s²: one g, for the G-safety limits.
pub const G0: f64 = 9.81;

/// One number per axis and direction in ship space: forward is -Z, right +X, up +Y.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Dirs {
    pub forward: f64,
    pub backward: f64,
    pub left: f64,
    pub right: f64,
    pub up: f64,
    pub down: f64,
}

impl Dirs {
    pub fn splat(x: f64) -> Dirs {
        Dirs { forward: x, backward: x, left: x, right: x, up: x, down: x }
    }

    fn zip(&self, o: &Dirs, f: impl Fn(f64, f64) -> f64) -> Dirs {
        Dirs {
            forward: f(self.forward, o.forward),
            backward: f(self.backward, o.backward),
            left: f(self.left, o.left),
            right: f(self.right, o.right),
            up: f(self.up, o.up),
            down: f(self.down, o.down),
        }
    }

    pub fn mul(&self, o: &Dirs) -> Dirs {
        self.zip(o, |a, b| a * b)
    }

    pub fn scaled(&self, s: f64) -> Dirs {
        self.zip(self, |a, _| a * s)
    }

    /// The box these limits span, applied to a local vector (x right, y up, z backward).
    pub fn clamp(&self, v: DVec3) -> DVec3 {
        DVec3::new(v.x.clamp(-self.left, self.right), v.y.clamp(-self.down, self.up), v.z.clamp(-self.forward, self.backward))
    }

    /// Each component of a local vector times the limit in its direction (a stick -1..1 per axis
    /// to the full thrust that way).
    pub fn along(&self, v: DVec3) -> DVec3 {
        DVec3::new(
            v.x * if v.x >= 0.0 { self.right } else { self.left },
            v.y * if v.y >= 0.0 { self.up } else { self.down },
            v.z * if v.z >= 0.0 { self.backward } else { self.forward },
        )
    }

    /// The smaller limit per direction (two boxes overlapped).
    pub fn min(&self, o: &Dirs) -> Dirs {
        self.zip(o, f64::min)
    }

    /// How far the box reaches along a local unit vector (the most thrust that way).
    pub fn support(&self, u: DVec3) -> f64 {
        self.along(u).dot(u)
    }

    pub(crate) fn validate(&self, what: &str) -> Result<(), String> {
        for (n, v) in [("forward", self.forward), ("backward", self.backward), ("left", self.left), ("right", self.right), ("up", self.up), ("down", self.down)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}

/// One number per rotation axis: pitch (about +X, nose up), yaw (about +Y, nose left), roll.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rot {
    pub pitch: f64,
    pub yaw: f64,
    pub roll: f64,
}

impl Rot {
    pub(crate) fn validate(&self, what: &str) -> Result<(), String> {
        for (n, v) in [("pitch", self.pitch), ("yaw", self.yaw), ("roll", self.roll)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}
