//! Orders, satisfaction and relationship (#168), and the buyers of #229. Reads `TimePassed`,
//! `OrderSettled`, `ProductHandedOver` and `OrderCountered`; raises `OrderPlaced` (wholesale),
//! `ProductOrderPlaced`, `ProductOrderSettled`, `CounterAnswered`, `CustomerPoached`,
//! `CustomerReferred` and `TrackChanged` (the wallet). Everything random comes from the seeded
//! generator in the state, so the same seed and the same events give the same orders.
use std::collections::BTreeMap;

use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::product::ProductSpec;
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::rng::Rng;
use gameplay_core::content::WALLET;
use gameplay_core::{CommodityId, Content, EffectId, Event, LocationId, OrderId, TownId, TrackId, WorldEvent};
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

/// Buyers (#229). TODO(initiator): every number here is a starting value.
/// Seconds an open product order waits for its handover.
pub const PRODUCT_DEADLINE_S: f64 = 900.0;
/// Relationship lost when an order is withdrawn at its deadline, and when a counter is refused.
pub const WITHDRAW_DROP: f64 = 0.3;
pub const COUNTER_DROP: f64 = 0.3;
/// A handover with a match of at least this is pleasing: +1, and the neglect count clears.
pub const PLEASED_MATCH: f64 = 0.75;
/// Below this match a handover displeases: -0.5. Between the two the relationship holds.
pub const DISPLEASED_MATCH: f64 = 0.4;
pub const PLEASED_UP: f64 = 1.0;
pub const DISPLEASED_DOWN: f64 = 0.5;
/// Withdrawn orders in a row (neglect) that hand the buyer to the rival.
pub const POACH_NEGLECT: u32 = 3;
/// Seconds a poached buyer orders nothing before it comes back with neglect 0.
pub const POACHED_S: f64 = 1200.0;
/// The relationship at which a buyer refers the hidden buyer it names.
pub const REFER_AT: f64 = 4.0;
/// Who a poached buyer goes to. TODO(initiator): the rival clan's name.
pub const RIVAL: &str = "rival_clan";

/// The chance that a counter-offer is accepted: 0.9 for a small ask, less for a big one, more
/// for a regular. Clamped to 0.05..0.95. TODO(initiator).
pub fn counter_chance(ask_pct: u32, rel: f64) -> f64 {
    (0.9 - ask_pct as f64 * 0.02 + rel * 0.08).clamp(0.05, 0.95)
}

/// A relationship moved by `delta`. A fall stops at the neglect floor (or where it already is,
/// when that is lower); a rise stops at 5.
fn move_rel(rel: f64, delta: f64) -> f64 {
    if delta < 0.0 { (rel + delta).max(NEGLECT_FLOOR.min(rel)) } else { (rel + delta).min(5.0) }
}

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
    let standard = c.wholesale.as_ref().map_or(0.0, |w| w.standard);
    let share = (delivered as f64 / asked as f64).clamp(0.0, 1.0);
    let cond = if condition >= standard { 0.2 } else { -(standard - condition) * 1.5 };
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
    /// Buyers (#229): withdrawn orders in a row. Cleared by a pleasing handover and by the return.
    #[serde(default)]
    neglect: u32,
    /// Buyers: seconds left of being poached (0 when not poached).
    #[serde(default)]
    poached_s: f64,
    /// False while the buyer is hidden (until a referral).
    #[serde(default = "yes")]
    active: bool,
    /// Whether this customer has already referred the one it names.
    #[serde(default)]
    referred: bool,
}

fn yes() -> bool {
    true
}

/// A buyer's open order (#229).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductOrder {
    pub customer: CustomerId,
    pub town: TownId,
    /// The handover spot.
    pub at: LocationId,
    /// The effects asked for (picked from the buyer's pool).
    pub wants: Vec<EffectId>,
    pub min_quality: f64,
    /// Crates asked for.
    pub amount: u32,
    /// What it pays in total, after a counter-offer if one was accepted.
    pub price: i64,
    /// Seconds left until the order is withdrawn.
    pub left_s: f64,
    /// A buyer answers one counter-offer per order.
    pub countered: bool,
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
    /// Buyers' open orders (#229), by order id; also in `open`.
    #[serde(default)]
    product: BTreeMap<OrderId, ProductOrder>,
}

