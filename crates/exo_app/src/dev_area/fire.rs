//! The fire (#298): fires start in the engine room's air (`fire_core::Deck`, stepped by the host),
//! burn the people in reach (their health takes the burn at the fire's rate, by its size: a
//! small fire less than the hot plate, a big one more, `health_core::HeatTuning::fire_per_unit`),
//! burn the pump, the junction box and the reactor's condition by the fire's size, and
//! spread through the open door. The extinguisher sprays while LMB is held with the aim in hand:
//! a cone from the eye, about 4 m, half-angle 20 degrees, and the host puts out what it lands on.
//!
//! - Values: `content/gameplay/fire/fire.json` (`fire_core::tuning`, strict loader). The spray's
//!   reach and half-angle are in the same file (`spray_reach`, `spray_half_angle_deg`).
//! - Host and one process: `spray_step` takes the sprays' requests (`Items::spray_asks`), `fire_step`
//!   steps the air, applies the burns and sets the ship's fire alarm (`FireAlarm`, which the reactor
//!   step takes the higher of). The snapshot (`items.fire`: fires as id, floor point and size, and
//!   the sprays) goes to the clients about 4 times a second while it changes.
//! - A client sends its spray's aim (`spray_input`, at most about 10 times a second while it
//!   changes) and draws the snapshot. Its own air is not stepped.
//! - Window only: a flickering flame, a warm glow on the floor and a point light per fire (`effects`:
//!   the light flickers per frame and reaches further as the fire grows), a white jet from each
//!   spray's nozzle to where it lands, and steam where a spray lands (`effects`). The crackle is not made yet (`TODO`: a quiet noise sound in `audio.rs`).
//! - Two models (#300, `FireModel`, a dev switch on the faults screen): the point fires above, or
//!   the cells of `fire_core::grid` (`FireGrid`, the engine room's and the corridor's cells, one
//!   per metre). The host decides and the snapshot says which (`FireNews::grid`); the grid's
//!   burning cells are its `cells`, and in the grid model `fires` lists them as cell-wide fires
//!   for the aim and the steam. Its flames are the placeholder blocks of `grid_sync`.
//! - The extinguisher wears down while it sprays and an empty one does not spray; the racks give full ones (`dev_area::extinguisher`, #300).
use crate::controls::Controls;
use crate::dev_area::engine_room::{footprint_of, ignition_at, place_of, source_at, EngineAir};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::reactor_screen::FAULTS_ID;
use crate::dev_area::engine_room::{CORRIDOR, ROOM};
use crate::dev_area::{area_of, DevArea};
use crate::displays::{ActiveDisplay, DisplayAction};
use crate::env::PlanetRes;
use crate::health::{area_pos, walker_points};
use crate::items::{Items, NetOut, Role};
use crate::net_live::RemoteWalker;
use crate::origin::{RenderOrigin, WorldPose};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use fire_core::tuning::FireSettings;
use fire_core::grid::{BurningCell, Grid, GridEvent};
use fire_core::{cone_landing, Fire, FireEvent, Target, TargetKind};
use item_core::wire::{Alarm, CellNew, FireNew, FireNews, SprayAsk, SprayNew};
use item_core::PlayerId;
use reactor_core::{ReactorEvent, RoomEvent, Source};
use std::collections::{BTreeMap, BTreeSet};
use walker_core::Frame as WalkFrame;

/// The extinguisher's `use` in its item file.
pub const EXTINGUISH: &str = "extinguish";
/// The flame's centre above the floor (m), for the spray's aim at it.
const FLAME_LIFT: f64 = 0.3;
/// Most sprays drawn at once.
const MAX_SPRAYS: usize = 4;
/// Seconds between two snapshots of changed fires (4 Hz), and between two spray aims (10 Hz).
pub const SEND_EVERY_S: f64 = 0.25;
pub const AIM_EVERY_S: f64 = 0.1;
/// The deck's targets for the devices: ids above any player's.
const DEVICE_ID: u64 = 1 << 32;

/// The fire values as loaded.
#[derive(Resource, Clone, Debug)]
pub struct FireConfig(pub FireSettings);

/// The ship's alarm from the fire this step: None, Warn, or Danger (a big fire in the engine room
/// or the corridor).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FireAlarm(pub Alarm);

/// Which fire model runs (#300): the point fires of `Deck` or the cells of `fire_core::grid`. A dev
/// switch on the faults screen; the host decides, and its snapshot says which one it runs.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireModel {
    #[default]
    Point,
    Grid,
}

impl FireModel {
    pub fn toggled(self) -> FireModel {
        match self {
            FireModel::Point => FireModel::Grid,
            FireModel::Grid => FireModel::Point,
        }
    }
}

/// The cells of the engine room and the corridor (#300): their rooms' rectangles, set up once.
#[derive(Resource)]
pub struct FireGrid(pub Grid);

/// A cell's id in the snapshot's `fires` (the grid model): its room and its index.
fn cell_id(c: &BurningCell) -> u64 {
    ((c.room.0 as u64) << 32) | c.index as u64
}

/// The burn of the fire on each player this step, per second (`fire_core`'s rate): their body
/// takes it (`health`).
#[derive(Resource, Default, Debug)]
pub struct FireHeat(pub BTreeMap<PlayerId, f64>);

