//! `planet-look` scenario (#63): for every planet in `content/system/system.json` an atlas
//! (equirectangular height, biome, landform and scatter maps) and the bake statistics, and in a
//! window a screenshot from each fixed viewpoint of `content/look/viewpoints.json`. Same seed,
//! same spots, same sun: two runs compare side by side. The `times` shots (#48) stop the clock at
//! a time of day (noon, dusk, night) and use its sun instead of the fixed one.
//!
//! Output: `<out>/look/<planet>/atlas-<layer>.png`, `stats.json`, `<viewpoint>.png`, `<time>.png`.
use crate::env::{from_v3, to_v3, PlanetRes};
use crate::ring::Ring;
use crate::scenario::{check, place_walker, teleport_ship, wait, Ctx, Step};
use crate::terrain::Terrain;
use crate::view::ViewState;
use crate::warp::SystemRes;
use bevy::asset::RenderAssetUsages;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use planet_core::look::walk;
use planet_core::{AtlasLayer, TimeShot, Viewpoint, Viewpoints};
use std::path::{Path, PathBuf};
use warp_core::PlanetId;

pub const VIEWPOINTS: &str = include_str!("../../../content/look/viewpoints.json");

/// Seconds the camera holds still at a viewpoint before the shot (frame times are taken here).
const HOLD_SECS: f64 = 1.5;
/// Longest wait for the terrain under a new viewpoint (simulated seconds).
const SETTLE_LIMIT: f64 = 25.0;

fn planet_dir(out: &Path, name: &str) -> PathBuf {
    out.join("look").join(name.to_lowercase())
}

/// RGB8 rows to a PNG through Bevy's image support.
pub fn save_rgb(path: &Path, w: usize, h: usize, rgb: &[u8]) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in rgb.chunks(3) {
        rgba.extend([p[0], p[1], p[2], 255]);
    }
    let img = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let dynamic = img.try_into_dynamic().map_err(|e| e.to_string())?;
    dynamic.to_rgb8().save(path).map_err(|e| e.to_string())
}

/// World camera pose of a viewpoint on `planet`, or None when the planet has no such spot.
pub fn camera_pose(planet: &PlanetRes, vp: &Viewpoint) -> Option<(DVec3, DQuat, DVec3)> {
    if vp.spot == "orbit" {
        let from = DVec3::from_array(vp.from?).normalize();
        let pos = planet.centre + from * vp.height_m;
        let up = if from.y.abs() < 0.99 { DVec3::Y } else { DVec3::X };
        // Pitched up from looking at the centre (towards the horizon on a descent).
        let t = (up - from * up.dot(from)).normalize();
        let p = vp.pitch_deg.to_radians();
        let look = -from * p.cos() + t * p.sin();
        let rot = walker_core::look_rot(look, from);
        return Some((pos, rot, from));
    }
    let spot = planet.pgen.spot(&vp.spot)?;
    let cam_dir = from_v3(walk(spot.dir, -spot.facing, vp.back_m, planet.radius));
    let up = cam_dir;
    let facing = from_v3(spot.facing);
    let f = (facing - up * facing.dot(up)).normalize();
    let f = DQuat::from_axis_angle(up, -vp.turn_deg.to_radians()) * f;
    let p = vp.pitch_deg.to_radians();
    let look = f * p.cos() + up * p.sin();
    let pos = planet.centre + cam_dir * (planet.surface(cam_dir) + vp.height_m);
    Some((pos, walker_core::look_rot(look, up), from_v3(spot.dir)))
}

/// Sun direction (towards the sun) for a viewpoint pose: the same angles in every spot's frame.
fn sun_for(vps: &Viewpoints, vp: &Viewpoint, pos: DVec3, rot: DQuat, centre: DVec3) -> DVec3 {
    let up = (pos - centre).normalize();
    let fwd = rot * DVec3::NEG_Z;
    if vp.spot == "orbit" {
        let left = rot * DVec3::NEG_X;
        return (up * 0.8 + left * 0.5 + (rot * DVec3::Y) * 0.3).normalize();
    }
    let f = (fwd - up * fwd.dot(up)).normalize();
    let right = f.cross(up);
    let (el, az) = (vps.sun_elevation_deg.to_radians(), vps.sun_azimuth_deg.to_radians());
    up * el.sin() + (f * az.cos() + right * az.sin()) * el.cos()
}

fn set_sun(w: &mut World, towards: DVec3) {
    w.resource_mut::<crate::daynight::Sun>().fixed = Some(towards);
}

