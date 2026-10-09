//! Which planet the simulation holds, and when its replacement is generated. One rule set for
//! the game and the tests, without tasks or Bevy: the caller keeps at most one generation running
//! in the background and does what `Loader::step` says.
//!
//! - On rails only the drive's target counts: a path through another planet's frame zone loads
//!   nothing (#111). The target is generated from the ramp-up on.
//! - A swap takes a finished generation. On rails the ship waits for it in the tunnel; only a
//!   ship off rails inside a frame zone (an arrival before the generation is done, a teleport)
//!   swaps at once and the caller finishes the work in that tick (#113).
//! - After an emergency drop the ship wants the planet of its frame zone, or the nearest one in
//!   open space: generated in the background, swapped when done (the ship drifts meanwhile).
//! - A generation nobody heads for any more (a drop near the old planet, a jump back) is
//!   discarded, so its planet does not stay in memory.
use crate::drive::{Drive, Phase};
use crate::system::{PlanetId, System};
use glam::DVec3;

/// The caller's background generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    None,
    Running(PlanetId),
    Ready(PlanetId),
}

impl Pending {
    fn id(self) -> Option<PlanetId> {
        match self {
            Pending::None => None,
            Pending::Running(p) | Pending::Ready(p) => Some(p),
        }
    }
}

/// What the caller does this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    Keep,
    /// Start generating this planet, replacing any other generation.
    Start(PlanetId),
    /// Drop the generation.
    Discard,
    /// Make this planet the simulation's: the finished generation, or, if there is none, wait
    /// for the running one or generate it here and now.
    Swap(PlanetId),
}

#[derive(Clone, Debug, Default)]
pub struct Loader {
    /// The planet the simulation should switch to; kept across ticks after a drop.
    wanted: Option<PlanetId>,
    /// After a drop the ship drifts far from any ground: it may wait for the generation even
    /// inside a frame zone.
    patient: bool,
}

impl Loader {
    /// `dropped`: an emergency drop ended this tick.
    pub fn step(&mut self, sys: &System, current: PlanetId, pos: DVec3, drive: &Drive, dropped: bool, pending: Pending) -> Load {
        let zone = sys.frame_of(pos);
        let rails = drive.phase.on_rails();
        if rails {
            self.patient = false;
            self.wanted = zone.filter(|z| Some(*z) == drive.target);
        } else if zone.is_some() {
            self.wanted = zone;
        } else if dropped {
            self.wanted = Some(sys.nearest(pos));
        }
        self.patient |= dropped;
        if self.wanted == Some(current) {
            self.wanted = None;
            self.patient = false;
        }
        if let Some(f) = self.wanted {
            // Off rails inside the zone there is no flight left to hide the generation in.
            if pending == Pending::Ready(f) || (!rails && !self.patient && zone == Some(f)) {
                self.wanted = None;
                self.patient = false;
                return Load::Swap(f);
            }
            return if pending == Pending::Running(f) { Load::Keep } else { Load::Start(f) };
        }
        let heading = match drive.phase {
            Phase::Idle | Phase::PostRampDown | Phase::Cooldown => None,
            _ => drive.target.filter(|t| *t != current),
        };
        match (pending.id(), heading) {
            (p, Some(t)) if rails && p != Some(t) => Load::Start(t),
            (Some(p), h) if Some(p) != h => Load::Discard,
            _ => Load::Keep,
        }
    }
}