/// Host: the sprays in progress, by player (their nozzle and aim).
#[derive(Resource, Default, Debug)]
pub struct Sprays(pub BTreeMap<PlayerId, SprayNew>);

impl Sprays {
    /// The player left: their spray goes.
    pub fn forget(&mut self, player: PlayerId) {
        self.0.remove(&player);
    }
}

/// Host: what the last snapshot sent was, and when.
#[derive(Resource, Default)]
struct FireSend {
    sent: Option<FireNews>,
    since: f64,
}

/// Client: whether this process is spraying, the aim it last sent, and when.
#[derive(Resource, Default)]
struct SprayClient {
    on: bool,
    last: Option<SprayAsk>,
    since: f64,
}

/// The fire a reactor or room event starts: its source and size.
pub fn start_of(e: &RoomEvent) -> Option<(Source, f64)> {
    match *e {
        RoomEvent::Reactor(ReactorEvent::Fire { size }) => Some((Source::Reactor, size)),
        RoomEvent::Fire { size, source } => Some((source, size)),
        _ => None,
    }
}

/// Starts a fire at a source's front face in the engine room (or joins one it touches): a point
/// fire of `size` in the point model, a burning cell at the grid's start intensity in the grid.
pub fn ignite(air: &mut EngineAir, grid: &mut FireGrid, model: FireModel, source: Source, size: f64) {
    let p = ignition_at(source);
    let at = DVec3::new(p[0], p[1], 0.0);
    match model {
        FireModel::Point => {
            air.deck.ignite(air.engine, at, size);
        }
        FireModel::Grid => {
            let start = grid.0.tuning().start_intensity;
            grid.0.ignite(air.engine, at, start);
        }
    }
}

fn device_id(s: Source) -> u64 {
    DEVICE_ID
        + match s {
            Source::Pump => 1,
            Source::Junction => 2,
            Source::Reactor => 3,
        }
}

/// The floor point of the fire's centre in the world, with its lift (`area`: the dev area is on).
fn flame_world(area: &DevArea, planet: &PlanetRes, p: [f64; 2]) -> Option<(DVec3, DVec3)> {
    area.ground(planet, p, FLAME_LIFT)
}

/// Window: where a spray lands on the fires in a snapshot (the same cone as `spray_step`), the
/// point of its aim nearest the fire it hits first. The steam puffs sit there.
pub(crate) fn landing_of(area: &DevArea, planet: &PlanetRes, cfg: &FireSettings, s: &SprayNew, fires: &[FireNew]) -> Option<DVec3> {
    let eye = DVec3::from_array(s.eye);
    let dir = DVec3::from_array(s.dir).normalize_or_zero();
    if dir == DVec3::ZERO {
        return None;
    }
    let half = cfg.spray_half_angle_deg.to_radians();
    let best = fires
        .iter()
        .filter_map(|f| {
            let (centre, _) = flame_world(area, planet, f.pos)?;
            cone_landing(eye, dir, centre, f.size * 0.5, cfg.spray_reach, half)
        })
        .fold(None, |b: Option<f64>, d| Some(b.map_or(d, |b| b.min(d))));
    best.map(|d| eye + dir * d)
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let settings = FireSettings::load(&files).unwrap_or_else(|e| panic!("fire: {e:?}"));
    let mut grid = Grid::new(settings.grid, 0x300);
    let ids = EngineAir::new();
    for (room, zone) in [(ids.engine, ROOM), (ids.corridor, CORRIDOR)] {
        grid.add_room(room, DVec2::new(zone.centre[0] - zone.half[0], zone.centre[1] - zone.half[1]), DVec2::new(zone.half[0] * 2.0, zone.half[1] * 2.0));
    }
    app.insert_resource(FireConfig(settings))
        .insert_resource(FireGrid(grid))
        .init_resource::<FireModel>()
        .init_resource::<FireAlarm>()
        .init_resource::<FireHeat>()
        .init_resource::<Sprays>()
        .init_resource::<FireSend>()
        .init_resource::<SprayClient>();
    app.add_systems(FixedUpdate, spray_input.after(crate::scenario::run_script).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, model_click.in_set(crate::phases::Fx::Effects));
    app.add_systems(FixedUpdate, (spray_step, fire_step).chain().after(super::engine_room::room_step).after(spray_input).after(crate::items_net::items_net_in).in_set(crate::phases::Fx::Input));
}

/// The faults screen's model button: toggles the model. A client's snapshot sets it again.
fn model_click(mut clicks: MessageReader<DisplayAction>, mut model: ResMut<FireModel>) {
    for c in clicks.read() {
        if c.panel == FAULTS_ID && c.action == "fire_model" {
            *model = model.toggled();
        }
    }
}

