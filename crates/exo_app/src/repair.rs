//! Repair at the machine (#299): the wrench or the screwdriver in hand, a timing ring around the
//! reticle. No repair screen.
//!
//! - Start (client): the right tool in the selected slot, on foot in the dev area, the machine's
//!   block in the look within reach, the machine below full condition, LMB. The ring starts at
//!   once and the request goes to the host (`RepairAsk::Start`).
//! - The ring runs here (`device_core::minigame`): LMB in a zone hits it (`Hit`), any LMB turns the
//!   pointer (a `Miss` outside every zone), RMB held speeds it up. The feel needs no latency.
//! - The host (`repair_host`) keeps one session per player and applies what arrives: a hit adds
//!   `hit_share` of the max, a miss costs `miss_share`, and a session raises the condition by
//!   itself in the device's repair time at the skill. Full condition ends the session.
//! - End (client, `Stop`): full condition, looking away, walking further than `walk_away_m` from
//!   where it began, switching slot, a display in use, F or ESC.
//! - A miss hurts the repairer (`device_core::minigame::hurt_of`): the junction box shocks (a burn
//!   on the host's body and a short stun), the reactor burns, the pump knocks (a stun, no health).
//!   The client feels it at once (its own miss: the stun, the `RepairHurt` flash and sound); the
//!   host applies the burn to the repairer's body, and the vitals reach the client as usual.
//! - Values: `content/gameplay/repair/repair.json` (`device_core::minigame::Tuning`, strict loader).
//!   TODO(initiator): starting values.
//!
//! Other players see nothing yet (later: an animation). A start the host refuses (another player
//! at the same machine, the wrong tool, a full machine) is told back (`ToClient::RepairRefused`,
//! or the flag for this process), and the client's ring ends.
//! - A player who leaves (the channel's Bye or a dead channel) loses their session, spray and
//!   body (`items_net_in`, `RepairSessions::forget`).
use crate::controls::{Actions, Controls, Tap};
use crate::dev_area::engine_room::{COOLANT_PUMP, JUNCTION_BOX, REACTOR};
use crate::dev_area::reactor::{route, ReactorCounts, ReactorSim};
use crate::dev_area::DevBox;
use crate::displays::ActiveDisplay;
use crate::health::HealthState;
use crate::items::{Items, NetOut, Role};
use crate::walker::{Player, EYE_HEIGHT};
use avian3d::prelude::{Position, Rotation};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use device_core::minigame::{hurt_of, may_rise, minigame_on, ray_hits_box, Hurt, Machine, Press, Session, Tuning};
use device_core::{Condition, DeviceEvent, Tool};
use item_core::wire::{PartNews, ReactorNews, RepairAsk, RepairDevice, RepairTool};
use item_core::PlayerId;
use reactor_core::{EngineRoom, RoomEvent, Source};
use walker_core::Frame;

/// The values file, relative to `content/gameplay`.
pub const REPAIR_PATH: &str = "repair/repair.json";

/// The loaded values.
#[derive(Resource, Clone, Copy, Debug)]
pub struct RepairConfig(pub Tuning);

/// Parses and checks the values file. Every error names the field.
pub fn load_tuning(files: &[content_core::File]) -> Result<Tuning, Vec<String>> {
    let Some(f) = files.iter().find(|f| f.path == REPAIR_PATH) else { return Err(vec![format!("{REPAIR_PATH}: missing")]) };
    let t: Tuning = content_core::parse_strict(REPAIR_PATH, &f.text).map_err(|m| vec![m])?;
    let mut e = Vec::new();
    let mut need = |ok: bool, field: &str, rule: &str| {
        if !ok {
            e.push(format!("{REPAIR_PATH}: {field}: {rule}"));
        }
    };
    need((0.0..=100.0).contains(&t.skill), "skill", "must be 0..100");
    need((0.0..=1.0).contains(&t.hit_share) && (0.0..=1.0).contains(&t.miss_share), "hit_share", "miss_share and hit_share must be 0..1");
    need(t.pointer_deg_s > 0.0 && t.boost > 0.0, "pointer_deg_s", "pointer_deg_s and boost must be > 0");
    need(t.zones_max >= 1, "zones_max", "must be at least 1");
    need(t.zone_deg > 0.0 && t.zone_deg < 360.0, "zone_deg", "must be 0 < zone_deg < 360");
    need(t.zone_life_s > 0.0 && t.respawn_s >= 0.0, "zone_life_s", "zone_life_s must be > 0, respawn_s >= 0");
    need((0.0..180.0).contains(&t.spawn_min_deg), "spawn_min_deg", "must be 0..180");
    need(t.reach_m > 0.0 && t.walk_away_m > 0.0 && t.flash_s >= 0.0, "reach_m", "reach_m and walk_away_m must be > 0, flash_s >= 0");
    need((0.0..=1.0).contains(&t.minigame_below), "minigame_below", "must be 0..1");
    need(t.shock_burn >= 0.0 && t.hot_burn >= 0.0, "shock_burn", "shock_burn and hot_burn must be >= 0");
    need(t.shock_stun_s >= 0.0 && t.knock_stun_s >= 0.0 && t.hurt_flash_s >= 0.0, "shock_stun_s", "the stuns and hurt_flash_s must be >= 0");
    if e.is_empty() { Ok(t) } else { Err(e) }
}

