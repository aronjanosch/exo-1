//! Small ballistic droplets, without engine types or particle-particle interaction.
use crate::bladder::Jet;
use glam::DVec3;

pub const RATE: f64 = 120.0;
pub const SPEED: f64 = 8.0;
pub const LIFETIME: f64 = 10.0;
/// Independent horizontal and vertical bounds; both offsets can occur together.
pub const SPREAD_DEGREES: f64 = 0.5;

pub trait World {
    fn gravity_at(&self, position: DVec3) -> DVec3;
    /// Sweep the complete travelled segment, including a start already inside a surface.
    fn hits(&self, from: DVec3, to: DVec3) -> bool;
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub previous: DVec3,
    pub position: DVec3,
    pub velocity: DVec3,
    pub age: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Alive, Hit, Expired }

impl Particle {
    pub fn new(jet: Jet, carrier_velocity: DVec3) -> Self {
        Self { previous: jet.start, position: jet.start, velocity: jet.direction * SPEED + carrier_velocity, age: 0.0 }
    }

    pub fn step(&mut self, dt: f64, world: &impl World) -> Status {
        let dt = dt.min((LIFETIME - self.age).max(0.0));
        self.previous = self.position;
        let gravity = world.gravity_at(self.position);
        let next = self.position + self.velocity * dt + gravity * (0.5 * dt * dt);
        self.age += dt;
        if world.hits(self.position, next) { return Status::Hit; }
        self.position = next;
        self.velocity += gravity * dt;
        if self.age >= LIFETIME - 1e-10 { Status::Expired } else { Status::Alive }
    }
}

#[derive(Default)]
pub struct Emitter { remainder: f64, rng_state: u64 }

impl Emitter {
    /// SplitMix64, matching the small deterministic generator used by net_core.
    /// Each emitter keeps its own sequence across cycles, so scenarios remain reproducible.
    fn random_angle(&mut self) -> f64 {
        self.rng_state = self.rng_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let unit = (z >> 11) as f64 / (1_u64 << 53) as f64;
        (unit * 2.0 - 1.0) * SPREAD_DEGREES.to_radians()
    }

    /// Return each birth with the time it still flies in this tick. Births are spaced at 1/120 s,
    /// rather than emitting two particles at the same place once per 60 Hz tick.
    pub fn emit(&mut self, jet: Option<Jet>, carrier_velocity: DVec3, dt: f64) -> Vec<(Particle, f64)> {
        let Some(jet) = jet else { self.remainder = 0.0; return Vec::new(); };
        self.remainder += dt;
        let mut births = Vec::new();
        while self.remainder >= 1.0 / RATE - 1e-12 {
            self.remainder = (self.remainder - 1.0 / RATE).max(0.0);
            let horizontal = self.random_angle();
            let vertical = self.random_angle();
            let direction = spread_direction(jet, horizontal, vertical);
            births.push((Particle::new(Jet { direction, ..jet }, carrier_velocity), self.remainder));
        }
        births
    }
}

