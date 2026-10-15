//! (TODO(initiator)).

/// The sampled history. Index 0 is the newest sample. All three rows always have `size` entries.
#[derive(Clone, Debug, PartialEq)]
pub struct History {
    size: usize,
    interval_s: f64,
    /// Time since the last sample, s (carried over, so the period holds over any step size).
    elapsed: f64,
    pub output: Vec<f64>,
    pub load: Vec<f64>,
    pub temp: Vec<f64>,
}

impl History {
    /// An empty history of `size` points (all zero), one sample every `interval_s` seconds.
    /// `size` is at least 1 and `interval_s` must be positive; anything else is clamped to that.
    pub fn new(size: usize, interval_s: f64) -> History {
        let size = size.max(1);
        let interval_s = if interval_s > 0.0 { interval_s } else { f64::MIN_POSITIVE };
        History { size, interval_s, elapsed: 0.0, output: vec![0.0; size], load: vec![0.0; size], temp: vec![0.0; size] }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn interval_s(&self) -> f64 {
        self.interval_s
    }

    /// Advances `dt` seconds with the current values. Every full `interval_s` takes one sample of
    /// the values given here; a long step takes at most `size` samples (the rest would be dropped).
    pub fn push_sample(&mut self, dt: f64, output_kw: f64, load_kw: f64, temp: f64) {
        self.elapsed += dt.max(0.0);
        let mut n = (self.elapsed / self.interval_s).floor() as usize;
        self.elapsed -= n as f64 * self.interval_s;
        n = n.min(self.size);
        for _ in 0..n {
            Self::shift(&mut self.output, output_kw);
            Self::shift(&mut self.load, load_kw);
            Self::shift(&mut self.temp, temp);
        }
    }

    fn shift(row: &mut Vec<f64>, v: f64) {
        row.pop();
        row.insert(0, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty_at_zero_with_the_given_size() {
        let h = History::new(25, 0.5);
        assert_eq!(h.output.len(), 25);
        assert_eq!(h.load.len(), 25);
        assert_eq!(h.temp.len(), 25);
        assert!(h.output.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn samples_once_per_interval() {
        let mut h = History::new(25, 0.5);
        h.push_sample(0.4, 100.0, 50.0, 10.0);
        assert_eq!(h.output[0], 0.0);
        h.push_sample(0.2, 100.0, 50.0, 10.0);
        assert_eq!(h.output[0], 100.0);
        assert_eq!(h.load[0], 50.0);
        assert_eq!(h.temp[0], 10.0);
        // Ten seconds in 0.25 s steps (exact in binary) give 20 samples; the rest stay untouched.
        let mut h = History::new(25, 0.5);
        for _ in 0..40 {
            h.push_sample(0.25, 7.0, 3.0, 1.0);
        }
        assert_eq!(h.output.iter().filter(|&&v| v == 7.0).count(), 20);
    }

    #[test]
    fn the_ring_keeps_only_the_newest_size_points() {
        let mut h = History::new(4, 1.0);
        for k in 1..=6 {
            h.push_sample(1.0, k as f64, 0.0, 0.0);
        }
        assert_eq!(h.output, vec![6.0, 5.0, 4.0, 3.0]);
        assert_eq!(h.output.len(), 4);
    }

    #[test]
    fn newest_comes_first_in_every_row() {
        let mut h = History::new(3, 1.0);
        h.push_sample(1.0, 1.0, 10.0, 100.0);
        h.push_sample(1.0, 2.0, 20.0, 200.0);
        assert_eq!(h.output, vec![2.0, 1.0, 0.0]);
        assert_eq!(h.load, vec![20.0, 10.0, 0.0]);
        assert_eq!(h.temp, vec![200.0, 100.0, 0.0]);
    }

    #[test]
    fn a_long_step_takes_at_most_size_samples() {
        let mut h = History::new(3, 0.5);
        h.push_sample(100.0, 9.0, 9.0, 9.0);
        assert_eq!(h.output, vec![9.0, 9.0, 9.0]);
        assert_eq!(h.size(), 3);
    }
}
