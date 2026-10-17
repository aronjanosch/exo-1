//! The captain's console CP-01 (r41, #246 "The station in 0.1"): what the captain sees of the ship, as
//! plain numbers in (`ReactorNews`), the view and its threat word out. The glue
//! (`exo_app/src/dev_area/captain_screen.rs`) draws it and reads the buttons; the engine room's door
//! screen shows the threat word as its EMERGENCY line (#301).
//!
//! What the captain sees follows PULSAR's captain screen (`docs/research/pulsar/pulsar-reference.md`, section
//! "Captain"): the ship's status, the shield, the systems, and the alert level (green, yellow, red),
//! which is the threat word here (NONE, DAMAGE, CRITICAL). The rows reuse Barotrauma's status monitor
//! (`monitor`, ported from MiniMap.cs 1165-1171 and 1228-1229): a room's air and alarm, a device's
//! condition. The hull is one minus the mean of the wall sections, as the status monitor's breach
//! line reads them (`ReactorNews::breaches`, r37/r40).
//!
//! TODO(initiator): every threshold below is a start value; the feel of the word is the initiator's.
use crate::monitor;
use crate::Tone;
use item_core::wire::{AlarmNews, JumpStage, PartNews, ReactorNews, RoomAlarmKind};

/// The hull below this share (0..1) is CRITICAL. TODO(initiator): start value, half the hull.
pub const HULL_CRITICAL: f64 = 0.5;
/// A room's air below this share (0..1) is CRITICAL: the same line as `monitor::o2_tone`'s bad.
pub const AIR_CRITICAL: f64 = 0.3;
/// The shield below this share (0..1) is DAMAGE. TODO(initiator): start value.
pub const SHIELD_DAMAGE: f64 = 0.5;

/// The room rows, in the order `ReactorNews::rooms` and `ReactorNews::alarms` have them.
pub const ROOM_NAMES: [&str; 3] = ["ENGINE ROOM", "CORRIDOR", "BRIDGE"];
/// The devices, in the order the captain's list names them; `PartNews.working == false` is DOWN.
pub const SYSTEM_NAMES: [&str; 4] = ["REACTOR", "PUMP", "JUNCTION", "O2 GEN"];

/// The alert level the captain reads: NONE, DAMAGE, CRITICAL (PULSAR's green, yellow, red).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Threat {
    #[default]
    None,
    Damage,
    Critical,
}

impl Threat {
    pub fn word(self) -> &'static str {
        match self {
            Threat::None => "NONE",
            Threat::Damage => "DAMAGE",
            Threat::Critical => "CRITICAL",
        }
    }

    /// The word's colour role: good with no threat, warn with damage, bad when critical.
    pub fn tone(self) -> Tone {
        match self {
            Threat::None => Tone::Good,
            Threat::Damage => Tone::Warn,
            Threat::Critical => Tone::Bad,
        }
    }
}

/// A room as the captain sees it: its name, its air (0..1), its fire, its alarm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomView {
    pub name: &'static str,
    pub air: f64,
    pub fire: bool,
    pub alarm: AlarmNews,
}

impl RoomView {
    /// The room's alarm word (the same words as ST-01's alarm column).
    pub fn alarm_word(&self) -> Option<&'static str> {
        match self.alarm.kind {
            RoomAlarmKind::Clear => None,
            RoomAlarmKind::Fire => Some("FIRE"),
            RoomAlarmKind::Breach => Some("BREACH"),
            RoomAlarmKind::LowO2 => Some("O2 LOW"),
        }
    }

    /// The room's alarm cell (`monitor::alarm_cell`): OK, the word in bad, or in warn once silenced.
    pub fn alarm_cell(&self) -> monitor::Cell {
        monitor::alarm_cell(self.alarm_word(), self.alarm.silenced)
    }
}

/// The captain's whole view of the ship. Shares are 0..1; power and load are kW.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptainView {
    /// 1 minus the mean of the wall sections' damage: 1 is a whole hull.
    pub hull: f64,
    pub shield: f64,
    pub quantum_up: bool,
    pub rooms: [RoomView; 3],
    /// The devices below their broken limit, by `SYSTEM_NAMES`.
    pub systems_down: Vec<&'static str>,
    /// The reactor's output and the main load (the demand the grid asks of the reactor).
    pub output_kw: f64,
    pub demand_kw: f64,
    pub jump_stage: JumpStage,
    pub jump_charge: f64,
    pub weapon_shots: u8,
    pub threat: Threat,
}

/// The hull integrity from the wall sections' damage (each 0..1): 1 minus their mean. No sections: whole.
pub fn hull_integrity(breaches: &[f64]) -> f64 {
    if breaches.is_empty() {
        return 1.0;
    }
    1.0 - breaches.iter().sum::<f64>() / breaches.len() as f64
}

