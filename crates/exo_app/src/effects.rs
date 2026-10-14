//! Visual feedback (#300, window only; the logic is the same in every process): the smoke and the
//! sparks of the broken machines, the reactor's critical lamp, the engine room's lights in a power
//! loss, the steam where a spray lands on a fire, and the screen's edges for burning, bad air,
//! radiation and a finished treatment.
//!
//! The rules are pure functions with unit tests (`smoke_level`, `spark_every`, `lamp_lit`,
//! `screen_fx`, ...). The systems only draw them: they read the snapshots (`Items::reactor`,
//! `Items::fire`) and the local vitals (`crew_hud::Vitals`), so a client shows the same as the host.
//! Everything is cheap: one shared mesh and one material per kind, short-lived particles that move
//! with `WorldPos`, and a few UI nodes for the screen.
use crate::crew_hud::Vitals;
use crate::dev_area::engine_room::{COOLANT_PUMP_AT, JUNCTION_BOX_AT, JUNCTION_BOX_SIZE, REACTOR_AT, REACTOR_SIZE};
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
use std::f64::consts::TAU;

// ---------- the rules (pure) ----------

/// A device's smoke by its condition: none, thin grey puffs below half, heavy below 15 %.
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

/// Sparks in one burst.
pub const SPARKS_PER_BURST: usize = 6;

/// The reactor's critical lamp: lit while the power is on and the core is above its allowed band.
pub fn is_critical(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.power_on && r.temp > r.allowed[1])
}

/// The lamp's blink: on for half of each second (1 Hz) while critical, else off.
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

/// A flame's light factor this frame: 0.8 to 1.0, random per frame and per flame.
pub fn flame_flicker(seed: u64) -> f32 {
    (0.8 + 0.2 * noise(seed)) as f32
}

/// A flame's light range (m) by its size: it grows with the fire, 4 m for a small one, 14 m at most.
pub fn flame_range(size: f64) -> f32 {
    (4.0 + 5.0 * size).min(14.0) as f32
}

/// The power loss and the return: the room's lights flicker for a moment, then go to their state.
/// Going dark takes 1 s of flicker, coming back on 0.6 s.
pub const DARK_FLICKER_S: f64 = 1.0;
pub const LIGHT_FLICKER_S: f64 = 0.6;

/// Whether a lamp is lit, given its target state, the time the target last changed and the time.
pub fn lamp_lit(target: bool, since: f64, now: f64) -> bool {
    let dur = if target { LIGHT_FLICKER_S } else { DARK_FLICKER_S };
    if now - since >= dur {
        target
    } else {
        // A flicker: each 1/30 s the lamp is on or off by its noise.
        noise((now * 30.0).floor() as u64) < 0.5
    }
}

/// The room's lamp state as the systems keep it: the target and when it last changed.
#[derive(Debug, Clone, Copy)]
pub struct LampFlicker {
    target: bool,
    since: f64,
}

impl Default for LampFlicker {
    fn default() -> LampFlicker {
        LampFlicker { target: false, since: f64::NEG_INFINITY }
    }
}

impl LampFlicker {
    /// The lamp's light this frame for the target (the first target sets no flicker).
    pub fn update(&mut self, target: bool, now: f64) -> bool {
        if target != self.target {
            self.target = target;
            self.since = now;
        }
        lamp_lit(self.target, self.since, now)
    }
}

/// The screen's edges and wash this frame: the red edge while burning (pulsing), the dark wash and
/// the black edge as the air runs out below half, and the grain above a quarter of radiation.
/// Alphas are 0..1. `oxygen` is the vitals' air left (1 is full; None when the air is no issue).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenFx {
    pub burn: f32,
    pub dark: f32,
    pub vignette: f32,
    pub grain: f32,
}

pub fn screen_fx(burning: bool, oxygen: Option<f64>, radiation: f64, t: f64) -> ScreenFx {
    let burn = if burning { 0.35 + 0.25 * (0.5 + 0.5 * (TAU * t / 0.7).sin()) as f32 } else { 0.0 };
    let low = oxygen.map_or(0.0, |o| ((0.5 - o) / 0.5).clamp(0.0, 1.0)) as f32;
    let grain = if radiation > 0.25 { (0.06 + 0.06 * ((radiation - 0.25) / 0.75).clamp(0.0, 1.0)) as f32 } else { 0.0 };
    ScreenFx { burn, dark: 0.35 * low, vignette: 0.85 * low, grain }
}

