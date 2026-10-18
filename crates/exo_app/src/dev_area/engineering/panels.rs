//! Engineering's screens: the coolant pump, the junction box (breakers and limits), AUX and the battery
//! (the reactor's own screens are `reactor_room`: main and supply, #317/#318). Each reads the host's
//! snapshot; a press asks the host.
//! TODO(initiator): the glow of the critical text and the scanlines' look are not drawn yet.
use crate::dev_area::*;
use crate::displays::{DisplayAction, Panels};
use crate::dev_area::screens::*;
use crate::dev_area::ship::{lamp_blink_on, ReactorConfig};
use crate::items::Items;
use display_core::{button, canvas, column, ctl, font, micro, readout, row, slider, space, Ink, Item, Panel, Shape, Tier, TextFace, Tone};
use item_core::wire::{BotStation, BreakerGroup, PumpMode, ReactorAsk, ReactorNews};

use crate::dev_area::crew::{bot_tagged, manned};
use crate::dev_area::engineering::reactor_room::{MAIN_ID, SUPPLY_ID};

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

pub const PUMP_ID: PanelId = PanelId::new("coolant_pump");

pub const PUMP_NAME: &str = "COOLANT PUMP";

pub const JUNCTION_ID: PanelId = PanelId::new("junction_box");

pub const JUNCTION_NAME: &str = "JUNCTION BOX";

pub const AUX_ID: PanelId = PanelId::new("aux_panel");

pub const BATTERY_ID: PanelId = PanelId::new("battery_panel");

pub const BATTERY_NAME: &str = "BATTERY";

pub const AUX_NAME: &str = "AUX";


/// The heat zones of the sim's bands: `Good` is the optimal band, `Low` and `High` the
/// rest of the allowed band, `Crit` above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Cold,
    Low,
    Good,
    High,
    Crit,
}

/// The bands `(optimal, allowed)` the screens show: the news' (the sim's, from the operator's
/// skill). Without news the skill-50 bands, so the screens stay readable.
pub fn bands(r: Option<&ReactorNews>) -> ([f64; 2], [f64; 2]) {
    match r {
        Some(r) if r.optimal[1] > r.optimal[0] => (r.optimal, r.allowed),
        _ => (SKILL_50_OPTIMAL, SKILL_50_ALLOWED),
    }
}

/// The sim's bands at skill 50 (`reactor_core::Bands::new`).
const SKILL_50_OPTIMAL: [f64; 2] = [35.0, 65.0];

const SKILL_50_ALLOWED: [f64; 2] = [20.0, 80.0];

