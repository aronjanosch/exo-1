//! The engine room's screens (#297, slice S3c), drawn to the initiator's design (Claude Design,
//! "EXO-1 Crew HUD", spec `engine-room-screens-spec.md` in the session scratchpad). The screens show
//! little, in the ship's fiction: no condition values, sensors instead.
//!
//! - The reactor (M, on the east wall): the core temperature arc (zones from the sim's bands), the
//!   output against the load, two fuel tubes, the fission and turbine sliders, MAN/AUTO, POWER, SCRAM.
//!   Above the allowed band with the power on it is the same readout with one hazard stripe at its
//!   top and the header word CRITICAL (`effects::is_critical`, the same test as the red lamp); the
//!   controls stay. After a meltdown it is the MELTDOWN view, with no button. The readout
//!   has its own start-up supply, so POWER can come back after a SCRAM even with the machines dark
//!   (TODO(initiator): confirm).
//! - The coolant pump (S, on the pump block): a picture of the pump, the flow, the status word, the
//!   three mode buttons OFF / LOW / HIGH. The impeller turns with the pump's speed (`effects::impeller_angle`).
//! - The junction box (S, on the wall above it): three breaker levers, nothing else. Overvolt shows
//!   in the world (sparks), not here.
//! - The reactor room's door screen (S, in the corridor by the door): the reactor temperature, its
//!   zone band, the room's oxygen, the lights breaker. TODO(captain): the emergency view with the
//!   captain role.
//! - The RX STATUS screen (S, on the south wall beside the readout, to its east): the warning lamps,
//!   the temperature boost (two buttons) and the implosion's countdown while the meltdown timer runs.
//!   NO POWER like the others.
//! - The dev faults screen (S, next to the test console): always powered.
//!
//! The machines' screens show NO POWER unless the snapshot says `powered`. Every process shows the
//! snapshot it has (`Items::reactor`); the clicks ask the host (`ReactorAsk`).
//!
//! One meaning of CRITICAL: the zones, the HOT and CRITICAL words, the door screen's band, the
//! readout's stripe and the lamp all read the sim's bands from `ReactorNews` (`optimal`,
//! `allowed`). The output scale is the room's maximum, `max_output_kw` in `reactor.json` (2000 kW;
//! the spec drew 5000).
//! TODO(initiator): the glow of the critical text and the scanlines' look are not drawn yet.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::dev_area::reactor::{lamp_blink_on, ReactorConfig};
use crate::items::Items;
use display_core::{button, canvas, column, ctl, font, micro, readout, row, slider, space, toggle, Ink, Item, Panel, Shape, Tier, TextFace, Tone};
use item_core::wire::{BreakerGroup, Lamp, PumpMode, ReactorAsk, ReactorFault, ReactorNews};

/// The status board's lamps, in the panel's order (three to a row).
const LAMPS: [Lamp; Lamp::COUNT] = [Lamp::Hot, Lamp::CritHeat, Lamp::Cold, Lamp::LowTemp, Lamp::HighOut, Lamp::CritOut, Lamp::LowOut, Lamp::FuelLow, Lamp::NoFuel, Lamp::Meltdown, Lamp::Scram];
/// A lamp's cell (px) and the gap between the cells.
const LAMP_W: f64 = 96.0;
const LAMP_H: f64 = 20.0;
const LAMP_GAP: f64 = 3.0;

pub const READOUT_ID: PanelId = PanelId::new("reactor_readout");
pub const READOUT_NAME: &str = "REACTOR";
pub const PUMP_ID: PanelId = PanelId::new("coolant_pump");
pub const PUMP_NAME: &str = "COOLANT PUMP";
pub const JUNCTION_ID: PanelId = PanelId::new("junction_box");
pub const JUNCTION_NAME: &str = "JUNCTION BOX";
pub const DOOR_ID: PanelId = PanelId::new("engine_door_screen");
pub const DOOR_NAME: &str = "REACTOR ROOM";
pub const FAULTS_ID: PanelId = PanelId::new("reactor_faults");
pub const FAULTS_NAME: &str = "Dev faults";
pub const STATUS_ID: PanelId = PanelId::new("reactor_status");
pub const STATUS_NAME: &str = "RX STATUS";
/// The dev faults screen's mount point (area metres, the engine room's north wall, the room side).
pub const FAULTS_AT: [f64; 2] = [-17.5, -11.04];

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

// ---------- the gauge's geometry ----------

/// The arc's start (bottom left) and its sweep (270 degrees, clockwise on screen).
const ARC_START: f64 = 135.0;
const ARC_SWEEP: f64 = 270.0;

/// The angle of a value 0..100 on the arc.
fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * v.clamp(0.0, 100.0) / 100.0
}

/// The temperature zones of the sim's bands: `Good` is the optimal band, `Low` and `High` the
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

/// The zone of `t` against the bands. The lines are the sim's alarm lines: above the optimal top
/// is HOT, above the allowed top is CRITICAL.
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

/// The zone's tone for text: cold dim, the warn zones warn, good, the critical zone bad.
pub fn zone_tone(z: Zone) -> Tone {
    match z {
        Zone::Cold => Tone::Dim,
        Zone::Low | Zone::High => Tone::Warn,
        Zone::Good => Tone::Good,
        Zone::Crit => Tone::Bad,
    }
}

/// An arc stroke of the gauge.
fn arc(cx: f64, cy: f64, r: f64, from: f64, to: f64, stroke: f64, ink: Ink, dash: Option<(f64, f64)>) -> Shape {
    Shape::Arc { cx, cy, r, from, to, stroke, ink, dash }
}

