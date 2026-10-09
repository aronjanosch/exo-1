//! Day and night (#48): the clock, the sun of the current planet and the light it gives. The
//! numbers come from `daynight_core` and `content/daynight/daynight.json`; this module only
//! advances the clock in the fixed step, keeps `Sun` up to date for the player's spot and, with a
//! window, drives the sun and night lights (the camera's ambient, fog and clear colour follow in
//! `view::update_camera`).
use crate::env::PlanetRes;
use crate::ship::Ship;
use crate::walker::Player;
use crate::warp::SystemRes;
use avian3d::prelude::Position;
use bevy::math::DVec3;
use bevy::prelude::*;
use daynight_core::{DayNight, LightKey};

pub const DAYNIGHT: &str = include_str!("../../../content/daynight/daynight.json");

#[derive(Resource, Clone)]
pub struct DayNightRes(pub DayNight);

/// The global clock (simulated seconds). Everything that changes with the time of day reads it.
/// Co-op: every player runs their own for now (open in #48).
#[derive(Resource, Clone, Copy, Debug)]
pub struct DayClock {
    pub t: f64,
    /// Clock seconds per simulated second (1 = real time; scenarios fast-forward).
    pub rate: f64,
}

impl Default for DayClock {
    fn default() -> Self {
        DayClock { t: 0.0, rate: 1.0 }
    }
}

/// The sun as the player sees it this step.
#[derive(Resource, Clone, Debug)]
pub struct Sun {
    /// Towards the sun (world space, unit).
    pub dir: DVec3,
    /// Elevation over the player's horizon (deg).
    pub elevation_deg: f64,
    /// Local hour at the player's spot (12 = noon); None at a pole.
    pub hour: Option<f64>,
    pub light: LightKey,
    /// A fixed sun instead of the clock's (the look harness's viewpoints).
    pub fixed: Option<DVec3>,
}

/// Marks the sun's and the night light's directional lights.
#[derive(Component)]
pub struct SunLight;
#[derive(Component)]
pub struct NightLight;

/// Where the player is (world): the ship when seated or in its cabin, the walker otherwise.
fn viewer(players: &Query<&Player>, ships: &Query<&Position, With<Ship>>) -> Option<DVec3> {
    let p = players.single().ok()?;
    if p.seated || p.ship.is_some() { ships.single().ok().map(|s| s.0) } else { Some(p.w.pos) }
}

/// The sun and its light for a planet, clock time and viewer.
pub fn sun_at(dn: &DayNight, sys: &SystemRes, planet: &PlanetRes, t: f64, at: DVec3, fixed: Option<DVec3>) -> Sun {
    let recipe = &sys.0.planet(planet.id).recipe;
    let (ps, look) = dn.planet(recipe).unwrap_or_else(|e| panic!("{e}"));
    let up = (at - planet.centre).try_normalize().unwrap_or(DVec3::Y);
    let dir = fixed.unwrap_or_else(|| ps.sun_dir(t));
    let elevation_deg = dir.dot(up).clamp(-1.0, 1.0).asin().to_degrees();
    Sun { dir, elevation_deg, hour: ps.local_hour(up, t), light: look.sample(elevation_deg), fixed }
}

/// Advance the clock and recompute the sun (end of the fixed step, after the walker moved).
pub fn tick(
    time: Res<Time>,
    mut clock: ResMut<DayClock>,
    dn: Res<DayNightRes>,
    sys: Res<SystemRes>,
    planet: Res<PlanetRes>,
    players: Query<&Player>,
    ships: Query<&Position, With<Ship>>,
    mut sun: ResMut<Sun>,
) {
    clock.t += time.delta_secs_f64() * clock.rate;
    let at = viewer(&players, &ships).unwrap_or(planet.centre + DVec3::Y * planet.radius);
    *sun = sun_at(&dn.0, &sys, &planet, clock.t, at, sun.fixed);
}

fn rgb(c: [f64; 3]) -> Color {
    Color::srgb(c[0] as f32, c[1] as f32, c[2] as f32)
}

/// Point a directional light along `-towards` (it shines away from where it sits).
fn aim(t: &mut Transform, towards: DVec3) {
    let up = if towards.y.abs() < 0.99 { DVec3::Y } else { DVec3::X };
    t.rotation = walker_core::look_rot(-towards, up).as_quat();
}

