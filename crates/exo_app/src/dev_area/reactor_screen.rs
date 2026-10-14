//! The reactor's placeholder readout (#297, slice S3b): an M screen on the engine room's east
//! wall (`engine_reactor_screen`) and a small dev fault screen next to the test console. Plain
//! text rows; the designed screen replaces them (TODO(initiator)).
//!
//! Every process shows the snapshot it has (`Items::reactor`). The buttons ask the host: manual or
//! Auto, fission and turbine by 5 (manual only), the three faults and the reset.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::items::Items;
use display_core::{bar, button, column, ctl, micro, readout, row, space, Item, Panel, Tier, Tone};
use item_core::wire::{Alarm, ReactorAsk, ReactorFault, ReactorNews};

pub const READOUT_ID: PanelId = PanelId::new("reactor_readout");
pub const READOUT_NAME: &str = "Reactor";
pub const FAULTS_ID: PanelId = PanelId::new("reactor_faults");
pub const FAULTS_NAME: &str = "Dev faults";
/// The dev fault screen's place next to the test console (zone local metres, as `PlaceholderBox`).
pub const FAULTS_AT: [f64; 2] = [1.9, -4.5];
/// How far a +/- press moves a target (0..100).
pub const STEP: f64 = 5.0;

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "dev_faults".into(),
        zone: LAB,
        local_pos: FAULTS_AT,
        size: [0.6, 1.0, 0.6],
        turn_deg: 0.0,
        colour: [0.32, 0.36, 0.42],
        panel: Some(PanelSpec { id: FAULTS_ID, name: FAULTS_NAME.into(), size_m: [0.48, 0.36], at: [0.0, 1.4, 0.2], tilt_deg: 15.0 }),
    });
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

/// One reading as a label and its value, in a row that shares the width.
fn cell(label: &str, value: String) -> Item {
    row(8.0, vec![micro(label).fixed(96.0), readout(value)])
}

/// `ON`/`OFF`-style words for a part.
fn works(share: f64, working: bool) -> String {
    let word = if working { "WORKS" } else { "BROKEN" };
    format!("{:.0}% {word}", share * 100.0)
}

/// The readout from the snapshot (none yet: dashes).
pub fn readout_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied();
    let manual = r.is_some_and(|r| r.manual);
    let temp = r.map(|r| r.temp).unwrap_or(0.0);
    let optimal = r.map(|r| r.optimal).unwrap_or([0.0, 0.0]);
    let allowed = r.map(|r| r.allowed).unwrap_or([0.0, 0.0]);
    let set = |f: fn(&ReactorNews) -> f64| r.map(|r| f(&r)).unwrap_or(0.0);
    let on = |p: fn(&ReactorNews) -> bool| r.map(|r| p(&r)).unwrap_or(false);
    Panel::screen(
        Tier::M,
        READOUT_NAME,
        "RX-01",
        column(
            6.0,
            vec![
                bar(temp / 100.0, format!("Temp {temp:.1}  optimal {:.0}-{:.0}  allowed {:.0}-{:.0}", optimal[0], optimal[1], allowed[0], allowed[1]))
                    .thresholds(Some(optimal[1] / 100.0), Some(allowed[1] / 100.0))
                    .fixed(ctl::BAR),
                row(12.0, vec![cell("Output", format!("{:.0} kW", set(|r| r.output_kw))), cell("Load", format!("{:.0} kW", set(|r| r.load_kw)))]).fixed(36.0),
                row(12.0, vec![button(if manual { "MAN" } else { "AUTO" }, "mode"), cell("Alarm", alarm_name(r.map(|r| r.alarm).unwrap_or_default()).into())]).fixed(36.0),
                row(
                    8.0,
                    vec![
                        micro("Fission").fixed(96.0),
                        button("−", "fis_dn").enabled(manual).fixed(64.0),
                        readout(format!("{:.1}", set(|r| r.set_fission))),
                        button("+", "fis_up").enabled(manual).fixed(64.0),
                    ],
                )
                .fixed(36.0),
                row(
                    8.0,
                    vec![
                        micro("Turbine").fixed(96.0),
                        button("−", "tur_dn").enabled(manual).fixed(64.0),
                        readout(format!("{:.1}", set(|r| r.set_turbine))),
                        button("+", "tur_up").enabled(manual).fixed(64.0),
                    ],
                )
                .fixed(36.0),
                row(12.0, vec![cell("Pump", r.map(|r| works(r.pump.share, r.pump.working)).unwrap_or_default()), cell("Junction", r.map(|r| works(r.junction.share, r.junction.working)).unwrap_or_default())]).fixed(36.0),
                row(12.0, vec![cell("Fire", format!("{:.0}%", set(|r| r.fire_timer * 100.0))), cell("Meltdown", format!("{:.0}%", set(|r| r.meltdown_timer * 100.0)))]).fixed(36.0),
                space(),
            ],
        ),
    )
    .status(
        if on(|r| r.power_on) { if manual { "MANUAL" } else { "AUTO" } } else { "OFF" },
        if on(|r| r.power_on) { Tone::Good } else { Tone::Micro },
    )
}