/// The treatment flash's length (s) and its alpha at an age: 1 at the start, 0 at the end.
pub const TREAT_FLASH_S: f64 = 0.6;

pub fn flash_alpha(age: f64) -> f32 {
    (1.0 - age / TREAT_FLASH_S).clamp(0.0, 1.0) as f32
}

/// The repair ring's progress colour: red at 0, orange at half, green at the full (rgb 0..1).
pub fn progress_rgb(p: f64) -> [f32; 3] {
    const RED: [f32; 3] = [0.9, 0.2, 0.2];
    const ORANGE: [f32; 3] = [1.0, 0.6, 0.1];
    const GREEN: [f32; 3] = [0.2, 0.9, 0.3];
    let p = p.clamp(0.0, 1.0) as f32;
    let (a, b, k) = if p < 0.5 { (RED, ORANGE, p * 2.0) } else { (ORANGE, GREEN, (p - 0.5) * 2.0) };
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// The coolant pump impeller's angle (degrees, 0..60: the six blades are alike) after `dt` at a
/// flow (0..1). At full flow it turns 240 degrees a second; at 0 it stops.
pub const IMPELLER_DEG_PER_S: f64 = 240.0;

pub fn impeller_angle(prev: f64, flow: f64, dt: f64) -> f64 {
    (prev + IMPELLER_DEG_PER_S * flow.clamp(0.0, 1.0) * dt).rem_euclid(60.0)
}

/// The particles: the kind's scale over its life (`t` 0..1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Steam,
    Smoke,
    Spark,
}

pub fn scale_of(kind: Kind, size: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        // Steam swells and thins out.
        Kind::Steam => size * (0.4 + 0.8 * t) * (1.0 - t * 0.6),
        // Smoke grows as it rises.
        Kind::Smoke => size * (0.5 + t),
        // A spark is small and goes out.
        Kind::Spark => size * (1.0 - t),
    }
}

// ---------- window ----------

/// A short-lived particle: its velocity (world m/s), its gravity, its life and its age (s).
#[derive(Component)]
struct Particle {
    vel: DVec3,
    gravity: f64,
    kind: Kind,
    size: f32,
    life: f32,
    age: f32,
}

/// The shared mesh and the materials of the particles, made once.
#[derive(Resource)]
struct FxAssets {
    mesh: Handle<Mesh>,
    steam: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
}

/// The emitters' clocks: the smoke per device (reactor, pump, junction), the sparks, the steam.
#[derive(Resource, Default)]
struct Emitters {
    smoke: [f64; 3],
    spark: f64,
    steam: f64,
}

/// The critical lamp: its light and its sphere (one entity, once per planet).
#[derive(Component)]
struct CriticalLamp {
    light: Entity,
    lens: Entity,
}

/// The screen's parts: the root, the edges and the wash, the grain dots and the flash.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ScreenPart {
    Burn,
    Bad,
    Dark,
    Flash,
    Grain(usize),
}

#[derive(Resource, Default)]
struct ScreenState {
    built: bool,
    /// The last hold, to see a treatment finish.
    hold: Option<crate::health::Hold>,
    flash_at: Option<f64>,
}

const GRAIN_DOTS: usize = 48;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<Emitters>().init_resource::<ScreenState>();
    app.add_systems(Update, (spawn_assets, device_fx, steam_fx, critical_lamp_sync, particles_step).chain().in_set(Frame::Sync));
    app.add_systems(Update, screen_sync.in_set(Frame::Hud));
}