/// Atlas and statistics of every planet (a fresh bake from its recipe, as the game loads it).
fn atlas_step(out: PathBuf, vps: Viewpoints) -> Step {
    Box::new(move |w, c| {
        let sys = w.resource::<SystemRes>().0.clone();
        for (i, def) in sys.planets.iter().enumerate() {
            let t0 = std::time::Instant::now();
            let (p, st) = PlanetRes::load_with_stats(PlanetId(i as u8), def);
            let atlas = p.pgen.atlas(vps.atlas_width, 0);
            let dir = planet_dir(&out, &def.name);
            let _ = std::fs::create_dir_all(&dir);
            let mut ok = true;
            for (layer, rgb) in &atlas.layers {
                let path = dir.join(format!("atlas-{}.png", layer.name()));
                if let Err(e) = save_rgb(&path, atlas.width, atlas.height, rgb) {
                    println!("atlas {}: {e}", path.display());
                    ok = false;
                }
            }
            let _ = std::fs::write(dir.join("stats.json"), serde_json::to_string_pretty(&st).unwrap_or_default());
            check(c, ok, format!("look {}: atlas {}x{} ({} layers) in {:.0} ms", def.name, atlas.width, atlas.height, AtlasLayer::ALL.len(), t0.elapsed().as_secs_f64() * 1e3));
            let shares: Vec<String> = st.biome_area_share.iter().map(|(k, v)| format!("{k}: {:.1} %", v * 100.0)).collect();
            let line = format!(
                "look {}: bake {:.0} ms, sea {:.1} m, land {:.1} % (macro {:.1} %), height above sea {:.0}..{:.0} m, biomes [{}], sites {} (land to the nearest site: worst {:.0} m, median {:.0} m; nearest-neighbour gap worst {:.0} m, median {:.0} m, closest pair {:.0} m), landmarks {}, 540 m walks crossing 2+ biomes {:.0} % (median {} rows)",
                def.name, st.bake_ms, st.sea_level_m, st.land_fraction_full * 100.0, st.land_fraction_macro * 100.0,
                st.min_height_above_sea, st.max_height_above_sea, shares.join(", "),
                st.site_count, st.site_cover_worst_m, st.site_cover_median_m, st.site_max_nn_m, st.site_median_nn_m, st.site_min_pair_m, st.landmark_count, st.walks_two_biomes_share * 100.0, st.walk_biomes_median,
            );
            println!("{line}");
            c.report.push(line);
        }
        true
    })
}

/// Bring the simulation to planet `i`: the parked ship into its frame zone (the swap follows),
/// then back onto its ground at the spawn direction.
pub(crate) fn go_to(i: usize) -> Step {
    Box::new(move |w, c| {
        let id = PlanetId(i as u8);
        if w.resource::<PlanetRes>().id == id {
            if c.t > 0.0 {
                let pl = w.resource::<PlanetRes>().clone();
                let pos = pl.centre + DVec3::Y * (pl.surface(DVec3::Y) + 0.05);
                teleport_ship(w, pos, DQuat::IDENTITY);
            }
            return true;
        }
        if c.t == 0.0 {
            let def = w.resource::<SystemRes>().0.planet(id).clone();
            teleport_ship(w, def.centre() + DVec3::Y * (def.radius + 3000.0), DQuat::IDENTITY);
        }
        c.t > 30.0
    })
}

fn settled(w: &World) -> bool {
    let ring = w.resource::<Ring>();
    ring.pending() == 0
        && w.get_resource::<Terrain>().is_none_or(|t| t.pending == 0)
        && w.get_resource::<crate::scatter::ScatterView>().is_none_or(|s| s.pending == 0)
}

/// Clock time of a time-of-day shot over the spot `ground` (unit, planet space), or None when
/// the sun never gets there.
fn shot_time(w: &World, pl: &PlanetRes, ground: DVec3, ts: &TimeShot) -> Option<f64> {
    let recipe = &w.resource::<SystemRes>().0.planet(pl.id).recipe;
    let (ps, _) = w.resource::<crate::daynight::DayNightRes>().0.planet(recipe).ok()?;
    let from = w.resource::<crate::daynight::DayClock>().t;
    match (ts.hour, ts.sun_elevation_deg) {
        (Some(h), _) => ps.time_for_hour(ground, h, from),
        (None, Some(e)) => ps.time_for_elevation(ground, e, ts.evening, from),
        (None, None) => None,
    }
}