/// The extinguisher's aim as this process has it now: LMB held, the mouse captured, no display in
/// use, on foot outside a ship, and the extinguisher in the selected slot. None otherwise.
fn spray_aim(controls: &Controls, active: &ActiveDisplay, players: &Query<&Player>, items: &Items) -> Option<SprayAsk> {
    if controls.free_cursor || !controls.primary_held || active.using().is_some() {
        return None;
    }
    let pl = players.single().ok()?;
    if pl.ship.is_some() || pl.seated || pl.fly {
        return None;
    }
    let v = &items.client.view;
    let stack = v.slot(v.selected)?;
    if items.defs.of(stack)?.use_effect.as_deref() != Some(EXTINGUISH) {
        return None;
    }
    // An empty one does not spray (#300).
    if !stack.item.condition.is_some_and(|c| c > 0.0) {
        return None;
    }
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let dir = pl.world_look(WalkFrame::IDENTITY).normalize_or_zero();
    Some(SprayAsk::Aim { eye: eye.to_array(), dir: dir.to_array() })
}

/// Every process: a spray's start and aim go to the host (a client sends them), and its stop
/// when LMB is let go. The aim is sent at most every `AIM_EVERY_S` while it changes.
fn spray_input(time: Res<Time>, controls: Res<Controls>, active: Res<ActiveDisplay>, players: Query<&Player>, mut items: ResMut<Items>, mut client: ResMut<SprayClient>) {
    let now = time.elapsed_secs_f64();
    match spray_aim(&controls, &active, &players, &items) {
        Some(aim) => {
            if !client.on || (client.last != Some(aim) && now - client.since >= AIM_EVERY_S) {
                items.ask_spray(aim);
                client.on = true;
                client.last = Some(aim);
                client.since = now;
            }
        }
        None => {
            if client.on {
                items.ask_spray(SprayAsk::Stop);
            }
            *client = SprayClient::default();
        }
    }
}

/// Host: the sprays' requests, then each spray in reach puts out the fires it lands on, for this
/// tick. The spray lands at the point of its aim nearest the fire it hits first.
fn spray_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut items: ResMut<Items>, mut sprays: ResMut<Sprays>, mut air: ResMut<EngineAir>, mut grid: ResMut<FireGrid>, model: Res<FireModel>, cfg: Res<FireConfig>, supply: Res<super::extinguisher::Supply>) {
    if items.role == Role::Client {
        return;
    }
    for (who, a) in std::mem::take(&mut items.spray_asks) {
        match a {
            SprayAsk::Aim { eye, dir } => {
                sprays.0.insert(who, SprayNew { eye, dir });
            }
            SprayAsk::Stop => {
                sprays.0.remove(&who);
            }
        }
    }
    let dt = time.delta_secs_f64();
    // The extinguisher runs down while it sprays; an empty one (or anything that does not wear)
    // stops the spray (#300).
    let now = time.elapsed_secs_f64();
    for who in sprays.0.keys().copied().collect::<Vec<_>>() {
        let slot = items.host.inventories.get(&who).map_or(0, |i| i.selected);
        match items.host.condition(who, slot) {
            Some(c) if c > 0.0 => items.wear(now, who, slot, supply.0.drain_per_s * dt),
            _ => {
                sprays.0.remove(&who);
            }
        }
    }
    let (Some(area), Some(frame)) = (area.as_deref(), area.as_deref().and_then(|a| a.frame)) else { return };
    let half = cfg.0.spray_half_angle_deg.to_radians();
    let reach = cfg.0.spray_reach;
    // What a spray aims at: the point fires, or each burning cell as a square of its width.
    let fires: Vec<([f64; 2], f64)> = match *model {
        FireModel::Point => air.deck.fires().iter().map(|f| ([f.pos.x, f.pos.y], f.size)).collect(),
        FireModel::Grid => grid.0.burning().map(|c| ([c.centre.x, c.centre.y], cfg.0.grid.cell_m)).collect(),
    };
    let mut landings = Vec::new();
    for s in sprays.0.values() {
        let eye = DVec3::from_array(s.eye);
        let dir = DVec3::from_array(s.dir).normalize_or_zero();
        if dir == DVec3::ZERO {
            continue;
        }
        let mut best: Option<f64> = None;
        for (p, size) in &fires {
            let Some((centre, _)) = flame_world(area, &planet, *p) else { continue };
            if let Some(d) = cone_landing(eye, dir, centre, size * 0.5, reach, half) {
                best = Some(best.map_or(d, |b: f64| b.min(d)));
            }
        }
        if let Some(d) = best {
            let at = eye + dir * d;
            landings.push(area_of(&frame, &planet, at));
        }
    }
    for p in landings {
        let at = DVec3::new(p[0], p[1], 0.0);
        match *model {
            FireModel::Point => {
                let _ = air.deck.spray(at, dt);
            }
            FireModel::Grid => {
                let _ = grid.0.spray(at, dt);
            }
        }
    }
}

/// The burns of a device (the deck's damage is its condition lost this step).
fn burn_device(sim: &mut ReactorSim, id: u64, amount: f64) {
    let event = match id - DEVICE_ID {
        1 => sim.room.pump_mut().condition_mut().damage(amount),
        2 => sim.room.junction_mut().condition_mut().damage(amount),
        3 => sim.room.reactor_mut().condition_mut().damage(amount),
        _ => None,
    };
    if let Some(e) = event {
        info!("fire: a device {id} {e:?}");
    }
}

/// One burn of the fire: a person's heat for this step (`FireHeat`), or a device's condition.
fn burn(target: u64, kind: TargetKind, amount: f64, dt: f64, heat: &mut FireHeat, sim: &mut ReactorSim) {
    match kind {
        TargetKind::Person if amount > 0.0 && dt > 0.0 => {
            *heat.0.entry(target as PlayerId).or_insert(0.0) += amount / dt;
        }
        TargetKind::Person => {}
        TargetKind::Device => burn_device(sim, target, amount),
    }
}