/// Once: the particles' mesh and materials, and the critical lamp's parts beside the reactor.
fn spawn_assets(mut commands: Commands, mut done: Local<bool>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if *done {
        return;
    }
    *done = true;
    let mat = |c: Color, a: f32| StandardMaterial { base_color: c, unlit: true, alpha_mode: if a < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque }, ..default() };
    commands.insert_resource(FxAssets {
        mesh: meshes.add(Sphere::new(0.5)),
        steam: mats.add(mat(Color::srgba(0.95, 0.95, 0.97, 0.55), 0.55)),
        smoke: mats.add(mat(Color::srgba(0.45, 0.46, 0.48, 0.6), 0.6)),
        spark: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.95, 0.6), emissive: LinearRgba::rgb(9.0, 8.0, 3.0), unlit: true, ..default() }),
    });
}

/// A particle at a world point (window only; `WorldPos` puts it on the render origin).
fn spawn_particle(commands: &mut Commands, assets: &FxAssets, kind: Kind, at: DVec3, vel: DVec3, gravity: f64, size: f32, life: f32) {
    let material = match kind {
        Kind::Steam => assets.steam.clone(),
        Kind::Smoke => assets.smoke.clone(),
        Kind::Spark => assets.spark.clone(),
    };
    commands.spawn((Particle { vel, gravity, kind, size, life, age: 0.0 }, WorldPos(at), Mesh3d(assets.mesh.clone()), MeshMaterial3d(material), Transform::from_scale(Vec3::splat(0.001))));
}

/// Every frame: the particles move, grow or shrink by their kind, and go at their life's end.
fn particles_step(mut commands: Commands, time: Res<Time>, mut parts: Query<(Entity, &mut Particle, &mut WorldPos, &mut Transform)>) {
    let dt = time.delta_secs_f64();
    for (e, mut p, mut pos, mut tr) in &mut parts {
        p.age += dt as f32;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        pos.0 += p.vel * dt;
        tr.scale = Vec3::splat(scale_of(p.kind, p.size, p.age / p.life));
    }
}

/// A random direction in the device's frame: a little sideways, mostly up along `up`.
fn jet(up: DVec3, seed: u64, speed: f64, side: f64) -> DVec3 {
    let a = noise(seed) * TAU;
    let s = noise(seed.wrapping_add(7)) * side;
    let t = up.any_orthonormal_vector();
    let b = up.cross(t);
    up * speed + (t * a.cos() + b * a.sin()) * s
}

/// The devices' smoke and the junction box's sparks, from the snapshot (every process).
fn device_fx(time: Res<Time>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, assets: Option<Res<FxAssets>>, mut em: ResMut<Emitters>, mut commands: Commands) {
    let (Some(r), Some(area), Some(assets)) = (items.reactor, area.as_deref(), assets) else { return };
    let dt = time.delta_secs_f64();
    let t = time.elapsed_secs_f64();
    let spots = [(REACTOR_AT, REACTOR_SIZE[1], r.reactor.share), (COOLANT_PUMP_AT, 1.2, r.pump.share), (JUNCTION_BOX_AT, JUNCTION_BOX_SIZE[1], r.junction.share)];
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
                spawn_particle(&mut commands, &assets, Kind::Smoke, p, jet(up, seed, 0.5, 0.15), 0.0, 0.35, 2.5);
            }
        }
    }
    if let Some(every) = spark_every(r.overvolt, !r.junction.working) {
        em.spark += dt;
        if em.spark >= every {
            em.spark = 0.0;
            if let Some((p, up)) = area.ground(&planet, JUNCTION_BOX_AT, JUNCTION_BOX_SIZE[1] + 0.2) {
                for k in 0..SPARKS_PER_BURST {
                    let seed = ((t * 1000.0) as u64).wrapping_mul(31).wrapping_add(k as u64);
                    spawn_particle(&mut commands, &assets, Kind::Spark, p, jet(up, seed, 2.0, 1.5), 9.8, 0.03, 0.35);
                }
            }
        }
    } else {
        em.spark = 0.0;
    }
}

