//! Orders, satisfaction and relationship (#168). Reads `TimePassed` and `OrderSettled`, raises
//! `OrderPlaced`; everything random comes from the seeded generator in the state, so the same
//! seed and the same events give the same orders.
use std::collections::BTreeMap;

use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::rng::Rng;
use gameplay_core::{CommodityId, Content, Event, OrderId, WorldEvent};
use serde::{Deserialize, Serialize};

use crate::customer::{Customer, CustomerContent, CustomerId, WHOLESALE_TAG};

pub const SECTION: &str = "customers";
pub const SECTION_VERSION: u32 = 1;

/// A neglected customer's relationship sinks to this and no further: never lost for good.
/// TODO(initiator): starting values for the neglect rule.
pub const NEGLECT_FLOOR: f64 = 1.0;
/// Seconds without a delivery per step of neglect.
pub const DECAY_PERIOD_S: f64 = 600.0;
pub const DECAY_STEP: f64 = 0.1;

/// How the relationship (0 to 5) scales the time between orders: strangers wait twice as long, a
/// friend a quarter. TODO(initiator).
pub fn rhythm_factor(rel: f64) -> f64 {
    (2.0 - 0.35 * rel).clamp(0.25, 2.0)
}

/// A regular pays a little more: 4 % per point, 20 % at 5. TODO(initiator).
pub fn pay_factor(rel: f64) -> f64 {
    1.0 + 0.04 * rel
}

