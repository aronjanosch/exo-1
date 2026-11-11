//! One physical thrust budget shared by velocity control and turn control.
use crate::limits::{Dirs, G0, G_SAFE_MAX_G};
use crate::limit_length;
use glam::DVec3;

#[derive(Clone, Copy, Debug)]
pub(super) struct Budget {
    pub axes: Dirs,
    pub g_safe: bool,
}

impl Budget {
    pub fn limit(&self, thrust: DVec3) -> DVec3 {
        let thrust = self.axes.clamp(thrust);
        if self.g_safe { limit_length(thrust, G_SAFE_MAX_G * G0) } else { thrust }
    }

    /// Largest share of `push` that fits after an already feasible `base`.
    pub fn share(&self, base: DVec3, push: DVec3) -> f64 {
        let mut share: f64 = 1.0;
        for (h, p, lo, hi) in [
            (base.x, push.x, -self.axes.left, self.axes.right),
            (base.y, push.y, -self.axes.down, self.axes.up),
            (base.z, push.z, -self.axes.forward, self.axes.backward),
        ] {
            if p > 1e-12 {
                share = share.min((hi - h) / p);
            } else if p < -1e-12 {
                share = share.min((lo - h) / p);
            }
        }
        if self.g_safe && push.length_squared() > 1e-18 {
            let a = push.length_squared();
            let b = 2.0 * base.dot(push);
            let c = base.length_squared() - (G_SAFE_MAX_G * G0).powi(2);
            share = share.min((-b + (b * b - 4.0 * a * c).max(0.0).sqrt()) / (2.0 * a));
        }
        share.clamp(0.0, 1.0)
    }

    pub fn add(&self, base: DVec3, push: DVec3) -> DVec3 {
        base + push * self.share(base, push)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_priority_thrust_uses_only_the_remaining_axis_and_total_g_budget() {
        let axes = Dirs { left: 20.0, right: 30.0, up: 40.0, down: 25.0, forward: 60.0, backward: 35.0 };
        for g_safe in [false, true] {
            let budget = Budget { axes, g_safe };
            for base in [DVec3::ZERO, DVec3::new(10.0, -15.0, 20.0), DVec3::new(-20.0, 40.0, -60.0)] {
                let base = budget.limit(base);
                for x in [-100.0, 0.0, 100.0] {
                    for y in [-100.0, 0.0, 100.0] {
                        for z in [-100.0, 0.0, 100.0] {
                            let push = DVec3::new(x, y, z);
                            let actual = budget.add(base, push);
                            assert!(actual.is_finite());
                            assert!((actual - axes.clamp(actual)).length() < 1e-8);
                            if g_safe {
                                assert!(actual.length() <= G_SAFE_MAX_G * G0 + 1e-8);
                            }
                            // Allocation may scale a secondary request, never rotate it
                            // or silently replace the already assigned priority.
                            assert!((actual - base).cross(push).length() < 1e-8);
                            assert!((actual - base).dot(push) >= -1e-8);
                        }
                    }
                }
            }
        }
    }
}
