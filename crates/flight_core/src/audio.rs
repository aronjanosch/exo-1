//! Thruster audio: one thrust signal drives the sound layers, each with its own attack and
//! release (exponential slew). Pure math, no Bevy types; `exo_app::audio` only plays the levels.
//!
//!
//! All tuning values are start values: `TODO(initiator)`, to tune by ear.

/// What the ship does this step, in ship space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThrusterSignal {
    /// Signed thrust share per axis (x, y, z), each -1..1; z negative is forward.
    pub thrust: [f64; 3],
    pub boost: bool,
    /// The ship is parked (static, nobody has sat down): every layer falls to 0.
    pub parked: bool,
}

/// Layer levels, each 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerLevels {
    pub rumble: f64,
    /// +x, -x, +y, -y, +z, -z.
    pub hiss: [f64; 6],
    pub boost: f64,
    /// Rising edges of `boost` so far (boost starts). The app plays the start punch on each.
    pub boost_starts: u32,
}

/// Time constants in seconds (`TODO(initiator)`: start values, tune by ear).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrusterAudioTuning {
    pub rumble_attack: f64,
    pub rumble_release: f64,
    pub idle: f64,
    /// Rumble added per unit of forward/backward (z) thrust.
    pub rumble_forward: f64,
    /// Rumble added per unit of overall thrust share (the length of the thrust vector, max 1).
    pub rumble_share: f64,
    pub hiss_attack: f64,
    pub hiss_release: f64,
    pub spool_up: f64,
    pub spool_down: f64,
}

impl Default for ThrusterAudioTuning {
    fn default() -> Self {
        ThrusterAudioTuning {
            rumble_attack: 0.05,   // TODO(initiator)
            rumble_release: 0.6,   // TODO(initiator)
            idle: 0.08,            // TODO(initiator)
            rumble_forward: 0.7,   // TODO(initiator)
            rumble_share: 0.3,     // TODO(initiator)
            hiss_attack: 0.03,     // TODO(initiator)
            hiss_release: 0.4,     // TODO(initiator)
            spool_up: 0.6,         // TODO(initiator)
            spool_down: 0.8,       // TODO(initiator)
        }
    }
}

/// Fraction of the way from `level` to `target` in one step of `dt` with time constant `tau`.
fn slew(level: f64, target: f64, dt: f64, tau: f64) -> f64 {
    if tau <= 0.0 {
        return target;
    }
    let k = 1.0 - (-dt.max(0.0) / tau).exp();
    level + (target - level) * k
}

#[derive(Clone, Debug, Default)]
pub struct ThrusterAudio {
    pub tuning: ThrusterAudioTuning,
    levels: LayerLevels,
    boosting: bool,
}

impl ThrusterAudio {
    pub fn new(tuning: ThrusterAudioTuning) -> ThrusterAudio {
        ThrusterAudio { tuning, levels: LayerLevels::default(), boosting: false }
    }

    pub fn levels(&self) -> LayerLevels {
        self.levels
    }

    /// One step of `dt` seconds. Input outside the ranges is clamped.
    pub fn step(&mut self, input: ThrusterSignal, dt: f64) -> LayerLevels {
        let t = &self.tuning;
        let thrust = input.thrust.map(|x| if x.is_finite() { x.clamp(-1.0, 1.0) } else { 0.0 });
        let on = !input.parked;
        let share = (thrust[0] * thrust[0] + thrust[1] * thrust[1] + thrust[2] * thrust[2]).sqrt().min(1.0);

        let rumble_target = if on { (t.idle + t.rumble_forward * thrust[2].abs() + t.rumble_share * share).min(1.0) } else { 0.0 };
        let rumble_tau = if rumble_target > self.levels.rumble { t.rumble_attack } else { t.rumble_release };
        self.levels.rumble = slew(self.levels.rumble, rumble_target, dt, rumble_tau);

        for axis in 0..3 {
            for (side, sign) in [(0, 1.0), (1, -1.0)] {
                let i = axis * 2 + side;
                let target = if on { (thrust[axis] * sign).max(0.0) } else { 0.0 };
                let tau = if target > self.levels.hiss[i] { t.hiss_attack } else { t.hiss_release };
                self.levels.hiss[i] = slew(self.levels.hiss[i], target, dt, tau);
            }
        }

        let roar_target = if on && input.boost { 1.0 } else { 0.0 };
        let roar_tau = if roar_target > self.levels.boost { t.spool_up } else { t.spool_down };
        self.levels.boost = slew(self.levels.boost, roar_target, dt, roar_tau);

        // Boost start: each rising edge counts once, held boost does not count again.
        let boosting = input.boost && on;
        if boosting && !self.boosting {
            self.levels.boost_starts += 1;
        }
        self.boosting = boosting;

        self.levels.rumble = self.levels.rumble.clamp(0.0, 1.0);
        self.levels.boost = self.levels.boost.clamp(0.0, 1.0);
        for h in &mut self.levels.hiss {
            *h = h.clamp(0.0, 1.0);
        }
        self.levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    fn run(a: &mut ThrusterAudio, input: ThrusterSignal, secs: f64) -> LayerLevels {
        let mut l = a.levels();
        for _ in 0..(secs / DT).round() as usize {
            l = a.step(input, DT);
        }
        l
    }

    #[test]
    fn boost_starts_count_rising_edges_only() {
        let mut a = ThrusterAudio::default();
        let on = ThrusterSignal { boost: true, ..Default::default() };
        let off = ThrusterSignal::default();
        assert_eq!(run(&mut a, on, 2.0).boost_starts, 1);
        assert_eq!(run(&mut a, on, 2.0).boost_starts, 1, "held: no new start");
        assert_eq!(run(&mut a, off, 0.5).boost_starts, 1);
        assert_eq!(run(&mut a, on, 0.5).boost_starts, 2, "second press");
    }

    #[test]
    fn levels_stay_in_range_for_any_input() {
        let mut a = ThrusterAudio::default();
        let wild = [
            ThrusterSignal { thrust: [5.0, -9.0, 3.0], boost: true, parked: false },
            ThrusterSignal { thrust: [f64::NAN, f64::INFINITY, -1e9], boost: false, parked: false },
            ThrusterSignal { thrust: [-2.0, 2.0, 2.0], boost: true, parked: true },
        ];
        for s in wild.iter().cycle().take(600) {
            let l = a.step(*s, DT);
            let all = std::iter::once(l.rumble).chain(l.hiss).chain(std::iter::once(l.boost));
            for v in all {
                assert!(v.is_finite() && (0.0..=1.0).contains(&v), "level {v} out of range");
            }
        }
    }

}
