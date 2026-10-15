//! The dev faults screen's asks on the host (DEV-02): each fault, the reset, the director's toggle,
//! the bots' count and the ambush. Dev tools poke every owner, through the owners' own calls.
use crate::dev_area::*;
use crate::dev_area::fire::FireConfig;
use crate::dev_area::hull::ShipAir;
use crate::dev_area::ship::{fault, host_asks, route, ReactorConfig, ReactorCounts, ReactorSim, ShipEvents, ShipSet};
use crate::dev_area::screens::show;
use crate::dev_area::turrets::panels::ambush_running;
use crate::displays::{DisplayAction, Panels};
use crate::items::Items;
use display_core::{button, column, row, PanelId, Panel, Tier};
use item_core::wire::ReactorNews;
use item_core::wire::{ReactorAsk, ReactorFault};
use reactor_core::{RoomEvent, Source, Tuning};

/// The dev fault SHIELD HIT's hit, points (`EngineRoom::absorb_hit`). TODO(initiator): a dev value.
const DEV_SHIELD_HIT: f64 = 300.0;

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "dev_faults_console".into(),
        zone: DEV,
        local_pos: FAULTS_AT,
        size: [1.2, 1.0, 0.6],
        turn_deg: 0.0,
        colour: [0.32, 0.36, 0.42],
        panel: Some(PanelSpec { id: FAULTS_ID, name: FAULTS_NAME.into(), size_m: [0.48, 0.36], at: [0.0, 1.3, 0.1], tilt_deg: 15.0 }),
    });
    app.add_systems(FixedUpdate, asks.in_set(ShipSet::Asks));
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}
pub const FAULTS_ID: PanelId = PanelId::new("reactor_faults");

pub const FAULTS_NAME: &str = "Dev faults";

/// Its console in the dev zone (m from the zone's centre), facing south.
pub const FAULTS_AT: [f64; 2] = [-3.0, -2.5];

pub fn faults_panel(r: Option<&ReactorNews>) -> Panel {
    let aux = if r.is_some_and(|r| r.aux_on) { "AUX ON" } else { "AUX OFF" };
    let director = if r.is_some_and(|r| r.director) { "DIRECTOR ON" } else { "DIRECTOR OFF" };
    let ambush = if r.is_some_and(ambush_running) { "AMBUSH ON" } else { "AMBUSH OFF" };
    let bots = r.map_or(0, |r| r.bot_count);
    let col = |items: Vec<_>| column(4.0, items);
    Panel::screen(
        Tier::S,
        FAULTS_NAME,
        "DEV-02",
        row(
            6.0,
            vec![
                col(vec![
                    button("Pump break", "pump_break").fixed(24.0),
                    button("Load drop", "load_drop").fixed(24.0),
                    button("Surge", "surge").fixed(24.0),
                    button("Hull breach", "hull_breach").fixed(24.0),
                    button("Hit light", "hit_light").fixed(24.0),
                    button("Hit heavy", "hit_heavy").fixed(24.0),
                    button("Drain tank", "drain_coolant").fixed(24.0),
                    button("O2 gen break", "oxygen_break").fixed(24.0),
                ]),
                col(vec![
                    button("Start fire", "fire").fixed(24.0),
                    button("Fire mode", "fire_model").fixed(24.0),
                    button("Warm start", "warm_start").fixed(24.0),
                    button(aux, "aux").fixed(24.0),
                    button("Reset RX", "reset").fixed(24.0),
                    button(director, "director").fixed(24.0),
                    button("Shield hit", "shield_hit").fixed(24.0),
                    button("Jump full", "jump_full").fixed(24.0),
                ]),
                col(vec![button(ambush, "ambush").fixed(24.0), button("COLD SHIP", "cold_ship").fixed(24.0), button(format!("BOTS {bots}"), "bots").fixed(24.0)]),
            ],
        ),
    )
}


fn is_mine(a: &ReactorAsk) -> bool {
    matches!(a, ReactorAsk::Trigger(_) | ReactorAsk::Reset | ReactorAsk::SetDirector(_) | ReactorAsk::SetBots(_) | ReactorAsk::Ambush(_))
}

fn asks(mut items: ResMut<Items>, cfg: Res<ReactorConfig>, fires: Res<FireConfig>, mut sim: ResMut<ReactorSim>, mut air: ResMut<ShipAir>, mut counts: ResMut<ReactorCounts>, mut events: ResMut<ShipEvents>) {
    for a in host_asks(&mut items, is_mine) {
        match a {
            // The fire starts in the air (the fire step), so it goes out as the room's event.
            ReactorAsk::Trigger(ReactorFault::Fire) => {
                info!("dev: fire");
                events.0.push(RoomEvent::Fire { size: fires.0.start_size, source: Source::Reactor });
            }
            // The fires are the air's: a fresh room starts with a fresh air (no fire survives).
            ReactorAsk::Reset => {
                *air = ShipAir::with_tuning(&cfg.0.airlock);
                apply_sim(&mut sim, a, &cfg.0, &mut counts);
            }
            a => {
                apply_sim(&mut sim, a, &cfg.0, &mut counts);
            }
        }
    }
}

