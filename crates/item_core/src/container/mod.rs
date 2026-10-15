//! Taken from Barotrauma: a stack is capped by the smaller of the slot's and the item's cap, slots
//! filter items by tag lists, a container can be locked or need a carried key item, and the preset
//! slot counts follow its cabinets and crates (`presets`, for the cabinet step).
//! Every operation either succeeds or hands everything back: nothing is lost or duplicated.
use std::collections::BTreeSet;
use std::fmt;

pub mod presets;

use serde::{Deserialize, Serialize};

macro_rules! text_id {
    ($($(#[$m:meta])* $name:ident;)*) => {$(
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> $name {
                $name(s.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    )*};
}

text_id! {
    ItemKind;
    Tag;
}

/// One item. Two items stack when they are equal in every field, payload included.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item<P = ()> {
    pub kind: ItemKind,
    pub tags: BTreeSet<Tag>,
    /// How many of it fit in one slot (at least 1).
    pub max_stack: u32,
    pub payload: Option<P>,
    /// Condition in percent (0..=100) for an item that wears (an extinguisher); None for
    /// one that does not. Two items with different conditions do not stack.
    #[serde(default)]
    pub condition: Option<f64>,
}

impl<P> Item<P> {
    /// An item without payload. A `max_stack` of 0 counts as 1.
    pub fn new(kind: &str, tags: &[&str], max_stack: u32) -> Item<P> {
        Item { kind: ItemKind::new(kind), tags: tags.iter().map(|t| Tag::new(*t)).collect(), max_stack: max_stack.max(1), payload: None, condition: None }
    }

    pub fn with_payload(mut self, p: P) -> Item<P> {
        self.payload = Some(p);
        self
    }

    /// The item wears: it starts at `percent` (0..=100).
    pub fn with_condition(mut self, percent: f64) -> Item<P> {
        self.condition = Some(percent.clamp(0.0, 100.0));
        self
    }

    pub fn has_tag(&self, t: &Tag) -> bool {
        self.tags.contains(t)
    }
}

/// A number of equal items. Outside a container a stack may be larger than one slot holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stack<P = ()> {
    pub item: Item<P>,
    pub count: u32,
}

impl<P: Clone> Stack<P> {
    pub fn new(item: Item<P>, count: u32) -> Stack<P> {
        Stack { item, count }
    }

    pub fn split(self, n: u32) -> (Stack<P>, Option<Stack<P>>) {
        let n = n.min(self.count);
        let rest = (n < self.count).then(|| Stack { item: self.item.clone(), count: self.count - n });
        (Stack { item: self.item, count: n }, rest)
    }
}

/// What a slot takes: an item with any `allow` tag (empty: anything) and no `deny` tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotRule {
    pub max_stack: u32,
    pub allow: BTreeSet<Tag>,
    pub deny: BTreeSet<Tag>,
}

impl SlotRule {
    pub fn any(max_stack: u32) -> SlotRule {
        SlotRule { max_stack, allow: BTreeSet::new(), deny: BTreeSet::new() }
    }

    pub fn tagged(max_stack: u32, allow: &[&str], deny: &[&str]) -> SlotRule {
        SlotRule { max_stack, allow: allow.iter().map(|t| Tag::new(*t)).collect(), deny: deny.iter().map(|t| Tag::new(*t)).collect() }
    }

    pub fn accepts<P>(&self, item: &Item<P>) -> bool {
        (self.allow.is_empty() || self.allow.iter().any(|t| item.has_tag(t))) && !self.deny.iter().any(|t| item.has_tag(t))
    }

