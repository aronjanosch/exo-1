//! Engineering's screen glue: the reactor's main and supply screens (`reactor_room`, #317/#318) read the host's
//! snapshot; a press on the main screen asks the host. The old wall screens (pump, junction box, AUX, battery)
//! are gone.
use crate::dev_area::*;
use crate::displays::{DisplayAction, Panels};
use crate::dev_area::ship::ReactorConfig;
use crate::items::Items;
use item_core::wire::{BotStation, ReactorAsk};

use crate::dev_area::crew::{bot_tagged, manned};
use crate::dev_area::engineering::reactor_room::{MAIN_ID, SUPPLY_ID};

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

/// The ask of a press on the reactor's main screen. The sliders send the setting; the host takes it in MANUAL only
/// (`asks::apply`), and the panel has them disabled in AUTO, so no press arrives.
pub fn main_ask(action: &str, value: Option<f32>) -> Option<ReactorAsk> {
    use display_core::reactor_main as m;
    match action {
        m::AUTO => Some(ReactorAsk::SetManual(false)),
        m::MANUAL => Some(ReactorAsk::SetManual(true)),
        m::INJECTION => value.map(|v| ReactorAsk::SetInjection(v as f64 * 100.0)),
        m::CONVERTER => value.map(|v| ReactorAsk::SetConverter(v as f64 * 100.0)),
        _ => None,
    }
}

/// The presses on the reactor's main screen become the host's asks.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        if c.panel == MAIN_ID
            && let Some(a) = main_ask(&c.action, c.value)
        {
            items.ask_reactor(a);
        }
    }
}

/// The reactor's panels from the snapshot, every tick (`Panels::set` skips an unchanged one).
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    let max_kw = cfg.0.reactor.max_output_kw;
    // The main screen draws its own dark and word faces (NO POWER, ...). The BOT tag of a manned station goes on its plate.
    let run_up = r.is_some_and(|r| r.start == item_core::wire::StartStage::RunUp);
    panels.set(MAIN_ID, bot_tagged(display_core::reactor_main::panel(r, max_kw, run_up), manned(&items, BotStation::Reactor)));
    // The supply screen goes dark by the main screen's rule (`reactor_main::face`), inside its panel.
    panels.set(SUPPLY_ID, display_core::reactor_supply::panel(r));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The main screen's sliders move the sim's set points in MANUAL; AUTO ignores them (#317).
    #[test]
    fn the_main_screens_sliders_move_the_set_points_in_manual_and_auto_ignores_them() {
        use crate::dev_area::ship::ReactorSim;
        let cfg = reactor_core::Tuning::default();
        let mut s = ReactorSim::new(&cfg);
        let press = |s: &mut ReactorSim, action: &str, v: Option<f32>| {
            if let Some(a) = main_ask(action, v) {
                crate::dev_area::engineering::asks::apply(s, a);
            }
        };
        press(&mut s, display_core::reactor_main::MANUAL, None);
        assert!(s.manual);
        press(&mut s, display_core::reactor_main::INJECTION, Some(0.7));
        press(&mut s, display_core::reactor_main::CONVERTER, Some(0.25));
        assert!((s.injection - 70.0).abs() < 1e-4 && (s.converter - 25.0).abs() < 1e-4, "{} {}", s.injection, s.converter);
        press(&mut s, display_core::reactor_main::AUTO, None);
        assert!(!s.manual);
        press(&mut s, display_core::reactor_main::INJECTION, Some(0.1));
        press(&mut s, display_core::reactor_main::CONVERTER, Some(0.9));
        assert!((s.injection - 70.0).abs() < 1e-4 && (s.converter - 25.0).abs() < 1e-4, "AUTO ignores the sliders");
    }
}