/// The threat word from the view's numbers. CRITICAL: the hull under `HULL_CRITICAL`, a room's air under
/// `AIR_CRITICAL`, or a fire. DAMAGE: a hull breach, the shield under `SHIELD_DAMAGE`, a device down, or an
/// alarm sounding. Otherwise NONE.
pub fn threat(hull: f64, shield: f64, rooms: &[RoomView], any_down: bool) -> Threat {
    if hull < HULL_CRITICAL || rooms.iter().any(|r| r.fire || r.air < AIR_CRITICAL) {
        return Threat::Critical;
    }
    if hull < 1.0 || shield < SHIELD_DAMAGE || any_down || rooms.iter().any(|r| r.alarm.sounding) {
        return Threat::Damage;
    }
    Threat::None
}

/// The captain's view of the reactor snapshot.
pub fn captain_view(r: &ReactorNews) -> CaptainView {
    let hull = hull_integrity(&r.breaches);
    let rooms = [0, 1, 2].map(|i| RoomView { name: ROOM_NAMES[i], air: r.rooms[i].o2, fire: r.rooms[i].fire, alarm: r.alarms[i] });
    let parts: [PartNews; 4] = [r.reactor, r.pump, r.junction, r.oxygen_generator];
    let systems_down: Vec<&'static str> = parts.iter().zip(SYSTEM_NAMES).filter(|(p, _)| !p.working).map(|(_, name)| name).collect();
    let threat = threat(hull, r.shield, &rooms, !systems_down.is_empty());
    CaptainView {
        hull,
        shield: r.shield,
        quantum_up: r.quantum_shield,
        rooms,
        systems_down,
        output_kw: r.output_kw,
        demand_kw: r.load_kw,
        jump_stage: r.jump_stage,
        jump_charge: r.jump_charge,
        weapon_shots: r.weapon_shots,
        threat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ship with nothing wrong: whole hull, full shield, every device working, air full, no alarms.
    fn healthy() -> ReactorNews {
        let part = PartNews { share: 1.0, working: true };
        let mut r = ReactorNews { reactor: part, pump: part, junction: part, oxygen_generator: part, shield: 1.0, ..ReactorNews::default() };
        for room in r.rooms.iter_mut() {
            room.o2 = 1.0;
        }
        r
    }

    #[test]
    fn the_hull_is_one_minus_the_mean_of_the_wall_sections() {
        assert_eq!(hull_integrity(&[0.0; 8]), 1.0);
        assert_eq!(hull_integrity(&[1.0; 8]), 0.0);
        assert_eq!(hull_integrity(&[0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]), 0.9375, "one half section: 1 - 0.5 / 8");
        assert_eq!(hull_integrity(&[]), 1.0, "no sections: whole");
        let mut r = healthy();
        r.breaches = [0.2; 8];
        assert!((captain_view(&r).hull - 0.8).abs() < 1e-12, "the view carries the hull");
    }

    #[test]
    fn the_threat_word_follows_the_hull_the_air_the_fire_and_the_systems() {
        let ok = healthy();
        assert_eq!(captain_view(&ok).threat, Threat::None, "a sound ship is NONE");
        assert_eq!(captain_view(&ok).threat.word(), "NONE");

        let mut breach = healthy();
        breach.breaches = [0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(captain_view(&breach).threat, Threat::Damage, "a breach is DAMAGE");

        let mut half = healthy();
        half.breaches = [0.5; 8];
        assert_eq!(captain_view(&half).threat, Threat::Damage, "a hull at exactly the line is DAMAGE, not CRITICAL");

        let mut holed = healthy();
        holed.breaches = [0.6; 8];
        assert_eq!(captain_view(&holed).threat, Threat::Critical, "a hull under half is CRITICAL");
        assert_eq!(captain_view(&holed).threat.word(), "CRITICAL");

        let mut thin = healthy();
        thin.rooms[1].o2 = 0.25;
        assert_eq!(captain_view(&thin).threat, Threat::Critical, "air under 30 % is CRITICAL");

        let mut burning = healthy();
        burning.rooms[0].fire = true;
        assert_eq!(captain_view(&burning).threat, Threat::Critical, "a fire is CRITICAL");

        let mut down = healthy();
        down.pump.working = false;
        assert_eq!(captain_view(&down).threat, Threat::Damage, "a device down is DAMAGE");
        assert_eq!(captain_view(&down).systems_down, vec!["PUMP"]);

        let mut weak = healthy();
        weak.shield = 0.3;
        assert_eq!(captain_view(&weak).threat, Threat::Damage, "a shield under half is DAMAGE");

        let mut alarm = healthy();
        alarm.alarms[0] = AlarmNews { kind: RoomAlarmKind::Breach, sounding: true, silenced: false };
        assert_eq!(captain_view(&alarm).threat, Threat::Damage, "a sounding alarm is DAMAGE");
    }

    #[test]
    fn a_room_reads_its_alarm_word_and_its_cell_follows_silence() {
        let mut r = healthy();
        r.alarms[1] = AlarmNews { kind: RoomAlarmKind::LowO2, sounding: true, silenced: true };
        let v = captain_view(&r);
        assert_eq!(v.rooms[1].alarm_word(), Some("O2 LOW"));
        assert_eq!(v.rooms[1].alarm_cell(), monitor::alarm_cell(Some("O2 LOW"), true));
        assert_eq!(v.rooms[0].alarm_word(), None);
    }
}
