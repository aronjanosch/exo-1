//! The engineer's corner of the dev area (#286, part of #284): the reactor with its panel, the
//! power board, a coolant pump to repair, fire in two rooms with a door between them, and an
//! extinguisher on the wall. The logic is `reactor_core`, `device_core` and `fire_core`; this
//! module places the boxes, steps the simulation, routes its events to notices and draws the
//! panels.
//!
//! Interactions (all with the interact key, F): use the reactor or power panel; hold F at the pump
//! or the reactor to repair it (skill `mechanical`); F at the wall mount takes or returns the
//! extinguisher, F held while carrying it sprays ahead; F at the door opens or closes it.
//!
//! Area frame here: `DVec3(east, up, north)` in area metres, as `fire_core` positions.
use super::*;
use crate::controls::Actions;
use crate::displays::{ActiveDisplay, DisplayAction, Panels};
use crate::gameplay::Gameplay;
use crate::interact::{Interaction, Target, Usable, Usables};
use crate::walker::{Player, EYE_HEIGHT};
use device_core::{Condition, DeviceEvent, DeviceTuning, RepairOutcome, Tool};
use fire_core::{Deck, FireEvent, FireTuning, GapId, RoomId, TargetKind};
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use reactor_core::{grid, FuelRod, Grid, Group, Mode, Reactor, ReactorEvent, ReactorTuning};
use skills::DevSkills;

mod look;
pub mod panels;

pub const REACTOR_PANEL: PanelId = PanelId::new("reactor");
pub const REACTOR_NAME: &str = "Reactor";
pub const POWER_PANEL: PanelId = PanelId::new("power");
pub const POWER_NAME: &str = "Power board";

// Usable ids (`interact::Usables`).
pub const PUMP: &str = "engineer.pump";
pub const REACTOR: &str = "engineer.reactor";
pub const EXTINGUISHER: &str = "engineer.extinguisher";
pub const DOOR: &str = "engineer.door";

// Layout in the engineering zone (m from its centre, x east, y north). Devices in a row at the
// back, consoles in front of them, a walkway between. TODO(initiator): the layout.
pub const REACTOR_AT: [f64; 2] = [0.0, 10.0];
pub const REACTOR_SIZE: [f64; 3] = [3.0, 2.5, 3.0];
pub const PUMP_AT: [f64; 2] = [-3.5, 10.0];
pub const PUMP_SIZE: [f64; 3] = [1.2, 1.2, 1.2];
pub const ROD_CRATE_AT: [f64; 2] = [-6.0, 10.0];
pub const REACTOR_CONSOLE_AT: [f64; 2] = [-2.0, 6.5];
pub const POWER_CONSOLE_AT: [f64; 2] = [-6.0, 6.5];
pub const EXTINGUISHER_AT: [f64; 2] = [2.5, 6.5];
pub const EXTINGUISHER_SIZE: [f64; 3] = [0.4, 0.9, 0.3];
pub const DOOR_AT: [f64; 2] = [5.0, 10.0];
pub const DOOR_SIZE: [f64; 3] = [0.3, 2.2, 1.6];
/// Where the reactor's fires start: its east side, towards the door.
pub const FIRE_AT: [f64; 2] = [2.2, 10.0];
/// The aux lights on posts behind the devices.
pub const LIGHTS: [[f64; 2]; 5] = [[-8.0, 14.0], [-4.0, 14.0], [0.0, 14.0], [4.0, 14.0], [8.0, 14.0]];
/// The side room lies east of the door: x from the door to here, y within `SIDE_HALF_Y` of it.
const SIDE_EAST: f64 = 12.0;
const SIDE_HALF_Y: f64 = 6.0;

