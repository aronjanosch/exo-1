//! Captain orders. Taken from Barotrauma: a small order wheel with an optional target per order
//! and the same order record for players and bots; from PULSAR: one ship-wide captain's order
//! shown to everybody, and free-text custom orders.
//!
//! The captain gives one order per role slot and one crew-wide order; a slot order wins over the
//! crew order for that role. An order has a kind, a target that fits the kind, a priority and an
//! optional expiry. Players see it as `OrderLine` (a HUD notice and a marker id); bots read it in
//! `priorities::choose`, where it lifts the matching task over the table.
use std::collections::BTreeMap;

use gameplay_core::TextKey;
use serde::{Deserialize, Serialize};

use crate::{Refusal, Role, Target, captain_only};

/// Free-text order slots.
pub const MAX_CUSTOM: usize = 4;

/// Characters per free-text order.
pub const MAX_CUSTOM_CHARS: usize = 60;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    /// Go to a room.
    GoTo,
    /// Sit at or work a station.
    Operate,
    /// Repair a device.
    Repair,
    /// Put out the fire in a room.
    Extinguish,
    /// Patch the breach in a room.
    FixBreach,
    /// Follow the captain.
    FollowMe,
    /// Stay where you are.
    Stay,
    /// Come to the captain and report.
    Report,
    /// The captain's free-text order in this slot.
    Custom(u8),
}

/// The kind of thing an order points at.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Room,
    Station,
    Device,
}

impl OrderKind {
    /// The order wheel, in the order the console shows it.
    pub const WHEEL: [OrderKind; 8] = [
        OrderKind::GoTo,
        OrderKind::Operate,
        OrderKind::Repair,
        OrderKind::Extinguish,
        OrderKind::FixBreach,
        OrderKind::FollowMe,
        OrderKind::Stay,
        OrderKind::Report,
    ];

    pub fn id(self) -> &'static str {
        match self {
            OrderKind::GoTo => "go_to",
            OrderKind::Operate => "operate",
            OrderKind::Repair => "repair",
            OrderKind::Extinguish => "extinguish",
            OrderKind::FixBreach => "fix_breach",
            OrderKind::FollowMe => "follow_me",
            OrderKind::Stay => "stay",
            OrderKind::Report => "report",
            OrderKind::Custom(_) => "custom",
        }
    }

    /// What the order needs as its target; none for orders without one.
    pub fn target_kind(self) -> Option<TargetKind> {
        match self {
            OrderKind::GoTo | OrderKind::Extinguish | OrderKind::FixBreach => Some(TargetKind::Room),
            OrderKind::Operate => Some(TargetKind::Station),
            OrderKind::Repair => Some(TargetKind::Device),
            OrderKind::FollowMe | OrderKind::Stay | OrderKind::Report | OrderKind::Custom(_) => None,
        }
    }

    fn fits(self, target: &Option<Target>) -> bool {
        matches!(
            (self.target_kind(), target),
            (None, None) | (Some(TargetKind::Room), Some(Target::Room(_))) | (Some(TargetKind::Station), Some(Target::Station(_))) | (Some(TargetKind::Device), Some(Target::Device(_)))
        )
    }
}

/// How hard the captain means it. Bots weigh it on top of the order's lift.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderPriority {
    Low,
    #[default]
    Normal,
    Urgent,
}

/// Who an order is for.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Addressee {
    /// Everybody without an order of their own.
    Crew,
    Slot(Role),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Order {
    /// Running number; also the id of the order's marker.
    pub serial: u64,
    pub kind: OrderKind,
    pub target: Option<Target>,
    pub priority: OrderPriority,
    pub to: Addressee,
    pub issued_s: f64,
    pub expires_s: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OrderEvent {
    /// A new order; `replaced` is the serial of the order it took the place of.
    Given { order: Order, replaced: Option<u64> },
    Cleared { to: Addressee, serial: u64 },
    Expired { to: Addressee, serial: u64 },
}

/// What a player's HUD shows for the order that applies to them.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderLine {
    /// `order.<kind id>`.
    pub key: TextKey,
    /// The captain's text for a free-text order.
    pub text: Option<String>,
    pub target: Option<Target>,
    /// The marker id on the ship (the order's serial).
    pub marker: u64,
    pub priority: OrderPriority,
}

