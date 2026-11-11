//! The dispatch chute's orders (#334): what the crew is asked to deliver. An order is one to three
//! lines of (item, count). The book knows counts by item id and nothing about containers: the glue
//! reads what the chute holds, asks `send`, and on `Ok` removes exactly the counts it reports.
//!
//! Orders come from tiers in the config (`done / orders_per_tier` picks the tier, the last one goes on),
//! drawn with the seeded generator, so the same seed gives the same sequence. No order repeats the one
//! before it exactly. No Bevy types.
use crate::fabricator::Amount;
use content_core::{File, parse_strict};
use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

pub const PATH: &str = "orders/orders.json";
/// The most lines an order has (the slate's room).
pub const MAX_LINES: usize = 3;
/// Tries to draw an order that differs from the one before; a tier with one possible order gives up and repeats.
const REDRAWS: usize = 32;

/// One possible line of a tier: the item and the range of its count.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LineSpec {
    pub item: String,
    pub min: u32,
    pub max: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tier {
    /// How many lines an order of this tier has.
    pub lines: usize,
    pub pool: Vec<LineSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderConfig {
    pub orders_per_tier: u32,
    pub max_per_line: u32,
    pub tiers: Vec<Tier>,
}

impl OrderConfig {
    pub fn load(files: &[File]) -> Result<OrderConfig, Vec<String>> {
        let Some(f) = files.iter().find(|f| f.path == PATH) else { return Err(vec![format!("{PATH}: missing")]) };
        OrderConfig::parse(&f.text)
    }

    pub fn parse(text: &str) -> Result<OrderConfig, Vec<String>> {
        let c = parse_strict::<OrderConfig>(PATH, text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        if c.orders_per_tier == 0 {
            e.push(format!("{PATH}: orders_per_tier must be > 0"));
        }
        if c.max_per_line == 0 {
            e.push(format!("{PATH}: max_per_line must be > 0"));
        }
        if c.tiers.is_empty() {
            e.push(format!("{PATH}: tiers: needs at least one"));
        }
        for (i, t) in c.tiers.iter().enumerate() {
            let mut need = |ok: bool, rule: &str| {
                if !ok {
                    e.push(format!("{PATH}: tiers[{i}]: {rule}"));
                }
            };
            need((1..=MAX_LINES).contains(&t.lines), "lines must be 1 to 3");
            need(t.pool.iter().all(|l| !l.item.is_empty() && l.min >= 1 && l.min <= l.max && l.max <= c.max_per_line), "every pool line needs an item and 1 <= min <= max <= max_per_line");
            let mut ids: Vec<&str> = t.pool.iter().map(|l| l.item.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            need(ids.len() == t.pool.len(), "an item may appear once in a pool");
            need(t.pool.len() >= t.lines, "the pool needs at least as many items as the order has lines");
        }
        if e.is_empty() { Ok(c) } else { Err(e) }
    }

    /// The tier the `done`th order is drawn from.
    pub fn tier_of(&self, done: u32) -> &Tier {
        let i = ((done / self.orders_per_tier.max(1)) as usize).min(self.tiers.len() - 1);
        &self.tiers[i]
    }
}

/// What the crew is asked for: one to three lines.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Order {
    pub lines: Vec<Amount>,
}

impl Order {
    /// The same order whatever the line order.
    fn same_as(&self, other: &Order) -> bool {
        let key = |o: &Order| {
            let mut v: Vec<(String, u32)> = o.lines.iter().map(|a| (a.item.clone(), a.count)).collect();
            v.sort();
            v
        };
        key(self) == key(other)
    }

    /// Whether the order is something the book could have drawn from `cfg` (a bad save is not).
    fn fits(&self, cfg: &OrderConfig) -> bool {
        (1..=MAX_LINES).contains(&self.lines.len()) && self.lines.iter().all(|a| a.count >= 1 && a.count <= cfg.max_per_line && !a.item.is_empty())
    }
}

/// What a complete send takes out of the chute: exactly the order's counts, and the order's number (1-based).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    pub remove: Vec<Amount>,
    pub number: u32,
}

/// An incomplete load: the indexes of the lines that are not met. Nothing changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missing(pub Vec<usize>);

/// The part of the book a save keeps.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Saved {
    pub done: u32,
    pub current: Order,
    pub rng: Rng,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderBook {
    cfg: OrderConfig,
    done: u32,
    current: Order,
    rng: Rng,
}

impl OrderBook {
    pub fn new(cfg: &OrderConfig, seed: u64) -> OrderBook {
        let mut rng = Rng::new(seed);
        let current = draw(cfg, 0, None, &mut rng);
        OrderBook { cfg: cfg.clone(), done: 0, current, rng }
    }

    /// Orders sent so far.
    pub fn done(&self) -> u32 {
        self.done
    }

    pub fn current(&self) -> &Order {
        &self.current
    }

    pub fn saved(&self) -> Saved {
        Saved { done: self.done, current: self.current.clone(), rng: self.rng.clone() }
    }

    /// The book as a save left it. An order that does not fit the config (a hand-edited save) is drawn anew.
    pub fn restore(cfg: &OrderConfig, saved: Saved, seed: u64) -> OrderBook {
        let mut book = OrderBook { cfg: cfg.clone(), done: saved.done, current: saved.current, rng: saved.rng };
        if !book.current.fits(cfg) {
            book.rng = Rng::new(seed ^ u64::from(book.done));
            book.current = draw(cfg, book.done, None, &mut book.rng);
        }
        book
    }

    fn have(contents: &[Amount], item: &str) -> u64 {
        contents.iter().filter(|a| a.item == item).map(|a| u64::from(a.count)).sum()
    }

    /// Per line of the current order: does the load hold enough.
    pub fn check(&self, contents: &[Amount]) -> Vec<bool> {
        self.current.lines.iter().map(|l| Self::have(contents, &l.item) >= u64::from(l.count)).collect()
    }

    /// Sends the load. Complete: reports the order's counts to remove (extras stay), `done` goes up and the next
    /// order is drawn. Incomplete: nothing changes.
    pub fn send(&mut self, contents: &[Amount]) -> Result<Taken, Missing> {
        let met = self.check(contents);
        if met.iter().any(|m| !m) {
            return Err(Missing(met.iter().enumerate().filter(|(_, m)| !**m).map(|(i, _)| i).collect()));
        }
        let remove = self.current.lines.clone();
        self.done = self.done.saturating_add(1);
        let taken = Taken { remove, number: self.done };
        self.current = draw(&self.cfg, self.done, Some(&self.current), &mut self.rng);
        Ok(taken)
    }
}

fn draw(cfg: &OrderConfig, done: u32, previous: Option<&Order>, rng: &mut Rng) -> Order {
    let tier = cfg.tier_of(done);
    let mut last = Order { lines: Vec::new() };
    for _ in 0..REDRAWS {
        let mut pool: Vec<&LineSpec> = tier.pool.iter().collect();
        let n = tier.lines.min(pool.len()).min(MAX_LINES);
        let mut lines = Vec::with_capacity(n);
        for _ in 0..n {
            let spec = pool.swap_remove(rng.below(pool.len()));
            let (min, max) = (spec.min.min(cfg.max_per_line), spec.max.min(cfg.max_per_line));
            let count = min + rng.below((max.max(min) - min) as usize + 1) as u32;
            lines.push(Amount::new(&spec.item, count));
        }
        last = Order { lines };
        if previous.is_none_or(|p| !p.same_as(&last)) {
            return last;
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OrderConfig {
        OrderConfig::parse(include_str!("../../../content/gameplay/orders/orders.json")).unwrap()
    }

    fn load(lines: &[(&str, u32)]) -> Vec<Amount> {
        lines.iter().map(|(i, n)| Amount::new(i, *n)).collect()
    }

    fn sequence(seed: u64, n: usize) -> Vec<Order> {
        let c = cfg();
        let mut b = OrderBook::new(&c, seed);
        (0..n)
            .map(|_| {
                let o = b.current().clone();
                b.send(&o.lines).unwrap();
                o
            })
            .collect()
    }

    #[test]
    fn the_same_seed_gives_the_same_sequence_and_another_seed_another() {
        assert_eq!(sequence(7, 20), sequence(7, 20));
        assert_ne!(sequence(7, 20), sequence(8, 20));
    }

    #[test]
    fn no_order_repeats_the_one_before() {
        for seed in 0..50 {
            let s = sequence(seed, 30);
            assert!(s.windows(2).all(|w| !w[0].same_as(&w[1])), "seed {seed}");
        }
    }

    #[test]
    fn an_order_stays_inside_its_tier_and_the_limits() {
        let c = cfg();
        for seed in 0..50 {
            let mut b = OrderBook::new(&c, seed);
            for done in 0..30u32 {
                assert_eq!(b.done(), done);
                let tier = c.tier_of(done);
                let o = b.current().clone();
                assert_eq!(o.lines.len(), tier.lines.min(MAX_LINES), "seed {seed} done {done}");
                let mut ids: Vec<&str> = o.lines.iter().map(|l| l.item.as_str()).collect();
                ids.sort_unstable();
                ids.dedup();
                assert_eq!(ids.len(), o.lines.len(), "distinct items");
                for l in &o.lines {
                    let spec = tier.pool.iter().find(|s| s.item == l.item).unwrap_or_else(|| panic!("{} is not in tier of done {done}", l.item));
                    assert!((spec.min..=spec.max).contains(&l.count) && l.count <= c.max_per_line && l.count >= 1);
                }
                b.send(&o.lines).unwrap();
            }
        }
    }

    #[test]
    fn the_tier_follows_done_and_the_last_one_goes_on() {
        let c = cfg();
        assert_eq!(c.tier_of(0), &c.tiers[0]);
        assert_eq!(c.tier_of(c.orders_per_tier - 1), &c.tiers[0]);
        assert_eq!(c.tier_of(c.orders_per_tier), &c.tiers[1]);
        assert_eq!(c.tier_of(u32::MAX), c.tiers.last().unwrap());
    }

    #[test]
    fn a_complete_load_gives_exactly_the_orders_counts_and_the_extras_stay_out_of_it() {
        let c = cfg();
        let mut b = OrderBook::new(&c, 3);
        let first = b.current().clone();
        let mut contents: Vec<Amount> = first.lines.iter().map(|l| Amount::new(&l.item, l.count + 2)).collect();
        contents.push(Amount::new("wrench", 1));
        let t = b.send(&contents).expect("complete");
        assert_eq!(t.remove, first.lines, "the order's counts, not the extras");
        assert_eq!((t.number, b.done()), (1, 1));
        assert!(!b.current().same_as(&first));
    }

    #[test]
    fn stacks_in_several_places_add_up() {
        let mut b = OrderBook::new(&cfg(), 3);
        let l = b.current().lines[0].clone();
        let split: Vec<Amount> = (0..l.count).map(|_| Amount::new(&l.item, 1)).collect();
        assert_eq!(b.check(&split), vec![true]);
        assert!(b.send(&split).is_ok());
    }

    #[test]
    fn an_incomplete_load_changes_nothing_and_names_the_missing_lines() {
        let c = cfg();
        let mut b = OrderBook::new(&c, 3);
        for _ in 0..4 {
            let o = b.current().clone();
            b.send(&o.lines).unwrap();
        }
        let before = b.clone();
        assert_eq!(b.send(&[]), Err(Missing((0..b.current().lines.len()).collect())));
        assert_eq!(b, before);
        // One of the first line short, the others met.
        let o = b.current().clone();
        let mut contents: Vec<Amount> = o.lines.iter().map(|l| Amount::new(&l.item, l.count)).collect();
        contents[0].count -= 1;
        assert_eq!(b.check(&contents)[0], false);
        assert_eq!(b.send(&contents), Err(Missing(vec![0])));
        assert_eq!(b, before, "nothing changed");
        assert_eq!(b.check(&contents).iter().skip(1).filter(|m| !**m).count(), 0);
    }

    #[test]
    fn done_never_decreases() {
        let c = cfg();
        let mut b = OrderBook::new(&c, 11);
        let mut last = 0;
        for i in 0..40 {
            let o = b.current().clone();
            let contents = if i % 3 == 0 { Vec::new() } else { o.lines.clone() };
            let _ = b.send(&contents);
            assert!(b.done() >= last);
            last = b.done();
        }
        let mut full = OrderBook::restore(&c, Saved { done: u32::MAX, current: b.current().clone(), rng: Rng::new(1) }, 1);
        let o = full.current().clone();
        full.send(&o.lines).unwrap();
        assert_eq!(full.done(), u32::MAX, "saturates");
    }

    #[test]
    fn save_and_load_keep_done_the_order_and_the_rng() {
        let c = cfg();
        let mut b = OrderBook::new(&c, 5);
        for _ in 0..3 {
            let o = b.current().clone();
            b.send(&o.lines).unwrap();
        }
        let text = serde_json::to_string(&b.saved()).unwrap();
        let mut back = OrderBook::restore(&c, serde_json::from_str(&text).unwrap(), 5);
        assert_eq!(back, b);
        for _ in 0..10 {
            let o = b.current().clone();
            assert_eq!(back.current(), &o);
            assert_eq!(b.send(&o.lines), back.send(&o.lines));
        }
        let bad = Saved { done: 4, current: Order { lines: Vec::new() }, rng: Rng::new(0) };
        let fixed = OrderBook::restore(&c, bad, 5);
        assert_eq!(fixed.done(), 4);
        assert!(fixed.current().fits(&c), "a bad saved order is drawn anew");
    }

    #[test]
    fn the_config_is_strict() {
        let good = include_str!("../../../content/gameplay/orders/orders.json");
        assert!(OrderConfig::parse(good).is_ok());
        assert!(OrderConfig::parse(&good.replace("\"orders_per_tier\": 2", "\"orders_per_tier\": 0")).is_err());
        assert!(OrderConfig::parse(&good.replace("\"lines\": 1", "\"lines\": 4")).is_err());
        assert!(OrderConfig::parse(&good.replace("\"max\": 4", "\"max\": 9")).is_err(), "over max_per_line");
        assert!(OrderConfig::parse(&good.replace("\"min\": 3", "\"mni\": 3")).is_err(), "unknown field");
        let short = r#"{ "orders_per_tier": 1, "max_per_line": 6, "tiers": [{ "lines": 2, "pool": [{ "item": "fuse", "min": 1, "max": 2 }] }] }"#;
        assert!(OrderConfig::parse(short).is_err(), "more lines than the pool has items");
    }
}
