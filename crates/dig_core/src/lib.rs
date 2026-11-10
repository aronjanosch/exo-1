//! The dig run's vein (#324): a rock the welder cuts chunks from. Heat rises while the cut is held,
//! a chunk comes off at full heat, the spot cools when the cut is let go, an emptied vein grows back
//! after a while. No Bevy types; the host steps it, clients only read its state.
use content_core::{parse_strict, File};
use serde::Deserialize;

pub const PATH: &str = "dig/dig.json";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VeinSpec {
    /// The ore's item id.
    pub ore: String,
    /// The vein's centre in metres from the dig site's centre (x east, y north).
    pub at: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigConfig {
    pub cut_s: f64,
    pub cool_per_s: f64,
    pub chunks: u32,
    pub regrow_s: f64,
    pub reach_m: f64,
    pub veins: Vec<VeinSpec>,
}

impl DigConfig {
    pub fn load(files: &[File]) -> Result<DigConfig, Vec<String>> {
        let Some(f) = files.iter().find(|f| f.path == PATH) else { return Err(vec![format!("{PATH}: missing")]) };
        DigConfig::parse(&f.text)
    }

    pub fn parse(text: &str) -> Result<DigConfig, Vec<String>> {
        let c = parse_strict::<DigConfig>(PATH, text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        let mut need = |ok: bool, field: &str, rule: &str| {
            if !ok {
                e.push(format!("{PATH}: {field}: {rule}"));
            }
        };
        need(c.cut_s.is_finite() && c.cut_s > 0.0, "cut_s", "must be > 0");
        need(c.cool_per_s.is_finite() && c.cool_per_s >= 0.0, "cool_per_s", "must be >= 0");
        need(c.chunks > 0, "chunks", "must be > 0");
        need(c.regrow_s.is_finite() && c.regrow_s >= 0.0, "regrow_s", "must be >= 0");
        need(c.reach_m.is_finite() && c.reach_m > 0.0, "reach_m", "must be > 0");
        need(!c.veins.is_empty(), "veins", "needs at least one vein");
        need(c.veins.iter().all(|v| v.at.iter().all(|x| x.is_finite())), "veins", "a place must be finite");
        if e.is_empty() { Ok(c) } else { Err(e) }
    }
}

/// One vein as the host runs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Vein {
    pub ore: String,
    pub chunks_left: u32,
    /// The cut spot's heat, 0..1.
    pub heat: f64,
    /// Seconds since the vein ran empty.
    pub regrow: f64,
}

impl Vein {
    pub fn new(ore: &str, cfg: &DigConfig) -> Vein {
        Vein { ore: ore.to_string(), chunks_left: cfg.chunks, heat: 0.0, regrow: 0.0 }
    }

    /// One step of `dt` seconds. Returns the ore of a chunk that came off, at most one per step: a
    /// huge `dt` is one chunk, not many (the heat the step had over is dropped). A `dt` that is
    /// negative or not finite is no time.
    pub fn step(&mut self, cfg: &DigConfig, dt: f64, cutting: bool) -> Option<String> {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        if self.chunks_left == 0 {
            self.heat = 0.0;
            self.regrow += dt;
            if self.regrow >= cfg.regrow_s {
                self.chunks_left = cfg.chunks;
                self.regrow = 0.0;
            }
            return None;
        }
        if cutting {
            self.heat += dt / cfg.cut_s;
            if self.heat >= 1.0 {
                self.heat = 0.0;
                self.chunks_left -= 1;
                return Some(self.ore.clone());
            }
        } else {
            self.heat -= dt * cfg.cool_per_s;
        }
        self.heat = if self.heat.is_finite() { self.heat.clamp(0.0, 1.0) } else { 0.0 };
        None
    }

    /// The vein's visible size, 1.0 full down to 0.5 at the last chunk; 0 when empty.
    pub fn size(&self, cfg: &DigConfig) -> f64 {
        if self.chunks_left == 0 {
            return 0.0;
        }
        0.5 + 0.5 * (self.chunks_left - 1) as f64 / (cfg.chunks.max(2) - 1) as f64
    }
}

/// Where a ray first meets an axis-aligned box: the distance along `dir` (a unit vector), 0 when the
/// origin is inside, None when it misses or the box lies behind. The slab test, in whatever frame
/// the three arrays share.
pub fn ray_box(origin: [f64; 3], dir: [f64; 3], centre: [f64; 3], half: [f64; 3]) -> Option<f64> {
    if origin.iter().chain(&dir).chain(&centre).chain(&half).any(|x| !x.is_finite()) {
        return None;
    }
    let (mut near, mut far) = (0.0_f64, f64::INFINITY);
    for i in 0..3 {
        let (o, d, lo, hi) = (origin[i], dir[i], centre[i] - half[i], centre[i] + half[i]);
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let (a, b) = ((lo - o) / d, (hi - o) / d);
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    near.is_finite().then_some(near)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_hits_a_box_in_front_and_misses_beside_and_behind_it() {
        let (c, h) = ([5.0, 0.0, 0.5], [0.5, 0.5, 0.5]);
        assert_eq!(ray_box([0.0, 0.0, 0.5], [1.0, 0.0, 0.0], c, h), Some(4.5));
        assert_eq!(ray_box([0.0, 2.0, 0.5], [1.0, 0.0, 0.0], c, h), None, "beside");
        assert_eq!(ray_box([0.0, 0.0, 0.5], [-1.0, 0.0, 0.0], c, h), None, "behind");
        assert_eq!(ray_box([5.0, 0.0, 0.5], [0.0, 1.0, 0.0], c, h), Some(0.0), "inside");
        let d = [1.0 / 2f64.sqrt(), 1.0 / 2f64.sqrt(), 0.0];
        let t = ray_box([0.0, 0.0, 0.5], d, [5.0, 5.0, 0.5], h).expect("the slanted ray reaches the box");
        assert!((t - 4.5 * 2f64.sqrt()).abs() < 1e-9, "{t}");
        assert_eq!(ray_box([f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], c, h), None);
    }

    fn cfg() -> DigConfig {
        DigConfig { cut_s: 2.0, cool_per_s: 0.5, chunks: 4, regrow_s: 10.0, reach_m: 2.5, veins: vec![VeinSpec { ore: "grumblite".into(), at: [0.0, 0.0] }] }
    }

    fn shipped() -> DigConfig {
        DigConfig::parse(include_str!("../../../content/gameplay/dig/dig.json")).expect("the shipped file parses")
    }

    #[test]
    fn the_shipped_config_parses_and_unknown_fields_are_named() {
        let c = shipped();
        assert_eq!(c.veins.len(), 3);
        let bad = include_str!("../../../content/gameplay/dig/dig.json").replace("\"chunks\": 8,", "\"chunks\": 8, \"lumps\": 3,");
        let e = DigConfig::parse(&bad).unwrap_err();
        assert!(e[0].contains("lumps"), "{e:?}");
        let zero = include_str!("../../../content/gameplay/dig/dig.json").replace("\"cut_s\": 2.5", "\"cut_s\": 0.0");
        assert!(DigConfig::parse(&zero).unwrap_err()[0].contains("cut_s"));
    }

    #[test]
    fn heat_stays_in_range_and_finite_for_any_dt_sequence() {
        let c = cfg();
        let mut v = Vein::new("grumblite", &c);
        let dts = [0.0, 0.016, 1e-9, 0.5, 3.0, 1e6, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.1];
        for i in 0..400 {
            let dt = dts[i % dts.len()];
            v.step(&c, dt, i % 3 != 0);
            assert!(v.heat.is_finite() && (0.0..=1.0).contains(&v.heat), "heat {} at step {i}", v.heat);
            assert!(v.chunks_left <= c.chunks);
        }
    }

    #[test]
    fn exactly_the_full_vein_comes_out_and_never_more() {
        let c = cfg();
        let mut v = Vein::new("grumblite", &c);
        let mut got = 0;
        for _ in 0..1000 {
            if v.step(&c, 0.1, true).is_some() {
                got += 1;
            }
            if v.chunks_left == 0 {
                break;
            }
        }
        assert_eq!(got, c.chunks);
        // Cut after it is empty (before it regrows): nothing.
        for _ in 0..20 {
            assert_eq!(v.step(&c, 0.1, true), None);
            assert_eq!(v.chunks_left, 0);
        }
    }

    #[test]
    fn a_chunk_takes_cut_s_of_cutting() {
        let c = cfg();
        let mut v = Vein::new("snoozium", &c);
        let mut t = 0.0;
        let ore = loop {
            t += 0.01;
            if let Some(o) = v.step(&c, 0.01, true) {
                break o;
            }
            assert!(t < 5.0);
        };
        assert_eq!(ore, "snoozium");
        assert!((t - c.cut_s).abs() < 0.05, "{t}");
        assert_eq!(v.heat, 0.0, "the heat starts again");
        assert_eq!(v.chunks_left, c.chunks - 1);
    }

    #[test]
    fn letting_go_cools_but_not_below_zero() {
        let c = cfg();
        let mut v = Vein::new("grumblite", &c);
        v.step(&c, 1.0, true);
        assert!((v.heat - 0.5).abs() < 1e-9);
        v.step(&c, 0.5, false);
        assert!((v.heat - 0.25).abs() < 1e-9);
        v.step(&c, 100.0, false);
        assert_eq!(v.heat, 0.0);
        assert_eq!(v.chunks_left, c.chunks, "cooling takes no chunk");
    }

    #[test]
    fn a_huge_step_gives_at_most_one_chunk() {
        let c = cfg();
        let mut v = Vein::new("grumblite", &c);
        assert!(v.step(&c, 1e9, true).is_some());
        assert_eq!(v.chunks_left, c.chunks - 1);
    }

    #[test]
    fn an_empty_vein_regrows_after_regrow_s_and_only_then() {
        let c = cfg();
        let mut v = Vein::new("grumblite", &c);
        v.chunks_left = 0;
        v.step(&c, 9.9, false);
        assert_eq!(v.chunks_left, 0);
        v.step(&c, 0.2, true);
        assert_eq!(v.chunks_left, c.chunks, "full again, cutting or not");
        assert_eq!(v.heat, 0.0);
    }

    #[test]
    fn the_size_runs_from_full_to_half_at_the_last_chunk_and_zero_empty() {
        let c = cfg();
        let mut v = Vein::new("grumblite", &c);
        assert_eq!(v.size(&c), 1.0);
        v.chunks_left = 1;
        assert_eq!(v.size(&c), 0.5);
        v.chunks_left = 0;
        assert_eq!(v.size(&c), 0.0);
    }
}
