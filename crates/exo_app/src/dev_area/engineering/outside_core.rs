//! The ejected core outside the ship (#325): a glowing box on the ground south of the engine room that keeps counting; at
//! zero it implodes there. The host hurts the players by distance (`outside_hurt`); every process shows the flash, the notice
//! and the box from the host's news (`ReactorNews::outside_eta_s`, `core_implosions`). Nothing happens to the hull or the
//! room: the sim gives no blast for it (`ReactorEvent::CoreImplodedOutside`).
use crate::dev_area::engineering::blast::BlastFlash;
use crate::dev_area::layout::{ROOM, WALL};
use crate::dev_area::ship::{counter_fresh, ReactorConfig};
use crate::dev_area::DevArea;
use crate::env::PlanetRes;
use crate::gameplay::Gameplay;
use crate::health::{area_pos, walker_points, HealthState};
use crate::items::{Items, Role};
use crate::net_live::RemoteWalker;
use crate::origin::{WorldPose, WorldPos};
use crate::walker::Player;
use bevy::prelude::*;
use gameplay_core::notice::{Notice, NoticeKind};
use reactor_core::Blast;

/// How far south of the engine room's south wall the core lies, metres. TODO(initiator): start value 6 m.
pub const CORE_OUT_SOUTH_M: f64 = 6.0;
/// Where the ejected core lies, area metres (the area's y runs north; the room's wall is at its centre minus half).
pub const CORE_OUT_AT: [f64; 2] = [ROOM.centre[0], ROOM.centre[1] - ROOM.half[1] - WALL * 0.5 - CORE_OUT_SOUTH_M];
/// The glowing box's edge, metres. TODO(initiator): the look.
pub const CORE_OUT_SIZE: f32 = 0.9;

/// The burn a body at `d` metres from the core takes when it implodes outside: full inside half the safe distance, falling
/// linearly to nothing at the safe distance and beyond. No NaN for a bad distance (it counts as far).
pub fn outside_hurt(b: &Blast, d: f64) -> f64 {
    let safe = b.safe_distance_m;
    if !(safe > 0.0) || !d.is_finite() || d >= safe {
        return 0.0;
    }
    let full = safe * 0.5;
    if d <= full {
        return b.outside_burn;
    }
    b.outside_burn * ((safe - d) / (safe - full)).clamp(0.0, 1.0)
}

/// Metres on the ground from the ejected core.
pub fn distance_to_core(p: [f64; 2]) -> f64 {
    ((p[0] - CORE_OUT_AT[0]).powi(2) + (p[1] - CORE_OUT_AT[1]).powi(2)).sqrt()
}

/// Every process: the outside implosion in the news shows the flash and the notice once; the host also hurts the players near the
/// core (the health stays on the host, as the blast's).
#[allow(clippy::too_many_arguments)]
pub fn core_implosion(time: Res<Time>, items: Res<Items>, cfg: Res<ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut health: Option<ResMut<HealthState>>, mut seen: Local<Option<u32>>, mut flash: ResMut<BlastFlash>, mut gp: ResMut<Gameplay>) {
    let Some(news) = items.reactor else { return };
    if !counter_fresh(&mut seen, news.core_implosions) {
        return;
    }
    // TODO(initiator): the flash shows to everybody; its reach (and the stun) are design.
    flash.blast(time.elapsed_secs_f64());
    gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.core_implosion"));
    if items.role == Role::Client {
        return;
    }
    let b = cfg.0.reactor.blast;
    info!("reactor: the ejected core imploded outside (burn {:.0} inside {:.1} m, none beyond {:.1} m)", b.outside_burn, b.safe_distance_m * 0.5, b.safe_distance_m);
    for (id, p) in walker_points(&players, &remotes) {
        let Some(q) = area_pos(area.as_deref(), &planet, Some(p)) else { continue };
        let burn = outside_hurt(&b, distance_to_core(q));
        if burn > 0.0
            && let Some(st) = health.as_deref_mut()
            && let Some(body) = st.bodies.get_mut(&id)
        {
            body.health.add(&st.tuning, "burn", burn);
        }
    }
}

#[derive(Component)]
pub struct OutsideCore;

/// Window: the glowing box while a core out there counts; it pulses faster and brighter as the time runs out.
#[allow(clippy::too_many_arguments)]
pub fn core_box(mut commands: Commands, time: Res<Time>, items: Res<Items>, cfg: Res<ReactorConfig>, planet: Res<PlanetRes>, area: Res<DevArea>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, boxes: Query<(Entity, &MeshMaterial3d<StandardMaterial>), With<OutsideCore>>) {
    let eta = items.reactor.and_then(|n| n.outside_eta_s).filter(|e| e.is_finite());
    let Some(eta) = eta else {
        for (e, _) in &boxes {
            commands.entity(e).despawn();
        }
        return;
    };
    let delay = cfg.0.reactor.implosion_delay_s.max(1.0);
    let left = (eta / delay).clamp(0.0, 1.0);
    let hz = 1.0 + 5.0 * (1.0 - left);
    let pulse = 0.65 + 0.35 * (time.elapsed_secs_f64() * hz * std::f64::consts::TAU).sin();
    let glow = (2.0 + 10.0 * (1.0 - left)) as f32 * pulse as f32;
    if let Ok((_, m)) = boxes.single() {
        if let Some(mut mat) = materials.get_mut(&m.0) {
            mat.emissive = LinearRgba::rgb(glow, glow * 0.35, glow * 0.1);
        }
        return;
    }
    let Some((pos, up)) = area.ground(&planet, CORE_OUT_AT, CORE_OUT_SIZE as f64 * 0.5) else { return };
    let mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.5, 0.15), emissive: LinearRgba::rgb(glow, glow * 0.35, glow * 0.1), ..default() });
    let rot = area.frame.map_or(Quat::IDENTITY, |f| f.rotation(up, 20.0).as_quat());
    commands.spawn((OutsideCore, Mesh3d(meshes.add(Cuboid::new(CORE_OUT_SIZE, CORE_OUT_SIZE, CORE_OUT_SIZE))), MeshMaterial3d(mat), Transform::from_rotation(rot), WorldPos(pos)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_burn_is_full_near_falls_to_nothing_at_the_safe_distance() {
        let b = Blast::default();
        assert_eq!(outside_hurt(&b, 0.0), b.outside_burn);
        assert_eq!(outside_hurt(&b, b.safe_distance_m * 0.5), b.outside_burn);
        let mid = outside_hurt(&b, b.safe_distance_m * 0.75);
        assert!((mid - b.outside_burn * 0.5).abs() < 1e-9, "{mid}");
        assert_eq!(outside_hurt(&b, b.safe_distance_m), 0.0);
        assert_eq!(outside_hurt(&b, 1000.0), 0.0);
    }

    #[test]
    fn a_bad_distance_or_a_bad_config_gives_no_burn_not_nan() {
        let b = Blast::default();
        for d in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(outside_hurt(&b, d), 0.0, "{d}");
        }
        assert_eq!(outside_hurt(&Blast { safe_distance_m: 0.0, ..b }, 1.0), 0.0);
        assert_eq!(outside_hurt(&Blast { safe_distance_m: f64::NAN, ..b }, 1.0), 0.0);
    }

    #[test]
    fn the_core_lies_about_six_metres_south_of_the_south_wall() {
        assert!((CORE_OUT_AT[1] - (-25.15)).abs() < 1e-9 && CORE_OUT_AT[0] == ROOM.centre[0]);
    }
}
