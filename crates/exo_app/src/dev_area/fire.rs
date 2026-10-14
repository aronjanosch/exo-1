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
//! - Window only: a flickering flame and a point light per fire (`effects`: the light flickers per
//!   frame and reaches further as the fire grows), a few white puffs in front of each spray, and
//!   steam where a spray lands (`effects`). The crackle is not made yet (`TODO`: a quiet noise sound in `audio.rs`).
//! - No ammo for the extinguisher yet (`TODO(initiator)`).
use crate::controls::Controls;
use crate::dev_area::engine_room::{place_of, source_at, EngineAir};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::{area_of, DevArea};
use crate::displays::ActiveDisplay;
use crate::env::PlanetRes;
use crate::health::{area_pos, walker_points};
use crate::items::{Items, NetOut, Role};
use crate::net_live::RemoteWalker;
use crate::origin::{RenderOrigin, WorldPose};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::DVec3;
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
/// The flame's centre above the floor (m), and the spray's puffs per spray.
const FLAME_LIFT: f64 = 0.3;
const PUFFS: usize = 3;
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

/// Starts a fire at a source's place in the engine room (or joins one it touches).
pub fn ignite(air: &mut EngineAir, source: Source, size: f64) -> FireId {
    let p = source_at(source);
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
                targets.push(Target { id: id as u64, kind: TargetKind::Person, room: air.room_of(place_of(q)), pos: DVec3::new(q[0], q[1], 0.0) });
            }
        }
    }
    for s in [Source::Pump, Source::Junction, Source::Reactor] {
        let p = source_at(s);
        targets.push(Target { id: device_id(s), kind: TargetKind::Device, room: air.engine, pos: DVec3::new(p[0], p[1], 0.0) });
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

// ---------- window only ----------

/// A flame on screen: one per fire, found by the fire's id in `FlameVisuals`.
#[derive(Component)]
struct Flame;

/// A puff of a spray on screen.
#[derive(Component)]
struct Puff;

/// Window: the flames by fire id, and the puffs (made once).
#[derive(Resource, Default)]
struct FlameVisuals {
    by_id: BTreeMap<u64, Entity>,
    puffs: Vec<Entity>,
}

pub fn window_plugin(app: &mut App) {
    app.init_resource::<FlameVisuals>();
    app.add_systems(Update, flames_sync.in_set(crate::phases::Frame::Sync));
}

/// Window: each fire is a flickering flame scaled by its size with a flickering light; each spray
/// shows a few white puffs along its aim. Positions follow the render origin.
#[allow(clippy::too_many_arguments)]
fn flames_sync(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    area: Option<Res<DevArea>>,
    items: Res<Items>,
    origin: Res<RenderOrigin>,
    mut vis: ResMut<FlameVisuals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flames: Query<(&Flame, &mut Transform, &mut PointLight), Without<Puff>>,
    mut puffs: Query<(&mut Transform, &mut Visibility), With<Puff>>,
) {
    let Some(news) = items.fire.as_ref() else { return };
    let (Some(area), Some(frame)) = (area.as_deref(), area.as_deref().and_then(|a| a.frame)) else { return };
    let t = time.elapsed_secs_f64();
    let to_render = |w: DVec3| (w - origin.origin).as_vec3();

    let alive: BTreeSet<u64> = news.fires.iter().map(|f| f.id).collect();
    vis.by_id.retain(|id, e| {
        if alive.contains(id) {
            true
        } else {
            commands.entity(*e).despawn();
            false
        }
    });
    if vis.puffs.is_empty() {
        let mesh = meshes.add(Sphere::new(0.07));
        let mat = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.45), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
        for _ in 0..MAX_SPRAYS * PUFFS {
            let e = commands.spawn((Puff, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden)).id();
            vis.puffs.push(e);
        }
    }
    for f in &news.fires {
        let Some((ground, up)) = flame_world(area, &planet, f.pos) else { continue };
        let k = (0.9 + 0.1 * (t * 17.0 + f.id as f64).sin()) as f32;
        let rot = frame.rotation(up, 0.0).as_quat();
        let scale = Vec3::new(f.size as f32 * 1.2, (f.size as f32 * 1.6 * k).max(0.05), f.size as f32 * 1.2);
        // The light flickers per frame (0.8 to 1.0) and reaches further as the fire grows.
        let light = 30_000.0 * f.size as f32 * crate::effects::flame_flicker(t.to_bits() ^ f.id.wrapping_mul(0x9E37_79B9));
        let range = crate::effects::flame_range(f.size);
        if let Some(&e) = vis.by_id.get(&f.id) {
            if let Ok((_, mut tr, mut l)) = flames.get_mut(e) {
                tr.translation = to_render(ground);
                tr.rotation = rot;
                tr.scale = scale;
                l.intensity = light;
                l.range = range;
            }
        } else {
            let mesh = meshes.add(Cone::new(0.5, 1.0));
            let mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.15), emissive: LinearRgba::rgb(6.0, 2.4, 0.4), unlit: true, ..default() });
            let e = commands
                .spawn((
                    Flame,
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform { translation: to_render(ground), rotation: rot, scale, ..default() },
                    PointLight { color: Color::srgb(1.0, 0.7, 0.35), intensity: light, range, shadow_maps_enabled: false, ..default() },
                ))
                .id();
            vis.by_id.insert(f.id, e);
        }
    }

    // The puffs: the sprays' first few, along the aim, flickering.
    let mut shown = vec![false; vis.puffs.len()];
    for (si, s) in news.sprays.iter().take(MAX_SPRAYS).enumerate() {
        let (eye, dir) = (DVec3::from_array(s.eye), DVec3::from_array(s.dir).normalize_or_zero());
        for j in 0..PUFFS {
            let i = si * PUFFS + j;
            let d = 0.8 + j as f64 * 0.9 + 0.1 * (t * 11.0 + j as f64).sin();
            shown[i] = true;
            if let Ok((mut tr, mut v)) = puffs.get_mut(vis.puffs[i]) {
                tr.translation = to_render(eye + dir * d);
                *v = Visibility::Inherited;
            }
        }
    }
    for (i, e) in vis.puffs.iter().enumerate() {
        if !shown[i]
            && let Ok((_, mut v)) = puffs.get_mut(*e)
        {
            *v = Visibility::Hidden;
        }
    }
}
