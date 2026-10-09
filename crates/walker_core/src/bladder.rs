//! First-version avatar need: 60 seconds filling, then 5 seconds automatically emptying.
use glam::{DQuat, DVec3};

pub const FILL_SECONDS: f64 = 60.0;
pub const URINATE_SECONDS: f64 = 5.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Bladder {
    elapsed: f64,
    urinating: bool,
}

impl Bladder {
    pub fn urinating(&self) -> bool { self.urinating }

    pub fn fullness(&self) -> f64 {
        if self.urinating { 1.0 - self.elapsed / URINATE_SECONDS } else { self.elapsed / FILL_SECONDS }
    }

    /// Preserve leftover time when a step crosses one or more phase boundaries.
    pub fn step(&mut self, dt: f64) {
        assert!(dt.is_finite() && dt >= 0.0);
        self.elapsed += dt;
        loop {
            let duration = if self.urinating { URINATE_SECONDS } else { FILL_SECONDS };
            if self.elapsed < duration { break; }
            self.elapsed -= duration;
            self.urinating = !self.urinating;
        }
    }
}

/// Same pitch convention as the first-person camera, valid in planet or cabin space.
pub fn look_direction(forward: DVec3, up: DVec3, pitch: f64) -> DVec3 {
    let forward = (forward - up * forward.dot(up)).normalize();
    DQuat::from_axis_angle(forward.cross(up).normalize(), pitch) * forward
}

/// Emitter pose in world doubles. Geometry values are initial visual choices.
#[derive(Clone, Copy, Debug, Default)]
pub struct Jet {
    pub start: DVec3,
    pub direction: DVec3,
    /// Avatar up, used to orient horizontal/vertical spread on planets and in rotated cabins.
    pub up: DVec3,
}

impl Jet {
    pub fn new(feet: DVec3, up: DVec3, look: DVec3) -> Self {
        Self { start: feet + up * 0.9 + look * 0.4, direction: look, up }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_cycle_and_leftover_time() {
        let mut b = Bladder::default();
        assert_eq!(b.fullness(), 0.0);
        b.step(30.0);
        assert_eq!(b.fullness(), 0.5);
        assert!(!b.urinating());
        b.step(30.0);
        assert!(b.urinating());
        assert_eq!(b.fullness(), 1.0);
        b.step(2.5);
        assert_eq!(b.fullness(), 0.5);
        b.step(2.5);
        assert!(!b.urinating());
        assert_eq!(b.fullness(), 0.0);
        b.step(132.5);
        assert!(!b.urinating());
        assert!((b.fullness() - 2.5 / 60.0).abs() < 1e-12);
    }

    #[test]
    fn look_and_jet_work_in_a_rotated_far_away_cabin() {
        let rotation = DQuat::from_rotation_z(1.2);
        let feet = DVec3::new(200_000.0, 5000.0, -100_000.0);
        let up = rotation * DVec3::Y;
        let look = look_direction(rotation * DVec3::NEG_Z, up, 0.7);
        let expected = rotation * DVec3::new(0.0, 0.7_f64.sin(), -0.7_f64.cos());
        assert!(look.distance(expected) < 1e-12);
        let jet = Jet::new(feet, up, look);
        assert!((jet.start - feet).distance(up * 0.9 + look * 0.4) < 1e-10);
    }
}
