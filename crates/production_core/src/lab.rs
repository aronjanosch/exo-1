//! The lab: kettles by station and the recipe book. A kettle takes a base and up
//! to `max_ingredients`, brews for `brew_s`, then waits for the release. The needle position at the
//! release sets the quality. The first brew of a mix goes into the book.
//!
//! The lab reads `TimePassed` to count brews down; it raises `Brewed` and notices on release. The
//! glue does the rest (the requests `GoodsBought` and the taking). Nothing here calls another system.
use std::collections::{BTreeMap, BTreeSet};

use gameplay_core::notice::{Notice, NoticeKind};
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::{CommodityId, EffectId, Event, ProductSpec, WorldEvent};
use serde::{Deserialize, Serialize};

use crate::ingredient::{Ingredient, LabContent};
use crate::mixing::{mix, needle, quality};

pub const SECTION: &str = "lab";
pub const SECTION_VERSION: u32 = 1;
/// The text pools of the notices (at least 3 lines each).
pub const BREWED_KEY: &str = "lab.brewed";
pub const DISCOVERED_KEY: &str = "lab.discovered";

/// A mix in the recipe book: the base and the effects it gave.
pub type Mix = (CommodityId, BTreeSet<EffectId>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a base or an ingredient of the lab.
    UnknownGoods,
    /// The kettle has a base already.
    SecondBase,
    /// An ingredient before the base.
    IngredientBeforeBase,
    /// The kettle is full.
    TooManyIngredients,
    /// No base in the kettle, nothing to start.
    NoBase,
    /// Brewing, done or ready: the kettle takes nothing now.
    Busy,
    /// The release needs a finished brew.
    NotDone,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kettle {
    Empty,
    Loaded { base: CommodityId, ingredients: Vec<CommodityId> },
    Brewing { base: CommodityId, ingredients: Vec<CommodityId>, remaining_s: f64 },
    /// Brewed; waits for the release. The needle is read then.
    Done { base: CommodityId, ingredients: Vec<CommodityId> },
    /// Released; waits for the take.
    Ready { product: ProductSpec, crates: u32 },
}

impl Kettle {
    fn tick(self, dt: f64) -> Kettle {
        match self {
            Kettle::Brewing { base, ingredients, remaining_s } => {
                let left = remaining_s - dt;
                if left <= 0.0 { Kettle::Done { base, ingredients } } else { Kettle::Brewing { base, ingredients, remaining_s: left } }
            }
            other => other,
        }
    }
}

/// What a lab step did: a domain event for every system, or a notice for the player.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Emit(WorldEvent),
    Notice(Notice),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lab {
    kettles: BTreeMap<String, Kettle>,
    book: BTreeSet<Mix>,
}

impl Lab {
    pub fn new() -> Lab {
        Lab { kettles: BTreeMap::new(), book: BTreeSet::new() }
    }

    /// The kettle of a station; None until something was put into it.
    pub fn kettle(&self, station: &str) -> Option<&Kettle> {
        self.kettles.get(station)
    }

    /// The mixes the crew has made, for the recipe tab.
    pub fn book(&self) -> &BTreeSet<Mix> {
        &self.book
    }

    fn current(&self, station: &str) -> Kettle {
        self.kettles.get(station).cloned().unwrap_or(Kettle::Empty)
    }

    /// Puts a base or an ingredient into the kettle. A refusal leaves the kettle as it was.
    pub fn insert(&mut self, lc: &LabContent, station: &str, commodity: &CommodityId) -> Result<(), Refusal> {
        let is_base = lc.bases.contains_key(commodity);
        if !is_base && !lc.ingredients.contains_key(commodity) {
            return Err(Refusal::UnknownGoods);
        }
        let next = match self.current(station) {
            Kettle::Empty if is_base => Kettle::Loaded { base: commodity.clone(), ingredients: Vec::new() },
            Kettle::Empty => return Err(Refusal::IngredientBeforeBase),
            Kettle::Loaded { base, mut ingredients } => {
                if is_base {
                    return Err(Refusal::SecondBase);
                }
                if ingredients.len() >= lc.tuning.max_ingredients as usize {
                    return Err(Refusal::TooManyIngredients);
                }
                ingredients.push(commodity.clone());
                Kettle::Loaded { base, ingredients }
            }
            _ => return Err(Refusal::Busy),
        };
        self.kettles.insert(station.to_string(), next);
        Ok(())
    }

    /// Starts the brew of a loaded kettle.
    pub fn start(&mut self, lc: &LabContent, station: &str) -> Result<(), Refusal> {
        match self.current(station) {
            Kettle::Loaded { base, ingredients } => {
                self.kettles.insert(station.to_string(), Kettle::Brewing { base, ingredients, remaining_s: lc.tuning.brew_s });
                Ok(())
            }
            Kettle::Empty => Err(Refusal::NoBase),
            _ => Err(Refusal::Busy),
        }
    }

    /// Counts the brewing kettles down by `dt` seconds; a finished brew waits for the release.
    pub fn step(&mut self, dt: f64) {
        for k in self.kettles.values_mut() {
            let old = std::mem::replace(k, Kettle::Empty);
            *k = old.tick(dt);
        }
    }

    /// Releases a finished brew `t_since_done` seconds after it was done: the needle is read there,
    /// the mix goes into the book (a discovery the first time) and the brew is ready to take.
    pub fn release(&mut self, lc: &LabContent, station: &str, t_since_done: f64) -> Result<Vec<Outcome>, Refusal> {
        let Kettle::Done { base, ingredients } = self.current(station) else { return Err(Refusal::NotDone) };
        // Both ids were checked when they went into the kettle, so the lookups hold.
        let b = &lc.bases[&base].record;
        let recipes: Vec<&Ingredient> = ingredients.iter().map(|i| &lc.ingredients[i].record).collect();
        let effects = mix(b, &recipes);
        let product = ProductSpec {
            base: base.clone(),
            effects: effects.clone(),
            quality: quality(needle(t_since_done, lc.tuning.needle_period_s), &lc.tuning),
        };
        let crates = b.crates_out;
        let discovered = self.book.insert((base, effects));
        self.kettles.insert(station.to_string(), Kettle::Ready { product: product.clone(), crates });
        let notice = if discovered {
            Notice::new(NoticeKind::Unlock, DISCOVERED_KEY)
        } else {
            Notice::new(NoticeKind::Updated, BREWED_KEY)
        };
        Ok(vec![
            Outcome::Emit(WorldEvent::Brewed { station: station.to_string(), product, crates, discovered }),
            Outcome::Notice(notice),
        ])
    }

    /// Takes a ready brew out of the kettle, which is empty after it.
    pub fn take(&mut self, station: &str) -> Option<(ProductSpec, u32)> {
        match self.current(station) {
            Kettle::Ready { product, crates } => {
                self.kettles.insert(station.to_string(), Kettle::Empty);
                Some((product, crates))
            }
            _ => None,
        }
    }

    /// The effects a loaded kettle would give, only when that mix is already in the book (an
    /// unknown mix shows "???", so None).
    pub fn preview(&self, lc: &LabContent, station: &str) -> Option<BTreeSet<EffectId>> {
        let Some(Kettle::Loaded { base, ingredients }) = self.kettles.get(station) else { return None };
        let b = &lc.bases.get(base)?.record;
        let recipes: Vec<&Ingredient> = ingredients.iter().map(|i| &lc.ingredients[i].record).collect();
        let effects = mix(b, &recipes);
        self.book.contains(&(base.clone(), effects.clone())).then_some(effects)
    }

    /// Reads the lab's own time: `TimePassed` counts the brews down. Raises nothing.
    pub fn apply_world(&mut self, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        if let WorldEvent::TimePassed { dt } = &ev.payload {
            self.step(*dt);
        }
        Vec::new()
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no lab section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Lab>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}

impl Default for Lab {
    fn default() -> Self {
        Self::new()
    }
}
