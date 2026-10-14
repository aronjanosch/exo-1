//! The engine room's screens (#297, slice S3c), drawn to the initiator's design (Claude Design,
//! "EXO-1 Crew HUD", spec `engine-room-screens-spec.md` in the session scratchpad). The screens show
//! little, in the ship's fiction: no condition values, sensors instead.
//!
//! - The reactor (M, on the east wall): the core temperature arc (zones 30/45/65/80), the output
//!   against the load, two fuel tubes, the fission and turbine sliders, MAN/AUTO, POWER, SCRAM. Above
//!   80 with the power on it is the CORE CRITICAL view; after a meltdown it says MELTDOWN. The readout
//!   has its own start-up supply, so POWER can come back after a SCRAM even with the machines dark
//!   (TODO(initiator): confirm).
//! - The coolant pump (S, on the pump block): a picture of the pump, the flow, the status word, the
//!   switch. The impeller is static (TODO: it turns with the flow).
//! - The junction box (S, on the wall above it): three breaker levers, nothing else. Overvolt shows
//!   in the world (sparks), not here.
//! - The reactor room's door screen (S, in the corridor by the door): the reactor temperature, its
//!   zone band, the room's oxygen, the lights breaker. TODO(captain): the emergency view with the
//!   captain role.
//! - The dev faults screen (S, next to the test console): always powered.
//!
//! The machines' screens show NO POWER unless the snapshot says `powered`. Every process shows the
//! snapshot it has (`Items::reactor`); the clicks ask the host (`ReactorAsk`).
//!
//! TODO(initiator): the zone limits are the skill-50 bands (30/45/65/80) as the spec has them; the
//! output scale is the room's maximum, 1000 kW (`ReactorTuning`; the spec drew 5000).
//! TODO(initiator): the glow of the critical text and the scanlines' look are not drawn yet.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::items::Items;
use display_core::{button, canvas, column, ctl, font, micro, readout, row, slider, space, toggle, Ink, Item, Panel, Shape, Tier, TextFace, Tone};
use item_core::wire::{BreakerGroup, ReactorAsk, ReactorFault, ReactorNews};

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
/// The dev fault screen's place next to the test console (zone local metres, as `PlaceholderBox`).
pub const FAULTS_AT: [f64; 2] = [1.9, -4.5];
/// The output scale of the power gauge, kW (the spec's).
pub const POWER_MAX_KW: f64 = 1000.0;

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

// ---------- the gauge's geometry ----------

/// The arc's start (bottom left) and its sweep (270 degrees, clockwise on screen).
const ARC_START: f64 = 135.0;
const ARC_SWEEP: f64 = 270.0;

/// The angle of a value 0..100 on the arc.
fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * v.clamp(0.0, 100.0) / 100.0
}

/// The temperature zones: the limits (30, 45, 65, 80) of the spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Cold,
    Low,
    Good,
    High,
    Crit,
}

