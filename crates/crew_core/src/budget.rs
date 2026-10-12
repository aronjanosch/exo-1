//! The crew budget. Taken from PULSAR: the captain holds the crew's money and releases it in
//! steps of 100, the console shows the total next to what is still available, and crew purchase
//! limits can be switched off; the allowance per role is ours (the design asks for it per role).
//!
//! The money itself is the crew's wallet track in `gameplay_core::Progress`; this keeps only the
//! allowances and takes the wallet as an argument, so the two cannot drift. A spend comes back
//! as the `TrackChanged` event the host applies; a sale is a positive `TrackChanged` the host
//! raises itself and simply grows the pool.
use std::collections::BTreeMap;

use gameplay_core::{TrackId, WorldEvent};
use serde::{Deserialize, Serialize};

use crate::{Refusal, Role, captain_only};

/// Allowances move in steps of this many credits.
pub const STEP: i64 = 100;

/// The two numbers of the console: `total` (white) is all the crew's money, `available` (blue)
/// what the captain has not released to a role.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pool {
    pub total: i64,
    pub available: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    /// Released and not yet spent, per role.
    allowance: BTreeMap<Role, i64>,
    /// On: crew members spend only their allowance. Off: anyone spends from the pool.
    limits: bool,
}

impl Default for Budget {
    fn default() -> Budget {
        Budget { allowance: BTreeMap::new(), limits: true }
    }
}

impl Budget {
    /// Released money a role has left.
    pub fn remaining(&self, role: Role) -> i64 {
        self.allowance.get(&role).copied().unwrap_or(0)
    }

    pub fn limits(&self) -> bool {
        self.limits
    }

    pub fn pool(&self, wallet: i64) -> Pool {
        let released: i64 = self.allowance.values().sum();
        Pool { total: wallet, available: (wallet - released).max(0) }
    }

    /// The captain releases one step to a role; returns the role's new allowance.
    pub fn release(&mut self, by: Role, role: Role, wallet: i64) -> Result<i64, Refusal> {
        captain_only(by)?;
        if role == Role::Captain {
            return Err(Refusal::NoSuchSlot);
        }
        let available = self.pool(wallet).available;
        if available < STEP {
            return Err(Refusal::NotEnoughAvailable { available });
        }
        let a = self.allowance.entry(role).or_insert(0);
        *a += STEP;
        Ok(*a)
    }

    /// The captain takes one step back from a role (what is left, if less).
    pub fn withdraw(&mut self, by: Role, role: Role) -> Result<i64, Refusal> {
        captain_only(by)?;
        let a = self.allowance.entry(role).or_insert(0);
        *a = (*a - STEP).max(0);
        Ok(*a)
    }

    /// The captain switches the purchase limits on or off.
    pub fn set_limits(&mut self, by: Role, on: bool) -> Result<(), Refusal> {
        captain_only(by)?;
        self.limits = on;
        Ok(())
    }

    /// Someone of `role` pays `amount` (a shop, a refill). The role's allowance pays first; the
    /// captain, and anyone when the limits are off, pays the rest from the available pool.
    /// Returns the wallet change for the host.
    pub fn try_spend(&mut self, role: Role, amount: i64, wallet: i64) -> Result<WorldEvent, Refusal> {
        if amount <= 0 {
            return Err(Refusal::BadAmount);
        }
        if wallet < amount {
            return Err(Refusal::NotEnoughCredits { wallet, asked: amount });
        }
        let remaining = self.remaining(role);
        let from_allowance = remaining.min(amount);
        let rest = amount - from_allowance;
        if rest > 0 {
            if self.limits && role != Role::Captain {
                return Err(Refusal::OverAllowance { remaining, asked: amount });
            }
            let available = self.pool(wallet).available;
            if rest > available {
                return Err(Refusal::NotEnoughAvailable { available });
            }
        }
        if from_allowance > 0 {
            self.allowance.insert(role, remaining - from_allowance);
        }
        Ok(WorldEvent::TrackChanged { track: TrackId::new(gameplay_core::content::WALLET), delta: -amount, player: None })
    }
}