/// The snapshot's fires and cells of this step: the point fires, or the grid's burning cells.
fn news_of(model: FireModel, air: &EngineAir, grid: &FireGrid, sprays: &Sprays) -> FireNews {
    let sprays: Vec<SprayNew> = sprays.0.values().copied().collect();
    match model {
        FireModel::Point => FireNews {
            fires: air.deck.fires().iter().map(|f| FireNew { id: f.id.0, pos: [f.pos.x, f.pos.y], size: (f.size * 100.0).round() / 100.0 }).collect(),
            sprays,
            grid: false,
            cells: Vec::new(),
        },
        FireModel::Grid => {
            let cells: Vec<BurningCell> = grid.0.burning().collect();
            let cell_m = grid.0.tuning().cell_m;
            FireNews {
                fires: cells.iter().map(|c| FireNew { id: cell_id(c), pos: [c.centre.x, c.centre.y], size: cell_m }).collect(),
                sprays,
                grid: true,
                cells: cells.iter().map(|c| CellNew { room: c.room.0 as u32, cell: c.index as u32, heat: (c.intensity * 1000.0).round() as u16 }).collect(),
            }
        }
    }
}

/// Host: the people and the devices in reach are targets; the air steps; the burns go to the
/// players' heat and to the devices; the alarm and the snapshot. A client only takes the model
/// and draws the snapshot. Its own air is not stepped.
#[allow(clippy::too_many_arguments)]
fn fire_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut air: ResMut<EngineAir>, cfg: Res<FireConfig>, mut sim: ResMut<ReactorSim>, mut items: ResMut<Items>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut heat: ResMut<FireHeat>, mut alarm: ResMut<FireAlarm>, sprays: Res<Sprays>, mut send: ResMut<FireSend>, mut model: ResMut<FireModel>, mut grid: ResMut<FireGrid>) {
    heat.0.clear();
    if items.role == Role::Client {
        if let Some(n) = items.fire.as_ref() {
            let m = if n.grid { FireModel::Grid } else { FireModel::Point };
            if *model != m {
                *model = m;
            }
        }
        return;
    }
    let dt = time.delta_secs_f64();
    let mut targets = Vec::new();
    if let Some(area) = area.as_deref() {
        for (id, p) in walker_points(&players, &remotes) {
            if let Some(q) = area_pos(Some(area), &planet, Some(p)) {
                targets.push(Target { id: id as u64, kind: TargetKind::Person, room: air.room_of(place_of(q)), pos: DVec3::new(q[0], q[1], 0.0), half: DVec2::ZERO });
            }
        }
    }
    for s in [Source::Pump, Source::Junction, Source::Reactor] {
        let p = source_at(s);
        let h = footprint_of(s);
        targets.push(Target { id: device_id(s), kind: TargetKind::Device, room: air.engine, pos: DVec3::new(p[0], p[1], 0.0), half: DVec2::new(h[0], h[1]) });
    }
    // The point fires burn the targets in `Deck::step`; the cells burn them in `Grid::step`, after
    // the air has moved (`Deck::step` with no fires only moves it).
    let point = *model == FireModel::Point;
    let point_targets: &[Target] = if point { &targets } else { &[] };
    for e in air.deck.step(dt, point_targets) {
        match e {
            FireEvent::Damage { target, kind, amount } => burn(target, kind, amount, dt, &mut heat, &mut sim),
            FireEvent::Spread { room, .. } => info!("fire: spread into room {room:?}"),
            FireEvent::Out { room, .. } => info!("fire: out in room {room:?}"),
        }
    }
    if !point {
        for e in grid.0.step(&mut air.deck, dt, &targets) {
            match e {
                GridEvent::Damage { target, kind, amount } => burn(target, kind, amount, dt, &mut heat, &mut sim),
                GridEvent::Spread { room, cell } => info!("fire: a cell caught in room {room:?}, cell {cell}"),
                GridEvent::Out { room, cell } => info!("fire: a cell out in room {room:?}, cell {cell}"),
            }
        }
    }

    let level = if point {
        let inside = |f: &Fire| f.room == air.engine || f.room == air.corridor;
        let mut level = Alarm::None;
        for f in air.deck.fires().iter().filter(|f| inside(f)) {
            level = level.max(if f.size >= cfg.0.alarm_danger_size { Alarm::Danger } else { Alarm::Warn });
        }
        level
    } else {
        let total = grid.0.total(air.engine) + grid.0.total(air.corridor);
        if total <= 0.0 {
            Alarm::None
        } else if total >= cfg.0.grid.alarm_danger_total {
            Alarm::Danger
        } else {
            Alarm::Warn
        }
    };
    alarm.0 = level;

    let news = news_of(*model, &air, &grid, &sprays);
    items.fire = Some(news.clone());
    send.since += dt;
    if items.role == Role::Host && send.sent.as_ref() != Some(&news) && send.since >= SEND_EVERY_S {
        items.net_out.push(NetOut::Fire { to: None, f: news.clone() });
        send.sent = Some(news);
        send.since = 0.0;
    }
}