impl Customers {
    pub fn new(cc: &CustomerContent, seed: u64) -> Customers {
        let mut rng = Rng::new(seed);
        let customers = cc
            .customers
            .values()
            .map(|c| {
                let r = &c.record;
                // A hidden buyer draws its first wait only when it is referred.
                let wait_s = if r.hidden { 0.0 } else { draw_wait(&mut rng, r, r.start) };
                (r.id.clone(), CustomerState { relationship: r.start, wait_s, open_order: None, since_served_s: 0.0, neglect: 0, poached_s: 0.0, active: !r.hidden, referred: false })
            })
            .collect();
        Customers { rng, next_order: 1, customers, open: BTreeMap::new(), product: BTreeMap::new() }
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

    /// The buyers' open orders (#229): order id and what it asks for.
    pub fn product_orders(&self) -> impl Iterator<Item = (OrderId, &ProductOrder)> {
        self.product.iter().map(|(o, p)| (*o, p))
    }

    /// Whether a buyer is poached by the rival right now.
    pub fn is_poached(&self, id: &CustomerId) -> bool {
        self.customers.get(id).is_some_and(|c| c.poached_s > 0.0)
    }

    /// Whether a buyer is still hidden (not referred yet).
    pub fn is_hidden(&self, id: &CustomerId) -> bool {
        self.customers.get(id).is_some_and(|c| !c.active)
    }

    /// Withdrawn orders in a row (see `POACH_NEGLECT`).
    pub fn neglect(&self, id: &CustomerId) -> u32 {
        self.customers.get(id).map_or(0, |c| c.neglect)
    }

    /// Applies a world event. `TimePassed`, `OrderSettled` (wholesale), `ProductHandedOver` and
    /// `OrderCountered` (buyers) matter here.
    pub fn apply_world(&mut self, cc: &CustomerContent, k: &Content, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        match &ev.payload {
            WorldEvent::TimePassed { dt } => self.time(cc, k, *dt),
            WorldEvent::OrderSettled { order, delivered, asked, condition, in_time } => self.settle(cc, *order, *delivered, *asked, *condition, *in_time),
            WorldEvent::ProductHandedOver { order, products } => self.hand_over(cc, *order, products),
            WorldEvent::OrderCountered { order, ask_pct } => self.counter(cc, *order, *ask_pct),
            _ => Vec::new(),
        }
    }

    fn time(&mut self, cc: &CustomerContent, k: &Content, dt: f64) -> Vec<Outcome> {
        let mut out = Vec::new();
        self.deadlines(cc, dt, &mut out);
        let wholesale = k.locations.values().find(|l| l.record.tags.iter().any(|t| t.as_str() == WHOLESALE_TAG)).map(|l| l.record.id.clone());
        // Buyers whose wait ran out; their orders are placed after the loop, with the whole state.
        let mut due = Vec::new();
        for (id, st) in self.customers.iter_mut() {
            let Some(c) = cc.customers.get(id).map(|c| &c.record) else { continue };
            if !st.active {
                continue;
            }
            // Neglect: a long time without a pleasing delivery wears the relationship down to the floor.
            st.since_served_s += dt;
            while st.since_served_s >= DECAY_PERIOD_S {
                st.since_served_s -= DECAY_PERIOD_S;
                if st.relationship > NEGLECT_FLOOR {
                    st.relationship = (st.relationship - DECAY_STEP).max(NEGLECT_FLOOR);
                }
            }
            if st.poached_s > 0.0 {
                // Back from the rival with a clean slate.
                st.poached_s -= dt;
                if st.poached_s <= 0.0 {
                    st.poached_s = 0.0;
                    st.neglect = 0;
                    st.wait_s = draw_wait(&mut self.rng, c, st.relationship);
                }
                continue;
            }
            if st.open_order.is_some() {
                continue;
            }
            st.wait_s -= dt;
            if st.wait_s > 0.0 {
                continue;
            }
            if c.taste.is_some() {
                due.push(id.clone());
                continue;
            }
            let Some(w) = c.wholesale.as_ref() else { continue };
            let Some(from) = wholesale.clone() else { continue };
            // Goods of the customer's taste, one drawn by the seed.
            let goods: Vec<&CommodityId> = k.commodities.values().filter(|g| g.record.tags.iter().any(|t| w.goods.contains(t))).map(|g| &g.record.id).collect();
            if goods.is_empty() {
                continue;
            }
            let commodity = goods[self.rng.below(goods.len())].clone();
            let amount = w.amount[0] + self.rng.below((w.amount[1] - w.amount[0] + 1) as usize) as u32;
            let price = cc.prices.record.customer.get(&commodity).copied().unwrap_or(0);
            let reward = (amount as f64 * price as f64 * pay_factor(st.relationship)).round() as i64;
            let order = OrderId(self.next_order);
            self.next_order += 1;
            st.open_order = Some(order);
            self.open.insert(order, id.clone());
            st.wait_s = draw_wait(&mut self.rng, c, st.relationship);
            out.push(Outcome::Emit(WorldEvent::OrderPlaced { order, by: id.to_string(), from, to: w.location.clone(), commodity, amount, reward, deadline_s: w.patience_s }));
            out.push(Outcome::Notice(Notice::new(NoticeKind::Available, "notice.order.placed").arg("customer", Arg::Key(c.name.clone()))));
        }
        for id in due {
            if let Some(ev) = self.place_product(cc, k, &id) {
                out.push(ev);
            }
        }
        out
    }

    /// A buyer's order: `picks` effects from its pool, the crates, and the price. The
    /// relationship at the time of the order sets the price: crates × base × (1 + the effects'
    /// values) × pay factor.
    fn place_product(&mut self, cc: &CustomerContent, k: &Content, id: &CustomerId) -> Option<Outcome> {
        let c = cc.get(id)?;
        let t = c.taste.as_ref()?;
        let st = self.customers.get(id)?;
        let rel = st.relationship;
        let mut pool = t.wants.clone();
        let mut wants = Vec::new();
        for _ in 0..t.picks {
            let i = self.rng.below(pool.len());
            wants.push(pool.remove(i));
        }
        wants.sort();
        let amount = t.amount[0] + self.rng.below((t.amount[1] - t.amount[0] + 1) as usize) as u32;
        let sum: f64 = wants.iter().map(|w| k.effects.get(w).map_or(0.0, |e| e.record.value)).sum();
        let price = (amount as f64 * cc.prices.record.product_base * (1.0 + sum) * pay_factor(rel)).round() as i64;
        let (town, at) = (c.town.clone()?, c.at.clone()?);
        let wait_s = draw_wait(&mut self.rng, c, rel);
        let order = OrderId(self.next_order);
        self.next_order += 1;
        let st = self.customers.get_mut(id)?;
        st.open_order = Some(order);
        st.wait_s = wait_s;
        self.open.insert(order, id.clone());
        self.product.insert(order, ProductOrder { customer: id.clone(), town: town.clone(), at: at.clone(), wants: wants.clone(), min_quality: t.min_quality, amount, price, left_s: PRODUCT_DEADLINE_S, countered: false });
        Some(Outcome::Emit(WorldEvent::ProductOrderPlaced { order, by: id.to_string(), town, at, wants, min_quality: t.min_quality, amount, price, deadline_s: Some(PRODUCT_DEADLINE_S) }))
    }

    /// Buyers' orders run out: an order not handed over in time is withdrawn, the buyer is let
    /// down, and after `POACH_NEGLECT` in a row the rival takes the buyer for a while.
    fn deadlines(&mut self, cc: &CustomerContent, dt: f64, out: &mut Vec<Outcome>) {
        let mut expired = Vec::new();
        for (order, po) in self.product.iter_mut() {
            po.left_s -= dt;
            if po.left_s <= 0.0 {
                expired.push(*order);
            }
        }
        for order in expired {
            let Some(po) = self.product.remove(&order) else { continue };
            self.open.remove(&order);
            let Some(st) = self.customers.get_mut(&po.customer) else { continue };
            st.open_order = None;
            st.relationship = move_rel(st.relationship, -WITHDRAW_DROP);
            st.neglect += 1;
            out.push(Outcome::Emit(WorldEvent::ProductOrderSettled { order, paid: 0, matched: 0.0 }));
            if st.neglect >= POACH_NEGLECT {
                st.poached_s = POACHED_S;
                out.push(Outcome::Emit(WorldEvent::CustomerPoached { by: RIVAL.to_string() }));
            }
            if let Some(c) = cc.get(&po.customer) {
                st.wait_s = draw_wait(&mut self.rng, c, st.relationship);
            }
        }
    }

    /// `ProductHandedOver` (#229): what the buyer pays and how the relationship moves.
    /// `paid = price × min(1, delivered/asked) × (0.4 + 0.6 × matched)`; a product below its
    /// minimum quality counts half towards the match.
    fn hand_over(&mut self, cc: &CustomerContent, order: OrderId, products: &[ProductSpec]) -> Vec<Outcome> {
        let Some(po) = self.product.get(&order).cloned() else { return Vec::new() };
        let Some(c) = cc.get(&po.customer) else { return Vec::new() };
        self.product.remove(&order);
        self.open.remove(&order);
        let scores: Vec<f64> = products
            .iter()
            .map(|p| {
                let m = p.matches(&po.wants);
                if p.quality < po.min_quality { m * 0.5 } else { m }
            })
            .collect();
        let matched = if scores.is_empty() { 0.0 } else { scores.iter().sum::<f64>() / scores.len() as f64 };
        let share = (products.len() as f64 / po.amount as f64).min(1.0);
        let paid = (po.price as f64 * share * (0.4 + 0.6 * matched)).round() as i64;
        let Some(st) = self.customers.get_mut(&po.customer) else { return Vec::new() };
        st.open_order = None;
        let delta = if matched >= PLEASED_MATCH {
            PLEASED_UP
        } else if matched < DISPLEASED_MATCH {
            -DISPLEASED_DOWN
        } else {
            0.0
        };
        st.relationship = move_rel(st.relationship, delta);
        if matched >= PLEASED_MATCH {
            st.since_served_s = 0.0;
            st.neglect = 0;
        }
        st.wait_s = draw_wait(&mut self.rng, c, st.relationship);
        let mut out = vec![
            Outcome::Emit(WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta: paid, player: None }),
            Outcome::Emit(WorldEvent::ProductOrderSettled { order, paid, matched }),
        ];
        let name = Arg::Key(c.name.clone());
        if matched >= PLEASED_MATCH {
            out.push(Outcome::Notice(Notice::new(NoticeKind::Updated, c.voice.thanks.as_str()).arg("customer", name)));
        } else {
            // TODO(initiator): a middling match gets the grumble pool too.
            out.push(Outcome::Notice(Notice::new(NoticeKind::Warning, c.voice.grumble.as_str()).arg("customer", name)));
        }
        self.refer(cc, &po.customer, &mut out);
        out
    }

