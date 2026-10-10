//! The pilot's switches of the SC model, after the flight-control switches in Star Citizen's
//! records (structure only): coupled or decoupled, gravity compensation, G-safe, comstab,
//! proximity assist, landing, wind compensation, master mode (SCM or NAV) and the speed limiter.
//! Each tap flips one; coupled and decoupled blend over `decouple_time`.
use serde::Deserialize;

/// The master mode: SCM flies at combat speeds, NAV is the faster travel mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Master {
    #[default]
    Scm,
    Nav,
}

/// `sc_modes.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModeTuning {
    /// Seconds over which coupled and decoupled blend (both directions).
    pub decouple_time: f64,
    /// Share of the cap one limiter step moves (`ModeCmds::limiter_steps`).
    pub limiter_step: f64,
    /// Lowest limiter share.
    pub limiter_min: f64,
}

impl ModeTuning {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.decouple_time >= 0.0 && self.decouple_time.is_finite()) {
            return Err(format!("decouple_time {} out of range", self.decouple_time));
        }
        if !(self.limiter_step > 0.0 && self.limiter_step <= 1.0) {
            return Err(format!("limiter_step {} out of range", self.limiter_step));
        }
        if !(self.limiter_min > 0.0 && self.limiter_min <= 1.0) {
            return Err(format!("limiter_min {} out of range", self.limiter_min));
        }
        Ok(())
    }
}

impl Default for ModeTuning {
    fn default() -> Self {
        ModeTuning { decouple_time: 4.0, limiter_step: 0.1, limiter_min: 0.1 }
    }
}

/// This step's taps, one flag per switch, and the limiter steps (+ faster, - slower).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModeCmds {
    pub decoupled: bool,
    pub grav_comp: bool,
    pub g_safe: bool,
    pub comstab: bool,
    pub proximity: bool,
    pub landing: bool,
    pub wind_comp: bool,
    pub master: bool,
    pub limiter_steps: i32,
}

/// The switches' state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modes {
    /// Coupled flight wanted (C); `coupling` follows it.
    pub coupled: bool,
    /// 1 = coupled, 0 = decoupled.
    pub coupling: f64,
    /// The flight computer holds the ship against gravity (H).
    pub grav_comp: bool,
    /// Thrust and turns stay inside the pilot's G tolerance.
    pub g_safe: bool,
    /// The flight computer tightens turns (holds the velocity on the nose).
    pub comstab: bool,
    /// Looks ahead along the flight path for the ground and slows a dive.
    pub proximity: bool,
    /// Landing mode: the precision band near the ground.
    pub landing: bool,
    /// The flight computer holds against the wind.
    pub wind_comp: bool,
    pub master: Master,
    /// Speed limiter, share of the cap; 1 = off.
    pub limiter: f64,
}

impl Default for Modes {
    /// TODO(initiator): which switches start on (all safeties on, like a fresh ship).
    fn default() -> Self {
        Modes { coupled: true, coupling: 1.0, grav_comp: true, g_safe: true, comstab: true, proximity: true, landing: false, wind_comp: true, master: Master::Scm, limiter: 1.0 }
    }
}

impl Modes {
    /// Applies this step's taps and runs the coupling blend.
    pub fn update(&mut self, c: &ModeCmds, t: &ModeTuning, dt: f64) {
        let flip = |b: &mut bool, tap: bool| {
            if tap {
                *b = !*b;
            }
        };
        if c.decoupled {
            self.coupled = !self.coupled;
        }
        flip(&mut self.grav_comp, c.grav_comp);
        flip(&mut self.g_safe, c.g_safe);
        flip(&mut self.comstab, c.comstab);
        flip(&mut self.proximity, c.proximity);
        flip(&mut self.landing, c.landing);
        flip(&mut self.wind_comp, c.wind_comp);
        if c.master {
            self.master = match self.master {
                Master::Scm => Master::Nav,
                Master::Nav => Master::Scm,
            };
        }
        if c.limiter_steps != 0 {
            self.limiter = (self.limiter + c.limiter_steps as f64 * t.limiter_step).clamp(t.limiter_min, 1.0);
        }
        let target = if self.coupled { 1.0 } else { 0.0 };
        let step = if t.decouple_time > 0.0 { dt / t.decouple_time } else { 1.0 };
        self.coupling += (target - self.coupling).clamp(-step, step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_flip_and_the_blend_takes_decouple_time() {
        let t = ModeTuning::default();
        let mut m = Modes::default();
        m.update(&ModeCmds { decoupled: true, grav_comp: true, master: true, ..Default::default() }, &t, 0.0);
        assert!(!m.coupled && !m.grav_comp && m.master == Master::Nav);
        for _ in 0..120 {
            m.update(&ModeCmds::default(), &t, 1.0 / 60.0);
        }
        assert!((m.coupling - 0.5).abs() < 1e-9, "half way after 2 s of 4: {}", m.coupling);
    }

    #[test]
    fn limiter_steps_stay_in_range() {
        let t = ModeTuning::default();
        let mut m = Modes::default();
        m.update(&ModeCmds { limiter_steps: 3, ..Default::default() }, &t, 0.0);
        assert_eq!(m.limiter, 1.0);
        m.update(&ModeCmds { limiter_steps: -30, ..Default::default() }, &t, 0.0);
        assert_eq!(m.limiter, t.limiter_min);
    }
}
