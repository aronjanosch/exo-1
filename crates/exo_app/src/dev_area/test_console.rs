//! The test console (#285): a box at the lab zone's front edge, facing the pads, with a display
//! that proves the loop, and the pattern a role module follows. It is an M screen on the design
//! system's widgets. A counter with - and + (the minus is disabled at
//! zero, the plus at the top), a bar that fills with the counter, a toggle and a slider. Its two
//! systems are what a role writes: one builds the panel from the state, one reads the clicks.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::dev_area::engine_room::ShipAlarm;
use crate::items::Items;
use display_core::{bar, button, column, ctl, micro, readout, row, slider, space, toggle, Panel, Tier, Tone};
use item_core::wire::{Alarm, RoomAsk};

pub const ID: PanelId = PanelId::new("test_console");
pub const NAME: &str = "Test console";
/// The counter's top (the bar is full there).
pub const MAX: u32 = 10;
/// Where it stands in the lab zone (m from the zone's centre): at its south edge, facing the walkway
/// and the pads.
pub const AT: [f64; 2] = [0.0, -4.5];

/// The console's state.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TestConsole {
    pub count: u32,
    pub lights: bool,
    pub trim: f64,
}

impl Default for TestConsole {
    fn default() -> Self {
        TestConsole { count: 0, lights: false, trim: 0.5 }
    }
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "test_console".into(),
        zone: LAB,
        local_pos: AT,
        size: [1.6, 1.0, 0.8],
        turn_deg: 0.0,
        colour: [0.32, 0.36, 0.42],
        panel: Some(PanelSpec { id: ID, name: NAME.into(), size_m: [0.96, 0.6], at: [0.0, 1.4, 0.2], tilt_deg: 15.0 }),
    });
    app.init_resource::<TestConsole>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects));
}

/// What the console shows: an M screen (640 x 400 panel pixels) on the design system's widgets.
pub fn panel(s: &TestConsole, alarm: Alarm) -> Panel {
    Panel::screen(
        Tier::M,
        NAME,
        "TC-01",
        column(
            12.0,
            vec![
                row(12.0, vec![micro("Count").fixed(90.0), button("−", "dec").enabled(s.count > 0).fixed(ctl::LG), readout(format!("{:02}", s.count)), button("+", "inc").enabled(s.count < MAX).fixed(ctl::LG)]).fixed(ctl::LG),
                bar(s.count as f64 / MAX as f64, "Load").thresholds(Some(0.7), Some(0.9)).fixed(ctl::BAR),
                row(12.0, vec![toggle(s.lights, "Lights", "lights"), button(format!("Alarm {}", alarm_name(alarm)), "alarm")]).fixed(ctl::MD),
                row(12.0, vec![micro("Trim").fixed(90.0), slider(s.trim, "", "trim"), readout(format!("{:.2}", s.trim)).text_size(display_core::font::TITLE).fixed(90.0)]).fixed(ctl::MD),
                space(),
            ],
        ),
    )
    .status(if s.lights { "LIT" } else { "DARK" }, if s.lights { Tone::Good } else { Tone::Micro })
}

/// The alarm's name on the button (the dev toggle: None, Warn, Danger).
pub fn alarm_name(a: Alarm) -> &'static str {
    match a {
        Alarm::None => "none",
        Alarm::Warn => "warn",
        Alarm::Danger => "danger",
    }
}

/// The clicks on this panel change the state; the alarm is the host's, so the click asks for it.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut s: ResMut<TestConsole>, mut items: ResMut<Items>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        match (c.action.as_str(), c.value) {
            ("inc", _) => s.count = (s.count + 1).min(MAX),
            ("dec", _) => s.count = s.count.saturating_sub(1),
            ("lights", Some(v)) => s.lights = v > 0.5,
            ("trim", Some(v)) => s.trim = v as f64,
            ("alarm", _) => items.ask_room(RoomAsk::CycleAlarm),
            _ => warn!("test console: unknown action {c:?}"),
        }
    }
}

/// The panel from the state, every tick (`Panels::set` skips an unchanged one).
pub fn write_panel(s: Res<TestConsole>, alarm: Res<ShipAlarm>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&s, alarm.level));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_stop_at_the_ends_and_the_bar_follows_the_counter() {
        let p = panel(&TestConsole::default(), Alarm::None);
        assert!(matches!(p.find("dec").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
        let p = panel(&TestConsole { count: MAX, ..default() }, Alarm::Warn);
        assert!(matches!(p.find("inc").unwrap().widget, display_core::Widget::Button { enabled: false, .. }));
        let p = panel(&TestConsole { count: 3, ..default() }, Alarm::Danger);
        let bar = p.layout().into_iter().find_map(|w| if let display_core::Widget::Bar { value, .. } = w.widget { Some(*value) } else { None });
        assert!((bar.unwrap() - 0.3).abs() < 1e-9);
        assert!(p.layout().iter().all(|w| w.rect.max().y <= p.content().max().y + 1e-9), "everything fits");
    }
}
