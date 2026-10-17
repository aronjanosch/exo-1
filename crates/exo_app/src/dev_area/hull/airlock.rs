//! The airlock (r40, #267; one-button cycle #309): the chamber between the equipment room and the
//! outside, its two doors, its two panels and its warning light. The state machine is
//! `reactor_core::Airlock` (the cycle and its rest sides); the air is `ShipAir` (the rooms, the gaps,
//! the pump's moves). Here: the doors' slabs, the air step, the panels' presses, the light.
//!
//! Sources (Barotrauma, `/home/agent/exo-research/barotrauma/`): the airlock is a hull flagged as a
//! wet room between two doors (`content/Items/Assemblies/AutomaticHorizontalAirlock.xml`, and
//! `Hull.IsWetRoom` at `IndoorsSteeringManager.cs:776`); the door's open state is `Door.cs:176-186`.
//! The cycle is ours (`reactor_core/src/airlock.rs` has the sources of the rules). The doors sink
//! into the floor when open, as the engine room's door slides away; the air is `ShipAir` in `hull/air.rs`.
//!
//! Host and clients: the host runs the cycle and the air; a client shows the host's doors (the news).
//! The suit rack in the airlock is gear's (`gear.rs`, `content/gameplay/gear/gear.json`).
use crate::dev_area::{hull::ShipAir, layout::{AIRLOCK, AIRLOCK_DOOR_SIZE, DOOR_TIME, INNER_DOOR_AT, OUTER_DOOR_AT, ROOM_HEIGHT}};
use crate::dev_area::*;
use crate::items::{Items, Role};
use crate::origin::WorldPos;
use item_core::wire::{AirlockAsk, AirlockCycle, ReactorAsk};
use reactor_core::Door;

/// A door slab of the airlock: its shut centre, the vector it sinks along when open, and how far it
/// has sunk (0 shut, 1 open).
#[derive(Component, Clone, Debug)]
pub struct AirlockSlab {
    pub door: Door,
    pub shut: DVec3,
    pub sink: DVec3,
    pub progress: f64,
}

/// The two doors' slabs, spawned by `hull::doors::spawn_doors`. Like the engine room's
/// door, the collider is the wall's; the slab is a dev box.
pub(super) fn spawn_doors(commands: &mut Commands, frame: &Frame, planet: &PlanetRes, area: &mut DevArea) {
    for (door, at, name) in [(Door::Inner, INNER_DOOR_AT, "airlock_inner_door"), (Door::Outer, OUTER_DOOR_AT, "airlock_outer_door")] {
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

/// The host's press from a panel or the inner door: PUSH runs the cycle to the other side, the inner
/// door's F toggles it while the airlock rests inside. False when the interlock ignores it.
pub fn apply(air: &mut ShipAir, a: AirlockAsk) -> bool {
    match a {
        AirlockAsk::Push => air.push(),
        AirlockAsk::ToggleInner => air.toggle_inner(),
    }
}

/// The panel's action names (the airlock screens' PUSH field) as the wire's asks.
pub fn panel_ask(action: &str) -> Option<ReactorAsk> {
    match action {
        "push" => Some(ReactorAsk::Airlock(AirlockAsk::Push)),
        _ => None,
    }
}

/// The warning light's turn (rad/s): about two turns a second, a beacon's rate (`TODO(initiator)`).
const LIGHT_SPIN: f32 = 12.0;
/// The light's height under the ceiling (m).
const LIGHT_HEIGHT: f64 = ROOM_HEIGHT - 0.25;
const LIGHT_INTENSITY: f32 = 40_000.0;

/// The airlock's rotating warning light (#309): an amber box on an arm that spins around the up axis,
/// with a lamp beside it. Dark and hidden unless the cycle runs (the host's news, so a client sees it too).
#[derive(Component)]
pub(super) struct WarningLight {
    base: Quat,
    angle: f32,
}

#[derive(Component)]
pub(super) struct WarningLamp;

pub(super) fn spawn_warning_light(mut commands: Commands, planet: Res<PlanetRes>, area: Res<DevArea>, mut done: Local<Option<warp_core::PlanetId>>, mut made: Local<Vec<Entity>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if *done == Some(planet.id) {
        return;
    }
    let (Some(frame), Some((at, up))) = (area.frame, area.ground(&planet, AIRLOCK.centre, LIGHT_HEIGHT)) else { return };
    *done = Some(planet.id);
    for e in made.drain(..) {
        commands.entity(e).despawn();
    }
    let base = frame.rotation(up, 0.0).as_quat();
    // The arm sits 0.1 m off the axis, so the spin shows; the lamp turns with it.
    let mesh = meshes.add(Cuboid::new(0.16, 0.12, 0.12));
    let mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.05), emissive: LinearRgba::rgb(10.0, 3.0, 0.2), unlit: true, ..default() });
    let e = commands
        .spawn((WorldPos(at), Transform::from_rotation(base), Visibility::Hidden, WarningLight { base, angle: 0.0 }))
        .with_children(|p| {
            p.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(0.1, 0.0, 0.0)));
            p.spawn((PointLight { color: Color::srgb(1.0, 0.55, 0.1), intensity: 0.0, range: 8.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.2, 0.0, 0.0), WarningLamp));
        })
        .id();
    made.push(e);
}

/// Every frame: the light shows and spins while the cycle runs, and is dark otherwise.
pub(super) fn warning_light(time: Res<Time>, items: Res<Items>, mut lights: Query<(&mut WarningLight, &mut Transform, &mut Visibility)>, mut lamps: Query<&mut PointLight, With<WarningLamp>>) {
    let on = items.reactor.is_some_and(|r| r.airlock.cycling != AirlockCycle::Idle);
    for (mut l, mut t, mut v) in &mut lights {
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        if on {
            l.angle = (l.angle + LIGHT_SPIN * time.delta_secs()).rem_euclid(std::f32::consts::TAU);
            t.rotation = l.base * Quat::from_rotation_y(l.angle);
        }
    }
    for mut p in &mut lamps {
        p.intensity = if on { LIGHT_INTENSITY } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panels_one_press_is_the_push() {
        assert_eq!(panel_ask("push"), Some(ReactorAsk::Airlock(AirlockAsk::Push)));
        assert_eq!(panel_ask("cycle"), None, "the old buttons are gone");
    }

    #[test]
    fn the_inner_door_toggles_only_inside_and_never_beside_the_outer() {
        let mut air = ShipAir::at_start(&reactor_core::AirlockTuning::default());
        assert!(!apply(&mut air, AirlockAsk::ToggleInner), "outside the inner door is locked");
        assert!(apply(&mut air, AirlockAsk::Push));
        air.airlock_step(10.0);
        assert!(air.lock.is_open(Door::Inner) && !air.lock.is_open(Door::Outer));
        assert!(apply(&mut air, AirlockAsk::ToggleInner), "inside it is an ordinary door");
        assert!(!air.lock.is_open(Door::Inner));
    }
}
