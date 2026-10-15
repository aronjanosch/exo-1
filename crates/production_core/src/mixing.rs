//! The pure rules of a brew: what a mix gives, where the needle sits, how good the
//! brew is and what the product sells for. No state and no events.
use std::collections::BTreeSet;

use gameplay_core::{Content, EffectId, ProductSpec};

use crate::ingredient::{Ingredient, ProductBase, Tuning};

/// The effects of a mix: the base's start effects, then each ingredient in order. Its transforms
/// run first (a `from` in the mix with its `to` absent is replaced), then its add (if it is not
/// there yet and the mix is below the base's `max_effects`).
pub fn mix(base: &ProductBase, ingredients: &[&Ingredient]) -> BTreeSet<EffectId> {
    let mut effects: BTreeSet<EffectId> = base.effects.iter().cloned().collect();
    for ing in ingredients {
        for t in &ing.transforms {
            if effects.contains(&t.from) && !effects.contains(&t.to) {
                effects.remove(&t.from);
                effects.insert(t.to.clone());
            }
        }
        if let Some(a) = &ing.adds
            && !effects.contains(a)
            && effects.len() < base.max_effects as usize
        {
            effects.insert(a.clone());
        }
    }
    effects
}

/// The needle position (0..1) at `t` seconds: a triangle wave, 0 to 1 to 0 in one `period`.
pub fn needle(t: f64, period: f64) -> f64 {
    let phase = (t / period).rem_euclid(1.0);
    if phase < 0.5 { 2.0 * phase } else { 2.0 * (1.0 - phase) }
}

/// 1 at the sweet spot, falling linearly to 0 at `sweet_width` away (never below 0).
pub fn quality(pos: f64, t: &Tuning) -> f64 {
    (1.0 - (pos - t.sweet_spot).abs() / t.sweet_width).clamp(0.0, 1.0)
}

/// Base price x (1 + the effects' values) x (0.5 + 0.5 x quality), rounded. None when the base or
/// an effect is not a kernel record (the lab's loader checks the shipped ones).
pub fn price(p: &ProductSpec, kernel: &Content) -> Option<i64> {
    let base = kernel.commodities.get(&p.base)?.record.base_price as f64;
    let mut sum = 0.0;
    for e in &p.effects {
        sum += kernel.effects.get(e)?.record.value;
    }
    Some((base * (1.0 + sum) * (0.5 + 0.5 * p.quality)).round() as i64)
}
