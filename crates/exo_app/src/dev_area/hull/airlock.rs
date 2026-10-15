//! The airlock (r40, #267): the room between the corridor and the outside, its two doors and their
//! panels. The state machine is `reactor_core::Airlock` (the interlock and the cycle); the air is
//! `ShipAir` (the rooms, the gaps, the pump's moves). Here: the doors' slabs, the air step, the
//! panels' presses.
//!
//! Sources (Barotrauma, `/home/agent/exo-research/barotrauma/`): the airlock is a hull flagged as a
//! wet room between two doors (`content/Items/Assemblies/AutomaticHorizontalAirlock.xml`, and
//! `Hull.IsWetRoom` at `IndoorsSteeringManager.cs:776`); the door's open state is `Door.cs:176-186`.
//! The cycle is ours (`reactor_core/src/airlock.rs` has the sources of the rules). The doors sink
//! into the floor when open, as the engine room's door slides away; the air is in `engine_room.rs`.
//!
//! Host and clients: the host runs the cycle and the air; a client shows the host's doors (the news).
//! The suit rack is the extinguisher racks' kind (`extinguisher.rs`, `extinguisher.json`).
use crate::dev_area::{hull::ShipAir, layout::{AIRLOCK_DOOR_SIZE, DOOR_TIME, INNER_DOOR_AT, OUTER_DOOR_AT}};
use crate::dev_area::*;
use crate::items::{Items, Role};
use item_core::wire::{AirlockAsk, ReactorAsk};
use reactor_core::{Door, Goal};

/// A door slab of the airlock: its shut centre, the vector it sinks along when open, and how far it
/// has sunk (0 shut, 1 open).
#[derive(Component, Clone, Debug)]
pub struct AirlockSlab {
    pub door: Door,
    pub shut: DVec3,
    pub sink: DVec3,
    pub progress: f64,
}

/// The two doors' slabs, spawned with the room (`engine_room::spawn_room`). Like the engine room's
/// door, the collider is the wall's; the slab is a dev box.
pub(super) fn spawn_doors(commands: &mut Commands, frame: &Frame, planet: &PlanetRes, area: &mut DevArea) {
    for (door, at, name) in [(Door::Inner, INNER_DOOR_AT, "engine_airlock_inner_door"), (Door::Outer, OUTER_DOOR_AT, "engine_airlock_outer_door")] {
        let d = AIRLOCK_DOOR_SIZE;
        let (ground, up) = frame.ground(planet, at[0], at[1], 0.0);
        let rot = frame.rotation(up, 0.0);
        let (bottom, top) = crate::dev_area::layout::span(0.0, d[1]);
        let shut = ground + up * ((bottom + top) * 0.5);
        let size = DVec3::new(d[0], top - bottom, d[2]);
        let e = commands
            .spawn((
                RigidBody::Kinematic,
                Collider::cuboid(size.x, size.y, size.z),
                Position(shut),
                Rotation(rot),
                CollisionLayers::new(Layer::World, LayerMask::ALL),
                DevBox { id: name.into(), colour: [0.55, 0.42, 0.2], size },
                AirlockSlab { door, shut, sink: -up * d[1], progress: 0.0 },
            ))
            .id();
        area.entities.push(e);
    }
}

/// Every step: the cycle's air and timer (`ShipAir::airlock_step`), then the slabs follow the
/// doors and the air's gaps follow the slabs. A client's doors are the host's (the news); its air
/// is the same model run on its own.
pub(super) fn airlock_step(time: Res<Time>, items: Res<Items>, mut air: ResMut<ShipAir>, mut slabs: Query<(&mut AirlockSlab, &mut Position)>) {
    let dt = time.delta_secs_f64();
    air.airlock_step(dt);
    let news = items.reactor.map(|r| r.airlock);
    let client = items.role == Role::Client;
    let step = dt / DOOR_TIME;
    for (mut slab, mut pos) in &mut slabs {
        let open = match (client, news) {
            (true, Some(n)) => match slab.door {
                Door::Inner => n.inner_open,
                Door::Outer => n.outer_open,
            },
            _ => air.lock.is_open(slab.door),
        };
        let goal = if open { 1.0 } else { 0.0 };
        slab.progress = if slab.progress < goal { (slab.progress + step).min(goal) } else { (slab.progress - step).max(goal) };
        pos.0 = slab.shut + slab.sink * slab.progress;
        air.set_door(slab.door, slab.progress);
    }
}

/// The host's press from a panel: IN and OUT are the doors (a press closes an open door, else starts
/// the door's cycle), CYCLE is the pump alone. The host refuses what the interlock refuses.
pub fn ask_of(a: AirlockAsk) -> Goal {
    match a {
        AirlockAsk::Inner => Goal::Door(Door::Inner),
        AirlockAsk::Outer => Goal::Door(Door::Outer),
        AirlockAsk::Cycle => Goal::Hold,
    }
}

/// The panel's action names (the reactor screen's buttons) as the wire's asks.
pub fn panel_ask(action: &str) -> Option<ReactorAsk> {
    match action {
        "in" => Some(ReactorAsk::Airlock(AirlockAsk::Inner)),
        "out" => Some(ReactorAsk::Airlock(AirlockAsk::Outer)),
        "cycle" => Some(ReactorAsk::Airlock(AirlockAsk::Cycle)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panels_actions_are_the_three_presses() {
        assert_eq!(panel_ask("in"), Some(ReactorAsk::Airlock(AirlockAsk::Inner)));
        assert_eq!(panel_ask("out"), Some(ReactorAsk::Airlock(AirlockAsk::Outer)));
        assert_eq!(panel_ask("cycle"), Some(ReactorAsk::Airlock(AirlockAsk::Cycle)));
        assert_eq!(panel_ask("in_nope"), None);
        assert_eq!(ask_of(AirlockAsk::Cycle), Goal::Hold);
    }
}