/// The orders in force.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Orders {
    next: u64,
    crew: Option<Order>,
    slots: BTreeMap<Role, Order>,
    custom: [Option<String>; MAX_CUSTOM],
}

impl Orders {
    /// The captain gives an order. `ttl_s`: seconds until it expires by itself.
    #[allow(clippy::too_many_arguments)]
    pub fn give(&mut self, by: Role, to: Addressee, kind: OrderKind, target: Option<Target>, priority: OrderPriority, now_s: f64, ttl_s: Option<f64>) -> Result<OrderEvent, Refusal> {
        captain_only(by)?;
        if to == Addressee::Slot(Role::Captain) {
            return Err(Refusal::NoSuchSlot);
        }
        if !kind.fits(&target) {
            return Err(Refusal::WrongTarget);
        }
        if let OrderKind::Custom(i) = kind
            && self.custom.get(i as usize).is_none_or(Option::is_none)
        {
            return Err(Refusal::NoCustomSlot);
        }
        let order = Order { serial: self.next, kind, target, priority, to, issued_s: now_s, expires_s: ttl_s.map(|t| now_s + t) };
        self.next += 1;
        let old = match to {
            Addressee::Crew => self.crew.replace(order.clone()),
            Addressee::Slot(r) => self.slots.insert(r, order.clone()),
        };
        Ok(OrderEvent::Given { order, replaced: old.map(|o| o.serial) })
    }

    /// The captain takes an order back. None if there was none.
    pub fn clear(&mut self, by: Role, to: Addressee) -> Result<Option<OrderEvent>, Refusal> {
        captain_only(by)?;
        let old = match to {
            Addressee::Crew => self.crew.take(),
            Addressee::Slot(r) => self.slots.remove(&r),
        };
        Ok(old.map(|o| OrderEvent::Cleared { to, serial: o.serial }))
    }

    /// Drops the orders whose time is up.
    pub fn tick(&mut self, now_s: f64) -> Vec<OrderEvent> {
        let due = |o: &Order| o.expires_s.is_some_and(|t| now_s >= t);
        let mut out = Vec::new();
        if self.crew.as_ref().is_some_and(due) {
            let o = self.crew.take().unwrap();
            out.push(OrderEvent::Expired { to: o.to, serial: o.serial });
        }
        let gone: Vec<Role> = self.slots.iter().filter(|(_, o)| due(o)).map(|(r, _)| *r).collect();
        for r in gone {
            let o = self.slots.remove(&r).unwrap();
            out.push(OrderEvent::Expired { to: o.to, serial: o.serial });
        }
        out
    }

    pub fn crew(&self) -> Option<&Order> {
        self.crew.as_ref()
    }

    /// The order that applies to a role: its slot order, else the crew order.
    pub fn for_role(&self, role: Role) -> Option<&Order> {
        self.slots.get(&role).or(self.crew.as_ref())
    }

    /// The HUD line for a role.
    pub fn line_for(&self, role: Role) -> Option<OrderLine> {
        let o = self.for_role(role)?;
        let text = match o.kind {
            OrderKind::Custom(i) => self.custom.get(i as usize).cloned().flatten(),
            _ => None,
        };
        Some(OrderLine { key: TextKey::new(format!("order.{}", o.kind.id())), text, target: o.target.clone(), marker: o.serial, priority: o.priority })
    }

    /// The captain writes a free-text order into a slot; an empty text clears it. A given order
    /// of that slot keeps running and shows the new text.
    pub fn set_custom(&mut self, by: Role, slot: usize, text: &str) -> Result<(), Refusal> {
        captain_only(by)?;
        let text = text.trim();
        if text.chars().count() > MAX_CUSTOM_CHARS {
            return Err(Refusal::TextTooLong);
        }
        let s = self.custom.get_mut(slot).ok_or(Refusal::NoCustomSlot)?;
        *s = (!text.is_empty()).then(|| text.to_string());
        Ok(())
    }

    /// The free-text slots, `MAX_CUSTOM` long.
    pub fn custom(&self) -> &[Option<String>] {
        &self.custom
    }
}
