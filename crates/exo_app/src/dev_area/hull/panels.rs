//! The hull's screens: the airlock's three PUSH panels (#309). A press asks the host. The door panels ER-01 and BD-01,
//! the vents' panels and the status monitor ST-01 are gone; the doors answer F (`Target::Door`, `Target::BridgeDoor`).
use crate::dev_area::*;
use crate::displays::{DisplayAction, Panels};
use crate::items::Items;
use display_core::Panel;
use item_core::wire::{AirlockCycle, ReactorNews};

use crate::dev_area::hull::airlock;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

/// The airlock's two panels (#309): one inside the chamber (right-hand wall seen from the outer door)
/// and one outside beside the outer door, to call the airlock from outside. Both show and press the same.
pub const AIRLOCK_IN_ID: PanelId = PanelId::new("airlock_panel_in");

pub const AIRLOCK_IN_NAME: &str = "AIRLOCK";

pub const AIRLOCK_OUT_ID: PanelId = PanelId::new("airlock_panel_out");

pub const AIRLOCK_OUT_NAME: &str = "AIRLOCK OUTER";

/// The third panel, in the equipment room beside the inner door (tag AL-SHIP): from the ship it calls
/// the airlock in when it rests outside. TODO(initiator): the name and tag.
pub const AIRLOCK_SHIP_ID: PanelId = PanelId::new("airlock_panel_ship");

pub const AIRLOCK_SHIP_NAME: &str = "AIRLOCK SHIP";

/// An airlock wall panel (#308, #309): the design's screen (`display_core::airlock_panel`). Dead glass while the ship's master is off (like the other
/// screens' OFF); the press goes to the host, which ignores it during a cycle.
pub fn airlock_panel(r: Option<&ReactorNews>) -> Panel {
    use display_core::airlock_panel::{panel, Face};
    let a = r.map(|r| r.airlock).unwrap_or_default();
    let live = r.map_or(true, |r| r.start != item_core::wire::StartStage::Cold);
    panel(if !live {
        Face::Unpowered
    } else if a.cycling == AirlockCycle::Idle {
        Face::Idle { inside: a.inside }
    } else {
        Face::Cycling
    })
}

/// The presses on the airlock's panels.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        if (c.panel == AIRLOCK_IN_ID || c.panel == AIRLOCK_OUT_ID || c.panel == AIRLOCK_SHIP_ID)
            && let Some(a) = airlock::panel_ask(&c.action)
        {
            items.ask_reactor(a);
        }
    }
}

/// The airlock's panels from the snapshot, every tick.
pub fn write_panels(items: Res<Items>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    panels.set(AIRLOCK_IN_ID, airlock_panel(r));
    panels.set(AIRLOCK_OUT_ID, airlock_panel(r));
    panels.set(AIRLOCK_SHIP_ID, airlock_panel(r));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_airlock_panels_have_one_push_and_follow_the_news() {
        use item_core::wire::AirlockNews;
        let mut r = ReactorNews::default();
        for (cycling, inside) in [(AirlockCycle::Idle, true), (AirlockCycle::Idle, false), (AirlockCycle::Outer, true)] {
            r.airlock = AirlockNews { cycling, inside, progress: 0.5, ..Default::default() };
            let p = airlock_panel(Some(&r));
            assert_eq!(p.find("push").is_some(), cycling == AirlockCycle::Idle, "PUSH only at rest");
            assert!(p.find("cycle").is_none() && p.find("in").is_none() && p.find("out").is_none(), "and no other button");
            assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
        }
    }
}
