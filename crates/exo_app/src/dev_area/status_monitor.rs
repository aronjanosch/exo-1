//! The status monitor ST-01 (r38): one screen on the engine room's east wall that shows, at a
//! glance, where the trouble is while the others work. Its rows are the engine room and the
//! corridor (air, fire, and the engine room's breach), the four devices (condition), and the three
//! grid groups (the share of their demand they get).
//!
//! Ported from Barotrauma's MiniMap, the status monitor's hull status and electrical view
//! (`BarotraumaClient/ClientSource/Items/Components/Machines/MiniMap.cs`, see the row model in
//! `display_core::monitor` for the lines). Not ported: the water (no water here), the item finder,
//! the blips and the search. The room rows carry each room's alarm word (r40, #165: FIRE, BREACH, O2 LOW,
//! from `ReactorNews::alarms`) and a SILENCE button mutes the sounding alarms (`ReactorAsk::SilenceAlarms`,
//! read in `reactor_screen::read_clicks`). The screen reads the reactor snapshot, which carries the rooms'
//! air, fire and alarms for clients too.
//! Placement: the east wall's inner face (x -9.0) at the reactor readout's end of the room, clear
//! of the coolant pump's screen and the oxygen generator (`engine_room.rs`).
//! TODO(initiator): the screen's name, its tier and the row order are start values.
use display_core::monitor::{self, Cell, RoomLine};
use display_core::{button, column, label, micro, readout, row, space, Item, Panel, PanelId, Tier, Tone};
use item_core::wire::{AlarmNews, BreakerGroup, PartNews, ReactorNews, RoomAlarmKind};
use reactor_core::Breach;

pub const ID: PanelId = PanelId::new("status_monitor");
pub const NAME: &str = "ST-01";
/// The screen's spot (area metres) and lift (m): the readout's size, on the east wall beside the
/// reactor screens, turned to face west into the room.
pub const AT: [f64; 2] = [-9.04, -16.9];
pub const LIFT: f64 = 0.9;
pub const SIZE: [f64; 2] = [1.6, 1.0];
pub const TURN: f64 = -90.0;
/// The SILENCE button's action (`reactor_screen::read_clicks`).
pub const SILENCE_ACTION: &str = "silence";

/// A row's height (px) and the gap between the rows.
const ROW_H: f64 = 18.0;
const GAP: f64 = 3.0;

/// The monitor as the reactor snapshot has it. Without a snapshot every reading is a dash.
pub fn monitor_panel(r: Option<&ReactorNews>) -> Panel {
    let alarms = r.map_or([AlarmNews::default(); 2], |r| r.alarms);
    let room_lines = [
        RoomLine { alarm: alarm_cell(alarms[0]), ..monitor::room_line(ROOM_NAME, r.map(|r| r.rooms[0].o2 * 100.0), r.is_some_and(|r| r.rooms[0].fire), Some(r.is_some_and(|r| breach_open(r.breach)))) },
        RoomLine { alarm: alarm_cell(alarms[1]), ..monitor::room_line(CORRIDOR_NAME, r.map(|r| r.rooms[1].o2 * 100.0), r.is_some_and(|r| r.rooms[1].fire), None) },
    ];
    let devices = [
        ("REACTOR", r.map(|r| r.reactor)),
        ("PUMP", r.map(|r| r.pump)),
        ("JUNCTION", r.map(|r| r.junction)),
        ("O2 GEN", r.map(|r| r.oxygen_generator)),
    ];
    let groups = [
        ("ENGINES", BreakerGroup::Engines),
        ("LIFE SUPP", BreakerGroup::LifeSupport),
        ("LIGHTS", BreakerGroup::Lights),
        ("SHIELDS", BreakerGroup::Shields),
        ("WEAPONS", BreakerGroup::Weapons),
        ("JUMP", BreakerGroup::JumpDrive),
    ];

    let mut lines = vec![head(&["ROOM", "AIR", "FIRE", "BREACH", "ALARM"])];
    lines.extend(room_rows(&room_lines));
    lines.push(silence_row(&alarms));
    lines.push(space().fixed(GAP));
    lines.push(head(&["DEVICE", "CONDITION"]));
    lines.extend(devices.iter().map(|(name, part)| device_row(name, part.as_ref())));
    lines.push(space().fixed(GAP));
    lines.push(head(&["GRID", "SUPPLY"]));
    lines.extend(groups.iter().map(|(name, g)| group_row(name, r, *g)));
    Panel::screen(Tier::M, NAME, "ST-01", column(GAP, lines)).with_padding(8.0)
}

const ROOM_NAME: &str = "ENGINE ROOM";
const CORRIDOR_NAME: &str = "CORRIDOR";

/// The column heads: the first takes the name's width, the rest share the cells'.
fn head(words: &[&str]) -> Item {
    row(6.0, words.iter().enumerate().map(|(i, w)| micro(*w).flex(if i == 0 { 2.0 } else { 1.0 })).collect()).fixed(ROW_H)
}

fn cell(c: &Cell) -> Item {
    readout(c.text.clone()).tone(c.tone).flex(1.0)
}

fn room_rows(lines: &[RoomLine]) -> Vec<Item> {
    lines
        .iter()
        .map(|l| {
            let mut cells = vec![label(l.name).flex(2.0), cell(&l.o2), cell(&l.fire)];
            cells.push(match &l.breach {
                Some(b) => cell(b),
                None => space().flex(1.0),
            });
            cells.push(cell(&l.alarm));
            row(6.0, cells).fixed(ROW_H)
        })
        .collect()
}

/// The alarm's word for a room's row (r40, #165): the kind it shows, None with no alarm.
pub fn alarm_word(a: AlarmNews) -> Option<&'static str> {
    match a.kind {
        RoomAlarmKind::Clear => None,
        RoomAlarmKind::Fire => Some("FIRE"),
        RoomAlarmKind::Breach => Some("BREACH"),
        RoomAlarmKind::LowO2 => Some("O2 LOW"),
    }
}

fn alarm_cell(a: AlarmNews) -> Cell {
    monitor::alarm_cell(alarm_word(a), a.silenced)
}

/// The SILENCE button: lit while a room's klaxon sounds; it is the only control on the screen.
fn silence_row(alarms: &[AlarmNews; 2]) -> Item {
    let sounding = alarms.iter().any(|a| a.sounding);
    let tone = if sounding { Tone::Bad } else { Tone::Normal };
    row(6.0, vec![button("SILENCE", SILENCE_ACTION).tone(tone).enabled(sounding).flex(1.0)]).fixed(ROW_H)
}

fn device_row(name: &str, part: Option<&PartNews>) -> Item {
    let c = match part {
        Some(p) => monitor::device_cell(Some(p.share * 100.0), p.working),
        None => monitor::device_cell(None, true),
    };
    row(6.0, vec![label(name).flex(2.0), cell(&c)]).fixed(ROW_H)
}

/// A group's share of its demand: delivered over demand, a dash while nothing asks for it.
fn group_row(name: &str, r: Option<&ReactorNews>, g: BreakerGroup) -> Item {
    let i = g.index();
    let factor = r.and_then(|r| (r.demand_kw[i] > 0.0).then(|| (r.delivered_kw[i] / r.demand_kw[i]).min(1.0) * 100.0));
    row(6.0, vec![label(name).flex(2.0), cell(&monitor::group_cell(factor))]).fixed(ROW_H)
}

/// The breach shows while the wall leaks: the same rule as the breach decal (`engine_room::breach_decal`).
fn breach_open(breach: f64) -> bool {
    Breach::new(breach).open() > 0.0
}