pub fn zone(t: f64) -> Zone {
    if t < 30.0 {
        Zone::Cold
    } else if t < 45.0 {
        Zone::Low
    } else if t <= 65.0 {
        Zone::Good
    } else if t < 80.0 {
        Zone::High
    } else {
        Zone::Crit
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
fn temp_gauge(temp: f64) -> Item {
    let (cx, cy, r, sw) = (108.0, 70.0, 60.0, 14.0);
    let z = zone(temp);
    let t = temp.clamp(0.0, 100.0);
    let mut s = vec![
        arc(cx, cy, r, angle(0.0), angle(30.0), sw, Ink::Line2, None),
        arc(cx, cy, r, angle(30.0), angle(45.0), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(45.0), angle(65.0), sw, Ink::Good, None),
        arc(cx, cy, r, angle(65.0), angle(80.0), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(80.0), angle(100.0), sw, Ink::Bad, Some((3.0, 2.0))),
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
fn power_gauge(r: Option<&ReactorNews>) -> Item {
    let (cx, cy, rr, sw) = (108.0, 70.0, 60.0, 14.0);
    let (out, load) = r.map_or((0.0, 0.0), |r| (r.output_kw, r.load_kw));
    let frac = (out / POWER_MAX_KW * 100.0).clamp(0.0, 100.0);
    let ink = power_ink(out, load);
    let mut s = vec![arc(cx, cy, rr, angle(0.0), angle(100.0), sw, Ink::Bg3, None)];
    if frac > 0.0 {
        s.push(arc(cx, cy, rr, angle(0.0), angle(frac), sw, ink, None));
    }
    let n = (load / POWER_MAX_KW * 100.0).clamp(0.0, 100.0);
    let (a, p0, p1) = (angle(n).to_radians(), 50.0, 70.0);
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
        Some(r) if r.power_on => match zone(r.temp) {
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

/// The critical view (CORE CRITICAL with the temperature, or MELTDOWN): the stripes, the words and,
/// with `scram`, the full-width SCRAM button (no button after a meltdown).
fn critical(r: &ReactorNews, melted: bool) -> Panel {
    let words = if melted {
        vec![text(0.0, 60.0, 624.0, 72.0, "MELTDOWN", 56.0, TextFace::DisplayBold, Ink::Bad, true)]
    } else {
        vec![text(0.0, 14.0, 624.0, 72.0, "CORE CRITICAL", 56.0, TextFace::DisplayBold, Ink::Bad, true), text(0.0, 96.0, 624.0, 48.0, &format!("{:.0} °C", r.temp), 40.0, TextFace::Mono, Ink::Bad, true)]
    };
    let mut items = vec![hazard(624.0), canvas(624.0, 200.0, words, None).fixed(200.0)];
    if !melted {
        items.push(button("SCRAM", "scram").tone(Tone::Bad).fixed(ctl::LG));
    }
    items.push(hazard(624.0));
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", column(8.0, items)).status(if melted { "MELTDOWN" } else { "CRITICAL" }, Tone::Bad).with_padding(8.0)
}

/// The reactor readout (M): the top row of gauges and fuel, the sliders and the switches below.
/// Above 80 with the power on it is the critical view; after a meltdown MELTDOWN. The status word
/// in the header.
pub fn readout_panel(r: Option<&ReactorNews>) -> Panel {
    if let Some(r) = r.filter(|r| r.melted) {
        return critical(r, true);
    }
    if let Some(r) = r.filter(|r| r.power_on && r.temp >= 80.0) {
        return critical(r, false);
    }
    let on = r.is_some_and(|r| r.power_on);
    let manual = r.is_some_and(|r| r.manual);
    let (fis, tur) = r.map_or((0.0, 0.0), |r| (r.set_fission, r.set_turbine));
    let (word, tone) = reactor_status(r);
    let bottom_left = column(
        10.0,
        vec![
            row(8.0, vec![micro("FISSION RATE").flex(1.0), readout(format!("{fis:.0} %")).text_size(font::LABEL).tone(if manual { Tone::Normal } else { Tone::Dim })]).fixed(24.0),
            slider(fis / 100.0, "", "fis_set").enabled(manual).fixed(ctl::MD),
            row(8.0, vec![micro("TURBINE OUTPUT").flex(1.0), readout(format!("{tur:.0} %")).text_size(font::LABEL).tone(if manual { Tone::Normal } else { Tone::Dim })]).fixed(24.0),
            slider(tur / 100.0, "", "tur_set").enabled(manual).fixed(ctl::MD),
            space(),
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
    let gauges = row(8.0, vec![temp_gauge(r.map_or(0.0, |r| r.temp)), power_gauge(r), fuel(r)]).fixed(152.0);
    let body = column(8.0, vec![gauges, row(16.0, vec![bottom_left.flex(1.0), switches.fixed(200.0)]).flex(1.0)]);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", body).status(word, tone).with_padding(8.0)
}

/// The NO POWER panel: a dark screen with the word centred.
pub fn no_power(title: &str, tag: &str, tier: Tier) -> Panel {
    let size = tier.canvas();
    let (w, h) = (size.x - 40.0, size.y - 80.0);
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, text(0.0, 0.0, w, h, "○  NO POWER", 28.0, TextFace::DisplayBold, Ink::Fg3, true)];
    Panel::screen(tier, title, tag, canvas(w, h, shapes, None).flex(1.0))
}

/// Whether the machines have power on this snapshot (none yet: no).
fn powered(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.powered)
}

/// The pump's status word and its tone: OFF when switched off, STALLED at no flow, LOW FLOW below
/// 60 %, RUNNING above.
pub fn pump_status(pump_on: bool, flow: f64) -> (&'static str, Tone) {
    if !pump_on {
        ("OFF", Tone::Dim)
    } else if flow <= 0.0 {
        ("STALLED", Tone::Bad)
    } else if flow < 0.6 {
        ("LOW FLOW", Tone::Warn)
    } else {
        ("RUNNING", Tone::Good)
    }
}

/// The pump's picture (152 x 109): the inlet and the outlet pipes with the fluid, the housing and the
/// six-blade impeller (static: TODO, it turns with the flow).
fn pump_picture(flow_on: bool) -> Item {
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
        let a = (k as f64 * 60.0).to_radians();
        s.push(Shape::Line { a: [cx, cy], b: [cx + 30.0 * a.cos(), cy + 30.0 * a.sin()], width: 6.0, ink: Ink::Hud, dash: None });
    }
    s.push(Shape::Circle { cx, cy, r: 9.0, fill: Some(Ink::Hud), stroke: None });
    canvas(152.0, 109.0, s, None).fixed(152.0)
}

/// The coolant pump's screen (S): the picture, the flow and its word, the switch.
pub fn pump_panel(r: Option<&ReactorNews>) -> Panel {
    if !powered(r) {
        return no_power(PUMP_NAME, "CP-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (word, tone) = pump_status(r.pump_on, r.flow);
    let body = column(
        8.0,
        vec![
            row(
                10.0,
                vec![
                    pump_picture(r.pump_on && r.flow > 0.0),
                    column(4.0, vec![micro("FLOW"), readout(format!("{:.0}%", r.flow * 100.0)).text_size(font::VALUE).tone(tone), readout(word).text_size(font::LABEL).tone(tone)]).flex(1.0),
                ],
            )
            .fixed(109.0),
            toggle(r.pump_on, "", "pump").labels("OFF", "ON").fixed(ctl::MD),
            space(),
        ],
    );
    Panel::screen(Tier::S, PUMP_NAME, "CP-01", body).with_padding(6.0)
}

/// One breaker lever (94 wide): its name, the slot, the handle at the top when on, at the bottom
/// when off. The whole lever is the button.
fn lever(name: &str, on: bool, action: &str) -> Item {
    let mut s = vec![
        text(0.0, 0.0, 94.0, 20.0, name, 16.0, TextFace::Display, if on { Ink::Fg1 } else { Ink::Fg3 }, true),
        Shape::Rect { x: 36.0, y: 36.0, w: 22.0, h: 124.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
    ];
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
            lever("ENGINES", b[BreakerGroup::Engines.index()], "br_engines"),
            lever("LIFE SUPPORT", b[BreakerGroup::LifeSupport.index()], "br_life"),
            lever("LIGHTS", b[BreakerGroup::Lights.index()], "br_lights"),
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
fn band(temp: f64) -> Item {
    let x = |v: f64| v * 2.0;
    let seg = |a: f64, b: f64, ink: Option<Ink>, fill: Ink| {
        let mut v = vec![Shape::Rect { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, fill: Some(fill), outline: None }];
        if let Some(i) = ink {
            v.push(Shape::Hatch { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, ink: i });
        }
        v
    };
    let mut s = Vec::new();
    s.extend(seg(0.0, 30.0, None, Ink::Line2));
    s.extend(seg(30.0, 45.0, Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(45.0, 65.0, None, Ink::GoodDim));
    s.extend(seg(65.0, 80.0, Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(80.0, 100.0, Some(Ink::Bad), Ink::Bg3));
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
    let z = zone_tone(zone(r.temp));
    let (word, word_tone) = reactor_status(Some(&r));
    // The screen's content is 288 x 180 px: the rows fit it.
    let temp = row(6.0, vec![readout(format!("{:.0}", r.temp)).text_size(72.0).tone(z), readout("°C").text_size(28.0).tone(z)]).fixed(70.0);
    let bandrow = row(8.0, vec![band(r.temp), readout(word).text_size(font::SMALL).tone(word_tone).flex(1.0)]).fixed(18.0);
    let air = row(
        8.0,
        vec![
            column(0.0, vec![micro("O₂").fixed(14.0), readout(format!("{:.0}%", r.oxygen * 100.0)).text_size(font::VALUE).tone(oxygen_tone(r.oxygen))]).flex(1.0),
            toggle(r.breakers[BreakerGroup::Lights.index()], "LIGHT", "br_lights").labels("OFF", "ON").fixed(112.0),
        ],
    )
    .fixed(50.0);
    let body = column(5.0, vec![micro("REACTOR").fixed(14.0), temp, bandrow, air, space()]);
    Panel::screen(Tier::S, DOOR_NAME, "ER-01", body).with_padding(8.0)
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
        } else if c.panel == PUMP_ID && c.action == "pump" {
            snap.map(|r| ReactorAsk::SetPump(new_state(r.pump_on)))
        } else if c.panel == JUNCTION_ID || c.panel == DOOR_ID {
            let group = match c.action.as_str() {
                "br_engines" => Some(BreakerGroup::Engines),
                "br_life" => Some(BreakerGroup::LifeSupport),
                "br_lights" => Some(BreakerGroup::Lights),
                _ => None,
            };
            group.and_then(|g| snap.map(|r| ReactorAsk::SetBreaker(g, new_state(r.breakers[g.index()]))))
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

/// Every panel from the state, every tick (`Panels::set` skips an unchanged one).
pub fn write_panels(items: Res<Items>, mut panels: ResMut<Panels>) {
    panels.set(READOUT_ID, readout_panel(items.reactor.as_ref()));
    panels.set(PUMP_ID, pump_panel(items.reactor.as_ref()));
    panels.set(JUNCTION_ID, junction_panel(items.reactor.as_ref()));
    panels.set(DOOR_ID, door_panel(items.reactor.as_ref()));
    panels.set(FAULTS_ID, faults_panel());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn news() -> ReactorNews {
        ReactorNews { powered: true, power_on: true, pump_on: true, flow: 1.0, breakers: [true; 3], optimal: [35.0, 65.0], allowed: [20.0, 80.0], temp: 54.0, output_kw: 500.0, load_kw: 500.0, fuel: [80.0, 12.0], ..Default::default() }
    }

    fn all_panels() -> Vec<Panel> {
        let mut v = Vec::new();
        for r in [None, Some(ReactorNews::default()), Some(ReactorNews { manual: true, ..news() }), Some(ReactorNews { melted: true, ..news() }), Some(ReactorNews { temp: 85.0, ..news() })] {
            v.push(readout_panel(r.as_ref()));
            v.push(pump_panel(r.as_ref()));
            v.push(junction_panel(r.as_ref()));
            v.push(door_panel(r.as_ref()));
        }
        for flow in [0.0, 0.5, 1.0] {
            v.push(pump_panel(Some(&ReactorNews { flow, ..news() })));
        }
        for temp in [10.0, 40.0, 60.0, 72.0, 95.0] {
            v.push(door_panel(Some(&ReactorNews { temp, ..news() })));
        }
        v.push(junction_panel(Some(&ReactorNews { overvolt: true, ..news() })));
        v.push(faults_panel());
        v
    }

    #[test]
    fn no_widget_and_no_shape_of_any_screen_overflows() {
        for (i, p) in all_panels().into_iter().enumerate() {
            assert!(p.overflowing().is_empty(), "panel {i}: {:?}", p.overflowing());
        }
    }

    #[test]
    fn the_manual_sliders_are_only_on_when_manual() {
        let auto = readout_panel(Some(&news()));
        assert!(matches!(auto.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: false, .. }));
        let man = readout_panel(Some(&ReactorNews { manual: true, ..news() }));
        assert!(matches!(man.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: true, .. }));
        assert!(man.find("mode").is_some() && man.find("reset").is_none());
        assert!(faults_panel().find("reset").is_some() && faults_panel().find("pump_break").is_some());
    }

    #[test]
    fn the_readout_has_power_mode_and_scram() {
        let p = readout_panel(Some(&news()));
        assert!(p.find("power").is_some() && p.find("scram").is_some() && p.find("mode").is_some());
        for gone in ["fire_timer", "meltdown_timer", "pump", "junction", "alarm", "reset"] {
            assert!(p.find(gone).is_none(), "{gone}");
        }
    }

    #[test]
    fn the_zones_follow_the_spec_limits() {
        assert_eq!(zone(20.0), Zone::Cold);
        assert_eq!(zone(40.0), Zone::Low);
        assert_eq!(zone(54.0), Zone::Good);
        assert_eq!(zone(70.0), Zone::High);
        assert_eq!(zone(85.0), Zone::Crit);
        assert_eq!(reactor_status(Some(&news())).0, "STABLE");
        assert_eq!(reactor_status(Some(&ReactorNews { temp: 70.0, ..news() })).0, "HOT");
        assert_eq!(reactor_status(Some(&ReactorNews { temp: 85.0, ..news() })).0, "CRITICAL");
        assert_eq!(reactor_status(Some(&ReactorNews { power_on: false, ..news() })).0, "OFF");
    }

    #[test]
    fn above_80_with_the_power_on_is_the_critical_view_with_scram_and_none_without() {
        let hot = readout_panel(Some(&ReactorNews { temp: 85.0, ..news() }));
        assert!(hot.find("scram").is_some() && hot.find("fis_set").is_none(), "the takeover view");
        let off = readout_panel(Some(&ReactorNews { temp: 85.0, power_on: false, ..news() }));
        assert!(off.find("fis_set").is_some(), "scrammed: the normal view");
    }

    #[test]
    fn a_melted_core_says_meltdown_without_a_button() {
        let p = readout_panel(Some(&ReactorNews { melted: true, ..news() }));
        assert!(p.find("scram").is_none() && p.find("power").is_none());
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
    fn the_pump_words_follow_the_flow_and_the_switch() {
        assert_eq!(pump_status(true, 1.0).0, "RUNNING");
        assert_eq!(pump_status(true, 0.4).0, "LOW FLOW");
        assert_eq!(pump_status(true, 0.0).0, "STALLED");
        assert_eq!(pump_status(false, 0.0).0, "OFF");
        let p = pump_panel(Some(&news()));
        assert!(p.find("pump").is_some() && p.find("br_lights").is_none());
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