/// The tool each machine takes: the wrench for the reactor and the pump, the screwdriver for the
/// junction box.
pub fn tool_for(device: RepairDevice) -> RepairTool {
    match device {
        RepairDevice::Reactor | RepairDevice::CoolantPump => RepairTool::Wrench,
        RepairDevice::JunctionBox => RepairTool::Screwdriver,
    }
}

/// The kind of machine a device is, for what a miss does to the repairer.
pub fn machine_of(device: RepairDevice) -> Machine {
    match device {
        RepairDevice::JunctionBox => Machine::Electrical,
        RepairDevice::Reactor => Machine::Hot,
        RepairDevice::CoolantPump => Machine::Mechanical,
    }
}

/// The tool an item is, by its `use` effect.
fn tool_of_effect(effect: &str) -> Option<RepairTool> {
    match effect {
        "repair_mechanical" => Some(RepairTool::Wrench),
        "repair_electrical" => Some(RepairTool::Screwdriver),
        _ => None,
    }
}

/// The machine a dev area part is, by its device id.
fn device_of(id: &str) -> Option<RepairDevice> {
    match id {
        REACTOR => Some(RepairDevice::Reactor),
        COOLANT_PUMP => Some(RepairDevice::CoolantPump),
        JUNCTION_BOX => Some(RepairDevice::JunctionBox),
        _ => None,
    }
}

fn condition(room: &EngineRoom, d: RepairDevice) -> &Condition {
    match d {
        RepairDevice::Reactor => room.reactor().condition(),
        RepairDevice::CoolantPump => room.pump().condition(),
        RepairDevice::JunctionBox => room.junction().condition(),
    }
}

fn condition_mut(room: &mut EngineRoom, d: RepairDevice) -> &mut Condition {
    match d {
        RepairDevice::Reactor => room.reactor_mut().condition_mut(),
        RepairDevice::CoolantPump => room.pump_mut().condition_mut(),
        RepairDevice::JunctionBox => room.junction_mut().condition_mut(),
    }
}

/// The room's source of a machine, for its events.
fn source_of(d: RepairDevice) -> Source {
    match d {
        RepairDevice::Reactor => Source::Reactor,
        RepairDevice::CoolantPump => Source::Pump,
        RepairDevice::JunctionBox => Source::Junction,
    }
}

/// The device's tool speed for a session's tool.
fn tool_speed(t: RepairTool) -> Tool {
    match t {
        RepairTool::Wrench => Tool::WRENCH,
        RepairTool::Screwdriver => Tool::SCREWDRIVER,
    }
}

/// The machine's part of the snapshot (what the client sees of it).
fn part_of(r: &ReactorNews, d: RepairDevice) -> PartNews {
    match d {
        RepairDevice::Reactor => r.reactor,
        RepairDevice::CoolantPump => r.pump,
        RepairDevice::JunctionBox => r.junction,
    }
}

/// One session on the host: who works on which machine, and the hits it has made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostSession {
    pub player: PlayerId,
    pub device: RepairDevice,
    /// The tool the session started with: it sets the rise.
    pub tool: RepairTool,
    pub hits: u32,
    /// The session began on a broken machine: no rise before its hits.
    pub gated: bool,
}

/// The host's repair sessions. The counts are for the scenario.
#[derive(Resource, Default, Debug)]
pub struct RepairSessions {
    pub sessions: Vec<HostSession>,
    pub hits: u32,
    pub misses: u32,
}

impl RepairSessions {
    /// The session of this player, if one runs.
    pub fn of_mut(&mut self, player: PlayerId) -> Option<&mut HostSession> {
        self.sessions.iter_mut().find(|h| h.player == player)
    }

