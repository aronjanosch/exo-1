//! The `customer` and `price_table` records (#168), one JSON file each in `customer/` and
//! `price_table/`.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::text::TextTable;
use gameplay_core::{CommodityId, Content, EffectId, LocationId, Tag, TextKey, TownId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// The text keys of a customer's voice (pools). The counter pools are needed by buyers (#229).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    /// What the customer says about the order (the reason in the briefing).
    pub order: TextKey,
    /// After a delivery that pleased.
    pub thanks: TextKey,
    /// After one that did not.
    pub grumble: TextKey,
    /// When a counter-offer is accepted (buyers only).
    #[serde(default)]
    pub counter_yes: Option<TextKey>,
    /// When a counter-offer is refused (buyers only).
    #[serde(default)]
    pub counter_no: Option<TextKey>,
}

/// A wholesale customer (#168): orders goods from the wholesaler for a location.
#[derive(Clone, Debug, PartialEq)]
pub struct Wholesale {
    /// Where the goods are brought (a location).
    pub location: LocationId,
    /// Commodity tags the customer likes; orders are for goods with one of them.
    pub goods: Vec<Tag>,
    /// The lowest condition (0..1) the customer is content with.
    pub standard: f64,
    /// Crates per order, min and max.
    pub amount: [u32; 2],
    /// Seconds from the first pickup that an order may take; none means no limit.
    pub patience_s: Option<f64>,
}

/// What a buyer wants (#229). `picks` effects are drawn from `wants` for each order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Taste {
    /// The effects the buyer may ask for (at least one, each a kernel effect).
    pub wants: Vec<EffectId>,
    /// Effects per order, 1 or 2.
    pub picks: u8,
    /// 0..1: a product below this counts half towards the match.
    pub min_quality: f64,
    /// Crates per order, min and max.
    pub amount: [u32; 2],
}

/// A customer (#168 wholesale, #229 buyers). Exactly one of `wholesale` and `taste` is set: an
/// old record with a tag list in `taste` is a wholesale customer, an object is a buyer.
#[derive(Clone, Debug, PartialEq)]
pub struct Customer {
    pub id: CustomerId,
    pub name: TextKey,
    pub wholesale: Option<Wholesale>,
    pub taste: Option<Taste>,
    /// Seconds between orders at the middle relationship, min and max.
    pub rhythm_s: [f64; 2],
    /// The relationship (0 to 5) a new crew starts with.
    pub start: f64,
    pub voice: Voice,
    /// Buyers: the town (the place id) they live in.
    pub town: Option<TownId>,
    /// Buyers: the handover spot (a location). TODO(initiator): the glue will move them.
    pub at: Option<LocationId>,
    /// Orders only after another customer refers it (#229).
    pub hidden: bool,
    /// Another customer (id), revealed when this one reaches relationship 4 (#229).
    pub refers: Option<String>,
}

/// The file as written. `taste` is a tag list (wholesale) or a product taste (buyer); the
/// conversion below sorts out which, and what each one may carry.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCustomer {
    id: CustomerId,
    name: TextKey,
    #[serde(default)]
    location: Option<LocationId>,
    #[serde(default)]
    taste: Option<Value>,
    #[serde(default)]
    standard: Option<f64>,
    rhythm_s: [f64; 2],
    #[serde(default)]
    amount: Option<[u32; 2]>,
    #[serde(default)]
    patience_s: Option<f64>,
    start: f64,
    voice: Voice,
    #[serde(default)]
    town: Option<TownId>,
    #[serde(default)]
    at: Option<LocationId>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    refers: Option<String>,
}