/// Where each spray lands on a fire this frame: a steam puff per landing, every 0.12 s.
fn steam_fx(time: Res<Time>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, assets: Option<Res<FxAssets>>, mut em: ResMut<Emitters>, mut commands: Commands) {
    let (Some(news), Some(area), Some(assets)) = (items.fire.as_ref(), area.as_deref(), assets) else { return };
    em.steam += time.delta_secs_f64();
    if em.steam < 0.12 {
        return;
    }
    em.steam = 0.0;
    let t = time.elapsed_secs_f64();
    for (i, s) in news.sprays.iter().enumerate() {
        let Some(at) = crate::dev_area::fire::landing_of(area, &planet, s, &news.fires) else { continue };
        let seed = ((t * 1000.0) as u64).wrapping_add(i as u64 * 977);
        let jitter = jet(DVec3::Y, seed, 0.0, 0.2);
        spawn_particle(&mut commands, &assets, Kind::Steam, at + jitter, DVec3::Y * 0.6, 0.0, 0.25, 1.2);
    }
}

/// Once per planet: the critical lamp on the reactor's top (a red sphere and its light), then the
/// sphere shows and the light blinks while the reactor is critical.
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
    for i in 0..GRAIN_DOTS {
        commands.spawn((ScreenPart::Grain(i), GlobalZIndex(-2), Node { position_type: PositionType::Absolute, width: px(2), height: px(2), ..default() }, BackgroundColor(Color::NONE)));
    }
}

/// The edge gradient: clear inside, the colour at the rim.
fn edge(c: Color, a: f32) -> BackgroundGradient {
    BackgroundGradient::from(RadialGradient::new(UiPosition::CENTER, RadialGradientShape::FarthestCorner, vec![ColorStop::new(c.with_alpha(0.0), percent(55.0)), ColorStop::new(c.with_alpha(a), percent(100.0))]))
}

