//! Terrain shape statistics for the 100 km planet: how tall the ground gets, how much of
//! the land is plain enough for a settlement, how many biomes show up. Sampled on a Fibonacci
//! lattice of the sphere, so the numbers are area-weighted and the same on every run.
use crate::look::{tangent_frame, walk};
use crate::math::*;
use crate::planet::Planet;
use serde::Serialize;
use std::collections::BTreeMap;

/// Slope limit and baseline of a "plain" (goal: under 5 degrees over 500 m).
pub const PLAIN_DEG: f64 = 5.0;
pub const PLAIN_BASE_M: f64 = 500.0;
pub const BIOME_MIN_SHARE: f64 = 0.005;

#[derive(Clone, Debug, Default, Serialize)]
pub struct ShapeStats {
    pub samples: usize,
    /// Dry land (above the sea, no lake or river water) of all samples.
    pub land_share: f64,
    /// Land samples whose slope over `PLAIN_BASE_M` stays under `PLAIN_DEG`, in both directions.
    pub plain_share_land: f64,
    /// Highest sampled ground above the base radius and above the sea (m).
    pub max_above_radius_m: f64,
    pub max_above_sea_m: f64,
    pub p99_land_above_sea_m: f64,
    pub biome_share_land: BTreeMap<u8, f64>,
    pub biomes_present: usize,
}

fn lattice(i: usize, n: usize) -> V3 {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let th = golden * i as f64;
    v3(r * th.cos(), y, r * th.sin())
}

struct One {
    height: f64,
    above_sea: f64,
    land: bool,
    plain: bool,
    biome: u8,
}

impl Planet {
    /// Shape statistics over `n` lattice points (`threads` 0 = all cores).
    pub fn shape_stats(&self, n: usize, threads: usize) -> ShapeStats {
        let threads = if threads == 0 { std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1) } else { threads };
        let chunk = n.div_ceil(threads);
        let ones: Vec<One> = std::thread::scope(|s| {
            let jobs: Vec<_> = (0..threads)
                .map(|k| {
                    s.spawn(move || {
                        (k * chunk..((k + 1) * chunk).min(n))
                            .map(|i| {
                                let d = lattice(i, n);
                                let smp = self.sample(d);
                                let land = smp.height_above_sea > 0.0 && smp.water_depth <= 0.0;
                                let plain = land && {
                                    let (t, b) = tangent_frame(d);
                                    let half = PLAIN_BASE_M * 0.5;
                                    let h = |dir: V3, m: f64| self.height_at(walk(d, dir, m, self.radius));
                                    let rise = (h(t, half) - h(t, -half)).abs().max((h(b, half) - h(b, -half)).abs());
                                    rise.atan2(PLAIN_BASE_M).to_degrees() < PLAIN_DEG
                                };
                                One { height: smp.height, above_sea: smp.height_above_sea, land, plain, biome: smp.biome.clamp(0, 255) as u8 }
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
        });
        let land: Vec<&One> = ones.iter().filter(|o| o.land).collect();
        let nl = land.len().max(1) as f64;
        let mut biome_share_land = BTreeMap::new();
        for o in &land {
            *biome_share_land.entry(o.biome).or_insert(0.0) += 1.0 / nl;
        }
        let mut hs: Vec<f64> = land.iter().map(|o| o.above_sea).collect();
        hs.sort_by(f64::total_cmp);
        ShapeStats {
            samples: ones.len(),
            land_share: land.len() as f64 / ones.len().max(1) as f64,
            plain_share_land: land.iter().filter(|o| o.plain).count() as f64 / nl,
            max_above_radius_m: ones.iter().map(|o| o.height).fold(f64::MIN, f64::max),
            max_above_sea_m: ones.iter().map(|o| o.above_sea).fold(f64::MIN, f64::max),
            p99_land_above_sea_m: hs.get(((hs.len() as f64 - 1.0) * 0.99).round().max(0.0) as usize).copied().unwrap_or(0.0),
            biomes_present: biome_share_land.values().filter(|&&v| v >= BIOME_MIN_SHARE).count(),
            biome_share_land,
        }
    }
}
