//! The player's inventory: the action bar, eight slots on the keys 1 to 8. The selected slot is
//! in the hand. Hands: two. A held item takes its `hands`; carried cargo takes its own; what does
//! not fit stays in its slot, lowered (not in the hand, not usable).
use serde::{Deserialize, Serialize};

use crate::container::{Access, Container, Refusal, SlotRule, Stack};
use crate::defs::{ItemDefs, Take};

/// Slots of the action bar.
pub const BAR_SLOTS: usize = 8;
/// Hands of a player.
pub const HANDS: u8 = 2;
/// Most items one bar slot holds (the item's own stack size usually caps it lower).
const SLOT_CAP: u32 = 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub bar: Container,
    /// The slot in the hand (0..BAR_SLOTS).
    pub selected: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory { bar: Container::new(vec![SlotRule::any(SLOT_CAP); BAR_SLOTS], Access::Open), selected: 0 }
    }
}

impl Inventory {
    pub fn slot(&self, i: usize) -> Option<&Stack> {
        self.bar.slot(i)
    }

    /// Whether the whole stack fits (nothing changes).
    pub fn fits(&self, defs: &ItemDefs, stack: &Stack) -> bool {
        self.clone().collect(defs, stack.clone()).is_ok()
    }

    /// Takes a whole stack in, or nothing: equal stacks with room first, then the selected slot
    /// if it is empty (so a collected item lands in the empty hand), then the first free slot.
    /// Only collectable items go in.
    pub fn collect(&mut self, defs: &ItemDefs, stack: Stack) -> Result<(), Refusal> {
        match defs.of(&stack) {
            Some(d) if d.take == Take::Collect => {}
            _ => return Err(Refusal::NotAllowed),
        }
        let mut trial = self.bar.clone();
        let equal_room = (0..BAR_SLOTS).any(|i| trial.slot(i).is_some_and(|s| s.item == stack.item && s.count < trial.rule(i).cap(&s.item)));
        if !equal_room && trial.slot(self.selected).is_none() {
            trial.insert_at(&[], self.selected, stack).map_err(|r| r.why)?;
        } else {
            match trial.try_insert(&[], stack) {
                Ok(None) => {}
                Ok(Some(_)) => return Err(Refusal::Full),
                Err(r) => return Err(r.why),
            }
        }
        self.bar = trial;
        Ok(())
    }

    /// Takes the whole stack out of a slot (to drop or throw it).
    pub fn take_slot(&mut self, slot: usize) -> Result<Stack, Refusal> {
        let n = self.bar.slot(slot).ok_or(if slot < BAR_SLOTS { Refusal::EmptySlot } else { Refusal::NoSuchSlot })?.count;
        self.bar.take(&[], slot, n)
    }

    pub fn select(&mut self, slot: usize) -> Result<(), Refusal> {
        if slot >= BAR_SLOTS {
            return Err(Refusal::NoSuchSlot);
        }
        self.selected = slot;
        Ok(())
    }

    /// Moves, stacks or swaps within the bar (the mouse in the open inventory).
    pub fn move_slot(&mut self, from: usize, to: usize) -> Result<(), Refusal> {
        self.bar.move_slot(&[], from, to)
    }

    /// Hands the selected item needs (0 for an empty slot or an unknown item).
    pub fn held_hands(&self, defs: &ItemDefs) -> u8 {
        self.slot(self.selected).and_then(|s| defs.of(s)).map_or(0, |d| d.hands)
    }

    /// The stack in the hand, if there is one and it fits next to `carrying` hands of cargo.
    pub fn in_hand(&self, defs: &ItemDefs, carrying: u8) -> Option<&Stack> {
        let s = self.slot(self.selected)?;
        let d = defs.of(s)?;
        (d.hands + carrying <= HANDS).then_some(s)
    }

    /// Whether cargo needing `hands` can be picked up next to the held item.
    pub fn may_carry(&self, defs: &ItemDefs, hands: u8) -> bool {
        self.held_hands(defs) + hands <= HANDS
    }

    /// Items of a kind, all slots.
    pub fn count(&self, id: &str) -> u32 {
        (0..BAR_SLOTS).filter_map(|i| self.slot(i)).filter(|s| s.item.kind.as_str() == id).map(|s| s.count).sum()
    }
}