    /// `OrderCountered` (#229): once per order, a seeded chance to get `ask_pct` percent more.
    fn counter(&mut self, cc: &CustomerContent, order: OrderId, ask_pct: u32) -> Vec<Outcome> {
        let Some(po) = self.product.get_mut(&order) else { return Vec::new() };
        if po.countered {
            return Vec::new();
        }
        po.countered = true;
        let customer = po.customer.clone();
        let rel = self.customers.get(&customer).map_or(0.0, |s| s.relationship);
        let accepted = self.rng.unit() < counter_chance(ask_pct, rel);
        let Some(po) = self.product.get_mut(&order) else { return Vec::new() };
        if accepted {
            po.price = (po.price as f64 * (1.0 + ask_pct as f64 / 100.0)).round() as i64;
        }
        let price = po.price;
        let Some(c) = cc.get(&customer) else { return Vec::new() };
        let mut out = vec![Outcome::Emit(WorldEvent::CounterAnswered { order, accepted, price })];
        let name = Arg::Key(c.name.clone());
        if accepted {
            if let Some(key) = &c.voice.counter_yes {
                out.push(Outcome::Notice(Notice::new(NoticeKind::Updated, key.as_str()).arg("customer", name)));
            }
        } else {
            if let Some(st) = self.customers.get_mut(&customer) {
                st.relationship = move_rel(st.relationship, -COUNTER_DROP);
            }
            if let Some(key) = &c.voice.counter_no {
                out.push(Outcome::Notice(Notice::new(NoticeKind::Warning, key.as_str()).arg("customer", name)));
            }
        }
        out
    }

