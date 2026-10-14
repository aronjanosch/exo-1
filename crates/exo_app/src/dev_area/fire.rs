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
//! - No ammo for the extinguisher yet (`TODO(initiator)`).
use crate::controls::Controls;
use crate::dev_area::engine_room::{footprint_of, ignition_at, place_of, source_at, EngineAir};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::{area_of, DevArea};
use crate::displays::ActiveDisplay;
use crate::env::PlanetRes;
use crate::health::{area_pos, walker_points};
use crate::items::{Items, NetOut, Role};
use crate::net_live::RemoteWalker;
use crate::origin::{RenderOrigin, WorldPose};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use fire_core::tuning::FireSettings;
use fire_core::{cone_landing, Fire, FireEvent, FireId, Target, TargetKind};
use item_core::wire::{Alarm, FireNew, FireNews, SprayAsk, SprayNew};
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

/// Starts a fire at a source's front face in the engine room (or joins one it touches).
pub fn ignite(air: &mut EngineAir, source: Source, size: f64) -> FireId {
    let p = ignition_at(source);
    air.deck.ignite(air.engine, DVec3::new(p[0], p[1], 0.0), size)
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
    app.insert_resource(FireConfig(settings))
        .init_resource::<FireAlarm>()
        .init_resource::<FireHeat>()
        .init_resource::<Sprays>()
        .init_resource::<FireSend>()
        .init_resource::<SprayClient>();
    app.add_systems(FixedUpdate, spray_input.after(crate::scenario::run_script).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, (spray_step, fire_step).chain().after(super::engine_room::room_step).after(spray_input).after(crate::items_net::items_net_in).in_set(crate::phases::Fx::Input));
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
fn spray_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut items: ResMut<Items>, mut sprays: ResMut<Sprays>, mut air: ResMut<EngineAir>, cfg: Res<FireConfig>) {
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
    let (Some(area), Some(frame)) = (area.as_deref(), area.as_deref().and_then(|a| a.frame)) else { return };
    let dt = time.delta_secs_f64();
    let half = cfg.0.spray_half_angle_deg.to_radians();
    let reach = cfg.0.spray_reach;
    let fires: Vec<([f64; 2], f64)> = air.deck.fires().iter().map(|f| ([f.pos.x, f.pos.y], f.size)).collect();
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
        let _ = air.deck.spray(DVec3::new(p[0], p[1], 0.0), dt);
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

/// Host: the people and the devices in reach are targets; the air steps; the burns go to the
/// players' heat and to the devices; the alarm and the snapshot. A client only draws the snapshot.
#[allow(clippy::too_many_arguments)]
fn fire_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut air: ResMut<EngineAir>, cfg: Res<FireConfig>, mut sim: ResMut<ReactorSim>, mut items: ResMut<Items>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut heat: ResMut<FireHeat>, mut alarm: ResMut<FireAlarm>, sprays: Res<Sprays>, mut send: ResMut<FireSend>) {
    heat.0.clear();
    if items.role == Role::Client {
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
    for e in air.deck.step(dt, &targets) {
        match e {
            FireEvent::Damage { target, kind: TargetKind::Person, amount } if amount > 0.0 && dt > 0.0 => {
                *heat.0.entry(target as PlayerId).or_insert(0.0) += amount / dt;
            }
            FireEvent::Damage { target, kind: TargetKind::Device, amount } => burn_device(&mut sim, target, amount),
            FireEvent::Damage { .. } => {}
            FireEvent::Spread { room, .. } => info!("fire: spread into room {room:?}"),
            FireEvent::Out { room, .. } => info!("fire: out in room {room:?}"),
        }
    }

    let inside = |f: &Fire| f.room == air.engine || f.room == air.corridor;
    let mut level = Alarm::None;
    for f in air.deck.fires().iter().filter(|f| inside(f)) {
        level = level.max(if f.size >= cfg.0.alarm_danger_size { Alarm::Danger } else { Alarm::Warn });
    }
    alarm.0 = level;

    let news = FireNews {
        fires: air.deck.fires().iter().map(|f| FireNew { id: f.id.0, pos: [f.pos.x, f.pos.y], size: (f.size * 100.0).round() / 100.0 }).collect(),
        sprays: sprays.0.values().copied().collect(),
    };
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
    app.init_resource::<FlameVisuals>();
    app.add_systems(Update, flames_sync.in_set(crate::phases::Frame::Sync));
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
    let fires: &[FireNew] = items.fire.as_ref().map_or(&[], |n| n.fires.as_slice());
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
        let end = landing_of(area, &planet, &cfg.0, s, fires).unwrap_or(eye + dir * cfg.0.spray_reach);
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