    /// The player left: their session ends (a disconnect, `health` and `fire` do the same for
    /// their body and spray).
    pub fn forget(&mut self, player: PlayerId) {
        self.sessions.retain(|h| h.player != player);
    }
}

/// The last hurt this process felt on its own miss: the machine and when (the elapsed seconds).
/// The screen's edge flash and the sound read it (`repair_hud`, `audio_slice`).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct RepairHurt {
    pub machine: Option<Machine>,
    pub at: Option<f64>,
}

/// One session on this client: the ring and what the flash shows.
pub struct Play {
    pub device: RepairDevice,
    /// The selected slot when it began: switching it ends the session.
    pub slot: usize,
    /// Where the walker stood when it began.
    pub start: DVec3,
    /// The ring and the hits; the ring turns only while the minigame is on (`minigame_below`).
    pub session: Session,
    /// The session began on a broken machine: the pips show the hits it needs.
    pub gated: bool,
    /// The last press (hit or miss) and its age, s, while the flash shows.
    pub flash: Option<(Press, f64)>,
    /// Seconds left of the stun after a miss: the controls are frozen, the ring keeps running.
    pub stun: f64,
}

/// This process's session, if one runs (the client's own ring).
#[derive(Resource, Default)]
pub struct RepairPlay(pub Option<Play>);

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = load_tuning(&files).unwrap_or_else(|e| panic!("repair: {e:?}"));
    app.insert_resource(RepairConfig(tuning)).init_resource::<RepairSessions>().init_resource::<RepairPlay>().init_resource::<RepairHurt>();
    app.add_systems(
        FixedUpdate,
        (repair_client, repair_host)
            .chain()
            .after(crate::scenario::run_script)
            .after(crate::items_net::items_net_in)
            .before(crate::health::use_input)
            .before(crate::displays::use_display)
            .in_set(crate::phases::Fx::Input),
    );
}

/// The machine whose block the look hits within `reach`, if any.
pub(crate) fn aimed(eye: DVec3, look: DVec3, machines: &Query<(&DevBox, &Position, &Rotation)>, reach: f64) -> Option<RepairDevice> {
    let dir = look.normalize_or_zero();
    machines.iter().find_map(|(b, p, r)| {
        let d = device_of(&b.id)?;
        let inv = r.0.inverse();
        let o = inv * (eye - p.0);
        let v = inv * dir;
        ray_hits_box(o.to_array(), v.to_array(), (b.size * 0.5).to_array(), reach).then_some(d)
    })
}