/// Reach of the interact key on a device (eye to its point, m).
pub const DEVICE_RANGE: f64 = 2.2;
/// Starting values. TODO(initiator): all by feel.
pub const PUMP_START: f64 = 15.0;
pub const RODS_IN_CRATE: u32 = 4;
/// Two rods, so Auto can feed the whole placeholder load (1000 kW); one rod caps it near half.
pub const START_RODS: usize = 2;
/// Rooms: the engineering zone and the side room behind the door (m³).
pub const ROOM_VOLUME: f64 = 300.0;
pub const SIDE_VOLUME: f64 = 80.0;
/// Oxygen life support adds to the engineering room at full supply (m³/s); the side room gets air
/// only through the door.
pub const LIFE_SUPPORT_RATE: f64 = 1.0;
/// Seconds of spray in a full extinguisher, and to refill an empty one on its mount.
pub const SPRAY_S: f64 = 20.0;
pub const REFILL_S: f64 = 10.0;
/// The spray reaches this far ahead (m), at this height.
pub const SPRAY_RANGE: f64 = 3.5;
pub const SPRAY_HEIGHT: f64 = 0.5;
/// Wear runs this much faster with the power board's "fast wear" (dev) toggle.
pub const FAST_WEAR: f64 = 30.0;
/// "You are burning" at most this often (s).
const BURN_NOTICE_S: f64 = 3.0;

