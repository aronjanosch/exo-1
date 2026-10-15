//! The captain's console CP-01 (r41, #246 "The station in 0.1"): one screen on the bridge's south wall that
//! shows the captain the ship (hull, shield, the three rooms' air and alarms, the systems that are down,
//! power, the jump drive, the guns) and the threat word, and has four presses: the engine room's door and
//! the bridge door (CLOSE / OPEN), the airlock's SEAL, and SILENCE. The captain decides; the crew acts. The rows are `display_core::captain`.
//!
//! Sources: PULSAR's captain screen (`docs/research/pulsar-reference.md`, section "Captain": the status, the
//! alert level, the orders). Barotrauma's status monitor reads the same rows (`display_core::monitor`, ported
//! from MiniMap.cs; the monitor here is `status_monitor.rs`). The door is Barotrauma's Door.cs: the door's
//! `set_state` takes a signal and sets the door's state directly (Door.cs:846-850, `IsOpen` at 176-185), so
//! the console closes it without wiring; here the press sends `RoomAsk::SetDoor` to the host.
//!
//! Placement (r44): the bridge's south wall, beside GN-01, facing north into the bridge. TODO(initiator): the
//! spot on the wall and the screen's size are start values. The presses (r44): the engine door, the bridge
//! door (`RoomAsk::SetBridgeDoor`), the airlock's SEAL and SILENCE.
use display_core::captain::{self, CaptainView, Threat};
use display_core::monitor::{self, Cell};
use display_core::{button, column, label, micro, readout, row, Item, Panel, PanelId, Tier, Tone};
use item_core::wire::{BotStation, JumpStage, ReactorNews};

pub const ID: PanelId = PanelId::new("captain_console");
pub const NAME: &str = "CP-01";
/// The screen's wall spot (area metres) and size (m): a 640 x 400 canvas on the bridge's south wall, turned
/// to face north into the bridge (0.04 m inside the wall's face at y -10.7).
pub const AT: [f64; 2] = [-6.5, -10.66];
pub const SIZE: [f64; 2] = [0.8, 0.5];
pub const TURN: f64 = 180.0;
/// The presses' actions (`crate::dev_area::engineering::panels::read_clicks`).
pub const DOOR_ACTION: &str = "door";
pub const BRIDGE_DOOR_ACTION: &str = "bridge_door";
pub const SEAL_ACTION: &str = "seal";
pub const SILENCE_ACTION: &str = "silence";
/// The bot priority buttons' prefix: "bot:<station index>:<level>" (r42, #253).
const BOT_PREFIX: &str = "bot:";
/// The priority levels' labels, Ignore .. Max (PULSAR's five nodes).
pub const LEVEL_WORDS: [&str; 5] = ["IGN", "LOW", "MED", "HIGH", "MAX"];

/// A row's height (px) and the gap between the rows; the buttons are 24 px tall (the faults screen's rule).
const ROW_H: f64 = 18.0;
const GAP: f64 = 4.0;
const BUTTON_H: f64 = 24.0;

