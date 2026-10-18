//! The canister's sound hook (#320): a named message when a canister starts or stops humming, nothing audio is wired to
//! it yet (same style as `PartSound` of the controls kit). A canister hums while it is carried (the selected bar slot) and
//! while it sits in a reactor slot; a spent one is silent.
use crate::items::Items;
use bevy::prelude::*;
use item_core::wire::CANISTER_SLOTS;

/// Where the humming canister is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumPlace {
    /// In the local player's hands.
    Carried,
    /// Seated in reactor slot `n` (0-based).
    Seated(usize),
}

/// A canister starts (`on`) or stops humming at `place`. `life` is its fill (0..100) when it changed, so the sound can
/// pitch with it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct CanisterHum {
    pub place: HumPlace,
    pub on: bool,
    pub life: f64,
}

pub fn plugin(app: &mut App) {
    app.add_message::<CanisterHum>();
    app.add_systems(FixedUpdate, hum_edges.in_set(crate::dev_area::ship::ShipSet::Effects));
}

/// What hums now: the carried canister and each seated one with life left.
fn humming(items: &Items) -> [Option<f64>; 1 + CANISTER_SLOTS] {
    let mut now = [None; 1 + CANISTER_SLOTS];
    let held = items.client.view.slot(items.client.view.selected).filter(|s| s.item.kind.as_str().starts_with("antimatter_canister"));
    now[0] = held.and_then(|s| s.item.condition).filter(|l| *l > 0.0);
    if let Some(n) = items.reactor {
        for (i, c) in n.canisters.iter().enumerate() {
            now[1 + i] = c.canister.is_some().then_some(c.life).filter(|l| *l > 0.0);
        }
    }
    now
}

fn hum_edges(items: Res<Items>, mut was: Local<[Option<f64>; 1 + CANISTER_SLOTS]>, mut out: MessageWriter<CanisterHum>) {
    let now = humming(&items);
    for (i, (a, b)) in was.iter().zip(now.iter()).enumerate() {
        if a.is_some() != b.is_some() {
            let place = if i == 0 { HumPlace::Carried } else { HumPlace::Seated(i - 1) };
            out.write(CanisterHum { place, on: b.is_some(), life: b.or(*a).unwrap_or(0.0) });
        }
    }
    *was = now;
}
