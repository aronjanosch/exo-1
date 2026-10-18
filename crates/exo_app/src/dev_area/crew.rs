//! The bots' glue (r42, #253; r44 bodies): the host's bot round and the bots' figures. `reactor_core::bots`
//! decides what a bot does and where it walks; this module sends the actions as the asks a player's
//! screen sends (`ReactorAsk`), says which station a player is at, lays out the stations and the pads
//! on the ground, and moves each bot's figure to its walker.
//!
//! Occupancy: a station is player-used while a player has its screen as the active display (`ActiveDisplay`,
//! the simpler of #253's two readings; the other is 1.5 m for 5 s). Every player's display is reported:
//! a client sends `ToHost::Using` when its display changes, the host keeps one entry per player
//! (`Items::using`), and a station is occupied while any entry names it. The grace time after a player
//! leaves is `reactor_core::bots` (`BotTuning::grace_s`).
//!
//! The round runs on the host (`Role::Local` and `Role::Host`), before `reactor::reactor_step`, which
//! applies the asks it pushes. A client sends nothing: the host's bots are the crew's. The figures are
//! host-side too: a client does not see the bots' bodies yet (TODO(initiator): networking the walkers).
//!
//! The figure (r44): a placeholder box, magenta as crew_core's demo bot was (`captain/bots.rs` on
//! `origin/round/dev-area`). It is not the player figure (#32) and has no status label yet
//! (`public_status.rs` labels the net figures only; TODO(initiator)). It walks in a straight line through
//! walls (TODO(initiator): pathing).
use crate::dev_area::layout::{CORRIDOR, GUNNERY_AT, HELM_AT, REACTOR_FRONT_AT};
use crate::dev_area::ship::{wire_pump, ReactorSim};
use super::DevArea;
use crate::displays::{ActiveDisplay, DisplayPanel};
use crate::env::PlanetRes;
use crate::items::{Items, Role};
use crate::origin::WorldPose;
use bevy::prelude::*;
use display_core::Panel;
use item_core::wire::{BotStation, BreakerGroup, ReactorAsk};
use item_core::PlayerId;
use reactor_core::bots::{Action, Job, Layout, Row, Station, Walker, MAX_BOTS, STATIONS};
use reactor_core::Group;
use std::collections::BTreeMap;

/// The row a wire name stands for (the same order: `BotStation::ALL` is `Row::ALL`).
pub fn row_of(s: BotStation) -> Row {
    Row::ALL[s.index()]
}

pub fn wire_row(r: Row) -> BotStation {
    BotStation::ALL[r.index()]
}

/// The station a wire name stands for; None for a work row.
pub fn station_of(s: BotStation) -> Option<Station> {
    row_of(s).station()
}

/// The station whose screen a panel is. The reactor's station is its main screen and its pump.
pub fn station_of_panel(id: &display_core::PanelId) -> Option<Station> {
    use crate::dev_area::{turrets::panels::GUNNERY_ID, bridge::panels::HELM_ID, engineering::{panels::{JUNCTION_ID, PUMP_ID}, reactor_room::MAIN_ID}};
    if *id == GUNNERY_ID {
        Some(Station::Gunnery)
    } else if *id == PUMP_ID || *id == MAIN_ID {
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
        if let Some(st) = station_of(*s) {
            out[st.index()] = true;
        }
    }
    out
}

/// Where the stations and the pads stand on the ground (dev area metres). The station rows are the
/// screens: GN-01 at its screen, the reactor in front of its main screen, POWER at the junction box, the HELM at its
/// panel. The bots start in the corridor, two metres apart. TODO(initiator): a stand point in front of a
/// screen, and the pads' spots.
pub fn layout() -> Layout {
    let pads = [CORRIDOR.point([-6.0, 0.0]), CORRIDOR.point([-4.0, 0.0]), CORRIDOR.point([-2.0, 0.0])];
    // POWER stands at JB-1 (the old JB-01 screen is gone, #322).
    Layout { stations: [GUNNERY_AT, REACTOR_FRONT_AT, crate::dev_area::layout::junction_front(0, 0.3), HELM_AT], pads }
}

/// The host's bot round: this player's station is reported when it changes (every role; a client sends
/// it to the host), then the occupancy from every player's entry, then the bots' asks for the round
/// (every `tick_s`, and the manning and the holds at once). The work rows have no jobs yet (`&[]`).
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
    let room = &sim.room;
    for (_, a) in sim.bots.tick(time.delta_secs_f64(), occupied, room, &layout(), &[] as &[Job]) {
        items.ask_reactor(ask_of(a));
    }
}

