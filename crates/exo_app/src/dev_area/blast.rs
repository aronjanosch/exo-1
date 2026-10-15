//! The meltdown blast: what `ReactorEvent::Blast` does to the people in the engine
//! room. The rules and the values are `reactor_core::Blast` (the core, unchanged). The host applies
//! it (`reactor_blast`, after the reactor step): the burn and the radiation fall off with the
//! distance from the reactor (`falloff`), three fires start round the core, the local screen flashes
//! white when the local walker is in the radiation's range, and the news says REACTOR BLAST.
//! The other processes see it on the news (`ReactorNews::blasts`, `client_blast_news`): the same flash and notice.
//! Open: the stun (no health affliction carries it yet, `TODO(initiator)`), the device damage.
use super::*;
use crate::dev_area::engine_room::{ignition_at, IGNITION_OUT, REACTOR_AT, REACTOR_SIZE};
use crate::dev_area::fire::{self, FireGrid, FireModel};
use crate::dev_area::engine_room::EngineAir;
use crate::dev_area::reactor::ReactorConfig;
use crate::gameplay::Gameplay;
use crate::health::{area_pos, walker_points, HealthState};
use crate::items::{Items, Role};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::walker::Player;
use crate::env::PlanetRes;
use bevy::math::DVec2;
use gameplay_core::notice::{Notice, NoticeKind};
use reactor_core::{Blast, Source};

/// The white flash's length, s: full at the blast, gone at the end.
pub const BLAST_FLASH_S: f64 = 1.0;

/// The blasts the reactor step queued this tick, for `reactor_blast` (host only).
#[derive(Resource, Default, Debug)]
pub struct PendingBlasts(pub Vec<Blast>);

/// When the local screen's blast flash began (`effects`), None when it is over.
#[derive(Resource, Default, Debug)]
pub struct BlastFlash(pub Option<f64>);

/// The share of a blast's strength at `d` metres from the reactor: 1 at the reactor, 0 at `range_m`
/// and beyond (linear, as the rules say: Barotrauma's explosions fall off with distance).
pub fn falloff(d: f64, range_m: f64) -> f64 {
    if range_m <= 0.0 || d >= range_m {
        return 0.0;
    }
    (1.0 - d.max(0.0) / range_m).clamp(0.0, 1.0)
}

/// The burn and the radiation a body at `d` metres takes from the blast (before the afflictions'
/// caps, which `health_core` applies when they are added).
pub fn hurt_at(b: &Blast, d: f64) -> (f64, f64) {
    (b.burn * falloff(d, b.range_m), b.radiation * falloff(d, b.radiation_range_m))
}

/// Metres on the engine room's deck from the reactor's centre (horizontal, as the room is flat).
pub fn distance_to_reactor(p: [f64; 2]) -> f64 {
    DVec2::new(p[0] - REACTOR_AT[0], p[1] - REACTOR_AT[1]).length()
}

/// The three fires' points on the deck: the front (the one `ignition_at` gives a reactor fire) and
/// the two sides, outside the reactor's box and on the floor.
pub fn fire_points() -> [[f64; 2]; 3] {
    let side = REACTOR_SIZE[0] * 0.5 + IGNITION_OUT;
    [ignition_at(Source::Reactor), [REACTOR_AT[0] - side, REACTOR_AT[1]], [REACTOR_AT[0] + side, REACTOR_AT[1]]]
}

/// The flash's alpha `age` seconds after the blast: 1 at the start, 0 from `BLAST_FLASH_S` on.
pub fn flash_alpha(age: f64) -> f32 {
    if !(0.0..BLAST_FLASH_S).contains(&age) {
        return 0.0;
    }
    (1.0 - age / BLAST_FLASH_S) as f32
}

/// Host: the blasts the reactor step queued. Every body takes its burn and radiation by the falloff
/// (the afflictions cap them), three fires start round the core, the local screen flashes if the
/// local walker is in the radiation's range, and the news says REACTOR BLAST.
#[allow(clippy::too_many_arguments)]
pub fn reactor_blast(time: Res<Time>, mut pending: ResMut<PendingBlasts>, items: Res<Items>, cfg: Res<ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut health: Option<ResMut<HealthState>>, mut air: ResMut<EngineAir>, mut grid: ResMut<FireGrid>, model: Res<FireModel>, mut gp: ResMut<Gameplay>, mut flash: ResMut<BlastFlash>) {
    let blasts = std::mem::take(&mut pending.0);
    if items.role == Role::Client {
        return;
    }
    let now = time.elapsed_secs_f64();
    let at = walker_points(&players, &remotes);
    for b in blasts {
        info!("reactor: BLAST (burn {:.0}, radiation {:.0})", b.burn, b.radiation);
        for (id, p) in &at {
            let Some(q) = area_pos(area.as_deref(), &planet, Some(*p)) else { continue };
            let d = distance_to_reactor(q);
            let (burn, radiation) = hurt_at(&b, d);
            if let Some(st) = health.as_deref_mut()
                && let Some(body) = st.bodies.get_mut(id)
            {
                body.health.add(&st.tuning, "burn", burn);
                body.health.add(&st.tuning, "radiation", radiation);
            }
            if *id == crate::items::LOCAL && d < b.radiation_range_m {
                flash.0 = Some(now);
            }
        }
        for p in fire_points() {
            fire::ignite_at(&mut air, &mut grid, *model, p, cfg.0.reactor.fire_size);
        }
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.blast"));
    }
}

