//! The star system generator: a template and a seed give one concrete system. Every
//! concern draws from its own stream (`Rng::derive`), so a change in one part does not shift the
//! others.
use crate::names::Names;
use crate::rng::Rng;
use crate::set::PlanetSet;
use crate::template::{EnemyKind, SystemKind, Template};
use glam::DVec3;
use std::f64::consts::TAU;
use warp_core::{DriveConfig, PlanetDef, PlanetId, StarDef, System};

const KEY_PLANETS: u64 = 0x504C_414E;
const KEY_LAYOUT: u64 = 0x4C41_5954;
const KEY_STATIONS: u64 = 0x5354_4154;
const KEY_SITES: u64 = 0x5349_5445;
const KEY_ENEMY: u64 = 0x454E_454D;

const LAYOUT_TRIES: u32 = 32;
const COLOUR_JITTER: f64 = 0.08;

// Copies of the constants in content/system/system.json.
// TODO(initiator): per template or per recipe?
const JUMP_ALTITUDE_FACTOR: f64 = 1.5;
const OBSTRUCTION_MARGIN: f64 = 0.0;
const ARRIVAL_RADIUS: f64 = 12_000.0;
const FRAME_RADIUS: f64 = 1_000_000.0;

#[derive(Clone, Debug)]
pub struct Generator {
    pub set: PlanetSet,
    pub names: Names,
    pub drive: DriveConfig,
}

#[derive(Clone, Debug)]
pub struct Generated {
    pub name: String,
    pub kind: SystemKind,
    pub seed: u64,
    pub system: System,
    pub stations: Vec<Station>,
    pub sites: Vec<Site>,
    pub enemy: Option<Enemy>,
    pub events: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    pub name: String,
    pub planet: PlanetId,
    pub position: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub planet: PlanetId,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub planet: PlanetId,
    pub position: [f64; 3],
}

pub fn open_sky(_: &PlanetDef, _: DVec3) -> bool {
    true
}

/// A copy of `pool` in random order (partial Fisher-Yates).
fn shuffled<T: Clone>(rng: &mut Rng, pool: &[T]) -> Vec<T> {
    let mut v = pool.to_vec();
    let n = v.len();
    for i in 0..n {
        let j = i + rng.count(0, (n - 1 - i) as u32) as usize;
        v.swap(i, j);
    }
    v
}

/// `n` planet indices: every planet once while there are some left, then random ones.
fn planet_slots(rng: &mut Rng, planets: usize, n: usize) -> Vec<usize> {
    let order = shuffled(rng, &(0..planets).collect::<Vec<usize>>());
    (0..n)
        .map(|k| if k < planets { order[k] } else { rng.count(0, (planets - 1) as u32) as usize })
        .collect()
}

impl Generator {
    pub fn new(set: PlanetSet, names: Names, drive: DriveConfig) -> Generator {
        Generator { set, names, drive }
    }

