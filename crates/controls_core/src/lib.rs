//! Physical controls kit (#315): lever with detents, toggle, push button, cover (guard) and slot.
//!
//! Plain state machines, no Bevy. The host owns one [`Controls`] per machine; a client sends only
//! "player pressed F on part X (holding item Y)" and the host calls [`Controls::press`]. Every part
//! is its own entry, so two players on two parts of one machine never touch each other (guideline
//! rule 2: lock per part, not per machine). A part is named like its glTF node.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An item as the host knows it (opaque here).
pub type ItemId = u32;

/// Part names are glTF node names, e.g. `lever_start`.
pub type PartId = String;

/// How long a push button stays down, in seconds (visual; the action fires on the press).
pub const BUTTON_DOWN_S: f64 = 0.25;

/// What a lever does at its top detent when F is pressed again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeverEnd {
    /// Steps back down one detent at a time, so no press ever skips a detent.
    #[default]
    Bounce,
    /// Goes round to detent 0 (a rotary lever: OFF, PRIME, RUN, OFF; F on RUN is the orderly shutdown).
    Wrap,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    /// `index` in `0..detents`; F steps up, at the top it follows `end`.
    Lever { detents: u8, index: u8, rising: bool, end: LeverEnd },
    Toggle { on: bool },
    /// Springs back; `down_s` counts the visual down time left.
    Button { down_s: f64 },
    /// A hinged cover. It guards other parts through their `guard` field.
    Cover { open: bool },
    Slot { item: Option<ItemId> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub kind: Kind,
    /// A cover part; this part answers F only while that cover is open.
    pub guard: Option<PartId>,
}

/// What a press did. The glue turns it into a sound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    /// A lever clicked into `index`, or a toggle flipped (`index` 0 or 1), or a cover moved (`open`).
    Lever { index: u8 },
    Toggle { on: bool },
    Cover { open: bool },
    /// A push button fired.
    Fired,
    /// A lever step the machine refused (the lever stays put).
    Refused,
    /// The part's cover is shut.
    Covered,
    /// Nothing to do (button still down, slot refuses or is full, empty hands on an empty slot).
    Nothing,
    Seated(ItemId),
    /// The item came out; the host gives it to the player's hands.
    Pulled(ItemId),
}

/// What the player has in hands when pressing F.
pub type Hand = Option<ItemId>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    parts: BTreeMap<PartId, Part>,
}

impl Controls {
    pub fn new() -> Self {
        Self::default()
    }

    fn add(&mut self, id: &str, kind: Kind, guard: Option<&str>) -> &mut Self {
        self.parts.insert(id.into(), Part { kind, guard: guard.map(Into::into) });
        self
    }

    /// `detents` is 2 or 3 in the kit; any count of at least 2 works.
    pub fn lever(&mut self, id: &str, detents: u8, guard: Option<&str>) -> &mut Self {
        self.lever_with(id, detents, LeverEnd::Bounce, guard)
    }
    /// A lever with its own end behaviour.
    pub fn lever_with(&mut self, id: &str, detents: u8, end: LeverEnd, guard: Option<&str>) -> &mut Self {
        self.add(id, Kind::Lever { detents: detents.max(2), index: 0, rising: true, end }, guard)
    }
    pub fn toggle(&mut self, id: &str, guard: Option<&str>) -> &mut Self {
        self.add(id, Kind::Toggle { on: false }, guard)
    }
    pub fn button(&mut self, id: &str, guard: Option<&str>) -> &mut Self {
        self.add(id, Kind::Button { down_s: 0.0 }, guard)
    }
    pub fn cover(&mut self, id: &str) -> &mut Self {
        self.add(id, Kind::Cover { open: false }, None)
    }
    pub fn slot(&mut self, id: &str) -> &mut Self {
        self.add(id, Kind::Slot { item: None }, None)
    }

    pub fn part(&self, id: &str) -> Option<&Part> {
        self.parts.get(id)
    }

    pub fn parts(&self) -> impl Iterator<Item = (&PartId, &Part)> {
        self.parts.iter()
    }

    /// The part's visible position for animation: lever detent index, toggle/cover 0 or 1,
    /// button 1 while down, slot 1 while seated.
    pub fn position(&self, id: &str) -> f64 {
        match self.parts.get(id).map(|p| &p.kind) {
            Some(Kind::Lever { index, .. }) => *index as f64,
            Some(Kind::Toggle { on }) => *on as u8 as f64,
            Some(Kind::Cover { open }) => *open as u8 as f64,
            Some(Kind::Button { down_s }) => (*down_s > 0.0) as u8 as f64,
            Some(Kind::Slot { item }) => item.is_some() as u8 as f64,
            None => 0.0,
        }
    }