/// A flame's half width (m) and height (m) by its size: 0.6 and 1.6 times the size, never under
/// 0.3 m wide or 0.6 m tall, so a small fire still shows from 3 to 5 m away.
pub fn flame_dims(size: f64) -> (f64, f64) {
    ((size * 0.6).max(0.3), (size * 1.6).max(0.6))
}

/// The flame as drawn: its height (m) is at most this, so a big fire does not fill the view (the
/// physical size keeps its `flame_dims`). `TODO(initiator)`: the start value.
pub const FLAME_VIEW_MAX_H: f64 = 2.0;

/// A flame's drawn half width and height (m): `flame_dims`, scaled down to `FLAME_VIEW_MAX_H`.
pub fn flame_view(size: f64) -> (f64, f64) {
    let (hw, h) = flame_dims(size);
    let k = (FLAME_VIEW_MAX_H / h).min(1.0);
    (hw * k, h * k)
}

/// The warm glow on the floor under a fire: its radius (m), never under 0.6 m.
pub fn glow_radius(size: f64) -> f64 {
    (size * 1.5).max(0.6)
}

/// The jet's particles per spray, and the seconds one particle takes to cross it.
pub const JET_PARTICLES: usize = 8;
pub const JET_LIFE_S: f64 = 0.4;

/// The nozzle: the held extinguisher's tip, about 0.5 m ahead of the eye and 0.3 m down (`up`:
/// the local up). An approximation of the hand (`TODO`: the held item's own tip, `items`).
pub fn nozzle_of(eye: DVec3, dir: DVec3, up: DVec3) -> DVec3 {
    eye + dir * 0.5 - up * 0.3
}

/// A jet particle's place: `phase` (0 at the nozzle, 1 at `end`) along the jet from `nozzle` to
/// `end`, its cone widening to `half_angle` (radians), turned `spin` radians round the jet. At
/// phase 0 the particle is at the nozzle; a jet with no length stays there.
pub fn jet_point(nozzle: DVec3, end: DVec3, half_angle: f64, phase: f64, spin: f64) -> DVec3 {
    let axis = end - nozzle;
    let dir = axis.normalize_or_zero();
    if dir == DVec3::ZERO {
        return nozzle;
    }
    let along = axis.length() * phase;
    let u = dir.any_orthonormal_vector();
    let v = dir.cross(u);
    let r = along * half_angle.tan();
    nozzle + dir * along + (u * spin.cos() + v * spin.sin()) * r
}

/// Particle `i` of a jet at time `t` (s): its phase 0..1, a new one each `JET_LIFE_S / JET_PARTICLES`.
pub fn jet_phase(t: f64, i: usize) -> f64 {
    (t / JET_LIFE_S + i as f64 / JET_PARTICLES as f64).fract()
}

/// Particle `i`'s turn round its jet (radians): the golden angle apart, so they fill the cone.
pub fn jet_spin(i: usize) -> f64 {
    i as f64 * 2.399_963_229_728_653
}

/// The sprays to draw: the snapshot's, with this process's own spray (its local aim, a snapshot
/// behind) replacing the entry whose nozzle is within 0.5 m of its eye, or added when there is none.
pub fn jet_sprays(snapshot: &[SprayNew], local: Option<SprayNew>) -> Vec<SprayNew> {
    let mut out = snapshot.to_vec();
    if let Some(l) = local {
        let le = DVec3::from_array(l.eye);
        match out.iter().position(|s| DVec3::from_array(s.eye).distance(le) < 0.5) {
            Some(i) => out[i] = l,
            None => out.push(l),
        }
    }
    out
}

#[cfg(test)]
mod visual_tests {
    use super::*;

    #[test]
    fn the_model_switch_toggles_and_a_cell_block_is_at_most_a_metre() {
        assert_eq!(FireModel::default(), FireModel::Point);
        assert_eq!(FireModel::Point.toggled(), FireModel::Grid);
        assert_eq!(FireModel::Grid.toggled(), FireModel::Point);
        for i in 0..50 {
            let h = cell_height(1.0, i as f64 * 0.1, i);
            assert!((0.0..=1.0).contains(&h), "{h}");
        }
        assert_eq!(cell_height(0.0, 1.0, 3), 0.0, "a cell out has no block");
    }

    #[test]
    fn flames_have_a_minimum_and_grow_with_the_fire() {
        let (hw, h) = flame_dims(0.5);
        assert!(h >= 0.6 && hw >= 0.3, "the start flame is {h:.2} m tall");
        assert_eq!(flame_dims(0.05), (0.3, 0.6), "a tiny fire is at the minimum");
        assert!(flame_dims(2.0).1 > flame_dims(0.5).1);
        assert!(flame_view(5.0).1 <= FLAME_VIEW_MAX_H + 1e-9, "a big fire is drawn at most the view height");
        assert_eq!(flame_view(0.5), flame_dims(0.5), "a small fire is drawn at its size");
        assert!(glow_radius(0.05) >= 0.6 && glow_radius(2.0) > glow_radius(0.5));
    }