/// The dev fault screen: the three faults, a fire at the reactor and the reset, one button a row.
pub fn faults_panel() -> Panel {
    Panel::screen(
        Tier::S,
        FAULTS_NAME,
        "DEV-02",
        column(
            4.0,
            vec![
                button("Pump break", "pump_break").fixed(28.0),
                button("Load drop", "load_drop").fixed(28.0),
                button("Surge", "surge").fixed(28.0),
                button("Start fire", "fire").fixed(28.0),
                button("Reset reactor", "reset").fixed(28.0),
                space(),
            ],
        ),
    )
}

/// The alarm's word on the readout.
pub fn alarm_name(a: Alarm) -> &'static str {
    match a {
        Alarm::None => "none",
        Alarm::Warn => "warn",
        Alarm::Danger => "danger",
    }
}

/// The clicks: the readout asks for manual and the targets from the snapshot's values; the fault
/// screen asks for the faults and the reset.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let ask = if c.panel == READOUT_ID {
            match c.action.as_str() {
                "mode" => snap.map(|r| ReactorAsk::SetManual(!r.manual)),
                "fis_up" => snap.map(|r| ReactorAsk::SetFission((r.set_fission + STEP).min(100.0))),
                "fis_dn" => snap.map(|r| ReactorAsk::SetFission((r.set_fission - STEP).max(0.0))),
                "tur_up" => snap.map(|r| ReactorAsk::SetTurbine((r.set_turbine + STEP).min(100.0))),
                "tur_dn" => snap.map(|r| ReactorAsk::SetTurbine((r.set_turbine - STEP).max(0.0))),
                _ => None,
            }
        } else if c.panel == FAULTS_ID {
            match c.action.as_str() {
                "pump_break" => Some(ReactorAsk::Trigger(ReactorFault::PumpBreak)),
                "load_drop" => Some(ReactorAsk::Trigger(ReactorFault::LoadDrop)),
                "surge" => Some(ReactorAsk::Trigger(ReactorFault::Surge)),
                "fire" => Some(ReactorAsk::Trigger(ReactorFault::Fire)),
                "reset" => Some(ReactorAsk::Reset),
                _ => None,
            }
        } else {
            None
        };
        if let Some(a) = ask {
            items.ask_reactor(a);
        }
    }
}

/// Both panels from the state, every tick (`Panels::set` skips an unchanged one).
pub fn write_panels(items: Res<Items>, mut panels: ResMut<Panels>) {
    panels.set(READOUT_ID, readout_panel(items.reactor.as_ref()));
    panels.set(FAULTS_ID, faults_panel());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_of_both_screens_is_inside_its_panel() {
        for p in [readout_panel(None), readout_panel(Some(&ReactorNews { manual: true, ..Default::default() })), faults_panel()] {
            assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
        }
    }

    #[test]
    fn the_manual_buttons_are_only_on_when_manual() {
        let auto = readout_panel(Some(&ReactorNews::default()));
        assert!(matches!(auto.find("fis_up").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
        let man = readout_panel(Some(&ReactorNews { manual: true, ..Default::default() }));
        assert!(matches!(man.find("fis_up").unwrap().widget, display_core::Widget::Button { enabled: true, .. }));
        assert!(man.find("mode").is_some() && man.find("reset").is_none());
        assert!(faults_panel().find("reset").is_some() && faults_panel().find("pump_break").is_some());
        assert!(faults_panel().overflowing().is_empty() && faults_panel().find("fire").is_some());
    }
}
