//! The captain's console CP-01 (r41, #246 "The station in 0.1"): one screen in the corridor that shows the
//! captain the ship (hull, shield, the two rooms' air and alarms, the systems that are down, power, the jump
//! drive, the guns) and the threat word, and has three presses: the engine room's door (CLOSE / OPEN), the
//! airlock's SEAL, and SILENCE. The captain decides; the crew acts. The rows are `display_core::captain`.
//!
//! Sources: PULSAR's captain screen (`docs/research/pulsar-reference.md`, section "Captain": the status, the
//! alert level, the orders). Barotrauma's status monitor reads the same rows (`display_core::monitor`, ported
//! from MiniMap.cs; the monitor here is `status_monitor.rs`). The door is Barotrauma's Door.cs: the door's
//! `set_state` takes a signal and sets the door's state directly (Door.cs:846-850, `IsOpen` at 176-185), so
//! the console closes it without wiring; here the press sends `RoomAsk::SetDoor` to the host.
//!
//! Placement: the corridor's west wall at y -9.0, between JD-01 (y -7.0) and the corridor's south end (y -10.7).
//! TODO(initiator): the spot on the wall and the screen's size are start values.
use display_core::captain::{self, CaptainView, Threat};
use display_core::monitor::{self, Cell};
use display_core::{button, column, label, micro, readout, row, Item, Panel, PanelId, Tier, Tone};
use item_core::wire::{JumpStage, ReactorNews};

pub const ID: PanelId = PanelId::new("captain_console");
pub const NAME: &str = "CP-01";
/// The screen's wall spot (area metres) and size (m): a 640 x 400 canvas on the corridor's west wall, turned
/// to face east into the corridor.
pub const AT: [f64; 2] = [-14.96, -9.0];
pub const SIZE: [f64; 2] = [0.8, 0.5];
pub const TURN: f64 = 90.0;
/// The presses' actions (`reactor_screen::read_clicks`).
pub const DOOR_ACTION: &str = "door";
pub const SEAL_ACTION: &str = "seal";
pub const SILENCE_ACTION: &str = "silence";

/// A row's height (px) and the gap between the rows; the buttons are 24 px tall (the faults screen's rule).
const ROW_H: f64 = 18.0;
const GAP: f64 = 4.0;
const BUTTON_H: f64 = 24.0;

