//! The reactor's start-up stand on the host (#319): the kit's toggles and START lever feed the sim (`reactor_core::Stand`), the sim's
//! conditions refuse the lever's steps, and a SCRAM or an eject drops the stand. The procedure and its stages are in
//! `reactor_core/src/startup.rs`; this file is the glue.
//!
//! Order of a tick: `sync_stand` (asks) shows the stand to the room and sets the lever's refusals; the room steps; `settle_lever`
//! (react) drops a lever the stage did not take (PRIME lost, the core gone); `drop_stand` (after `react`) answers `EmergencyStop`.
use super::reactor_room::{react, host_presses, EmergencyStop, ReactorRoom, LEVER_START, TOGGLES};
use crate::controls_kit::Machine;
use crate::dev_area::ship::{ReactorSim, ShipSet};
use crate::items::{Items, Role};
use bevy::prelude::*;
use item_core::wire::{ReactorNews, StartStage};
use reactor_core::{Stand, StartStage as Core};

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, sync_stand.after(host_presses).in_set(ShipSet::Asks));
    app.add_systems(FixedUpdate, settle_lever.in_set(ShipSet::React));
    app.add_systems(FixedUpdate, drop_stand.after(react).in_set(crate::phases::Fx::Walker));
    app.add_systems(FixedUpdate, dev_cold_start.in_set(crate::phases::Fx::Effects));
}

/// The stand as the kit holds it.
pub fn stand_of(m: &Machine) -> Stand {
    Stand { toggles: TOGGLES.map(|t| m.controls.is_on(t)), lever: m.controls.lever_index(LEVER_START).unwrap_or(0) }
}

/// Host: the room sees the stand; the lever refuses the detents the sim does not allow now (PRIME without its conditions, RUN before the
/// trap is charged).
fn sync_stand(items: Res<Items>, mut sim: ResMut<ReactorSim>, mut machines: Query<&mut Machine, With<ReactorRoom>>) {
    if items.role == Role::Client {
        return;
    }
    let Ok(mut m) = machines.single_mut() else { return };
    sim.room.set_stand(stand_of(&m));
    for detent in 0..3u8 {
        if !sim.room.lever_allows(detent) {
            m.refused.insert((LEVER_START.into(), detent));
        }
    }
}

/// Host: after the room's step, a lever the stage did not take falls back to OFF (PRIME lost, the core gone, a lever left at RUN).
fn settle_lever(items: Res<Items>, sim: Res<ReactorSim>, mut machines: Query<&mut Machine, With<ReactorRoom>>) {
    if items.role == Role::Client {
        return;
    }
    let Ok(mut m) = machines.single_mut() else { return };
    if matches!(sim.room.start().stage(), Core::Cold | Core::Boot) && m.controls.lever_index(LEVER_START).is_some_and(|i| i > 0) {
        m.controls.drop_to_off(LEVER_START);
    }
}

/// Host: a SCRAM or an eject drops the START lever and the three boot toggles (the screens stay awake on battery).
fn drop_stand(mut stops: MessageReader<EmergencyStop>, mut machines: Query<&mut Machine>) {
    for s in stops.read() {
        let Ok(mut m) = machines.get_mut(s.machine) else { continue };
        m.controls.drop_to_off(LEVER_START);
        for t in TOGGLES {
            m.controls.drop_to_off(t);
        }
    }
}

/// F10 'Reactor: cold start': the reactor off and dark, toggles off, lever at OFF, to test the procedure. The battery keeps its charge.
fn dev_cold_start(mut menu: ResMut<crate::dev::DevMenu>, items: Res<Items>, mut sim: ResMut<ReactorSim>, mut machines: Query<&mut Machine, With<ReactorRoom>>) {
    if !std::mem::take(&mut menu.cold_reactor) {
        return;
    }
    if items.role == Role::Client {
        return;
    }
    sim.room.cold_reactor();
    if let Ok(mut m) = machines.single_mut() {
        m.controls.drop_to_off(LEVER_START);
        for t in TOGGLES {
            m.controls.drop_to_off(t);
        }
    }
    menu.last = Some("reactor: cold start (dark, toggles off)".into());
}

/// Prime lamp `i` (0 is the bottom one) is lit: the trap's charge reaches its segment, and stays lit while the reactor runs.
pub fn prime_lamp_on(n: &ReactorNews, i: usize) -> bool {
    n.trap * 5.0 > i as f64 + 1e-9 && n.start != StartStage::Cold
}

/// The rising whine of the trap while it charges: its pitch factor (1 to 2), None when it does not sound. The sound system reads it.
pub fn whine_pitch(n: &ReactorNews) -> Option<f32> {
    (n.start == StartStage::Prime && n.trap < 1.0).then(|| 1.0 + n.trap.clamp(0.0, 1.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prime_lamps_light_one_after_another_from_the_bottom() {
        let n = |trap: f64| ReactorNews { start: StartStage::Prime, trap, ..default() };
        let lit = |t: f64| (0..5).filter(|i| prime_lamp_on(&n(t), *i)).count();
        assert_eq!([lit(0.0), lit(0.1), lit(0.45), lit(0.95), lit(1.0)], [0, 1, 3, 5, 5]);
        assert!(prime_lamp_on(&n(0.5), 0) && !prime_lamp_on(&n(0.5), 3));
        assert_eq!(whine_pitch(&n(0.5)), Some(1.5));
        assert_eq!(whine_pitch(&n(1.0)), None);
    }
}