/// The text keys this module uses (checked against `en.json` in a test).
pub const TEXT_KEYS: &[&str] = &[
    "reactor.temp_high",
    "reactor.temp_critical",
    "reactor.fuel_low",
    "reactor.shutdown",
    "notice.engineer.fire",
    "notice.engineer.fire_spread",
    "notice.engineer.meltdown",
    "notice.engineer.burning",
    "notice.engineer.broke",
    "notice.engineer.repaired",
    "notice.engineer.repair_failed",
    "notice.engineer.locked",
    "notice.engineer.no_rods",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dev {
    Pump,
    Reactor,
}

impl Dev {
    pub fn name(self) -> &'static str {
        match self {
            Dev::Pump => "coolant pump",
            Dev::Reactor => "reactor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extinguisher {
    pub held: bool,
    /// 0..1.
    pub charge: f64,
}

/// Counters for checks and the panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub fires_started: u32,
    pub shutdowns: u32,
    /// (time, room) of each spread.
    pub spreads: Vec<(f64, RoomId)>,
    pub repairs_done: u32,
    pub repair_fails: u32,
    pub skill_gained: f64,
    pub sprayed_s: f64,
    pub burning_s: f64,
}

/// Everything the engineer's corner simulates.
#[derive(Resource, Clone, Debug)]
pub struct Engineering {
    pub reactor: Reactor,
    pub grid: Grid,
    pub deck: Deck,
    pub room: RoomId,
    pub side_room: RoomId,
    pub door: GapId,
    pub pump: Condition,
    pub rods_in_crate: u32,
    pub extinguisher: Extinguisher,
    pub fast_wear: bool,
    /// The device under the player's hands, and what the last repair step did.
    pub repairing: Option<Dev>,
    pub last_repair: Option<(Dev, RepairOutcome)>,
    pub stats: Stats,
    /// Simulated seconds.
    pub time: f64,
    burn_notice_at: f64,
    /// Fire damage to the player since the health glue last took it (#287).
    pub player_burn: f64,
}

impl Default for Engineering {
    fn default() -> Self {
        let mut reactor = Reactor::new(ReactorTuning::default(), DeviceTuning::default(), 41);
        for _ in 0..START_RODS {
            reactor.insert_rod(FuelRod::standard()).expect("an empty reactor has slots");
        }
        reactor.set_mode(Mode::Manual);
        let mut pump = Condition::new(DeviceTuning::default(), 42);
        pump.set_value(PUMP_START);
        let mut deck = Deck::new(FireTuning::default(), 43);
        let room = deck.add_room(ROOM_VOLUME);
        let side_room = deck.add_room(SIDE_VOLUME);
        let door = deck.add_gap(room, side_room, area3(DOOR_AT, 1.0), 2.0, 1.0);
        Engineering {
            reactor,
            grid: Grid::default(),
            deck,
            room,
            side_room,
            door,
            pump,
            rods_in_crate: RODS_IN_CRATE,
            extinguisher: Extinguisher { held: false, charge: 1.0 },
            fast_wear: false,
            repairing: None,
            last_repair: None,
            stats: Stats::default(),
            time: 0.0,
            burn_notice_at: f64::MIN,
            player_burn: 0.0,
        }
    }
}

impl Engineering {
    pub fn condition(&self, d: Dev) -> &Condition {
        match d {
            Dev::Pump => &self.pump,
            Dev::Reactor => self.reactor.condition(),
        }
    }
    fn condition_mut(&mut self, d: Dev) -> &mut Condition {
        match d {
            Dev::Pump => &mut self.pump,
            Dev::Reactor => self.reactor.condition_mut(),
        }
    }
    pub fn door_open(&self) -> bool {
        self.deck.gap(self.door).open > 0.0
    }
    /// The room a point (area frame) is in.
    pub fn room_at(&self, p: DVec3) -> RoomId {
        let door = ENGINEERING.point(DOOR_AT);
        let side = p.x > door[0] && p.x < ENGINEERING.point([SIDE_EAST, 0.0])[0] && (p.z - door[1]).abs() < SIDE_HALF_Y;
        if side { self.side_room } else { self.room }
    }
}

/// A point of the engineering zone in the area frame: `DVec3(east, up, north)`.
pub fn area3(local: [f64; 2], h: f64) -> DVec3 {
    let p = ENGINEERING.point(local);
    DVec3::new(p[0], h, p[1])
}

/// World position of an area-frame point.
pub fn world_of(frame: &Frame, planet: &PlanetRes, p: DVec3) -> DVec3 {
    frame.ground(planet, p.x, p.z, p.y).0
}

/// Area-frame position of a world point (height above the ground).
pub fn area_of(frame: &Frame, planet: &PlanetRes, w: DVec3) -> DVec3 {
    let d = planet.up(w) - frame.dir;
    DVec3::new(d.dot(frame.east) * planet.radius, planet.above_ground(w), d.dot(frame.north) * planet.radius)
}

fn console(id: &str, at: [f64; 2], panel: PanelId, name: &str, size_m: [f64; 2]) -> PlaceholderBox {
    PlaceholderBox {
        id: id.into(),
        zone: ENGINEERING,
        local_pos: at,
        size: [1.6, 1.0, 0.8],
        turn_deg: 0.0,
        colour: [0.36, 0.33, 0.30],
        panel: Some(PanelSpec { id: panel, name: name.into(), size_m, at: [0.0, 1.0 + size_m[1] * 0.5 + 0.05, 0.2], tilt_deg: 12.0 }),
    }
}

fn device(id: &str, at: [f64; 2], size: [f64; 3], colour: [f32; 3]) -> PlaceholderBox {
    PlaceholderBox { id: id.into(), zone: ENGINEERING, local_pos: at, size, turn_deg: 0.0, colour, panel: None }
}

pub fn plugin(app: &mut App) {
    {
        let mut d = app.world_mut().resource_mut::<DevAreaDevices>();
        d.add(device("engineer_reactor", REACTOR_AT, REACTOR_SIZE, [0.30, 0.42, 0.36]));
        d.add(device("engineer_pump", PUMP_AT, PUMP_SIZE, [0.35, 0.45, 0.6]));
        d.add(device("engineer_rod_crate", ROD_CRATE_AT, [0.9, 0.7, 0.9], [0.85, 0.75, 0.2]));
        d.add(device("engineer_extinguisher", EXTINGUISHER_AT, EXTINGUISHER_SIZE, [0.85, 0.1, 0.1]));
        d.add(device("engineer_door", DOOR_AT, DOOR_SIZE, [0.45, 0.3, 0.2]));
        d.add(console("engineer_reactor_console", REACTOR_CONSOLE_AT, REACTOR_PANEL, REACTOR_NAME, [0.84, 0.84 * panels::REACTOR_H / panels::REACTOR_W]));
        d.add(console("engineer_power_console", POWER_CONSOLE_AT, POWER_PANEL, POWER_NAME, [0.84, 0.84 * panels::POWER_H / panels::POWER_W]));
    }
    app.init_resource::<Engineering>();
    app.add_systems(FixedUpdate, offer_usables.before(crate::interact::interaction).in_set(crate::phases::Fx::Walker));
    app.add_systems(FixedUpdate, (read_clicks, step, write_panels).chain().in_set(crate::phases::Fx::Effects));
    app.add_systems(Update, look::draw.in_set(crate::phases::Frame::World));
}

/// The devices the interact key can use, with their prompts.
fn offer_usables(e: Res<Engineering>, area: Res<DevArea>, planet: Res<PlanetRes>, mut usables: ResMut<Usables>) {
    usables.0.retain(|u| !u.id.starts_with("engineer."));
    let Some(frame) = area.frame else { return };
    let at = |local: [f64; 2], h: f64, front: f64| world_of(&frame, &planet, area3([local[0], local[1] - front], h));
    let mut add = |id: &str, pos: DVec3, verb: String| usables.0.push(Usable { id: id.into(), pos, range: DEVICE_RANGE, verb });
    if !e.extinguisher.held {
        add(PUMP, at(PUMP_AT, 0.7, PUMP_SIZE[2] * 0.5), repair_verb(&e, Dev::Pump));
        add(REACTOR, at([REACTOR_AT[0] + 1.0, REACTOR_AT[1]], 1.0, REACTOR_SIZE[2] * 0.5), repair_verb(&e, Dev::Reactor));
    }
    let ext = if e.extinguisher.held {
        format!("return the extinguisher ({:.0} %)", e.extinguisher.charge * 100.0)
    } else {
        format!("take the extinguisher ({:.0} %)", e.extinguisher.charge * 100.0)
    };
    add(EXTINGUISHER, at(EXTINGUISHER_AT, 0.9, EXTINGUISHER_SIZE[2] * 0.5), ext);
    add(DOOR, at(DOOR_AT, 1.1, 0.0), if e.door_open() { "close the door".into() } else { "open the door".into() });
}

/// "hold to repair the pump  [#####-----] 48 %", or what the last step said.
pub fn repair_verb(e: &Engineering, d: Dev) -> String {
    let c = e.condition(d);
    let n = (c.fraction() * 10.0).round() as usize;
    let bar = format!("[{}{}]", "#".repeat(n), "-".repeat(10 - n));
    let state = match e.last_repair {
        Some((x, RepairOutcome::Locked { remaining_s })) if x == d => format!("  failed, wait {remaining_s:.1} s"),
        Some((x, RepairOutcome::Failed { .. })) if x == d => "  failed".into(),
        _ if !c.functioning() => "  broken".into(),
        _ if c.needs_repair() => "  needs repair".into(),
        _ => String::new(),
    };
    format!("hold to repair the {}  {bar} {:.0} %{state}", d.name(), c.fraction() * 100.0)
}

/// Where the spray lands: along the nozzle's line (ahead, `SPRAY_RANGE` long, at `SPRAY_HEIGHT`),
/// the point nearest to the nearest fire; with no fire, the end of the line.
pub fn spray_point(deck: &Deck, feet: DVec3, flat: DVec3) -> DVec3 {
    let a = DVec3::new(feet.x, SPRAY_HEIGHT, feet.z) + flat * 0.3;
    let b = a + flat * (SPRAY_RANGE - 0.3);
    let on_line = |p: DVec3| {
        let ab = b - a;
        a + ab * ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0)
    };
    deck.fires()
        .iter()
        .map(|f| on_line(f.pos))
        .min_by(|x, y| {
            let d = |p: &DVec3| deck.fires().iter().map(|f| f.pos.distance(*p) - f.size * 0.5).fold(f64::MAX, f64::min);
            d(x).total_cmp(&d(y))
        })
        .unwrap_or(b)
}