/// The captain's console as the reactor snapshot and the engine room's door have them. `door_open` is the
/// engine room's door (`RoomNews::door_open`), which the press flips; `bridge_door` the bridge door's (r44).
pub fn captain_panel(r: Option<&ReactorNews>, door_open: bool, bridge_door: bool) -> Panel {
    let Some(news) = r else {
        return Panel::screen(Tier::M, NAME, "CP-01", label("NO SIGNAL")).with_padding(8.0);
    };
    let v = captain::captain_view(news);
    let sounding = v.rooms.iter().any(|room| room.alarm.sounding);

    let mut left = vec![
        row(6.0, vec![label("HULL").flex(2.0), cell(&hull_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![label("SHIELD").flex(2.0), cell(&shield_cell(&v)), cell(&quantum_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![micro("ROOM").flex(2.0), micro("AIR").flex(1.0), micro("FIRE").flex(1.0), micro("ALARM").flex(1.0)]).fixed(ROW_H),
        room_row(&v.rooms[0]),
        room_row(&v.rooms[1]),
        room_row(&v.rooms[2]),
        row(6.0, vec![label("POWER").flex(2.0), cell(&power_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![label("JUMP").flex(2.0), cell(&jump_cell(&v))]).fixed(ROW_H),
        row(6.0, vec![label("GUNS").flex(2.0), cell(&guns_cell(&v))]).fixed(ROW_H),
    ];
    // The bots' priority list (r42, #253): one row per station, five level buttons, the set one lit.
    left.push(micro("BOT PRIORITY").fixed(14.0));
    for s in BotStation::ALL {
        let level = news.bots[s.index()].level;
        let mut cells = vec![label(station_word(s)).flex(2.0)];
        cells.extend(LEVEL_WORDS.iter().enumerate().map(|(l, word)| {
            let set = usize::from(level) == l;
            button(*word, bot_action(s, l as u8)).tone(if set { Tone::Good } else { Tone::Normal }).flex(1.0)
        }));
        left.push(row(4.0, cells).fixed(BUTTON_H));
    }

    let down = if v.systems_down.is_empty() { "NONE".to_string() } else { v.systems_down.join(", ") };
    let right = vec![
        micro("THREAT").fixed(14.0),
        readout(v.threat.word()).text_size(28.0).tone(v.threat.tone()).fixed(36.0),
        micro("SYSTEMS DOWN").fixed(14.0),
        readout(down).tone(if v.systems_down.is_empty() { Tone::Good } else { Tone::Bad }).fixed(ROW_H),
        row(6.0, vec![button(door_label(door_open), DOOR_ACTION).flex(1.0)]).fixed(BUTTON_H),
        row(6.0, vec![button(bridge_door_label(bridge_door), BRIDGE_DOOR_ACTION).flex(1.0)]).fixed(BUTTON_H),
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

/// The bridge door's press (r44): while it is open the press shuts it, else it opens it.
pub fn bridge_door_label(open: bool) -> &'static str {
    if open {
        "CLOSE BRIDGE DOOR"
    } else {
        "OPEN BRIDGE DOOR"
    }
}

pub fn station_word(s: BotStation) -> &'static str {
    match s {
        BotStation::Gunnery => "GUNNERY",
        BotStation::Reactor => "REACTOR",
        BotStation::Power => "POWER",
        BotStation::Helm => "HELM",
        BotStation::Fire => "FIRE",
        BotStation::Repair => "REPAIR",
        BotStation::Breach => "BREACH",
    }
}

/// The action of a priority button: "bot:<station index>:<level>".
pub fn bot_action(s: BotStation, level: u8) -> String {
    format!("{BOT_PREFIX}{}:{level}", s.index())
}

/// The station and level a priority button's action names, None for any other action.
pub fn parse_bot_action(action: &str) -> Option<(BotStation, u8)> {
    let (i, level) = action.strip_prefix(BOT_PREFIX)?.split_once(':')?;
    let s = *BotStation::ALL.get(i.parse::<usize>().ok()?)?;
    Some((s, level.parse::<u8>().ok()?))
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
    fn a_priority_button_names_its_station_and_level_and_back() {
        for s in BotStation::ALL {
            for l in 0..5u8 {
                assert_eq!(parse_bot_action(&bot_action(s, l)), Some((s, l)));
            }
        }
        assert_eq!(parse_bot_action(SEAL_ACTION), None);
        assert_eq!(parse_bot_action("bot:9:1"), None);
    }

    #[test]
    fn the_door_press_says_what_it_does() {
        assert_eq!(door_label(true), "CLOSE ENGINE DOOR");
        assert_eq!(door_label(false), "OPEN ENGINE DOOR");
    }

    #[test]
    fn the_bridge_door_press_says_what_it_does() {
        assert_eq!(bridge_door_label(true), "CLOSE BRIDGE DOOR");
        assert_eq!(bridge_door_label(false), "OPEN BRIDGE DOOR");
    }

    #[test]
    fn the_emergency_line_shows_only_a_threat() {
        assert_eq!(emergency(Threat::None), None);
        assert_eq!(emergency(Threat::Damage), Some("DAMAGE"));
        assert_eq!(emergency(Threat::Critical), Some("CRITICAL"));
    }
}