    #[test]
    fn a_jet_starts_at_the_nozzle_and_reaches_its_end_along_the_axis() {
        let nozzle = DVec3::new(1.0, 2.0, 0.5);
        let end = nozzle + DVec3::new(0.0, 0.0, 4.0);
        assert_eq!(jet_point(nozzle, end, 0.3, 0.0, 1.0), nozzle);
        let far = jet_point(nozzle, end, 0.3, 1.0, 0.0);
        assert!((far.z - (nozzle.z + 4.0)).abs() < 1e-9, "the far particle is at the jet's length");
        let lateral = (far.x - nozzle.x).hypot(far.y - nozzle.y);
        assert!((lateral - 4.0 * 0.3f64.tan()).abs() < 1e-9, "its radius is the cone's at the end");
    }

    #[test]
    fn jet_particles_stay_in_the_cone_and_widen() {
        let nozzle = DVec3::ZERO;
        let end = DVec3::new(3.0, 0.0, 0.0);
        let half = 20f64.to_radians();
        let mut last_r = 0.0;
        for k in 1..=10 {
            let phase = k as f64 / 10.0;
            for i in 0..JET_PARTICLES {
                let p = jet_point(nozzle, end, half, phase, jet_spin(i));
                let r = (p.y * p.y + p.z * p.z).sqrt();
                assert!((p.x - 3.0 * phase).abs() < 1e-9);
                assert!(r <= p.x * half.tan() + 1e-9, "outside the cone at phase {phase}");
            }
            let r = (jet_point(nozzle, end, half, phase, 0.0).y).abs();
            assert!(r > last_r, "the cone widens with the phase");
            last_r = r;
        }
        // A jet with no length stays at the nozzle.
        assert_eq!(jet_point(nozzle, nozzle, half, 0.5, 0.3), nozzle);
    }

    #[test]
    fn jet_particles_are_spread_in_time_and_round_the_jet() {
        let mut phases: Vec<f64> = (0..JET_PARTICLES).map(|i| jet_phase(1.0, i)).collect();
        assert!(phases.iter().all(|p| (0.0..1.0).contains(p)));
        phases.sort_by(|a, b| a.total_cmp(b));
        phases.dedup();
        assert_eq!(phases.len(), JET_PARTICLES, "no two particles share a phase");
        assert!(jet_spin(1) - jet_spin(0) > 1.0);
    }

    #[test]
    fn the_nozzle_is_in_front_of_and_below_the_eye() {
        let eye = DVec3::new(0.0, 0.0, 1.6);
        let n = nozzle_of(eye, DVec3::X, DVec3::Z);
        assert!((n.x - 0.5).abs() < 1e-9 && (n.z - 1.3).abs() < 1e-9);
    }

    #[test]
    fn the_local_spray_replaces_its_snapshot_entry_or_is_added() {
        let a = SprayNew { eye: [0.0, 0.0, 1.6], dir: [1.0, 0.0, 0.0] };
        let b = SprayNew { eye: [10.0, 0.0, 1.6], dir: [0.0, 1.0, 0.0] };
        let snap = [a, b];
        let local = SprayNew { eye: [0.1, 0.0, 1.6], dir: [0.0, 0.0, 1.0] };
        assert_eq!(jet_sprays(&snap, Some(local)), vec![local, b], "the snapshot's own entry is replaced");
        let far = SprayNew { eye: [5.0, 5.0, 1.6], dir: [1.0, 0.0, 0.0] };
        assert_eq!(jet_sprays(&snap, Some(far)).len(), 3);
        assert_eq!(jet_sprays(&snap, None), snap.to_vec());
    }
}

// ---------- window only ----------

/// A flame on screen: one per fire, found by the fire's id in `FlameVisuals`.
#[derive(Component)]
struct Flame;

/// The warm glow on the floor under a flame.
#[derive(Component)]
struct Glow;

/// A particle of a spray's jet on screen.
#[derive(Component)]
struct Jet;

/// Window: the flames and their glows by fire id, and the jet particles (made once).
#[derive(Resource, Default)]
struct FlameVisuals {
    by_id: BTreeMap<u64, Entity>,
    glows: BTreeMap<u64, Entity>,
    jets: Vec<Entity>,
}

pub fn window_plugin(app: &mut App) {
    app.init_resource::<FlameVisuals>().init_resource::<GridVisuals>();
    app.add_systems(Update, (flames_sync, grid_sync).in_set(crate::phases::Frame::Sync));
}

