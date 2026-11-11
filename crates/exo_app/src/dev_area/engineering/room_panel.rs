//! The engine room's wall panel (#313, the airlock panels' kind, #308): beside the door on the room's north wall.
//! One field, LIGHTS ON / OFF: F on the panel and the cursor on it ask the host (`ReactorAsk::SetEngineLamps`, the host owns the
//! switch, a client sees it in the news). Below it two read-only rows: HEAT and AIR. Dark with a dim NO POWER while
//! JB-1 passes no power (or the ship's master is off); the host ignores the field then.
use crate::dev_area::*;
use crate::displays::{DisplayAction, Panels};
use crate::items::Items;
use display_core::engine_panel::{panel, Air, Face, Heat, LIGHTS};
use display_core::{Panel, PanelId};
use item_core::wire::{ReactorAsk, ReactorNews, StartStage};

pub const ENGINE_PANEL_ID: PanelId = PanelId::new("engine_room_panel");
pub const ENGINE_PANEL_NAME: &str = "ENGINE ROOM";

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}

/// The face for the news; no news yet reads as powered with nothing to say (the airlock panel does the same).
pub fn engine_panel(r: Option<&ReactorNews>) -> Panel {
    let Some(n) = r else {
        return panel(Face::Live { lights_on: true, heat: Heat::Ok, heat_fill: 0.0, air: Air::Ok, air_fill: 1.0 });
    };
    if n.start == StartStage::Cold || !super::junction_boxes::box_passes(Some(n), 0) {
        return panel(Face::Unpowered);
    }
    panel(Face::Live {
        lights_on: !n.engine_lamps_off,
        heat: Heat::of(n.heat, n.optimal[1], n.allowed[1]),
        heat_fill: n.heat / 100.0,
        air: Air::of(n.oxygen),
        air_fill: n.oxygen,
    })
}

/// The panel's one action as the wire's ask: the field shows the state, a press asks for the other one.
pub fn lamps_ask(action: &str, r: Option<&ReactorNews>) -> Option<ReactorAsk> {
    (action == LIGHTS).then(|| ReactorAsk::SetEngineLamps(r.is_some_and(|n| n.engine_lamps_off)))
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        if c.panel == ENGINE_PANEL_ID
            && let Some(a) = lamps_ask(&c.action, items.reactor.as_ref())
        {
            items.ask_reactor(a);
        }
    }
}

pub fn write_panel(items: Res<Items>, mut panels: ResMut<Panels>) {
    panels.set(ENGINE_PANEL_ID, engine_panel(items.reactor.as_ref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn news() -> ReactorNews {
        let mut n = ReactorNews { start: StartStage::Running, oxygen: 1.0, optimal: [40.0, 60.0], allowed: [30.0, 80.0], heat: 50.0, ..Default::default() };
        n.junctions[0].working = true;
        n.breakers = [true; item_core::wire::BREAKER_GROUPS];
        n
    }

    #[test]
    fn the_field_asks_for_the_other_state() {
        let mut n = news();
        assert_eq!(lamps_ask(LIGHTS, Some(&n)), Some(ReactorAsk::SetEngineLamps(false)), "lit: a press switches off");
        n.engine_lamps_off = true;
        assert_eq!(lamps_ask(LIGHTS, Some(&n)), Some(ReactorAsk::SetEngineLamps(true)), "dark by hand: a press switches on");
        assert_eq!(lamps_ask("push", Some(&n)), None);
    }

    #[test]
    fn a_dead_jb1_shows_no_power_and_has_no_field() {
        let mut n = news();
        assert!(engine_panel(Some(&n)).find(LIGHTS).is_some());
        n.junctions[0].working = false;
        assert!(engine_panel(Some(&n)).find(LIGHTS).is_none(), "NO POWER, nothing to press");
        let mut cold = news();
        cold.start = StartStage::Cold;
        assert!(engine_panel(Some(&cold)).find(LIGHTS).is_none());
    }

    #[test]
    fn the_panel_follows_the_news() {
        let n = news();
        let base = engine_panel(Some(&n));
        for change in [|n: &mut ReactorNews| n.engine_lamps_off = true, |n: &mut ReactorNews| n.heat = 70.0, |n: &mut ReactorNews| n.heat = 95.0, |n: &mut ReactorNews| n.oxygen = 0.4, |n: &mut ReactorNews| n.oxygen = 0.0] {
            let mut m = n;
            change(&mut m);
            let p = engine_panel(Some(&m));
            assert_ne!(p, base);
            assert!(p.overflowing().is_empty());
        }
    }
}
