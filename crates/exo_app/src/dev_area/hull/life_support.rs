//! Life support: the outside's air follows the world, and every living body breathes its room's air.
use crate::dev_area::*;
use crate::dev_area::layout::*;
use crate::dev_area::hull::air::ShipAir;
use crate::items::{Items, Role};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::walker::Player;

/// Every step: the air outside the hull follows the planet's atmosphere at the area's height.
pub(super) fn ambient_step(planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut air: ResMut<ShipAir>) {
    let Some(frame) = area.as_deref().and_then(|a| a.frame) else { return };
    let (at, _) = frame.ground(&planet, 0.0, 0.0, 0.0);
    air.set_ambient(planet.ambient_air(at));
}

/// Host only: every living player breathes the air of the room they stand in, `breath_m3_per_s`
/// a second (`reactor_core` `RoomTuning`). Barotrauma `Character.cs:3835-3848` takes
/// `Hull.OxygenConsumptionSpeed` (`Hull.cs:124`, 700 px²/s) times dt from the hull the character is
/// in; the hull's area is in px (`Hull.cs:378-381`, 100 px = 1 m, `Physics.cs:22`), which is how the
/// rate in m³ comes out. Not ported: Barotrauma's "only while the body's own oxygen is lower"
/// (the body has no oxygen store here). The outside has no end, so nobody drains it. Unconscious
/// players breathe too: every body that is not dead counts. A body in its mask (r38) breathes
/// the mask while its tank has air (`dev_area/mask.rs`, Barotrauma `Character.cs:3851` with `UseHullOxygen` false).
pub(super) fn breath_step(time: Res<Time>, items: Res<Items>, cfg: Res<crate::dev_area::ship::ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, health: Option<Res<crate::health::HealthState>>, masked: Option<Res<crate::dev_area::gear::MaskAir>>, mut air: ResMut<ShipAir>) {
    if items.role == Role::Client {
        return;
    }
    let area = area.as_deref();
    let dt = time.delta_secs_f64();
    let rate = cfg.0.life_support.breath_m3_per_s * dt;
    for (id, p) in crate::health::walker_points(&players, &remotes) {
        let q = crate::health::area_pos(area, &planet, Some(p));
        let room = air.room_of(q.map_or(Place::Outside, place_of));
        if health.as_ref().and_then(|h| h.bodies.get(&id)).is_some_and(|b| b.health.state() == health_core::Life::Dead) {
            continue;
        }
        // A body breathing from its mask's tank (r38) takes no room air; an empty tank breathes the room.
        if masked.as_ref().is_some_and(|m| m.0.contains_key(&id)) {
            continue;
        }
        if room != air.outside {
            air.deck.breathe(room, rate);
        }
    }
}