/// Window: each fire is a flickering flame on a warm glow, scaled by its size, with a flickering
/// light. Each spray shows a jet of white particles from its nozzle along the aim to where it
/// lands. Positions follow the render origin.
#[allow(clippy::too_many_arguments)]
fn flames_sync(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    area: Option<Res<DevArea>>,
    items: Res<Items>,
    (client, cfg): (Res<SprayClient>, Res<FireConfig>),
    origin: Res<RenderOrigin>,
    mut vis: ResMut<FlameVisuals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flames: Query<(&mut Transform, &mut PointLight), (With<Flame>, Without<Glow>, Without<Jet>)>,
    mut glows: Query<&mut Transform, (With<Glow>, Without<Flame>, Without<Jet>)>,
    mut jets: Query<(&mut Transform, &mut Visibility), (With<Jet>, Without<Flame>, Without<Glow>)>,
) {
    // In the grid model the flames are the cells (`grid_sync`); the snapshot's fires are the cells
    // then, kept for the sprays' aim and the steam.
    let all: &[FireNew] = items.fire.as_ref().map_or(&[], |n| n.fires.as_slice());
    let grid = items.fire.as_ref().is_some_and(|n| n.grid);
    let fires: &[FireNew] = if grid { &[] } else { all };
    let snapshot: &[SprayNew] = items.fire.as_ref().map_or(&[], |n| n.sprays.as_slice());
    let (Some(area), Some(frame)) = (area.as_deref(), area.as_deref().and_then(|a| a.frame)) else { return };
    let t = time.elapsed_secs_f64();
    let to_render = |w: DVec3| (w - origin.origin).as_vec3();

    let alive: BTreeSet<u64> = fires.iter().map(|f| f.id).collect();
    vis.by_id.retain(|id, e| {
        if alive.contains(id) {
            true
        } else {
            commands.entity(*e).despawn();
            false
        }
    });
    vis.glows.retain(|id, e| {
        if alive.contains(id) {
            true
        } else {
            commands.entity(*e).despawn();
            false
        }
    });
    if vis.jets.is_empty() {
        let mesh = meshes.add(Sphere::new(1.0));
        let mat = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.4), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
        for _ in 0..MAX_SPRAYS * JET_PARTICLES {
            let e = commands.spawn((Jet, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden)).id();
            vis.jets.push(e);
        }
    }
    for f in fires {
        let Some((ground, up)) = flame_world(area, &planet, f.pos) else { continue };
        let k = 0.9 + 0.1 * (t * 17.0 + f.id as f64).sin();
        let (hw, h) = flame_view(f.size);
        let h = h * k;
        let rot = frame.rotation(up, 0.0).as_quat();
        // The flame's base sits on the floor; the cone's mesh is centred and 1 m tall.
        let centre = ground + up * (h * 0.5 - 0.05);
        let scale = Vec3::new((2.0 * hw) as f32, h as f32, (2.0 * hw) as f32);
        // The light flickers per frame (0.8 to 1.0) and reaches further as the fire grows.
        let light = 30_000.0 * f.size as f32 * crate::effects::flame_flicker(t.to_bits() ^ f.id.wrapping_mul(0x9E37_79B9));
        let range = crate::effects::flame_range(f.size);
        // The glow: a flat disc on the floor, flickering in size (the disc's mesh has radius 1).
        let glow_r = (glow_radius(f.size) * k) as f32;
        let glow_tr = Transform {
            translation: to_render(ground + up * 0.03),
            rotation: rot * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(glow_r),
            ..default()
        };
        if let Some(&e) = vis.by_id.get(&f.id) {
            if let Ok((mut tr, mut l)) = flames.get_mut(e) {
                tr.translation = to_render(centre);
                tr.rotation = rot;
                tr.scale = scale;
                l.intensity = light;
                l.range = range;
            }
            if let Some(&g) = vis.glows.get(&f.id)
                && let Ok(mut tr) = glows.get_mut(g)
            {
                *tr = glow_tr;
            }
        } else {
            let mesh = meshes.add(Cone::new(0.5, 1.0));
            // Lit, with a strong emissive: the flame reads from the far side of the room.
            let mat = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.45, 0.1, 0.7), emissive: LinearRgba::rgb(3.0, 1.0, 0.2), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
            let e = commands
                .spawn((
                    Flame,
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform { translation: to_render(centre), rotation: rot, scale, ..default() },
                    PointLight { color: Color::srgb(1.0, 0.7, 0.35), intensity: light, range, shadow_maps_enabled: false, ..default() },
                ))
                .id();
            vis.by_id.insert(f.id, e);
            let glow_mesh = meshes.add(Circle::new(1.0));
            let glow_mat = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.5, 0.12, 0.35), emissive: LinearRgba::rgb(2.0, 0.8, 0.2), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
            let g = commands.spawn((Glow, Mesh3d(glow_mesh), MeshMaterial3d(glow_mat), glow_tr)).id();
            vis.glows.insert(f.id, g);
        }
    }

    // The jets: the snapshot's sprays, and this process's own from its local aim.
    let local = match client.last {
        Some(SprayAsk::Aim { eye, dir }) if client.on => Some(SprayNew { eye, dir }),
        _ => None,
    };
    let sprays = jet_sprays(snapshot, local);
    let half = cfg.0.spray_half_angle_deg.to_radians();
    let mut shown = vec![false; vis.jets.len()];
    for (si, s) in sprays.iter().take(MAX_SPRAYS).enumerate() {
        let (eye, dir) = (DVec3::from_array(s.eye), DVec3::from_array(s.dir).normalize_or_zero());
        if dir == DVec3::ZERO {
            continue;
        }
        let up = area.ground(&planet, area_of(&frame, &planet, eye), 0.0).map_or(DVec3::Z, |g| g.1);
        let nozzle = nozzle_of(eye, dir, up);
        let end = landing_of(area, &planet, &cfg.0, s, all).unwrap_or(eye + dir * cfg.0.spray_reach);
        for i in 0..JET_PARTICLES {
            let idx = si * JET_PARTICLES + i;
            let phase = jet_phase(t, i);
            let p = jet_point(nozzle, end, half, phase, jet_spin(i));
            if let Ok((mut tr, mut v)) = jets.get_mut(vis.jets[idx]) {
                tr.translation = to_render(p);
                // A drop of 5 to 10 cm across, the far ones the larger.
                tr.scale = Vec3::splat((0.025 + 0.025 * phase) as f32);
                *v = Visibility::Inherited;
                shown[idx] = true;
            }
        }
    }
    for (i, e) in vis.jets.iter().enumerate() {
        if !shown[i]
            && let Ok((_, mut v)) = jets.get_mut(*e)
        {
            *v = Visibility::Hidden;
        }
    }
}

