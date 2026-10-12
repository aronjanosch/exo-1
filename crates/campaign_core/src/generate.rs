//! The star system generator (#274): a template and a seed give one concrete system. Every
//! concern draws from its own stream (`Rng::derive`), so a change in one part does not shift the
//! others.
use crate::names::Names;
use crate::rng::Rng;
use crate::set::PlanetSet;
use crate::template::{EnemyKind, SystemKind, Template};
use glam::DVec3;
use warp_core::{DriveConfig, PlanetDef, PlanetId, StarDef, System};

const KEY_PLANETS: u64 = 0x504C_414E;
const KEY_LAYOUT: u64 = 0x4C41_5954;
const KEY_STAR: u64 = 0x5354_4152;
const KEY_STATIONS: u64 = 0x5354_4154;
const KEY_SITES: u64 = 0x5349_5445;
const KEY_ENEMY: u64 = 0x454E_454D;

const LAYOUT_TRIES: u32 = 64;
const STAR_TRIES: u32 = 32;
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
        if self.names.planet.len() < t.planets.count[1] as usize {
            return Err(format!("names.planet: {} entries, the template needs {}", self.names.planet.len(), t.planets.count[1]));
        }
        if self.names.station.len() < t.stations.count[1] as usize {
            return Err(format!("names.station: {} entries, the template needs {}", self.names.station.len(), t.stations.count[1]));
        }

        let mut rng = Rng::derive(seed, KEY_PLANETS);
        let count = rng.count(t.planets.count[0], t.planets.count[1]) as usize;
        let mut names = shuffled(&mut rng, &self.names.planet).into_iter();

        let centres = self.layout(t, seed, count)?;
        let mut planets = Vec::with_capacity(count);
        for centre in centres {
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
                centre,
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

        let mut rng = Rng::derive(seed, KEY_STAR);
        let mut star = None;
        for _ in 0..STAR_TRIES {
            let s = DVec3::from_array(planets[0].centre)
                + rng.direction() * rng.range(t.star.distance_m[0], t.star.distance_m[1]);
            if planets.iter().all(|p| sky_ok(p, s)) {
                star = Some(s);
                break;
            }
        }
        let star = star.ok_or("no star position the sky allows")?;

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

    /// Planet centres: planet 0 at the origin, each next one a gap from the one before it.
// TODO(initiator): planets are scattered 8 to 16 million m apart, not on orbit rings round the star; real orbits?
    fn layout(&self, t: &Template, seed: u64, count: usize) -> Result<Vec<[f64; 3]>, String> {
        let mut rng = Rng::derive(seed, KEY_LAYOUT);
        let gap = t.planets.gap_m;
        let mut centres: Vec<DVec3> = vec![DVec3::ZERO];
        for i in 1..count {
            let mut next = None;
            for _ in 0..LAYOUT_TRIES {
                let c = centres[i - 1] + rng.direction() * rng.range(gap[0], gap[1]);
                if centres.iter().all(|e| e.distance(c) >= gap[0]) {
                    next = Some(c);
                    break;
                }
            }
            centres.push(next.ok_or_else(|| format!("no room for planet {i}"))?);
        }
        Ok(centres.into_iter().map(|c| c.to_array()).collect())
    }
}

/// A point above a planet's atmosphere at a random altitude in `altitude_m`.
fn above(rng: &mut Rng, p: &PlanetDef, altitude_m: [f64; 2]) -> [f64; 3] {
    let dir = rng.direction();
    let alt = rng.range(altitude_m[0], altitude_m[1]);
    (p.centre() + dir * (p.radius + p.atmosphere_height + alt)).to_array()
}
