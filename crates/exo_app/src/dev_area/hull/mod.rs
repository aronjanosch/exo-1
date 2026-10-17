//! The hull and life support: the ship's air, the airlock, the inner doors, the hull's sections, the
//! room alarms and lamps, the vents' valves and the rooms' panels (ER-01, BD-01, the vents, the
//! airlock's AL-01/02, the status monitor ST-01).
use crate::dev_area::*;
use reactor_core::Tuning;

pub mod air;
pub mod airlock;
pub mod asks;
pub mod alarms;
pub mod breach;
pub mod doors;
pub mod life_support;
pub mod lights;
pub mod panels;
pub mod status_monitor;

pub use air::ShipAir;
pub use alarms::ShipAlarm;
pub use doors::{BridgeDoor, EngineDoor, EquipDoor};

pub fn plugin(app: &mut App) {
    let tuning = Tuning::load(&crate::content_files("gameplay")).unwrap_or_else(|e| panic!("hull: {e:?}"));
    app.init_resource::<ShipAlarm>().init_resource::<EngineDoor>().init_resource::<BridgeDoor>().init_resource::<EquipDoor>().insert_resource(ShipAir::at_start(&tuning.airlock));
    app.add_systems(Update, (doors::spawn_doors, breach::spawn_patches).after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_plugins((asks::plugin, panels::plugin));
    app.add_systems(FixedUpdate, (doors::room_state.after(crate::items_net::items_net_in), doors::room_step, doors::bridge_step, doors::equip_step, life_support::ambient_step, airlock::airlock_step, life_support::breath_step).chain().in_set(crate::dev_area::ship::ShipSet::Rooms));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (lights::spawn_lights, (lights::room_lights, lights::alarm_lights, lights::emergency_lamps).after(lights::spawn_lights)).after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, breach::breach_decal.after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, (airlock::spawn_warning_light, airlock::warning_light).chain().after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, (alarms::spawn_alarm_lamps, alarms::alarm_lamps).chain().after(super::spawn_devices).in_set(crate::phases::Frame::World));
}