/// The wire name of a station.
pub fn wire_station(s: Station) -> BotStation {
    wire_row(Row::from(s))
}

/// One bot's figure: the walker at `slot` (`Bots::walkers`).
#[derive(Component)]
pub struct BotBody {
    pub slot: usize,
}

/// The bots' figures follow their walkers (r44): one figure per walker, spawned at the pads with its
/// bot, moved to the walker's place on the ground, and removed when the bot is gone. Headless too (the
/// figure's look is window-only, `window_plugin`).
pub fn body_step(mut commands: Commands, sim: Res<ReactorSim>, area: Option<Res<DevArea>>, planet: Option<Res<PlanetRes>>, mut bodies: Query<(Entity, &BotBody, &mut WorldPose)>) {
    let (Some(area), Some(planet)) = (area, planet) else { return };
    let Some(frame) = area.frame else { return };
    let walkers: &[Walker] = sim.bots.walkers();
    let mut have = [false; MAX_BOTS as usize];
    for (e, b, mut pose) in &mut bodies {
        match walkers.get(b.slot) {
            Some(w) => {
                have[b.slot] = true;
                let (pos, up) = frame.ground(&planet, w.pos[0], w.pos[1], 0.85);
                *pose = WorldPose { pos, rot: frame.rotation(up, 0.0) };
            }
            None => commands.entity(e).despawn(),
        }
    }
    for (slot, w) in walkers.iter().enumerate() {
        if !have[slot] {
            let (pos, up) = frame.ground(&planet, w.pos[0], w.pos[1], 0.85);
            commands.spawn((BotBody { slot }, WorldPose { pos, rot: frame.rotation(up, 0.0) }, Transform::default(), Visibility::default()));
        }
    }
}

/// Window: the bot figure is a magenta box (placeholder until the figure, #32).
fn add_body_mesh(mut commands: Commands, added: Query<Entity, Added<BotBody>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for e in &added {
        commands.entity(e).insert((Mesh3d(meshes.add(Cuboid::new(0.6, 1.7, 0.6))), MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.2, 0.85), ..default() }))));
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (bots_step, body_step).chain().in_set(crate::dev_area::ship::ShipSet::Crew));
    app.add_systems(FixedUpdate, asks.in_set(crate::dev_area::ship::ShipSet::Asks));
}

fn asks(mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>) {
    for a in crate::dev_area::ship::host_asks(&mut items, |a| matches!(a, ReactorAsk::SetBotPriority(..))) {
        apply(&mut sim, a);
    }
}

/// The captain's priority list (r42, CP-01) on the bots; false for an ask that is not the crew's.
pub fn apply(s: &mut ReactorSim, a: ReactorAsk) -> bool {
    match a {
        ReactorAsk::SetBotPriority(st, level) => s.bots.set_level(row_of(st), level),
        _ => return false,
    }
    true
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, add_body_mesh.in_set(crate::phases::Frame::World));
}

/// Whether a bot runs a station now (the host's snapshot).
pub fn manned(items: &Items, s: BotStation) -> bool {
    items.reactor.is_some_and(|r| r.bots[s.index()].manned)
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
    fn the_wire_and_the_core_name_the_same_rows() {
        for s in BotStation::ALL {
            assert_eq!(wire_row(row_of(s)), s);
        }
        assert_eq!(station_of(BotStation::Fire), None, "a work row is no station");
        assert_eq!(station_of(BotStation::Helm), Some(Station::Helm));
    }

    #[test]
    fn the_reactor_station_is_its_main_screen_and_its_pump() {
        assert_eq!(station_of_panel(&crate::dev_area::engineering::reactor_room::MAIN_ID), Some(Station::Reactor));
        assert_eq!(station_of_panel(&crate::dev_area::engineering::panels::PUMP_ID), Some(Station::Reactor));
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

    #[test]
    fn the_stations_are_laid_out_in_the_station_order_and_the_bots_start_in_the_corridor() {
        let l = layout();
        assert_eq!(l.stations[Station::Gunnery.index()], GUNNERY_AT);
        assert_eq!(l.stations[Station::Helm.index()], HELM_AT);
        for p in l.pads {
            assert!(CORRIDOR.contains(p), "a bot starts in the corridor");
        }
    }
}
