//! The room alarms (r40, #165): one alarm per room for fire, breach and thin air, with a klaxon
//! and a lamp in the room and a SILENCE on the status monitor.
//!
//! Ported from Barotrauma's signal items: the smoke detector (`SmokeDetector.cs:7`,
//! `FireCheckInterval` 1 s) reports a fire in its hull; the oxygen detector (`OxygenDetector.cs:5`,
//! `LowOxygenPercentage` 35, sent as `low_oxygen` at `<=` at `:28`) reports thin air; a hull breach
//! counts once its wall leaks (`breach.rs`, Barotrauma `Structure.cs` LeakThreshold 0.1). The
//! alarm buzzer (`content/Items/Electricity/signalitems.xml:1725`, `alarmbuzzer`) loops its
//! `AlarmBuzzerLoop.ogg` with a blink of 2.6 Hz (`:1752`). There is no `alarmsiren` item in the
//! source we have. The smoke detector's FireSizeThreshold (50, `signalitems.xml:1418`) is not read
//! by this version's code, and its range (`max(DamageRange * 2, 500)`) is ours: a fire in the room.
//! We have no wiring: each detector feeds its room's alarm directly.
//!
//! Ours: the kinds by priority (FIRE, then BREACH, then LOW O2); the air's line has a band
//! (`low_o2` in, `low_o2_clear` out) so it does not flap; fire and breach stay on for `clear_s`
//! after their input goes off (the detector's one-second sampling, as a hold). SILENCE mutes the
//! sound until the kind changes or `silence_s` pass; the lamp keeps blinking. All numbers are
//! `hull/hull.json` `alarm`, TODO(initiator).
use serde::{Deserialize, Serialize};

/// The kind of a room's alarm, by priority: a fire over a breach over thin air.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    #[default]
    Clear,
    Fire,
    Breach,
    LowO2,
}

/// What the alarm shows: its kind, whether the klaxon sounds, and whether SILENCE holds it quiet.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct AlarmState {
    pub kind: Kind,
    pub sounding: bool,
    pub silenced: bool,
}

/// The alarm's numbers (`hull/hull.json` `alarm`).
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AlarmTuning {
    /// The air (0..1) at or below which the room has thin air (Barotrauma's oxygen detector, 35 %).
    pub low_o2: f64,
    /// The air above which the thin-air alarm clears again (the band's top).
    pub low_o2_clear: f64,
    /// Seconds a fire or a breach stays on after its input goes off.
    pub clear_s: f64,
    /// Seconds a SILENCE holds the sound when the kind does not change.
    pub silence_s: f64,
}

impl Default for AlarmTuning {
    fn default() -> AlarmTuning {
        AlarmTuning { low_o2: 0.35, low_o2_clear: 0.40, clear_s: 1.0, silence_s: 60.0 }
    }
}

/// One room's alarm. The host steps it in `reactor_step`; the clients take its news.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RoomAlarm {
    tuning: AlarmTuning,
    /// The fire and breach latches: seconds left on each.
    fire_left: f64,
    breach_left: f64,
    /// The thin-air latch: on at or below `low_o2`, off above `low_o2_clear`.
    low_o2: bool,
    kind: Kind,
    /// The kind a SILENCE was pressed in, and the seconds it still holds.
    silenced_kind: Option<Kind>,
    silence_left: f64,
}

impl RoomAlarm {
    pub fn new(tuning: AlarmTuning) -> RoomAlarm {
        RoomAlarm { tuning, fire_left: 0.0, breach_left: 0.0, low_o2: false, kind: Kind::Clear, silenced_kind: None, silence_left: 0.0 }
    }

    /// One step: the room's fire, its breach (the leak is open) and its air (0..1), `dt` seconds on.
    pub fn step(&mut self, fire: bool, breach: bool, o2: f64, dt: f64) -> AlarmState {
        let t = self.tuning;
        self.fire_left = hold(fire, self.fire_left, dt, t.clear_s);
        self.breach_left = hold(breach, self.breach_left, dt, t.clear_s);
        if o2 <= t.low_o2 {
            self.low_o2 = true;
        } else if o2 > t.low_o2_clear {
            self.low_o2 = false;
        }
        self.kind = if self.fire_left > 0.0 {
            Kind::Fire
        } else if self.breach_left > 0.0 {
            Kind::Breach
        } else if self.low_o2 {
            Kind::LowO2
        } else {
            Kind::Clear
        };
        // A new kind (or no alarm at all) re-arms the klaxon; the same kind is held for silence_s.
        if self.silenced_kind != Some(self.kind) {
            self.silenced_kind = None;
        }
        if self.silenced_kind.is_some() {
            self.silence_left -= dt;
            if self.silence_left <= 0.0 {
                self.silenced_kind = None;
            }
        }
        self.state()
    }

    /// SILENCE: mutes the sound for the current kind (no effect with no alarm).
    pub fn silence(&mut self) {
        if self.kind != Kind::Clear {
            self.silenced_kind = Some(self.kind);
            self.silence_left = self.tuning.silence_s;
        }
    }

    pub fn state(&self) -> AlarmState {
        let silenced = self.silenced_kind.is_some();
        AlarmState { kind: self.kind, sounding: self.kind != Kind::Clear && !silenced, silenced }
    }
}

