//! The room lights' brightness: a port of Barotrauma's light update, and our emergency lamps.
//!
//! Source: `BarotraumaShared/SharedSource/Items/Components/Signal/LightComponent.cs:392-410`.
//! Each update there is a 5 % chance that, if `Voltage < Rand(0, MinVoltage)`, the light is off for
//! that frame (a flicker; the zap sound plays when `Voltage > 0.1`). Otherwise brightness lerps
//! toward `min(Voltage, 1)` by 0.1. `MinVoltage` is 0.5 by default (`Powered.cs:122-128`).
//! Barotrauma runs this at 60 Hz; here the chance and the lerp are per 1/60 s of `dt`, so the frame
//! rate does not change the look. Not ported: its `powerConsumption <= 0` case (we always draw) and
//! the flicker timer.
//!
//! Ours: emergency lamps. Below `EMERGENCY_BELOW` of voltage the room lights are out and the red
//! emergency lamps come on. The lamps are fed by the backup, not by the grid (the alarm lights work
//! the same way). The threshold is a `TODO(initiator)`.

pub const MIN_VOLTAGE: f64 = 0.5;
/// Ours: the lights factor below which the emergency lamps come on. `TODO(initiator)`.
pub const EMERGENCY_BELOW: f64 = 0.1;
/// Barotrauma's zap sound on a flicker only above this voltage (`LightComponent.cs`, client block).
pub const ZAP_ABOVE: f64 = 0.1;
const HZ: f64 = 60.0;
const FLICKER_CHANCE_PER_UPDATE: f64 = 0.05;
const LERP_PER_UPDATE: f64 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightState {
    brightness: f64,
    flickered: bool,
}

impl LightState {
    pub fn brightness(&self) -> f64 {
        self.brightness
    }

    /// The last step was a flicker (the caller plays the zap sound when the voltage is above 0.1).
    pub fn flickered(&self) -> bool {
        self.flickered
    }

    /// The emergency lamps are on: the lights factor is below `EMERGENCY_BELOW`.
    pub fn emergency(voltage: f64) -> bool {
        voltage < EMERGENCY_BELOW
    }

    /// One update of `dt` seconds at `voltage` (the grid's factor for the lights, 0..1).
    /// `rng_roll` is one uniform draw in [0, 1). Returns the brightness, 0..1.
    pub fn step(&mut self, dt: f64, voltage: f64, rng_roll: f64) -> f64 {
        let frames = dt.max(0.0) * HZ;
        let flicker_chance = 1.0 - (1.0 - FLICKER_CHANCE_PER_UPDATE).powf(frames);
        self.flickered = false;
        // Barotrauma draws twice: the 5 % chance, then Rand(0, MinVoltage). Given roll < chance,
        // roll / chance is uniform in [0, 1) and independent of that event, so one draw does both.
        if rng_roll < flicker_chance && voltage < MIN_VOLTAGE * (rng_roll / flicker_chance) {
            self.flickered = true;
            self.brightness = 0.0;
            return 0.0;
        }
        let target = voltage.clamp(0.0, 1.0);
        let lerp = 1.0 - (1.0 - LERP_PER_UPDATE).powf(frames);
        self.brightness += (target - self.brightness) * lerp;
        self.brightness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small seeded generator for the test: uniform draws in [0, 1).
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    const FRAME: f64 = 1.0 / 60.0;

    #[test]
    fn full_voltage_converges_to_full_brightness() {
        let mut light = LightState::default();
        let mut rng = Rng(7);
        for _ in 0..600 {
            light.step(FRAME, 1.0, rng.next());
        }
        assert!(light.brightness() > 0.999, "brightness {}", light.brightness());
    }

    #[test]
    fn low_voltage_flickers_and_its_mean_stays_below_the_voltage() {
        let mut light = LightState::default();
        let mut rng = Rng(42);
        let (mut sum, mut flickers, n) = (0.0, 0, 20_000);
        for i in 0..n {
            let b = light.step(FRAME, 0.3, rng.next());
            if i > 600 {
                sum += b;
                if light.flickered() {
                    flickers += 1;
                }
            }
        }
        let mean = sum / (n - 601) as f64;
        assert!(flickers > 0, "a voltage of 0.3 flickers sometimes");
        assert!(mean < 0.3 + 1e-3, "the flicker pulls the mean down, got {mean}");
    }

    #[test]
    fn zero_voltage_is_dark_with_emergency_on() {
        let mut light = LightState::default();
        let mut rng = Rng(3);
        for _ in 0..300 {
            light.step(FRAME, 0.0, rng.next());
        }
        assert!(light.brightness() < 1e-6, "brightness {}", light.brightness());
        assert!(LightState::emergency(0.0));
        assert!(LightState::emergency(0.05));
        assert!(!LightState::emergency(0.3), "a voltage of 0.3 is no emergency");
    }

    #[test]
    fn the_lerp_does_not_depend_on_the_frame_rate() {
        let mut once = LightState::default();
        let mut twice = LightState::default();
        // A roll of 0.99 is above every flicker chance here, so only the lerp acts.
        once.step(FRAME, 0.8, 0.99);
        twice.step(FRAME / 2.0, 0.8, 0.99);
        twice.step(FRAME / 2.0, 0.8, 0.99);
        assert!((once.brightness() - twice.brightness()).abs() < 1e-12, "{} vs {}", once.brightness(), twice.brightness());
    }
}