    pub fn lever_index(&self, id: &str) -> Option<u8> {
        match self.parts.get(id)?.kind {
            Kind::Lever { index, .. } => Some(index),
            _ => None,
        }
    }
    pub fn is_on(&self, id: &str) -> bool {
        matches!(self.parts.get(id).map(|p| &p.kind), Some(Kind::Toggle { on: true }))
    }
    pub fn is_open(&self, id: &str) -> bool {
        matches!(self.parts.get(id).map(|p| &p.kind), Some(Kind::Cover { open: true }))
    }
    pub fn seated(&self, id: &str) -> Option<ItemId> {
        match self.parts.get(id)?.kind {
            Kind::Slot { item } => item,
            _ => None,
        }
    }

    /// F on a part. `allow_detent(index)` lets the machine refuse a lever step (START without its
    /// conditions); `accepts(item)` lets a slot refuse an item. Unknown part: `Nothing`.
    pub fn press(&mut self, id: &str, hand: Hand, allow_detent: impl Fn(u8) -> bool, accepts: impl Fn(ItemId) -> bool) -> Outcome {
        let Some(guard) = self.parts.get(id).map(|p| p.guard.clone()) else { return Outcome::Nothing };
        if guard.is_some_and(|g| !self.is_open(&g)) {
            return Outcome::Covered;
        }
        let Some(part) = self.parts.get_mut(id) else { return Outcome::Nothing };
        match &mut part.kind {
            Kind::Lever { detents, index, rising, end } => {
                let wrap = *end == LeverEnd::Wrap && *index + 1 >= *detents;
                let next = if wrap {
                    0
                } else if *rising {
                    *index + 1
                } else {
                    index.saturating_sub(1)
                };
                if !allow_detent(next) {
                    return Outcome::Refused;
                }
                *index = next;
                if *end == LeverEnd::Wrap {
                    *rising = true;
                } else if next + 1 >= *detents {
                    *rising = false;
                } else if next == 0 {
                    *rising = true;
                }
                Outcome::Lever { index: next }
            }
            Kind::Toggle { on } => {
                *on = !*on;
                Outcome::Toggle { on: *on }
            }
            Kind::Button { down_s } => {
                if *down_s > 0.0 {
                    return Outcome::Nothing;
                }
                *down_s = BUTTON_DOWN_S;
                Outcome::Fired
            }
            Kind::Cover { open } => {
                *open = !*open;
                Outcome::Cover { open: *open }
            }
            Kind::Slot { item } => match (*item, hand) {
                (None, Some(h)) if accepts(h) => {
                    *item = Some(h);
                    Outcome::Seated(h)
                }
                (Some(i), None) => {
                    *item = None;
                    Outcome::Pulled(i)
                }
                _ => Outcome::Nothing,
            },
        }
    }

    /// Springs buttons back.
    pub fn tick(&mut self, dt: f64) {
        for p in self.parts.values_mut() {
            if let Kind::Button { down_s } = &mut p.kind {
                *down_s = (*down_s - dt).max(0.0);
            }
        }
    }

    /// The machine drops a lever or toggle back to off by itself (a SCRAM resets the boot
    /// toggles). Returns whether it moved; a lever falls straight to 0 and rises again.
    pub fn drop_to_off(&mut self, id: &str) -> bool {
        match self.parts.get_mut(id).map(|p| &mut p.kind) {
            Some(Kind::Lever { index, rising, .. }) if *index > 0 => {
                *index = 0;
                *rising = true;
                true
            }
            Some(Kind::Toggle { on }) if *on => {
                *on = false;
                true
            }
            _ => false,
        }
    }

    /// The machine takes the seated item out (it was used up or ejected).
    pub fn clear_slot(&mut self, id: &str) -> Option<ItemId> {
        match self.parts.get_mut(id).map(|p| &mut p.kind) {
            Some(Kind::Slot { item }) => item.take(),
            _ => None,
        }
    }

