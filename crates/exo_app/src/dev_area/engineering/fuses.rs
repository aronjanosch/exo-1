//! Fuses for the junction boxes (#328): a box that broke is blown, and the repair waits for a new fuse.
//! F at the blown box with a fuse selected sends `Request::Load { slot, into: box }` (the path the antimatter canister takes
//! into the reactor's slot); the host's item authority uses one fuse up (`use` is `fit_fuse`) and `Items::fuse_fits` hands
//! the box to `fit_fuses`, which fits it. A box that is no longer blown (two players, one box) gives the fuse back.
use crate::dev_area::ship::{ReactorSim, ShipSet};
use crate::items::{Items, Role};
use bevy::prelude::*;
use item_core::wire::ReactorNews;

/// The `use` of the fuse item.
pub const FIT: &str = "fit_fuse";
/// The fuse's item id.
pub const FUSE: &str = "fuse";

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (fit_fuses, dev_incidents).in_set(ShipSet::Asks));
}

/// F10 "Director: on/off" and "Fault: force an incident now" (#328): the asks go to the host (`dev_asks`); the menu shows the
/// director's state from the news.
fn dev_incidents(mut menu: ResMut<crate::dev::DevMenu>, mut items: ResMut<Items>) {
    if let Some(n) = items.reactor
        && menu.director_on != n.director
    {
        menu.director_on = n.director;
    }
    if std::mem::take(&mut menu.director_toggle) {
        let on = !items.reactor.is_some_and(|n| n.director);
        items.ask_reactor(item_core::wire::ReactorAsk::SetDirector(on));
        menu.last = Some(format!("director {}", if on { "on" } else { "off" }));
    }
    if std::mem::take(&mut menu.force_incident) {
        items.ask_reactor(item_core::wire::ReactorAsk::ForceIncident);
        menu.last = Some("incident forced".into());
    }
}

/// The selected bar slot holds a fuse (the item whose use is `FIT`).
pub fn holds_fuse(items: &Items) -> bool {
    let slot = items.client.view.selected;
    items.client.view.slot(slot).and_then(|s| items.defs.of(s)).is_some_and(|d| d.use_effect.as_deref() == Some(FIT))
}

/// What F at box `i` does, by the news: None when the box is not blown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuseHint {
    /// A fuse is in hand: F fits it.
    Fit,
    /// Blown and no fuse in hand: it says so.
    NeedsFuse,
}

pub fn hint(news: &ReactorNews, i: usize, holds: bool) -> Option<FuseHint> {
    news.junction_blown[i].then_some(if holds { FuseHint::Fit } else { FuseHint::NeedsFuse })
}

/// F at a blown box with a fuse selected: asks the host to fit it. Nothing without a fuse (the prompt says one is needed).
pub fn press(i: usize, items: &mut Items, now: f64) {
    if !holds_fuse(items) {
        return;
    }
    let slot = items.client.view.selected;
    let _ = items.request(now, item_core::Request::Load { slot, into: i as u8 });
}

/// Host: the fuses taken out of hands go into their boxes.
fn fit_fuses(time: Res<Time>, mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>) {
    if items.role == Role::Client {
        return;
    }
    for (player, into) in std::mem::take(&mut items.fuse_fits) {
        let i = into as usize;
        if i < reactor_core::JUNCTIONS && sim.room.junction_mut(i).fit_fuse() {
            info!("junction box {}: a new fuse is in", i + 1);
            continue;
        }
        // Not blown (or no such box): the fuse was not needed, it goes back to the hand.
        let now = time.elapsed_secs_f64();
        if let Some(def) = items.defs.get(FUSE).cloned()
            && let Err(stack) = items.give(now, player, def.stack_of(1))
        {
            warn!("fuse: no room to hand a fuse back, {} lost", stack.count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_follows_blown_and_the_hand() {
        let n = ReactorNews { junction_blown: [false, true, false], ..Default::default() };
        assert_eq!(hint(&n, 0, true), None);
        assert_eq!(hint(&n, 1, false), Some(FuseHint::NeedsFuse));
        assert_eq!(hint(&n, 1, true), Some(FuseHint::Fit));
    }

    #[test]
    fn the_fuse_is_an_item_with_the_fit_use() {
        let defs = crate::items::load_defs();
        let d = defs.get(FUSE).expect("fuse.json");
        assert_eq!(d.use_effect.as_deref(), Some(FIT));
    }
}