/// How an order's result sits with the customer: about -0.9 (nothing came) to +0.7 (all of it,
/// above their standard, in time). Share counts most, then the condition against the customer's
/// standard, then time. TODO(initiator): starting values.
pub fn satisfaction(c: &Customer, delivered: u32, asked: u32, condition: f64, in_time: bool) -> f64 {
    if delivered == 0 || asked == 0 {
        return -0.9;
    }
    let share = (delivered as f64 / asked as f64).clamp(0.0, 1.0);
    let cond = if condition >= c.standard { 0.2 } else { -(c.standard - condition) * 1.5 };
    0.6 * share - 0.2 + cond + if in_time { 0.1 } else { -0.2 }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CustomerState {
    /// 0..5.
    relationship: f64,
    /// Seconds until the next order; counts down only without an open order.
    wait_s: f64,
    open_order: Option<OrderId>,
    /// Seconds since a delivery pleased them (or since the start), for neglect.
    since_served_s: f64,
}

pub enum Outcome {
    /// A domain event for every system (`OrderPlaced`); the host gives it an id and applies it.
    Emit(WorldEvent),
    Notice(Notice),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Customers {
    rng: Rng,
    next_order: u64,
    customers: BTreeMap<CustomerId, CustomerState>,
    /// Orders placed and not settled: which customer.
    open: BTreeMap<OrderId, CustomerId>,
}

impl Customers {
    pub fn new(cc: &CustomerContent, seed: u64) -> Customers {
        let mut rng = Rng::new(seed);
        let customers = cc
            .customers
            .values()
            .map(|c| {
                let r = &c.record;
                let wait_s = draw_wait(&mut rng, r, r.start);
                (r.id.clone(), CustomerState { relationship: r.start, wait_s, open_order: None, since_served_s: 0.0 })
            })
            .collect();
        Customers { rng, next_order: 1, customers, open: BTreeMap::new() }
    }

    pub fn relationship(&self, id: &CustomerId) -> f64 {
        self.customers.get(id).map_or(0.0, |c| c.relationship)
    }

    /// Seconds until the customer's next order (none while one is open).
    pub fn next_order_in(&self, id: &CustomerId) -> Option<f64> {
        self.customers.get(id).filter(|c| c.open_order.is_none()).map(|c| c.wait_s.max(0.0))
    }

    /// Test and scenario hook: the next order comes in `secs`.
    pub fn set_wait(&mut self, id: &CustomerId, secs: f64) {
        if let Some(c) = self.customers.get_mut(id) {
            c.wait_s = secs;
        }
    }

    pub fn open_orders(&self) -> usize {
        self.open.len()
    }

    /// Applies a world event. Only `TimePassed` and `OrderSettled` matter here.
    pub fn apply_world(&mut self, cc: &CustomerContent, k: &Content, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        match &ev.payload {
            WorldEvent::TimePassed { dt } => self.time(cc, k, *dt),
            WorldEvent::OrderSettled { order, delivered, asked, condition, in_time } => self.settle(cc, *order, *delivered, *asked, *condition, *in_time),
            _ => Vec::new(),
        }
    }

    fn time(&mut self, cc: &CustomerContent, k: &Content, dt: f64) -> Vec<Outcome> {
        let mut out = Vec::new();
        let wholesale = k.locations.values().find(|l| l.record.tags.iter().any(|t| t.as_str() == WHOLESALE_TAG)).map(|l| l.record.id.clone());
        for (id, st) in self.customers.iter_mut() {
            let Some(c) = cc.customers.get(id).map(|c| &c.record) else { continue };
            // Neglect: a long time without a pleasing delivery wears the relationship down to the floor.
            st.since_served_s += dt;
            while st.since_served_s >= DECAY_PERIOD_S {
                st.since_served_s -= DECAY_PERIOD_S;
                if st.relationship > NEGLECT_FLOOR {
                    st.relationship = (st.relationship - DECAY_STEP).max(NEGLECT_FLOOR);
                }
            }
            if st.open_order.is_some() {
                continue;
            }
            st.wait_s -= dt;
            if st.wait_s > 0.0 {
                continue;
            }
            let Some(from) = wholesale.clone() else { continue };
            // Goods of the customer's taste, one drawn by the seed.
            let goods: Vec<&CommodityId> = k.commodities.values().filter(|g| g.record.tags.iter().any(|t| c.taste.contains(t))).map(|g| &g.record.id).collect();
            if goods.is_empty() {
                continue;
            }
            let commodity = goods[self.rng.below(goods.len())].clone();
            let amount = c.amount[0] + self.rng.below((c.amount[1] - c.amount[0] + 1) as usize) as u32;
            let price = cc.prices.record.customer.get(&commodity).copied().unwrap_or(0);
            let reward = (amount as f64 * price as f64 * pay_factor(st.relationship)).round() as i64;
            let order = OrderId(self.next_order);
            self.next_order += 1;
            st.open_order = Some(order);
            self.open.insert(order, id.clone());
            st.wait_s = draw_wait(&mut self.rng, c, st.relationship);
            out.push(Outcome::Emit(WorldEvent::OrderPlaced { order, by: id.to_string(), from, to: c.location.clone(), commodity, amount, reward, deadline_s: c.patience_s }));
            out.push(Outcome::Notice(Notice::new(NoticeKind::Available, "notice.order.placed").arg("customer", Arg::Key(c.name.clone()))));
        }
        out
    }

    fn settle(&mut self, cc: &CustomerContent, order: OrderId, delivered: u32, asked: u32, condition: f64, in_time: bool) -> Vec<Outcome> {
        // An unknown order, or one seen before, changes nothing.
        let Some(id) = self.open.remove(&order) else { return Vec::new() };
        let (Some(st), Some(c)) = (self.customers.get_mut(&id), cc.customers.get(&id).map(|c| &c.record)) else { return Vec::new() };
        st.open_order = None;
        let delta = satisfaction(c, delivered, asked, condition, in_time);
        st.relationship = (st.relationship + delta).clamp(0.0, 5.0);
        if delta > 0.0 {
            st.since_served_s = 0.0;
        }
        st.wait_s = draw_wait(&mut self.rng, c, st.relationship);
        // The customer answers in their own voice when it mattered.
        let name = Arg::Key(c.name.clone());
        if delta >= 0.2 {
            vec![Outcome::Notice(Notice::new(NoticeKind::Updated, c.voice.thanks.as_str()).arg("customer", name))]
        } else if delta <= -0.2 {
            vec![Outcome::Notice(Notice::new(NoticeKind::Warning, c.voice.grumble.as_str()).arg("customer", name))]
        } else {
            Vec::new()
        }
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no customers section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Customers>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}

/// The wait to the next order: a draw from the customer's rhythm, scaled by the relationship.
fn draw_wait(rng: &mut Rng, c: &Customer, rel: f64) -> f64 {
    let [lo, hi] = c.rhythm_s;
    (lo + (hi - lo) * rng.unit()) * rhythm_factor(rel)
}