    /// Closes a cover from the machine side (e.g. after a SCRAM).
    pub fn close_cover(&mut self, id: &str) {
        if let Some(Kind::Cover { open }) = self.parts.get_mut(id).map(|p| &mut p.kind) {
            *open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yes(_: u8) -> bool {
        true
    }
    fn any(_: ItemId) -> bool {
        true
    }

    #[test]
    fn guarded_button_cannot_fire_with_cover_shut() {
        let mut c = Controls::new();
        c.cover("cover_scram").button("button_scram", Some("cover_scram"));
        assert_eq!(c.press("button_scram", None, yes, any), Outcome::Covered);
        assert_eq!(c.position("button_scram"), 0.0);
        assert_eq!(c.press("cover_scram", None, yes, any), Outcome::Cover { open: true });
        assert_eq!(c.press("button_scram", None, yes, any), Outcome::Fired);
        c.tick(1.0);
        assert_eq!(c.press("cover_scram", None, yes, any), Outcome::Cover { open: false });
        assert_eq!(c.press("button_scram", None, yes, any), Outcome::Covered);
    }

    #[test]
    fn button_springs_back_and_fires_once_per_press() {
        let mut c = Controls::new();
        c.button("b", None);
        assert_eq!(c.press("b", None, yes, any), Outcome::Fired);
        assert_eq!(c.position("b"), 1.0);
        assert_eq!(c.press("b", None, yes, any), Outcome::Nothing);
        c.tick(BUTTON_DOWN_S + 0.01);
        assert_eq!(c.position("b"), 0.0);
        assert_eq!(c.press("b", None, yes, any), Outcome::Fired);
    }

    #[test]
    fn lever_never_skips_a_detent() {
        let mut c = Controls::new();
        c.lever("l", 3, None);
        let mut seen = vec![0u8];
        for _ in 0..12 {
            if let Outcome::Lever { index } = c.press("l", None, yes, any) {
                seen.push(index);
            }
        }
        assert!(seen.windows(2).all(|w| w[0].abs_diff(w[1]) == 1), "{seen:?}");
        assert_eq!(seen.iter().max(), Some(&2));
    }

    #[test]
    fn rotary_lever_wraps_from_the_top_to_off() {
        let mut c = Controls::new();
        c.lever_with("l", 3, LeverEnd::Wrap, None);
        let seen: Vec<u8> = (0..7).filter_map(|_| if let Outcome::Lever { index } = c.press("l", None, yes, any) { Some(index) } else { None }).collect();
        assert_eq!(seen, [1, 2, 0, 1, 2, 0, 1]);
        // A refused wrap (the machine will not go off) leaves the lever on top.
        let mut c = Controls::new();
        c.lever_with("l", 3, LeverEnd::Wrap, None);
        c.press("l", None, yes, any);
        c.press("l", None, yes, any);
        assert_eq!(c.press("l", None, |i| i != 0, any), Outcome::Refused);
        assert_eq!(c.lever_index("l"), Some(2));
        // The machine's drop to off keeps the climb going.
        assert!(c.drop_to_off("l"));
        assert_eq!(c.press("l", None, yes, any), Outcome::Lever { index: 1 });
    }

    #[test]
    fn refused_step_leaves_lever_put_and_direction_intact() {
        let mut c = Controls::new();
        c.lever("l", 3, None);
        assert_eq!(c.press("l", None, |i| i <= 1, any), Outcome::Lever { index: 1 });
        // PRIME reached; RUN refused.
        assert_eq!(c.press("l", None, |_| false, any), Outcome::Refused);
        assert_eq!(c.lever_index("l"), Some(1));
        // The same press with conditions met goes on up, not down.
        assert_eq!(c.press("l", None, yes, any), Outcome::Lever { index: 2 });
    }

    #[test]
    fn slot_holds_at_most_one_item() {
        let mut c = Controls::new();
        c.slot("s");
        assert_eq!(c.press("s", Some(7), yes, any), Outcome::Seated(7));
        assert_eq!(c.press("s", Some(8), yes, any), Outcome::Nothing);
        assert_eq!(c.seated("s"), Some(7));
        assert_eq!(c.press("s", None, yes, any), Outcome::Pulled(7));
        assert_eq!(c.seated("s"), None);
        assert_eq!(c.press("s", None, yes, any), Outcome::Nothing);
        assert_eq!(c.press("s", Some(8), yes, |i| i != 8), Outcome::Nothing);
    }

    #[test]
    fn two_players_on_two_parts_of_one_machine() {
        let mut c = Controls::new();
        c.lever("lever_start", 3, None).slot("slot_1").toggle("toggle_boot_1", None);
        // Player A on the lever, player B seats a canister in the same tick.
        assert_eq!(c.press("lever_start", None, yes, any), Outcome::Lever { index: 1 });
        assert_eq!(c.press("slot_1", Some(3), yes, any), Outcome::Seated(3));
        assert_eq!(c.press("toggle_boot_1", None, yes, any), Outcome::Toggle { on: true });
        assert_eq!(c.lever_index("lever_start"), Some(1));
        assert_eq!(c.seated("slot_1"), Some(3));
    }

    #[test]
    fn machine_can_drop_lever_and_toggle_to_off() {
        let mut c = Controls::new();
        c.lever("l", 3, None).toggle("t", None).button("b", None);
        c.press("l", None, yes, any);
        c.press("l", None, yes, any);
        c.press("t", None, yes, any);
        assert!(c.drop_to_off("l") && c.drop_to_off("t"));
        assert_eq!((c.lever_index("l"), c.is_on("t")), (Some(0), false));
        assert!(!c.drop_to_off("l") && !c.drop_to_off("b"));
        // After a drop the lever climbs from the bottom again.
        assert_eq!(c.press("l", None, yes, any), Outcome::Lever { index: 1 });
    }
}
