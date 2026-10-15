//! The status monitor's row model (r38): plain numbers in, the text and tone of each cell out. The
//! glue (`exo_app/src/dev_area/status_monitor.rs`) draws them and reads the news.
//!
//! Ported from Barotrauma's MiniMap, the hull status and the electrical view
//! (`BarotraumaClient/ClientSource/Items/Components/Machines/MiniMap.cs`): a room's breach line in
//! red (1165-1166), its air as a colour from red to green (1168-1171), and an electrical device's
//! condition in a red-orange-green gradient (1228-1229). Not ported: the water (no water here),
//! the item finder and the blips. Our rooms are the engine room and the corridor.
//! The thresholds are ours (TODO(initiator): start values): air below 60 % is a warning and below
//! 30 % bad; a device's condition below 50 % warns and below 20 % is bad, as the inventory bar does.
use crate::Tone;

/// One cell of the monitor: its text and its colour role (`Tone::Dim` for no reading).
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
}

impl Cell {
    fn new(text: impl Into<String>, tone: Tone) -> Cell {
        Cell { text: text.into(), tone }
    }
}

/// A room's line: its air in percent, its fire, its breach (the engine room only), and its alarm
/// word (r40, #165; `alarm_cell`).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomLine {
    pub name: &'static str,
    pub o2: Cell,
    pub fire: Cell,
    pub breach: Option<Cell>,
    pub alarm: Cell,
}

/// The air's tone, 0..100 %: bad below 30, warn below 60.
pub fn o2_tone(pct: f64) -> Tone {
    if pct < 30.0 {
        Tone::Bad
    } else if pct < 60.0 {
        Tone::Warn
    } else {
        Tone::Good
    }
}

/// A device's condition's tone, 0..100 %: bad below 20, warn below 50.
pub fn condition_tone(pct: f64) -> Tone {
    if pct < 20.0 {
        Tone::Bad
    } else if pct < 50.0 {
        Tone::Warn
    } else {
        Tone::Good
    }
}

/// A room's line. `o2_pct` is None without a snapshot (the dash, dim); `breach` is None for a room
/// without a hull breach.
pub fn room_line(name: &'static str, o2_pct: Option<f64>, fire: bool, breach: Option<bool>) -> RoomLine {
    let o2 = match o2_pct {
        Some(p) => Cell::new(format!("{:.0} %", p), o2_tone(p)),
        None => Cell::new("--", Tone::Dim),
    };
    let fire = if fire { Cell::new("YES", Tone::Bad) } else { Cell::new("NO", Tone::Good) };
    let breach = breach.map(|open| if open { Cell::new("YES", Tone::Bad) } else { Cell::new("NO", Tone::Good) });
    RoomLine { name, o2, fire, breach, alarm: alarm_cell(None, false) }
}

/// A room's alarm cell (r40, #165): OK with no alarm; the alarm's word in bad while it sounds, and
/// in warn once SILENCE holds it quiet (the lamp still blinks).
pub fn alarm_cell(word: Option<&str>, silenced: bool) -> Cell {
    match word {
        None => Cell::new("OK", Tone::Good),
        Some(w) => Cell::new(w, if silenced { Tone::Warn } else { Tone::Bad }),
    }
}

/// A device's condition cell. A device that is down reads DOWN in bad whatever its condition.
pub fn device_cell(condition_pct: Option<f64>, working: bool) -> Cell {
    match condition_pct {
        _ if !working => Cell::new("DOWN", Tone::Bad),
        Some(p) => Cell::new(format!("{:.0} %", p), condition_tone(p)),
        None => Cell::new("--", Tone::Dim),
    }
}

/// A grid group's factor cell: the share of its demand it gets, 0..100 %. Below full supply it
/// warns, below half it is bad (TODO(initiator): these two are start values).
pub fn group_cell(factor_pct: Option<f64>) -> Cell {
    match factor_pct {
        Some(p) if p < 50.0 => Cell::new(format!("{:.0} %", p), Tone::Bad),
        Some(p) if p < 100.0 => Cell::new(format!("{:.0} %", p), Tone::Warn),
        Some(p) => Cell::new(format!("{:.0} %", p), Tone::Good),
        None => Cell::new("--", Tone::Dim),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_below_thirty_is_bad_below_sixty_warn() {
        assert_eq!(o2_tone(12.0), Tone::Bad);
        assert_eq!(o2_tone(29.9), Tone::Bad);
        assert_eq!(o2_tone(30.0), Tone::Warn);
        assert_eq!(o2_tone(59.0), Tone::Warn);
        assert_eq!(o2_tone(60.0), Tone::Good);
    }

    #[test]
    fn condition_below_twenty_is_bad_below_fifty_warn() {
        assert_eq!(condition_tone(19.0), Tone::Bad);
        assert_eq!(condition_tone(20.0), Tone::Warn);
        assert_eq!(condition_tone(49.0), Tone::Warn);
        assert_eq!(condition_tone(50.0), Tone::Good);
    }

    #[test]
    fn a_room_line_shows_its_air_fire_and_breach() {
        let engine = room_line("ENGINE ROOM", Some(25.0), true, Some(true));
        assert_eq!(engine.o2, Cell { text: "25 %".into(), tone: Tone::Bad });
        assert_eq!(engine.fire, Cell { text: "YES".into(), tone: Tone::Bad });
        assert_eq!(engine.breach, Some(Cell { text: "YES".into(), tone: Tone::Bad }));
        let corridor = room_line("CORRIDOR", Some(95.0), false, None);
        assert_eq!(corridor.o2.tone, Tone::Good);
        assert_eq!(corridor.fire, Cell { text: "NO".into(), tone: Tone::Good });
        assert_eq!(corridor.breach, None);
        let none = room_line("CORRIDOR", None, false, None);
        assert_eq!(none.o2, Cell { text: "--".into(), tone: Tone::Dim });
    }

    #[test]
    fn the_alarm_cell_reads_the_word_and_warns_when_silenced() {
        assert_eq!(alarm_cell(None, false), Cell { text: "OK".into(), tone: Tone::Good });
        assert_eq!(alarm_cell(Some("FIRE"), false), Cell { text: "FIRE".into(), tone: Tone::Bad });
        assert_eq!(alarm_cell(Some("FIRE"), true), Cell { text: "FIRE".into(), tone: Tone::Warn });
        assert_eq!(room_line("CORRIDOR", None, false, None).alarm, alarm_cell(None, false), "a fresh line reads OK");
    }

    #[test]
    fn a_down_device_reads_down_whatever_its_condition() {
        assert_eq!(device_cell(Some(80.0), false), Cell { text: "DOWN".into(), tone: Tone::Bad });
        assert_eq!(device_cell(Some(40.0), true), Cell { text: "40 %".into(), tone: Tone::Warn });
        assert_eq!(device_cell(None, true).tone, Tone::Dim);
    }

    #[test]
    fn a_group_short_of_its_demand_warns_or_is_bad() {
        assert_eq!(group_cell(Some(100.0)).tone, Tone::Good);
        assert_eq!(group_cell(Some(70.0)).tone, Tone::Warn);
        assert_eq!(group_cell(Some(30.0)).tone, Tone::Bad);
    }
}
