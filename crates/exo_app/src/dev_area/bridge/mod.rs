//! The bridge: the captain (CP-01 and the captain's consoles), the helm, the jump drive and the shield.
use crate::dev_area::*;
use crate::dev_area::ship::{counter_fresh, host_asks, ReactorCounts, ReactorSim, ShipSet};
use crate::gameplay::Gameplay;
use crate::items::Items;
use gameplay_core::notice::{Notice, NoticeKind};
use item_core::wire::ReactorAsk;

pub mod captain;
pub mod captain_screen;

pub fn plugin(app: &mut App) {
    app.add_plugins(captain::plugin);
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
    app.add_systems(FixedUpdate, jump_notice.in_set(ShipSet::Effects));
}

pub fn window_plugin(app: &mut App) {
    captain::window_plugin(app);
}

fn is_mine(a: &ReactorAsk) -> bool {
    matches!(a, ReactorAsk::SetThrottle(_) | ReactorAsk::JumpArm | ReactorAsk::JumpGo)
}

fn asks(mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>, mut counts: ResMut<ReactorCounts>) {
    for a in host_asks(&mut items, is_mine) {
        apply(&mut sim, a, &mut counts);
    }
}

/// One of the bridge's asks on the room: the helm's throttle and the jump drive's ARM and JUMP;
/// false for an ask that is not the bridge's.
pub fn apply(s: &mut ReactorSim, a: ReactorAsk, counts: &mut ReactorCounts) -> bool {
    match a {
        // The HELM's slider (r38): the engines' target, the room's clamp (-100..100) and the host's.
        ReactorAsk::SetThrottle(pct) => s.room.set_engine_throttle(pct.clamp(-100.0, 100.0)),
        ReactorAsk::JumpArm => {
            if !s.room.jump_mut().arm() {
                info!("reactor: jump arm refused (the drive is not cold)");
            }
        }
        ReactorAsk::JumpGo => {
            if s.room.jump_go() {
                counts.jumps += 1;
                info!("reactor: JUMP");
            } else {
                info!("reactor: jump refused (the charge is not full)");
            }
        }
        _ => return false,
    }
    true
}

/// The jump's notice and flash, on every process (the news counter `jumps`, as the ejects): the
/// white flash at half the blast's strength (`JUMP_FLASH`) and the JUMP line.
pub fn jump_notice(time: Res<Time>, items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>, mut flash: ResMut<crate::dev_area::engineering::blast::BlastFlash>) {
    let Some(news) = items.reactor else { return };
    if counter_fresh(&mut seen, news.jumps) {
        flash.jump(time.elapsed_secs_f64());
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.jump"));
    }
}