/// The night light next to the sun of `view::setup_view`.
pub fn setup_lights(mut commands: Commands) {
    commands.spawn((NightLight, DirectionalLight { illuminance: 0.0, shadow_maps_enabled: false, ..default() }, Transform::default()));
}

/// The sun and night lights from `Sun` (with a window).
pub fn apply_lights(
    sun: Res<Sun>,
    mut suns: Query<(&mut DirectionalLight, &mut Transform), (With<SunLight>, Without<NightLight>)>,
    mut nights: Query<(&mut DirectionalLight, &mut Transform), (With<NightLight>, Without<SunLight>)>,
) {
    let l = &sun.light;
    for (mut d, mut t) in &mut suns {
        d.illuminance = l.sun_lux as f32;
        d.color = rgb(l.sun_color);
        aim(&mut t, sun.dir);
    }
    for (mut d, mut t) in &mut nights {
        d.illuminance = l.night_lux as f32;
        d.color = rgb(l.night_color);
        aim(&mut t, -sun.dir);
    }
}

/// Ambient, fog and clear colour of the camera at air `density` (0 in space, 1 at the ground):
/// (ambient colour, ambient brightness, fog colour, fog density, clear colour).
pub fn camera_light(sun: &Sun, haze_color: [f32; 3], haze_density: f32, density: f32) -> (Color, f32, Color, f32, Color) {
    let l = &sun.light;
    let fog = std::array::from_fn(|i| haze_color[i] as f64 * l.fog_tint[i]);
    (rgb(l.ambient_color), l.ambient as f32 * (0.2 + 0.8 * density), rgb(fog), haze_density * l.fog_density as f32 * density, rgb(l.sky_color))
}

pub fn plugin(app: &mut App) {
    let dn = DayNight::from_json(DAYNIGHT).unwrap_or_else(|e| panic!("{e}"));
    let sys = warp_core::System::from_json(crate::warp::SYSTEM).expect("system.json");
    for p in &sys.planets {
        dn.planet(&p.recipe).unwrap_or_else(|e| panic!("{e} (planet {} in system.json)", p.name));
    }
    let (ps, look) = dn.planet(&sys.planets[0].recipe).unwrap();
    let dir = ps.sun_dir(0.0);
    let elevation_deg = dir.dot(daynight_core::SPAWN_UP).asin().to_degrees();
    app.insert_resource(Sun { dir, elevation_deg, hour: Some(ps.start_hour), light: look.sample(elevation_deg), fixed: None });
    app.insert_resource(DayNightRes(dn));
    app.init_resource::<DayClock>();
    app.add_systems(FixedUpdate, tick.after(crate::ship::camera_fx));
}

/// Simulated seconds the `daynight` scenario takes for one day.
const FAST_DAY_SECS: f64 = 12.0;

/// One tick of the fast-forwarded day: elevation, ground brightness and local hour.
#[derive(Clone, Copy)]
struct Sample {
    elevation: f64,
    brightness: f64,
    hour: f64,
    sun_lux: f64,
    night_lux: f64,
}

