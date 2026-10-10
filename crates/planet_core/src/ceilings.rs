//! Ceilings first (#177, spike 14): radius, atmosphere height and the tallest terrain are planet
//! data, and the heights a path or a jump depends on follow from them by one rule.
//!
//! The rule (TODO(initiator): start values, KSP's ratio is a reference, not a rule):
//! - atmosphere: 1.2 km at 5 km radius (today's Hearth), then 1/8.57 of the radius (35 km at 300 km);
//! - the tallest terrain is 1/10 of the atmosphere;
//! - the obstruction radius is the radius plus a third of the atmosphere (400 m at Hearth);
//! - the arrival radius is the radius plus the larger of 7 km and two atmospheres.

/// Atmosphere share of the radius above the Hearth minimum.
pub const ATMOSPHERE_PER_RADIUS: f64 = 0.1167;
pub const ATMOSPHERE_MIN_M: f64 = 1200.0;
/// Tallest terrain as a share of the atmosphere height.
pub const TERRAIN_SHARE: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ceilings {
    pub radius: f64,
    pub atmosphere_height: f64,
    /// Highest ground above the radius a planet of this atmosphere may have (m).
    pub terrain_max: f64,
    pub obstruction_radius: f64,
    pub arrival_radius: f64,
}

impl Ceilings {
    /// The rule's atmosphere for a radius.
    pub fn atmosphere_for(radius: f64) -> f64 {
        (radius * ATMOSPHERE_PER_RADIUS).max(ATMOSPHERE_MIN_M)
    }

    pub fn for_radius(radius: f64) -> Ceilings {
        Ceilings::with_atmosphere(radius, Ceilings::atmosphere_for(radius))
    }

    /// The other ceilings of a radius with the atmosphere given.
    pub fn with_atmosphere(radius: f64, atmosphere_height: f64) -> Ceilings {
        Ceilings {
            radius,
            atmosphere_height,
            terrain_max: atmosphere_height * TERRAIN_SHARE,
            obstruction_radius: radius + atmosphere_height / 3.0,
            arrival_radius: radius + (2.0 * atmosphere_height).max(7000.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hearth_keeps_its_numbers() {
        let c = Ceilings::for_radius(5000.0);
        assert_eq!(c.atmosphere_height, 1200.0);
        assert_eq!(c.obstruction_radius, 5400.0);
        assert_eq!(c.arrival_radius, 12000.0);
        assert_eq!(c.terrain_max, 120.0);
    }

    #[test]
    fn the_three_radii_of_the_spike() {
        for (r, atm) in [(30_000.0, 3501.0), (100_000.0, 11_670.0), (300_000.0, 35_010.0)] {
            let c = Ceilings::for_radius(r);
            assert!((c.atmosphere_height - atm).abs() < 1.0, "{r}: {}", c.atmosphere_height);
            assert!(c.terrain_max * 10.0 <= c.atmosphere_height * 1.0001);
            assert!(c.obstruction_radius > r + c.terrain_max, "a path keeps out of the tallest terrain");
            assert!(c.arrival_radius > c.obstruction_radius + c.atmosphere_height, "the exit point is above the atmosphere");
        }
    }

    #[test]
    fn atmosphere_is_a_plain_parameter() {
        let c = Ceilings::with_atmosphere(30_000.0, 9000.0);
        assert_eq!(c.terrain_max, 900.0);
        assert_eq!(c.radius, 30_000.0);
        assert_eq!(c.obstruction_radius, 33_000.0);
    }
}