/// The temperature gauge (216 x 152): the arc with its zones, the needle, the value and caption.
fn temp_gauge(temp: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let (cx, cy, r, sw) = (108.0, 70.0, 60.0, 14.0);
    let z = zone(temp, optimal, allowed);
    let t = temp.clamp(0.0, 100.0);
    let mut s = vec![
        arc(cx, cy, r, angle(0.0), angle(allowed[0]), sw, Ink::Line2, None),
        arc(cx, cy, r, angle(allowed[0]), angle(optimal[0]), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(optimal[0]), angle(optimal[1]), sw, Ink::Good, None),
        arc(cx, cy, r, angle(optimal[1]), angle(allowed[1]), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(allowed[1]), angle(100.0), sw, Ink::Bad, Some((3.0, 2.0))),
    ];
    let a = angle(t).to_radians();
    s.push(Shape::Line { a: [cx, cy], b: [cx + 46.0 * a.cos(), cy + 46.0 * a.sin()], width: 5.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Circle { cx, cy, r: 8.0, fill: Some(Ink::Bg0), stroke: Some((Ink::FgTitle, 3.0)) });
    s.push(text(0.0, 74.0, 216.0, 28.0, &format!("{temp:.0} °C"), 28.0, TextFace::Mono, zone_ink(z), true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "CORE TEMP", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(216.0, 152.0, s, None).fixed(216.0)
}

fn zone_ink(z: Zone) -> Ink {
    match z {
        Zone::Cold => Ink::Fg2,
        Zone::Low | Zone::High => Ink::Warn,
        Zone::Good => Ink::Good,
        Zone::Crit => Ink::Bad,
    }
}

/// The POWER gauge (216 x 152): output against the scale, its colour by the shortage of the load,
/// a notch at the load.
fn power_gauge(r: Option<&ReactorNews>, max_kw: f64) -> Item {
    let (cx, cy, rr, sw) = (108.0, 70.0, 60.0, 14.0);
    let (out, load) = r.map_or((0.0, 0.0), |r| (r.output_kw, r.load_kw));
    let frac = (out / max_kw * 100.0).clamp(0.0, 100.0);
    let ink = power_ink(out, load);
    let mut s = vec![arc(cx, cy, rr, angle(0.0), angle(100.0), sw, Ink::Bg3, None)];
    if frac > 0.0 {
        s.push(arc(cx, cy, rr, angle(0.0), angle(frac), sw, ink, None));
    }
    let n = (load / max_kw * 100.0).clamp(0.0, 100.0);
    let (a, p0, p1) = (angle(n).to_radians(), 50.0, 66.0);
    s.push(Shape::Line { a: [cx + p0 * a.cos(), cy + p0 * a.sin()], b: [cx + p1 * a.cos(), cy + p1 * a.sin()], width: 4.0, ink: Ink::FgTitle, dash: None });
    s.push(text(0.0, 62.0, 216.0, 32.0, &format!("{out:.0}"), 28.0, TextFace::Mono, ink, true));
    s.push(text(0.0, 98.0, 216.0, 18.0, &format!("LOAD {load:.0}"), 16.0, TextFace::Mono, Ink::Fg2, true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "POWER kW", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(216.0, 152.0, s, None).fixed(216.0)
}

/// The output's colour: none when off, good while it covers the load, warn when it falls short by
/// more than 5 %, bad by more than 40 %.
pub fn power_ink(out: f64, load: f64) -> Ink {
    if out <= 0.0 {
        return Ink::Fg3;
    }
    if load <= 0.0 {
        return Ink::Good;
    }
    let short = (load - out) / load;
    if short <= 0.05 {
        Ink::Good
    } else if short <= 0.40 {
        Ink::Warn
    } else {
        Ink::Bad
    }
}

/// The fuel tubes (176 x 152): two tubes with their fill (life), hatched when low.
fn fuel(r: Option<&ReactorNews>) -> Item {
    let life = r.map_or([0.0, 0.0], |r| r.fuel);
    let mut s = Vec::new();
    for (i, &l) in life.iter().enumerate() {
        let x = 40.0 + 66.0 * i as f64;
        let (top, h) = (10.0, 100.0);
        s.push(Shape::Rect { x, y: top, w: 30.0, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
        let fill_h = h * (l / 100.0).clamp(0.0, 1.0);
        let fy = top + h - fill_h;
        if fill_h > 0.0 {
            if l <= 10.0 {
                s.push(Shape::Hatch { x: x + 2.0, y: fy, w: 26.0, h: fill_h, ink: Ink::Bad });
            } else if l <= 25.0 {
                s.push(Shape::Hatch { x: x + 2.0, y: fy, w: 26.0, h: fill_h, ink: Ink::Warn });
            } else {
                s.push(Shape::Rect { x: x + 2.0, y: fy, w: 26.0, h: fill_h, fill: Some(Ink::Good), outline: None });
            }
        }
        s.push(text(x - 10.0, 116.0, 50.0, 16.0, &format!("R{}", i + 1), 16.0, TextFace::Mono, Ink::Fg2, true));
    }
    s.push(text(0.0, 136.0, 176.0, 16.0, "FUEL", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(176.0, 152.0, s, None).flex(1.0)
}

/// A shape's text in a box.
fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink, centred: bool) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred }
}

/// The status word of the reactor header: OFF, STABLE, HOT or CRITICAL.
pub fn reactor_status(r: Option<&ReactorNews>) -> (&'static str, Tone) {
    match r {
        Some(r) if r.power_on => match zone(r.temp, r.optimal, r.allowed) {
            Zone::Crit => ("CRITICAL", Tone::Bad),
            Zone::High => ("HOT", Tone::Warn),
            _ => ("STABLE", Tone::Good),
        },
        _ => ("OFF", Tone::Dim),
    }
}

/// A full-width hazard stripe (14 high): diagonal bad stripes on the dark ground.
fn hazard(w: f64) -> Item {
    canvas(w, 14.0, vec![Shape::Rect { x: 0.0, y: 0.0, w, h: 14.0, fill: Some(Ink::Bg0), outline: None }, Shape::Hatch { x: 0.0, y: 0.0, w, h: 14.0, ink: Ink::Bad }], None).fixed(14.0)
}

/// The meltdown view (M): the stripes and the word MELTDOWN, no button. The core stays broken
/// until a repair brings it back (`Reactor::melted`).
fn meltdown_view() -> Panel {
    let word = canvas(624.0, 200.0, vec![text(0.0, 60.0, 624.0, 72.0, "MELTDOWN", 56.0, TextFace::DisplayBold, Ink::Bad, true)], None).fixed(200.0);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", column(8.0, vec![hazard(624.0), word, hazard(624.0)])).status("MELTDOWN", Tone::Bad).with_padding(8.0)
}

/// The reactor readout (M): the top row of gauges and fuel, the sliders and the switches below.
/// Above the allowed band with the power on, one hazard stripe under the sliders, inside the bottom-left
/// column (in place of its filler, so the body gets no new row), and the header word CRITICAL. After a
/// meltdown the MELTDOWN view.
/// The readout panel; `max_kw` is the scale of the power gauge (the reactor's `max_output_kw`).
pub fn readout_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    if r.is_some_and(|r| r.melted) {
        return meltdown_view();
    }
    let on = r.is_some_and(|r| r.power_on);
    let manual = r.is_some_and(|r| r.manual);
    let (fis, tur) = r.map_or((0.0, 0.0), |r| (r.set_fission, r.set_turbine));
    let (word, tone) = reactor_status(r);
    // Critical: the stripe is as wide as the bottom-left column (624 content - 16 gap - 200 switches)
    // and the spacing tightens by 18 px so the stripe fits (TODO(initiator): the readout's height budget).
    let critical = crate::effects::is_critical(r);
    let (filler, gap) = if critical { (hazard(408.0), 6.0) } else { (space(), 10.0) };
    let bottom_left = column(
        gap,
        vec![
            row(8.0, vec![micro("FISSION RATE").flex(1.0), readout(format!("{fis:.0} %")).text_size(font::LABEL).tone(if manual { Tone::Normal } else { Tone::Dim })]).fixed(24.0),
            slider(fis / 100.0, "", "fis_set").enabled(manual).fixed(ctl::MD),
            row(8.0, vec![micro("TURBINE OUTPUT").flex(1.0), readout(format!("{tur:.0} %")).text_size(font::LABEL).tone(if manual { Tone::Normal } else { Tone::Dim })]).fixed(24.0),
            slider(tur / 100.0, "", "tur_set").enabled(manual).fixed(ctl::MD),
            filler,
        ],
    );
    let switches = column(
        8.0,
        vec![
            toggle(manual, "", "mode").labels("AUTO", "MAN").fixed(ctl::MD),
            toggle(on, "", "power").labels("OFF", "ON").fixed(ctl::MD),
            button("SCRAM", "scram").tone(Tone::Bad).fixed(ctl::LG),
        ],
    );
    let (optimal, allowed) = bands(r);
    let gauges = row(8.0, vec![temp_gauge(r.map_or(0.0, |r| r.temp), optimal, allowed), power_gauge(r, max_kw), fuel(r)]).fixed(152.0);
    let body = column(if critical { 4.0 } else { 8.0 }, vec![gauges, row(16.0, vec![bottom_left.flex(1.0), switches.fixed(200.0)]).flex(1.0)]);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", body).status(word, tone).with_padding(8.0)
}

/// The NO POWER panel: a dark screen with the word centred.
pub fn no_power(title: &str, tag: &str, tier: Tier) -> Panel {
    let size = tier.canvas();
    let (w, h) = (size.x - 40.0, size.y - 80.0);
    // A ring above the word (drawn, not a glyph: the display font has no ○).
    let ring = Shape::Circle { cx: w * 0.5, cy: h * 0.5 - 36.0, r: 14.0, fill: None, stroke: Some((Ink::Fg3, 3.0)) };
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, ring, text(0.0, 12.0, w, h - 12.0, "NO POWER", 28.0, TextFace::DisplayBold, Ink::Fg3, true)];
    Panel::screen(tier, title, tag, canvas(w, h, shapes, None).flex(1.0))
}

/// Whether the machines have power on this snapshot (none yet: no).
fn powered(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.powered)
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

/// The pump's three mode buttons: the active one in the good tone, the others normal.
fn pump_modes(mode: PumpMode) -> Item {
    let mode_button = |label: &str, action: &str, m: PumpMode| button(label, action).tone(if mode == m { Tone::Good } else { Tone::Normal });
    row(6.0, vec![mode_button("OFF", "pump_off", PumpMode::Off).flex(1.0), mode_button("LOW", "pump_low", PumpMode::Low).flex(1.0), mode_button("HIGH", "pump_high", PumpMode::High).flex(1.0)]).fixed(ctl::MD)
}

/// The pump's picture (152 x 109): the inlet and the outlet pipes with the fluid, the housing and the
/// six-blade impeller, turned by `angle` degrees (`effects::impeller_angle`).
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

/// The coolant pump's screen (S) with the impeller at 0 degrees.
pub fn pump_panel(r: Option<&ReactorNews>) -> Panel {
    pump_panel_at(r, 0.0)
}

/// The coolant pump's screen (S): the picture with the impeller at `angle`, the flow and its word,
/// the switch.
pub fn pump_panel_at(r: Option<&ReactorNews>, angle: f64) -> Panel {
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
                    coolant_tube(&r).fixed(60.0),
                ],
            )
            .fixed(109.0),
            pump_modes(r.pump_mode),
            space(),
        ],
    );
    Panel::screen(Tier::S, PUMP_NAME, "CP-01", body).with_padding(6.0)
}