impl TryFrom<RawCustomer> for Customer {
    type Error = String;
    fn try_from(r: RawCustomer) -> Result<Customer, String> {
        let taste = r.taste.ok_or("taste: needed, a list of tags for a wholesale customer or a product taste for a buyer")?;
        let (wholesale, taste) = match taste {
            Value::Array(_) => {
                let goods: Vec<Tag> = serde_json::from_value(taste).map_err(|e| format!("taste: {e}"))?;
                if r.town.is_some() || r.at.is_some() {
                    return Err("town, at: only a buyer has a town and a handover spot (taste is a tag list here)".into());
                }
                let location = r.location.ok_or("location: needed for a wholesale customer")?;
                let standard = r.standard.ok_or("standard: needed for a wholesale customer")?;
                let amount = r.amount.ok_or("amount: needed for a wholesale customer")?;
                (Some(Wholesale { location, goods, standard, amount, patience_s: r.patience_s }), None)
            }
            Value::Object(_) => {
                let t: Taste = serde_json::from_value(taste).map_err(|e| format!("taste: {e}"))?;
                for (field, set) in [("location", r.location.is_some()), ("standard", r.standard.is_some()), ("amount", r.amount.is_some()), ("patience_s", r.patience_s.is_some())] {
                    if set {
                        return Err(format!("{field}: not used by a buyer (its taste has the amount, the deadline is fixed)"));
                    }
                }
                if r.town.is_none() {
                    return Err("town: needed for a buyer".into());
                }
                if r.at.is_none() {
                    return Err("at: needed for a buyer (the handover spot)".into());
                }
                (None, Some(t))
            }
            _ => return Err("taste: a tag list for a wholesale customer or a product taste for a buyer".into()),
        };
        Ok(Customer { id: r.id, name: r.name, wholesale, taste, rhythm_s: r.rhythm_s, start: r.start, voice: r.voice, town: r.town, at: r.at, hidden: r.hidden, refers: r.refers })
    }
}

impl<'de> Deserialize<'de> for Customer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Customer, D::Error> {
        let raw = RawCustomer::deserialize(d)?;
        Customer::try_from(raw).map_err(serde::de::Error::custom)
    }
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
    /// What a buyer's order costs per crate before the effects and the relationship: the base
    /// reference of the product price (#229). TODO(initiator): a starting value.
    #[serde(default = "default_product_base")]
    pub product_base: f64,
}

