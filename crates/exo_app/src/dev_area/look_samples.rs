use super::*;
use crate::displays::{DisplayAction, Panels};
use display_core::{bar, button, column, ctl, dial, label, level, micro, readout, row, slider, space, toggle, trend, Panel, Tier, Tone};

pub const LAB_ID: PanelId = PanelId::new("sample_lab");
pub const REACTOR_ID: PanelId = PanelId::new("sample_reactor");

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Warmer {
    pub temp: i32,
    pub mins: i32,
    pub run: bool,
}

impl Default for Warmer {
    fn default() -> Self {
        Warmer { temp: 37, mins: 12, run: false }
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ReactorSample {
    pub auto: bool,
    /// Setpoint, percent.
    pub setpoint: i32,
}

impl Default for ReactorSample {
    fn default() -> Self {
        ReactorSample { auto: true, setpoint: 45 }
    }
}

pub fn plugin(app: &mut App) {
    let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
    devices.add(PlaceholderBox {
        id: "sample_lab".into(),
        zone: LAB,
        local_pos: [-3.5, -4.5],
        size: [0.7, 1.1, 0.5],
        turn_deg: 0.0,
        colour: [0.3, 0.42, 0.4],
        panel: Some(PanelSpec { id: LAB_ID, name: "Warmer".into(), size_m: [0.48, 0.36], at: [0.0, 1.32, 0.15], tilt_deg: 15.0 }),
    });
    devices.add(PlaceholderBox {
        id: "sample_reactor".into(),
        zone: ENGINEERING,
        local_pos: [-4.0, -4.5],
        size: [1.4, 1.0, 0.8],
        turn_deg: 0.0,
        colour: [0.45, 0.36, 0.3],
        panel: Some(PanelSpec { id: REACTOR_ID, name: "Reactor 2".into(), size_m: [0.96, 0.6], at: [0.0, 1.4, 0.2], tilt_deg: 15.0 }),
    });
    app.init_resource::<Warmer>().init_resource::<ReactorSample>();
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

pub fn lab_panel(s: &Warmer) -> Panel {
    let tone = if s.temp >= 60 {
        Tone::Bad
    } else if s.temp >= 45 {
        Tone::Warn
    } else {
        Tone::Normal
    };
    Panel::screen(
        Tier::S,
        "Warmer",
        "WRM-07",
        column(
            6.0,
            vec![
                row(6.0, vec![button("−", "temp_down").enabled(!s.run).fixed(ctl::MD), readout(format!("{} °C", s.temp)).tone(tone), button("+", "temp_up").enabled(!s.run).fixed(ctl::MD)]).fixed(ctl::MD),
                row(6.0, vec![micro("Time").fixed(48.0), slider((s.mins - 1) as f64 / 29.0, "", "time").enabled(!s.run), label(format!("{}m", s.mins)).mono().fixed(48.0)]).fixed(ctl::SM),
                button(if s.run { "Stop" } else { "Start" }, "run").tone(if s.run { Tone::Bad } else { Tone::Normal }).fixed(ctl::SM + 4.0),
                space(),
            ],
        ),
    )
    .with_padding(6.0)
    .status(if s.run { "RUN" } else { "IDLE" }, if s.run { Tone::Good } else { Tone::Micro })
}

const HISTORY: [f64; 12] = [380.0, 384.0, 390.0, 395.0, 402.0, 410.0, 415.0, 420.0, 426.0, 430.0, 436.0, 492.0];

pub fn reactor_panel(s: &ReactorSample) -> Panel {
    let output = if s.auto { 45.0 } else { s.setpoint as f64 };
    let temp = 300.0 + output * 3.2;
    // Temperatures 200..600 °C as 0..1; warn 450, critical 540.
    let t01 = |t: f64| (t - 200.0) / 400.0;
    let (warn, crit) = (Some(t01(450.0)), Some(t01(540.0)));
    let lvl = level(t01(temp), warn, crit);
    let mut hist: Vec<f64> = HISTORY.iter().map(|t| t01(*t)).collect();
    *hist.last_mut().unwrap() = t01(temp);
    let status = ["STABLE", "HOT", "OVERHEAT"][lvl as usize];
    Panel::screen(
        Tier::M,
        "Reactor 2",
        "RX-02",
        row(
            24.0,
            vec![
                column(
                    12.0,
                    vec![
                        row(
                            16.0,
                            vec![
                                dial(output / 100.0, format!("{output:.0}%"), "Output").thresholds(Some(0.8), Some(0.95)).fixed(150.0),
                                column(8.0, vec![row(0.0, vec![micro("Core temp"), micro("Max 600")]).fixed(18.0), readout(format!("{temp:.0} °C")).tone(lvl.tone()).fixed(44.0), bar(t01(temp), "").thresholds(warn, crit).fixed(36.0), space()]),
                            ],
                        )
                        .fixed(180.0),
                        trend(hist).thresholds(warn, crit),
                    ],
                ),
                column(
                    12.0,
                    vec![
                        micro("Mode").fixed(18.0),
                        toggle(s.auto, "", "mode").labels("MAN", "AUTO").fixed(ctl::MD),
                        row(0.0, vec![micro("Setpoint"), label(format!("{}%", s.setpoint)).mono().tone(if s.auto { Tone::Dim } else { Tone::Normal }).fixed(60.0)]).fixed(22.0),
                        slider(s.setpoint as f64 / 100.0, "", "setpoint").enabled(!s.auto).fixed(ctl::MD),
                    ],
                )
                .fixed(190.0),
            ],
        ),
    )
    .status(status, lvl.tone())
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut lab: ResMut<Warmer>, mut rx: ResMut<ReactorSample>) {
    for c in clicks.read() {
        match (c.panel.as_str(), c.action.as_str(), c.value) {
            ("sample_lab", "temp_down", _) => lab.temp = (lab.temp - 1).max(20),
            ("sample_lab", "temp_up", _) => lab.temp = (lab.temp + 1).min(80),
            ("sample_lab", "time", Some(v)) => lab.mins = 1 + (v as f64 * 29.0).round() as i32,
            ("sample_lab", "run", _) => lab.run = !lab.run,
            ("sample_reactor", "mode", Some(v)) => rx.auto = v > 0.5,
            ("sample_reactor", "setpoint", Some(v)) => rx.setpoint = (v as f64 * 100.0).round() as i32,
            _ => {}
        }
    }
}

pub fn write_panels(lab: Res<Warmer>, rx: Res<ReactorSample>, mut panels: ResMut<Panels>) {
    panels.set(LAB_ID, lab_panel(&lab));
    panels.set(REACTOR_ID, reactor_panel(&rx));
}
