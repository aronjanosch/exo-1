//! Engineering: the reactor's glue (its asks, its screens), the coolant, the antimatter canisters, the blast
//! and the cold start.
use crate::dev_area::*;
use crate::dev_area::ship::{counter_fresh, ShipSet};
use crate::gameplay::Gameplay;
use crate::items::Items;
use gameplay_core::notice::{Notice, NoticeKind};

pub mod asks;
pub mod blast;
pub mod coolant;
pub mod panels;
pub mod canisters;
pub mod startup_screen;

pub fn plugin(app: &mut App) {
    app.add_plugins((asks::plugin, panels::plugin, coolant::plugin, canisters::plugin));
    app.init_resource::<blast::PendingBlasts>().init_resource::<blast::BlastFlash>();
    app.add_systems(FixedUpdate, blast::collect_blasts.in_set(ShipSet::React));
    app.add_systems(FixedUpdate, (blast::reactor_blast, blast::client_blast_news, eject_notice, ecool_notice).chain().in_set(ShipSet::Effects));
}

pub fn eject_notice(items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>) {
    let Some(news) = items.reactor else { return };
    if counter_fresh(&mut seen, news.ejects) {
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.eject"));
    }
}

pub fn ecool_notice(items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>) {
    let Some(news) = items.reactor else { return };
    if counter_fresh(&mut seen, news.ecools) {
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.ecool"));
    }
}