/// One viewpoint: camera and sun there, walker on the spot, wait for the terrain, hold, shoot.
/// With `time` the sun is the clock's, stopped at that time of day, and the shot is `<time id>.png`.
fn shoot(out: PathBuf, vps: Viewpoints, k: usize, time: Option<TimeShot>) -> Step {
    Box::new(move |w, c: &mut Ctx| {
        let vp = &vps.viewpoints[k];
        let pl = w.resource::<PlanetRes>().clone();
        let name = w.resource::<SystemRes>().0.planet(pl.id).name.clone();
        let shot_id = time.as_ref().map_or(vp.id.clone(), |t| t.id.clone());
        if c.t == 0.0 {
            let Some((pos, rot, ground)) = camera_pose(&pl, vp) else {
                let line = format!("look {name} {}: SKIP, the planet has no spot '{}'", vp.id, vp.spot);
                println!("{line}");
                c.report.push(line);
                return true;
            };
            if let Some(ts) = &time {
                let Some(t) = shot_time(w, &pl, ground, ts) else {
                    let line = format!("look {name} {}: SKIP, the sun never gets there at '{}'", ts.id, vp.id);
                    println!("{line}");
                    c.report.push(line);
                    return true;
                };
                w.resource_mut::<crate::daynight::Sun>().fixed = None;
                *w.resource_mut::<crate::daynight::DayClock>() = crate::daynight::DayClock { t, rate: 0.0 };
            } else {
                set_sun(w, sun_for(&vps, vp, pos, rot, pl.centre));
            }
            w.resource_mut::<ViewState>().look = Some((pos, rot));
            place_walker(w, pl.centre + ground * pl.surface(ground));
            c.v.insert("look_stage", 0.0);
            c.v.insert("look_t", 0.0);
            c.phase = "look settle".into();
            let s = pl.pgen.sample(to_v3(ground));
            println!("look {name} {}: spot biome {}, {:.1} m above sea, slope {:.1} deg", vp.id, s.biome, s.height_above_sea, s.slope_deg);
            return false;
        }
        let stage = c.v["look_stage"];
        if stage == 0.0 && (c.t > 1.0 && settled(w) || c.t > SETTLE_LIMIT) {
            c.v.insert("look_stage", 1.0);
            c.v.insert("look_t", c.t);
            c.phase = format!("look {} {}", name.to_lowercase(), shot_id);
        } else if stage == 1.0 && c.t - c.v["look_t"] >= HOLD_SECS {
            use bevy::render::view::screenshot::{save_to_disk, Screenshot};
            if let Some(sv) = w.get_resource::<crate::scatter::ScatterView>() {
                let line = format!("look {name} {}: scatter {} instances in {} cells", vp.id, sv.instances, sv.shown_cells);
                println!("{line}");
                c.report.push(line);
            }
            let dir = planet_dir(&out, &name);
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("{shot_id}.png"));
            let sun = w.resource::<crate::daynight::Sun>();
            if time.is_some() {
                let line = format!("look {name} {shot_id}: hour {:.2}, sun {:.1} deg, sun {:.0} lux, night light {:.0} lux, ambient {:.0}, {}", sun.hour.unwrap_or(f64::NAN), sun.elevation_deg, sun.light.sun_lux, sun.light.night_lux, sun.light.ambient, path.display());
                println!("{line}");
                c.report.push(line);
            }
            w.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            c.v.insert("look_stage", 2.0);
            c.v.insert("look_t", c.t);
            c.v.insert("look_shots", c.v.get("look_shots").copied().unwrap_or(0.0) + 1.0);
            c.phase = "look shot".into();
        } else if stage == 2.0 && c.t - c.v["look_t"] >= 0.3 {
            c.phase.clear();
            return true;
        }
        false
    })
}

