//! The ship state display: PULSAR's status screen over `crew_core::ShipStatus`, as
//! `live_status` builds it (the engineer corner's reactor, grid and fire deck over the placeholder
//! ship). The dev buttons force trouble onto the placeholder (anyone may press them), and the
//! supply row buys a rod pack for the rod crate from the buyer's role allowance.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crew_core::status::{Forced, ShipSystem, ShipStatus, SYSTEM_MAX};
use crew_core::RoomId;
use display_core::{bar, button, column, label, row, space, title, Panel, Tone};

pub const ID: PanelId = PanelId::new("captain_ship_state");
pub const NAME: &str = "Ship state";

/// A rod pack: this many rods into the engineer's rod crate for this many credits.
/// TODO(initiator): price and size.
pub const ROD_PACK: (u32, i64) = (2, 100);

/// Which room the fire and breach buttons hit, and the last purchase.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct ShipStateUi {
    pub room: usize,
    pub bought: Option<String>,
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(console(ID, NAME, -7.0, [0.28, 0.36, 0.48]));
    app.init_resource::<ShipStateUi>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects).after(super::overview::read_clicks));
}

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

pub fn panel(s: &ShipStatus, ui: &ShipStateUi, mission: Option<String>) -> Panel {
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
    right.push(label("FORCE A STATE (dev)").tone(Tone::Dim).fixed(22.0));
    let room = s.rooms.get(ui.room % s.rooms.len().max(1)).map(|r| r.id.as_str().replace('_', " ")).unwrap_or_default();
    right.push(row(6.0, vec![label(format!("ROOM  {room}")).flex(2.0), button("NEXT ROOM", "room_next").flex(1.0)]).fixed(40.0));
    right.push(row(6.0, vec![button("FIRE", "fire"), button("BREACH", "breach"), button("REACTOR HOT", "hot")]).fixed(44.0));
    right.push(row(6.0, vec![button("WEAPONS DOWN", "down"), button("CLEAR", "clear")]).fixed(44.0));
    right.push(label("SUPPLY").tone(Tone::Dim).fixed(22.0));
    right.push(row(6.0, vec![button(format!("BUY ROD PACK ({}) {} cr", ROD_PACK.0, ROD_PACK.1), "buy_rods").flex(1.4), label(ui.bought.clone().unwrap_or_else(|| "paid from your role's allowance".into())).tone(Tone::Dim).flex(1.6)]).fixed(44.0));
    Panel::new(PANEL_PX[0], PANEL_PX[1], row(28.0, vec![column(6.0, left).flex(1.0), column(6.0, right).flex(1.0)]))
}

/// The tracked job's line: "Title 1/2".
pub fn mission_line(gp: &Gameplay) -> Option<String> {
    let j = gp.tracked.and_then(|t| gp.jobs.get(t))?;
    let r = crew_core::mission_view::row(j, &gp.jobs_content, Role::Captain, gp.tracked)?;
    let l = crew_core::mission_view::line(&r);
    Some(format!("{}  {}/{}", gp.text(&l.title), l.done, l.of))
}

#[allow(clippy::too_many_arguments)]
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut ui: ResMut<ShipStateUi>, mut crew: ResMut<CrewState>, live: Res<live_status::LiveShip>, mut eng: Option<ResMut<crate::dev_area::engineer::Engineering>>, mut gp: ResMut<Gameplay>, role: Res<DevRole>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        let status = live.get(&crew);
        let room = status.rooms.get(ui.room % status.rooms.len().max(1)).map(|r| r.id.clone()).unwrap_or_else(|| RoomId::new("bridge"));
        match c.action.as_str() {
            "room_next" => ui.room = (ui.room + 1) % status.rooms.len().max(1),
            "fire" => crew.ship.force(Forced::Fire(room)),
            "breach" => crew.ship.force(Forced::Breach(room)),
            "hot" => crew.ship.force(Forced::ReactorHot),
            "down" => crew.ship.force(Forced::SystemDown(ShipSystem::Weapons)),
            "clear" => crew.ship.clear(),
            "buy_rods" => match crew.budget.try_spend(role.0, ROD_PACK.1, gp.progress.wallet()) {
                Ok(ev) => {
                    gp.push_world(crate::gameplay::HOST, ev);
                    if let Some(e) = eng.as_deref_mut() {
                        e.rods_in_crate += ROD_PACK.0;
                    }
                    let left = crew.budget.remaining(role.0);
                    ui.bought = Some(format!("{:?} paid {} cr, allowance left {left}", role.0, ROD_PACK.1));
                    gp.log.push(format!("captain: bought rods as {:?}", role.0));
                }
                Err(r) => refuse(&mut gp, &r),
            },
            _ => warn!("ship state: unknown action {c:?}"),
        }
    }
}

pub fn write_panel(gp: Res<Gameplay>, crew: Res<CrewState>, live: Res<live_status::LiveShip>, ui: Res<ShipStateUi>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&live.get(&crew), &ui, mission_line(&gp)));
}