pub fn zone(t: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Zone {
    if t > allowed[1] {
        Zone::Crit
    } else if t > optimal[1] {
        Zone::High
    } else if t >= optimal[0] {
        Zone::Good
    } else if t >= allowed[0] {
        Zone::Low
    } else {
        Zone::Cold
    }
}

pub fn zone_tone(z: Zone) -> Tone {
    match z {
        Zone::Cold => Tone::Dim,
        Zone::Low | Zone::High => Tone::Warn,
        Zone::Good => Tone::Good,
        Zone::Crit => Tone::Bad,
    }
}

pub fn reactor_status(r: Option<&ReactorNews>) -> (&'static str, Tone) {
    match r {
        Some(r) if r.ejected => ("NO CORE", Tone::Dim),
        Some(r) if r.power_on => match zone(r.heat, r.optimal, r.allowed) {
            Zone::Crit => ("CRITICAL", Tone::Bad),
            Zone::High => ("HOT", Tone::Warn),
            _ => ("STABLE", Tone::Good),
        },
        _ => ("OFF", Tone::Dim),
    }
}


/// The pump's status word and its tone: OFF, LOW or HIGH by the mode, DRY with an empty tank (the
/// column is 60 px: "NO COOLANT" does not fit it).
pub fn pump_status(mode: PumpMode, coolant: f64) -> (&'static str, Tone) {
    match mode {
        PumpMode::Off => ("OFF", Tone::Dim),
        _ if coolant <= 0.0 => ("DRY", Tone::Bad),
        PumpMode::Low => ("LOW", Tone::Normal),
        PumpMode::High => ("HIGH", Tone::Good),
    }
}

fn pump_modes(mode: PumpMode, r: &ReactorNews) -> Item {
    let mode_button = |label: &str, action: &str, m: PumpMode| button(label, action).tone(if mode == m { Tone::Good } else { Tone::Normal });
    // Parked (#306 Parkplatz): the E-COOL button (`ecool_button`) was a fourth cell here, flex 1.4.
    let _ = r;
    row(6.0, vec![mode_button("OFF", "pump_off", PumpMode::Off).flex(1.0), mode_button("LOW", "pump_low", PumpMode::Low).flex(1.0), mode_button("HIGH", "pump_high", PumpMode::High).flex(1.0)]).fixed(ctl::MD)
}

/// when a press would be taken, the cooldown as m:ss greyed while it runs (TODO(initiator): the
/// label "E-COOL 0:42" does not fit a cell of this row), COOL greyed while the extra cooling runs.
#[allow(dead_code)] // parked (#306 Parkplatz)
pub fn ecool_button(r: &ReactorNews) -> Item {
    if r.ecool_active {
        return button("COOL", "ecool").tone(Tone::Good).enabled(false);
    }
    if r.ecool_ready_in_s > 0.0 {
        let s = r.ecool_ready_in_s.ceil() as u32;
        return button(format!("{}:{:02}", s / 60, s % 60), "ecool").enabled(false);
    }
    button("E-COOL", "ecool").enabled(r.ecool_ok)
}

fn pump_picture(flow_on: bool, angle: f64) -> Item {
    let fluid = if flow_on { Ink::Fg1 } else { Ink::Fg3 };
    let (cx, cy) = (76.0, 60.0);
    let mut s = vec![
        Shape::Rect { x: 0.0, y: 44.0, w: 44.0, h: 16.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Line { a: [1.5, 52.0], b: [44.0, 52.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Rect { x: 70.0, y: 0.0, w: 14.0, h: 22.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Rect { x: 70.0, y: 0.0, w: 82.0, h: 14.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Line { a: [76.0, 20.0], b: [76.0, 7.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Line { a: [76.0, 7.0], b: [150.5, 7.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Circle { cx, cy, r: 40.0, fill: Some(Ink::Bg1), stroke: Some((Ink::Hud, 3.0)) },
    ];
    for k in 0..6 {
        let a = (k as f64 * 60.0 + angle).to_radians();
        s.push(Shape::Line { a: [cx, cy], b: [cx + 30.0 * a.cos(), cy + 30.0 * a.sin()], width: 6.0, ink: Ink::Hud, dash: None });
    }
    s.push(Shape::Circle { cx, cy, r: 9.0, fill: Some(Ink::Hud), stroke: None });
    canvas(152.0, 109.0, s, None).fixed(152.0)
}

pub fn pump_panel(r: Option<&ReactorNews>) -> Panel {
    pump_panel_at(r, 0.0, true)
}

/// `blink_on` is the lamps' blink (`lamp_blink_on`): the LEAK word shows on its lit half.
pub fn pump_panel_at(r: Option<&ReactorNews>, angle: f64, blink_on: bool) -> Panel {
    if !powered(r) {
        return no_power(PUMP_NAME, "CP-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (word, tone) = pump_status(r.pump_mode, r.coolant);
    let body = column(
        8.0,
        vec![
            row(
                10.0,
                vec![
                    pump_picture(r.pump_on && r.flow > 0.0, angle),
                    column(4.0, vec![micro("FLOW").fixed(14.0), readout(format!("{:.0}", r.flow * 100.0)).text_size(font::LABEL).tone(tone).fixed(46.0), readout(word).text_size(font::LABEL).tone(tone)]).flex(1.0),
                    coolant_tube(&r, blink_on).fixed(60.0),
                ],
            )
            .fixed(109.0),
            pump_modes(r.pump_mode, &r),
            space(),
        ],
    );
    Panel::screen(Tier::S, PUMP_NAME, "CP-01", body).with_padding(6.0)
}

/// fill) and EMPTY at none (the word in bad). The sim's level and line (`ReactorNews::coolant`).
/// A leak blinks LEAK in the word's place (not over EMPTY).
fn coolant_tube(r: &ReactorNews, blink_on: bool) -> Item {
    let (x, top, w, h) = (14.0, 16.0, 32.0, 70.0);
    let mut s = vec![text(0.0, 0.0, 60.0, 14.0, "COOL", 14.0, TextFace::Display, Ink::HudDim, true), Shape::Rect { x, y: top, w, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    let fill_h = h * (r.coolant / 100.0).clamp(0.0, 1.0);
    if fill_h > 0.0 {
        let fy = top + h - fill_h;
        if r.coolant_low {
            s.push(Shape::Hatch { x: x + 2.0, y: fy, w: w - 4.0, h: fill_h, ink: Ink::Warn });
        } else {
            s.push(Shape::Rect { x: x + 2.0, y: fy, w: w - 4.0, h: fill_h, fill: Some(Ink::Good), outline: None });
        }
    }
    let (word, ink) = if r.coolant <= 0.0 {
        ("EMPTY", Ink::Bad)
    } else if r.coolant_leak > 0.0 && blink_on {
        ("LEAK", Ink::Warn)
    } else if r.coolant_low {
        ("LOW", Ink::Warn)
    } else {
        ("", Ink::Fg2)
    };
    if !word.is_empty() {
        s.push(text(0.0, 90.0, 60.0, 16.0, word, 16.0, TextFace::DisplayBold, ink, true));
    }
    canvas(60.0, 109.0, s, None)
}

/// A group's bar under its lever (94 wide, 18 high): the fill is `delivered / demand` (0 with no demand)
fn group_bar(delivered: f64, demand: f64, limit: f64) -> Item {
    let (w, top, h) = (94.0, 6.0, 12.0);
    let frac = if demand > 0.0 { (delivered / demand).clamp(0.0, 1.0) } else { 0.0 };
    let mut s = vec![Shape::Rect { x: 0.0, y: top, w, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if frac > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: top + 2.0, w: (w - 4.0) * frac, h: h - 4.0, fill: Some(Ink::Hud), outline: None });
    }
    let cx = (limit.clamp(0.0, 1.0) * w).clamp(5.0, w - 5.0);
    // The sides start 1 px down: a 2 px stroke at the canvas's top edge would stick out of it.
    s.push(Shape::Line { a: [cx - 4.0, 1.0], b: [cx, top], width: 2.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Line { a: [cx + 4.0, 1.0], b: [cx, top], width: 2.0, ink: Ink::FgTitle, dash: None });
    canvas(w, 18.0, s, None).fixed(18.0)
}

fn total_bar(total: f64, max_kw: f64) -> Item {
    let (w, bar_x, bar_w) = (292.0, 60.0, 140.0);
    let frac = if max_kw > 0.0 { (total / max_kw).clamp(0.0, 1.0) } else { 0.0 };
    let mut s = vec![
        text(0.0, 0.0, 56.0, 18.0, "TOTAL", 16.0, TextFace::Display, Ink::HudDim, false),
        Shape::Rect { x: bar_x, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
    ];
    if frac > 0.0 {
        s.push(Shape::Rect { x: bar_x + 2.0, y: 5.0, w: (bar_w - 4.0) * frac, h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &format!("{total:.0} kW"), 16.0, TextFace::Mono, Ink::Fg2, false));
    canvas(w, 18.0, s, None).fixed(18.0)
}

fn junction_lever(r: &ReactorNews, g: BreakerGroup, names: &[&str], action: &str, limit: &str) -> Item {
    let i = g.index();
    let on = r.breakers[i];
    let name_ink = if on { Ink::Fg1 } else { Ink::Fg3 };
    column(
        3.0,
        vec![
            lever_in(94.0, LEVER_PACKED, names, on, action, name_ink).fixed(44.0 + LEVER_PACKED.track),
            group_bar(r.delivered_kw[i], r.demand_kw[i], r.limits[i]),
            slider(r.limits[i], "", limit).fixed(20.0),
        ],
    )
    .fixed(94.0)
}

/// Parked (#307): the old JB-01 screen. The junction boxes have no screen since #322 (the toggles sit on the boxes) and this is
/// not placed; the central power screen is built from it.
pub fn junction_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    if !powered(r) {
        return tall(no_power(JUNCTION_NAME, "JB-01", Tier::S));
    }
    let r = r.copied().unwrap_or_default();
    // Two rows of three (r39): the room's three groups, then the r40 groups. Each row is 112 px.
    let row_a = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Engines, &["ENGINES"], "br_engines", "lim_engines"),
            junction_lever(&r, BreakerGroup::LifeSupport, &["LIFE", "SUPPORT"], "br_life", "lim_life"),
            junction_lever(&r, BreakerGroup::Lights, &["LIGHTS"], "br_lights", "lim_lights"),
        ],
    )
    .fixed(112.0);
    let row_b = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Shields, &["SHIELDS"], "br_shields", "lim_shields"),
            junction_lever(&r, BreakerGroup::Weapons, &["WEAPONS"], "br_weapons", "lim_weapons"),
            junction_lever(&r, BreakerGroup::JumpDrive, &["JUMP", "DRIVE"], "br_jump", "lim_jump"),
        ],
    )
    .fixed(112.0);
    let levers = column(4.0, vec![row_a, row_b]).fixed(228.0);
    let total = r.delivered_kw.iter().sum::<f64>();
    let body = column(6.0, vec![levers, total_bar(total, max_kw)]);
    tall(Panel::screen(Tier::S, JUNCTION_NAME, "JB-01", body).with_padding(6.0))
}

/// BT-01 (r38, Barotrauma PowerContainer): the battery behind the reactor: its charge, the kW it
/// gives the grid now, and the recharge slider (the share of its max charge speed taken from the surplus).
pub fn battery_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let bar_w = 200.0;
    let pct = format!("{:.0} %", r.battery_charge.clamp(0.0, 1.0) * 100.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if r.battery_charge > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * r.battery_charge.clamp(0.0, 1.0), h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &pct, 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("CHARGE").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let out = readout(format!("OUTPUT {:.0} kW", r.battery_kw)).text_size(font::SMALL).tone(Tone::Normal);
    let rate = column(4.0, vec![micro("RECHARGE").fixed(14.0), slider(r.battery_recharge, "", "bat_rate").fixed(40.0)]);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![charge, out.fixed(20.0), rate]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, BATTERY_NAME, "BT-01", body)).with_padding(6.0)
}

/// power: the aux reactor has its own supply. TODO(initiator): confirm.
pub fn aux_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    if !r.aux_on {
        let (w, h) = (292.0, 264.0);
        let shapes = vec![text(0.0, h * 0.5 - 10.0, w, 20.0, "AUX OFFLINE", 22.0, TextFace::DisplayBold, Ink::Fg3, true)];
        return tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", canvas(w, h, shapes, None).flex(1.0)));
    }
    let name = Ink::Fg1;
    let switches = row(
        0.0,
        vec![lever_in(76.0, LEVER, &["LIFE", "SUPPORT"], r.aux_life_support, "aux_life", name), lever_in(76.0, LEVER, &["LIGHTS"], r.aux_lights, "aux_lights", name)],
    )
    .fixed(152.0);
    let boost = column(4.0, vec![micro("AUX OUTPUT").fixed(14.0), readout(format!("BOOST +{:.0} kW", r.aux_boost_kw)).text_size(font::SMALL).tone(Tone::Normal)]);
    let body = column(0.0, vec![space().flex(1.0), row(10.0, vec![switches, boost.flex(1.0)]).fixed(168.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", body)).with_padding(6.0)
}

pub fn band(heat: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let x = |v: f64| v * 2.0;
    let seg = |a: f64, b: f64, ink: Option<Ink>, fill: Ink| {
        let mut v = vec![Shape::Rect { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, fill: Some(fill), outline: None }];
        if let Some(i) = ink {
            v.push(Shape::Hatch { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, ink: i });
        }
        v
    };
    let mut s = Vec::new();
    s.extend(seg(0.0, allowed[0], None, Ink::Line2));
    s.extend(seg(allowed[0], optimal[0], Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(optimal[0], optimal[1], None, Ink::GoodDim));
    s.extend(seg(optimal[1], allowed[1], Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(allowed[1], 100.0, Some(Ink::Bad), Ink::Bg3));
    let m = heat.clamp(0.0, 100.0);
    s.push(Shape::Rect { x: x(m) - 2.0, y: 0.0, w: 4.0, h: 22.0, fill: Some(Ink::FgTitle), outline: None });
    canvas(200.0, 22.0, s, None).fixed(200.0)
}

pub fn limit_group(action: &str) -> Option<BreakerGroup> {
    match action {
        "lim_engines" => Some(BreakerGroup::Engines),
        "lim_life" => Some(BreakerGroup::LifeSupport),
        "lim_lights" => Some(BreakerGroup::Lights),
        "lim_shields" => Some(BreakerGroup::Shields),
        "lim_weapons" => Some(BreakerGroup::Weapons),
        "lim_jump" => Some(BreakerGroup::JumpDrive),
        _ => None,
    }
}

/// The impeller's angle is kept in steps of 6 degrees, so the panel is only redrawn when it shows.
const IMPELLER_STEP: f64 = 6.0;

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

/// The presses on engineering's screens become the host's asks.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let new_state = |current: bool| c.value.map(|v| v >= 0.5).unwrap_or(!current);
        let ask = if c.panel == MAIN_ID {
            main_ask(&c.action, c.value)
        } else if c.panel == PUMP_ID {
            match c.action.as_str() {
                "pump_off" => Some(ReactorAsk::SetPumpMode(PumpMode::Off)),
                "pump_low" => Some(ReactorAsk::SetPumpMode(PumpMode::Low)),
                "pump_high" => Some(ReactorAsk::SetPumpMode(PumpMode::High)),
                "ecool" => Some(ReactorAsk::EmergencyCool),
                _ => None,
            }
        } else if c.panel == JUNCTION_ID && let Some(g) = limit_group(&c.action) {
            c.value.map(|v| ReactorAsk::SetLimit(g, v as f64))
        } else if c.panel == JUNCTION_ID {
            let group = match c.action.as_str() {
                "br_engines" => Some(BreakerGroup::Engines),
                "br_life" => Some(BreakerGroup::LifeSupport),
                "br_lights" => Some(BreakerGroup::Lights),
                "br_shields" => Some(BreakerGroup::Shields),
                "br_weapons" => Some(BreakerGroup::Weapons),
                "br_jump" => Some(BreakerGroup::JumpDrive),
                _ => None,
            };
            group.and_then(|g| snap.map(|r| ReactorAsk::SetBreaker(g, new_state(r.breakers[g.index()]))))
        } else if c.panel == AUX_ID {
            match c.action.as_str() {
                "aux_life" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::LifeSupport, new_state(r.aux_life_support))),
                "aux_lights" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::Lights, new_state(r.aux_lights))),
                _ => None,
            }
        } else if c.panel == BATTERY_ID {
            match c.action.as_str() {
                "bat_rate" => c.value.map(|v| ReactorAsk::SetBatteryRecharge(v as f64)),
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

/// Engineering's panels from the snapshot, every tick (`Panels::set` skips an unchanged one). The
/// impeller's angle is kept here.
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, time: Res<Time>, mut spin: Local<f64>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    let speed = items.reactor.map_or(0.0, |r| r.pump_speed);
    *spin = crate::effects::impeller_angle(*spin, speed, time.delta_secs_f64());
    let angle = (*spin / IMPELLER_STEP).round() * IMPELLER_STEP;
    let max_kw = cfg.0.reactor.max_output_kw;
    let blink = lamp_blink_on(time.elapsed_secs_f64());
    // The main screen draws its own dark and word faces (NO POWER, ...), so it does not go through `show`.
    // The BOT tag of a manned station goes on its plate like on every other screen.
    let run_up = r.is_some_and(|r| r.start == item_core::wire::StartStage::RunUp);
    panels.set(MAIN_ID, bot_tagged(display_core::reactor_main::panel(r, max_kw, run_up), manned(&items, BotStation::Reactor)));
    // The supply screen goes dark by the main screen's rule (`reactor_main::face`), inside its panel.
    panels.set(SUPPLY_ID, display_core::reactor_supply::panel(r));
    show(&mut panels, r, PUMP_ID, bot_tagged(pump_panel_at(r, angle, blink), manned(&items, BotStation::Reactor)));
    show(&mut panels, r, AUX_ID, aux_panel(r));
    show(&mut panels, r, BATTERY_ID, battery_panel(r));
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
