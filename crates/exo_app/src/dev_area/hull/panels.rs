//! The hull's screens: ER-01 by the engine room's door (the reactor's temperature, the room's air,
//! the emergency), BD-01 (the bridge door), the three vents' valves, the airlock's AL-01 and AL-02,
//! and the status monitor ST-01 (`status_monitor`). A press asks the host.
use crate::dev_area::*;
use crate::displays::{DisplayAction, Panels};
use crate::dev_area::screens::*;
use crate::items::Items;
use display_core::{button, column, font, micro, readout, row, space, toggle, Panel, Tier, Tone};
use item_core::wire::{AirlockCycle, ReactorAsk, RoomAsk, RoomNews, ReactorNews};

use crate::dev_area::bridge::captain_screen;
use crate::dev_area::engineering::panels::{band, bands, reactor_status, zone, zone_tone};
use crate::dev_area::hull::{airlock, status_monitor};

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

pub const DOOR_ID: PanelId = PanelId::new("engine_door_screen");

pub const DOOR_NAME: &str = "REACTOR ROOM";

/// The airlock's two panels (r40): the inner door's (corridor side) and the outer door's (inside the airlock).
pub const AIRLOCK_INNER_ID: PanelId = PanelId::new("airlock_inner");

pub const AIRLOCK_INNER_NAME: &str = "INNER DOOR";

pub const AIRLOCK_OUTER_ID: PanelId = PanelId::new("airlock_outer");

pub const AIRLOCK_OUTER_NAME: &str = "OUTER DOOR";

/// The vents' panels (r38, r44): index 0 the engine room's, 1 the corridor's, 2 the bridge's (`ReactorAsk::SetVent`).
pub const VENT_ENGINE_ID: PanelId = PanelId::new("vent_engine");

pub const VENT_ENGINE_NAME: &str = "VENT ENGINE";

pub const VENT_CORRIDOR_ID: PanelId = PanelId::new("vent_corridor");

pub const VENT_CORRIDOR_NAME: &str = "VENT CORRIDOR";

pub const VENT_BRIDGE_ID: PanelId = PanelId::new("vent_bridge");

pub const VENT_BRIDGE_NAME: &str = "VENT BRIDGE";

/// The bridge door's panel (r44, BD-01): its toggle, beside the bridge door in the corridor.
pub const BRIDGE_DOOR_ID: PanelId = PanelId::new("bridge_door_screen");

pub const BRIDGE_DOOR_NAME: &str = "BRIDGE DOOR";

/// A vent's grille panel (r38): one OPEN/SHUT toggle. A shut vent takes no air; the generator's
/// flow goes to the other one. Press asks the host (`ReactorAsk::SetVent`); the host keeps the state.
pub fn vent_panel(r: Option<&ReactorNews>, i: usize) -> Panel {
    let open = r.map_or(false, |r| r.vents[i]);
    let (name, code) = match i {
        0 => (VENT_ENGINE_NAME, "VT-01"),
        1 => (VENT_CORRIDOR_NAME, "VT-02"),
        _ => (VENT_BRIDGE_NAME, "VT-03"),
    };
    let word = if open { "OPEN" } else { "SHUT" };
    let body = column(6.0, vec![micro("VENT").fixed(14.0), toggle(open, "", "vent").labels("SHUT", "OPEN").fixed(48.0), readout(word).text_size(font::SMALL).tone(if open { Tone::Good } else { Tone::Warn })]);
    Panel::screen(Tier::S, name, code, body).with_padding(8.0)
}

/// The bridge door's panel (r44, BD-01): the door's state and the toggle, as the vent panels show their valve.
/// Press asks the host (`RoomAsk::SetBridgeDoor`); the room's news carries the state.
pub fn bridge_door_panel(room: Option<&RoomNews>) -> Panel {
    let open = room.is_some_and(|r| r.bridge_door);
    let word = if open { "OPEN" } else { "SHUT" };
    let body = column(6.0, vec![micro("BRIDGE DOOR").fixed(14.0), toggle(open, "", "bridge_door").labels("SHUT", "OPEN").fixed(48.0), readout(word).text_size(font::SMALL).tone(if open { Tone::Good } else { Tone::Warn })]);
    Panel::screen(Tier::S, BRIDGE_DOOR_NAME, "BD-01", body).with_padding(8.0)
}

