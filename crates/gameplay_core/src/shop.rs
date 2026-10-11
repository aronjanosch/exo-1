//! Shops (#230): money turned into progress. A shop stands at a location and sells its items:
//! an unlock (the progress charges its price when the event is applied) or goods (a wallet change
//! and `GoodsBought`). Records in `shop/`, loaded by `Shops::load`. The shop only reads progress;
//! `buy` returns the events for the host to route.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records};
use serde::Deserialize;

use crate::condition::Condition;
use crate::content::{Content, WALLET};
use crate::id::{CommodityId, LocationId, TextKey, TrackId, UnlockId};
use crate::event::WorldEvent;
use crate::progress::Progress;
use crate::text::TextTable;

pub const FOLDER: &str = "shop";

/// One item on sale. In JSON: `{ "unlock": "hidden_hold" }`, or `{ "goods": "fizzy_mud", "amount": 3,
/// "price": 90, "condition": { … } }` (the condition is optional).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawItem")]
pub enum ShopItem {
    Unlock { unlock: UnlockId },
    Goods { goods: CommodityId, amount: u32, price: i64, condition: Option<Condition> },
}

/// The item as written; `TryFrom` checks it into a `ShopItem`, so a bad item names what is wrong.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawItem {
    unlock: Option<UnlockId>,
    goods: Option<CommodityId>,
    amount: Option<u32>,
    price: Option<i64>,
    condition: Option<Condition>,
}

impl TryFrom<RawItem> for ShopItem {
    type Error = String;

    fn try_from(r: RawItem) -> Result<ShopItem, String> {
        match (r.unlock, r.goods) {
            (Some(_), Some(_)) => Err("items: an item is an unlock or goods, not both".to_string()),
            (None, None) => Err("items: an item needs 'unlock' or 'goods'".to_string()),
            (Some(unlock), None) => {
                if r.amount.is_some() || r.price.is_some() || r.condition.is_some() {
                    return Err("items: an unlock item takes no amount, price or condition (the unlock has its own price)".to_string());
                }
                Ok(ShopItem::Unlock { unlock })
            }
            (None, Some(goods)) => {
                let (Some(amount), Some(price)) = (r.amount, r.price) else {
                    return Err("items: goods need 'amount' and 'price'".to_string());
                };
                if amount == 0 {
                    return Err("items: amount must be at least 1".to_string());
                }
                if price <= 0 {
                    return Err("items: price must be positive".to_string());
                }
                Ok(ShopItem::Goods { goods, amount, price, condition: r.condition })
            }
        }
    }
}

/// A shop: where it stands, what it sells. TODO(initiator): the stock and prices are starting values.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shop {
    pub id: String,
    pub name: TextKey,
    pub location: LocationId,
    pub items: Vec<ShopItem>,
}

impl Record for Shop {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

/// What the shop screen shows for one item. `amount` is set for goods; `owned` is only ever true
/// for an unlock (goods are used up).
#[derive(Clone, Debug, PartialEq)]
pub struct OfferView {
    pub label: TextKey,
    pub price: i64,
    pub amount: Option<u32>,
    /// The shop's location is open and the item's condition holds.
    pub available: bool,
    pub affordable: bool,
    pub owned: bool,
}

/// Why a purchase did not happen. Nothing changed.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// No item at that index.
    NoSuchItem,
    /// The shop is not at this location.
    WrongLocation,
    /// The location is not open to the crew yet.
    Closed,
    /// The unlock is already owned.
    Owned,
    ConditionNotMet,
    NotEnoughMoney { price: i64, wallet: i64 },
}

