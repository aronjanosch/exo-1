//! The systems only draw the effects: they read the snapshots (`Items::reactor`,
//! `Items::fire`) and the local vitals (`crew_hud::Vitals`), so a client shows the same as the host.
use crate::crew_hud::Vitals;
use crate::dev_area::engine_room::{ignition_at, COOLANT_PUMP_AT, JUNCTION_BOX_AT, JUNCTION_BOX_SIZE, OXYGEN_GENERATOR_AT, OXYGEN_GENERATOR_SIZE, REACTOR_AT, REACTOR_SIZE};
use crate::dev_area::DevArea;
use crate::env::PlanetRes;
use crate::health::UseHold;
use crate::items::Items;
use crate::origin::WorldPos;
use crate::phases::Frame;
use bevy::prelude::*;
use bevy::math::DVec3;
use bevy::ui::{ColorStop, RadialGradient, RadialGradientShape, UiPosition, BackgroundGradient};
use item_core::wire::ReactorNews;
use reactor_core::hull::{openness, SECTION_CENTRES};
use reactor_core::Source;
use std::f64::consts::TAU;

// ---------- the rules (pure) ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Smoke {
    None,
    Thin,
    Heavy,
}

/// The condition share (0..1) of a device as its smoke level.
pub fn smoke_level(share: f64) -> Smoke {
    if share < 0.15 {
        Smoke::Heavy
    } else if share < 0.5 {
        Smoke::Thin
    } else {
        Smoke::None
    }
}

/// Seconds between two smoke puffs of a level (`TODO(initiator)`: start values).
pub fn smoke_every(level: Smoke) -> Option<f64> {
    match level {
        Smoke::None => None,
        Smoke::Thin => Some(0.6),
        Smoke::Heavy => Some(0.2),
    }
}

/// Sparks of the junction box: a burst every 2 s while overvolted, every 4 s while broken (an
/// overvolt takes the faster rate). `TODO(initiator)`: the rates are start values.
pub fn spark_every(overvolt: bool, broken: bool) -> Option<f64> {
    if overvolt {
        Some(2.0)
    } else if broken {
        Some(4.0)
    } else {
        None
    }
}

/// Sparks in one burst, their diameter at birth (m, 4 cm), their life (s) and the height of the
/// burst above the floor (m, the box's front at 1 m).
pub const SPARKS_PER_BURST: usize = 6;
pub const SPARK_SIZE: f32 = 0.045;
pub const SPARK_LIFE: f32 = 0.6;
pub const SPARK_HEIGHT: f64 = 1.0;

pub fn is_critical(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.power_on && r.temp > r.allowed[1])
}

pub fn critical_lamp_lit(critical: bool, t: f64) -> bool {
    critical && t.rem_euclid(1.0) < 0.5
}

/// A pseudo-random value in 0..1 from a seed (splitmix64).
pub fn noise(seed: u64) -> f64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

pub fn flame_flicker(seed: u64) -> f32 {
    (0.8 + 0.2 * noise(seed)) as f32
}

pub fn flame_range(size: f64) -> f32 {
    (4.0 + 5.0 * size).min(14.0) as f32
}

pub const DARK_FLICKER_S: f64 = 1.0;
pub const LIGHT_FLICKER_S: f64 = 0.6;

pub fn lamp_lit(target: bool, since: f64, now: f64) -> bool {
    let dur = if target { LIGHT_FLICKER_S } else { DARK_FLICKER_S };
    if now - since >= dur {
        target
    } else {
        noise((now * 30.0).floor() as u64) < 0.5
    }
}

/// Alphas are 0..1. `oxygen` is the vitals' air left (1 is full; None when the air is no issue).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenFx {
    pub burn: f32,
    pub dark: f32,
    pub vignette: f32,
    pub grain: f32,
}

pub fn screen_fx(burning: bool, oxygen: Option<f64>, radiation: f64, t: f64) -> ScreenFx {
    let burn = if burning { 0.25 + 0.25 * (0.5 + 0.5 * (TAU * t / 0.7).sin()) as f32 } else { 0.0 };
    let low = oxygen.map_or(0.0, |o| ((0.5 - o) / 0.5).clamp(0.0, 1.0)) as f32;
    let edge = if low > 0.0 { 0.5 + 0.5 * low } else { 0.0 };
    let grain = if radiation > 0.25 { (0.06 + 0.06 * ((radiation - 0.25) / 0.75).clamp(0.0, 1.0)) as f32 } else { 0.0 };
    ScreenFx { burn, dark: 0.35 * edge, vignette: 0.85 * edge, grain }
}