/// A latch: on for `clear_s` after the input was last on, and the seconds left on it.
fn hold(on: bool, left: f64, dt: f64, clear_s: f64) -> f64 {
    if on {
        clear_s
    } else {
        (left - dt).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 0.1;

    fn alarm() -> RoomAlarm {
        RoomAlarm::new(AlarmTuning::default())
    }

    /// Steps `secs` seconds with the same inputs.
    fn run(a: &mut RoomAlarm, fire: bool, breach: bool, o2: f64, secs: f64) -> AlarmState {
        let mut s = a.state();
        for _ in 0..(secs / DT).round() as usize {
            s = a.step(fire, breach, o2, DT);
        }
        s
    }

    #[test]
    fn each_kind_triggers_on_its_own_input() {
        assert_eq!(run(&mut alarm(), false, false, 0.9, 0.2).kind, Kind::Clear, "a healthy room is quiet");
        assert_eq!(run(&mut alarm(), true, false, 0.9, 0.2).kind, Kind::Fire);
        assert_eq!(run(&mut alarm(), false, true, 0.9, 0.2).kind, Kind::Breach);
        assert_eq!(run(&mut alarm(), false, false, 0.30, 0.2).kind, Kind::LowO2);
    }

    #[test]
    fn the_thin_air_line_is_barotrauma_35_percent_at_or_below() {
        assert_eq!(run(&mut alarm(), false, false, 0.35, 0.2).kind, Kind::LowO2, "at 35 % it is on");
        assert_eq!(run(&mut alarm(), false, false, 0.36, 0.2).kind, Kind::Clear, "just above it is not");
    }

    #[test]
    fn the_kinds_keep_their_priority() {
        assert_eq!(run(&mut alarm(), true, true, 0.1, 0.2).kind, Kind::Fire, "a fire over a breach and thin air");
        assert_eq!(run(&mut alarm(), false, true, 0.1, 0.2).kind, Kind::Breach, "a breach over thin air");
    }

    #[test]
    fn the_thin_air_band_does_not_flap() {
        let mut a = alarm();
        run(&mut a, false, false, 0.30, 0.2);
        for o2 in [0.37, 0.38, 0.40] {
            assert_eq!(run(&mut a, false, false, o2, 0.5).kind, Kind::LowO2, "{o2} is inside the band: it stays on");
        }
        assert_eq!(run(&mut a, false, false, 0.41, 0.2).kind, Kind::Clear, "above the band it clears");
        // Back inside the band from above: stays clear.
        assert_eq!(run(&mut a, false, false, 0.37, 0.5).kind, Kind::Clear);
    }

    #[test]
    fn a_fire_that_blinks_off_for_a_moment_stays_one_alarm() {
        let mut a = alarm();
        run(&mut a, true, false, 0.9, 0.2);
        let gap = run(&mut a, false, false, 0.9, 0.5);
        assert_eq!(gap.kind, Kind::Fire, "half a second without fire is inside the hold");
        assert_eq!(run(&mut a, true, false, 0.9, 0.2).kind, Kind::Fire);
        assert_eq!(run(&mut a, false, false, 0.9, 1.5).kind, Kind::Clear, "after clear_s with no fire it clears");
    }

    #[test]
    fn silence_mutes_the_sound_and_the_kind_stays() {
        let mut a = alarm();
        assert!(run(&mut a, true, false, 0.9, 0.2).sounding);
        a.silence();
        let s = a.state();
        assert_eq!(s.kind, Kind::Fire, "the lamp keeps the kind");
        assert!(!s.sounding && s.silenced);
    }

    #[test]
    fn silence_holds_for_silence_s_then_rearms() {
        let mut a = alarm();
        run(&mut a, true, false, 0.9, 0.2);
        a.silence();
        let held = run(&mut a, true, false, 0.9, 59.0);
        assert!(!held.sounding, "still silent at 59 s");
        let back = run(&mut a, true, false, 0.9, 1.5);
        assert!(back.sounding && !back.silenced, "the sound comes back after silence_s");
    }

    #[test]
    fn a_new_kind_rearms_the_sound() {
        let mut a = alarm();
        run(&mut a, true, false, 0.9, 0.2);
        a.silence();
        let under = run(&mut a, true, true, 0.9, 0.2);
        assert_eq!(under.kind, Kind::Fire, "the fire outranks the breach while it burns");
        assert!(!under.sounding, "so the silenced fire stays silent");
        let s = run(&mut a, false, true, 0.9, 1.5);
        assert_eq!(s.kind, Kind::Breach, "the fire is out and the breach is left");
        assert!(s.sounding && !s.silenced, "a new kind sounds again");
    }

    #[test]
    fn a_clear_room_and_a_return_of_the_same_kind_rearm_too() {
        let mut a = alarm();
        run(&mut a, true, false, 0.9, 0.2);
        a.silence();
        assert_eq!(run(&mut a, false, false, 0.9, 1.5).kind, Kind::Clear);
        let again = run(&mut a, true, false, 0.9, 0.2);
        assert!(again.sounding, "the fire came back after a clear: it sounds");
    }

    #[test]
    fn silence_with_no_alarm_does_nothing() {
        let mut a = alarm();
        run(&mut a, false, false, 0.9, 0.2);
        a.silence();
        let s = a.state();
        assert_eq!(s.kind, Kind::Clear);
        assert!(!s.sounding && !s.silenced);
        assert!(run(&mut a, true, false, 0.9, 0.2).sounding, "a fire after that sounds");
    }
}
