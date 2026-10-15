//! SP-01, the ship's power panel (r42, #268): the cold start as a checklist of six steps, with the
//! MAIN BREAKER, BOOT and OVERRIDE. The steps, their order and their sources (PULSAR's six-step
//! startup, the manual's "Ship Startup Sequence") are in `reactor_core/src/startup.rs`. SP-01 is
//! the one screen that stays on while the master is off: the others show OFF (`crate::dev_area::screens::off`).
use crate::dev_area::screens::tall;
use crate::displays::DisplayAction;
use display_core::{button, column, font, micro, readout, row, space, toggle, Panel, PanelId, Tier, Tone};
use item_core::wire::{BreakerGroup, ReactorAsk, ReactorNews, StartupStage};

pub const ID: PanelId = PanelId::new("ship_power");
pub const NAME: &str = "SHIP POWER";
pub const TAG: &str = "SP-01";
pub const MASTER: &str = "sp_master";
pub const BOOT: &str = "sp_boot";
pub const OVERRIDE: &str = "sp_override";

/// The six steps as the checklist shows them, in order (`reactor_core::startup`).
const STEPS: [&str; 6] = ["MAIN BREAKER ON", "BOOT SYSTEMS", "REACTOR POWER ON", "ARM JUMP DRIVE", "LIFE SUPPORT, LIGHTS ON", "OVERRIDE"];

pub fn panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let on = r.startup != StartupStage::Cold;
    let current = r.startup_ticks.iter().position(|done| !done);
    let mut lines = vec![micro("STARTUP").fixed(14.0)];
    for (i, text) in STEPS.iter().enumerate() {
        let mark = if r.startup_ticks[i] { "[X]" } else { "[ ]" };
        let pointer = if Some(i) == current { "> " } else { "  " };
        let tone = if r.startup_ticks[i] {
            Tone::Good
        } else if Some(i) == current {
            Tone::Normal
        } else {
            Tone::Dim
        };
        lines.push(readout(format!("{pointer}{mark} {text}")).text_size(font::SMALL).tone(tone).fixed(18.0));
    }
    let (word, word_tone) = match r.startup {
        StartupStage::Cold => ("COLD", Tone::Dim),
        StartupStage::Booting => ("BOOTING", Tone::Warn),
        StartupStage::Ready => ("READY", Tone::Good),
        _ => ("STANDBY", Tone::Normal),
    };
    let status = readout(word).text_size(font::SMALL).tone(word_tone).fixed(20.0);
    let steps = column(3.0, lines).fixed(150.0);
    let master = toggle(on, "", MASTER).labels("OFF", "ON").fixed(28.0);
    let boot = button("BOOT", BOOT).enabled(r.startup == StartupStage::Breaker).flex(1.0);
    let powered = r.breakers[BreakerGroup::LifeSupport.index()] && r.breakers[BreakerGroup::Lights.index()];
    let over = button("OVERRIDE", OVERRIDE).enabled(r.startup == StartupStage::Breakers && powered).flex(1.0);
    let presses = row(8.0, vec![boot, over]).fixed(28.0);
    let body = column(10.0, vec![space().flex(1.0), status, steps, master, presses, space().flex(1.0)]);
    tall(Panel::screen(Tier::S, NAME, TAG, body)).with_padding(6.0)
}

/// The SP-01 presses: the breaker is a switch (its value when the host sends one, else the flip),
/// BOOT and OVERRIDE are asks the host refuses when out of order.
pub fn ask(c: &DisplayAction, r: Option<&ReactorNews>) -> Option<ReactorAsk> {
    let current = r.map_or(false, |r| r.startup != StartupStage::Cold);
    match c.action.as_str() {
        MASTER => Some(ReactorAsk::MasterBreaker(c.value.map(|v| v >= 0.5).unwrap_or(!current))),
        BOOT => Some(ReactorAsk::Boot),
        OVERRIDE => Some(ReactorAsk::Override),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_breaker_flips_and_boot_and_override_are_asks() {
        let click = |action: &str, value: Option<f32>| DisplayAction { panel: ID, action: action.to_string(), value };
        let cold = ReactorNews { startup: StartupStage::Cold, ..ReactorNews::default() };
        assert_eq!(ask(&click(MASTER, None), Some(&cold)), Some(ReactorAsk::MasterBreaker(true)));
        assert_eq!(ask(&click(MASTER, None), Some(&ReactorNews::default())), Some(ReactorAsk::MasterBreaker(false)));
        assert_eq!(ask(&click(BOOT, None), None), Some(ReactorAsk::Boot));
        assert_eq!(ask(&click(OVERRIDE, None), None), Some(ReactorAsk::Override));
        assert_eq!(ask(&click("nope", None), None), None);
    }
}
