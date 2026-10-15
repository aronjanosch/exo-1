//! Which planet the simulation holds, and when its replacement is generated. One rule set for
//! the game and the tests, without tasks or Bevy: the caller keeps at most one generation running
//! in the background and does what `Loader::step` says.
use crate::drive::{Drive, Phase};
use crate::system::{PlanetId, System};
use glam::DVec3;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    Keep,
    /// Start generating this planet, replacing any other generation.
    Start(PlanetId),
    Discard,
    /// Make this planet the simulation's: the finished generation, or, if there is none, wait
    /// for the running one or generate it here and now.
    Swap(PlanetId),
}

#[derive(Clone, Debug, Default)]
pub struct Loader {
    /// The planet the simulation should switch to; kept across ticks after a drop.
    wanted: Option<PlanetId>,
    /// The wanted planet comes from a drop: it is taken at the end of the post-ramp at the latest,
    /// wherever the ship is.
    after_drop: bool,
}

impl Loader {
    /// `dropped`: an emergency drop ended this tick.
    pub fn step(&mut self, sys: &System, current: PlanetId, pos: DVec3, drive: &Drive, dropped: bool, pending: Pending) -> Load {
        let zone = sys.frame_of(pos);
        let rails = drive.phase.on_rails();
        if rails {
            self.after_drop = false;
            self.wanted = zone.filter(|z| Some(*z) == drive.target);
        } else if zone.is_some() {
            self.wanted = zone;
        } else if dropped {
            self.wanted = Some(sys.nearest(pos));
        }
        self.after_drop |= dropped;
        if self.wanted == Some(current) {
            self.wanted = None;
            self.after_drop = false;
        }
        if let Some(f) = self.wanted {
            // Off rails there is no flight left to hide the generation in: inside the zone, or once
            // the drop's post-ramp is over.
            let now = !rails && if self.after_drop { drive.phase != Phase::PostRampDown } else { zone == Some(f) };
            if pending == Pending::Ready(f) || now {
                self.wanted = None;
                self.after_drop = false;
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
            (Some(p), _) if p == current => Load::Discard,
            (Some(p), Some(t)) if p != t => Load::Discard,
            _ => Load::Keep,
        }
    }
}
