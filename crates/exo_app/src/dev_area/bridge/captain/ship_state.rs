//! The ship state display: PULSAR's status screen over `crew_core::ShipStatus`, as `live_status`
//! builds it from the engine room's reactor news. Read only: the captain sees the ship, the crew
//! acts on it (the faults screen forces states, the reactor screens press the buttons).
//!
//! The rod pack purchase is not here: our rods are items with no crate to fill. TODO(initiator): a
//! supply line for rods once the rack has a purchase rule.
use super::*;
use crate::displays::Panels;
use crew_core::status::{ShipStatus, ShipSystem, SYSTEM_MAX};
use display_core::{bar, column, label, row, space, title, Panel, Tone};

pub const ID: PanelId = PanelId::new("captain_ship_state");
pub const NAME: &str = "Ship state";

fn system_name(s: ShipSystem) -> &'static str {
    match s {
        ShipSystem::Engineering => "ENGINEERING",
        ShipSystem::Weapons => "WEAPONS",
        ShipSystem::LifeSupport => "LIFE SUPPORT",
        ShipSystem::Science => "SCIENCE",
    }
}

fn clock(s: f64) -> String {
    format!("{:02}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32)
}

pub fn plugin(app: &mut App) {
    add_console(app, 2, ID, NAME, [0.28, 0.36, 0.48]);
    app.add_systems(FixedUpdate, write_panel.in_set(crate::phases::Fx::Effects).after(super::overview::read_clicks));
}

pub fn panel(s: &ShipStatus, mission: Option<String>) -> Panel {
    let mut left = vec![title("SHIP STATE").fixed(40.0)];
    for b in &s.systems {
        let (v, text) = match b.condition {
            Some(c) => (c as f64 / SYSTEM_MAX as f64, format!("{}  {c}/{SYSTEM_MAX}{}", system_name(b.system), if b.fire { "  FIRE" } else { "" })),
            None => (0.0, format!("{}  (-)", system_name(b.system))),
        };
        left.push(bar(v, text).thresholds(Some(0.5), Some(0.25)).fixed(34.0));
    }
    left.push(bar(s.shields.fraction(), format!("SHIELDS  {:.0}", s.shields.value)).thresholds(Some(0.5), Some(0.25)).fixed(34.0));
    left.push(bar(s.hull.fraction(), format!("HULL  {:.0}", s.hull.value)).thresholds(Some(0.6), Some(0.3)).fixed(34.0));
    left.push(bar(s.oxygen.level, format!("O2  {:.0} %   {:+.2} %/s", s.oxygen.level * 100.0, s.oxygen.delta_per_s * 100.0)).thresholds(Some(0.8), Some(0.6)).fixed(34.0));
    let r = &s.reactor;
    let tone = match r.band {
        crew_core::status::TempBand::Hot => Tone::Warn,
        crew_core::status::TempBand::Critical => Tone::Bad,
        _ => Tone::Normal,
    };
    left.push(label(format!("REACTOR  {:?}  {:.0} %  {:?}   fuel {:.0} %", r.mode, r.temp * 100.0, r.band, r.fuel * 100.0)).tone(tone).fixed(26.0));
    if let (Some(out), Some(load)) = (r.output_kw, r.load_kw) {
        left.push(label(format!("OUTPUT  {out:.0} kW of {load:.0} kW asked")).tone(if out + 1.0 < load { Tone::Warn } else { Tone::Normal }).fixed(26.0));
    }
    left.push(label(format!("CLOCK  {}", s.clock_left_s.map(clock).unwrap_or_else(|| "-".into()))).fixed(26.0));
    left.push(label(format!("MISSION  {}", mission.unwrap_or_else(|| "none tracked".into()))).fixed(26.0));
    left.push(space());

    let mut right = vec![label("ROOMS").tone(Tone::Dim).fixed(40.0)];
    for room in &s.rooms {
        let flags = [(room.fire, "FIRE"), (room.breach, "BREACH")].iter().filter(|(on, _)| *on).map(|(_, n)| *n).collect::<Vec<_>>().join(" ");
        let tone = if room.fire || room.breach { Tone::Bad } else { Tone::Normal };
        let cells = [room.id.as_str().replace('_', " "), format!("{:.0} %", room.condition * 100.0), format!("O2 {:.0} %", room.oxygen * 100.0), format!("crew {}", room.crew), flags];
        right.push(row(6.0, cells.into_iter().zip([1.5, 0.7, 0.9, 0.8, 1.4]).map(|(c, f)| label(c).tone(tone).flex(f)).collect()).fixed(28.0));
    }
    right.push(space());
    Panel::new(PANEL_PX[0], PANEL_PX[1], row(28.0, vec![column(6.0, left).flex(1.0), column(6.0, right).flex(1.0)]))
}

/// The tracked job's line: "Title 1/2".
pub fn mission_line(gp: &Gameplay) -> Option<String> {
    let j = gp.tracked.and_then(|t| gp.jobs.get(t))?;
    let r = crew_core::mission_view::row(j, &gp.jobs_content, Role::Captain, gp.tracked)?;
    let l = crew_core::mission_view::line(&r);
    Some(format!("{}  {}/{}", gp.text(&l.title), l.done, l.of))
}

pub fn write_panel(gp: Res<Gameplay>, live: Res<live_status::LiveShip>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&live.get(), mission_line(&gp)));
}
