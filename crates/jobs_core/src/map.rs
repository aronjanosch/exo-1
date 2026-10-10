//! The map's pin list (#166), without engine types: which places the crew may use, where the
//! tracked job leads next, and where the players are. The window draws the list on a picture of
//! the ground (`planet_core`'s local map); the same list feeds the job line's pointer and later
//! the HUD arrow (#136).
use gameplay_core::{ClientId, Content, LocationId, Progress, TextKey};

use crate::id::JobId;
use crate::job::{CrateMark, Job, JobState, Jobs};

/// Where a location's pad is, as the glue knows it: the planet (index in the system) and the
/// direction from the planet's centre (a unit vector).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacePos {
    pub location: LocationId,
    pub planet: u32,
    pub dir: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerPos {
    pub who: ClientId,
    pub planet: u32,
    pub dir: [f64; 3],
}

/// What the tracked job asks for next.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Crates still wait to be picked up there.
    Pickup,
    /// Everything is carried: bring it there.
    Dropoff,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PinKind {
    Place,
    /// The tracked job's next stop.
    Target(Stop),
    Player { own: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub kind: PinKind,
    /// A text key: the location's name, or `map.you` / `map.crew` for players.
    pub label: TextKey,
    pub planet: u32,
    pub dir: [f64; 3],
}

/// The next stop of an active job: the pickup of the first leg that still has crates waiting (or
/// no crates yet), else the dropoff of the first leg with crates in hand. None when nothing is
/// left to do or the job is not active.
pub fn next_stop(job: &Job) -> Option<(LocationId, Stop)> {
    if job.state != JobState::Active {
        return None;
    }
    for l in &job.legs {
        if l.resolved() {
            continue;
        }
        if l.crates.is_empty() || l.crates.values().any(|m| *m == CrateMark::Waiting) {
            return Some((l.from.clone(), Stop::Pickup));
        }
        if l.crates.values().any(|m| *m == CrateMark::Carried) {
            return Some((l.to.clone(), Stop::Dropoff));
        }
    }
    None
}

/// The pins for `planet`: the places the crew may use (unknown ones and those of other planets
/// left out), then the tracked job's target at its stop (shown even if the place is not open yet:
/// it is where the job leads), then the players, `own` first.
#[allow(clippy::too_many_arguments)]
pub fn pins(kernel: &Content, progress: &Progress, jobs: &Jobs, tracked: Option<JobId>, places: &[PlacePos], players: &[PlayerPos], own: ClientId, planet: u32) -> Vec<Pin> {
    let mut out = Vec::new();
    for p in places.iter().filter(|p| p.planet == planet) {
        if let Some(l) = kernel.locations.get(&p.location)
            && progress.location_available(kernel, &p.location)
        {
            out.push(Pin { kind: PinKind::Place, label: l.record.name.clone(), planet, dir: p.dir });
        }
    }
    if let Some((loc, stop)) = tracked.and_then(|j| jobs.get(j)).and_then(next_stop)
        && let Some(p) = places.iter().find(|p| p.planet == planet && p.location == loc)
        && let Some(l) = kernel.locations.get(&loc)
    {
        out.push(Pin { kind: PinKind::Target(stop), label: l.record.name.clone(), planet, dir: p.dir });
    }
    let mut ps: Vec<&PlayerPos> = players.iter().filter(|p| p.planet == planet).collect();
    ps.sort_by_key(|p| (p.who != own, p.who));
    for p in ps {
        let mine = p.who == own;
        out.push(Pin { kind: PinKind::Player { own: mine }, label: TextKey::new(if mine { "map.you" } else { "map.crew" }), planet, dir: p.dir });
    }
    out
}
