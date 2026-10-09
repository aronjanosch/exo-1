//! The crew's progress: track values, owned tags, bought unlocks, raised flags (#125). Applies the
//! world events that touch it; the rest belong to the systems.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::content::{Content, Owner, WALLET};
use crate::event::{Event, WorldEvent};
use crate::notice::{Arg, Notice, NoticeKind};
use crate::id::{ClientId, Flag, LocationId, Tag, TrackId, UnlockId};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    crew: BTreeMap<TrackId, i64>,
    players: BTreeMap<ClientId, BTreeMap<TrackId, i64>>,
    tags: BTreeSet<Tag>,
    unlocks: BTreeSet<UnlockId>,
    flags: BTreeSet<Flag>,
}

/// Why an event was not applied. The state is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    UnknownUnlock,
    AlreadyOwned,
    ConditionNotMet,
    NotEnoughMoney { price: i64, wallet: i64 },
    UnknownTrack,
    /// A personal track changed without naming the player.
    NoPlayer,
}

impl Progress {
    /// A new crew: crew tracks at their start values.
    pub fn new(content: &Content) -> Progress {
        let crew = content.tracks.values().filter(|t| t.record.owner == Owner::Crew).map(|t| (t.record.id.clone(), t.record.start)).collect();
        Progress { crew, ..Progress::default() }
    }

    /// A track's value; a personal track is read for `player` (a player without a value yet has
    /// the start value). None for an unknown track or a personal track without a player.
    pub fn value(&self, content: &Content, track: &TrackId, player: Option<ClientId>) -> Option<i64> {
        let t = &content.tracks.get(track)?.record;
        match t.owner {
            Owner::Crew => Some(self.crew.get(track).copied().unwrap_or(t.start)),
            Owner::Player => Some(self.players.get(&player?).and_then(|m| m.get(track)).copied().unwrap_or(t.start)),
        }
    }

    /// How many thresholds the value has reached (0 below the first).
    pub fn level(&self, content: &Content, track: &TrackId, player: Option<ClientId>) -> usize {
        let (Some(t), Some(v)) = (content.tracks.get(track), self.value(content, track, player)) else { return 0 };
        t.record.thresholds.iter().take_while(|th| v >= **th).count()
    }

    pub fn wallet(&self) -> i64 {
        self.crew.get(&TrackId::new(WALLET)).copied().unwrap_or(0)
    }

    pub fn has_tag(&self, t: &Tag) -> bool {
        self.tags.contains(t)
    }

    pub fn has_flag(&self, f: &Flag) -> bool {
        self.flags.contains(f)
    }

    pub fn owns(&self, u: &UnlockId) -> bool {
        self.unlocks.contains(u)
    }

    /// Whether the crew may use a location now.
    pub fn location_available(&self, content: &Content, id: &LocationId) -> bool {
        content.locations.get(id).is_some_and(|l| l.record.available.as_ref().is_none_or(|c| c.holds(content, self, None)))
    }

    /// Applies one world event. Events that are not about progress are accepted and change
    /// nothing. Deduplication is the caller's (`Dedup`), once for all systems.
    pub fn apply(&mut self, content: &Content, ev: &Event<WorldEvent>) -> Result<(), Refusal> {
        self.apply_with_notices(content, ev).map(|_| ())
    }

    /// `apply`, and the notices the event raises (#165): an unlock bought, a track level reached.
    pub fn apply_with_notices(&mut self, content: &Content, ev: &Event<WorldEvent>) -> Result<Vec<Notice>, Refusal> {
        let level_before = match &ev.payload {
            WorldEvent::TrackChanged { track, player, .. } => Some(self.level(content, track, *player)),
            _ => None,
        };
        self.apply_event(content, ev)?;
        let mut out = Vec::new();
        match &ev.payload {
            WorldEvent::UnlockBought { unlock } => {
                if let Some(u) = content.unlocks.get(unlock) {
                    out.push(Notice::new(NoticeKind::Unlock, "notice.unlock").arg("name", Arg::Key(u.record.name.clone())));
                }
            }
            WorldEvent::TrackChanged { track, player, .. } => {
                let level = self.level(content, track, *player);
                if Some(level) > level_before
                    && let Some(t) = content.tracks.get(track)
                {
                    out.push(Notice::new(NoticeKind::Rank, "notice.rank").arg("track", Arg::Key(t.record.name.clone())).arg("level", Arg::Number(level as i64)));
                }
            }
            _ => {}
        }
        Ok(out)
    }

    fn apply_event(&mut self, content: &Content, ev: &Event<WorldEvent>) -> Result<(), Refusal> {
        match &ev.payload {
            WorldEvent::UnlockBought { unlock } => self.buy(content, unlock, ev.sender()),
            WorldEvent::TrackChanged { track, delta, player } => self.change(content, track, *delta, *player),
            WorldEvent::FlagRaised { flag } => {
                self.flags.insert(flag.clone());
                Ok(())
            }
            WorldEvent::PlayerJoined => {
                let m = self.players.entry(ev.sender()).or_default();
                for t in content.tracks.values().filter(|t| t.record.owner == Owner::Player) {
                    m.entry(t.record.id.clone()).or_insert(t.record.start);
                }
                Ok(())
            }
            WorldEvent::CratePickedUp { .. } | WorldEvent::CrateDelivered { .. } | WorldEvent::CrateLost { .. } | WorldEvent::TimePassed { .. } => Ok(()),
        }
    }

    fn buy(&mut self, content: &Content, id: &UnlockId, buyer: ClientId) -> Result<(), Refusal> {
        let u = &content.unlocks.get(id).ok_or(Refusal::UnknownUnlock)?.record;
        if self.owns(id) {
            return Err(Refusal::AlreadyOwned);
        }
        if let Some(c) = &u.condition
            && !c.holds(content, self, Some(buyer))
        {
            return Err(Refusal::ConditionNotMet);
        }
        let wallet = self.wallet();
        if wallet < u.price {
            return Err(Refusal::NotEnoughMoney { price: u.price, wallet });
        }
        self.crew.insert(TrackId::new(WALLET), wallet - u.price);
        self.unlocks.insert(id.clone());
        self.tags.extend(u.grants.iter().cloned());
        Ok(())
    }

    fn change(&mut self, content: &Content, track: &TrackId, delta: i64, player: Option<ClientId>) -> Result<(), Refusal> {
        let t = &content.tracks.get(track).ok_or(Refusal::UnknownTrack)?.record;
        let v = match t.owner {
            Owner::Crew => self.crew.entry(track.clone()).or_insert(t.start),
            Owner::Player => self.players.entry(player.ok_or(Refusal::NoPlayer)?).or_default().entry(track.clone()).or_insert(t.start),
        };
        *v += delta;
        Ok(())
    }
}