fn default_product_base() -> f64 {
    120.0
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

/// The voice pools a customer has, with their field names (the counter ones only for buyers).
fn voice_pools(c: &Customer) -> Vec<(&'static str, &TextKey)> {
    let mut v = vec![("voice.order", &c.voice.order), ("voice.thanks", &c.voice.thanks), ("voice.grumble", &c.voice.grumble)];
    v.extend(c.voice.counter_yes.as_ref().map(|k| ("voice.counter_yes", k)));
    v.extend(c.voice.counter_no.as_ref().map(|k| ("voice.counter_no", k)));
    v
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
        c.check_referrals(&mut e);
        c.check_prices(kernel, &mut e);
        if e.is_empty() { Ok(c) } else { Err(e) }
    }

    /// One customer by id.
    pub fn get(&self, id: &CustomerId) -> Option<&Customer> {
        self.customers.get(id).map(|l| &l.record)
    }

    fn check_customer(&self, path: &str, r: &Customer, k: &Content, e: &mut Vec<String>) {
        check_id(path, "id", r.id.as_str(), e);
        if let Some(w) = &r.wholesale {
            self.check_wholesale(path, w, k, e);
        }
        if let Some(t) = &r.taste {
            self.check_buyer(path, r, t, k, e);
        }
        if !(r.rhythm_s[0] > 0.0 && r.rhythm_s[0] <= r.rhythm_s[1]) {
            e.push(format!("{path}: rhythm_s: needs 0 < min <= max"));
        }
        if !(0.0..=5.0).contains(&r.start) {
            e.push(format!("{path}: start: must be in 0..5"));
        }
    }

    fn check_wholesale(&self, path: &str, w: &Wholesale, k: &Content, e: &mut Vec<String>) {
        k.check_location(path, "location", &w.location, e);
        if w.goods.is_empty() {
            e.push(format!("{path}: taste: empty"));
        }
        for t in &w.goods {
            if !k.commodities.values().any(|c| c.record.tags.contains(t)) {
                e.push(format!("{path}: taste: no commodity has the tag '{t}'"));
            }
        }
        if !(0.0..=1.0).contains(&w.standard) {
            e.push(format!("{path}: standard: must be in 0..1"));
        }
        if w.amount[0] == 0 || w.amount[0] > w.amount[1] {
            e.push(format!("{path}: amount: needs 1 <= min <= max"));
        }
        if w.patience_s.is_some_and(|p| !(p > 0.0)) {
            e.push(format!("{path}: patience_s: must be positive"));
        }
    }

    fn check_buyer(&self, path: &str, r: &Customer, t: &Taste, k: &Content, e: &mut Vec<String>) {
        if let Some(at) = &r.at {
            k.check_location(path, "at", at, e);
        }
        if let Some(town) = &r.town {
            check_id(path, "town", town.as_str(), e);
        }
        if t.wants.is_empty() {
            e.push(format!("{path}: wants: needs at least one effect"));
        }
        let mut seen = BTreeSet::new();
        for w in &t.wants {
            if !k.effects.contains_key(w) {
                e.push(format!("{path}: wants: unknown effect '{w}'"));
            }
            if !seen.insert(w) {
                e.push(format!("{path}: wants: '{w}' is listed twice"));
            }
        }
        if !(1..=2).contains(&t.picks) || t.picks as usize > t.wants.len() {
            e.push(format!("{path}: picks: needs 1 or 2, and no more than the wants ({})", t.wants.len()));
        }
        if !(0.0..=1.0).contains(&t.min_quality) {
            e.push(format!("{path}: min_quality: must be in 0..1"));
        }
        if t.amount[0] == 0 || t.amount[0] > t.amount[1] {
            e.push(format!("{path}: amount: needs 1 <= min <= max"));
        }
        if r.voice.counter_yes.is_none() {
            e.push(format!("{path}: voice.counter_yes: needed for a buyer"));
        }
        if r.voice.counter_no.is_none() {
            e.push(format!("{path}: voice.counter_no: needed for a buyer"));
        }
    }

    /// `refers` names a hidden customer; a hidden customer is referred by someone.
    fn check_referrals(&self, e: &mut Vec<String>) {
        for Loaded { path, record: r } in self.customers.values() {
            if let Some(new) = &r.refers {
                match self.customers.get(&CustomerId::new(new.clone())) {
                    None => e.push(format!("{path}: refers: unknown customer '{new}'")),
                    Some(n) if !n.record.hidden => e.push(format!("{path}: refers: '{new}' is not hidden")),
                    Some(_) => {}
                }
                if new == r.id.as_str() {
                    e.push(format!("{path}: refers: a customer cannot refer itself"));
                }
            }
        }
        for Loaded { path, record: r } in self.customers.values() {
            if r.hidden && !self.customers.values().any(|x| x.record.refers.as_deref() == Some(r.id.as_str())) {
                e.push(format!("{path}: hidden: nobody refers to it, so it would never order"));
            }
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
        for c in k.commodities.values().filter(|c| self.customers.values().any(|x| x.record.wholesale.as_ref().is_some_and(|w| w.goods.iter().any(|t| c.record.tags.contains(t))))) {
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
        if !(p.product_base > 0.0) {
            e.push(format!("{path}: product_base: must be positive"));
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
            for (field, k) in voice_pools(c) {
                match table.lines(k.as_str()) {
                    None => e.push(format!("{path}: {field}: no text '{k}'")),
                    Some(l) if l.len() < MIN_POOL => e.push(format!("{path}: {field}: '{k}' needs at least {MIN_POOL} lines (it has {})", l.len())),
                    Some(_) => {}
                }
            }
        }
        e
    }

    /// Checks that all voice pools have at least 3 lines (TODO(initiator): pool text placeholders).
    pub fn check_pool_sizes(&self, table: &TextTable) -> Vec<String> {
        let mut e = Vec::new();
        for Loaded { path, record: c } in self.customers.values() {
            for (field, k) in voice_pools(c) {
                if let Some(lines) = table.lines(k.as_str())
                    && lines.len() < MIN_POOL
                {
                    e.push(format!("{path}: {field}: '{k}' has {}, needs at least {MIN_POOL}", lines.len()));
                }
            }
        }
        e
    }
}