impl Shop {
    /// What the shop offers the crew now.
    pub fn offers(&self, p: &Progress, k: &Content) -> Vec<OfferView> {
        let open = p.location_available(k, &self.location);
        self.items
            .iter()
            .map(|item| match item {
                ShopItem::Unlock { unlock } => {
                    let u = &k.unlocks.get(unlock).expect("checked by Shops::load").record;
                    OfferView {
                        label: u.name.clone(),
                        price: u.price,
                        amount: None,
                        available: open && u.condition.as_ref().is_none_or(|c| c.holds(k, p, None)),
                        affordable: p.wallet() >= u.price,
                        owned: p.owns(unlock),
                    }
                }
                ShopItem::Goods { goods, amount, price, condition } => {
                    let c = &k.commodities.get(goods).expect("checked by Shops::load").record;
                    OfferView {
                        label: c.name.clone(),
                        price: *price,
                        amount: Some(*amount),
                        available: open && condition.as_ref().is_none_or(|c| c.holds(k, p, None)),
                        affordable: p.wallet() >= *price,
                        owned: false,
                    }
                }
            })
            .collect()
    }

    /// Buys item `idx` at `at`. An unlock gives `UnlockBought` (the progress charges its price);
    /// goods give the wallet change and `GoodsBought`. The purchase is read for the crew (no player).
    pub fn buy(&self, idx: usize, p: &Progress, k: &Content, at: LocationId) -> Result<Vec<WorldEvent>, Refusal> {
        let item = self.items.get(idx).ok_or(Refusal::NoSuchItem)?;
        if at != self.location {
            return Err(Refusal::WrongLocation);
        }
        if !p.location_available(k, &at) {
            return Err(Refusal::Closed);
        }
        match item {
            ShopItem::Unlock { unlock } => {
                if p.owns(unlock) {
                    return Err(Refusal::Owned);
                }
                let u = &k.unlocks.get(unlock).expect("checked by Shops::load").record;
                if u.condition.as_ref().is_some_and(|c| !c.holds(k, p, None)) {
                    return Err(Refusal::ConditionNotMet);
                }
                if p.wallet() < u.price {
                    return Err(Refusal::NotEnoughMoney { price: u.price, wallet: p.wallet() });
                }
                Ok(vec![WorldEvent::UnlockBought { unlock: unlock.clone() }])
            }
            ShopItem::Goods { goods, amount, price, condition } => {
                if condition.as_ref().is_some_and(|c| !c.holds(k, p, None)) {
                    return Err(Refusal::ConditionNotMet);
                }
                if p.wallet() < *price {
                    return Err(Refusal::NotEnoughMoney { price: *price, wallet: p.wallet() });
                }
                Ok(vec![
                    WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta: -price, player: None },
                    WorldEvent::GoodsBought { commodity: goods.clone(), amount: *amount, at },
                ])
            }
        }
    }
}

/// The loaded shops, by id.
#[derive(Clone, Debug, PartialEq)]
pub struct Shops {
    pub shops: BTreeMap<String, Loaded<Shop>>,
}

impl Shops {
    /// Loads `shop/` from `files` and checks ids, locations, items (unlocks and goods exist, goods
    /// conditions hold against the kernel). Returns every error, each naming the file and the field.
    pub fn load(files: &[File], k: &Content) -> Result<Shops, Vec<String>> {
        let mut e = Vec::new();
        let shops = load_records::<Shop>(files, FOLDER, &mut e);
        for Loaded { path, record: r } in shops.values() {
            check_id(path, "id", &r.id, &mut e);
            k.check_location(path, "location", &r.location, &mut e);
            if r.items.is_empty() {
                e.push(format!("{path}: items: empty"));
            }
            for item in &r.items {
                match item {
                    ShopItem::Unlock { unlock } => {
                        if !k.unlocks.contains_key(unlock) {
                            e.push(format!("{path}: items: unknown unlock '{unlock}'"));
                        }
                    }
                    ShopItem::Goods { goods, condition, .. } => {
                        k.check_commodity(path, "items", goods, &mut e);
                        if let Some(c) = condition {
                            k.check_condition(path, "items", c, &mut e);
                        }
                    }
                }
            }
        }
        if e.is_empty() { Ok(Shops { shops }) } else { Err(e) }
    }

    /// The name of every shop is a text key the table has.
    pub fn check_texts(&self, table: &TextTable) -> Vec<String> {
        self.shops.values().filter(|s| !table.has(s.record.name.as_str())).map(|s| format!("{}: name: no text '{}'", s.path, s.record.name)).collect()
    }
}