/// Window, grid model (#300): the placeholders of the cells. Each burning cell is a flickering
/// orange block (its height by its intensity, at most 1 m) on a dark red tint of the floor, and
/// each room with burning cells has one point light at the centroid of them. The cells are made
/// again every frame (a few dozen at most): a placeholder until the real fire art.
#[derive(Resource, Default)]
struct GridVisuals {
    spawned: Vec<Entity>,
    handles: Option<GridHandles>,
}

#[derive(Clone)]
struct GridHandles {
    block_mesh: Handle<Mesh>,
    block_mat: Handle<StandardMaterial>,
    tint_mesh: Handle<Mesh>,
    tint_mat: Handle<StandardMaterial>,
}

/// A cell's block, its floor tint, and a room's light: markers of `grid_sync`'s entities.
#[derive(Component)]
struct CellBlock;

#[derive(Component)]
struct CellTint;

#[derive(Component)]
struct CellLight;

/// The height (m) of a cell's block at intensity `v`, flickering by `t` and the cell's index: at
/// most 1 m.
pub fn cell_height(v: f64, t: f64, index: usize) -> f64 {
    let flicker = 0.85 + 0.15 * (t * 17.0 + index as f64 * 1.7).sin();
    (v.clamp(0.0, 1.0) * flicker).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn grid_sync(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    area: Option<Res<DevArea>>,
    items: Res<Items>,
    cfg: Res<FireConfig>,
    origin: Res<RenderOrigin>,
    mut vis: ResMut<GridVisuals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for e in vis.spawned.drain(..) {
        commands.entity(e).despawn();
    }
    let Some(news) = items.fire.as_ref().filter(|n| n.grid) else { return };
    let (Some(area), Some(frame)) = (area.as_deref(), area.as_deref().and_then(|a| a.frame)) else { return };
    let handles = vis.handles.get_or_insert_with(|| GridHandles {
        block_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        block_mat: materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.45, 0.1), emissive: LinearRgba::rgb(3.0, 1.0, 0.2), unlit: true, ..default() }),
        tint_mesh: meshes.add(Circle::new(1.0)),
        tint_mat: materials.add(StandardMaterial { base_color: Color::srgba(0.35, 0.02, 0.01, 0.55), unlit: true, alpha_mode: AlphaMode::Blend, ..default() }),
    })
    .clone();
    let t = time.elapsed_secs_f64();
    let to_render = |w: DVec3| (w - origin.origin).as_vec3();
    let cell_m = cfg.0.grid.cell_m;
    let mut per_room: BTreeMap<u32, (DVec3, f64, u32)> = BTreeMap::new();
    for c in &news.cells {
        let Some(id) = news.fires.iter().find(|f| f.id == (((c.room as u64) << 32) | c.cell as u64)).map(|f| f.pos) else { continue };
        let Some((ground, up)) = area.ground(&planet, id, 0.0) else { continue };
        let v = c.heat as f64 / 1000.0;
        let h = cell_height(v, t, c.cell as usize);
        let rot = frame.rotation(up, 0.0).as_quat();
        let block = commands
            .spawn((
                CellBlock,
                Mesh3d(handles.block_mesh.clone()),
                MeshMaterial3d(handles.block_mat.clone()),
                Transform { translation: to_render(ground + up * (h * 0.5)), rotation: rot, scale: Vec3::new(cell_m as f32 * 0.9, h.max(0.02) as f32, cell_m as f32 * 0.9), ..default() },
            ))
            .id();
        let tint = commands
            .spawn((
                CellTint,
                Mesh3d(handles.tint_mesh.clone()),
                MeshMaterial3d(handles.tint_mat.clone()),
                Transform { translation: to_render(ground + up * 0.03), rotation: rot * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), scale: Vec3::splat((cell_m * 0.5) as f32), ..default() },
            ))
            .id();
        vis.spawned.push(block);
        vis.spawned.push(tint);
        let r = per_room.entry(c.room).or_insert((DVec3::ZERO, 0.0, 0));
        r.0 += ground + up * 1.5;
        r.1 += v;
        r.2 += 1;
    }
    for (room, (sum, heat, n)) in per_room {
        let centre = sum / n as f64;
        let light = commands
            .spawn((
                CellLight,
                PointLight { color: Color::srgb(1.0, 0.6, 0.3), intensity: (30_000.0 * heat.min(3.0)) as f32 * crate::effects::flame_flicker(t.to_bits() ^ room as u64), range: crate::effects::flame_range(heat), shadow_maps_enabled: false, ..default() },
                Transform::from_translation(to_render(centre)),
            ))
            .id();
        vis.spawned.push(light);
    }
}