fn spread_direction(jet: Jet, horizontal: f64, vertical: f64) -> DVec3 {
    let right = jet.direction.cross(jet.up).try_normalize().unwrap_or_else(|| jet.direction.any_orthonormal_vector());
    let up = right.cross(jet.direction).normalize();
    // Plane projections retain the independently requested angles, including diagonal offsets.
    (jet.direction + right * horizontal.tan() + up * vertical.tan()).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture { gravity: DVec3, wall_z: Option<f64> }
    impl World for Fixture {
        fn gravity_at(&self, _: DVec3) -> DVec3 { self.gravity }
        fn hits(&self, from: DVec3, to: DVec3) -> bool {
            self.wall_z.is_some_and(|z| from.z <= z || to.z <= z)
        }
    }
    fn jet() -> Jet { Jet { start: DVec3::Y, direction: DVec3::NEG_Z, up: DVec3::Y } }

    #[test]
    fn parabola_and_swept_collision() {
        let world = Fixture { gravity: DVec3::NEG_Y * 9.81, wall_z: None };
        let mut p = Particle::new(jet(), DVec3::ZERO);
        for _ in 0..60 { assert_eq!(p.step(1.0 / 60.0, &world), Status::Alive); }
        assert!(p.position.distance(DVec3::new(0.0, 1.0 - 4.905, -8.0)) < 1e-10);
        assert!(p.velocity.distance(DVec3::new(0.0, -9.81, -8.0)) < 1e-10);
        let wall = Fixture { gravity: DVec3::ZERO, wall_z: Some(-0.01) };
        assert_eq!(Particle::new(jet(), DVec3::ZERO).step(1.0 / 60.0, &wall), Status::Hit);
    }

    #[test]
    fn vacuum_keeps_trajectory_and_expires_at_ten_seconds() {
        let world = Fixture { gravity: DVec3::ZERO, wall_z: None };
        let mut p = Particle::new(jet(), DVec3::X * 400.0);
        for _ in 0..599 { assert_eq!(p.step(1.0 / 60.0, &world), Status::Alive); }
        assert_eq!(p.step(1.0 / 60.0, &world), Status::Expired);
        assert!(p.position.distance(DVec3::new(4000.0, 1.0, -80.0)) < 1e-8);
        assert!(p.age <= LIFETIME);
    }

    #[test]
    fn rate_is_tick_independent_and_old_drops_do_not_retarget() {
        for hz in [30, 60, 144] {
            let mut e = Emitter::default();
            let mut count = 0;
            for _ in 0..hz { count += e.emit(Some(jet()), DVec3::ZERO, 1.0 / hz as f64).len(); }
            assert_eq!(count, 120);
        }
        let mut e = Emitter::default();
        let old = e.emit(Some(jet()), DVec3::ZERO, 1.0 / 60.0);
        assert_eq!(old.len(), 2);
        assert!((old[0].1 - old[1].1 - 1.0 / RATE).abs() < 1e-12);
        let old_velocity = old[0].0.velocity;
        let new = e.emit(Some(Jet { direction: DVec3::X, ..jet() }), DVec3::ZERO, 1.0 / 60.0);
        assert_eq!(old[0].0.velocity, old_velocity);
        assert!(old_velocity.normalize().dot(DVec3::NEG_Z) > 0.999);
        assert!(new[0].0.velocity.normalize().dot(DVec3::X) > 0.999);
        assert!(e.emit(None, DVec3::ZERO, 1.0).is_empty());
    }

    #[test]
    fn random_spread_respects_both_plane_bounds_and_keeps_launch_speed() {
        let rotation = glam::DQuat::from_rotation_z(1.2);
        let pitch = glam::DQuat::from_rotation_x(0.7);
        let forward = rotation * pitch * DVec3::NEG_Z;
        let right = rotation * DVec3::X;
        let vertical_up = rotation * pitch * DVec3::Y;
        let jet = Jet::new(DVec3::new(200_000.0, 5000.0, 0.0), rotation * DVec3::Y, forward);
        let carrier = DVec3::new(400.0, 1.0, -20.0);
        let mut e = Emitter::default();
        let mut quadrants = [false; 4];
        let mut previous_direction = None;
        for (p, _) in e.emit(Some(jet), carrier, 1.0) {
            assert_eq!(p.position, jet.start);
            let velocity = p.velocity - carrier;
            assert!((velocity.length() - SPEED).abs() < 1e-12);
            let direction = velocity.normalize();
            let horizontal = direction.dot(right).atan2(direction.dot(forward)).to_degrees();
            let vertical = direction.dot(vertical_up).atan2(direction.dot(forward)).to_degrees();
            assert!(horizontal.abs() <= SPREAD_DEGREES + 1e-12);
            assert!(vertical.abs() <= SPREAD_DEGREES + 1e-12);
            if horizontal.abs() > 0.1 && vertical.abs() > 0.1 {
                quadrants[(horizontal > 0.0) as usize + 2 * (vertical > 0.0) as usize] = true;
            }
            assert_ne!(previous_direction, Some(direction));
            previous_direction = Some(direction);
        }
        assert!(quadrants.into_iter().all(|seen| seen), "independent offsets must cover all diagonal combinations");
    }

    #[test]
    fn single_axis_and_combined_spread_reach_the_requested_limits() {
        let limit = SPREAD_DEGREES.to_radians();
        for (horizontal, vertical) in [(limit, 0.0), (0.0, -limit), (limit, -limit)] {
            let direction = spread_direction(jet(), horizontal, vertical);
            assert!((direction.x.atan2(-direction.z) - horizontal).abs() < 1e-12);
            assert!((direction.y.atan2(-direction.z) - vertical).abs() < 1e-12);
            assert!((direction.length() - 1.0).abs() < 1e-12);
        }
    }
}
