//! The bots' glue (r42, #253): the host's bot round. `reactor_core::bots` decides what a bot does at a
//! station, this module sends those actions as the asks a player's screen sends (`ReactorAsk`), and
//! says which station a player is at.
//!
//! Occupancy: a station is player-used while a player has its screen as the active display (`ActiveDisplay`,
//! the simpler of #253's two readings; the other is 1.5 m for 5 s). Every player's display is reported:
//! a client sends `ToHost::Using` when its display changes, the host keeps one entry per player
//! (`Items::using`), and a station is occupied while any entry names it. The grace time after a player
//! leaves is `reactor_core::bots` (`BotTuning::grace_s`).
//!
//! The round runs on the host (`Role::Local` and `Role::Host`), before `reactor::reactor_step`, which
//! applies the asks it pushes. A client sends nothing: the host's bots are the crew's.
use super::reactor_screen::{GUNNERY_ID, HELM_ID, JUNCTION_ID, PUMP_ID, READOUT_ID};
use super::reactor::{wire_pump, ReactorSim};
use crate::displays::{ActiveDisplay, DisplayPanel};
use crate::items::{Items, Role};
use bevy::prelude::*;
use display_core::Panel;
use item_core::wire::{BotStation, BreakerGroup, ReactorAsk};
use item_core::PlayerId;
use reactor_core::bots::{Action, Station, STATIONS};
use reactor_core::Group;
use std::collections::BTreeMap;

/// The station a wire name stands for.
pub fn station_of(s: BotStation) -> Station {
    match s {
        BotStation::Gunnery => Station::Gunnery,
        BotStation::Reactor => Station::Reactor,
        BotStation::Power => Station::Power,
        BotStation::Helm => Station::Helm,
    }
}

pub fn wire_station(s: Station) -> BotStation {
    match s {
        Station::Gunnery => BotStation::Gunnery,
        Station::Reactor => BotStation::Reactor,
        Station::Power => BotStation::Power,
        Station::Helm => BotStation::Helm,
    }
}

/// The station whose screen a panel is. The reactor's station is its readout and its pump.
pub fn station_of_panel(id: &display_core::PanelId) -> Option<Station> {
    if *id == GUNNERY_ID {
        Some(Station::Gunnery)
    } else if *id == READOUT_ID || *id == PUMP_ID {
        Some(Station::Reactor)
    } else if *id == JUNCTION_ID {
        Some(Station::Power)
    } else if *id == HELM_ID {
        Some(Station::Helm)
    } else {
        None
    }
}

fn breaker_of(g: Group) -> BreakerGroup {
    match g {
        Group::Engines => BreakerGroup::Engines,
        Group::LifeSupport => BreakerGroup::LifeSupport,
        Group::Lights => BreakerGroup::Lights,
        Group::Shields => BreakerGroup::Shields,
        Group::Weapons => BreakerGroup::Weapons,
        Group::JumpDrive => BreakerGroup::JumpDrive,
    }
}

/// The ask a player's screen sends for a bot's action. GN-01's hold is `ClearJam` (the bot's hold).
pub fn ask_of(a: Action) -> ReactorAsk {
    match a {
        Action::Fire => ReactorAsk::Fire,
        Action::Hold(on) => ReactorAsk::ClearJam(on),
        Action::SetManual(on) => ReactorAsk::SetManual(on),
        Action::SetPump(m) => ReactorAsk::SetPumpMode(wire_pump(m)),
        Action::SetLimit(g, v) => ReactorAsk::SetLimit(breaker_of(g), v),
        Action::SetThrottle(pct) => ReactorAsk::SetThrottle(pct),
    }
}

/// The stations with a player on them: a station is occupied while any player's entry names it.
pub fn occupied(using: &BTreeMap<PlayerId, BotStation>) -> [bool; STATIONS] {
    let mut out = [false; STATIONS];
    for s in using.values() {
        out[station_of(*s).index()] = true;
    }
    out
}

/// The host's bot round: this player's station is reported when it changes (every role; a client sends
/// it to the host), then the occupancy from every player's entry, then the bots' asks for the round
/// (every `tick_s`, and the manning and the holds at once).
pub fn bots_step(time: Res<Time>, active: Res<ActiveDisplay>, displays: Query<&DisplayPanel>, mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>, mut shown: Local<Option<BotStation>>) {
    let now = active.using().and_then(|e| displays.get(e).ok()).and_then(|d| station_of_panel(&d.id)).map(wire_station);
    if now != *shown {
        *shown = now;
        items.ask_using(now);
    }
    if items.role == Role::Client {
        return;
    }
    let occupied = occupied(&items.using);
    let sim = &mut *sim;
    for (_, a) in sim.bots.tick(time.delta_secs_f64(), occupied, &sim.room) {
        items.ask_reactor(ask_of(a));
    }
}

/// The BOT tag (r42, #253): the station's plate tag reads "<code> BOT" while a bot runs it.
pub fn bot_tagged(mut p: Panel, manned: bool) -> Panel {
    if manned {
        if let Some(plate) = p.plate.as_mut() {
            plate.tag = format!("{} BOT", plate.tag);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_and_the_core_name_the_same_stations() {
        for s in BotStation::ALL {
            assert_eq!(wire_station(station_of(s)), s);
        }
    }

    #[test]
    fn the_reactor_station_is_its_readout_and_its_pump() {
        assert_eq!(station_of_panel(&READOUT_ID), Some(Station::Reactor));
        assert_eq!(station_of_panel(&PUMP_ID), Some(Station::Reactor));
        assert_eq!(station_of_panel(&display_core::PanelId::new("captain_console")), None);
    }

    #[test]
    fn a_station_is_occupied_while_any_player_uses_it() {
        let mut using: BTreeMap<PlayerId, BotStation> = BTreeMap::new();
        using.insert(2, BotStation::Gunnery);
        using.insert(3, BotStation::Gunnery);
        using.insert(4, BotStation::Helm);
        assert_eq!(occupied(&using), [true, false, false, true]);
        using.remove(&2);
        assert_eq!(occupied(&using), [true, false, false, true], "player 3 still at GN-01");
        using.remove(&3);
        assert_eq!(occupied(&using), [false, false, false, true]);
    }

    #[test]
    fn a_bots_hold_is_the_clear_jam_ask_and_its_limit_is_the_breaker_group() {
        assert_eq!(ask_of(Action::Hold(true)), ReactorAsk::ClearJam(true));
        assert_eq!(ask_of(Action::SetLimit(Group::Shields, 1.0)), ReactorAsk::SetLimit(BreakerGroup::Shields, 1.0));
        assert_eq!(ask_of(Action::SetManual(false)), ReactorAsk::SetManual(false));
    }
}