/// `daynight` scenario (#48): on every planet the walker stands at the spawn point while the clock
/// runs one day in `FAST_DAY_SECS`. Checks every tick's sun against `daynight_core`, that the sun
/// comes back after `day_length_s`, the highest and lowest sun, and noon brighter than dusk
/// brighter than night, with the night light on and the sun off at night.
pub fn scenario_steps(s: &mut Vec<crate::scenario::Step>) {
    use crate::scenario::{check, place_walker, settle, wait};
    let sys = warp_core::System::from_json(crate::warp::SYSTEM).expect("system.json");
    for i in 0..sys.planets.len() {
        s.push(crate::look::go_to(i));
        s.push(settle());
        s.push(Box::new(|w, _| {
            let pl = w.resource::<PlanetRes>().clone();
            place_walker(w, pl.centre + DVec3::Y * pl.surface(DVec3::Y));
            true
        }));
        s.push(wait(1.0));
        let mut samples: Vec<Sample> = Vec::new();
        let (mut t0, mut dir0, mut max_err) = (0.0, DVec3::ZERO, 0.0f64);
        s.push(Box::new(move |w, c| {
            let pl = w.resource::<PlanetRes>().clone();
            let sys = w.resource::<SystemRes>().0.clone();
            let def = sys.planet(pl.id);
            let (ps, _) = w.resource::<DayNightRes>().0.planet(&def.recipe).map(|(p, l)| (p.clone(), l.clone())).unwrap();
            let clock = *w.resource::<DayClock>();
            let sun = w.resource::<Sun>().clone();
            let up = (crate::scenario::player_world(w) - pl.centre).normalize();
            if c.t == 0.0 {
                samples.clear();
                (t0, dir0, max_err) = (clock.t, sun.dir, 0.0);
                w.resource_mut::<DayClock>().rate = ps.day_length_s / FAST_DAY_SECS;
                c.phase = format!("daynight {}", def.name.to_lowercase());
                return false;
            }
            // `tick` ran after the clock's last advance: the sun belongs to `clock.t`.
            max_err = max_err.max(sun.dir.angle_between(ps.sun_dir(clock.t)).to_degrees());
            max_err = max_err.max((sun.elevation_deg - ps.elevation_deg(up, clock.t)).abs());
            let l = &sun.light;
            samples.push(Sample { elevation: sun.elevation_deg, brightness: l.ground_brightness(), hour: sun.hour.unwrap_or(f64::NAN), sun_lux: l.sun_lux, night_lux: l.night_lux });
            if clock.t - t0 < ps.day_length_s {
                return false;
            }
            w.resource_mut::<DayClock>().rate = 1.0;
            let name = &def.name;
            check(c, max_err < 0.01, format!("daynight {name}: sun matches daynight_core every tick (worst {max_err:.4} deg)"));
            check(c, (c.t - FAST_DAY_SECS).abs() < 0.2, format!("daynight {name}: one day of {:.0} s fast-forwarded in {:.2} s", ps.day_length_s, c.t));
            // One tick of the clock turns the sun by at most this much.
            let step = c.dt * ps.day_length_s / FAST_DAY_SECS * ps.omega();
            let back = dir0.angle_between(ps.sun_dir(t0 + ps.day_length_s));
            let end = sun.dir.angle_between(dir0);
            check(c, back < 1e-6 && end <= step * 1.5, format!("daynight {name}: the sun is back after a day ({:.2} deg from the start, one tick {:.2} deg)", end.to_degrees(), step.to_degrees()));
            let hi = samples.iter().copied().max_by(|a, b| a.elevation.total_cmp(&b.elevation)).unwrap();
            let lo = samples.iter().copied().min_by(|a, b| a.elevation.total_cmp(&b.elevation)).unwrap();
            let noon_el = ps.time_for_hour(up, 12.0, t0).map_or(f64::NAN, |t| ps.elevation_deg(up, t));
            let night_el = ps.time_for_hour(up, 0.0, t0).map_or(f64::NAN, |t| ps.elevation_deg(up, t));
            let tol = step.to_degrees() + 0.05;
            check(c, (hi.elevation - noon_el).abs() < tol && (hi.hour - 12.0).abs() < 0.2, format!("daynight {name}: highest sun {:.1} deg at {:.2} h (noon {noon_el:.1} deg)", hi.elevation, hi.hour));
            let midnight = lo.hour.is_finite() && lo.hour.min(24.0 - lo.hour) < 0.2;
            check(c, (lo.elevation - night_el).abs() < tol && lo.elevation < -6.0 && midnight, format!("daynight {name}: lowest sun {:.1} deg at {:.2} h (midnight {night_el:.1} deg)", lo.elevation, lo.hour));
            let dusk = samples.iter().copied().filter(|x| x.hour > 12.0).min_by(|a, b| (a.elevation + 2.0).abs().total_cmp(&(b.elevation + 2.0).abs())).unwrap();
            check(
                c,
                hi.brightness > dusk.brightness && dusk.brightness > lo.brightness && lo.brightness > 0.0,
                format!("daynight {name}: brightness noon {:.0} > dusk ({:.1} deg) {:.0} > night {:.0} > 0", hi.brightness, dusk.elevation, dusk.brightness, lo.brightness),
            );
            check(c, lo.sun_lux == 0.0 && lo.night_lux > 0.0 && hi.night_lux == 0.0, format!("daynight {name}: at night the sun is off and the night light on ({:.0} lux); at noon the night light is off", lo.night_lux));
            c.phase.clear();
            true
        }));
    }
}
