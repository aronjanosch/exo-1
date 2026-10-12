//! crew_core: the captain's logic without engine types (#288, part of #284, #246, #253).
//!
//! - `alert`: green, yellow, red; set by the captain, automatic yellow on a threat.
//! - `orders`: the order wheel (one order per role slot plus a crew-wide one) and free-text orders.
//! - `priorities`: the bot priority table (five levels per task and bot class, overrides) and the
//!   pure choice a bot makes from open tasks, with hysteresis and station claims.
//! - `budget`: money the captain releases to each role in steps of 100.
//! - `status`: the ship snapshot for the overview and ship state displays, and a seeded
//!   placeholder generator for the dev area.
//! - `mission_view`: display rows over `jobs_core` and the captain-only mission actions.
//!
//! Every function here is plain data in, plain data out; the glue keeps the state in resources,
//! routes the events and draws the panels.
use std::fmt;

use serde::{Deserialize, Serialize};

pub mod alert;
pub mod budget;
pub mod orders;
pub mod priorities;
pub mod status;

pub use alert::{Alert, AlertCause, AlertChange, AlertEvent, AlertLevel};
pub use budget::Budget;
pub use orders::{Addressee, Order, OrderEvent, OrderKind, OrderLine, OrderPriority, Orders, TargetKind};
pub use priorities::{BotId, BotView, Choice, Claims, Level, Pick, PriorityTable, Situation, Task, TaskId, TaskKind, Taker, choose};
pub use status::{PlaceholderStatus, ShipStatus};

/// The five crew roles. One player or bot fills each slot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Captain,
    Pilot,
    Engineer,
    Tactical,
    Scientist,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Captain, Role::Pilot, Role::Engineer, Role::Tactical, Role::Scientist];

    /// The id used in text keys (`role.<id>`).
    pub fn id(self) -> &'static str {
        match self {
            Role::Captain => "captain",
            Role::Pilot => "pilot",
            Role::Engineer => "engineer",
            Role::Tactical => "tactical",
            Role::Scientist => "scientist",
        }
    }
}

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
    /// A room of the ship (`bridge`, `engine_room`), as the ship data names it.
    RoomId;
    /// A station a crew member sits or stands at (`turret_port`, `helm`).
    StationId;
    /// A repairable device (`oxygen_generator`, `reactor`).
    DeviceId;
}

/// What an order or a task points at.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Room(RoomId),
    Station(StationId),
    Device(DeviceId),
}

/// Why a request was not applied. The state is unchanged. The glue shows `refusal.<snake name>`.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// Only the captain may do this (alert, orders, budget, missions).
    NotCaptain,
    /// The order kind needs a target of another kind, or none.
    WrongTarget,
    /// Orders go to the crew or to a role other than the captain's.
    NoSuchSlot,
    /// The free-text order slot does not exist.
    NoCustomSlot,
    /// A free-text order is longer than `orders::MAX_CUSTOM_CHARS`.
    TextTooLong,
    /// Allowances move in whole steps of `budget::STEP`; spending needs a positive amount.
    BadAmount,
    /// The captain has less unreleased money than one step.
    NotEnoughAvailable { available: i64 },
    /// The role's released money does not cover the price.
    OverAllowance { remaining: i64, asked: i64 },
    /// The crew does not have that much money at all.
    NotEnoughCredits { wallet: i64, asked: i64 },
    UnknownJob,
    NotOffered,
    NotActive,
    /// `jobs_core::MAX_ACTIVE` jobs are active already.
    TooManyActive,
}

impl Refusal {
    /// The text key the glue shows.
    pub fn key(&self) -> &'static str {
        match self {
            Refusal::NotCaptain => "refusal.not_captain",
            Refusal::WrongTarget => "refusal.wrong_target",
            Refusal::NoSuchSlot => "refusal.no_such_slot",
            Refusal::NoCustomSlot => "refusal.no_custom_slot",
            Refusal::TextTooLong => "refusal.text_too_long",
            Refusal::BadAmount => "refusal.bad_amount",
            Refusal::NotEnoughAvailable { .. } => "refusal.not_enough_available",
            Refusal::OverAllowance { .. } => "refusal.over_allowance",
            Refusal::NotEnoughCredits { .. } => "refusal.not_enough_credits",
            Refusal::UnknownJob => "refusal.unknown_job",
            Refusal::NotOffered => "refusal.not_offered",
            Refusal::NotActive => "refusal.not_active",
            Refusal::TooManyActive => "refusal.too_many_active",
        }
    }
}

/// The captain check every captain-only action starts with.
pub(crate) fn captain_only(by: Role) -> Result<(), Refusal> {
    if by == Role::Captain { Ok(()) } else { Err(Refusal::NotCaptain) }
}
