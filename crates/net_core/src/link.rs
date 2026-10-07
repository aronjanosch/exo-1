//! One-way artificial delay, jitter and loss in the snapshot layer, receiver side and after the
//! real transport (port of link.gd). Own deterministic RNG, so no dependency.
use std::collections::VecDeque;

/// splitmix64: small, deterministic, good enough for loss and jitter.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }
}

pub struct Item<T> {
    pub due: f64,
    pub data: T,
}

pub struct Link<T> {
    pub delay: f64,
    pub jitter: f64,
    pub loss: f64,
    pub rng: Rng,
    queue: VecDeque<Item<T>>,
    pub sent: u64,
    pub dropped: u64,
}

impl<T> Link<T> {
    /// Milliseconds and percent, as on the command line of spike 4.
    pub fn new(delay_ms: f64, jitter_ms: f64, loss_percent: f64, seed: u64) -> Link<T> {
        Link {
            delay: delay_ms.clamp(0.0, 5000.0) / 1000.0,
            jitter: jitter_ms.clamp(0.0, 5000.0) / 1000.0,
            loss: loss_percent.clamp(0.0, 100.0) / 100.0,
            rng: Rng::new(seed),
            queue: VecDeque::new(),
            sent: 0,
            dropped: 0,
        }
    }

    pub fn enqueue(&mut self, now: f64, data: T) {
        self.sent += 1;
        if self.rng.f64() < self.loss {
            self.dropped += 1;
            return;
        }
        let due = now + (self.delay + self.rng.range(-self.jitter, self.jitter)).max(0.0);
        let mut i = self.queue.len();
        while i > 0 && self.queue[i - 1].due > due {
            i -= 1;
        }
        self.queue.insert(i, Item { due, data });
        if self.queue.len() > 4096 {
            self.queue.pop_front();
            self.dropped += 1;
        }
    }

    pub fn ready(&mut self, now: f64) -> Vec<T> {
        let mut out = Vec::new();
        while self.queue.front().is_some_and(|i| i.due <= now) {
            out.push(self.queue.pop_front().unwrap().data);
        }
        out
    }
}
