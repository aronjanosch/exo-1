//! The `customer` and `price_table` records (#168), one JSON file each in `customer/` and
//! `price_table/`.
use std::collections::BTreeMap;
use std::fmt;

use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::text::TextTable;
use gameplay_core::{CommodityId, Content, LocationId, Tag, TextKey};
use serde::{Deserialize, Serialize};

pub const FOLDER: &str = "customer";
pub const PRICE_FOLDER: &str = "price_table";
/// The tag of the location where customers' goods are picked up.
pub const WHOLESALE_TAG: &str = "wholesale";
/// Pools of a customer's voice need this many lines.
pub const MIN_POOL: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomerId(pub String);

impl CustomerId {
    pub fn new(s: impl Into<String>) -> CustomerId {
        CustomerId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CustomerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The text keys of a customer's voice (pools).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    /// What the customer says about the order (the reason in the briefing).
    pub order: TextKey,
    /// After a delivery that pleased.
    pub thanks: TextKey,
    /// After one that did not.
    pub grumble: TextKey,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Customer {
    pub id: CustomerId,
    pub name: TextKey,
    /// Where the goods are brought (a location).
    pub location: LocationId,
    /// Commodity tags the customer likes; orders are for goods with one of them.
    pub taste: Vec<Tag>,
    /// The lowest condition (0..1) the customer is content with.
    pub standard: f64,
    /// Seconds between orders at the middle relationship, min and max.
    pub rhythm_s: [f64; 2],
    /// Crates per order, min and max.
    pub amount: [u32; 2],
    /// Seconds from the first pickup that an order may take; none means no limit.
    #[serde(default)]
    pub patience_s: Option<f64>,
    /// The relationship (0 to 5) a new crew starts with.
    pub start: f64,
    pub voice: Voice,
}

impl Record for Customer {
    type Id = CustomerId;
    fn id(&self) -> &CustomerId {
        &self.id
    }
}

/// One table for the legal prices, so the later chains (G, H) balance against it: what the
/// wholesaler asks per crate, what customers pay per crate (before the relationship's bonus), and
/// what the courier jobs pay (checked against the templates by the glue). TODO(initiator): all
/// numbers are starting values.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceTable {
    pub id: String,
    pub wholesale: BTreeMap<CommodityId, i64>,
    pub customer: BTreeMap<CommodityId, i64>,
    #[serde(default)]
    pub courier: BTreeMap<String, i64>,
}

impl Record for PriceTable {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CustomerContent {
    pub customers: BTreeMap<CustomerId, Loaded<Customer>>,
    pub prices: Loaded<PriceTable>,
}

impl CustomerContent {
    /// Loads `customer/` and `price_table/` from `files` and checks fields and references.
    /// Returns every error, each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<CustomerContent, Vec<String>> {
        let mut e = Vec::new();
        let customers = load_records::<Customer>(files, FOLDER, &mut e);
        let mut tables: Vec<Loaded<PriceTable>> = load_records::<PriceTable>(files, PRICE_FOLDER, &mut e).into_values().collect();
        if tables.len() != 1 && e.is_empty() {
            e.push(format!("{PRICE_FOLDER}: exactly one price table is needed, found {}", tables.len()));
        }
        let Some(prices) = tables.pop() else { return Err(e) };
        let c = CustomerContent { customers, prices };
        for Loaded { path, record: r } in c.customers.values() {
            c.check_customer(path, r, kernel, &mut e);
        }
        c.check_prices(kernel, &mut e);
        if e.is_empty() { Ok(c) } else { Err(e) }
    }

    fn check_customer(&self, path: &str, r: &Customer, k: &Content, e: &mut Vec<String>) {
        check_id(path, "id", r.id.as_str(), e);
        k.check_location(path, "location", &r.location, e);
        if r.taste.is_empty() {
            e.push(format!("{path}: taste: empty"));
        }
        for t in &r.taste {
            if !k.commodities.values().any(|c| c.record.tags.contains(t)) {
                e.push(format!("{path}: taste: no commodity has the tag '{t}'"));
            }
        }
        if !(0.0..=1.0).contains(&r.standard) {
            e.push(format!("{path}: standard: must be in 0..1"));
        }
        if !(r.rhythm_s[0] > 0.0 && r.rhythm_s[0] <= r.rhythm_s[1]) {
            e.push(format!("{path}: rhythm_s: needs 0 < min <= max"));
        }
        if r.amount[0] == 0 || r.amount[0] > r.amount[1] {
            e.push(format!("{path}: amount: needs 1 <= min <= max"));
        }
        if r.patience_s.is_some_and(|p| !(p > 0.0)) {
            e.push(format!("{path}: patience_s: must be positive"));
        }
        if !(0.0..=5.0).contains(&r.start) {
            e.push(format!("{path}: start: must be in 0..5"));
        }
    }

    fn check_prices(&self, k: &Content, e: &mut Vec<String>) {
        let Loaded { path, record: p } = &self.prices;
        check_id(path, "id", &p.id, e);
        for (field, map) in [("wholesale", &p.wholesale), ("customer", &p.customer)] {
            for (c, price) in map {
                k.check_commodity(path, field, c, e);
                if *price <= 0 {
                    e.push(format!("{path}: {field}: '{c}' must cost something"));
                }
            }
        }
        // Everything a customer may order has both prices, and the customer pays more than the
        // wholesaler asks (a legal margin; illegal goods must not always out-pay it later).
        for c in k.commodities.values().filter(|c| self.customers.values().any(|x| x.record.taste.iter().any(|t| c.record.tags.contains(t)))) {
            let (w, s) = (p.wholesale.get(&c.record.id), p.customer.get(&c.record.id));
            if w.is_none() {
                e.push(format!("{path}: wholesale: no price for '{}', which customers order", c.record.id));
            }
            if s.is_none() {
                e.push(format!("{path}: customer: no price for '{}', which customers order", c.record.id));
            }
            if let (Some(w), Some(s)) = (w, s)
                && s <= w
            {
                e.push(format!("{path}: customer: '{}' pays {s}, not more than the wholesale price {w}", c.record.id));
            }
        }
        if !k.locations.values().any(|l| l.record.tags.iter().any(|t| t.as_str() == WHOLESALE_TAG)) {
            e.push(format!("{path}: no location has the tag '{WHOLESALE_TAG}': orders need a wholesaler"));
        }
    }

    /// The text keys and pools the customers need: name, order title, the three voice pools.
    pub fn check_texts(&self, table: &TextTable) -> Vec<String> {
        let mut e = Vec::new();
        for Loaded { path, record: c } in self.customers.values() {
            let title = format!("customer.{}.order_title", c.id);
            for (field, key) in [("name", c.name.as_str()), ("order_title", title.as_str())] {
                if !table.has(key) {
                    e.push(format!("{path}: {field}: no text '{key}'"));
                }
            }
            for (field, k) in [("voice.order", &c.voice.order), ("voice.thanks", &c.voice.thanks), ("voice.grumble", &c.voice.grumble)] {
                match table.lines(k.as_str()) {
                    None => e.push(format!("{path}: {field}: no text '{k}'")),
                    Some(l) if l.len() < MIN_POOL => e.push(format!("{path}: {field}: '{k}' needs at least {MIN_POOL} lines (it has {})", l.len())),
                    Some(_) => {}
                }
            }
        }
        e
    }
}
