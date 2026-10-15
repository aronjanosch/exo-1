use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::id::{CommodityId, EffectId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductSpec {
    pub base: CommodityId,
    pub effects: BTreeSet<EffectId>,
    /// 0..1, from the brew.
    pub quality: f64,
}

impl ProductSpec {
    /// The share of `wants` this product has, 0..1 (1 when nothing is wanted).
    pub fn matches(&self, wants: &[EffectId]) -> f64 {
        if wants.is_empty() {
            return 1.0;
        }
        wants.iter().filter(|w| self.effects.contains(*w)).count() as f64 / wants.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_share() {
        let p = ProductSpec { base: CommodityId::new("x"), effects: [EffectId::new("a"), EffectId::new("b")].into(), quality: 0.5 };
        assert_eq!(p.matches(&[]), 1.0);
        assert_eq!(p.matches(&[EffectId::new("a")]), 1.0);
        assert_eq!(p.matches(&[EffectId::new("a"), EffectId::new("c")]), 0.5);
    }
}
