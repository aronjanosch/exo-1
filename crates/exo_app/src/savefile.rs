//! The host's save file (#135): the #128 envelope as JSON in `saves/` in the game's own folder
//! (next to where it runs; scripted and headless runs use `<out_dir>/saves`). Autosave after job
//! events and on an interval, on the game clock; the save is loaded once at start. A client id is
//! made once per game folder (`saves/client_id`). No network here: sending the state on join is #134.
//!
//! A load is a restart of the gameplay state: the goods crates go, progress, jobs and customers
//! come from the save, the crates of active jobs come back where they were, with their condition.
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use gameplay_core::save::{CratePlace, CrateSave, Envelope, SaveError, WorldSave};
use gameplay_core::{ClientId, CrateId};
use std::path::{Path, PathBuf};

use crate::cargo::{crate_bundle, Crate, Crates};
use crate::env::PlanetRes;
use crate::gameplay::{Gameplay, Goods};

/// The one slot for now. TODO(initiator): slots and a load menu.
pub const SLOT: &str = "autosave";
/// Longest time between autosaves (s, game clock). TODO(initiator).
pub const AUTOSAVE_INTERVAL_S: f64 = 60.0;
/// Shortest time between saves after job events (s): a burst of events writes once. TODO(initiator).
pub const AUTOSAVE_GAP_S: f64 = 2.0;

/// Where the saves live.
#[derive(Resource, Clone, Debug)]
pub struct SaveDir(pub PathBuf);

impl SaveDir {
    pub fn default_dir() -> PathBuf {
        PathBuf::from("saves")
    }
}

/// This game folder's client id (#134 will send it on join; until then the host plays as
/// `gameplay::HOST`).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct LocalClient(pub ClientId);

/// A save waiting to be applied (at start, or by a scenario's restart); applied once the pads of
/// the planet are known.
#[derive(Resource, Clone, Debug)]
pub struct PendingLoad(pub Envelope);

/// Autosave pacing and what it did.
#[derive(Resource, Clone, Debug, Default)]
pub struct Autosave {
    since_write: f64,
    pub writes: u32,
    pub last_error: Option<String>,
}

impl Autosave {
    /// Advances the clock; true when a save is due: soon after a job event, or on the interval.
    pub fn due(&mut self, dt: f64, wanted: bool) -> bool {
        self.since_write += dt;
        let due = (wanted && self.since_write >= AUTOSAVE_GAP_S) || self.since_write >= AUTOSAVE_INTERVAL_S;
        if due {
            self.since_write = 0.0;
        }
        due
    }
}

pub fn plugin(app: &mut App, dir: PathBuf, load_at_start: bool) {
    match client_id(&dir) {
        Ok(id) => {
            app.insert_resource(LocalClient(id));
        }
        Err(e) => println!("save: no client id in {}: {e}", dir.display()),
    }
    if load_at_start {
        match read_slot(&dir, SLOT) {
            Ok(Some(text)) => match Envelope::from_json(&text) {
                Ok(env) => {
                    app.insert_resource(PendingLoad(env));
                }
                Err(e) => println!("save: {} not loaded: {e}", dir.join(format!("{SLOT}.json")).display()),
            },
            Ok(None) => {}
            Err(e) => println!("save: {e}"),
        }
    }
    app.insert_resource(SaveDir(dir)).init_resource::<Autosave>();
    app.add_systems(FixedUpdate, (apply_pending_load, autosave).chain().after(crate::gameplay::gameplay_step).in_set(crate::phases::Fx::Cargo));
}

/// Writes `text` as the slot, through a temporary file and a rename, so a crash mid-write never
/// leaves half a save.
pub fn write_slot(dir: &Path, slot: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{slot}.json.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, dir.join(format!("{slot}.json")))
}

/// The slot's text; None when there is no such save.
pub fn read_slot(dir: &Path, slot: &str) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(dir.join(format!("{slot}.json"))) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// The folder's client id: read from `client_id`, made once when missing or unreadable.
pub fn client_id(dir: &Path) -> std::io::Result<ClientId> {
    let path = dir.join("client_id");
    if let Ok(t) = std::fs::read_to_string(&path)
        && let Ok(n) = t.trim().parse::<u64>()
    {
        return Ok(ClientId(n));
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    // Never 0 nor the host's stand-in id.
    let id = gameplay_core::rng::Rng::new(nanos ^ u64::from(std::process::id())).next_u64().max(2);
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, id.to_string())?;
    Ok(ClientId(id))
}

/// The whole save: gameplay sections plus the world's crates of active jobs. The ship's pose is
/// not saved yet (TODO(initiator): with the ship's place in the world, E).
pub fn snapshot<'a>(gp: &Gameplay, planet: &PlanetRes, crates: impl Iterator<Item = (&'a Crate, &'a Goods)>) -> Envelope {
    let mut env = Envelope::new();
    gp.save_to(&mut env);
    let mut world = WorldSave { next_crate: gp.next_crate(), crates: Vec::new(), ship: None };
    for (c, g) in crates {
        let place = if c.ship.is_some() {
            CratePlace::Ship { pos: c.body.pos.to_array() }
        } else {
            CratePlace::Planet { planet: u32::from(c.planet.unwrap_or(planet.id).0), pos: (c.body.pos - planet.centre).to_array() }
        };
        world.crates.push(CrateSave { id: g.id, commodity: g.commodity.clone(), job: Some(g.job.0), condition: c.condition, place });
    }
    world.crates.sort_by_key(|c| c.id);
    world.save(&mut env);
    env
}