/// Client (and every process but the host): a news snapshot's blast counter against the last one this
/// process saw. True when it went up (the notice shows); the first news only sets the baseline, so a
/// late joiner does not replay old blasts. The flash is the radiation's range (as `reactor_blast`):
/// full at the blast when `dist` (this process's own walker, metres from the reactor) is inside it.
pub fn client_blast(seen: &mut Option<u32>, blasts: u32, dist: Option<f64>, radiation_range_m: f64, now: f64, flash: &mut BlastFlash) -> bool {
    let fresh = matches!(*seen, Some(s) if blasts > s);
    *seen = Some(blasts);
    if fresh && dist.is_some_and(|d| d < radiation_range_m) {
        flash.0 = Some(now);
    }
    fresh
}

/// Every process but the host: the blast the host applied shows here (the flash, the REACTOR BLAST
/// notice). The health effects stay on the host (`reactor_blast`), so nothing is applied twice.
#[allow(clippy::too_many_arguments)]
pub fn client_blast_news(time: Res<Time>, items: Res<Items>, cfg: Res<ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut seen: Local<Option<u32>>, mut flash: ResMut<BlastFlash>, mut gp: ResMut<Gameplay>) {
    if items.role != Role::Client {
        return;
    }
    let Some(news) = items.reactor else { return };
    let dist = walker_points(&players, &remotes).get(&crate::items::LOCAL).and_then(|p| area_pos(area.as_deref(), &planet, Some(*p))).map(distance_to_reactor);
    let now = time.elapsed_secs_f64();
    if client_blast(&mut seen, news.blasts, dist, cfg.0.reactor.blast.radiation_range_m, now, &mut flash) {
        info!("reactor: BLAST seen on the news ({})", news.blasts);
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.blast"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_area::engine_room::ROOM;

    #[test]
    fn the_falloff_is_full_at_the_reactor_and_gone_at_the_range() {
        assert_eq!(falloff(0.0, 7.5), 1.0);
        assert!((falloff(3.75, 7.5) - 0.5).abs() < 1e-12);
        assert_eq!(falloff(7.5, 7.5), 0.0);
        assert_eq!(falloff(25.0, 7.5), 0.0);
        assert_eq!(falloff(1.0, 0.0), 0.0, "no range, no effect");
    }

    #[test]
    fn a_walker_two_metres_off_takes_the_falloff_share_and_one_at_25_takes_nothing() {
        let b = Blast { min_fuel: 1.0, range_m: 7.5, burn: 500.0, stun_s: 15.0, device_damage: 500.0, radiation_range_m: 20.0, radiation: 75.0 };
        let (burn, rad) = hurt_at(&b, 2.0);
        assert!((burn - 500.0 * (1.0 - 2.0 / 7.5)).abs() < 1e-9, "burn {burn}");
        assert!((rad - 75.0 * (1.0 - 2.0 / 20.0)).abs() < 1e-9, "radiation {rad}");
        assert_eq!(hurt_at(&b, 25.0), (0.0, 0.0));
    }

    #[test]
    fn the_flash_is_full_at_the_blast_and_gone_after_a_second() {
        assert_eq!(flash_alpha(0.0), 1.0);
        assert!((flash_alpha(0.5) - 0.5).abs() < 1e-6);
        assert_eq!(flash_alpha(BLAST_FLASH_S), 0.0);
        assert_eq!(flash_alpha(-0.1), 0.0);
    }

    #[test]
    fn the_fires_start_outside_the_reactor_and_inside_the_room() {
        for p in fire_points() {
            let inside = (p[0] - REACTOR_AT[0]).abs() < REACTOR_SIZE[0] * 0.5 && (p[1] - REACTOR_AT[1]).abs() < REACTOR_SIZE[2] * 0.5;
            assert!(!inside, "fire at {p:?} is inside the reactor");
            assert!((p[0] - ROOM.centre[0]).abs() < ROOM.half[0] && (p[1] - ROOM.centre[1]).abs() < ROOM.half[1], "fire at {p:?} is outside the room");
        }
        assert_eq!(fire_points()[0], ignition_at(Source::Reactor), "the front is the reactor's own fire point");
    }

    #[test]
    fn a_higher_blast_counter_flashes_once_and_the_same_counter_again_does_not() {
        let mut seen = None;
        let mut flash = BlastFlash::default();
        assert!(!client_blast(&mut seen, 0, Some(2.0), 20.0, 1.0, &mut flash), "the first news is the baseline");
        assert_eq!(flash.0, None, "no flash on the baseline");
        assert!(client_blast(&mut seen, 1, Some(2.0), 20.0, 5.0, &mut flash), "the counter went up");
        assert_eq!(flash.0, Some(5.0), "the flash starts at the blast");
        flash.0 = None;
        assert!(!client_blast(&mut seen, 1, Some(2.0), 20.0, 6.0, &mut flash), "the same counter is not a new blast");
        assert_eq!(flash.0, None, "and no second flash");
        assert!(client_blast(&mut seen, 2, Some(2.0), 20.0, 7.0, &mut flash), "the next blast counts");
    }

    #[test]
    fn the_flash_needs_the_walker_in_the_radiation_range_but_the_notice_does_not() {
        let mut seen = Some(0);
        let mut flash = BlastFlash::default();
        assert!(client_blast(&mut seen, 1, Some(25.0), 20.0, 3.0, &mut flash), "the notice shows anywhere");
        assert_eq!(flash.0, None, "25 m is out of the radiation's range");
        assert!(client_blast(&mut seen, 2, None, 20.0, 4.0, &mut flash), "no walker of its own, still the notice");
        assert_eq!(flash.0, None);
    }
}
