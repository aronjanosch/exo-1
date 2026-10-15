//! Open: the stun (no health affliction carries it yet, `TODO(initiator)`), the device damage.
use crate::dev_area::*;
use crate::dev_area::{layout::{ignition_at, IGNITION_OUT, REACTOR_AT, REACTOR_SIZE}};
use crate::dev_area::fire::{self, FireGrid, FireModel};
use crate::dev_area::hull::ShipAir;
use crate::dev_area::ship::ReactorConfig;
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

pub const BLAST_FLASH_S: f64 = 1.0;

#[derive(Resource, Default, Debug)]
pub struct PendingBlasts(pub Vec<Blast>);

/// The white flash: the start time (`.0`, None when none runs) and its peak strength (`.1`, 0..1).
/// A blast flashes at full strength; a jump at `JUMP_FLASH`, weaker.
#[derive(Resource, Default, Debug)]
pub struct BlastFlash(pub Option<f64>, pub f32);

/// The jump's flash peak (r40, #252): the blast's flash at half strength. TODO(initiator): the value.
pub const JUMP_FLASH: f32 = 0.5;

impl BlastFlash {
    pub fn blast(&mut self, now: f64) {
        self.0 = Some(now);
        self.1 = 1.0;
    }

    pub fn jump(&mut self, now: f64) {
        self.0 = Some(now);
        self.1 = JUMP_FLASH;
    }
}

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

pub fn flash_alpha(age: f64) -> f32 {
    if !(0.0..BLAST_FLASH_S).contains(&age) {
        return 0.0;
    }
    (1.0 - age / BLAST_FLASH_S) as f32
}

#[allow(clippy::too_many_arguments)]
pub fn reactor_blast(time: Res<Time>, mut pending: ResMut<PendingBlasts>, items: Res<Items>, cfg: Res<ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut health: Option<ResMut<HealthState>>, mut air: ResMut<ShipAir>, mut grid: ResMut<FireGrid>, model: Res<FireModel>, mut gp: ResMut<Gameplay>, mut flash: ResMut<BlastFlash>) {
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
                flash.blast(now);
            }
        }
        for p in fire_points() {
            fire::ignite_at(&mut air, &mut grid, *model, p, cfg.0.reactor.fire_size);
        }
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.blast"));
    }
}

/// process saw. True when it went up (the notice shows); the first news only sets the baseline, so a
/// late joiner does not replay old blasts. The flash is the radiation's range (as `reactor_blast`):
/// full at the blast when `dist` (this process's own walker, metres from the reactor) is inside it.
pub fn client_blast(seen: &mut Option<u32>, blasts: u32, dist: Option<f64>, radiation_range_m: f64, now: f64, flash: &mut BlastFlash) -> bool {
    let fresh = matches!(*seen, Some(s) if blasts > s);
    *seen = Some(blasts);
    if fresh && dist.is_some_and(|d| d < radiation_range_m) {
        flash.blast(now);
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

/// Host: the core's blasts of this tick (`ShipEvents`), for `reactor_blast`.
pub fn collect_blasts(events: Res<crate::dev_area::ship::ShipEvents>, mut pending: ResMut<PendingBlasts>) {
    for e in &events.0 {
        if let reactor_core::RoomEvent::Reactor(reactor_core::ReactorEvent::Blast(b)) = e {
            pending.0.push(b.clone());
        }
    }
}