/// Saves now; the error is kept and printed, the game goes on.
pub fn save_now(world: &mut World) -> Result<(), String> {
    let mut q = world.query::<(&Crate, &Goods)>();
    let env = snapshot(world.resource::<Gameplay>(), world.resource::<PlanetRes>(), q.iter(world));
    let dir = world.resource::<SaveDir>().0.clone();
    write_slot(&dir, SLOT, &env.to_json()).map_err(|e| format!("save: {}: {e}", dir.display()))?;
    world.resource_mut::<Gameplay>().save_wanted = false;
    world.resource_mut::<Autosave>().writes += 1;
    Ok(())
}

fn autosave(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let wanted = world.resource::<Gameplay>().save_wanted;
    if !world.resource_mut::<Autosave>().due(dt, wanted) {
        return;
    }
    if let Err(e) = save_now(world) {
        let mut a = world.resource_mut::<Autosave>();
        if a.last_error.as_ref() != Some(&e) {
            println!("{e}");
        }
        a.last_error = Some(e);
    }
}

fn apply_pending_load(world: &mut World) {
    if !world.contains_resource::<PendingLoad>() || world.resource::<Gameplay>().pads.is_empty() {
        return;
    }
    let PendingLoad(env) = world.remove_resource::<PendingLoad>().unwrap();
    if let Err(e) = load(world, &env) {
        println!("save: not loaded: {e}");
    }
}

/// A restart from `env`: the goods crates of the running game go, the saved state and crates come.
pub fn load(world: &mut World, env: &Envelope) -> Result<(), SaveError> {
    let saved = WorldSave::load(env)?.unwrap_or_default();
    world.resource_mut::<Gameplay>().restore(env, saved.next_crate)?;
    let old: Vec<Entity> = world.query_filtered::<Entity, With<Goods>>().iter(world).collect();
    for e in old {
        world.despawn(e);
    }
    let ship = world.query_filtered::<Entity, With<crate::ship::Ship>>().iter(world).next();
    let (centre, here) = {
        let p = world.resource::<PlanetRes>();
        (p.centre, u32::from(p.id.0))
    };
    for c in saved.crates {
        let Some(job) = c.job else { continue };
        let (pos, in_ship) = match c.place {
            CratePlace::Planet { planet, pos } if planet == here => (centre + DVec3::from_array(pos), None),
            CratePlace::Ship { pos } if ship.is_some() => (DVec3::from_array(pos), ship),
            _ => {
                // TODO(initiator): crates on another planet wait in the save until the players warp there.
                world.resource_mut::<Gameplay>().log.push(format!("save: crate {} is not here, left out", c.id.0));
                continue;
            }
        };
        let gp = world.resource::<Gameplay>();
        let size = gp.kernel.commodities[&c.commodity].record.crate_size.clone();
        let up = if in_ship.is_some() { DVec3::Y } else { (pos - centre).normalize() };
        let on_pad = if in_ship.is_some() { None } else { gp.pad_at(pos).map(|p| p.location.clone()) };
        let fwd = DQuat::from_rotation_arc(DVec3::Y, up) * DVec3::NEG_Z;
        let bundle = crate_bundle(&world.resource::<Crates>().0, &size, in_ship, pos, fwd);
        let mut e = world.spawn((bundle, Goods { id: CrateId(c.id.0), commodity: c.commodity, job: jobs_core::JobId(job), on_pad }));
        e.get_mut::<Crate>().unwrap().condition = c.condition;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("exo-saves-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn the_client_id_is_made_once_and_kept() {
        let d = dir("client");
        let a = client_id(&d).unwrap();
        let b = client_id(&d).unwrap();
        assert_eq!(a, b);
        assert!(a.0 >= 2, "never 0 nor the host's stand-in");
    }

    #[test]
    fn a_slot_is_written_whole_and_read_back() {
        let d = dir("slot");
        assert_eq!(read_slot(&d, SLOT).unwrap(), None);
        write_slot(&d, SLOT, "{\"a\": 1}").unwrap();
        write_slot(&d, SLOT, "{\"a\": 2}").unwrap();
        assert_eq!(read_slot(&d, SLOT).unwrap().as_deref(), Some("{\"a\": 2}"));
        assert!(!d.join(format!("{SLOT}.json.tmp")).exists());
    }

    #[test]
    fn a_burst_of_job_events_saves_once_and_quiet_play_saves_on_the_interval() {
        let mut a = Autosave::default();
        let step = 1.0 / 60.0;
        let mut writes = 0;
        // Events every tick for 5 s: one save per gap, not one per event.
        for _ in 0..300 {
            writes += a.due(step, true) as u32;
        }
        assert_eq!(writes, (5.0 / AUTOSAVE_GAP_S) as u32);
        // One more event writes; after it nothing happens: the next save comes with the interval.
        while !a.due(step, true) {}
        let mut quiet = step;
        while !a.due(step, false) {
            quiet += step;
        }
        assert!((quiet - AUTOSAVE_INTERVAL_S).abs() < 0.1, "{quiet}");
    }
}
