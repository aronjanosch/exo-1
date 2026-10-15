//! The ship's inner doors: the engine room's sliding door and the bridge door, their state
//! (host-owned, `RoomNews`), the slabs that move and the gaps of the air that open with them.
use crate::dev_area::*;
use crate::dev_area::layout::*;
use crate::dev_area::hull::air::ShipAir;
use crate::dev_area::hull::alarms::ShipAlarm;
use crate::items::{Items, NetOut, Role};
use item_core::wire::{RoomAsk, RoomNews};

/// The bridge door's state (r44): open, as the engine room's `EngineDoor`.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct BridgeDoor {
    pub open: bool,
}


#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct EngineDoor {
    pub open: bool,
}

/// A door slab: its shut centre, the slide's direction (east, m) and how far it has slid (0..1).
#[derive(Component, Clone, Debug)]
pub struct Slab {
    pub shut: DVec3,
    pub slide: DVec3,
    pub progress: f64,
}

/// The bridge door's slab (r44): its shut centre, the vector it sinks along when open, and how far it has
/// sunk (0 shut, 1 open). A separate component so the engine door's walker pick does not take it.
#[derive(Component, Clone, Debug)]
pub struct BridgeSlab {
    pub shut: DVec3,
    pub sink: DVec3,
    pub progress: f64,
}

pub fn room_state(mut items: ResMut<Items>, mut alarm: ResMut<ShipAlarm>, mut door: ResMut<EngineDoor>, mut bridge: ResMut<BridgeDoor>) {
    let asks = std::mem::take(&mut items.room_asks);
    match items.role {
        Role::Client => {
            for a in asks {
                items.net_out.push(NetOut::Ask(a));
            }
            door.open = items.room.door_open;
            bridge.open = items.room.bridge_door;
            alarm.level = items.room.alarm;
        }
        Role::Host | Role::Local => {
            for a in asks {
                match a {
                    RoomAsk::ToggleDoor => door.open = !door.open,
                    // The captain's CP-01 (r41): the press sets the door, as a signal sets Barotrauma's door.
                    RoomAsk::SetDoor(open) => door.open = open,
                    RoomAsk::CycleAlarm => alarm.level = alarm.level.next(),
                    // The bridge door (r44): its panel toggles it, CP-01 sets it.
                    RoomAsk::ToggleBridgeDoor => bridge.open = !bridge.open,
                    RoomAsk::SetBridgeDoor(open) => bridge.open = open,
                }
            }
            let now = RoomNews { door_open: door.open, alarm: alarm.level, bridge_door: bridge.open };
            if now != items.room {
                items.room = now;
                if items.role == Role::Host {
                    items.net_out.push(NetOut::Room { to: None, r: now });
                }
            }
        }
    }
}

pub fn room_step(time: Res<Time>, door: Res<EngineDoor>, mut air: ResMut<ShipAir>, mut slabs: Query<(&mut Slab, &mut Position)>) {
    let dt = time.delta_secs_f64();
    let goal = if door.open { 1.0 } else { 0.0 };
    let step = dt / DOOR_TIME;
    for (mut slab, mut pos) in &mut slabs {
        slab.progress = if slab.progress < goal { (slab.progress + step).min(goal) } else { (slab.progress - step).max(goal) };
        pos.0 = slab.shut + slab.slide * slab.progress;
        let door = air.door;
        air.deck.set_open(door, slab.progress);
    }
}

/// Every step (r44): the bridge door's slab follows the door's state and sinks into the floor, and its
/// gap to the corridor opens by the slab's progress, as the engine door's does.
pub fn bridge_step(time: Res<Time>, bridge: Res<BridgeDoor>, mut air: ResMut<ShipAir>, mut slabs: Query<(&mut BridgeSlab, &mut Position)>) {
    let dt = time.delta_secs_f64();
    let goal = if bridge.open { 1.0 } else { 0.0 };
    let step = dt / DOOR_TIME;
    for (mut slab, mut pos) in &mut slabs {
        slab.progress = if slab.progress < goal { (slab.progress + step).min(goal) } else { (slab.progress - step).max(goal) };
        pos.0 = slab.shut + slab.sink * slab.progress;
        let gap = air.bridge_door;
        air.deck.set_open(gap, slab.progress);
    }
}

/// Once per planet: the engine door's and the bridge door's slabs, and the airlock's doors.
pub(super) fn spawn_doors(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, door: Res<EngineDoor>, bridge: Res<BridgeDoor>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    let d = DOOR_SIZE;
    let (ground, up) = frame.ground(&planet, DOOR_AT[0], DOOR_AT[1], 0.0);
    let rot = frame.rotation(up, 0.0);
    let (bottom, top) = span(0.0, d[1]);
    let shut = ground + up * ((bottom + top) * 0.5);
    let size = DVec3::new(d[0], top - bottom, d[2]);
    let progress = if door.open { 1.0 } else { 0.0 };
    let e = commands
        .spawn((
            RigidBody::Kinematic,
            Collider::cuboid(size.x, size.y, size.z),
            Position(shut + frame.east * (DOOR_SLIDE * progress)),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id: DOOR.into(), colour: [0.55, 0.42, 0.2], size },
            Slab { shut, slide: frame.east * DOOR_SLIDE, progress },
        ))
        .id();
    area.entities.push(e);
    // The bridge door (r44): a slab in the corridor's east wall's opening, as the engine door's.
    let d = BRIDGE_DOOR_SIZE;
    let (ground, up) = frame.ground(&planet, BRIDGE_DOOR_AT[0], BRIDGE_DOOR_AT[1], 0.0);
    let rot = frame.rotation(up, 0.0);
    let (bottom, top) = span(0.0, d[1]);
    let shut = ground + up * ((bottom + top) * 0.5);
    let size = DVec3::new(d[0], top - bottom, d[2]);
    let progress = if bridge.open { 1.0 } else { 0.0 };
    let sink = -up * d[1];
    let e = commands
        .spawn((
            RigidBody::Kinematic,
            Collider::cuboid(size.x, size.y, size.z),
            Position(shut + sink * progress),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id: BRIDGE_DOOR.into(), colour: [0.55, 0.42, 0.2], size },
            BridgeSlab { shut, sink, progress },
        ))
        .id();
    area.entities.push(e);
    crate::dev_area::hull::airlock::spawn_doors(&mut commands, &frame, &planet, &mut area);
}