pub fn steps(s: &mut Vec<Step>, out_dir: &Path, windowed: bool) {
    let vps = Viewpoints::from_json(VIEWPOINTS).expect("content/look/viewpoints.json");
    let out = out_dir.to_path_buf();
    s.push(atlas_step(out.clone(), vps.clone()));
    if !windowed {
        return;
    }
    // Dev filter: EXO_LOOK=<planet>[:<viewpoint>,...] shoots only those (quick iterations).
    let filter = std::env::var("EXO_LOOK").ok().filter(|f| !f.is_empty());
    let (only_planet, only_vps) = match filter.as_deref().map(|f| f.split_once(':').unwrap_or((f, ""))) {
        Some((p, v)) => (Some(p.to_lowercase()), v.split(',').filter(|x| !x.is_empty()).map(String::from).collect::<Vec<_>>()),
        None => (None, Vec::new()),
    };
    let sys = warp_core::System::from_json(crate::warp::SYSTEM).expect("system.json");
    let n = sys.planets.len();
    for i in 0..n {
        if only_planet.as_ref().is_some_and(|p| *p != sys.planets[i].name.to_lowercase()) {
            continue;
        }
        s.push(go_to(i));
        s.push(crate::scenario::settle());
        for k in 0..vps.viewpoints.len() {
            if !only_vps.is_empty() && !only_vps.contains(&vps.viewpoints[k].id) {
                continue;
            }
            s.push(shoot(out.clone(), vps.clone(), k, None));
        }
        for ts in &vps.times {
            if !only_vps.is_empty() && !only_vps.contains(&ts.id) {
                continue;
            }
            let k = vps.viewpoints.iter().position(|v| v.id == ts.viewpoint).expect("validated");
            s.push(shoot(out.clone(), vps.clone(), k, Some(ts.clone())));
        }
    }
    s.push(Box::new(move |w, c| {
        w.resource_mut::<ViewState>().look = None;
        w.resource_mut::<crate::daynight::Sun>().fixed = None;
        w.resource_mut::<crate::daynight::DayClock>().rate = 1.0;
        let shots = c.v.get("look_shots").copied().unwrap_or(0.0) as usize;
        check(c, shots > 0, format!("look: {shots} screenshots for {n} planets"));
        true
    }));
    s.push(wait(1.0));
}

/// `site-walk` (#70): the walker starts 40 m from the first ruin (a flat pad), walks in at 1.8 m/s and
/// stops at its centre. Checks it stands on the ground of the edited height function and that
/// the ground there is flat.
pub fn site_walk_steps(s: &mut Vec<Step>) {
    use crate::scenario::{face_towards, keys, player_world, with_player};
    s.push(Box::new(|w, c| {
        let pl = w.resource::<PlanetRes>().clone();
        let Some(site) = pl.pgen.sites.iter().find(|s| s.id == "ruin").or(pl.pgen.sites.first()).cloned() else {
            check(c, false, "site-walk: the planet has no site".into());
            return true;
        };
        let (e, _) = planet_core::look::tangent_frame(site.dir);
        let start = from_v3(walk(site.dir, e, 40.0, pl.radius));
        let centre = pl.centre + from_v3(site.dir) * pl.surface(from_v3(site.dir));
        place_walker(w, pl.centre + start * pl.surface(start));
        face_towards(w, centre);
        with_player(w, |p| p.w.cfg.walk_speed = 1.8);
        w.resource_mut::<Ring>().force_update();
        c.p.insert("site", centre);
        c.v.insert("site_flat", site.ground_m);
        let line = format!("site-walk: to a '{}' site, 40 m", site.id);
        println!("{line}");
        c.report.push(line);
        true
    }));
    s.push(crate::scenario::settle());
    s.push(Box::new(|w, c| {
        let target = c.p["site"];
        if c.t == 0.0 {
            face_towards(w, target);
            keys(w, &[KeyCode::KeyW], true);
        }
        let p = player_world(w);
        let near = (p - target).length() < 2.0;
        if near || c.t > 45.0 {
            keys(w, &[KeyCode::KeyW], false);
            c.v.insert("site_reached", near as u8 as f64);
            c.v.insert("site_walk_t", c.t);
            return true;
        }
        false
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let pl = w.resource::<PlanetRes>().clone();
        let p = player_world(w);
        let grounded = with_player(w, |p| p.w.grounded);
        let dir = (p - pl.centre).normalize();
        let feet = (p - pl.centre).length() - pl.radius;
        let ground = pl.pgen.height_at(to_v3(dir));
        let base = pl.pgen.base_height_at(to_v3(dir));
        let slope = pl.pgen.sample(to_v3(dir)).slope_deg;
        check(c, c.v["site_reached"] == 1.0, format!("site-walk: reached the site centre in {:.1} s", c.v["site_walk_t"]));
        check(c, grounded && (feet - ground).abs() < 0.15, format!("site-walk: standing on the edited ground (feet {feet:.2} m, ground {ground:.2} m, noise ground {base:.2} m, flatten level {:.2} m)", c.v["site_flat"]));
        check(c, slope < 3.0, format!("site-walk: ground under the walker is flat ({slope:.2} deg)"));
        true
    }));
}