/// Client: starts a ring on LMB at a machine, runs it, hits and misses go to the host, and ends
/// the session when a rule says so.
#[allow(clippy::too_many_arguments)]
fn repair_client(time: Res<Time>, mut controls: ResMut<Controls>, mut actions: ResMut<Actions>, mut items: ResMut<Items>, cfg: Res<RepairConfig>, active: Res<ActiveDisplay>, mut play: ResMut<RepairPlay>, mut hurt: ResMut<RepairHurt>, players: Query<&Player>, machines: Query<(&DevBox, &Position, &Rotation)>) {
    let t = cfg.0;
    let dt = time.delta_secs_f64();
    let Ok(pl) = players.single() else {
        play.0 = None;
        return;
    };
    // The host ended the session (the machine is full): the ring stops, nothing more is sent.
    let host_done = std::mem::take(&mut items.repair_done);
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let device = aimed(eye, pl.world_look(Frame::IDENTITY), &machines, t.reach_m);
    let slot = items.client.view.selected;
    let tool = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).and_then(|d| d.use_effect.as_deref()).and_then(tool_of_effect);
    // The snapshot is the client's view of the condition; without one, nothing can be repaired.
    let part = |d: RepairDevice| items.reactor.as_ref().map(|r| part_of(r, d));
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly && active.using().is_none() && !controls.free_cursor;
    match play.0.take() {
        None => {
            let Some(d) = device else { return };
            let usable = tool == Some(tool_for(d)) && part(d).is_some_and(|p| p.share < 1.0);
            if !controls.click || !on_foot || !usable {
                return;
            }
            // The LMB is the repair's: the bandage hold and the use do not see it.
            controls.click = false;
            let gated = part(d).is_some_and(|p| !p.working);
            items.repair_done = false;
            items.ask_repair(RepairAsk::Start { device: d, tool: tool_for(d) });
            let seed = (time.elapsed_secs_f64() * 1e6) as u64 ^ 0x299;
            play.0 = Some(Play { device: d, slot, start: pl.w.pos, session: Session::new(t, seed), gated, flash: None, stun: 0.0 });
        }
        Some(mut p) => {
            // The key is taken here so that F ends the session and does nothing else.
            let key = actions.take_tap(Tap::Interact);
            let done = part(p.device).is_none_or(|x| x.share >= 1.0);
            if host_done {
                return;
            }
            let ended = key || done || !on_foot || slot != p.slot || device != Some(p.device) || pl.w.pos.distance(p.start) > t.walk_away_m;
            if ended {
                items.ask_repair(RepairAsk::Stop);
                return;
            }
            // The fraction the minigame goes by: the client's view of the machine's condition.
            let fraction = part(p.device).map_or(1.0, |x| x.share);
            // Stunned: the controls are frozen (no LMB, no walk, no look); the ring still turns.
            let stunned = p.stun > 0.0;
            p.stun = (p.stun - dt).max(0.0);
            if stunned {
                controls.click = false;
                actions.move_dir = DVec3::ZERO;
                actions.look = Vec2::ZERO;
                actions.turn = DVec2::ZERO;
                actions.roll = 0.0;
                actions.jump = false;
            }
            if controls.click {
                controls.click = false;
                // With the minigame off (half or more condition) LMB does nothing here.
                if let Some(press) = p.session.press(fraction) {
                    items.ask_repair(match press {
                        Press::Hit => RepairAsk::Hit,
                        Press::Miss => RepairAsk::Miss,
                    });
                    p.flash = Some((press, 0.0));
                    // The miss hurts here at once (the feel needs no latency); the host burns the body.
                    let h = hurt_of(machine_of(p.device), press, &t);
                    if h != Hurt::default() {
                        p.stun = p.stun.max(h.stun_s);
                        hurt.machine = Some(machine_of(p.device));
                        hurt.at = Some(time.elapsed_secs_f64());
                    }
                }
            }
            p.session.step(dt, controls.secondary_held, fraction);
            p.flash = p.flash.and_then(|(k, age)| (age + dt < t.flash_s).then_some((k, age + dt)));
            play.0 = Some(p);
        }
    }
}

/// Tells a player that the host refused its start: this process's own flag, or a message to the
/// client. The client's ring ends (as at a full machine).
fn refuse(items: &mut Items, player: PlayerId) {
    if player == items.me() {
        items.repair_done = true;
    } else {
        items.net_out.push(NetOut::RepairRefused { to: player });
    }
}

/// Host and one process: the requests, one step of every session (the hits and misses while the
/// minigame is on, the rise by itself where the rules let it), the full ones end. A client does
/// nothing here (its requests went out). A refused start is told to its player.
fn repair_host(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<RepairConfig>, sim: Option<ResMut<ReactorSim>>, mut sessions: ResMut<RepairSessions>, counts: Option<ResMut<ReactorCounts>>, mut health: Option<ResMut<HealthState>>) {
    let asks = std::mem::take(&mut items.repair_asks);
    if items.role == Role::Client {
        return;
    }
    let Some(mut sim) = sim else { return };
    let Some(mut counts) = counts else { return };
    let t = cfg.0;
    for (player, a) in asks {
        match a {
            RepairAsk::Start { device, tool } => {
                // The tool must be the machine's, the machine must need it, and no one else may work on it.
                let busy = sessions.sessions.iter().any(|h| h.device == device && h.player != player);
                if tool != tool_for(device) || busy || condition(&sim.room, device).fraction() >= 1.0 {
                    refuse(&mut items, player);
                    continue;
                }
                let gated = !condition(&sim.room, device).functioning();
                sessions.sessions.retain(|h| h.player != player);
                sessions.sessions.push(HostSession { player, device, tool, hits: 0, gated });
            }
            RepairAsk::Hit => {
                let Some(h) = sessions.of_mut(player) else { continue };
                let d = h.device;
                let c = condition_mut(&mut sim.room, d);
                if !minigame_on(c.fraction(), &t) {
                    continue;
                }
                let e = c.restore(t.hit_share * c.tuning().max);
                h.hits += 1;
                sessions.hits += 1;
                emit(&mut counts, d, e);
            }
            RepairAsk::Miss => {
                let Some(h) = sessions.of_mut(player) else { continue };
                let d = h.device;
                let c = condition_mut(&mut sim.room, d);
                if !minigame_on(c.fraction(), &t) {
                    continue;
                }
                let e = c.damage(t.miss_share * c.tuning().max);
                sessions.misses += 1;
                emit(&mut counts, d, e);
                // The miss burns the repairer's body by what the machine does (the host's part).
                let burn = hurt_of(machine_of(d), Press::Miss, &t).burn;
                if let Some(st) = health.as_deref_mut()
                    && let Some(b) = st.bodies.get_mut(&player)
                {
                    b.health.add(&st.tuning, "burn", burn);
                }
            }
            RepairAsk::Stop => sessions.sessions.retain(|h| h.player != player),
        }
    }
    let dt = time.delta_secs_f64();
    let mut full = Vec::new();
    for h in sessions.sessions.clone() {
        // A broken machine waits for its hits; otherwise the rise runs at the session's tool's repair time.
        if may_rise(h.gated, h.hits, condition(&sim.room, h.device).fraction(), &t) {
            let e = condition_mut(&mut sim.room, h.device).repair_rise(dt, t.skill, tool_speed(h.tool));
            emit(&mut counts, h.device, e);
        }
        if condition(&sim.room, h.device).fraction() >= 1.0 {
            full.push(h.player);
        }
    }
    sessions.sessions.retain(|h| !full.contains(&h.player));
    // The players whose session ended: this process's own through the flag, others over the network.
    for player in full {
        if player == items.me() {
            items.repair_done = true;
        } else {
            items.net_out.push(NetOut::RepairDone { to: player });
        }
    }
}