/// The coolant tank as a tube beside the flow (60 x 109): its level, LOW under the line (a warn
/// fill) and EMPTY at none (the word in bad). The sim's level and line (`ReactorNews::coolant`).
fn coolant_tube(r: &ReactorNews) -> Item {
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

/// One breaker lever (94 wide): its name, the slot, the handle at the top when on, at the bottom
/// when off. The whole lever is the button.
fn lever(lines: &[&str], on: bool, action: &str) -> Item {
    // The names are set in lines of 17 px, bottom-aligned on the slot's top (y 36): a name too wide
    // for the lever (94 px) is split over two lines rather than cut.
    let ink = if on { Ink::Fg1 } else { Ink::Fg3 };
    let mut s: Vec<Shape> = lines.iter().enumerate().map(|(i, l)| text(0.0, 36.0 - 17.0 * (lines.len() - i) as f64, 94.0, 17.0, l, 16.0, TextFace::Display, ink, true)).collect();
    s.push(Shape::Rect { x: 36.0, y: 36.0, w: 22.0, h: 124.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
    let hy = if on { 36.0 } else { 126.0 };
    s.push(Shape::Rect { x: 23.0, y: hy, w: 48.0, h: 34.0, fill: Some(if on { Ink::Hud } else { Ink::Warn }), outline: Some((Ink::FgTitle, 2.0)) });
    s.push(text(23.0, hy, 48.0, 34.0, if on { "ON" } else { "OFF" }, 16.0, TextFace::DisplayBold, Ink::Bg0, true));
    canvas(94.0, 168.0, s, Some(action)).flex(1.0)
}

/// The junction box's screen (S): three breaker levers.
pub fn junction_panel(r: Option<&ReactorNews>) -> Panel {
    if !powered(r) {
        return no_power(JUNCTION_NAME, "JB-01", Tier::S);
    }
    let b = r.map_or([false; 3], |r| r.breakers);
    let body = row(
        0.0,
        vec![
            lever(&["ENGINES"], b[BreakerGroup::Engines.index()], "br_engines"),
            lever(&["LIFE", "SUPPORT"], b[BreakerGroup::LifeSupport.index()], "br_life"),
            lever(&["LIGHTS"], b[BreakerGroup::Lights.index()], "br_lights"),
        ],
    );
    Panel::screen(Tier::S, JUNCTION_NAME, "JB-01", body).with_padding(6.0)
}

/// The oxygen's tone: bad at 40 % or less, warn at 70 % or less, else good.
pub fn oxygen_tone(o: f64) -> Tone {
    let p = o * 100.0;
    if p <= 40.0 {
        Tone::Bad
    } else if p <= 70.0 {
        Tone::Warn
    } else {
        Tone::Good
    }
}

/// The band under the temperature (200 wide): the zones as the gauge has them, the marker at the
/// temperature.
fn band(temp: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
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
    let m = temp.clamp(0.0, 100.0);
    s.push(Shape::Rect { x: x(m) - 2.0, y: 0.0, w: 4.0, h: 22.0, fill: Some(Ink::FgTitle), outline: None });
    canvas(200.0, 22.0, s, None).fixed(200.0)
}

/// The engine room's door screen (S): the reactor's temperature big, its band, the oxygen, the
/// lights breaker. TODO(captain): the emergency view comes with the captain role.
pub fn door_panel(r: Option<&ReactorNews>) -> Panel {
    if !powered(r) {
        return no_power(DOOR_NAME, "ER-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (optimal, allowed) = bands(Some(&r));
    let z = zone_tone(zone(r.temp, optimal, allowed));
    let (word, word_tone) = reactor_status(Some(&r));
    // The screen's content is 288 x 180 px: the rows fit it.
    let temp = row(6.0, vec![readout(format!("{:.0}", r.temp)).text_size(72.0).tone(z), readout("°C").text_size(28.0).tone(z)]).fixed(70.0);
    let bandrow = row(8.0, vec![band(r.temp, optimal, allowed), readout(word).text_size(font::SMALL).tone(word_tone).flex(1.0)]).fixed(18.0);
    // Each caption has its own line above its value or switch, so no caption touches a control.
    let air = row(
        8.0,
        vec![
            column(0.0, vec![micro("O₂").fixed(14.0), readout(format!("{:.0}%", r.oxygen * 100.0)).text_size(font::VALUE).tone(oxygen_tone(r.oxygen))]).flex(1.0),
            column(0.0, vec![micro("LIGHT").fixed(14.0), toggle(r.breakers[BreakerGroup::Lights.index()], "", "br_lights").labels("OFF", "ON").fixed(48.0)]).fixed(112.0),
        ],
    )
    .fixed(62.0);
    let body = column(4.0, vec![micro("REACTOR").fixed(14.0), temp, bandrow, air, space()]);
    Panel::screen(Tier::S, DOOR_NAME, "ER-01", body).with_padding(8.0)
}

/// The dev fault screen: the three faults, a fire at the reactor and the reset, one button a row.
pub fn faults_panel() -> Panel {
    // Two columns: the faults on the left, the fire and the resets on the right.
    let col = |items: Vec<_>| column(4.0, items);
    Panel::screen(
        Tier::S,
        FAULTS_NAME,
        "DEV-02",
        row(
            6.0,
            vec![
                col(vec![
                    button("Pump break", "pump_break").fixed(32.0),
                    button("Load drop", "load_drop").fixed(32.0),
                    button("Surge", "surge").fixed(32.0),
                    button("Drain tank", "drain_coolant").fixed(32.0),
                    space(),
                ]),
                col(vec![
                    button("Start fire", "fire").fixed(32.0),
                    button("Fire mode", "fire_model").fixed(32.0),
                    button("Reset RX", "reset").fixed(32.0),
                    space(),
                ]),
            ],
        ),
    )
}

/// A lamp's word and its ink when lit: warn, or bad for the critical ones.
fn lamp_word(l: Lamp) -> (&'static str, Ink) {
    match l {
        Lamp::Hot => ("HOT", Ink::Warn),
        Lamp::CritHeat => ("CRIT HEAT", Ink::Bad),
        Lamp::Cold => ("COLD", Ink::Warn),
        Lamp::LowTemp => ("LOW TEMP", Ink::Warn),
        Lamp::HighOut => ("HIGH OUT", Ink::Warn),
        Lamp::CritOut => ("CRIT OUT", Ink::Bad),
        Lamp::LowOut => ("LOW OUT", Ink::Warn),
        Lamp::FuelLow => ("FUEL LOW", Ink::Warn),
        Lamp::NoFuel => ("NO FUEL", Ink::Bad),
        Lamp::Meltdown => ("MELTDOWN", Ink::Bad),
        Lamp::Scram => ("SCRAM", Ink::Warn),
    }
}

/// One lamp: lit in its ink with the word dark on it, or dark with the word dim.
fn lamp_cell(l: Lamp, lit: bool) -> Item {
    let (word, ink) = lamp_word(l);
    let shapes = if lit {
        vec![Shape::Rect { x: 0.0, y: 0.0, w: LAMP_W, h: LAMP_H, fill: Some(ink), outline: None }, text(0.0, 0.0, LAMP_W, LAMP_H, word, 14.0, TextFace::Display, Ink::Bg0, true)]
    } else {
        vec![Shape::Rect { x: 0.0, y: 0.0, w: LAMP_W, h: LAMP_H, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }, text(0.0, 0.0, LAMP_W, LAMP_H, word, 14.0, TextFace::Display, Ink::Fg3, true)]
    };
    canvas(LAMP_W, LAMP_H, shapes, None).flex(1.0)
}

/// A countdown's clock: minutes and seconds, the seconds rounded up.
pub fn clock(s: f64) -> String {
    let s = s.max(0.0).ceil() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The RX STATUS panel (S, beside the readout): the warning lamps in a grid (the ones that are on
/// lit, the warn or bad ink; the 2 Hz blink is `blink_on`, SCRAM stays lit), the temperature boost
/// with its two buttons (disabled while a press would not pass), and the implosion's countdown
/// along the bottom while the meltdown timer runs.
pub fn status_panel(r: Option<&ReactorNews>, blink_on: bool) -> Panel {
    if !powered(r) {
        return no_power(STATUS_NAME, "RX-02", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let lit = |l: Lamp| r.lamps[l.index()] && (l == Lamp::Scram || blink_on);
    let mut grid = Vec::new();
    for chunk in LAMPS.chunks(3) {
        let mut cells: Vec<Item> = chunk.iter().map(|&l| lamp_cell(l, lit(l))).collect();
        while cells.len() < 3 {
            cells.push(space());
        }
        grid.push(row(LAMP_GAP, cells).fixed(LAMP_H));
    }
    let lamps = column(LAMP_GAP, grid).fixed(4.0 * LAMP_H + 3.0 * LAMP_GAP);
    let boost = row(8.0, vec![micro("BOOST").flex(1.0), readout(format!("{:+.0}", r.boost)).text_size(font::SMALL).tone(Tone::Normal)]).fixed(16.0);
    let presses = row(6.0, vec![button("UP", "boost_up").enabled(r.boost_ok).flex(1.0), button("DOWN", "boost_down").enabled(r.boost_ok).flex(1.0)]).fixed(ctl::SM);
    let countdown = match r.meltdown_eta_s {
        Some(s) => readout(format!("IMPLOSION {}", clock(s))).text_size(font::LABEL).tone(Tone::Bad).fixed(22.0),
        None => space().fixed(22.0),
    };
    Panel::screen(Tier::S, STATUS_NAME, "RX-02", column(5.0, vec![lamps, boost, presses, countdown])).with_padding(6.0)
}

/// The clicks: the readout asks for the reactor (the toggles give the new state as the value, the
/// sliders the value 0..1), the machines' screens for their switches, the fault screen for the
/// faults and the reset.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let new_state = |current: bool| c.value.map(|v| v >= 0.5).unwrap_or(!current);
        let ask = if c.panel == READOUT_ID {
            match c.action.as_str() {
                "mode" => snap.map(|r| ReactorAsk::SetManual(new_state(r.manual))),
                "power" => snap.map(|r| ReactorAsk::SetPower(new_state(r.power_on))),
                "scram" => Some(ReactorAsk::Scram),
                "fis_set" => c.value.map(|v| ReactorAsk::SetFission(v as f64 * 100.0)),
                "tur_set" => c.value.map(|v| ReactorAsk::SetTurbine(v as f64 * 100.0)),
                _ => None,
            }
        } else if c.panel == PUMP_ID {
            match c.action.as_str() {
                "pump_off" => Some(ReactorAsk::SetPumpMode(PumpMode::Off)),
                "pump_low" => Some(ReactorAsk::SetPumpMode(PumpMode::Low)),
                "pump_high" => Some(ReactorAsk::SetPumpMode(PumpMode::High)),
                _ => None,
            }
        } else if c.panel == JUNCTION_ID || c.panel == DOOR_ID {
            let group = match c.action.as_str() {
                "br_engines" => Some(BreakerGroup::Engines),
                "br_life" => Some(BreakerGroup::LifeSupport),
                "br_lights" => Some(BreakerGroup::Lights),
                _ => None,
            };
            group.and_then(|g| snap.map(|r| ReactorAsk::SetBreaker(g, new_state(r.breakers[g.index()]))))
        } else if c.panel == STATUS_ID {
            match c.action.as_str() {
                "boost_up" => Some(ReactorAsk::Boost(true)),
                "boost_down" => Some(ReactorAsk::Boost(false)),
                _ => None,
            }
        } else if c.panel == FAULTS_ID {
            match c.action.as_str() {
                "pump_break" => Some(ReactorAsk::Trigger(ReactorFault::PumpBreak)),
                "load_drop" => Some(ReactorAsk::Trigger(ReactorFault::LoadDrop)),
                "surge" => Some(ReactorAsk::Trigger(ReactorFault::Surge)),
                "fire" => Some(ReactorAsk::Trigger(ReactorFault::Fire)),
                "reset" => Some(ReactorAsk::Reset),
                "drain_coolant" => Some(ReactorAsk::Trigger(ReactorFault::DrainCoolant)),
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

/// The impeller's angle is kept in steps of 6 degrees, so the panel is only redrawn when it shows.
const IMPELLER_STEP: f64 = 6.0;

/// Every panel from the state, every tick (`Panels::set` skips an unchanged one). The impeller
/// turns with the pump's flow (`effects::impeller_angle`), from the fixed step's time.
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, time: Res<Time>, mut spin: Local<f64>, mut panels: ResMut<Panels>) {
    let speed = items.reactor.map_or(0.0, |r| r.pump_speed);
    *spin = crate::effects::impeller_angle(*spin, speed, time.delta_secs_f64());
    let angle = (*spin / IMPELLER_STEP).round() * IMPELLER_STEP;
    panels.set(READOUT_ID, readout_panel(items.reactor.as_ref(), cfg.0.reactor.max_output_kw));
    panels.set(PUMP_ID, pump_panel_at(items.reactor.as_ref(), angle));
    panels.set(JUNCTION_ID, junction_panel(items.reactor.as_ref()));
    panels.set(DOOR_ID, door_panel(items.reactor.as_ref()));
    panels.set(FAULTS_ID, faults_panel());
    panels.set(STATUS_ID, status_panel(items.reactor.as_ref(), lamp_blink_on(time.elapsed_secs_f64())));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The readout at the shipped scale (`reactor.json`'s `max_output_kw`).
    fn shipped_panel(r: Option<&ReactorNews>) -> Panel {
        let shipped = reactor_core::Tuning::parse(include_str!("../../../../content/gameplay/reactor/reactor.json")).expect("the shipped reactor.json");
        readout_panel(r, shipped.reactor.max_output_kw)
    }

    /// The header's status word and tone of a panel with a plate.
    fn header(p: &Panel) -> Option<(String, Tone)> {
        p.plate.as_ref()?.status.clone()
    }

    fn news() -> ReactorNews {
        ReactorNews { powered: true, power_on: true, pump_on: true, pump_mode: PumpMode::High, pump_speed: 1.0, flow: 1.0, breakers: [true; 3], optimal: [35.0, 65.0], allowed: [20.0, 80.0], temp: 54.0, output_kw: 500.0, load_kw: 500.0, fuel: [80.0, 12.0], ..Default::default() }
    }

    fn all_panels() -> Vec<Panel> {
        let mut v = Vec::new();
        for r in [None, Some(ReactorNews::default()), Some(ReactorNews { manual: true, ..news() }), Some(ReactorNews { melted: true, ..news() }), Some(ReactorNews { temp: 85.0, ..news() })] {
            v.push(shipped_panel(r.as_ref()));
            v.push(pump_panel(r.as_ref()));
            v.push(junction_panel(r.as_ref()));
            v.push(door_panel(r.as_ref()));
        }
        for flow in [0.0, 0.5, 1.0] {
            v.push(pump_panel(Some(&ReactorNews { flow, ..news() })));
        }
        // The coolant tube: full, LOW (warn) and EMPTY (bad).
        for (coolant, coolant_low) in [(100.0, false), (20.0, true), (0.0, true)] {
            v.push(pump_panel(Some(&ReactorNews { coolant, coolant_low, ..news() })));
        }
        for temp in [10.0, 40.0, 60.0, 72.0, 95.0] {
            v.push(door_panel(Some(&ReactorNews { temp, ..news() })));
        }
        v.push(junction_panel(Some(&ReactorNews { overvolt: true, ..news() })));
        for blink in [false, true] {
            for r in [None, Some(status_news()), Some(ReactorNews { lamps: [true; Lamp::COUNT], meltdown_eta_s: Some(42.0), ..status_news() }), Some(ReactorNews { powered: false, ..status_news() })] {
                v.push(status_panel(r.as_ref(), blink));
            }
        }
        v.push(faults_panel());
        v
    }

    #[test]
    fn no_widget_and_no_shape_of_any_screen_overflows() {
        for (i, p) in all_panels().into_iter().enumerate() {
            assert!(p.overflowing().is_empty(), "panel {i}: {:?}", p.overflowing());
        }
    }

    /// The lit lamps of a status panel: the words of the lamps drawn in the warn or bad ink.
    fn lit_lamps(p: &Panel) -> Vec<String> {
        let mut out = Vec::new();
        for w in p.layout() {
            if let display_core::Widget::Canvas { shapes, .. } = w.widget {
                let lit = shapes.iter().any(|s| matches!(s, Shape::Rect { fill: Some(Ink::Warn | Ink::Bad), .. }));
                if lit {
                    for s in shapes {
                        if let Shape::Text { text, .. } = s {
                            out.push(text.clone());
                        }
                    }
                }
            }
        }
        out
    }

    fn status_news() -> ReactorNews {
        ReactorNews { lamps: [false; Lamp::COUNT], boost_ok: true, ..news() }
    }

    #[test]
    fn the_status_board_lights_the_lamps_of_the_news() {
        let mut r = status_news();
        r.lamps[Lamp::Hot.index()] = true;
        r.lamps[Lamp::NoFuel.index()] = true;
        r.lamps[Lamp::Scram.index()] = true;
        let lit = lit_lamps(&status_panel(Some(&r), true));
        assert!(lit.contains(&"HOT".to_string()) && lit.contains(&"NO FUEL".to_string()) && lit.contains(&"SCRAM".to_string()), "{lit:?}");
        assert_eq!(lit.len(), 3, "only the three lamps on: {lit:?}");
    }

    #[test]
    fn the_lamps_blink_but_scram_stays_lit() {
        let mut r = status_news();
        r.lamps[Lamp::Cold.index()] = true;
        r.lamps[Lamp::Scram.index()] = true;
        let dark = lit_lamps(&status_panel(Some(&r), false));
        assert_eq!(dark, vec!["SCRAM".to_string()], "the blink's off phase: only SCRAM is lit");
        assert_eq!(lit_lamps(&status_panel(Some(&r), true)).len(), 2, "the blink's on phase: both lit");
    }

    #[test]
    fn the_boost_buttons_are_disabled_when_a_press_would_not_pass() {
        let p = status_panel(Some(&status_news()), true);
        assert!(p.find("boost_up").is_some_and(|b| matches!(b.widget, display_core::Widget::Button { enabled: true, .. })));
        let off = status_panel(Some(&ReactorNews { boost_ok: false, ..status_news() }), true);
        for a in ["boost_up", "boost_down"] {
            assert!(matches!(off.find(a).unwrap().widget, display_core::Widget::Button { enabled: false, .. }), "{a}");
        }
    }

    #[test]
    fn the_implosion_countdown_shows_only_with_an_eta() {
        let label = |p: &Panel, t: &str| p.layout().into_iter().any(|w| matches!(&w.widget, display_core::Widget::Label { text, .. } if text == t));
        let counting = status_panel(Some(&ReactorNews { meltdown_eta_s: Some(42.0), ..status_news() }), true);
        assert!(label(&counting, "IMPLOSION 0:42"));
        assert!(!label(&status_panel(Some(&status_news()), true), "IMPLOSION 0:42"), "no countdown without an eta");
        assert_eq!(clock(125.0), "2:05");
        assert_eq!(clock(-1.0), "0:00");
    }

    #[test]
    fn the_status_board_says_no_power_and_has_nothing_to_press_without_power() {
        let off = status_panel(Some(&ReactorNews { powered: false, ..status_news() }), true);
        assert!(off.find("boost_up").is_none() && off.find("boost_down").is_none());
        assert!(off.overflowing().is_empty());
        assert!(status_panel(None, true).find("boost_up").is_none());
    }

    #[test]
    fn every_button_label_fits_its_button() {
        // The draw sets a button's label at LABEL (20 px) or 55 % of the button's height, in caps
        // with the caps tracking: about 0.72 of the size a letter. It must fit the button's width.
        for (i, p) in all_panels().into_iter().enumerate() {
            for w in p.layout() {
                if let display_core::Widget::Button { label, .. } = w.widget {
                    let size = display_core::font::LABEL.min(w.rect.size.y * 0.55);
                    let width = label.to_uppercase().chars().count() as f64 * size * 0.72;
                    assert!(width <= w.rect.size.x, "panel {i}: {label:?} is {width:.0} px, its button {:.0} px", w.rect.size.x);
                }
            }
        }
    }

    #[test]
    fn every_readout_fits_its_box() {
        // A readout is set in the mono font: at most 0.6 of its size a character (the font's advance
        // is 0.54; the flow's 28 px digits were clipped on the screen, so the flow is 20 px now).
        let mut panels = all_panels();
        panels.push(pump_panel(Some(&ReactorNews { flow: 1.0, coolant: 100.0, ..news() })));
        for (i, p) in panels.into_iter().enumerate() {
            for w in p.layout() {
                if let display_core::Widget::Label { text, size, mono: true, .. } = w.widget {
                    let width = text.chars().count() as f64 * size * 0.6;
                    assert!(width <= w.rect.size.x, "panel {i}: {text:?} is {width:.0} px, its box {:.0} px", w.rect.size.x);
                }
            }
        }
    }

    #[test]
    fn the_manual_sliders_are_only_on_when_manual() {
        let auto = shipped_panel(Some(&news()));
        assert!(matches!(auto.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: false, .. }));
        let man = shipped_panel(Some(&ReactorNews { manual: true, ..news() }));
        assert!(matches!(man.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: true, .. }));
        assert!(man.find("mode").is_some() && man.find("reset").is_none());
        assert!(faults_panel().find("reset").is_some() && faults_panel().find("pump_break").is_some());
    }

    #[test]
    fn the_readout_has_power_mode_and_scram() {
        let p = shipped_panel(Some(&news()));
        assert!(p.find("power").is_some() && p.find("scram").is_some() && p.find("mode").is_some());
        for gone in ["fire_timer", "meltdown_timer", "pump", "junction", "alarm", "reset"] {
            assert!(p.find(gone).is_none(), "{gone}");
        }
    }

    #[test]
    fn the_zones_are_the_sim_bands() {
        // The news' bands at skill 50: optimal 35..65, allowed 20..80.
        let (o, a) = ([35.0, 65.0], [20.0, 80.0]);
        assert_eq!(zone(15.0, o, a), Zone::Cold);
        assert_eq!(zone(25.0, o, a), Zone::Low);
        assert_eq!(zone(54.0, o, a), Zone::Good);
        assert_eq!(zone(70.0, o, a), Zone::High);
        assert_eq!(zone(85.0, o, a), Zone::Crit);
        assert_eq!(reactor_status(Some(&news())).0, "STABLE");
        assert_eq!(reactor_status(Some(&ReactorNews { temp: 70.0, ..news() })).0, "HOT");
        assert_eq!(reactor_status(Some(&ReactorNews { temp: 85.0, ..news() })).0, "CRITICAL");
        assert_eq!(reactor_status(Some(&ReactorNews { power_on: false, ..news() })).0, "OFF");
        // A skilled operator's wider bands move the words with them: 70 is inside the optimal band.
        let skilled = ReactorNews { optimal: [30.0, 70.0], allowed: [10.0, 90.0], temp: 70.0, ..news() };
        assert_eq!(reactor_status(Some(&skilled)).0, "STABLE");
        assert_eq!(reactor_status(Some(&ReactorNews { temp: 85.0, ..skilled })).0, "HOT");
        // Without news the skill-50 bands.
        assert_eq!(bands(None), ([35.0, 65.0], [20.0, 80.0]));
        assert_eq!(bands(Some(&ReactorNews::default())), ([35.0, 65.0], [20.0, 80.0]));
    }

    #[test]
    fn the_critical_readout_keeps_its_controls_and_says_critical() {
        // Above the allowed band with the power on: the same readout with the controls, the header
        // CRITICAL, as the lamp is lit.
        let hot = ReactorNews { temp: 85.0, ..news() };
        assert!(crate::effects::is_critical(Some(&hot)));
        let crit = shipped_panel(Some(&hot));
        for a in ["scram", "power", "mode", "fis_set", "tur_set"] {
            assert!(crit.find(a).is_some(), "{a} while critical");
        }
        assert_eq!(header(&crit), Some(("CRITICAL".to_string(), Tone::Bad)));
        assert!(crit.overflowing().is_empty());
        // Scrammed at 85 °C: not critical any more, the header OFF (the needle shows the hot core).
        let off = ReactorNews { power_on: false, ..hot };
        assert!(!crate::effects::is_critical(Some(&off)));
        let normal = shipped_panel(Some(&off));
        assert!(normal.find("fis_set").is_some(), "scrammed: the normal view");
        assert_eq!(reactor_status(Some(&off)), ("OFF", Tone::Dim));
        assert_eq!(header(&normal), Some(("OFF".to_string(), Tone::Dim)));
    }

    #[test]
    fn critical_is_the_same_at_every_temperature_on_the_readout_and_the_lamp() {
        // A widened band: 85 is HOT (not CRITICAL) for this operator, on the readout and the lamp.
        for (allowed, temp) in [([20.0, 80.0], 85.0), ([10.0, 90.0], 85.0), ([10.0, 90.0], 95.0)] {
            let r = ReactorNews { allowed, temp, ..news() };
            let p = shipped_panel(Some(&r));
            let critical = crate::effects::is_critical(Some(&r));
            assert_eq!(header(&p).is_some_and(|h| h.0 == "CRITICAL"), critical, "allowed {allowed:?}, temp {temp}");
            assert_eq!(reactor_status(Some(&r)).0 == "CRITICAL", critical, "word at {temp} with {allowed:?}");
            assert!(p.find("fis_set").is_some() && p.find("scram").is_some(), "the controls stay at {temp}");
        }
    }

    #[test]
    fn a_melted_core_says_meltdown_without_a_button() {
        let p = shipped_panel(Some(&ReactorNews { melted: true, ..news() }));
        assert!(p.find("scram").is_none() && p.find("power").is_none() && p.find("fis_set").is_none());
        assert_eq!(header(&p), Some(("MELTDOWN".to_string(), Tone::Bad)));
        assert!(p.overflowing().is_empty());
    }

    #[test]
    fn the_power_gauge_colours_by_the_shortage() {
        assert_eq!(power_ink(0.0, 500.0), Ink::Fg3);
        assert_eq!(power_ink(480.0, 500.0), Ink::Good, "4 % short");
        assert_eq!(power_ink(400.0, 500.0), Ink::Warn, "20 % short");
        assert_eq!(power_ink(200.0, 500.0), Ink::Bad, "60 % short");
    }

    #[test]
    fn the_coolant_tube_says_low_and_empty_and_the_drain_is_a_fault() {
        let low = pump_panel(Some(&ReactorNews { coolant: 20.0, coolant_low: true, ..news() }));
        let empty = pump_panel(Some(&ReactorNews { coolant: 0.0, coolant_low: true, ..news() }));
        let full = pump_panel(Some(&ReactorNews { coolant: 100.0, ..news() }));
        assert_ne!(format!("{low:?}"), format!("{full:?}"), "a low tank is drawn");
        assert_ne!(format!("{empty:?}"), format!("{low:?}"), "an empty tank says EMPTY");
        assert!(faults_panel().find("drain_coolant").is_some());
    }

    #[test]
    fn the_pump_words_follow_the_flow_and_the_switch() {
        assert_eq!(pump_status(PumpMode::High, 100.0).0, "HIGH");
        assert_eq!(pump_status(PumpMode::Low, 100.0).0, "LOW");
        assert_eq!(pump_status(PumpMode::High, 0.0).0, "DRY");
        assert_eq!(pump_status(PumpMode::Off, 0.0).0, "OFF");
        assert_eq!(pump_status(PumpMode::Off, 100.0).0, "OFF");
        let p = pump_panel(Some(&news()));
        for a in ["pump_off", "pump_low", "pump_high"] {
            assert!(p.find(a).is_some(), "{a}");
        }
        assert!(p.find("br_lights").is_none());
    }

    #[test]
    fn the_active_pump_mode_is_the_selected_button() {
        let p = pump_panel(Some(&ReactorNews { pump_mode: PumpMode::Low, ..news() }));
        let tone = |a: &str| match &p.find(a).expect(a).widget {
            display_core::Widget::Button { tone, .. } => *tone,
            _ => panic!("{a} is a button"),
        };
        assert_eq!((tone("pump_off"), tone("pump_low"), tone("pump_high")), (Tone::Normal, Tone::Good, Tone::Normal));
    }

    #[test]
    fn the_junction_box_has_three_levers_and_nothing_else() {
        let p = junction_panel(Some(&news()));
        for a in ["br_engines", "br_life", "br_lights"] {
            assert!(p.find(a).is_some(), "{a}");
        }
        assert!(matches!(p.find("br_lights").unwrap().widget, display_core::Widget::Canvas { action: Some(_), .. }));
    }

    #[test]
    fn the_names_on_the_screens_are_labels_and_do_not_collide() {
        let label = |p: &Panel, t: &str| p.layout().into_iter().find(|w| matches!(&w.widget, display_core::Widget::Label { text, .. } if text == t)).map(|w| w.rect);
        assert!(label(&pump_panel(Some(&news())), "FLOW").is_some(), "the pump names its flow");
        // The door's caption sits above its switch, not on it.
        let door = door_panel(Some(&news()));
        let (cap, sw) = (label(&door, "LIGHT").expect("LIGHT caption"), door.find("br_lights").expect("the switch").rect);
        assert!(cap.max().y <= sw.min.y, "the caption is above the switch: {cap:?} {sw:?}");
        // The junction's lever names: above the slot, apart from each other, and inside their lever.
        let junction = junction_panel(Some(&news()));
        for a in ["br_engines", "br_life", "br_lights"] {
            let display_core::Widget::Canvas { shapes, .. } = &junction.find(a).unwrap().widget else { panic!("{a} is a canvas") };
            let names: Vec<(f64, f64, f64, f64, &str, f64)> = shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Text { x, y, w, h, text, size, .. } if *y < 36.0 => Some((*x, *y, *w, *h, text.as_str(), *size)),
                    _ => None,
                })
                .collect();
            assert!(!names.is_empty(), "{a} has its name");
            for (i, n) in names.iter().enumerate() {
                assert!(n.1 + n.3 <= 36.0, "{a}: {} sits above the slot", n.4);
                assert!(n.4.len() as f64 * n.5 * 0.65 <= n.2, "{a}: {} fits the lever ({} px)", n.4, n.2);
                for m in &names[i + 1..] {
                    assert!(n.1 + n.3 <= m.1 || m.1 + m.3 <= n.1, "{a}: {} and {} do not overlap", n.4, m.4);
                }
            }
        }
    }

    #[test]
    fn the_door_screen_has_the_lights_and_no_fire() {
        let p = door_panel(Some(&news()));
        assert!(p.find("br_lights").is_some());
        assert!(p.find("fire").is_none() && p.find("reset").is_none());
    }

    #[test]
    fn unpowered_machines_show_no_power_and_nothing_to_press() {
        let off = Some(ReactorNews { powered: false, ..news() });
        for p in [pump_panel(off.as_ref()), junction_panel(off.as_ref()), door_panel(off.as_ref()), pump_panel(None)] {
            assert!(p.find("pump").is_none() && p.find("br_lights").is_none() && p.find("br_engines").is_none());
            assert!(p.overflowing().is_empty());
        }
    }

    #[test]
    fn the_oxygen_tone_has_the_spec_limits() {
        assert_eq!(oxygen_tone(0.40), Tone::Bad);
        assert_eq!(oxygen_tone(0.70), Tone::Warn);
        assert_eq!(oxygen_tone(0.98), Tone::Good);
    }
}