/// The captain's console as the reactor snapshot and the engine room's door have them. `door_open` is the
/// engine room's door (`RoomNews::door_open`), which the press flips.
pub fn captain_panel(r: Option<&ReactorNews>, door_open: bool) -> Panel {
    let Some(news) = r else {
        return Panel::screen(Tier::M, NAME, "CP-01", label("NO SIGNAL")).with_padding(8.0);
    };
    let v = captain::captain_view(news);
    let sounding = v.rooms.iter().any(|room| room.alarm.sounding);

    let left = vec![
        row(6.0, vec![label("HULL").flex(2.0), cell(&hull_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![label("SHIELD").flex(2.0), cell(&shield_cell(&v)), cell(&quantum_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![micro("ROOM").flex(2.0), micro("AIR").flex(1.0), micro("FIRE").flex(1.0), micro("ALARM").flex(1.0)]).fixed(ROW_H),
        room_row(&v.rooms[0]),
        room_row(&v.rooms[1]),
        row(6.0, vec![label("POWER").flex(2.0), cell(&power_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![label("JUMP").flex(2.0), cell(&jump_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![label("GUNS").flex(2.0), cell(&guns_cell(&v))]).fixed(ROW_H),
    ];

    let down = if v.systems_down.is_empty() { "NONE".to_string() } else { v.systems_down.join(", ") };
    let right = vec![
        micro("THREAT").fixed(14.0),
        readout(v.threat.word()).text_size(28.0).tone(v.threat.tone()).fixed(36.0),
        micro("SYSTEMS DOWN").fixed(14.0),
        readout(down).tone(if v.systems_down.is_empty() { Tone::Good } else { Tone::Bad }).fixed(ROW_H),
        row(6.0, vec![button(door_label(door_open), DOOR_ACTION).flex(1.0)]).fixed(BUTTON_H),
        row(6.0, vec![button("SEAL AIRLOCK", SEAL_ACTION).flex(1.0)]).fixed(BUTTON_H),
        row(6.0, vec![button("SILENCE", SILENCE_ACTION).tone(if sounding { Tone::Bad } else { Tone::Normal }).enabled(sounding).flex(1.0)]).fixed(BUTTON_H),
    ];

    let body = row(12.0, vec![column(GAP, left).flex(1.0), column(GAP, right).flex(1.0)]);
    Panel::screen(Tier::M, NAME, "CP-01", body).with_padding(8.0).status(v.threat.word(), v.threat.tone())
}

/// The door's press: while it is open the press shuts it, else it opens it.
pub fn door_label(door_open: bool) -> &'static str {
    if door_open {
        "CLOSE ENGINE DOOR"
    } else {
        "OPEN ENGINE DOOR"
    }
}

fn cell(c: &Cell) -> Item {
    readout(c.text.clone()).tone(c.tone).flex(1.0)
}

fn hull_cell(v: &CaptainView) -> Cell {
    let tone = if v.hull < captain::HULL_CRITICAL {
        Tone::Bad
    } else if v.hull < 1.0 {
        Tone::Warn
    } else {
        Tone::Good
    };
    Cell { text: format!("{:.0} %", v.hull * 100.0), tone }
}

fn shield_cell(v: &CaptainView) -> Cell {
    let tone = if v.shield < captain::SHIELD_DAMAGE { Tone::Warn } else { Tone::Good };
    Cell { text: format!("{:.0} %", v.shield * 100.0), tone }
}

fn quantum_cell(v: &CaptainView) -> Cell {
    if v.quantum_up {
        Cell { text: "QUANTUM UP".into(), tone: Tone::Good }
    } else {
        Cell { text: "QUANTUM DOWN".into(), tone: Tone::Warn }
    }
}

fn room_row(room: &captain::RoomView) -> Item {
    let air = monitor::o2_tone(room.air * 100.0);
    let fire = if room.fire { Cell { text: "YES".into(), tone: Tone::Bad } } else { Cell { text: "NO".into(), tone: Tone::Good } };
    row(6.0, vec![label(room.name).flex(2.0), readout(format!("{:.0} %", room.air * 100.0)).tone(air).flex(1.0), cell(&fire), cell(&room.alarm_cell())]).fixed(ROW_H)
}

fn power_cell(v: &CaptainView) -> Cell {
    let tone = if v.output_kw < v.demand_kw { Tone::Warn } else { Tone::Good };
    Cell { text: format!("{:.0} / {:.0} kW", v.output_kw, v.demand_kw), tone }
}

fn jump_cell(v: &CaptainView) -> Cell {
    let word = match v.jump_stage {
        JumpStage::ColdStart => "COLD START",
        JumpStage::Prepping => "PREPPING",
        JumpStage::Paused => "PAUSED",
        JumpStage::Ready => "READY",
        JumpStage::Active => "JUMPED",
    };
    let tone = if v.jump_stage == JumpStage::Ready { Tone::Good } else { Tone::Normal };
    Cell { text: format!("{word} {:.0} %", v.jump_charge.clamp(0.0, 1.0) * 100.0), tone }
}

fn guns_cell(v: &CaptainView) -> Cell {
    let tone = if v.weapon_shots == 0 { Tone::Warn } else { Tone::Good };
    Cell { text: format!("{} SHOTS", v.weapon_shots), tone }
}

/// The door screen's EMERGENCY word (#301): None while there is no threat, else the threat's word.
pub fn emergency(threat: Threat) -> Option<&'static str> {
    (threat != Threat::None).then(|| threat.word())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_door_press_says_what_it_does() {
        assert_eq!(door_label(true), "CLOSE ENGINE DOOR");
        assert_eq!(door_label(false), "OPEN ENGINE DOOR");
    }

    #[test]
    fn the_emergency_line_shows_only_a_threat() {
        assert_eq!(emergency(Threat::None), None);
        assert_eq!(emergency(Threat::Damage), Some("DAMAGE"));
        assert_eq!(emergency(Threat::Critical), Some("CRITICAL"));
    }
}