    pub fn cap<P>(&self, item: &Item<P>) -> u32 {
        self.max_stack.min(item.max_stack)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    Open,
    Locked,
    /// Whoever carries an item with this tag (an id card).
    NeedsKey(Tag),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Locked,
    NeedsKey(Tag),
    NotAllowed,
    Full,
    NoSuchSlot,
    EmptySlot,
    NothingToMove,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Refused<P = ()> {
    pub why: Refusal,
    pub stack: Stack<P>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    pub slots: usize,
    pub used_slots: usize,
    pub items: u32,
}

impl Capacity {
    pub fn free_slots(&self) -> usize {
        self.slots - self.used_slots
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Container<P = ()> {
    rules: Vec<SlotRule>,
    slots: Vec<Option<Stack<P>>>,
    access: Access,
}

impl<P: Clone + PartialEq> Container<P> {
    pub fn new(rules: Vec<SlotRule>, access: Access) -> Container<P> {
        let slots = vec![None; rules.len()];
        Container { rules, slots, access }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn rule(&self, slot: usize) -> &SlotRule {
        &self.rules[slot]
    }

    pub fn slot(&self, slot: usize) -> Option<&Stack<P>> {
        self.slots.get(slot)?.as_ref()
    }

    /// The stack in a slot, to change it in place (its item's condition, say).
    pub fn slot_mut(&mut self, slot: usize) -> Option<&mut Stack<P>> {
        self.slots.get_mut(slot)?.as_mut()
    }

    pub fn access(&self) -> &Access {
        &self.access
    }

    pub fn set_access(&mut self, a: Access) {
        self.access = a;
    }

    pub fn check_access(&self, carried: &[Tag]) -> Result<(), Refusal> {
        match &self.access {
            Access::Open => Ok(()),
            Access::Locked => Err(Refusal::Locked),
            Access::NeedsKey(k) if carried.contains(k) => Ok(()),
            Access::NeedsKey(k) => Err(Refusal::NeedsKey(k.clone())),
        }
    }

    pub fn count(&self, kind: &ItemKind) -> u32 {
        self.slots.iter().flatten().filter(|s| &s.item.kind == kind).map(|s| s.count).sum()
    }

    pub fn capacity(&self) -> Capacity {
        let used: Vec<&Stack<P>> = self.slots.iter().flatten().collect();
        Capacity { slots: self.slots.len(), used_slots: used.len(), items: used.iter().map(|s| s.count).sum() }
    }

    /// Puts as much of `stack` in as fits: equal stacks are topped up first, then empty slots
    /// take the rest, in slot order. Returns the part that did not fit (None: all went in). A
    /// refusal means nothing went in and gives the stack back.
    pub fn try_insert(&mut self, carried: &[Tag], stack: Stack<P>) -> Result<Option<Stack<P>>, Refused<P>> {
        if let Err(why) = self.check_access(carried) {
            return Err(Refused { why, stack });
        }
        if stack.count == 0 {
            return Err(Refused { why: Refusal::NothingToMove, stack });
        }
        if !self.rules.iter().any(|r| r.accepts(&stack.item)) {
            return Err(Refused { why: Refusal::NotAllowed, stack });
        }
        let mut left = stack.count;
        for (rule, slot) in self.rules.iter().zip(self.slots.iter_mut()) {
            if let Some(s) = slot
                && s.item == stack.item
                && rule.accepts(&s.item)
            {
                let n = left.min(rule.cap(&s.item).saturating_sub(s.count));
                s.count += n;
                left -= n;
            }
        }
        for (rule, slot) in self.rules.iter().zip(self.slots.iter_mut()) {
            if left == 0 {
                break;
            }
            if slot.is_none() && rule.accepts(&stack.item) {
                let n = left.min(rule.cap(&stack.item));
                *slot = Some(Stack { item: stack.item.clone(), count: n });
                left -= n;
            }
        }
        match left {
            0 => Ok(None),
            n if n == stack.count => Err(Refused { why: Refusal::Full, stack }),
            n => Ok(Some(Stack { item: stack.item, count: n })),
        }
    }

    pub fn take(&mut self, carried: &[Tag], slot: usize, count: u32) -> Result<Stack<P>, Refusal> {
        self.check_access(carried)?;
        let cell = self.slots.get_mut(slot).ok_or(Refusal::NoSuchSlot)?;
        let s = cell.take().ok_or(Refusal::EmptySlot)?;
        if count == 0 {
            *cell = Some(s);
            return Err(Refusal::NothingToMove);
        }
        let (part, rest) = s.split(count);
        *cell = rest;
        Ok(part)
    }

    /// Puts a stack back into the slot it was just taken from (it fits there by construction).
    fn put_back(&mut self, slot: usize, stack: Stack<P>) {
        match &mut self.slots[slot] {
            Some(s) => s.count += stack.count,
            cell @ None => *cell = Some(stack),
        }
    }

    /// Puts the whole stack into one slot: an empty one, or one with equal items and room for
    /// all of it. A refusal gives the stack back.
    pub fn insert_at(&mut self, carried: &[Tag], slot: usize, stack: Stack<P>) -> Result<(), Refused<P>> {
        if let Err(why) = self.check_access(carried) {
            return Err(Refused { why, stack });
        }
        if stack.count == 0 {
            return Err(Refused { why: Refusal::NothingToMove, stack });
        }
        let Some(rule) = self.rules.get(slot) else { return Err(Refused { why: Refusal::NoSuchSlot, stack }) };
        if !rule.accepts(&stack.item) {
            return Err(Refused { why: Refusal::NotAllowed, stack });
        }
        let cap = rule.cap(&stack.item);
        match &mut self.slots[slot] {
            None if stack.count <= cap => self.slots[slot] = Some(stack),
            Some(s) if s.item == stack.item && s.count + stack.count <= cap => s.count += stack.count,
            _ => return Err(Refused { why: Refusal::Full, stack }),
        }
        Ok(())
    }

    /// Moves the stack of slot `from` onto slot `to` of the same container: into an empty slot,
    /// onto equal items as far as the cap allows (the rest stays), or the two stacks swap places.
    pub fn move_slot(&mut self, carried: &[Tag], from: usize, to: usize) -> Result<(), Refusal> {
        self.check_access(carried)?;
        if from >= self.slots.len() || to >= self.slots.len() {
            return Err(Refusal::NoSuchSlot);
        }
        if from == to {
            return Err(Refusal::NothingToMove);
        }
        let Some(moving) = self.slots[from].clone() else { return Err(Refusal::EmptySlot) };
        if !self.rules[to].accepts(&moving.item) {
            return Err(Refusal::NotAllowed);
        }
        match self.slots[to].clone() {
            None => {
                if moving.count > self.rules[to].cap(&moving.item) {
                    return Err(Refusal::Full);
                }
                self.slots[to] = self.slots[from].take();
            }
            Some(there) if there.item == moving.item => {
                let room = self.rules[to].cap(&there.item).saturating_sub(there.count);
                if room == 0 {
                    return Err(Refusal::Full);
                }
                let n = room.min(moving.count);
                if let Some(s) = self.slots[to].as_mut() {
                    s.count += n;
                }
                let left = moving.count - n;
                self.slots[from] = (left > 0).then(|| Stack { item: moving.item, count: left });
            }
            Some(there) => {
                let fits_back = self.rules[from].accepts(&there.item) && there.count <= self.rules[from].cap(&there.item);
                if !fits_back || moving.count > self.rules[to].cap(&moving.item) {
                    return Err(Refusal::NotAllowed);
                }
                self.slots.swap(from, to);
            }
        }
        Ok(())
    }
}

/// Moves up to `count` items from one slot of `from` into `to`; what does not fit stays in its
/// slot. Returns how many moved. The user needs access to both.
pub fn move_between<P: Clone + PartialEq>(from: &mut Container<P>, to: &mut Container<P>, carried: &[Tag], slot: usize, count: u32) -> Result<u32, Refusal> {
    to.check_access(carried)?;
    let part = from.take(carried, slot, count)?;
    let n = part.count;
    match to.try_insert(carried, part) {
        Ok(None) => Ok(n),
        Ok(Some(left)) => {
            let moved = n - left.count;
            from.put_back(slot, left);
            Ok(moved)
        }
        Err(r) => {
            from.put_back(slot, r.stack);
            Err(r.why)
        }
    }
}