fn note(gp: &mut Gameplay, key: &str) {
    gp.notify(Notice::new(NoticeKind::Warning, key));
}

fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut e: ResMut<Engineering>, mut gp: ResMut<Gameplay>) {
    for c in clicks.read() {
        let v = c.value.map(|v| v as f64);
        if c.panel == REACTOR_PANEL {
            let (fis, turb) = e.reactor.targets();
            match (c.action.as_str(), v) {
                ("power", _) => {
                    let on = !e.reactor.power_on();
                    if !e.reactor.set_power(on) {
                        note(&mut gp, "notice.engineer.locked");
                    }
                }
                ("auto", Some(v)) => e.reactor.set_mode(if v > 0.5 { Mode::Auto } else { Mode::Manual }),
                ("fission", Some(v)) => e.reactor.set_targets(v * 100.0, turb),
                ("turbine", Some(v)) => e.reactor.set_targets(fis, v * 100.0),
                ("safety", Some(v)) => e.reactor.set_safety(v > 0.5),
                ("rod_in", _) => {
                    if e.rods_in_crate == 0 {
                        note(&mut gp, "notice.engineer.no_rods");
                    } else if e.reactor.insert_rod(FuelRod::standard()).is_ok() {
                        e.rods_in_crate -= 1;
                    }
                }
                ("rod_out", _) => {
                    while let Some(i) = e.reactor.rods().iter().position(|r| r.spent()) {
                        e.reactor.remove_rod(i);
                    }
                }
                _ => warn!("reactor panel: unknown action {c:?}"),
            }
        } else if c.panel == POWER_PANEL {
            let group = |s: &str| Group::ALL.into_iter().find(|g| panels::group_key(*g) == s);
            match (c.action.as_str(), v) {
                ("fast_wear", Some(v)) => e.fast_wear = v > 0.5,
                (a, Some(v)) if a.starts_with("limit_") => {
                    if let Some(g) = group(&a["limit_".len()..]) {
                        // The ends snap, so "off" and "full" are easy to hit.
                        e.grid.set_limit(g, if v < 0.03 { 0.0 } else if v > 0.97 { 1.0 } else { v });
                    }
                }
                (a, _) if a.starts_with("prio_") => {
                    if let Some(g) = group(&a["prio_".len()..]) {
                        let p = e.grid.group(g).priority % 5 + 1;
                        e.grid.set_priority(g, p);
                    }
                }
                _ => warn!("power panel: unknown action {c:?}"),
            }
        }
    }
}

