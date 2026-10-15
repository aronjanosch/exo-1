use glam::DVec3;

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64. Same seed, same numbers, on every platform.
#[derive(Clone, Debug)]
pub struct Rng(u64);

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// An independent stream for (seed, key): one stream per concern, so changing one part of a
    /// generator does not shift the others.
    pub fn derive(seed: u64, key: u64) -> Rng {
        Rng::new(mix(seed ^ mix(key)))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(GOLDEN);
        mix(self.0)
    }

    /// [0, 1): the top 53 bits.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / 9_007_199_254_740_992.0
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }

    /// Inclusive at both ends; `lo <= hi`.
    pub fn count(&mut self, lo: u32, hi: u32) -> u32 {
        let n = (self.unit() * (f64::from(hi - lo) + 1.0)) as u32;
        lo + n.min(hi - lo)
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        let i = ((self.unit() * items.len() as f64) as usize).min(items.len() - 1);
        items.get(i)
    }

    /// Uniform on the unit sphere: z uniform in [-1, 1], angle uniform.
    pub fn direction(&mut self) -> DVec3 {
        let z = self.range(-1.0, 1.0);
        let phi = self.range(0.0, std::f64::consts::TAU);
        let r = (1.0 - z * z).sqrt();
        DVec3::new(r * phi.cos(), r * phi.sin(), z)
    }
}