/// A change of the machine's state from a repair (works again, needs repair, broke) goes to the
/// room's event log, as the room's own events do.
fn emit(counts: &mut ReactorCounts, d: RepairDevice, e: Option<DeviceEvent>) {
    if let Some(event) = e {
        route(RoomEvent::Device { source: source_of(d), event }, counts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Vec<content_core::File> {
        vec![content_core::File { path: REPAIR_PATH.into(), text: include_str!("../../../content/gameplay/repair/repair.json").into() }]
    }

    #[test]
    fn the_shipped_file_loads() {
        let t = load_tuning(&shipped()).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(t.zones_max, 2);
        assert_eq!(t.skill, 50.0);
    }

    #[test]
    fn unknown_fields_and_bad_values_are_named() {
        let good = include_str!("../../../content/gameplay/repair/repair.json");
        let typo = good.replacen("\"zones_max\"", "\"zone_max\"", 1);
        let e = load_tuning(&[content_core::File { path: REPAIR_PATH.into(), text: typo }]).unwrap_err();
        assert!(e[0].contains(REPAIR_PATH) && e[0].contains("zone_max"), "{e:?}");
        let bad = good.replacen("\"zones_max\": 2", "\"zones_max\": 0", 1);
        let e = load_tuning(&[content_core::File { path: REPAIR_PATH.into(), text: bad }]).unwrap_err();
        assert!(e.iter().any(|m| m.contains("zones_max")), "{e:?}");
        assert!(load_tuning(&[]).unwrap_err()[0].contains("missing"));
    }

    #[test]
    fn a_player_who_left_loses_only_their_session() {
        let h = |player| HostSession { player, device: RepairDevice::Reactor, tool: RepairTool::Wrench, hits: 0, gated: false };
        let mut s = RepairSessions { sessions: vec![h(2), h(3)], ..Default::default() };
        s.forget(2);
        assert_eq!(s.sessions.iter().map(|h| h.player).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn the_session_tool_sets_the_rise() {
        assert!(tool_speed(RepairTool::Screwdriver).speed < tool_speed(RepairTool::Wrench).speed);
    }

    #[test]
    fn each_device_is_its_machine_kind() {
        assert_eq!(machine_of(RepairDevice::JunctionBox), Machine::Electrical);
        assert_eq!(machine_of(RepairDevice::Reactor), Machine::Hot);
        assert_eq!(machine_of(RepairDevice::CoolantPump), Machine::Mechanical);
    }

    #[test]
    fn each_machine_takes_its_tool() {
        assert_eq!(tool_for(RepairDevice::Reactor), RepairTool::Wrench);
        assert_eq!(tool_for(RepairDevice::CoolantPump), RepairTool::Wrench);
        assert_eq!(tool_for(RepairDevice::JunctionBox), RepairTool::Screwdriver);
        assert_eq!(tool_of_effect("repair_mechanical"), Some(RepairTool::Wrench));
        assert_eq!(tool_of_effect("repair_electrical"), Some(RepairTool::Screwdriver));
        assert_eq!(tool_of_effect("bandage"), None);
    }
}