/// A dev ask on the room; false for an ask that is not a dev one.
pub fn apply_sim(s: &mut ReactorSim, a: ReactorAsk, cfg: &Tuning, counts: &mut ReactorCounts) -> bool {
    match a {
        ReactorAsk::Trigger(ReactorFault::DrainCoolant) => {
            s.room.pump_mut().set_tank(0.0);
            info!("dev: drain of the coolant tank");
        }
        ReactorAsk::Trigger(ReactorFault::WarmStart) => {
            let secs = s.room.warm_start();
            info!("dev: warm start, settled after {secs:.1} s of sim time");
        }
        ReactorAsk::Trigger(ReactorFault::ShieldHit) => {
            let hull = s.room.absorb_hit(DEV_SHIELD_HIT);
            info!("dev: shield hit of {DEV_SHIELD_HIT} points, {hull:.1} past the shield");
        }
        ReactorAsk::Trigger(ReactorFault::JumpFull) => {
            s.room.jump_mut().fill();
            info!("dev: jump drive full");
        }
        ReactorAsk::Trigger(ReactorFault::ColdShip) => {
            s.room.cold_ship();
            info!("dev: COLD SHIP");
        }
        ReactorAsk::Trigger(ReactorFault::Aux) => {
            let on = !s.room.aux_enabled();
            s.room.set_aux(on);
            info!("dev: aux reactor {}", if on { "on" } else { "off" });
        }
        ReactorAsk::Trigger(f) => {
            if let Some(f) = fault(f) {
                let events = s.room.trigger(f);
                info!("dev: fault {f:?}");
                for e in events {
                    route(e, counts);
                }
            }
        }
        ReactorAsk::Reset => {
            s.reset(cfg);
            info!("dev: reset");
        }
        ReactorAsk::SetDirector(on) => {
            s.director.set_enabled(on);
            info!("dev: director {}", if on { "on" } else { "off" });
        }
        // The dev BOTS count (r42).
        ReactorAsk::SetBots(n) => s.bots.set_count(n),
        // The dev AMBUSH (r41): a new encounter, refused while one runs; off ends it.
        ReactorAsk::Ambush(true) => {
            if !s.room.start_ambush() {
                info!("dev: ambush refused (an encounter runs)");
            }
        }
        ReactorAsk::Ambush(false) => s.room.stop_ambush(),
        _ => return false,
    }
    true
}

/// The presses on DEV-02 become dev asks; "Fire mode" flips the fire's model here (it is the fire's
/// state, no ask: the host and each process keep it).
fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>, mut model: ResMut<crate::dev_area::fire::FireModel>) {
    for c in clicks.read() {
        if c.panel != FAULTS_ID {
            continue;
        }
        let snap = items.reactor;
        let ask = match c.action.as_str() {
            "pump_break" => Some(ReactorAsk::Trigger(ReactorFault::PumpBreak)),
            "oxygen_break" => Some(ReactorAsk::Trigger(ReactorFault::OxygenGeneratorBreak)),
            "load_drop" => Some(ReactorAsk::Trigger(ReactorFault::LoadDrop)),
            "surge" => Some(ReactorAsk::Trigger(ReactorFault::Surge)),
            "hull_breach" => Some(ReactorAsk::Trigger(ReactorFault::HullBreach)),
            "shield_hit" => Some(ReactorAsk::Trigger(ReactorFault::ShieldHit)),
            "hit_light" => Some(ReactorAsk::Trigger(ReactorFault::HitLight)),
            "hit_heavy" => Some(ReactorAsk::Trigger(ReactorFault::HitHeavy)),
            "fire" => Some(ReactorAsk::Trigger(ReactorFault::Fire)),
            "reset" => Some(ReactorAsk::Reset),
            "drain_coolant" => Some(ReactorAsk::Trigger(ReactorFault::DrainCoolant)),
            "warm_start" => Some(ReactorAsk::Trigger(ReactorFault::WarmStart)),
            "cold_ship" => Some(ReactorAsk::Trigger(ReactorFault::ColdShip)),
            "aux" => Some(ReactorAsk::Trigger(ReactorFault::Aux)),
            "director" => snap.map(|r| ReactorAsk::SetDirector(!r.director)),
            "jump_full" => Some(ReactorAsk::Trigger(ReactorFault::JumpFull)),
            "ambush" => snap.map(|r| ReactorAsk::Ambush(!ambush_running(&r))),
            // The dev BOTS count, 0..3 in turn (r42, #253).
            "bots" => snap.map(|r| ReactorAsk::SetBots((r.bot_count + 1) % (reactor_core::bots::MAX_BOTS + 1))),
            "fire_model" => {
                *model = model.toggled();
                None
            }
            _ => None,
        };
        if let Some(a) = ask {
            items.ask_reactor(a);
        }
    }
}

fn write_panel(items: Res<Items>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    show(&mut panels, r, FAULTS_ID, faults_panel(r));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_faults_screen_has_its_buttons() {
        let p = faults_panel(None);
        for a in ["reset", "pump_break", "oxygen_break", "director", "fire_model"] {
            assert!(p.find(a).is_some(), "{a}");
        }
    }
}