pub const TREAT_FLASH_S: f64 = 0.6;

pub fn flash_alpha(age: f64) -> f32 {
    (1.0 - age / TREAT_FLASH_S).clamp(0.0, 1.0) as f32
}

pub fn progress_rgb(p: f64) -> [f32; 3] {
    const RED: [f32; 3] = [0.9, 0.2, 0.2];
    const ORANGE: [f32; 3] = [1.0, 0.6, 0.1];
    const GREEN: [f32; 3] = [0.2, 0.9, 0.3];
    let p = p.clamp(0.0, 1.0) as f32;
    let (a, b, k) = if p < 0.5 { (RED, ORANGE, p * 2.0) } else { (ORANGE, GREEN, (p - 0.5) * 2.0) };
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

pub const IMPELLER_DEG_PER_S: f64 = 240.0;

pub fn impeller_angle(prev: f64, flow: f64, dt: f64) -> f64 {
    (prev + IMPELLER_DEG_PER_S * flow.clamp(0.0, 1.0) * dt).rem_euclid(60.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Steam,
    Smoke,
    Spark,
}

pub fn scale_of(kind: Kind, size: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        Kind::Steam => size * (0.4 + 0.8 * t) * (1.0 - t * 0.6),
        Kind::Smoke => size * (0.5 + t),
        Kind::Spark => size * (1.0 - t),
    }
}

/// A particle's alpha by its age share: steam and smoke fade in fast and out to nothing, sparks
/// stay bright until they go (their emissive does the work).
pub fn fade_of(kind: Kind, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let ramp = (t * 5.0).min(1.0) * (1.0 - t);
    match kind {
        Kind::Steam => 0.55 * ramp,
        Kind::Smoke => 0.5 * ramp,
        Kind::Spark => 1.0,
    }
}

// ---------- window ----------

/// A short-lived particle: its velocity (world m/s), its gravity, its life and its age (s), and
/// its own material (its alpha fades with its age; removed with the particle).
#[derive(Component)]
struct Particle {
    vel: DVec3,
    gravity: f64,
    kind: Kind,
    size: f32,
    life: f32,
    age: f32,
    mat: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct FxAssets {
    mesh: Handle<Mesh>,
}

#[derive(Resource)]
struct Emitters {
    smoke: [f64; 4],
    spark: f64,
    steam: f64,
    /// Time since each section's last hiss puff (r37, by section in r40).
    breach: [f64; SECTION_CENTRES.len()],
    /// Time since the pump's last leak puff (r36).
    leak: f64,
}

impl Default for Emitters {
    fn default() -> Emitters {
        Emitters { smoke: [0.0; 4], spark: f64::INFINITY, steam: 0.0, leak: 0.0, breach: [0.0; SECTION_CENTRES.len()] }
    }
}

#[derive(Component)]
struct CriticalLamp {
    light: Entity,
    lens: Entity,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ScreenPart {
    Burn,
    Bad,
    Dark,
    Flash,
    /// The meltdown blast's white flash (`dev_area::blast`).
    White,
    Grain(usize),
}

#[derive(Resource, Default)]
struct ScreenState {
    built: bool,
    hold: Option<crate::health::Hold>,
    flash_at: Option<f64>,
}

const GRAIN_DOTS: usize = 48;
/// Where the burn's and the vignette's clear centre ends (percent of the farthest corner): the
/// burn is an edge only, the vignette a wider rim.
const BURN_CLEAR: f32 = 72.0;
const VIGNETTE_CLEAR: f32 = 55.0;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<Emitters>().init_resource::<ScreenState>();
    app.add_systems(Update, (spawn_assets, device_fx, steam_fx, critical_lamp_sync, particles_step).chain().in_set(Frame::Sync));
    app.add_systems(Update, screen_sync.in_set(Frame::Hud));
}

fn spawn_assets(mut commands: Commands, mut done: Local<bool>, mut meshes: ResMut<Assets<Mesh>>) {
    if *done {
        return;
    }
    *done = true;
    commands.insert_resource(FxAssets { mesh: meshes.add(Sphere::new(0.5)) });
}

/// A particle's material: steam and smoke are lit soft puffs (not unlit, so a sphere shades like a
/// puff and does not read as a flat disc); sparks are unlit and emissive.
fn particle_material(kind: Kind) -> StandardMaterial {
    match kind {
        Kind::Steam => StandardMaterial { base_color: Color::srgba(0.95, 0.95, 0.97, 0.0), alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() },
        Kind::Smoke => StandardMaterial { base_color: Color::srgba(0.42, 0.43, 0.45, 0.0), alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() },
        Kind::Spark => StandardMaterial { base_color: Color::srgb(1.0, 0.7, 0.3), emissive: LinearRgba::rgb(14.0, 7.0, 2.0), unlit: true, ..default() },
    }
}

fn spawn_particle(commands: &mut Commands, assets: &FxAssets, mats: &mut Assets<StandardMaterial>, kind: Kind, at: DVec3, vel: DVec3, gravity: f64, size: f32, life: f32) {
    let mat = mats.add(particle_material(kind));
    commands.spawn((Particle { vel, gravity, kind, size, life, age: 0.0, mat: mat.clone() }, WorldPos(at), Mesh3d(assets.mesh.clone()), MeshMaterial3d(mat), Transform::from_scale(Vec3::splat(0.001))));
}

fn particles_step(mut commands: Commands, time: Res<Time>, mut mats: ResMut<Assets<StandardMaterial>>, mut parts: Query<(Entity, &mut Particle, &mut WorldPos, &mut Transform)>) {
    let dt = time.delta_secs_f64();
    for (e, mut p, mut pos, mut tr) in &mut parts {
        p.age += dt as f32;
        if p.age >= p.life {
            mats.remove(&p.mat);
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        pos.0 += p.vel * dt;
        let t = p.age / p.life;
        tr.scale = Vec3::splat(scale_of(p.kind, p.size, t));
        if let Some(mut m) = mats.get_mut(&p.mat) {
            m.base_color.set_alpha(fade_of(p.kind, t));
        }
    }
}

fn jet(up: DVec3, seed: u64, speed: f64, side: f64) -> DVec3 {
    let a = noise(seed) * TAU;
    let s = noise(seed.wrapping_add(7)) * side;
    let t = up.any_orthonormal_vector();
    let b = up.cross(t);
    up * speed + (t * a.cos() + b * a.sin()) * s
}

fn device_fx(time: Res<Time>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, assets: Option<Res<FxAssets>>, mut mats: ResMut<Assets<StandardMaterial>>, mut em: ResMut<Emitters>, mut commands: Commands) {
    let (Some(r), Some(area), Some(assets)) = (items.reactor, area.as_deref(), assets) else { return };
    let dt = time.delta_secs_f64();
    let t = time.elapsed_secs_f64();
    let spots = [(REACTOR_AT, REACTOR_SIZE[1], r.reactor.share), (COOLANT_PUMP_AT, 1.2, r.pump.share), (JUNCTION_BOX_AT, JUNCTION_BOX_SIZE[1], r.junction.share), (OXYGEN_GENERATOR_AT, OXYGEN_GENERATOR_SIZE[1], r.oxygen_generator.share)];
    for (i, (at, lift, share)) in spots.into_iter().enumerate() {
        let Some(every) = smoke_every(smoke_level(share)) else {
            em.smoke[i] = 0.0;
            continue;
        };
        em.smoke[i] += dt;
        if em.smoke[i] >= every {
            em.smoke[i] = 0.0;
            if let Some((p, up)) = area.ground(&planet, at, lift) {
                let seed = (t * 1000.0) as u64 ^ (i as u64) << 40;
                spawn_particle(&mut commands, &assets, &mut mats, Kind::Smoke, p, jet(up, seed, 1.0, 0.3), 0.0, 0.3, 3.0);
            }
        }
    }
    if let Some(every) = spark_every(r.overvolt, !r.junction.working) {
        // The first burst comes at once (the clock is idle, `INFINITY`, while nothing sparks).
        if !em.spark.is_finite() {
            em.spark = every;
        }
        em.spark += dt;
        if em.spark >= every {
            em.spark = 0.0;
            // The burst leaves the box's front face (the side the room's centre is on), 1 m up.
            if let Some((p, up)) = area.ground(&planet, ignition_at(Source::Junction), SPARK_HEIGHT) {
                for k in 0..SPARKS_PER_BURST {
                    let seed = ((t * 1000.0) as u64).wrapping_mul(31).wrapping_add(k as u64);
                    spawn_particle(&mut commands, &assets, &mut mats, Kind::Spark, p, jet(up, seed, 1.5, 2.5), 9.8, SPARK_SIZE, SPARK_LIFE);
                }
            }
        }
    } else {
        em.spark = f64::INFINITY;
    }
    // The pump's leak (r36): a puff at its base, more of them the faster it leaks (0.4 s at the
    // full 0.01 per s, 1.6 s at a quarter of it).
    if r.coolant_leak > 0.0 {
        em.leak += dt;
        if em.leak >= 0.4 / (r.coolant_leak * 100.0) {
            em.leak = 0.0;
            if let Some((p, up)) = area.ground(&planet, COOLANT_PUMP_AT, 0.1) {
                let seed = (t * 1000.0) as u64 ^ 0x5eed;
                spawn_particle(&mut commands, &assets, &mut mats, Kind::Steam, p, jet(up, seed, 0.3, 0.1), 0.0, 0.2, 1.2);
            }
        }
    } else {
        em.leak = 0.0;
    }
    // The hull breach (r37, by section in r40): a hiss of steam-like air at each leaking section's
    // wall, more of it the more open the hole (0.5 s at fully open, 1.5 s at a third of it).
    for (i, centre) in SECTION_CENTRES.iter().enumerate() {
        let open = openness(r.breaches[i]);
        if open > 0.0 {
            em.breach[i] += dt;
            if em.breach[i] >= 0.5 / open {
                em.breach[i] = 0.0;
                if let Some((p, up)) = area.ground(&planet, *centre, 0.9) {
                    let seed = (t * 1000.0) as u64 ^ 0xb2ea ^ ((i as u64) << 8);
                    spawn_particle(&mut commands, &assets, &mut mats, Kind::Steam, p, jet(up, seed, 0.4, 0.2), 0.0, 0.25, 1.2);
                }
            }
        } else {
            em.breach[i] = 0.0;
        }
    }
}

fn steam_fx(time: Res<Time>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<crate::dev_area::fire::FireConfig>, assets: Option<Res<FxAssets>>, mut mats: ResMut<Assets<StandardMaterial>>, mut em: ResMut<Emitters>, mut commands: Commands) {
    let (Some(news), Some(area), Some(assets)) = (items.fire.as_ref(), area.as_deref(), assets) else { return };
    em.steam += time.delta_secs_f64();
    if em.steam < 0.12 {
        return;
    }
    em.steam = 0.0;
    let t = time.elapsed_secs_f64();
    for (i, s) in news.sprays.iter().enumerate() {
        let Some(at) = crate::dev_area::fire::landing_of(area, &planet, &cfg.0, s, &news.fires) else { continue };
        let seed = ((t * 1000.0) as u64).wrapping_add(i as u64 * 977);
        let jitter = jet(DVec3::Y, seed, 0.0, 0.2);
        spawn_particle(&mut commands, &assets, &mut mats, Kind::Steam, at + jitter, DVec3::Y * 0.6, 0.0, 0.25, 1.2);
    }
}

#[allow(clippy::too_many_arguments)]
fn critical_lamp_sync(mut commands: Commands, time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, items: Res<Items>, mut done: Local<Option<warp_core::PlanetId>>, lamps: Query<&CriticalLamp>, mut lights: Query<&mut PointLight>, mut vis: Query<&mut Visibility>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let Some(area) = area.as_deref() else { return };
    if area.frame.is_none() {
        return;
    }
    if *done != Some(planet.id) {
        *done = Some(planet.id);
        for l in &lamps {
            commands.entity(l.light).despawn();
            commands.entity(l.lens).despawn();
        }
        let Some((p, up)) = area.ground(&planet, REACTOR_AT, REACTOR_SIZE[1] + 0.2) else { return };
        let mesh = meshes.add(Sphere::new(0.08));
        let mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.1, 0.05), emissive: LinearRgba::rgb(8.0, 0.3, 0.1), unlit: true, ..default() });
        let lens = commands.spawn((WorldPos(p), Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden)).id();
        let light = commands.spawn((WorldPos(p + up * 0.1), PointLight { color: Color::srgb(1.0, 0.1, 0.05), intensity: 0.0, range: 6.0, shadow_maps_enabled: false, ..default() }, Transform::default())).id();
        commands.spawn(CriticalLamp { light, lens });
        return;
    }
    let lit = critical_lamp_lit(is_critical(items.reactor.as_ref()), time.elapsed_secs_f64());
    for l in &lamps {
        if let Ok(mut light) = lights.get_mut(l.light) {
            light.intensity = if lit { 25_000.0 } else { 0.0 };
        }
        if let Ok(mut v) = vis.get_mut(l.lens) {
            *v = if lit { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

/// Builds the screen's parts once. They sit under the HUD (`GlobalZIndex` -2), over the scene.
fn build_screen(commands: &mut Commands) {
    let full = || Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() };
    commands.spawn((ScreenPart::Dark, GlobalZIndex(-2), full(), BackgroundColor(Color::NONE)));
    commands.spawn((ScreenPart::Burn, GlobalZIndex(-2), full(), BackgroundGradient::default()));
    commands.spawn((ScreenPart::Bad, GlobalZIndex(-2), full(), BackgroundGradient::default()));
    commands.spawn((ScreenPart::Flash, GlobalZIndex(-2), full(), BackgroundGradient::default()));
    commands.spawn((ScreenPart::White, GlobalZIndex(-2), full(), BackgroundColor(Color::NONE)));
    for i in 0..GRAIN_DOTS {
        commands.spawn((ScreenPart::Grain(i), GlobalZIndex(-2), Node { position_type: PositionType::Absolute, width: px(2), height: px(2), ..default() }, BackgroundColor(Color::NONE)));
    }
}

fn edge(c: Color, a: f32, clear: f32) -> BackgroundGradient {
    BackgroundGradient::from(RadialGradient::new(UiPosition::CENTER, RadialGradientShape::FarthestCorner, vec![ColorStop::new(c.with_alpha(0.0), percent(clear)), ColorStop::new(c.with_alpha(a), percent(100.0))]))
}

#[allow(clippy::too_many_arguments)]
fn screen_sync(mut commands: Commands, time: Res<Time>, vitals: Res<Vitals>, hold: Res<UseHold>, blast: Res<crate::dev_area::blast::BlastFlash>, mut st: ResMut<ScreenState>, mut parts: Query<(&ScreenPart, &mut BackgroundColor, &mut BackgroundGradient, &mut Node)>) {
    if !st.built {
        st.built = true;
        build_screen(&mut commands);
    }
    let now = time.elapsed_secs_f64();
    // A treatment finishes when its hold runs out on the last frame it was held.
    if let Some(h) = st.hold
        && hold.0.is_none()
        && h.elapsed + time.delta_secs_f64() >= h.need
    {
        st.flash_at = Some(now);
    }
    st.hold = hold.0;
    let fx = screen_fx(vitals.burning, vitals.oxygen, vitals.radiation, now);
    let flash = st.flash_at.map_or(0.0, |at| flash_alpha(now - at));
    let white = blast.0.map_or(0.0, |at| crate::dev_area::blast::flash_alpha(now - at) * blast.1);
    for (part, mut bg, mut gradient, mut node) in &mut parts {
        match *part {
            ScreenPart::Dark => *bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, fx.dark)),
            ScreenPart::Burn => *gradient = edge(Color::srgb(0.95, 0.35, 0.05), fx.burn, BURN_CLEAR),
            ScreenPart::Bad => *gradient = edge(Color::BLACK, fx.vignette, VIGNETTE_CLEAR),
            ScreenPart::Flash => *gradient = edge(Color::srgb(0.3, 1.0, 0.4), 0.6 * flash, 55.0),
            ScreenPart::White => *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9 * white)),
            ScreenPart::Grain(i) => {
                let slot = (now * 20.0).floor() as u64;
                let x = noise(slot * 131 + i as u64) * 100.0;
                let y = noise(slot * 977 + i as u64 + 5) * 100.0;
                node.left = percent(x as f32);
                node.top = percent(y as f32);
                *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, fx.grain));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_critical_lamp_is_lit_only_while_critical_and_blinks_at_one_hertz() {
        let off = ReactorNews { power_on: true, temp: 50.0, allowed: [20.0, 80.0], ..default() };
        let crit = ReactorNews { temp: 85.0, ..off };
        assert!(!is_critical(None));
        assert!(!is_critical(Some(&off)), "inside the allowed band");
        assert!(is_critical(Some(&crit)));
        assert!(!is_critical(Some(&ReactorNews { power_on: false, ..crit })), "the power off is not critical");
        for k in 0..20 {
            let t = k as f64 * 0.1;
            assert!(!critical_lamp_lit(false, t), "no blink without CRITICAL");
        }
        assert!(critical_lamp_lit(true, 0.1) && !critical_lamp_lit(true, 0.7) && critical_lamp_lit(true, 1.2), "on half of each second");
    }
}