    /// A buyer at `REFER_AT` or more refers the hidden one it names, once. The hidden one starts
    /// ordering. (No event when it is already active.)
    fn refer(&mut self, cc: &CustomerContent, by: &CustomerId, out: &mut Vec<Outcome>) {
        let Some(st) = self.customers.get(by) else { return };
        if st.referred || st.relationship < REFER_AT {
            return;
        }
        let Some(new) = cc.get(by).and_then(|c| c.refers.clone()) else { return };
        if let Some(st) = self.customers.get_mut(by) {
            st.referred = true;
        }
        let new_id = CustomerId::new(new.clone());
        let (Some(n), Some(nc)) = (self.customers.get_mut(&new_id), cc.get(&new_id)) else { return };
        if n.active {
            return;
        }
        n.active = true;
        n.wait_s = draw_wait(&mut self.rng, nc, n.relationship);
        out.push(Outcome::Emit(WorldEvent::CustomerReferred { by: by.to_string(), new }));
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
        let mut out = Vec::new();
        if delta >= 0.2 {
            out.push(Outcome::Notice(Notice::new(NoticeKind::Updated, c.voice.thanks.as_str()).arg("customer", name)));
        } else if delta <= -0.2 {
            out.push(Outcome::Notice(Notice::new(NoticeKind::Warning, c.voice.grumble.as_str()).arg("customer", name)));
        }
        self.refer(cc, &id, &mut out);
        out
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