/// One fixed step of the engineer's corner.
#[allow(clippy::too_many_arguments)]
pub fn step(
    time: Res<Time>,
    mut e: ResMut<Engineering>,
    mut gp: ResMut<Gameplay>,
    mut skills: ResMut<DevSkills>,
    actions: Res<Actions>,
    inter: Res<Interaction>,
    active: Res<ActiveDisplay>,
    displays: Query<(Entity, &crate::displays::DisplayPanel)>,
    players: Query<&Player>,
    (area, planet): (Res<DevArea>, Res<PlanetRes>),
) {
    let dt = time.delta_secs_f64();
    if dt <= 0.0 {
        return;
    }
    let Some(frame) = area.frame else { return };
    let e = &mut *e;
    e.time += dt;

    // Where the player is and looks, in the area frame (on foot only).
    let me = players.single().ok().and_then(crate::displays::eye_on_foot).map(|(eye, look)| {
        let feet = area_of(&frame, &planet, eye) - DVec3::Y * EYE_HEIGHT;
        let flat = DVec3::new(look.dot(frame.east), 0.0, look.dot(frame.north)).normalize_or_zero();
        (feet, flat)
    });

    // Taps on the extinguisher's mount and the door.
    if let Some(Target::Device(id)) = &inter.tapped {
        match id.as_str() {
            EXTINGUISHER => e.extinguisher.held = !e.extinguisher.held,
            DOOR => {
                let open = if e.door_open() { 0.0 } else { 1.0 };
                e.deck.set_open(e.door, open);
            }
            _ => {}
        }
    }

    // Hands: spray, or hold to repair.
    let target = match &inter.target {
        Some(Target::Device(id)) => Some(id.as_str()),
        _ => None,
    };
    let held = actions.interact_held;
    let mut repairing_now = None;
    if e.extinguisher.held {
        if held && target != Some(EXTINGUISHER) && e.extinguisher.charge > 0.0
            && let Some((feet, flat)) = me
        {
            let at = spray_point(&e.deck, feet, flat);
            e.deck.spray(at, dt);
            e.extinguisher.charge = (e.extinguisher.charge - dt / SPRAY_S).max(0.0);
            e.stats.sprayed_s += dt;
        }
    } else {
        e.extinguisher.charge = (e.extinguisher.charge + dt / REFILL_S).min(1.0);
        let dev = match target {
            Some(PUMP) => Some(Dev::Pump),
            Some(REACTOR) => Some(Dev::Reactor),
            _ => None,
        };
        if held && let Some(d) = dev {
            repairing_now = Some(d);
            if e.repairing.is_some_and(|x| x != d) {
                let x = e.repairing.unwrap();
                e.condition_mut(x).stop_repair();
            }
            e.repairing = Some(d);
            let skill = skills.mechanical;
            let out = e.condition_mut(d).repair_step(dt, skill, Tool::WRENCH);
            match out {
                RepairOutcome::Failed { lost } => {
                    e.stats.repair_fails += 1;
                    gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.repair_failed").arg("device", Arg::Text(d.name().into())).arg("lost", Arg::Number(lost.round() as i64)));
                }
                RepairOutcome::Done { skill_gain } => {
                    skills.mechanical = (skills.mechanical + skill_gain).min(100.0);
                    e.stats.repairs_done += 1;
                    e.stats.skill_gained += skill_gain;
                    gp.notify(Notice::new(NoticeKind::Completed, "notice.engineer.repaired").arg("device", Arg::Text(d.name().into())));
                }
                _ => {}
            }
            e.last_repair = Some((d, out));
        }
    }
    if repairing_now.is_none()
        && let Some(x) = e.repairing.take()
    {
        e.condition_mut(x).stop_repair();
        e.last_repair = None;
    }

    // The reactor and the grid. The operator's skill counts while the reactor panel is in use.
    let reactor_display = displays.iter().find(|(_, d)| d.id == REACTOR_PANEL).map(|(x, _)| x);
    let operating = active.using().is_some_and(|x| Some(x) == reactor_display);
    e.reactor.set_operator_skill(if operating { skills.electrical } else { 0.0 });
    let mut events = grid::run(&mut e.reactor, &mut e.grid, dt);

    // Wear; the dev toggle speeds it up.
    let mult = if e.fast_wear { FAST_WEAR } else { 1.0 };
    let pump_load = if e.reactor.power_on() { 1.0 } else { 0.0 };
    if let Some(DeviceEvent::Broke) = e.pump.deteriorate(dt * mult, pump_load) {
        broke(&mut gp, Dev::Pump);
    }
    if mult > 1.0 {
        let use_ = (e.reactor.fission() + e.reactor.turbine()) / 200.0;
        if let Some(ev) = e.reactor.condition_mut().deteriorate(dt * (mult - 1.0), use_) {
            events.push(ReactorEvent::Device(ev));
        }
    }

    for ev in events {
        match ev {
            ReactorEvent::Fire { size } => {
                e.deck.ignite(e.room, area3(FIRE_AT, 0.5), size);
                e.stats.fires_started += 1;
                note(&mut gp, "notice.engineer.fire");
            }
            ReactorEvent::Shutdown => e.stats.shutdowns += 1,
            ReactorEvent::Meltdown => note(&mut gp, "notice.engineer.meltdown"),
            ReactorEvent::Alarm { kind, on: true } => gp.notify(kind.notice()),
            ReactorEvent::Alarm { .. } => {}
            ReactorEvent::Device(DeviceEvent::Broke) => broke(&mut gp, Dev::Reactor),
            ReactorEvent::Device(_) => {}
        }
    }

    // Life support feeds the engineering room.
    let ls = e.grid.factor(Group::LifeSupport);
    let room = e.deck.room_mut(e.room);
    room.oxygen = (room.oxygen + LIFE_SUPPORT_RATE * ls * dt).min(room.volume);

    // Fire: the player, the reactor and the pump can burn.
    let mut targets = vec![
        fire_core::Target { id: 1, kind: TargetKind::Device, room: e.room, pos: area3(REACTOR_AT, 1.0) },
        fire_core::Target { id: 2, kind: TargetKind::Device, room: e.room, pos: area3(PUMP_AT, 0.6) },
    ];
    if let Some((feet, _)) = me {
        targets.push(fire_core::Target { id: 0, kind: TargetKind::Person, room: e.room_at(feet), pos: feet + DVec3::Y });
    }
    let mut burning = false;
    for ev in e.deck.step(dt, &targets) {
        match ev {
            FireEvent::Damage { target: 0, amount, .. } => {
                burning = true;
                e.player_burn += amount;
            }
            FireEvent::Damage { target: 1, amount, .. } => {
                if let Some(DeviceEvent::Broke) = e.reactor.condition_mut().damage(amount) {
                    broke(&mut gp, Dev::Reactor);
                }
            }
            FireEvent::Damage { target: 2, amount, .. } => {
                if let Some(DeviceEvent::Broke) = e.pump.damage(amount) {
                    broke(&mut gp, Dev::Pump);
                }
            }
            FireEvent::Damage { .. } => {}
            FireEvent::Spread { room, .. } => {
                e.stats.spreads.push((e.time, room));
                note(&mut gp, "notice.engineer.fire_spread");
            }
            FireEvent::Out { .. } => {}
        }
    }
    if burning {
        e.stats.burning_s += dt;
        if e.time - e.burn_notice_at >= BURN_NOTICE_S {
            e.burn_notice_at = e.time;
            note(&mut gp, "notice.engineer.burning");
        }
    }
}

