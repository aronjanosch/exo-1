//! Who holds which crate on the network (#332). The host owns this book: it gives every crate a
//! stable id, lets one player hold a crate at a time (first ask wins), and frees what a player
//! held when they leave. No engine types; ids are never reused within a session.
use std::collections::BTreeMap;

pub type CrateId = u32;
pub type PlayerId = u32;

/// Why an ask was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Another player holds it.
    Taken(PlayerId),
    /// The asker already holds another crate (hands full).
    HandsFull(CrateId),
    /// No such crate.
    Unknown,
}

#[derive(Clone, Debug, Default)]
pub struct Holders {
    next: CrateId,
    known: BTreeMap<CrateId, Option<PlayerId>>,
}

impl Holders {
    /// A new crate: its id (1, 2, ...), never given again.
    pub fn register(&mut self) -> CrateId {
        self.next += 1;
        self.known.insert(self.next, None);
        self.next
    }

    /// A crate the other side told us about (a client mirrors the host's book).
    pub fn learn(&mut self, id: CrateId, by: Option<PlayerId>) {
        self.next = self.next.max(id);
        self.known.insert(id, by);
    }

    pub fn holder(&self, id: CrateId) -> Option<PlayerId> {
        self.known.get(&id).copied().flatten()
    }

    pub fn held_by(&self, player: PlayerId) -> Option<CrateId> {
        self.known.iter().find(|(_, h)| **h == Some(player)).map(|(id, _)| *id)
    }

    pub fn knows(&self, id: CrateId) -> bool {
        self.known.contains_key(&id)
    }

    /// `player` asks to hold `id`. The same player asking again for the crate they hold is fine.
    pub fn ask(&mut self, id: CrateId, player: PlayerId) -> Result<(), Refusal> {
        match self.known.get(&id) {
            None => Err(Refusal::Unknown),
            Some(Some(p)) if *p == player => Ok(()),
            Some(Some(p)) => Err(Refusal::Taken(*p)),
            Some(None) => {
                if let Some(other) = self.held_by(player) {
                    return Err(Refusal::HandsFull(other));
                }
                self.known.insert(id, Some(player));
                Ok(())
            }
        }
    }

    /// `player` lets go of `id`; false if they did not hold it (nothing changes).
    pub fn release(&mut self, id: CrateId, player: PlayerId) -> bool {
        match self.known.get_mut(&id) {
            Some(h) if *h == Some(player) => {
                *h = None;
                true
            }
            _ => false,
        }
    }

    /// A player left: what they held is free again; the crates, in id order.
    pub fn player_left(&mut self, player: PlayerId) -> Vec<CrateId> {
        let mut freed = Vec::new();
        for (id, h) in self.known.iter_mut() {
            if *h == Some(player) {
                *h = None;
                freed.push(*id);
            }
        }
        freed
    }

    /// The crate is gone (despawned); its id stays used.
    pub fn forget(&mut self, id: CrateId) {
        self.known.remove(&id);
    }

    pub fn ids(&self) -> impl Iterator<Item = CrateId> + '_ {
        self.known.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_ask_wins_and_the_second_is_told_who_holds_it() {
        let mut h = Holders::default();
        let c = h.register();
        assert_eq!(h.ask(c, 1), Ok(()));
        assert_eq!(h.ask(c, 2), Err(Refusal::Taken(1)));
        assert_eq!(h.holder(c), Some(1));
        assert_eq!(h.ask(c, 1), Ok(()), "asking again for the own crate changes nothing");
    }

    #[test]
    fn only_the_holder_can_let_go() {
        let mut h = Holders::default();
        let c = h.register();
        h.ask(c, 2).unwrap();
        assert!(!h.release(c, 3), "a stranger's release is ignored");
        assert_eq!(h.holder(c), Some(2));
        assert!(h.release(c, 2));
        assert_eq!(h.holder(c), None);
        assert_eq!(h.ask(c, 3), Ok(()), "free again");
    }

    #[test]
    fn a_player_holds_one_crate_at_a_time() {
        let mut h = Holders::default();
        let (a, b) = (h.register(), h.register());
        h.ask(a, 2).unwrap();
        assert_eq!(h.ask(b, 2), Err(Refusal::HandsFull(a)));
        h.release(a, 2);
        assert_eq!(h.ask(b, 2), Ok(()));
    }

    #[test]
    fn a_holder_who_leaves_frees_the_crate() {
        let mut h = Holders::default();
        let (a, b) = (h.register(), h.register());
        h.ask(a, 4).unwrap();
        h.ask(b, 5).unwrap();
        assert_eq!(h.player_left(4), vec![a]);
        assert_eq!((h.holder(a), h.holder(b)), (None, Some(5)));
        assert!(h.player_left(4).is_empty());
    }

    #[test]
    fn ids_are_never_reused_and_unknown_crates_are_refused() {
        let mut h = Holders::default();
        let a = h.register();
        h.forget(a);
        let b = h.register();
        assert!(b > a, "{a} then {b}");
        assert_eq!(h.ask(a, 1), Err(Refusal::Unknown));
        let mut mirror = Holders::default();
        mirror.learn(7, Some(2));
        assert!(mirror.register() > 7, "a mirror never hands out a heard id");
    }
}