pub fn door_panel(r: Option<&ReactorNews>) -> Panel {
    if !powered(r) {
        return no_power(DOOR_NAME, "ER-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (optimal, allowed) = bands(Some(&r));
    let z = zone_tone(zone(r.temp, optimal, allowed));
    let (word, word_tone) = reactor_status(Some(&r));
    // The captain's threat (#301): a red EMERGENCY line while the ship is under threat.
    let emergency = captain_screen::emergency(display_core::captain::captain_view(&r).threat).map(|w| readout(format!("EMERGENCY {w}")).text_size(font::SMALL).tone(Tone::Bad).fixed(18.0));
    let temp = row(6.0, vec![readout(format!("{:.0}", r.temp)).text_size(72.0).tone(z), readout("°C").text_size(28.0).tone(z)]).fixed(70.0);
    let bandrow = row(8.0, vec![band(r.temp, optimal, allowed), readout(word).text_size(font::SMALL).tone(word_tone).flex(1.0)]).fixed(18.0);
    let air = column(0.0, vec![micro("O₂").fixed(14.0), readout(format!("{:.0}%", r.oxygen * 100.0)).text_size(font::VALUE).tone(oxygen_tone(r.oxygen))]).fixed(62.0);
    let mut body = vec![micro("REACTOR").fixed(14.0), temp, bandrow, air];
    body.extend(emergency);
    body.push(space());
    Panel::screen(Tier::S, DOOR_NAME, "ER-01", column(4.0, body)).with_padding(8.0)
}

/// An airlock panel (r40): the doors' state or the cycle's progress, and IN (the inner door), OUT (the
/// outer door), CYCLE (the pump alone). Both panels show the same; the press goes to the host.
pub fn airlock_panel(r: Option<&ReactorNews>, name: &'static str, code: &'static str) -> Panel {
    let a = r.map(|r| r.airlock).unwrap_or_default();
    let word = match a.cycling {
        AirlockCycle::Idle if a.inner_open => "INNER OPEN".to_string(),
        AirlockCycle::Idle if a.outer_open => "OUTER OPEN".to_string(),
        AirlockCycle::Idle => "DOORS SHUT".to_string(),
        _ => format!("CYCLING {:.0} %", a.progress * 100.0),
    };
    let presses = row(4.0, vec![button("IN", "in").flex(1.0), button("OUT", "out").flex(1.0), button("CYCLE", "cycle").flex(1.0)]).fixed(36.0);
    let body = column(6.0, vec![micro("AIRLOCK").fixed(14.0), readout(word).text_size(font::SMALL).tone(Tone::Normal).fixed(20.0), presses]);
    Panel::screen(Tier::S, name, code, body).with_padding(8.0)
}

/// The presses on the hull's screens: the vents' valves, the bridge door, the airlock's panels and
/// ST-01's SILENCE.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let new_state = |current: bool| c.value.map(|v| v >= 0.5).unwrap_or(!current);
        let ask = if c.panel == VENT_ENGINE_ID || c.panel == VENT_CORRIDOR_ID || c.panel == VENT_BRIDGE_ID {
            let i = if c.panel == VENT_ENGINE_ID {
                0
            } else if c.panel == VENT_CORRIDOR_ID {
                1
            } else {
                2
            };
            snap.map(|r| ReactorAsk::SetVent(i, new_state(r.vents[i as usize])))
        } else if c.panel == BRIDGE_DOOR_ID {
            let open = new_state(items.room.bridge_door);
            items.ask_room(RoomAsk::SetBridgeDoor(open));
            None
        } else if c.panel == status_monitor::ID {
            match c.action.as_str() {
                status_monitor::SILENCE_ACTION => Some(ReactorAsk::SilenceAlarms),
                _ => None,
            }
        } else if c.panel == AIRLOCK_INNER_ID || c.panel == AIRLOCK_OUTER_ID {
            airlock::panel_ask(&c.action)
        } else {
            None
        };
        if let Some(a) = ask {
            items.ask_reactor(a);
        }
    }
}

/// The hull's panels from the snapshot and the room's news, every tick.
pub fn write_panels(items: Res<Items>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    show(&mut panels, r, DOOR_ID, door_panel(r));
    show(&mut panels, r, VENT_ENGINE_ID, vent_panel(r, 0));
    show(&mut panels, r, VENT_CORRIDOR_ID, vent_panel(r, 1));
    show(&mut panels, r, VENT_BRIDGE_ID, vent_panel(r, 2));
    show(&mut panels, r, BRIDGE_DOOR_ID, bridge_door_panel(Some(&items.room)));
    show(&mut panels, r, AIRLOCK_INNER_ID, airlock_panel(r, AIRLOCK_INNER_NAME, "AL-01"));
    show(&mut panels, r, AIRLOCK_OUTER_ID, airlock_panel(r, AIRLOCK_OUTER_NAME, "AL-02"));
    show(&mut panels, r, status_monitor::ID, status_monitor::monitor_panel(r));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_airlock_panels_have_the_three_presses() {
        assert!(airlock_panel(None, AIRLOCK_INNER_NAME, "AL-01").find("cycle").is_some(), "the airlock panels have IN, OUT and CYCLE");
    }
}