fn broke(gp: &mut Gameplay, d: Dev) {
    gp.notify(Notice::new(NoticeKind::Warning, "notice.engineer.broke").arg("device", Arg::Text(d.name().into())));
}

fn write_panels(e: Res<Engineering>, skills: Res<DevSkills>, mut out: ResMut<Panels>) {
    out.set(REACTOR_PANEL, panels::reactor(&e, skills.electrical));
    out.set(POWER_PANEL, panels::power(&e));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_key_is_in_the_table() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../../../content/gameplay/text/en.json")).unwrap();
        let missing: Vec<_> = TEXT_KEYS.iter().filter(|k| v.get(**k).is_none()).collect();
        assert!(missing.is_empty(), "en.json lacks {missing:?}");
        for k in reactor_core::AlarmKind::ALL {
            assert!(TEXT_KEYS.contains(&k.notice().key.as_str()));
        }
    }

    #[test]
    fn the_side_room_is_behind_the_door() {
        let e = Engineering::default();
        assert_eq!(e.room_at(area3([8.0, 10.0], 0.0)), e.side_room);
        assert_eq!(e.room_at(area3([0.0, 6.0], 0.0)), e.room);
        assert_eq!(e.room_at(area3(FIRE_AT, 0.0)), e.room);
    }

    #[test]
    fn the_spray_lands_on_the_fire_ahead() {
        let mut e = Engineering::default();
        let feet = DVec3::new(0.0, 0.0, 0.0);
        let north = DVec3::Z;
        // No fire: the end of the nozzle's line.
        assert!((spray_point(&e.deck, feet, north) - DVec3::new(0.0, SPRAY_HEIGHT, SPRAY_RANGE)).length() < 1e-9);
        // A fire 3 m ahead, a little to the side: the line's point nearest to it, in its reach.
        e.deck.ignite(e.room, DVec3::new(0.4, 0.5, 3.0), 1.0);
        let at = spray_point(&e.deck, feet, north);
        assert!((at - DVec3::new(0.0, SPRAY_HEIGHT, 3.0)).length() < 1e-9, "{at:?}");
        assert_eq!(e.deck.spray(at, 0.1), 1);
        // A fire behind is not hit.
        let mut d = Deck::new(FireTuning::default(), 1);
        let r = d.add_room(100.0);
        d.ignite(r, DVec3::new(0.0, 0.5, -3.0), 1.0);
        assert_eq!(d.spray(spray_point(&d, feet, north), 0.1), 0);
    }

    #[test]
    fn the_pump_starts_broken() {
        let e = Engineering::default();
        assert!(!e.pump.functioning());
        assert!(repair_verb(&e, Dev::Pump).contains("broken"));
    }
}
