//! The collision patch's fixed-point steps stop once settled (#216): the patch then matches a
//! long fixed-point run to well under a millimetre, over steep and flat ground.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

#[test]
fn patch_settles_like_a_long_fixed_point_run() {
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, 5000.0).unwrap());
    p.bake_with(0, None, false);
    let r = p.radius;
    let centres = [v3(-0.4, 0.3, 1.0), v3(-1.0, -0.2, -0.4), v3(0.3, 1.0, 0.2)];
    let mut worst: f64 = 0.0;
    for c in centres {
        let up = c.normalized();
        let east = up.cross(v3(0.0, 1.0, 0.0)).normalized();
        let t = east;
        let b = t.cross(up);
        let hs = p.patch_heights(up, t, b, 32);
        for j in 0..32 {
            for i in 0..32 {
                let (x, z) = (i as f64 - 15.5, j as f64 - 15.5);
                let mut y = 0.0;
                for _ in 0..40 {
                    let dir = (up * r + t * x + b * z + up * y).normalized();
                    let surface = r + p.height_at(dir);
                    y = (surface * surface - x * x - z * z).sqrt() - r;
                }
                worst = worst.max((y - hs[j * 32 + i] as f64).abs());
            }
        }
    }
    println!("patch settle: worst |patch - 40-step fixed point| = {worst:.3e} m");
    assert!(worst < 1e-3, "worst {worst:.3e} m");
}