    pub fn generate(&self, t: &Template, seed: u64, sky_ok: &dyn Fn(&PlanetDef, DVec3) -> bool) -> Result<Generated, String> {
        t.check_set(&self.set)?;
        if t.orbits.ring_gap_m[0] <= 2.0 * FRAME_RADIUS {
            return Err(format!("orbits.ring_gap_m: lo must be > {} (twice the frame radius)", 2.0 * FRAME_RADIUS));
        }
        if self.names.planet.len() < t.planets.count[1] as usize {
            return Err(format!("names.planet: {} entries, the template needs {}", self.names.planet.len(), t.planets.count[1]));
        }
        if self.names.station.len() < t.stations.count[1] as usize {
            return Err(format!("names.station: {} entries, the template needs {}", self.names.station.len(), t.stations.count[1]));
        }

        let mut rng = Rng::derive(seed, KEY_PLANETS);
        let count = rng.count(t.planets.count[0], t.planets.count[1]) as usize;
        let mut names = shuffled(&mut rng, &self.names.planet).into_iter();

        let mut planets = Vec::with_capacity(count);
        for _ in 0..count {
            let id = rng.pick(&t.planets.recipes).ok_or("planets.recipes: empty")?;
            let recipe = self.set.get(id).ok_or_else(|| format!("unknown recipe {id}"))?;
            let mut color = recipe.color;
            for c in &mut color {
                *c = (f64::from(*c) + rng.range(-COLOUR_JITTER, COLOUR_JITTER)).clamp(0.0, 1.0) as f32;
            }
            let radius = rng.range(t.planets.radius_m[0], t.planets.radius_m[1]);
            planets.push(PlanetDef {
                name: names.next().ok_or("names.planet: too few entries")?,
                recipe: id.clone(),
                seed: rng.count(1, 1_000_000) as i32,
                centre: [0.0; 3],
                radius,
                atmosphere_height: rng.range(t.planets.atmosphere_height_m[0], t.planets.atmosphere_height_m[1]),
                jump_altitude_factor: JUMP_ALTITUDE_FACTOR,
                obstruction_radius: radius + recipe.terrain_m,
                obstruction_margin: OBSTRUCTION_MARGIN,
                arrival_radius: ARRIVAL_RADIUS,
                frame_radius: FRAME_RADIUS,
                terrain_ceiling_m: None,
                color,
            });
        }

        let (centres, star) = layout(t, seed, &planets, sky_ok)?;
        for (p, c) in planets.iter_mut().zip(centres) {
            p.centre = c.to_array();
        }

        let mut rng = Rng::derive(seed, KEY_STATIONS);
        let n = rng.count(t.stations.count[0], t.stations.count[1]) as usize;
        let slots = planet_slots(&mut rng, planets.len(), n);
        let mut names = shuffled(&mut rng, &self.names.station).into_iter();
        let mut stations = Vec::with_capacity(n);
        for i in slots {
            let position = above(&mut rng, &planets[i], t.stations.altitude_m);
            stations.push(Station {
                name: names.next().ok_or("names.station: too few entries")?,
                planet: PlanetId(i as u8),
                position,
            });
        }

        let mut rng = Rng::derive(seed, KEY_SITES);
        let n = rng.count(t.sites.count[0], t.sites.count[1]) as usize;
        let slots = planet_slots(&mut rng, planets.len(), n);
        let mut sites = Vec::with_capacity(n);
        for i in slots {
            sites.push(Site {
                planet: PlanetId(i as u8),
                kind: rng.pick(&t.sites.kinds).ok_or("sites.kinds: empty")?.clone(),
            });
        }

        let mut rng = Rng::derive(seed, KEY_ENEMY);
        let enemy = match &t.enemy {
            None => None,
            Some(rule) => {
                let kind = *rng.pick(&rule.kinds).ok_or("enemy.kinds: empty")?;
                let i = rng.count(0, (planets.len() - 1) as u32) as usize;
                let position = above(&mut rng, &planets[i], rule.altitude_m);
                Some(Enemy { kind, planet: PlanetId(i as u8), position })
            }
        };

        let system = System::new(
            StarDef { position: star.to_array() },
            planets,
            self.drive.clone(),
        )?;
        Ok(Generated {
            name: self.names.system_name(seed),
            kind: t.kind,
            seed,
            system,
            stations,
            sites,
            enemy,
            events: t.events.clone(),
        })
    }

}

// TODO(initiator): ring values
/// Planet centres on orbit rings round the star, and the star, shifted so planet 0 is at the
/// origin. The rings share one plane, tilted against the world's XZ plane; a layout the sky hook
/// refuses is drawn again.
fn layout(t: &Template, seed: u64, planets: &[PlanetDef], sky_ok: &dyn Fn(&PlanetDef, DVec3) -> bool) -> Result<(Vec<DVec3>, DVec3), String> {
    let mut rng = Rng::derive(seed, KEY_LAYOUT);
    let o = &t.orbits;
    let mut radii = Vec::with_capacity(planets.len());
    let mut r = rng.range(o.first_ring_m[0], o.first_ring_m[1]);
    for i in 0..planets.len() {
        if i > 0 {
            r += rng.range(o.ring_gap_m[0], o.ring_gap_m[1]);
        }
        radii.push(r);
    }
    for _ in 0..LAYOUT_TRIES {
        let tilt = rng.range(o.plane_tilt_deg[0], o.plane_tilt_deg[1]).to_radians();
        let phi = rng.range(0.0, TAU);
        let e1 = DVec3::new(phi.cos(), 0.0, phi.sin());
        let normal = DVec3::new(-e1.z * tilt.sin(), tilt.cos(), e1.x * tilt.sin());
        let e2 = normal.cross(e1);
        let mut centres = Vec::with_capacity(radii.len());
        for &r in &radii {
            let a = rng.unit() * TAU;
            centres.push(r * (a.cos() * e1 + a.sin() * e2));
        }
        let shift = centres[0];
        for c in &mut centres {
            *c -= shift;
        }
        let star = -shift;
        let ok = planets.iter().zip(&centres).all(|(p, c)| {
            let mut q = p.clone();
            q.centre = c.to_array();
            sky_ok(&q, star)
        });
        if ok {
            return Ok((centres, star));
        }
    }
    Err("no layout and star position the sky allows".into())
}

/// A point above a planet's atmosphere at a random altitude in `altitude_m`.
fn above(rng: &mut Rng, p: &PlanetDef, altitude_m: [f64; 2]) -> [f64; 3] {
    let dir = rng.direction();
    let alt = rng.range(altitude_m[0], altitude_m[1]);
    (p.centre() + dir * (p.radius + p.atmosphere_height + alt)).to_array()
}