/// The screen: the burning edge, the bad air, the grain and the treatment flash, from the local
/// vitals and the local hold (every process). Under the HUD (`GlobalZIndex` -2).
#[allow(clippy::too_many_arguments)]
fn screen_sync(mut commands: Commands, time: Res<Time>, vitals: Res<Vitals>, hold: Res<UseHold>, mut st: ResMut<ScreenState>, mut parts: Query<(&ScreenPart, &mut BackgroundColor, &mut BackgroundGradient, &mut Node)>) {
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
    for (part, mut bg, mut gradient, mut node) in &mut parts {
        match *part {
            ScreenPart::Dark => *bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, fx.dark)),
            ScreenPart::Burn => *gradient = edge(Color::srgb(0.85, 0.12, 0.05), fx.burn),
            ScreenPart::Bad => *gradient = edge(Color::BLACK, fx.vignette),
            ScreenPart::Flash => *gradient = edge(Color::srgb(0.3, 1.0, 0.4), 0.6 * flash),
            ScreenPart::Grain(i) => {
                // The dots move every 1/20 s, spread by their noise.
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
    fn smoke_is_none_then_thin_below_half_then_heavy_below_fifteen() {
        assert_eq!(smoke_level(1.0), Smoke::None);
        assert_eq!(smoke_level(0.5), Smoke::None, "half is still fine");
        assert_eq!(smoke_level(0.49), Smoke::Thin);
        assert_eq!(smoke_level(0.15), Smoke::Thin);
        assert_eq!(smoke_level(0.14), Smoke::Heavy);
        assert_eq!(smoke_level(0.0), Smoke::Heavy);
        assert_eq!(smoke_every(Smoke::None), None);
        assert!(smoke_every(Smoke::Heavy).unwrap() < smoke_every(Smoke::Thin).unwrap());
    }

    #[test]
    fn sparks_come_while_overvolted_or_broken_and_the_overvolt_is_faster() {
        assert_eq!(spark_every(false, false), None);
        assert_eq!(spark_every(true, false), Some(2.0));
        assert_eq!(spark_every(false, true), Some(4.0));
        assert_eq!(spark_every(true, true), Some(2.0));
    }

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

    #[test]
    fn the_flame_flickers_between_point_eight_and_one_and_its_range_grows_with_size() {
        for seed in 0..200u64 {
            let k = flame_flicker(seed);
            assert!((0.8..=1.0).contains(&k), "{k}");
        }
        assert!((flame_flicker(1) - flame_flicker(2)).abs() > 1e-6, "random per flame");
        assert!(flame_range(0.5) < flame_range(1.5));
        assert!(flame_range(100.0) <= 14.0);
    }

    #[test]
    fn a_power_loss_flickers_for_a_second_then_goes_dark_and_back_on_after_its_flicker() {
        // Off: a flicker for 1 s, then dark for good.
        assert!(!lamp_lit(false, 10.0, 11.0), "dark after the flicker");
        assert!(!lamp_lit(false, 10.0, 15.0));
        // On: flicker for 0.6 s, then on.
        assert!(lamp_lit(true, 20.0, 20.6));
        assert!(lamp_lit(true, 20.0, 25.0));
        // The flicker really flickers: some frames on and some off in the first second.
        let on = (0..60).filter(|k| lamp_lit(false, 10.0, 10.0 + *k as f64 / 60.0)).count();
        assert!(on > 5 && on < 55, "{on}");
        let mut f = LampFlicker::default();
        assert!(!f.update(false, 0.0), "the first target sets no flicker");
        assert!(f.update(true, 1.0) == lamp_lit(true, 1.0, 1.0));
        assert!(f.update(true, 3.0), "the lamp is on after its flicker");
    }

    #[test]
    fn the_screen_edges_follow_burning_air_and_radiation() {
        let quiet = screen_fx(false, None, 0.0, 0.0);
        assert_eq!(quiet, ScreenFx { burn: 0.0, dark: 0.0, vignette: 0.0, grain: 0.0 });
        assert!(screen_fx(true, None, 0.0, 0.2).burn > 0.3);
        let half = screen_fx(false, Some(0.5), 0.0, 0.0);
        assert!(half.dark.abs() < 1e-6 && half.vignette.abs() < 1e-6, "half the air is the edge of the rule");
        let low = screen_fx(false, Some(0.1), 0.0, 0.0);
        assert!(low.dark > 0.2 && low.vignette > 0.6, "{low:?}");
        assert!(screen_fx(false, Some(0.0), 0.0, 0.0).vignette > low.vignette, "grows as the air falls");
        assert_eq!(screen_fx(false, None, 0.25, 0.0).grain, 0.0, "grain only above a quarter");
        assert!(screen_fx(false, None, 0.9, 0.0).grain > screen_fx(false, None, 0.3, 0.0).grain);
    }

    #[test]
    fn the_treatment_flash_fades_in_its_time() {
        assert_eq!(flash_alpha(0.0), 1.0);
        assert!(flash_alpha(0.3) > 0.0 && flash_alpha(0.3) < 1.0);
        assert_eq!(flash_alpha(TREAT_FLASH_S), 0.0);
    }

    #[test]
    fn the_progress_colour_runs_red_orange_green() {
        let red = progress_rgb(0.0);
        let green = progress_rgb(1.0);
        assert!(red[0] > red[1] && green[1] > green[0]);
        let mid = progress_rgb(0.5);
        assert!(mid[0] > 0.9 && mid[1] > 0.5 && mid[1] < 0.7 && mid[2] < 0.2, "{mid:?}");
    }

    #[test]
    fn the_impeller_turns_with_the_flow_and_stops_at_zero() {
        assert_eq!(impeller_angle(12.0, 0.0, 1.0), 12.0, "stopped at 0");
        let a = impeller_angle(0.0, 1.0, 0.1);
        assert!((a - 24.0).abs() < 1e-9, "240 deg/s at full flow");
        assert!(impeller_angle(0.0, 0.5, 0.1) < a, "half the flow turns less");
        assert!((0.0..60.0).contains(&impeller_angle(55.0, 1.0, 0.1)), "the six blades: mod 60");
    }

    #[test]
    fn particles_swell_then_thin_steam_and_shrink_sparks() {
        assert!(scale_of(Kind::Steam, 1.0, 0.5) > scale_of(Kind::Steam, 1.0, 0.0));
        assert!(scale_of(Kind::Steam, 1.0, 1.0) < scale_of(Kind::Steam, 1.0, 0.5));
        assert!(scale_of(Kind::Spark, 1.0, 1.0) == 0.0 && scale_of(Kind::Spark, 1.0, 0.0) == 1.0);
        assert!(scale_of(Kind::Smoke, 1.0, 1.0) > scale_of(Kind::Smoke, 1.0, 0.0));
    }
}
